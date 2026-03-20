//! The display subsystem including window management, text shaping, and GPU drawing.

use std::cmp;
use std::fmt::{self, Formatter};
use std::time::{Duration, Instant};

use log::{info, warn};
use parking_lot::MutexGuard;
use serde::{Deserialize, Serialize};
use unicode_width::UnicodeWidthChar;
use vello::kurbo::Affine;
use vello::peniko::{Color, Fill};
use vello::{Glyph, Scene};
use winit::dpi::PhysicalSize;
use winit::keyboard::ModifiersState;
use winit::window::CursorIcon;

use cutty_terminal::event::{EventListener, OnResize, WindowSize};
use cutty_terminal::grid::Dimensions as TermDimensions;
use cutty_terminal::index::{Column, Direction, Line, Point};
use cutty_terminal::selection::Selection;
use cutty_terminal::term::cell::Flags;
use cutty_terminal::term::{
    self, LineDamageBounds, MIN_COLUMNS, MIN_SCREEN_LINES, Term, TermDamage, TermMode,
};
use cutty_terminal::vte::ansi::{CursorShape, NamedColor};

use crate::config::UiConfig;
use crate::config::font::{Font, FontSize};
use crate::config::window::Dimensions;
#[cfg(not(windows))]
use crate::config::window::StartupMode;
use crate::display::bell::VisualBell;
use crate::display::color::{List, Rgb};
use crate::display::content::{RenderableContent, RenderableCursor};
use crate::display::cursor::IntoRects;
use crate::display::damage::{DamageTracker, damage_y_to_viewport_y};
use crate::display::hint::{HintMatch, HintState};
use crate::display::meter::Meter;
use crate::display::rects::{RenderLine, RenderLines, RenderRect, paint_rect, paint_rects};
use crate::display::renderer::SceneRenderer;
use crate::display::text::{TextMetrics, TextSystem, color_from_rgb};
use crate::display::window::Window;
use crate::event::{Event, EventType, Mouse, SearchState};
use crate::message_bar::{MessageBuffer, MessageType};
use crate::scheduler::{Scheduler, TimerId, Topic};
use crate::string::{ShortenDirection, StrShortener};

pub mod color;
pub mod content;
pub mod cursor;
pub mod hint;
pub mod window;

mod bell;
mod damage;
mod meter;
mod rects;
mod renderer;
mod text;

/// Label for the forward terminal search bar.
const FORWARD_SEARCH_LABEL: &str = "Search: ";

/// Label for the backward terminal search bar.
const BACKWARD_SEARCH_LABEL: &str = "Backward Search: ";

/// The character used to shorten the visible text like uri preview or search regex.
const SHORTENER: char = '…';

/// Color which is used to highlight damaged rects when debugging.
const DAMAGE_RECT_COLOR: Rgb = Rgb::new(255, 0, 255);

#[derive(Debug)]
pub enum Error {
    Window(window::Error),
    Render(renderer::Error),
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Window(err) => err.source(),
            Error::Render(err) => err.source(),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Error::Window(err) => err.fmt(f),
            Error::Render(err) => err.fmt(f),
        }
    }
}

impl From<window::Error> for Error {
    fn from(val: window::Error) -> Self {
        Error::Window(val)
    }
}

impl From<renderer::Error> for Error {
    fn from(val: renderer::Error) -> Self {
        Error::Render(val)
    }
}

/// Terminal size info.
#[derive(Serialize, Deserialize, Debug, Copy, Clone, PartialEq, Eq)]
pub struct SizeInfo<T = f32> {
    width: T,
    height: T,
    cell_width: T,
    cell_height: T,
    padding_x: T,
    padding_y: T,
    screen_lines: usize,
    columns: usize,
}

impl From<SizeInfo<f32>> for SizeInfo<u32> {
    fn from(size_info: SizeInfo<f32>) -> Self {
        Self {
            width: size_info.width as u32,
            height: size_info.height as u32,
            cell_width: size_info.cell_width as u32,
            cell_height: size_info.cell_height as u32,
            padding_x: size_info.padding_x as u32,
            padding_y: size_info.padding_y as u32,
            screen_lines: size_info.screen_lines,
            columns: size_info.columns,
        }
    }
}

impl From<SizeInfo<f32>> for WindowSize {
    fn from(size_info: SizeInfo<f32>) -> Self {
        Self {
            num_cols: size_info.columns() as u16,
            num_lines: size_info.screen_lines() as u16,
            cell_width: size_info.cell_width() as u16,
            cell_height: size_info.cell_height() as u16,
        }
    }
}

impl<T: Clone + Copy> SizeInfo<T> {
    #[inline]
    pub fn width(&self) -> T {
        self.width
    }

    #[inline]
    pub fn height(&self) -> T {
        self.height
    }

    #[inline]
    pub fn cell_width(&self) -> T {
        self.cell_width
    }

    #[inline]
    pub fn cell_height(&self) -> T {
        self.cell_height
    }

    #[inline]
    pub fn padding_x(&self) -> T {
        self.padding_x
    }

    #[inline]
    pub fn padding_y(&self) -> T {
        self.padding_y
    }
}

impl SizeInfo<f32> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        width: f32,
        height: f32,
        cell_width: f32,
        cell_height: f32,
        mut padding_x: f32,
        mut padding_y: f32,
        dynamic_padding: bool,
    ) -> SizeInfo {
        if dynamic_padding {
            padding_x = Self::dynamic_padding(padding_x.floor(), width, cell_width);
            padding_y = Self::dynamic_padding(padding_y.floor(), height, cell_height);
        }

        let lines = (height - 2. * padding_y) / cell_height;
        let screen_lines = cmp::max(lines as usize, MIN_SCREEN_LINES);

        let columns = (width - 2. * padding_x) / cell_width;
        let columns = cmp::max(columns as usize, MIN_COLUMNS);

        SizeInfo {
            width,
            height,
            cell_width,
            cell_height,
            padding_x: padding_x.floor(),
            padding_y: padding_y.floor(),
            screen_lines,
            columns,
        }
    }

    #[inline]
    pub fn reserve_lines(&mut self, count: usize) {
        self.screen_lines = cmp::max(self.screen_lines.saturating_sub(count), MIN_SCREEN_LINES);
    }

    #[inline]
    pub fn contains_point(&self, x: usize, y: usize) -> bool {
        x <= (self.padding_x + self.columns as f32 * self.cell_width) as usize
            && x > self.padding_x as usize
            && y <= (self.padding_y + self.screen_lines as f32 * self.cell_height) as usize
            && y > self.padding_y as usize
    }

    #[inline]
    fn dynamic_padding(padding: f32, dimension: f32, cell_dimension: f32) -> f32 {
        padding + ((dimension - 2. * padding) % cell_dimension) / 2.
    }
}

impl TermDimensions for SizeInfo {
    #[inline]
    fn columns(&self) -> usize {
        self.columns
    }

    #[inline]
    fn screen_lines(&self) -> usize {
        self.screen_lines
    }

    #[inline]
    fn total_lines(&self) -> usize {
        self.screen_lines()
    }
}

#[derive(Default, Clone, Debug, PartialEq, Eq)]
pub struct DisplayUpdate {
    pub dirty: bool,
    dimensions: Option<PhysicalSize<u32>>,
    cursor_dirty: bool,
    font: Option<Font>,
}

impl DisplayUpdate {
    pub fn dimensions(&self) -> Option<PhysicalSize<u32>> {
        self.dimensions
    }

    pub fn font(&self) -> Option<&Font> {
        self.font.as_ref()
    }

    pub fn cursor_dirty(&self) -> bool {
        self.cursor_dirty
    }

    pub fn set_dimensions(&mut self, dimensions: PhysicalSize<u32>) {
        self.dimensions = Some(dimensions);
        self.dirty = true;
    }

    pub fn set_font(&mut self, font: Font) {
        self.font = Some(font);
        self.dirty = true;
    }

    pub fn set_cursor_dirty(&mut self) {
        self.cursor_dirty = true;
        self.dirty = true;
    }
}

pub struct Display {
    pub window: Window,
    pub size_info: SizeInfo,
    pub highlighted_hint: Option<HintMatch>,
    highlighted_hint_age: usize,
    pub vi_highlighted_hint: Option<HintMatch>,
    vi_highlighted_hint_age: usize,
    pub cursor_hidden: bool,
    pub visual_bell: VisualBell,
    pub colors: List,
    pub hint_state: HintState,
    pub pending_update: DisplayUpdate,
    pub pending_renderer_update: Option<RendererUpdate>,
    pub ime: Ime,
    pub frame_timer: FrameTimer,
    pub damage_tracker: DamageTracker,
    pub font_size: FontSize,

    hint_mouse_point: Option<Point>,
    scene_renderer: SceneRenderer,
    text_system: TextSystem,
    meter: Meter,
}

impl Display {
    pub fn new(window: Window, config: &UiConfig, _tabbed: bool) -> Result<Display, Error> {
        let scale_factor = window.scale_factor as f32;
        let font_size = config.font.size().scale(scale_factor);
        let font = config.font.clone().with_size(font_size);
        let text_system = TextSystem::new(font);
        let metrics = text_system.metrics();

        let mut viewport_size = window.inner_size();
        if let Some(dimensions) = config.window.dimensions() {
            viewport_size = window_size(
                config,
                dimensions,
                metrics.cell_width,
                metrics.cell_height,
                scale_factor,
            );
            window.request_inner_size(viewport_size);
        }

        let scene_renderer = SceneRenderer::new(window.winit_window(), viewport_size)?;
        let viewport_size = scene_renderer.clamp_render_size(viewport_size);
        let padding = config.window.padding(window.scale_factor as f32);
        let size_info = SizeInfo::new(
            viewport_size.width as f32,
            viewport_size.height as f32,
            metrics.cell_width,
            metrics.cell_height,
            padding.0,
            padding.1,
            config.window.dynamic_padding && config.window.dimensions().is_none(),
        );

        info!("Cell size: {} x {}", metrics.cell_width, metrics.cell_height);
        info!("Padding: {} x {}", size_info.padding_x(), size_info.padding_y());
        info!("Width: {}, Height: {}", size_info.width(), size_info.height());

        #[cfg(target_os = "macos")]
        window.set_has_shadow(config.window_opacity() >= 1.0);

        if config.window.resize_increments {
            window
                .set_resize_increments(PhysicalSize::new(metrics.cell_width, metrics.cell_height));
        }

        window.set_visible(true);

        #[cfg(target_os = "macos")]
        window.focus_window();

        #[allow(clippy::single_match)]
        #[cfg(not(windows))]
        if !_tabbed {
            match config.window.startup_mode {
                #[cfg(target_os = "macos")]
                StartupMode::SimpleFullscreen => window.set_simple_fullscreen(true),
                StartupMode::Maximized => window.set_maximized(true),
                _ => (),
            }
        }

        let hint_state = HintState::new(config.hints.alphabet());
        let mut damage_tracker = DamageTracker::new(size_info.screen_lines(), size_info.columns());
        damage_tracker.debug = config.debug.highlight_damage;

        Ok(Self {
            window,
            size_info,
            highlighted_hint: Default::default(),
            highlighted_hint_age: Default::default(),
            vi_highlighted_hint: Default::default(),
            vi_highlighted_hint_age: Default::default(),
            cursor_hidden: Default::default(),
            visual_bell: VisualBell::from(&config.bell),
            colors: List::from(&config.colors),
            hint_state,
            pending_update: Default::default(),
            pending_renderer_update: Default::default(),
            ime: Default::default(),
            frame_timer: FrameTimer::new(),
            damage_tracker,
            font_size,
            hint_mouse_point: Default::default(),
            scene_renderer,
            text_system,
            meter: Default::default(),
        })
    }

    pub fn handle_update<T>(
        &mut self,
        terminal: &mut Term<T>,
        pty_resize_handle: &mut dyn OnResize,
        message_buffer: &MessageBuffer,
        search_state: &mut SearchState,
        config: &UiConfig,
    ) where
        T: EventListener,
    {
        let pending_update = std::mem::take(&mut self.pending_update);
        let mut metrics = self.text_system.metrics();

        if let Some(font) = pending_update.font().cloned() {
            self.text_system.update_font(font);
            metrics = self.text_system.metrics();
            self.damage_tracker.frame().mark_fully_damaged();
        } else if pending_update.cursor_dirty() {
            self.text_system.clear_cache();
        }

        let (mut width, mut height) = (self.size_info.width(), self.size_info.height());
        if let Some(dimensions) = pending_update.dimensions() {
            let dimensions = self.scene_renderer.clamp_render_size(dimensions);
            width = dimensions.width as f32;
            height = dimensions.height as f32;
        }

        let padding = config.window.padding(self.window.scale_factor as f32);
        let mut new_size = SizeInfo::new(
            width,
            height,
            metrics.cell_width,
            metrics.cell_height,
            padding.0,
            padding.1,
            config.window.dynamic_padding,
        );

        let search_active = search_state.history_index.is_some();
        let message_bar_lines = message_buffer.message().map_or(0, |m| m.text(&new_size).len());
        let search_lines = usize::from(search_active);
        new_size.reserve_lines(message_bar_lines + search_lines);

        if config.window.resize_increments {
            self.window
                .set_resize_increments(PhysicalSize::new(metrics.cell_width, metrics.cell_height));
        }

        if self.size_info.screen_lines() != new_size.screen_lines
            || self.size_info.columns() != new_size.columns()
        {
            pty_resize_handle.on_resize(new_size.into());
            terminal.resize(new_size);
            self.damage_tracker.resize(new_size.screen_lines(), new_size.columns());
        }

        if new_size != self.size_info {
            let renderer_update = self.pending_renderer_update.get_or_insert(Default::default());
            renderer_update.resize = true;
            search_state.clear_focused_match();
        }

        self.size_info = new_size;
    }

    pub fn process_renderer_update(&mut self) {
        let renderer_update = match self.pending_renderer_update.take() {
            Some(update) => update,
            None => return,
        };

        if renderer_update.resize {
            self.scene_renderer.resize(PhysicalSize::new(
                self.size_info.width() as u32,
                self.size_info.height() as u32,
            ));
        }

        info!("Padding: {} x {}", self.size_info.padding_x(), self.size_info.padding_y());
        info!("Width: {}, Height: {}", self.size_info.width(), self.size_info.height());
    }

    pub fn draw<T: EventListener>(
        &mut self,
        mut terminal: MutexGuard<'_, Term<T>>,
        scheduler: &mut Scheduler,
        message_buffer: &MessageBuffer,
        config: &UiConfig,
        search_state: &mut SearchState,
    ) {
        let mut content = RenderableContent::new(config, self, &terminal, search_state);
        let mut grid_cells = Vec::new();
        for cell in &mut content {
            grid_cells.push(cell);
        }

        let selection_range = content.selection_range();
        let foreground_color = content.color(NamedColor::Foreground as usize);
        let background_color = content.color(NamedColor::Background as usize);
        let display_offset = content.display_offset();
        let cursor = content.cursor();

        let cursor_point = terminal.grid().cursor.point;
        let total_lines = terminal.grid().total_lines();
        let size_info = self.size_info;
        let metrics = self.text_system.metrics();

        let vi_mode = terminal.mode().contains(TermMode::VI);
        let vi_cursor_point = if vi_mode { Some(terminal.vi_mode_cursor.point) } else { None };

        match terminal.damage() {
            TermDamage::Full => self.damage_tracker.frame().mark_fully_damaged(),
            TermDamage::Partial(damaged_lines) => {
                for damage in damaged_lines {
                    self.damage_tracker.frame().damage_line(damage);
                }
            },
        }
        terminal.reset_damage();

        drop(terminal);

        self.validate_hint_highlights(display_offset);

        let requires_full_damage = self.visual_bell.intensity() != 0.
            || self.hint_state.active()
            || search_state.regex().is_some();
        if requires_full_damage {
            self.damage_tracker.frame().mark_fully_damaged();
            self.damage_tracker.next_frame().mark_fully_damaged();
        }

        let vi_cursor_viewport_point =
            vi_cursor_point.and_then(|cursor| term::point_to_viewport(display_offset, cursor));
        self.damage_tracker.damage_vi_cursor(vi_cursor_viewport_point);
        self.damage_tracker.damage_selection(selection_range, display_offset);

        let mut lines = RenderLines::new();
        let has_highlighted_hint =
            self.highlighted_hint.is_some() || self.vi_highlighted_hint.is_some();
        let highlighted_hint = self.highlighted_hint.clone();
        let vi_highlighted_hint = self.vi_highlighted_hint.clone();
        let mut prepared_cells = Vec::with_capacity(grid_cells.len());

        for mut cell in grid_cells {
            if has_highlighted_hint {
                let point = term::viewport_to_point(display_offset, cell.point);
                let hyperlink = cell.extra.as_ref().and_then(|extra| extra.hyperlink.as_ref());
                let should_highlight = |hint: &Option<HintMatch>| {
                    hint.as_ref().is_some_and(|hint| hint.should_highlight(point, hyperlink))
                };
                if should_highlight(&highlighted_hint) || should_highlight(&vi_highlighted_hint) {
                    self.damage_tracker.frame().damage_point(cell.point);
                    cell.flags.insert(Flags::UNDERLINE);
                }
            }

            lines.update(&cell);
            prepared_cells.push(cell);
        }

        let mut scene = Scene::new();

        let render_start = Instant::now();
        {
            let text_system = &mut self.text_system;

            for cell in &prepared_cells {
                Self::paint_cell_background(&mut scene, cell, size_info);
                Self::paint_cell_text(&mut scene, text_system, size_info, cell);
            }

            let mut rects = lines.rects(&metrics, &size_info);

            if let Some(vi_cursor_point) = vi_cursor_point {
                let line = (-vi_cursor_point.line.0 + size_info.bottommost_line().0) as usize;
                let obstructed_column = Some(vi_cursor_point)
                    .filter(|point| point.line == -(display_offset as i32))
                    .map(|point| point.column);
                self.draw_line_indicator(&mut scene, config, total_lines, obstructed_column, line);
            } else if search_state.regex().is_some() {
                self.draw_line_indicator(&mut scene, config, total_lines, None, display_offset);
            }

            rects.extend(cursor.rects(&size_info, config.cursor.thickness()));

            let visual_bell_intensity = self.visual_bell.intensity();
            if visual_bell_intensity != 0. {
                rects.push(RenderRect::new(
                    0.,
                    0.,
                    size_info.width(),
                    size_info.height(),
                    config.bell.color,
                    visual_bell_intensity as f32,
                ));
            }

            let ime_position = match search_state.regex() {
                Some(regex) => {
                    let search_label = match search_state.direction() {
                        Direction::Right => FORWARD_SEARCH_LABEL,
                        Direction::Left => BACKWARD_SEARCH_LABEL,
                    };
                    let search_text = Self::format_search(regex, search_label, size_info.columns());
                    self.draw_search(&mut scene, config, &search_text);

                    let line = size_info.screen_lines();
                    let column = Column(search_text.chars().count() - 1);
                    if self.ime.preedit().is_none() {
                        let fg = config.colors.footer_bar_foreground();
                        let shape = CursorShape::Underline;
                        let cursor_width = std::num::NonZeroU32::new(1).unwrap();
                        let cursor = RenderableCursor::new(
                            Point::new(line, column),
                            shape,
                            fg,
                            cursor_width,
                        );
                        rects.extend(cursor.rects(&size_info, config.cursor.thickness()));
                    }

                    Some(Point::new(line, column))
                },
                None => {
                    let num_lines = self.size_info.screen_lines();
                    match vi_cursor_viewport_point {
                        None => term::point_to_viewport(display_offset, cursor_point)
                            .filter(|point| point.line < num_lines),
                        point => point,
                    }
                },
            };

            if self.ime.is_enabled() {
                if let Some(point) = ime_position {
                    let (fg, bg) = if search_state.regex().is_some() {
                        (
                            config.colors.footer_bar_foreground(),
                            config.colors.footer_bar_background(),
                        )
                    } else {
                        (foreground_color, background_color)
                    };
                    self.draw_ime_preview(&mut scene, point, fg, bg, &mut rects, config);
                }
            }

            if let Some(message) = message_buffer.message() {
                let search_offset = usize::from(search_state.regex().is_some());
                let text = message.text(&size_info);
                let start_line = size_info.screen_lines() + search_offset;
                let y = size_info.cell_height().mul_add(start_line as f32, size_info.padding_y());
                let bg = match message.ty() {
                    MessageType::Error => config.colors.normal.red,
                    MessageType::Warning => config.colors.normal.yellow,
                };
                let x = 0;
                let width = size_info.width() as i32;
                let height = (size_info.height() - y) as i32;
                let message_bar_rect =
                    RenderRect::new(x as f32, y, width as f32, height as f32, bg, 1.);
                rects.push(message_bar_rect);

                self.damage_tracker
                    .frame()
                    .add_viewport_rect(&size_info, x, y as i32, width, height);

                paint_rects(&mut scene, rects);

                let fg = config.colors.primary.background;
                for (i, message_text) in text.iter().enumerate() {
                    let point = Point::new(start_line + i, Column(0));
                    self.paint_string_cells(&mut scene, point, fg, bg, message_text.chars());
                }
            } else {
                paint_rects(&mut scene, rects);
            }

            self.draw_render_timer(&mut scene, config);

            if has_highlighted_hint {
                let cursor_point = vi_cursor_point.or(Some(cursor_point));
                self.draw_hyperlink_preview(&mut scene, config, cursor_point, display_offset);
            }

            if self.damage_tracker.debug {
                let mut rects = Vec::new();
                self.highlight_damage(&mut rects);
                for rect in rects {
                    paint_rect(&mut scene, &rect);
                }
            }
        }
        self.meter.record(render_start.elapsed());

        self.window.pre_present_notify();

        let base_color = Color::from_rgba8(
            background_color.r,
            background_color.g,
            background_color.b,
            (config.window_opacity() * 255.) as u8,
        );
        if let Err(err) = self.scene_renderer.render(&scene, base_color) {
            warn!("{err}");
        }

        self.request_frame(scheduler);
        self.damage_tracker.swap_damage();
    }

    pub fn update_config(&mut self, config: &UiConfig) {
        self.damage_tracker.debug = config.debug.highlight_damage;
        self.visual_bell.update_config(&config.bell);
        self.colors = List::from(&config.colors);
    }

    pub fn update_highlighted_hints<T>(
        &mut self,
        term: &Term<T>,
        config: &UiConfig,
        mouse: &Mouse,
        modifiers: ModifiersState,
    ) -> bool {
        let vi_highlighted_hint = if term.mode().contains(TermMode::VI) {
            let mods = ModifiersState::all();
            let point = term.vi_mode_cursor.point;
            hint::highlighted_at(term, config, point, mods)
        } else {
            None
        };
        let mut dirty = vi_highlighted_hint != self.vi_highlighted_hint;
        self.vi_highlighted_hint = vi_highlighted_hint;
        self.vi_highlighted_hint_age = 0;

        if dirty {
            self.damage_tracker.frame().mark_fully_damaged();
        }

        if !self.window.mouse_visible()
            || !mouse.inside_text_area
            || !term.selection.as_ref().is_none_or(Selection::is_empty)
        {
            if self.highlighted_hint.take().is_some() {
                self.damage_tracker.frame().mark_fully_damaged();
                dirty = true;
            }
            return dirty;
        }

        let point = mouse.point(&self.size_info, term.grid().display_offset());
        let highlighted_hint = hint::highlighted_at(term, config, point, modifiers);

        if highlighted_hint.is_some() {
            dirty = self.hint_mouse_point.is_some_and(|p| p.line != point.line);
            self.hint_mouse_point = Some(point);
            self.window.set_mouse_cursor(CursorIcon::Pointer);
        } else if self.highlighted_hint.is_some() {
            self.hint_mouse_point = None;
            if term.mode().intersects(TermMode::MOUSE_MODE) && !term.mode().contains(TermMode::VI) {
                self.window.set_mouse_cursor(CursorIcon::Default);
            } else {
                self.window.set_mouse_cursor(CursorIcon::Text);
            }
        }

        let mouse_highlight_dirty = self.highlighted_hint != highlighted_hint;
        dirty |= mouse_highlight_dirty;
        self.highlighted_hint = highlighted_hint;
        self.highlighted_hint_age = 0;

        if mouse_highlight_dirty {
            self.damage_tracker.frame().mark_fully_damaged();
        }

        dirty
    }

    fn paint_cell_background(
        scene: &mut Scene,
        cell: &crate::display::content::RenderableCell,
        size: SizeInfo,
    ) {
        if cell.bg_alpha <= 0.0 {
            return;
        }

        let width_cells = if cell.flags.contains(Flags::WIDE_CHAR) { 2.0 } else { 1.0 };
        let rect = RenderRect::new(
            size.padding_x() + cell.point.column.0 as f32 * size.cell_width(),
            size.padding_y() + cell.point.line as f32 * size.cell_height(),
            size.cell_width() * width_cells,
            size.cell_height(),
            cell.bg,
            cell.bg_alpha,
        );
        paint_rect(scene, &rect);
    }

    fn paint_cell_text(
        scene: &mut Scene,
        text_system: &mut TextSystem,
        size: SizeInfo,
        cell: &crate::display::content::RenderableCell,
    ) {
        let Some(layout) = text_system.shape_cell(cell) else {
            return;
        };
        Self::paint_layout(
            scene,
            &layout,
            text_system.metrics(),
            size,
            cell.point.line,
            cell.point.column.0,
            cell.fg,
        );
    }

    fn paint_layout(
        scene: &mut Scene,
        layout: &parley::Layout<()>,
        metrics: TextMetrics,
        size: SizeInfo,
        line: usize,
        column: usize,
        fg: Rgb,
    ) {
        let transform = Affine::translate((
            (size.padding_x() + column as f32 * size.cell_width() + metrics.glyph_offset_x) as f64,
            (size.padding_y() + line as f32 * size.cell_height() + metrics.glyph_offset_y) as f64,
        ));
        let brush = vello::peniko::Brush::Solid(color_from_rgb(fg));

        for line in layout.lines() {
            for item in line.items() {
                let parley::layout::PositionedLayoutItem::GlyphRun(glyph_run) = item else {
                    continue;
                };

                let run = glyph_run.run();
                let font = run.font();
                let font_size = run.font_size();
                let mut x = glyph_run.offset();
                let y = glyph_run.baseline();

                scene
                    .draw_glyphs(font)
                    .brush(&brush)
                    .hint(false)
                    .transform(transform)
                    .font_size(font_size)
                    .normalized_coords(run.normalized_coords())
                    .draw(
                        Fill::NonZero,
                        glyph_run.glyphs().map(|glyph| scene_glyph_from_layout(&mut x, y, glyph)),
                    );
            }
        }
    }

    fn paint_string_cells(
        &mut self,
        scene: &mut Scene,
        point: Point<usize>,
        fg: Rgb,
        bg: Rgb,
        chars: impl Iterator<Item = char>,
    ) {
        let mut column = point.column.0;
        for character in chars {
            let width = character.width().unwrap_or(1).max(1);
            let rect = RenderRect::new(
                self.size_info.padding_x() + column as f32 * self.size_info.cell_width(),
                self.size_info.padding_y() + point.line as f32 * self.size_info.cell_height(),
                self.size_info.cell_width() * width as f32,
                self.size_info.cell_height(),
                bg,
                1.0,
            );
            paint_rect(scene, &rect);

            if !character.is_whitespace() {
                if let Some(layout) =
                    self.text_system.shape_string(character.to_string(), false, false)
                {
                    Self::paint_layout(
                        scene,
                        &layout,
                        self.text_system.metrics(),
                        self.size_info,
                        point.line,
                        column,
                        fg,
                    );
                }
            }

            column += width;
        }
    }

    fn draw_ime_preview(
        &mut self,
        scene: &mut Scene,
        point: Point<usize>,
        fg: Rgb,
        bg: Rgb,
        rects: &mut Vec<RenderRect>,
        config: &UiConfig,
    ) {
        let preedit = match self.ime.preedit().cloned() {
            Some(preedit) => preedit,
            None => {
                self.window.update_ime_position(point, &self.size_info);
                return;
            },
        };

        let num_cols = self.size_info.columns();
        let visible_text: String = match (preedit.cursor_byte_offset, preedit.cursor_end_offset) {
            (Some(byte_offset), Some(end_offset)) if end_offset.0 > num_cols => StrShortener::new(
                &preedit.text[byte_offset.0..],
                num_cols,
                ShortenDirection::Right,
                Some(SHORTENER),
            ),
            _ => {
                StrShortener::new(&preedit.text, num_cols, ShortenDirection::Left, Some(SHORTENER))
            },
        }
        .collect();

        let visible_len = visible_text.chars().count();
        let end = cmp::min(point.column.0 + visible_len, num_cols);
        let start = end.saturating_sub(visible_len);

        let start = Point::new(point.line, Column(start));
        let end = Point::new(point.line, Column(end - 1));

        self.paint_string_cells(scene, start, fg, bg, visible_text.chars());

        if point.line < self.size_info.screen_lines() {
            let damage = LineDamageBounds::new(start.line, 0, num_cols);
            self.damage_tracker.frame().damage_line(damage);
            self.damage_tracker.next_frame().damage_line(damage);
        }

        let underline = RenderLine { start, end, color: fg };
        rects.extend(underline.rects(
            &self.text_system.metrics(),
            &self.size_info,
            Flags::UNDERLINE,
        ));

        let ime_popup_point = match preedit.cursor_end_offset {
            Some(cursor_end_offset) => {
                let (shape, width) = if let Some(width) =
                    std::num::NonZeroU32::new((cursor_end_offset.0 - cursor_end_offset.1) as u32)
                {
                    (CursorShape::HollowBlock, width)
                } else {
                    (CursorShape::Beam, std::num::NonZeroU32::new(1).unwrap())
                };

                let cursor_column = Column(
                    (end.column.0 as isize - cursor_end_offset.0 as isize + 1).max(0) as usize,
                );
                let cursor_point = Point::new(point.line, cursor_column);
                let cursor = RenderableCursor::new(cursor_point, shape, fg, width);
                rects.extend(cursor.rects(&self.size_info, config.cursor.thickness()));
                cursor_point
            },
            _ => end,
        };

        self.window.update_ime_position(ime_popup_point, &self.size_info);
    }

    fn format_search(search_regex: &str, search_label: &str, max_width: usize) -> String {
        let label_len = search_label.len();
        if label_len > max_width {
            return search_label[..max_width].to_owned();
        }

        let mut bar_text = String::from(search_label);
        bar_text.extend(StrShortener::new(
            search_regex,
            max_width.wrapping_sub(label_len + 1),
            ShortenDirection::Left,
            Some(SHORTENER),
        ));
        bar_text.push(' ');
        bar_text
    }

    fn draw_hyperlink_preview(
        &mut self,
        scene: &mut Scene,
        config: &UiConfig,
        cursor_point: Option<Point>,
        display_offset: usize,
    ) {
        let num_cols = self.size_info.columns();
        let uris: Vec<String> = self
            .highlighted_hint
            .iter()
            .chain(&self.vi_highlighted_hint)
            .filter_map(|hint| hint.hyperlink().map(|hyperlink| hyperlink.uri()))
            .map(|uri| {
                StrShortener::new(uri, num_cols, ShortenDirection::Right, Some(SHORTENER)).collect()
            })
            .collect();

        if uris.is_empty() {
            return;
        }

        let max_protected_lines = uris.len() * 2;
        let mut protected_lines = Vec::with_capacity(max_protected_lines);
        if self.size_info.screen_lines() > max_protected_lines {
            protected_lines.push(self.hint_mouse_point.map(|point| point.line));
            protected_lines.push(cursor_point.map(|point| point.line));
        }

        let viewport_bottom = self.size_info.bottommost_line() - Line(display_offset as i32);
        let viewport_top = viewport_bottom - (self.size_info.screen_lines() - 1);
        let uri_lines = (viewport_top.0..=viewport_bottom.0)
            .rev()
            .map(|line| Some(Line(line)))
            .filter_map(|line| {
                if protected_lines.contains(&line) {
                    None
                } else {
                    protected_lines.push(line);
                    line
                }
            })
            .take(uris.len())
            .flat_map(|line| term::point_to_viewport(display_offset, Point::new(line, Column(0))));

        let fg = config.colors.footer_bar_foreground();
        let bg = config.colors.footer_bar_background();
        for (uri, point) in uris.into_iter().zip(uri_lines) {
            let damage = LineDamageBounds::new(point.line, point.column.0, num_cols);
            self.damage_tracker.frame().damage_line(damage);
            self.damage_tracker.next_frame().damage_line(damage);
            self.paint_string_cells(scene, point, fg, bg, uri.chars());
        }
    }

    fn draw_search(&mut self, scene: &mut Scene, config: &UiConfig, text: &str) {
        let num_cols = self.size_info.columns();
        let text = format!("{text:<num_cols$}");
        let point = Point::new(self.size_info.screen_lines(), Column(0));
        self.paint_string_cells(
            scene,
            point,
            config.colors.footer_bar_foreground(),
            config.colors.footer_bar_background(),
            text.chars(),
        );
    }

    fn draw_render_timer(&mut self, scene: &mut Scene, config: &UiConfig) {
        if !config.debug.render_timer {
            return;
        }

        let timing = format!("{:.3} usec", self.meter.average());
        let point = Point::new(self.size_info.screen_lines().saturating_sub(2), Column(0));
        let damage = LineDamageBounds::new(point.line, point.column.0, timing.len());
        self.damage_tracker.frame().damage_line(damage);
        self.damage_tracker.next_frame().damage_line(damage);
        self.paint_string_cells(
            scene,
            point,
            config.colors.primary.background,
            config.colors.normal.red,
            timing.chars(),
        );
    }

    fn draw_line_indicator(
        &mut self,
        scene: &mut Scene,
        config: &UiConfig,
        total_lines: usize,
        obstructed_column: Option<Column>,
        line: usize,
    ) {
        let columns = self.size_info.columns();
        let text = format!("[{}/{}]", line, total_lines - 1);
        let column = Column(self.size_info.columns().saturating_sub(text.len()));
        let point = Point::new(0, column);
        let damage = LineDamageBounds::new(point.line, point.column.0, columns - 1);
        self.damage_tracker.frame().damage_line(damage);
        self.damage_tracker.next_frame().damage_line(damage);

        let colors = &config.colors;
        let fg = colors.line_indicator.foreground.unwrap_or(colors.primary.background);
        let bg = colors.line_indicator.background.unwrap_or(colors.primary.foreground);

        if obstructed_column.is_none_or(|obstructed_column| obstructed_column < column) {
            self.paint_string_cells(scene, point, fg, bg, text.chars());
        }
    }

    fn highlight_damage(&self, render_rects: &mut Vec<RenderRect>) {
        for damage_rect in &self.damage_tracker.shape_frame_damage(self.size_info.into()) {
            let render_rect = RenderRect::new(
                damage_rect.x as f32,
                damage_y_to_viewport_y(&self.size_info, damage_rect) as f32,
                damage_rect.width as f32,
                damage_rect.height as f32,
                DAMAGE_RECT_COLOR,
                0.5,
            );
            render_rects.push(render_rect);
        }
    }

    fn validate_hint_highlights(&mut self, display_offset: usize) {
        let frame = self.damage_tracker.frame();
        let hints = [
            (&mut self.highlighted_hint, &mut self.highlighted_hint_age, true),
            (&mut self.vi_highlighted_hint, &mut self.vi_highlighted_hint_age, false),
        ];

        let num_lines = self.size_info.screen_lines();
        for (hint, hint_age, reset_mouse) in hints {
            let (start, end) = match hint {
                Some(hint) => (*hint.bounds().start(), *hint.bounds().end()),
                None => continue,
            };

            *hint_age += 1;
            if *hint_age == 1 {
                continue;
            }

            let start = term::point_to_viewport(display_offset, start)
                .filter(|point| point.line < num_lines)
                .unwrap_or_default();
            let end = term::point_to_viewport(display_offset, end)
                .filter(|point| point.line < num_lines)
                .unwrap_or_else(|| Point::new(num_lines - 1, self.size_info.last_column()));

            if frame.intersects(start, end) {
                if reset_mouse {
                    self.window.set_mouse_cursor(CursorIcon::Default);
                }
                frame.mark_fully_damaged();
                *hint = None;
            }
        }
    }

    fn request_frame(&mut self, scheduler: &mut Scheduler) {
        self.window.has_frame = false;

        let monitor_vblank_interval = 1_000_000.
            / self
                .window
                .current_monitor()
                .and_then(|monitor| monitor.refresh_rate_millihertz())
                .unwrap_or(60_000) as f64;
        let monitor_vblank_interval =
            Duration::from_micros((1000. * monitor_vblank_interval) as u64);

        let swap_timeout = self.frame_timer.compute_timeout(monitor_vblank_interval);
        let window_id = self.window.id();
        let timer_id = TimerId::new(Topic::Frame, window_id);
        let event = Event::new(EventType::Frame, window_id);
        scheduler.schedule(event, swap_timeout, false, timer_id);
    }
}

fn scene_glyph_from_layout(
    cursor_x: &mut f32,
    baseline: f32,
    glyph: parley::layout::Glyph,
) -> Glyph {
    let positioned = Glyph { id: glyph.id, x: *cursor_x + glyph.x, y: baseline - glyph.y };
    *cursor_x += glyph.advance;
    positioned
}

#[cfg(test)]
mod tests {
    use super::scene_glyph_from_layout;

    #[test]
    fn scene_glyphs_use_baseline_relative_y_coordinates() {
        let mut cursor_x = 10.0;
        let glyph = parley::layout::Glyph { id: 42, style_index: 0, x: 1.5, y: 2.0, advance: 8.0 };

        let positioned = scene_glyph_from_layout(&mut cursor_x, 20.0, glyph);

        assert_eq!(positioned.id, 42);
        assert_eq!(positioned.x, 11.5);
        assert_eq!(positioned.y, 18.0);
        assert_eq!(cursor_x, 18.0);
    }
}

#[derive(Debug, Default)]
pub struct Ime {
    enabled: bool,
    preedit: Option<Preedit>,
}

impl Ime {
    #[inline]
    pub fn set_enabled(&mut self, is_enabled: bool) {
        if is_enabled {
            self.enabled = is_enabled
        } else {
            *self = Default::default();
        }
    }

    #[inline]
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    #[inline]
    pub fn set_preedit(&mut self, preedit: Option<Preedit>) {
        self.preedit = preedit;
    }

    #[inline]
    pub fn preedit(&self) -> Option<&Preedit> {
        self.preedit.as_ref()
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Preedit {
    text: String,
    cursor_byte_offset: Option<(usize, usize)>,
    cursor_end_offset: Option<(usize, usize)>,
}

impl Preedit {
    pub fn new(text: String, cursor_byte_offset: Option<(usize, usize)>) -> Self {
        let cursor_end_offset = if let Some(byte_offset) = cursor_byte_offset {
            let start_to_end_offset =
                text[byte_offset.0..].chars().fold(0, |acc, ch| acc + ch.width().unwrap_or(1));
            let end_to_end_offset =
                text[byte_offset.1..].chars().fold(0, |acc, ch| acc + ch.width().unwrap_or(1));
            Some((start_to_end_offset, end_to_end_offset))
        } else {
            None
        };

        Self { text, cursor_byte_offset, cursor_end_offset }
    }
}

#[derive(Debug, Default, Copy, Clone)]
pub struct RendererUpdate {
    resize: bool,
}

pub struct FrameTimer {
    base: Instant,
    last_synced_timestamp: Instant,
    refresh_interval: Duration,
}

impl FrameTimer {
    pub fn new() -> Self {
        let now = Instant::now();
        Self { base: now, last_synced_timestamp: now, refresh_interval: Duration::ZERO }
    }

    pub fn compute_timeout(&mut self, refresh_interval: Duration) -> Duration {
        let now = Instant::now();

        if self.refresh_interval != refresh_interval {
            self.base = now;
            self.last_synced_timestamp = now;
            self.refresh_interval = refresh_interval;
            return refresh_interval;
        }

        let next_frame = self.last_synced_timestamp + self.refresh_interval;
        if next_frame < now {
            let elapsed_micros = (now - self.base).as_micros() as u64;
            let refresh_micros = self.refresh_interval.as_micros() as u64;
            self.last_synced_timestamp =
                now - Duration::from_micros(elapsed_micros % refresh_micros);
            Duration::ZERO
        } else {
            self.last_synced_timestamp = next_frame;
            next_frame - now
        }
    }
}

fn window_size(
    config: &UiConfig,
    dimensions: Dimensions,
    cell_width: f32,
    cell_height: f32,
    scale_factor: f32,
) -> PhysicalSize<u32> {
    let padding = config.window.padding(scale_factor);
    let grid_width = cell_width * dimensions.columns.max(MIN_COLUMNS) as f32;
    let grid_height = cell_height * dimensions.lines.max(MIN_SCREEN_LINES) as f32;
    let width = padding.0.mul_add(2., grid_width).floor();
    let height = padding.1.mul_add(2., grid_height).floor();
    PhysicalSize::new(width as u32, height as u32)
}
