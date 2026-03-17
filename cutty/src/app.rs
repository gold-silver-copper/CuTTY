use std::collections::HashSet;

use anyhow::Result;
use arboard::Clipboard;
use winit::{
    application::ApplicationHandler,
    dpi::PhysicalSize,
    event::{ElementState, KeyEvent, MouseButton, MouseScrollDelta, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy},
    keyboard::{Key, ModifiersState, NamedKey},
    window::WindowId,
};

use crate::{
    config::AppConfig,
    events::UserEvent,
    input::bytes_for_key_event,
    parser::AnsiParser,
    pty::PtyProcess,
    renderer::GpuWindow,
    selection::{CellPos, SelectionState, StableCellPos, cell_at_position},
    terminal::{MouseTrackingMode, TerminalState},
    text::{TextSystem, grid_pixel_size},
};

const FONT_SIZE_STEP: f32 = 1.0;
const BRACKETED_PASTE_START: &[u8] = b"\x1b[200~";
const BRACKETED_PASTE_END: &[u8] = b"\x1b[201~";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SelectionMove {
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    WordLeft,
    WordRight,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TextClass {
    Space,
    Word,
    Punctuation,
}

const MAX_PENDING_RESIZE_EVENTS: u8 = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PendingGridResize {
    cols: u16,
    rows: u16,
    target_size: PhysicalSize<u32>,
    misses: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ResizeDecision {
    Ignore,
    Retry(PhysicalSize<u32>),
    ApplyGrid(u16, u16),
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct ResizeController {
    pending_programmatic_resize: Option<PendingGridResize>,
}

impl ResizeController {
    fn request_grid_preserving_resize(
        &mut self,
        cols: u16,
        rows: u16,
        target_size: PhysicalSize<u32>,
    ) {
        self.pending_programmatic_resize = Some(PendingGridResize {
            cols,
            rows,
            target_size,
            misses: 0,
        });
    }

    fn has_pending_resize(&self) -> bool {
        self.pending_programmatic_resize.is_some()
    }

    fn resolve_window_resize(
        &mut self,
        size: PhysicalSize<u32>,
        text: &TextSystem,
        current_grid: (u16, u16),
    ) -> ResizeDecision {
        let actual_grid = text.visible_grid(size);

        if let Some(pending) = self.pending_programmatic_resize.as_mut() {
            let target_grid = (pending.cols, pending.rows);
            let target_grid_preserved = current_grid == target_grid;

            if target_grid_preserved {
                if actual_grid == target_grid {
                    self.pending_programmatic_resize = None;
                    return ResizeDecision::Ignore;
                }

                if pending.misses < MAX_PENDING_RESIZE_EVENTS {
                    pending.misses += 1;
                    return ResizeDecision::Retry(pending.target_size);
                }
            }

            self.pending_programmatic_resize = None;
        }

        ResizeDecision::ApplyGrid(actual_grid.0, actual_grid.1)
    }
}

pub fn run(config: AppConfig) -> Result<()> {
    let event_loop = EventLoop::<UserEvent>::with_user_event().build()?;
    let mut app = App {
        config,
        state: None,
        proxy: event_loop.create_proxy(),
    };
    event_loop.run_app(&mut app)?;
    Ok(())
}

pub struct App {
    config: AppConfig,
    state: Option<AppState>,
    proxy: EventLoopProxy<UserEvent>,
}

struct AppState {
    window: GpuWindow,
    parser: AnsiParser,
    terminal: TerminalState,
    pty: PtyProcess,
    text: TextSystem,
    resize_controller: ResizeController,
    modifiers: ModifiersState,
    clipboard: Option<Clipboard>,
    selection: SelectionState,
    cursor_position: Option<winit::dpi::PhysicalPosition<f64>>,
    last_mouse_cell: Option<CellPos>,
    pressed_mouse_buttons: HashSet<MouseButton>,
    keyboard_selection_anchor: Option<usize>,
    keyboard_selection_focus: Option<usize>,
    viewport_scroll_remainder: f64,
}

impl AppState {
    fn sync_to_window_size(&mut self) {
        let size = self.window.inner_size();
        let (current_rows, current_cols) = self.terminal.size();
        match self.resize_controller.resolve_window_resize(
            size,
            &self.text,
            (current_cols, current_rows),
        ) {
            ResizeDecision::Ignore => {}
            ResizeDecision::Retry(target_size) => {
                self.window.request_inner_size(target_size);
            }
            ResizeDecision::ApplyGrid(cols, rows) => {
                if cols != current_cols || rows != current_rows {
                    self.apply_terminal_size(cols, rows, false);
                }
            }
        }
        self.window.request_redraw();
    }

    fn reconcile_pending_zoom_resize(&mut self) {
        if self.resize_controller.has_pending_resize() {
            self.sync_to_window_size();
        }
    }

    fn apply_terminal_size(&mut self, cols: u16, rows: u16, resize_window: bool) {
        let (current_rows, current_cols) = self.terminal.size();
        let size_changed = cols != current_cols || rows != current_rows;

        if size_changed {
            self.terminal.resize(rows, cols);
            self.text.resize_cache(rows);
            self.clear_selection();
            self.pty.resize(cols, rows);
        }

        if resize_window {
            let size =
                self.window
                    .request_inner_size(grid_pixel_size(cols, rows, self.text.metrics()));
            self.resize_controller
                .request_grid_preserving_resize(cols, rows, size);
        }
    }

    fn copy_selection(&mut self) {
        let Some(clipboard) = self.clipboard.as_mut() else {
            return;
        };
        let Some(text) = self.selection.selection_text(&self.terminal) else {
            return;
        };
        let _ = clipboard.set_text(text);
    }

    fn paste_clipboard(&mut self) {
        let Some(text) = self
            .clipboard
            .as_mut()
            .and_then(|clipboard| clipboard.get_text().ok())
        else {
            return;
        };

        let bytes = if self.terminal.bracketed_paste() {
            bracketed_paste_bytes(&text)
        } else {
            text.into_bytes()
        };
        self.send_terminal_input(&bytes);
    }

    fn adjust_font_size(&mut self, delta: f32) {
        let (rows, cols) = self.terminal.size();
        if self
            .text
            .adjust_font_size_to_window(delta, cols, rows, self.window.max_render_size())
        {
            self.apply_terminal_size(cols, rows, true);
            self.window.request_redraw();
        }
    }

    fn clear_selection(&mut self) {
        self.selection.clear();
        self.reset_keyboard_selection();
    }

    fn reset_keyboard_selection(&mut self) {
        self.keyboard_selection_anchor = None;
        self.keyboard_selection_focus = None;
    }

    fn prepare_for_terminal_input(&mut self) {
        self.clear_selection();
        self.viewport_scroll_remainder = 0.0;
        if self.terminal.follow_viewport_bottom() {
            self.window.request_redraw();
        }
    }

    fn send_terminal_input(&mut self, bytes: &[u8]) {
        self.prepare_for_terminal_input();
        self.pty.write_all(bytes);
        self.window.request_redraw();
    }

    fn scroll_viewport_lines(&mut self, lines: i32) {
        let changed = match lines.cmp(&0) {
            std::cmp::Ordering::Greater => self
                .terminal
                .scroll_viewport_up(lines.min(u16::MAX as i32) as u16),
            std::cmp::Ordering::Less => self
                .terminal
                .scroll_viewport_down(lines.unsigned_abs().min(u16::MAX as u32) as u16),
            std::cmp::Ordering::Equal => false,
        };

        if changed {
            self.reset_keyboard_selection();
            self.window.request_redraw();
        }
    }

    fn scroll_viewport_page(&mut self, direction: i32) {
        let (rows, _) = self.terminal.size();
        let lines = rows.saturating_sub(1).max(1) as i32;
        self.scroll_viewport_lines(direction.saturating_mul(lines));
    }

    fn handle_local_mouse_wheel(&mut self, delta: MouseScrollDelta) {
        let line_delta = viewport_scroll_lines_from_delta(delta, self.text.metrics().height);
        if line_delta == 0.0 {
            return;
        }

        self.viewport_scroll_remainder += line_delta;
        let whole_lines = self.viewport_scroll_remainder.trunc() as i32;
        if whole_lines != 0 {
            self.viewport_scroll_remainder -= whole_lines as f64;
            self.scroll_viewport_lines(whole_lines);
        }
    }

    fn terminal_caret_index(&self) -> usize {
        let (rows, cols) = self.terminal.size();
        if rows == 0 || cols == 0 {
            return 0;
        }

        let (row, col) = self.terminal.cursor_position();
        (row as usize * cols as usize + col as usize).min(rows as usize * cols as usize)
    }

    fn active_keyboard_focus_index(&self) -> usize {
        self.keyboard_selection_focus
            .unwrap_or_else(|| self.terminal_caret_index())
    }

    fn apply_keyboard_selection(&mut self, move_kind: SelectionMove) {
        let (rows, cols) = self.terminal.size();
        if rows == 0 || cols == 0 {
            return;
        }

        let total_cells = rows as usize * cols as usize;
        let anchor = self
            .keyboard_selection_anchor
            .unwrap_or_else(|| self.terminal_caret_index().min(total_cells));
        let focus = self.active_keyboard_focus_index().min(total_cells);
        let next_focus = move_selection_focus(&self.terminal, focus, move_kind);

        self.keyboard_selection_anchor = Some(anchor);
        self.keyboard_selection_focus = Some(next_focus);

        if anchor == next_focus {
            self.selection.clear();
        } else {
            let (start, end) = if anchor < next_focus {
                (anchor, next_focus)
            } else {
                (next_focus, anchor)
            };
            let selection_start = stable_cell_pos_from_linear(&self.terminal, start, cols);
            let selection_end =
                stable_cell_pos_from_linear(&self.terminal, end.saturating_sub(1), cols);
            self.selection.set_range(selection_start, selection_end);
        }

        self.window.request_redraw();
    }

    fn mouse_cell_at_position(
        &self,
        position: winit::dpi::PhysicalPosition<f64>,
    ) -> Option<CellPos> {
        cell_at_position(position, self.text.metrics(), &self.terminal)
    }

    fn stable_cell_pos(&self, cell: CellPos) -> StableCellPos {
        StableCellPos {
            row: self.terminal.visible_row_to_stable_row(cell.row),
            col: cell.col,
        }
    }

    fn current_mouse_cell(&self) -> Option<CellPos> {
        self.cursor_position
            .and_then(|position| self.mouse_cell_at_position(position))
    }

    fn mouse_cell_for_report(&self, allow_last: bool) -> Option<CellPos> {
        self.current_mouse_cell().or(if allow_last {
            self.last_mouse_cell
        } else {
            None
        })
    }

    fn report_focus(&mut self, focused: bool) {
        if self.terminal.focus_reporting() {
            let response = if focused { b"\x1b[I" } else { b"\x1b[O" };
            self.pty.write_all(response);
        }

        if !focused {
            self.pressed_mouse_buttons.clear();
        }
    }

    fn report_mouse_motion(&mut self, cell: CellPos) {
        let Some(bytes) = mouse_motion_bytes(
            cell,
            self.modifiers,
            &self.pressed_mouse_buttons,
            self.terminal.mouse_tracking_mode(),
            self.terminal.sgr_mouse(),
        ) else {
            return;
        };
        self.pty.write_all(&bytes);
    }

    fn report_mouse_input(&mut self, button_state: ElementState, button: MouseButton) {
        if button_state == ElementState::Pressed && is_reportable_mouse_button(button) {
            self.pressed_mouse_buttons.insert(button);
        }

        if let Some(cell) = self.mouse_cell_for_report(button_state == ElementState::Released)
            && let Some(bytes) = mouse_button_bytes(
                button,
                button_state,
                cell,
                self.modifiers,
                self.terminal.sgr_mouse(),
            )
        {
            self.pty.write_all(&bytes);
        }

        if button_state == ElementState::Released {
            self.pressed_mouse_buttons.remove(&button);
        }
    }

    fn report_mouse_wheel(&mut self, delta: MouseScrollDelta) {
        let Some(cell) = self.mouse_cell_for_report(true) else {
            return;
        };

        let bytes = mouse_wheel_bytes(delta, cell, self.modifiers, self.terminal.sgr_mouse());
        if !bytes.is_empty() {
            self.pty.write_all(&bytes);
        }
    }

    fn drain_pty(&mut self) {
        let mut changed = false;
        for chunk in self.pty.drain() {
            self.parser.process(&mut self.terminal, &chunk);
            changed = true;
        }

        let responses = self.parser.take_responses();
        let title = self.parser.take_title();

        for response in responses {
            self.pty.write_all(&response);
        }

        if let Some(title) = title {
            if title.is_empty() {
                self.window.window.set_title("cutty");
            } else {
                self.window.window.set_title(&title);
            }
        }

        if changed {
            self.window.request_redraw();
        }
    }

    fn handle_pressed_key(&mut self, event: &KeyEvent) {
        if let Some(selection_move) = selection_move_for_key(&event.logical_key, self.modifiers) {
            self.apply_keyboard_selection(selection_move);
            return;
        }

        if let Some(page_direction) = viewport_scroll_shortcut(&event.logical_key, self.modifiers) {
            self.scroll_viewport_page(page_direction);
            return;
        }

        if is_copy_shortcut(&event.logical_key, self.modifiers) {
            self.copy_selection();
            return;
        }

        if is_paste_shortcut(&event.logical_key, self.modifiers) {
            self.paste_clipboard();
            return;
        }

        if is_font_increase_shortcut(&event.logical_key, self.modifiers) {
            self.adjust_font_size(FONT_SIZE_STEP);
            return;
        }

        if is_font_decrease_shortcut(&event.logical_key, self.modifiers) {
            self.adjust_font_size(-FONT_SIZE_STEP);
            return;
        }

        if let Some(bytes) =
            bytes_for_key_event(event, self.modifiers, self.terminal.application_cursor())
        {
            self.send_terminal_input(&bytes);
        }
    }
}

impl ApplicationHandler<UserEvent> for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_some() {
            return;
        }

        let window = GpuWindow::new(
            event_loop,
            "cutty",
            (self.config.window.width, self.config.window.height),
        )
        .expect("failed to initialize GPU window");
        let mut text = TextSystem::new(1, &self.config.font);
        let (cols, rows) = text.visible_grid(window.inner_size());
        text.resize_cache(rows);
        let parser = AnsiParser::new();
        let terminal = TerminalState::new(rows, cols, self.config.terminal.scrollback);
        let pty = PtyProcess::spawn(
            self.proxy.clone(),
            cols,
            rows,
            self.config.terminal.shell.as_deref(),
        )
        .expect("failed to spawn shell");

        self.state = Some(AppState {
            window,
            parser,
            terminal,
            pty,
            text,
            resize_controller: ResizeController::default(),
            modifiers: ModifiersState::default(),
            clipboard: Clipboard::new().ok(),
            selection: SelectionState::default(),
            cursor_position: None,
            last_mouse_cell: None,
            pressed_mouse_buttons: HashSet::new(),
            keyboard_selection_anchor: None,
            keyboard_selection_focus: None,
            viewport_scroll_remainder: 0.0,
        });

        if let Some(state) = self.state.as_mut() {
            state.window.request_redraw();
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        let Some(state) = self.state.as_mut() else {
            return;
        };

        if state.window.window.id() != window_id {
            return;
        }

        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Focused(focused) => {
                state.report_focus(focused);
            }
            WindowEvent::Resized(size) => {
                state.window.resize(size);
                state.sync_to_window_size();
            }
            WindowEvent::ModifiersChanged(modifiers) => {
                state.modifiers = modifiers.state();
            }
            WindowEvent::CursorMoved { position, .. } => {
                state.cursor_position = Some(position);
                let cell = state.mouse_cell_at_position(position);
                if let Some(cell) = cell {
                    state.last_mouse_cell = Some(cell);
                    if state.terminal.mouse_reporting_enabled() {
                        state.report_mouse_motion(cell);
                    } else if state.selection.update(state.stable_cell_pos(cell)) {
                        state.window.request_redraw();
                    }
                }
            }
            WindowEvent::MouseInput {
                state: button_state,
                button,
                ..
            } => {
                if state.terminal.mouse_reporting_enabled() {
                    state.report_mouse_input(button_state, button);
                } else if button == MouseButton::Left {
                    match button_state {
                        ElementState::Pressed => {
                            if let Some(position) = state.cursor_position {
                                if let Some(cell) = state.mouse_cell_at_position(position) {
                                    state.keyboard_selection_anchor = None;
                                    state.keyboard_selection_focus = None;
                                    state.selection.begin(state.stable_cell_pos(cell));
                                    state.window.request_redraw();
                                } else if state.selection.is_selected() {
                                    state.clear_selection();
                                    state.window.request_redraw();
                                }
                            }
                        }
                        ElementState::Released => {
                            if state.selection.finish() {
                                state.window.request_redraw();
                            }
                        }
                    }
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                if state.terminal.mouse_reporting_enabled() {
                    state.report_mouse_wheel(delta);
                } else {
                    state.handle_local_mouse_wheel(delta);
                }
            }
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                state.handle_pressed_key(&event);
            }
            WindowEvent::RedrawRequested => {
                state.drain_pty();
                state.text.sync_terminal_rows(&state.terminal);
                if let Err(error) =
                    state
                        .window
                        .render_terminal(&state.terminal, &state.text, &state.selection)
                {
                    eprintln!("{error:#}");
                }
            }
            _ => {}
        }
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: UserEvent) {
        let Some(state) = self.state.as_mut() else {
            return;
        };

        match event {
            UserEvent::PtyUpdate => state.drain_pty(),
            UserEvent::PtyExit => event_loop.exit(),
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(state) = self.state.as_mut() {
            state.reconcile_pending_zoom_resize();
            state.window.request_redraw();
        }
    }
}

fn is_copy_shortcut(key: &Key, modifiers: ModifiersState) -> bool {
    matches_character(key, "c")
        && if cfg!(target_os = "macos") {
            modifiers.super_key() && !modifiers.control_key()
        } else {
            modifiers.control_key() && modifiers.shift_key()
        }
}

fn is_paste_shortcut(key: &Key, modifiers: ModifiersState) -> bool {
    matches_character(key, "v")
        && if cfg!(target_os = "macos") {
            modifiers.super_key() && !modifiers.control_key()
        } else {
            modifiers.control_key() && modifiers.shift_key()
        }
}

fn is_font_increase_shortcut(key: &Key, modifiers: ModifiersState) -> bool {
    uses_command_shortcuts(modifiers) && matches_one_of(key, &["+", "="])
}

fn is_font_decrease_shortcut(key: &Key, modifiers: ModifiersState) -> bool {
    uses_command_shortcuts(modifiers) && matches_one_of(key, &["-", "_"])
}

fn selection_move_for_key(key: &Key, modifiers: ModifiersState) -> Option<SelectionMove> {
    if !modifiers.shift_key() || modifiers.control_key() || modifiers.super_key() {
        return None;
    }

    let word_mode = modifiers.alt_key();
    match key.as_ref() {
        Key::Named(NamedKey::ArrowLeft) => Some(if word_mode {
            SelectionMove::WordLeft
        } else {
            SelectionMove::Left
        }),
        Key::Named(NamedKey::ArrowRight) => Some(if word_mode {
            SelectionMove::WordRight
        } else {
            SelectionMove::Right
        }),
        Key::Named(NamedKey::ArrowUp) if !word_mode => Some(SelectionMove::Up),
        Key::Named(NamedKey::ArrowDown) if !word_mode => Some(SelectionMove::Down),
        Key::Named(NamedKey::Home) if !word_mode => Some(SelectionMove::Home),
        Key::Named(NamedKey::End) if !word_mode => Some(SelectionMove::End),
        _ => None,
    }
}

fn viewport_scroll_shortcut(key: &Key, modifiers: ModifiersState) -> Option<i32> {
    if !modifiers.shift_key()
        || modifiers.control_key()
        || modifiers.alt_key()
        || modifiers.super_key()
    {
        return None;
    }

    match key.as_ref() {
        Key::Named(NamedKey::PageUp) => Some(1),
        Key::Named(NamedKey::PageDown) => Some(-1),
        _ => None,
    }
}

fn bracketed_paste_bytes(text: &str) -> Vec<u8> {
    let mut bytes =
        Vec::with_capacity(BRACKETED_PASTE_START.len() + text.len() + BRACKETED_PASTE_END.len());
    bytes.extend_from_slice(BRACKETED_PASTE_START);
    bytes.extend_from_slice(text.as_bytes());
    bytes.extend_from_slice(BRACKETED_PASTE_END);
    bytes
}

fn mouse_button_bytes(
    button: MouseButton,
    button_state: ElementState,
    cell: CellPos,
    modifiers: ModifiersState,
    sgr_mouse: bool,
) -> Option<Vec<u8>> {
    let modifier_bits = mouse_modifier_bits(modifiers);
    let button_code = mouse_button_code(button)?;

    if sgr_mouse {
        let release = button_state == ElementState::Released;
        encode_mouse_event(button_code | modifier_bits, cell, true, release)
    } else {
        let cb = match button_state {
            ElementState::Pressed => button_code | modifier_bits,
            ElementState::Released => 3 | modifier_bits,
        };
        encode_mouse_event(cb, cell, false, false)
    }
}

fn mouse_motion_bytes(
    cell: CellPos,
    modifiers: ModifiersState,
    pressed_buttons: &HashSet<MouseButton>,
    tracking_mode: MouseTrackingMode,
    sgr_mouse: bool,
) -> Option<Vec<u8>> {
    let modifier_bits = mouse_modifier_bits(modifiers);
    let cb = match tracking_mode {
        MouseTrackingMode::Disabled | MouseTrackingMode::Normal => return None,
        MouseTrackingMode::ButtonMotion => {
            mouse_button_code(active_reported_mouse_button(pressed_buttons)?)? | 32 | modifier_bits
        }
        MouseTrackingMode::AnyMotion => {
            if let Some(button) = active_reported_mouse_button(pressed_buttons) {
                mouse_button_code(button)? | 32 | modifier_bits
            } else {
                3 | 32 | modifier_bits
            }
        }
    };

    encode_mouse_event(cb, cell, sgr_mouse, false)
}

fn mouse_wheel_bytes(
    delta: MouseScrollDelta,
    cell: CellPos,
    modifiers: ModifiersState,
    sgr_mouse: bool,
) -> Vec<u8> {
    let modifier_bits = mouse_modifier_bits(modifiers);
    let (x, y) = match delta {
        MouseScrollDelta::LineDelta(x, y) => (x as f64, y as f64),
        MouseScrollDelta::PixelDelta(position) => (position.x, position.y),
    };

    let mut bytes = Vec::new();
    for code in wheel_codes_from_delta(x, y) {
        if let Some(sequence) = encode_mouse_event(code | modifier_bits, cell, sgr_mouse, false) {
            bytes.extend(sequence);
        }
    }
    bytes
}

fn wheel_codes_from_delta(x: f64, y: f64) -> Vec<u8> {
    let mut codes = Vec::new();
    if y > 0.0 {
        codes.push(64);
    } else if y < 0.0 {
        codes.push(65);
    }

    if x > 0.0 {
        codes.push(66);
    } else if x < 0.0 {
        codes.push(67);
    }

    codes
}

fn active_reported_mouse_button(pressed_buttons: &HashSet<MouseButton>) -> Option<MouseButton> {
    [MouseButton::Left, MouseButton::Middle, MouseButton::Right]
        .into_iter()
        .find(|button| pressed_buttons.contains(button))
}

fn is_reportable_mouse_button(button: MouseButton) -> bool {
    mouse_button_code(button).is_some()
}

fn mouse_button_code(button: MouseButton) -> Option<u8> {
    match button {
        MouseButton::Left => Some(0),
        MouseButton::Middle => Some(1),
        MouseButton::Right => Some(2),
        _ => None,
    }
}

fn mouse_modifier_bits(modifiers: ModifiersState) -> u8 {
    let mut bits = 0;
    if modifiers.shift_key() {
        bits |= 4;
    }
    if modifiers.alt_key() {
        bits |= 8;
    }
    if modifiers.control_key() {
        bits |= 16;
    }
    bits
}

fn encode_mouse_event(cb: u8, cell: CellPos, sgr_mouse: bool, release: bool) -> Option<Vec<u8>> {
    if sgr_mouse {
        let suffix = if release { 'm' } else { 'M' };
        Some(format!("\x1b[<{cb};{};{}{}", cell.col + 1, cell.row + 1, suffix).into_bytes())
    } else {
        let x = cell.col + 1;
        let y = cell.row + 1;
        if x > 223 || y > 223 {
            return None;
        }

        Some(vec![
            0x1b,
            b'[',
            b'M',
            cb.saturating_add(32),
            x as u8 + 32,
            y as u8 + 32,
        ])
    }
}

fn uses_command_shortcuts(modifiers: ModifiersState) -> bool {
    modifiers.super_key() && !modifiers.control_key() && !modifiers.alt_key()
}

fn move_selection_focus(terminal: &TerminalState, focus: usize, move_kind: SelectionMove) -> usize {
    let (rows, cols) = terminal.size();
    let cols = cols as usize;
    let total_cells = rows as usize * cols;
    if cols == 0 || total_cells == 0 {
        return 0;
    }

    match move_kind {
        SelectionMove::Left => focus.saturating_sub(1),
        SelectionMove::Right => focus.saturating_add(1).min(total_cells),
        SelectionMove::Up => focus.saturating_sub(cols),
        SelectionMove::Down => focus.saturating_add(cols).min(total_cells),
        SelectionMove::Home => (focus / cols) * cols,
        SelectionMove::End => ((focus / cols) + 1).saturating_mul(cols).min(total_cells),
        SelectionMove::WordLeft => previous_word_boundary(terminal, focus),
        SelectionMove::WordRight => next_word_boundary(terminal, focus),
    }
}

fn previous_word_boundary(terminal: &TerminalState, mut focus: usize) -> usize {
    while focus > 0 && text_class_at(terminal, focus - 1) == TextClass::Space {
        focus -= 1;
    }
    if focus == 0 {
        return 0;
    }

    let class = text_class_at(terminal, focus - 1);
    while focus > 0 && text_class_at(terminal, focus - 1) == class {
        focus -= 1;
    }
    focus
}

fn next_word_boundary(terminal: &TerminalState, mut focus: usize) -> usize {
    let (rows, cols) = terminal.size();
    let total_cells = rows as usize * cols as usize;
    while focus < total_cells && text_class_at(terminal, focus) == TextClass::Space {
        focus += 1;
    }
    if focus >= total_cells {
        return total_cells;
    }

    let class = text_class_at(terminal, focus);
    while focus < total_cells && text_class_at(terminal, focus) == class {
        focus += 1;
    }
    focus
}

fn text_class_at(terminal: &TerminalState, index: usize) -> TextClass {
    let (_, cols) = terminal.size();
    if cols == 0 {
        return TextClass::Space;
    }

    let cell = terminal.cell(
        (index / cols as usize) as u16,
        (index % cols as usize) as u16,
    );
    let ch = cell
        .filter(|cell| !cell.is_wide_continuation())
        .and_then(|cell| cell.contents().chars().next())
        .unwrap_or(' ');

    if ch.is_whitespace() {
        TextClass::Space
    } else if ch.is_alphanumeric() || ch == '_' {
        TextClass::Word
    } else {
        TextClass::Punctuation
    }
}

fn cell_pos_from_linear(index: usize, cols: u16) -> CellPos {
    let cols = cols as usize;
    CellPos {
        row: (index / cols) as u16,
        col: (index % cols) as u16,
    }
}

fn stable_cell_pos_from_linear(terminal: &TerminalState, index: usize, cols: u16) -> StableCellPos {
    let cell = cell_pos_from_linear(index, cols);
    StableCellPos {
        row: terminal.visible_row_to_stable_row(cell.row),
        col: cell.col,
    }
}

fn viewport_scroll_lines_from_delta(delta: MouseScrollDelta, row_height: f32) -> f64 {
    match delta {
        MouseScrollDelta::LineDelta(_, y) => y as f64,
        MouseScrollDelta::PixelDelta(position) if row_height > 0.0 => {
            position.y / row_height as f64
        }
        MouseScrollDelta::PixelDelta(_) => 0.0,
    }
}

fn matches_character(key: &Key, expected: &str) -> bool {
    matches!(key.as_ref(), Key::Character(text) if text.eq_ignore_ascii_case(expected))
}

fn matches_one_of(key: &Key, expected: &[&str]) -> bool {
    expected
        .iter()
        .any(|candidate| matches_character(key, candidate))
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::{
        MAX_PENDING_RESIZE_EVENTS, MouseTrackingMode, ResizeController, ResizeDecision,
        SelectionMove, bracketed_paste_bytes, cell_pos_from_linear, encode_mouse_event,
        is_font_decrease_shortcut, is_font_increase_shortcut, mouse_button_bytes,
        mouse_motion_bytes, move_selection_focus, selection_move_for_key, uses_command_shortcuts,
        viewport_scroll_lines_from_delta, viewport_scroll_shortcut, wheel_codes_from_delta,
    };
    use crate::{
        config::FontConfig,
        selection::CellPos,
        terminal::TerminalState,
        text::{TextSystem, grid_pixel_size},
    };
    use winit::{
        dpi::PhysicalSize,
        event::{ElementState, MouseButton, MouseScrollDelta},
        keyboard::{Key, ModifiersState, NamedKey},
    };

    #[test]
    fn command_shortcuts_require_super_without_ctrl_or_alt() {
        assert!(uses_command_shortcuts(ModifiersState::SUPER));
        assert!(!uses_command_shortcuts(
            ModifiersState::SUPER.union(ModifiersState::CONTROL)
        ));
        assert!(!uses_command_shortcuts(
            ModifiersState::SUPER.union(ModifiersState::ALT)
        ));
    }

    #[test]
    fn font_shortcuts_match_plus_equals_minus_and_underscore() {
        let modifiers = ModifiersState::SUPER;

        assert!(is_font_increase_shortcut(
            &Key::Character("+".into()),
            modifiers
        ));
        assert!(is_font_increase_shortcut(
            &Key::Character("=".into()),
            modifiers
        ));
        assert!(is_font_decrease_shortcut(
            &Key::Character("-".into()),
            modifiers
        ));
        assert!(is_font_decrease_shortcut(
            &Key::Character("_".into()),
            modifiers
        ));
    }

    #[test]
    fn window_size_for_grid_round_trips_through_visible_grid() {
        let font = FontConfig::default();
        let text = TextSystem::new(24, &font);
        let size = grid_pixel_size(80, 24, text.metrics());

        assert_eq!(text.visible_grid(size), (80, 24));
    }

    #[test]
    fn programmatic_resize_suppresses_matching_window_resize_events() {
        let font = FontConfig::default();
        let text = TextSystem::new(24, &font);
        let size = grid_pixel_size(80, 24, text.metrics());
        let mut controller = ResizeController::default();

        controller.request_grid_preserving_resize(80, 24, size);

        assert_eq!(
            controller.resolve_window_resize(size, &text, (80, 24)),
            ResizeDecision::Ignore
        );
        assert_eq!(controller.pending_programmatic_resize, None);
    }

    #[test]
    fn programmatic_resize_retries_when_window_is_still_too_small_for_target_grid() {
        let font = FontConfig::default();
        let text = TextSystem::new(24, &font);
        let requested_size = grid_pixel_size(80, 24, text.metrics());
        let actual_size = PhysicalSize::new(requested_size.width, requested_size.height - 1);
        let mut controller = ResizeController::default();

        controller.request_grid_preserving_resize(80, 24, requested_size);

        for _ in 0..MAX_PENDING_RESIZE_EVENTS {
            assert_eq!(
                controller.resolve_window_resize(actual_size, &text, (80, 24)),
                ResizeDecision::Retry(requested_size)
            );
        }
    }

    #[test]
    fn programmatic_resize_times_out_if_window_never_reaches_requested_size() {
        let font = FontConfig::default();
        let text = TextSystem::new(24, &font);
        let requested_size = grid_pixel_size(80, 24, text.metrics());
        let actual_size = PhysicalSize::new(requested_size.width, requested_size.height - 1);
        let fallback_grid = text.visible_grid(actual_size);
        let mut controller = ResizeController::default();

        controller.request_grid_preserving_resize(80, 24, requested_size);

        for _ in 0..MAX_PENDING_RESIZE_EVENTS {
            let _ = controller.resolve_window_resize(actual_size, &text, (80, 24));
        }

        assert_eq!(
            controller.resolve_window_resize(actual_size, &text, (80, 24)),
            ResizeDecision::ApplyGrid(fallback_grid.0, fallback_grid.1)
        );
        assert_eq!(controller.pending_programmatic_resize, None);
    }

    #[test]
    fn bracketed_paste_wraps_text_in_expected_markers() {
        assert_eq!(
            bracketed_paste_bytes("hello"),
            b"\x1b[200~hello\x1b[201~".to_vec()
        );
    }

    #[test]
    fn sgr_mouse_release_uses_lowercase_suffix() {
        let bytes = mouse_button_bytes(
            MouseButton::Left,
            ElementState::Released,
            CellPos { row: 2, col: 4 },
            ModifiersState::empty(),
            true,
        )
        .expect("mouse bytes");

        assert_eq!(bytes, b"\x1b[<0;5;3m".to_vec());
    }

    #[test]
    fn any_motion_mouse_reports_without_pressed_buttons() {
        let bytes = mouse_motion_bytes(
            CellPos { row: 1, col: 2 },
            ModifiersState::empty(),
            &HashSet::new(),
            MouseTrackingMode::AnyMotion,
            true,
        )
        .expect("mouse bytes");

        assert_eq!(bytes, b"\x1b[<35;3;2M".to_vec());
    }

    #[test]
    fn legacy_mouse_encoding_is_one_based_with_offset() {
        let bytes =
            encode_mouse_event(0, CellPos { row: 0, col: 0 }, false, false).expect("mouse bytes");

        assert_eq!(bytes, vec![0x1b, b'[', b'M', 32, 33, 33]);
    }

    #[test]
    fn wheel_codes_follow_scroll_direction_signs() {
        assert_eq!(wheel_codes_from_delta(0.0, 2.0), vec![64]);
        assert_eq!(wheel_codes_from_delta(0.0, -1.0), vec![65]);
        assert_eq!(wheel_codes_from_delta(3.0, 0.0), vec![66]);
        assert_eq!(wheel_codes_from_delta(-2.0, 0.0), vec![67]);
    }

    #[test]
    fn selection_shortcuts_require_shift_and_respect_alt_word_mode() {
        assert_eq!(
            selection_move_for_key(
                &Key::Named(NamedKey::ArrowLeft),
                ModifiersState::SHIFT.union(ModifiersState::ALT)
            ),
            Some(SelectionMove::WordLeft)
        );
        assert_eq!(
            selection_move_for_key(&Key::Named(NamedKey::ArrowDown), ModifiersState::SHIFT),
            Some(SelectionMove::Down)
        );
        assert_eq!(
            selection_move_for_key(&Key::Named(NamedKey::ArrowLeft), ModifiersState::empty()),
            None
        );
    }

    #[test]
    fn viewport_scroll_shortcuts_use_shift_page_keys_only() {
        assert_eq!(
            viewport_scroll_shortcut(&Key::Named(NamedKey::PageUp), ModifiersState::SHIFT),
            Some(1)
        );
        assert_eq!(
            viewport_scroll_shortcut(&Key::Named(NamedKey::PageDown), ModifiersState::SHIFT),
            Some(-1)
        );
        assert_eq!(
            viewport_scroll_shortcut(
                &Key::Named(NamedKey::PageUp),
                ModifiersState::SHIFT.union(ModifiersState::CONTROL)
            ),
            None
        );
    }

    #[test]
    fn pixel_mouse_wheel_deltas_convert_to_row_units() {
        assert_eq!(
            viewport_scroll_lines_from_delta(
                MouseScrollDelta::PixelDelta(winit::dpi::PhysicalPosition::new(0.0, 36.0)),
                18.0
            ),
            2.0
        );
        assert_eq!(
            viewport_scroll_lines_from_delta(MouseScrollDelta::LineDelta(0.0, -3.0), 18.0),
            -3.0
        );
    }

    #[test]
    fn word_movement_skips_spaces_and_whole_words() {
        let mut terminal = TerminalState::new(1, 12, 0);
        for ch in "hello  world".chars() {
            terminal.print(ch);
        }

        assert_eq!(
            move_selection_focus(&terminal, 12, SelectionMove::WordLeft),
            7
        );
        assert_eq!(
            move_selection_focus(&terminal, 0, SelectionMove::WordRight),
            5
        );
        assert_eq!(
            move_selection_focus(&terminal, 5, SelectionMove::WordRight),
            12
        );
    }

    #[test]
    fn linear_indices_map_back_to_cells() {
        assert_eq!(cell_pos_from_linear(0, 4), CellPos { row: 0, col: 0 });
        assert_eq!(cell_pos_from_linear(6, 4), CellPos { row: 1, col: 2 });
    }
}
