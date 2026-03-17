use anyhow::Result;
use arboard::Clipboard;
use winit::{
    application::ApplicationHandler,
    dpi::PhysicalSize,
    event::{ElementState, MouseButton, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy},
    keyboard::{Key, ModifiersState},
    window::WindowId,
};

use crate::{
    config::AppConfig,
    events::UserEvent,
    input::bytes_for_key_event,
    parser::AnsiParser,
    pty::PtyProcess,
    renderer::GpuWindow,
    selection::{SelectionState, cell_at_position},
    terminal::TerminalState,
    text::{PADDING_X, PADDING_Y, TextSystem},
};

const FONT_SIZE_STEP: f32 = 1.0;

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
    prev_terminal: Option<TerminalState>,
    pty: PtyProcess,
    text: TextSystem,
    modifiers: ModifiersState,
    clipboard: Option<Clipboard>,
    selection: SelectionState,
    cursor_position: Option<winit::dpi::PhysicalPosition<f64>>,
}

impl AppState {
    fn sync_to_window_size(&mut self) {
        let size = self.window.inner_size();
        let (cols, rows) = self.text.visible_grid(size);
        let (current_rows, current_cols) = self.terminal.size();
        if cols != current_cols || rows != current_rows {
            self.apply_terminal_size(cols, rows, false);
        }
        self.window.request_redraw();
    }

    fn apply_terminal_size(&mut self, cols: u16, rows: u16, resize_window: bool) {
        self.terminal.resize(rows, cols);
        self.text.resize_cache(rows);
        self.selection.clear();
        self.pty.resize(cols, rows);

        if resize_window {
            let metrics = self.text.metrics();
            let width = (cols as f32 * metrics.width + PADDING_X * 2.0).ceil() as u32;
            let height = (rows as f32 * metrics.height + PADDING_Y * 2.0).ceil() as u32;
            let _ = self
                .window
                .window
                .request_inner_size(PhysicalSize::new(width, height));
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
        let Some(clipboard) = self.clipboard.as_mut() else {
            return;
        };
        let Ok(text) = clipboard.get_text() else {
            return;
        };

        self.selection.clear();
        self.pty.write_all(text.as_bytes());
        self.window.request_redraw();
    }

    fn adjust_font_size(&mut self, delta: f32) {
        if self.text.adjust_font_size(delta) {
            self.sync_to_window_size();
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
        let resize_request = self.parser.take_resize_request();

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

        if let Some((rows, cols)) = resize_request {
            self.apply_terminal_size(cols.max(1), rows.max(1), true);
            changed = true;
        }

        if changed {
            self.window.request_redraw();
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
            prev_terminal: None,
            pty,
            text,
            modifiers: ModifiersState::default(),
            clipboard: Clipboard::new().ok(),
            selection: SelectionState::default(),
            cursor_position: None,
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
            WindowEvent::Resized(size) => {
                state.window.resize(size);
                state.sync_to_window_size();
            }
            WindowEvent::ModifiersChanged(modifiers) => {
                state.modifiers = modifiers.state();
            }
            WindowEvent::CursorMoved { position, .. } => {
                state.cursor_position = Some(position);
                if let Some(cell) =
                    cell_at_position(position, state.text.metrics(), &state.terminal)
                {
                    if state.selection.update(cell) {
                        state.window.request_redraw();
                    }
                }
            }
            WindowEvent::MouseInput {
                state: button_state,
                button: MouseButton::Left,
                ..
            } => match button_state {
                ElementState::Pressed => {
                    if let Some(position) = state.cursor_position {
                        if let Some(cell) =
                            cell_at_position(position, state.text.metrics(), &state.terminal)
                        {
                            state.selection.begin(cell);
                            state.window.request_redraw();
                        } else if state.selection.is_selected() {
                            state.selection.clear();
                            state.window.request_redraw();
                        }
                    }
                }
                ElementState::Released => {
                    if state.selection.finish() {
                        state.window.request_redraw();
                    }
                }
            },
            WindowEvent::KeyboardInput { event, .. } => {
                if event.state == ElementState::Pressed {
                    let is_copy = is_copy_shortcut(&event.logical_key, state.modifiers);
                    let is_paste = is_paste_shortcut(&event.logical_key, state.modifiers);
                    let is_font_increase =
                        is_font_increase_shortcut(&event.logical_key, state.modifiers);
                    let is_font_decrease =
                        is_font_decrease_shortcut(&event.logical_key, state.modifiers);

                    if is_copy {
                        state.copy_selection();
                        return;
                    }

                    if is_paste {
                        state.paste_clipboard();
                        return;
                    }

                    if is_font_increase {
                        state.adjust_font_size(FONT_SIZE_STEP);
                        return;
                    }

                    if is_font_decrease {
                        state.adjust_font_size(-FONT_SIZE_STEP);
                        return;
                    }

                    if let Some(bytes) = bytes_for_key_event(
                        &event,
                        state.modifiers,
                        state.terminal.application_cursor(),
                    ) {
                        state.selection.clear();
                        state.pty.write_all(&bytes);
                    }
                }
            }
            WindowEvent::RedrawRequested => {
                state.drain_pty();
                let dirty = state.terminal.dirty_rows(state.prev_terminal.as_ref());
                state.text.sync_terminal_rows(&state.terminal, &dirty);
                if let Err(error) =
                    state
                        .window
                        .render_terminal(&state.terminal, &state.text, &state.selection)
                {
                    eprintln!("{error:#}");
                }
                state.prev_terminal = Some(state.terminal.clone());
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
        if let Some(state) = self.state.as_ref() {
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

fn uses_command_shortcuts(modifiers: ModifiersState) -> bool {
    modifiers.super_key() && !modifiers.control_key() && !modifiers.alt_key()
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
    use super::{is_font_decrease_shortcut, is_font_increase_shortcut, uses_command_shortcuts};
    use winit::keyboard::{Key, ModifiersState};

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
}
