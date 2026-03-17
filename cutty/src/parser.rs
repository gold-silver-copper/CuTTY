use std::mem;

use vte::ansi::{
    self, Attr, CharsetIndex, ClearMode, Color, Handler, LineClearMode, NamedColor, PrivateMode,
    StandardCharset, TabulationClearMode,
};

use crate::terminal::{TerminalColor, TerminalState};

#[derive(Debug, Default)]
pub struct ParserCallbacks {
    responses: Vec<Vec<u8>>,
    pending_resize: Option<(u16, u16)>,
    pending_title: Option<String>,
}

impl ParserCallbacks {
    pub fn take_responses(&mut self) -> Vec<Vec<u8>> {
        mem::take(&mut self.responses)
    }

    pub fn take_resize_request(&mut self) -> Option<(u16, u16)> {
        self.pending_resize.take()
    }

    pub fn take_title(&mut self) -> Option<String> {
        self.pending_title.take()
    }
}

pub struct AnsiParser {
    parser: ansi::Processor,
    callbacks: ParserCallbacks,
    active_charset: CharsetIndex,
    charsets: [StandardCharset; 4],
    current_title: String,
    title_stack: Vec<String>,
}

impl Default for AnsiParser {
    fn default() -> Self {
        Self {
            parser: ansi::Processor::new(),
            callbacks: ParserCallbacks::default(),
            active_charset: CharsetIndex::G0,
            charsets: [StandardCharset::Ascii; 4],
            current_title: String::new(),
            title_stack: Vec::new(),
        }
    }
}

impl AnsiParser {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn process(&mut self, terminal: &mut TerminalState, bytes: &[u8]) {
        let mut handler = TerminalHandler {
            terminal,
            callbacks: &mut self.callbacks,
            active_charset: &mut self.active_charset,
            charsets: &mut self.charsets,
            current_title: &mut self.current_title,
            title_stack: &mut self.title_stack,
        };
        self.parser.advance(&mut handler, bytes);
    }

    pub fn take_responses(&mut self) -> Vec<Vec<u8>> {
        self.callbacks.take_responses()
    }

    pub fn take_resize_request(&mut self) -> Option<(u16, u16)> {
        self.callbacks.take_resize_request()
    }

    pub fn take_title(&mut self) -> Option<String> {
        self.callbacks.take_title()
    }
}

struct TerminalHandler<'a> {
    terminal: &'a mut TerminalState,
    callbacks: &'a mut ParserCallbacks,
    active_charset: &'a mut CharsetIndex,
    charsets: &'a mut [StandardCharset; 4],
    current_title: &'a mut String,
    title_stack: &'a mut Vec<String>,
}

impl Handler for TerminalHandler<'_> {
    fn set_title(&mut self, title: Option<String>) {
        let Some(title) = title else {
            return;
        };

        *self.current_title = title.clone();
        self.callbacks.pending_title = Some(title);
    }

    fn input(&mut self, c: char) {
        let mapped = self.charsets[charset_slot(*self.active_charset)].map(c);
        self.terminal.print(mapped);
    }

    fn goto(&mut self, line: i32, col: usize) {
        self.terminal
            .set_cursor_position(cursor_row(line), to_u16(col));
    }

    fn goto_line(&mut self, line: i32) {
        self.terminal.set_cursor_row(cursor_row(line));
    }

    fn goto_col(&mut self, col: usize) {
        self.terminal.set_cursor_col(to_u16(col));
    }

    fn insert_blank(&mut self, count: usize) {
        self.terminal.insert_blank_chars(to_u16(count));
    }

    fn move_up(&mut self, rows: usize) {
        self.terminal.cursor_up(to_u16(rows));
    }

    fn move_down(&mut self, rows: usize) {
        self.terminal.cursor_down(to_u16(rows));
    }

    fn device_status(&mut self, status: usize) {
        match status {
            5 => self.callbacks.responses.push(b"\x1b[0n".to_vec()),
            6 => {
                let (row, col) = self.terminal.cursor_position();
                self.callbacks
                    .responses
                    .push(format!("\x1b[{};{}R", row + 1, col + 1).into_bytes());
            }
            _ => {}
        }
    }

    fn move_forward(&mut self, cols: usize) {
        self.terminal.cursor_forward(to_u16(cols));
    }

    fn move_backward(&mut self, cols: usize) {
        self.terminal.cursor_back(to_u16(cols));
    }

    fn move_down_and_cr(&mut self, rows: usize) {
        self.terminal.cursor_next_line(to_u16(rows));
    }

    fn move_up_and_cr(&mut self, rows: usize) {
        self.terminal.cursor_prev_line(to_u16(rows));
    }

    fn put_tab(&mut self, count: u16) {
        self.terminal.move_forward_tabs(count);
    }

    fn backspace(&mut self) {
        self.terminal.backspace();
    }

    fn carriage_return(&mut self) {
        self.terminal.carriage_return();
    }

    fn linefeed(&mut self) {
        self.terminal.linefeed();
    }

    fn scroll_up(&mut self, rows: usize) {
        self.terminal.scroll_up(to_u16(rows));
    }

    fn scroll_down(&mut self, rows: usize) {
        self.terminal.scroll_down(to_u16(rows));
    }

    fn insert_blank_lines(&mut self, count: usize) {
        self.terminal.insert_lines(to_u16(count));
    }

    fn delete_lines(&mut self, count: usize) {
        self.terminal.delete_lines(to_u16(count));
    }

    fn erase_chars(&mut self, count: usize) {
        self.terminal.erase_chars(to_u16(count));
    }

    fn delete_chars(&mut self, count: usize) {
        self.terminal.delete_chars(to_u16(count));
    }

    fn move_backward_tabs(&mut self, count: u16) {
        self.terminal.move_backward_tabs(count);
    }

    fn move_forward_tabs(&mut self, count: u16) {
        self.terminal.move_forward_tabs(count);
    }

    fn save_cursor_position(&mut self) {
        self.terminal.save_cursor();
    }

    fn restore_cursor_position(&mut self) {
        self.terminal.restore_cursor();
    }

    fn clear_line(&mut self, mode: LineClearMode) {
        let mode = match mode {
            LineClearMode::Right => 0,
            LineClearMode::Left => 1,
            LineClearMode::All => 2,
        };
        self.terminal.erase_in_line(mode);
    }

    fn clear_screen(&mut self, mode: ClearMode) {
        let mode = match mode {
            ClearMode::Below => 0,
            ClearMode::Above => 1,
            ClearMode::All => 2,
            ClearMode::Saved => 3,
        };
        self.terminal.erase_in_display(mode);
    }

    fn clear_tabs(&mut self, mode: TabulationClearMode) {
        match mode {
            TabulationClearMode::Current => self.terminal.clear_current_tab_stop(),
            TabulationClearMode::All => self.terminal.clear_all_tab_stops(),
        }
    }

    fn set_tabs(&mut self, interval: u16) {
        self.terminal.set_default_tab_stops(interval);
    }

    fn reset_state(&mut self) {
        self.terminal.reset();
        *self.active_charset = CharsetIndex::G0;
        *self.charsets = [StandardCharset::Ascii; 4];
    }

    fn reverse_index(&mut self) {
        self.terminal.reverse_index();
    }

    fn terminal_attribute(&mut self, attr: Attr) {
        match attr {
            Attr::Reset => self.terminal.set_attr_reset(),
            Attr::Bold => self.terminal.set_bold(true),
            Attr::Dim => self.terminal.set_dim(true),
            Attr::Italic => self.terminal.set_italic(true),
            Attr::Underline
            | Attr::DoubleUnderline
            | Attr::Undercurl
            | Attr::DottedUnderline
            | Attr::DashedUnderline => self.terminal.set_underline(true),
            Attr::Reverse => self.terminal.set_inverse(true),
            Attr::CancelBold => self.terminal.set_bold(false),
            Attr::CancelBoldDim => {
                self.terminal.set_bold(false);
                self.terminal.set_dim(false);
            }
            Attr::CancelItalic => self.terminal.set_italic(false),
            Attr::CancelUnderline => self.terminal.set_underline(false),
            Attr::CancelReverse => self.terminal.set_inverse(false),
            Attr::Foreground(color) => self.terminal.set_fg(terminal_color(color)),
            Attr::Background(color) => self.terminal.set_bg(terminal_color(color)),
            _ => {}
        }
    }

    fn set_private_mode(&mut self, mode: PrivateMode) {
        self.terminal.set_private_mode(mode.raw(), true);
    }

    fn unset_private_mode(&mut self, mode: PrivateMode) {
        self.terminal.set_private_mode(mode.raw(), false);
    }

    fn set_scrolling_region(&mut self, top: usize, bottom: Option<usize>) {
        let (_, rows) = (self.terminal.size().1, self.terminal.size().0);
        let bottom = bottom.unwrap_or(rows as usize);
        self.terminal.set_scroll_region(
            to_u16(top.saturating_sub(1)),
            to_u16(bottom.saturating_sub(1)),
        );
    }

    fn set_keypad_application_mode(&mut self) {
        self.terminal.set_keypad_application_mode(true);
    }

    fn unset_keypad_application_mode(&mut self) {
        self.terminal.set_keypad_application_mode(false);
    }

    fn set_active_charset(&mut self, index: CharsetIndex) {
        *self.active_charset = index;
    }

    fn configure_charset(&mut self, index: CharsetIndex, charset: StandardCharset) {
        self.charsets[charset_slot(index)] = charset;
    }

    fn push_title(&mut self) {
        self.title_stack.push(self.current_title.clone());
    }

    fn pop_title(&mut self) {
        let Some(title) = self.title_stack.pop() else {
            return;
        };

        *self.current_title = title.clone();
        self.callbacks.pending_title = Some(title);
    }

    fn text_area_size_chars(&mut self) {
        let (rows, cols) = self.terminal.size();
        self.callbacks
            .responses
            .push(format!("\x1b[8;{rows};{cols}t").into_bytes());
    }
}

fn charset_slot(index: CharsetIndex) -> usize {
    match index {
        CharsetIndex::G0 => 0,
        CharsetIndex::G1 => 1,
        CharsetIndex::G2 => 2,
        CharsetIndex::G3 => 3,
    }
}

fn cursor_row(line: i32) -> u16 {
    if line <= 0 { 0 } else { to_u16(line as usize) }
}

fn to_u16(value: usize) -> u16 {
    value.min(u16::MAX as usize) as u16
}

fn terminal_color(color: Color) -> TerminalColor {
    match color {
        Color::Indexed(index) => TerminalColor::Indexed(index),
        Color::Spec(rgb) => TerminalColor::Rgb(rgb.r, rgb.g, rgb.b),
        Color::Named(named) => named_terminal_color(named),
    }
}

fn named_terminal_color(color: NamedColor) -> TerminalColor {
    named_color_index(color)
        .map(TerminalColor::Indexed)
        .unwrap_or(TerminalColor::Default)
}

fn named_color_index(color: NamedColor) -> Option<u8> {
    match color {
        NamedColor::Black | NamedColor::DimBlack => Some(0),
        NamedColor::Red | NamedColor::DimRed => Some(1),
        NamedColor::Green | NamedColor::DimGreen => Some(2),
        NamedColor::Yellow | NamedColor::DimYellow => Some(3),
        NamedColor::Blue | NamedColor::DimBlue => Some(4),
        NamedColor::Magenta | NamedColor::DimMagenta => Some(5),
        NamedColor::Cyan | NamedColor::DimCyan => Some(6),
        NamedColor::White | NamedColor::DimWhite => Some(7),
        NamedColor::BrightBlack => Some(8),
        NamedColor::BrightRed => Some(9),
        NamedColor::BrightGreen => Some(10),
        NamedColor::BrightYellow => Some(11),
        NamedColor::BrightBlue => Some(12),
        NamedColor::BrightMagenta => Some(13),
        NamedColor::BrightCyan => Some(14),
        NamedColor::BrightWhite => Some(15),
        NamedColor::Foreground
        | NamedColor::Background
        | NamedColor::Cursor
        | NamedColor::BrightForeground
        | NamedColor::DimForeground => None,
    }
}

#[cfg(test)]
mod tests {
    use super::AnsiParser;
    use crate::terminal::TerminalState;

    #[test]
    fn csi_save_and_restore_cursor_position() {
        let mut parser = AnsiParser::new();
        let mut terminal = TerminalState::new(4, 4, 0);

        parser.process(&mut terminal, b"\x1b[3;2H");
        parser.process(&mut terminal, b"\x1b[s");
        parser.process(&mut terminal, b"\x1b[1;1H");
        parser.process(&mut terminal, b"\x1b[u");

        assert_eq!(terminal.cursor_position(), (2, 1));
    }

    #[test]
    fn reports_cursor_position_via_dsr() {
        let mut parser = AnsiParser::new();
        let mut terminal = TerminalState::new(4, 4, 0);

        parser.process(&mut terminal, b"\x1b[2;3H");
        parser.process(&mut terminal, b"\x1b[6n");

        assert_eq!(parser.take_responses(), vec![b"\x1b[2;3R".to_vec()]);
    }

    #[test]
    fn applies_dec_special_graphics_charset() {
        let mut parser = AnsiParser::new();
        let mut terminal = TerminalState::new(1, 4, 0);

        parser.process(&mut terminal, b"\x1b(0q");

        assert_eq!(terminal.cell(0, 0).expect("cell").contents(), "\u{2500}");
    }

    #[test]
    fn reports_text_area_size_in_characters() {
        let mut parser = AnsiParser::new();
        let mut terminal = TerminalState::new(7, 11, 0);

        parser.process(&mut terminal, b"\x1b[18t");

        assert_eq!(parser.take_responses(), vec![b"\x1b[8;7;11t".to_vec()]);
    }
}
