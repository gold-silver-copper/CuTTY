use std::mem;

use vte::{Params, Perform};

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

#[derive(Default)]
pub struct AnsiParser {
    parser: vte::Parser,
    callbacks: ParserCallbacks,
}

impl AnsiParser {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn process(&mut self, terminal: &mut TerminalState, bytes: &[u8]) {
        let mut performer = TerminalPerformer {
            terminal,
            callbacks: &mut self.callbacks,
        };
        self.parser.advance(&mut performer, bytes);
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

struct TerminalPerformer<'a> {
    terminal: &'a mut TerminalState,
    callbacks: &'a mut ParserCallbacks,
}

impl Perform for TerminalPerformer<'_> {
    fn print(&mut self, c: char) {
        self.terminal.print(c);
    }

    fn execute(&mut self, byte: u8) {
        match byte {
            8 => self.terminal.backspace(),
            9 => self.terminal.tab(),
            10..=12 => self.terminal.linefeed(),
            13 => self.terminal.carriage_return(),
            _ => {}
        }
    }

    fn osc_dispatch(&mut self, params: &[&[u8]], _bell_terminated: bool) {
        match params {
            [b"0", title] | [b"2", title] => {
                self.callbacks.pending_title = Some(String::from_utf8_lossy(title).into_owned());
            }
            _ => {}
        }
    }

    fn csi_dispatch(&mut self, params: &Params, intermediates: &[u8], ignore: bool, action: char) {
        if ignore {
            return;
        }

        match intermediates.first().copied() {
            None => self.dispatch_standard_csi(params, action),
            Some(b'?') => self.dispatch_private_csi(params, action),
            _ => {}
        }
    }

    fn esc_dispatch(&mut self, intermediates: &[u8], ignore: bool, byte: u8) {
        if ignore || !intermediates.is_empty() {
            return;
        }

        match byte {
            b'7' => self.terminal.save_cursor(),
            b'8' => self.terminal.restore_cursor(),
            b'D' => self.terminal.linefeed(),
            b'E' => {
                self.terminal.carriage_return();
                self.terminal.linefeed();
            }
            b'M' => self.terminal.reverse_index(),
            b'c' => self.terminal.reset(),
            _ => {}
        }
    }
}

impl TerminalPerformer<'_> {
    fn dispatch_standard_csi(&mut self, params: &Params, action: char) {
        match action {
            '@' => self.terminal.insert_blank_chars(first_param(params, 1)),
            'A' => self.terminal.cursor_up(first_param(params, 1)),
            'B' => self.terminal.cursor_down(first_param(params, 1)),
            'C' => self.terminal.cursor_forward(first_param(params, 1)),
            'D' => self.terminal.cursor_back(first_param(params, 1)),
            'E' => self.terminal.cursor_next_line(first_param(params, 1)),
            'F' => self.terminal.cursor_prev_line(first_param(params, 1)),
            'G' => self
                .terminal
                .set_cursor_col(first_param(params, 1).saturating_sub(1)),
            'H' | 'f' => {
                let (row, col) = two_params(params, 1, 1);
                self.terminal
                    .set_cursor_position(row.saturating_sub(1), col.saturating_sub(1));
            }
            'J' => self
                .terminal
                .erase_in_display(first_param_allow_zero(params, 0)),
            'K' => self
                .terminal
                .erase_in_line(first_param_allow_zero(params, 0)),
            'L' => self.terminal.insert_lines(first_param(params, 1)),
            'M' => self.terminal.delete_lines(first_param(params, 1)),
            'P' => self.terminal.delete_chars(first_param(params, 1)),
            'S' => self.terminal.scroll_up(first_param(params, 1)),
            'T' => self.terminal.scroll_down(first_param(params, 1)),
            'X' => self.terminal.erase_chars(first_param(params, 1)),
            'd' => self
                .terminal
                .set_cursor_row(first_param(params, 1).saturating_sub(1)),
            'm' => self.apply_sgr(params),
            'n' => self.device_status_report(params),
            'r' => {
                let (top, bottom) = two_params(params, 1, self.terminal.size().0.max(1));
                self.terminal
                    .set_scroll_region(top.saturating_sub(1), bottom.saturating_sub(1));
            }
            't' => self.handle_window_op(params),
            _ => {}
        }
    }

    fn dispatch_private_csi(&mut self, params: &Params, action: char) {
        let enabled = match action {
            'h' => true,
            'l' => false,
            _ => return,
        };

        for mode in params.iter().flatten().copied() {
            self.terminal.set_private_mode(mode, enabled);
        }
    }

    fn apply_sgr(&mut self, params: &Params) {
        let mut values = params_to_values(params);
        if values.is_empty() {
            values.push(0);
        }

        let mut index = 0;
        while index < values.len() {
            match values[index] {
                0 => self.terminal.set_attr_reset(),
                1 => self.terminal.set_bold(true),
                2 => self.terminal.set_dim(true),
                3 => self.terminal.set_italic(true),
                4 => self.terminal.set_underline(true),
                7 => self.terminal.set_inverse(true),
                22 => {
                    self.terminal.set_bold(false);
                    self.terminal.set_dim(false);
                }
                23 => self.terminal.set_italic(false),
                24 => self.terminal.set_underline(false),
                27 => self.terminal.set_inverse(false),
                30..=37 => self
                    .terminal
                    .set_fg(TerminalColor::Indexed((values[index] - 30) as u8)),
                39 => self.terminal.set_fg(TerminalColor::Default),
                40..=47 => self
                    .terminal
                    .set_bg(TerminalColor::Indexed((values[index] - 40) as u8)),
                49 => self.terminal.set_bg(TerminalColor::Default),
                90..=97 => self
                    .terminal
                    .set_fg(TerminalColor::Indexed((values[index] - 90 + 8) as u8)),
                100..=107 => self
                    .terminal
                    .set_bg(TerminalColor::Indexed((values[index] - 100 + 8) as u8)),
                38 | 48 => {
                    let is_fg = values[index] == 38;
                    if let Some((color, consumed)) = parse_extended_color(&values[index + 1..]) {
                        if is_fg {
                            self.terminal.set_fg(color);
                        } else {
                            self.terminal.set_bg(color);
                        }
                        index += consumed;
                    }
                }
                _ => {}
            }
            index += 1;
        }
    }

    fn device_status_report(&mut self, params: &Params) {
        match first_param_allow_zero(params, 0) {
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

    fn handle_window_op(&mut self, params: &Params) {
        let values = params_to_values(params);
        if values.first().copied() != Some(8) {
            return;
        }

        let (current_rows, current_cols) = self.terminal.size();
        let rows = values.get(1).copied().unwrap_or(current_rows).max(1);
        let cols = values.get(2).copied().unwrap_or(current_cols).max(1);
        self.callbacks.pending_resize = Some((rows, cols));
    }
}

fn first_param(params: &Params, default: u16) -> u16 {
    params
        .iter()
        .next()
        .and_then(|items| items.first())
        .copied()
        .filter(|value| *value != 0)
        .unwrap_or(default)
}

fn first_param_allow_zero(params: &Params, default: u16) -> u16 {
    params
        .iter()
        .next()
        .and_then(|items| items.first())
        .copied()
        .unwrap_or(default)
}

fn two_params(params: &Params, first_default: u16, second_default: u16) -> (u16, u16) {
    let mut iter = params.iter();
    let first = iter
        .next()
        .and_then(|items| items.first())
        .copied()
        .filter(|value| *value != 0)
        .unwrap_or(first_default);
    let second = iter
        .next()
        .and_then(|items| items.first())
        .copied()
        .filter(|value| *value != 0)
        .unwrap_or(second_default);
    (first, second)
}

fn params_to_values(params: &Params) -> Vec<u16> {
    params
        .iter()
        .flat_map(|items| {
            if items.is_empty() {
                vec![0]
            } else {
                items.to_vec()
            }
        })
        .collect()
}

fn parse_extended_color(values: &[u16]) -> Option<(TerminalColor, usize)> {
    match values {
        [5, index, ..] => Some((TerminalColor::Indexed(*index as u8), 2)),
        [2, r, g, b, ..] => Some((TerminalColor::Rgb(*r as u8, *g as u8, *b as u8), 4)),
        _ => None,
    }
}
