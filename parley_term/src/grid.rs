use std::cmp;
use std::num::NonZeroU32;

use bitflags::bitflags;
use unicode_width::UnicodeWidthStr as _;

use crate::color::Rgb;

bitflags! {
    #[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
    pub struct CellFlags: u16 {
        const BOLD = 1 << 0;
        const ITALIC = 1 << 1;
        const HIDDEN = 1 << 2;
        const UNDERLINE = 1 << 3;
        const DOUBLE_UNDERLINE = 1 << 4;
        const STRIKEOUT = 1 << 5;
        const UNDERCURL = 1 << 6;
        const DOTTED_UNDERLINE = 1 << 7;
        const DASHED_UNDERLINE = 1 << 8;
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum CursorShape {
    Hidden,
    Block,
    HollowBlock,
    Beam,
    Underline,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TerminalCell {
    pub row: usize,
    pub column: usize,
    pub text: Box<str>,
    pub width: NonZeroU32,
    pub fg: Rgb,
    pub bg: Rgb,
    pub bg_alpha: f32,
    pub underline: Rgb,
    pub flags: CellFlags,
}

impl TerminalCell {
    pub fn new(row: usize, column: usize, text: impl Into<Box<str>>, fg: Rgb, bg: Rgb) -> Self {
        let text = text.into();
        let width = NonZeroU32::new(cell_text_width(&text) as u32).unwrap_or(NonZeroU32::MIN);
        Self {
            row,
            column,
            text,
            width,
            fg,
            bg,
            bg_alpha: 1.0,
            underline: fg,
            flags: CellFlags::empty(),
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct SceneCursor {
    pub row: usize,
    pub column: usize,
    pub width: NonZeroU32,
    pub shape: CursorShape,
    pub color: Rgb,
    pub text_color: Rgb,
}

impl SceneCursor {
    pub fn new(row: usize, column: usize, shape: CursorShape, color: Rgb) -> Self {
        Self {
            row,
            column,
            width: NonZeroU32::MIN,
            shape,
            color,
            text_color: Rgb::new(0, 0, 0),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TerminalGrid {
    pub columns: usize,
    pub rows: usize,
    pub cells: Vec<TerminalCell>,
    pub cursor: Option<SceneCursor>,
}

impl TerminalGrid {
    pub fn new(columns: usize, rows: usize) -> Self {
        Self { columns, rows, cells: Vec::new(), cursor: None }
    }
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub struct SizeInfo {
    width: f32,
    height: f32,
    cell_width: f32,
    cell_height: f32,
    padding_x: f32,
    padding_y: f32,
    screen_lines: usize,
    columns: usize,
}

impl SizeInfo {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        width: f32,
        height: f32,
        cell_width: f32,
        cell_height: f32,
        mut padding_x: f32,
        mut padding_y: f32,
        dynamic_padding: bool,
    ) -> Self {
        if dynamic_padding {
            padding_x = Self::dynamic_padding(padding_x.floor(), width, cell_width);
            padding_y = Self::dynamic_padding(padding_y.floor(), height, cell_height);
        }

        let lines = (height - 2. * padding_y) / cell_height;
        let columns = (width - 2. * padding_x) / cell_width;

        Self {
            width,
            height,
            cell_width,
            cell_height,
            padding_x: padding_x.floor(),
            padding_y: padding_y.floor(),
            screen_lines: cmp::max(lines as usize, 1),
            columns: cmp::max(columns as usize, 1),
        }
    }

    pub fn width(&self) -> f32 {
        self.width
    }

    pub fn height(&self) -> f32 {
        self.height
    }

    pub fn cell_width(&self) -> f32 {
        self.cell_width
    }

    pub fn cell_height(&self) -> f32 {
        self.cell_height
    }

    pub fn padding_x(&self) -> f32 {
        self.padding_x
    }

    pub fn padding_y(&self) -> f32 {
        self.padding_y
    }

    pub fn screen_lines(&self) -> usize {
        self.screen_lines
    }

    pub fn columns(&self) -> usize {
        self.columns
    }

    pub fn last_column(&self) -> usize {
        self.columns.saturating_sub(1)
    }

    fn dynamic_padding(padding: f32, dimension: f32, cell_dimension: f32) -> f32 {
        padding + ((dimension - 2. * padding) % cell_dimension) / 2.
    }
}

fn cell_text_width(text: &str) -> usize {
    text.width().max(1)
}
