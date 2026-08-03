//! Deterministic, in-memory ports of the upstream vtebench and Kitty parser workloads.

use std::fmt::Write as _;
use std::hint::black_box;
use std::time::{Duration, Instant};

use cutty_terminal::event::VoidListener;
use cutty_terminal::grid::Dimensions;
use cutty_terminal::term::{Config as TermConfig, Term};
use cutty_terminal::vte::ansi::Processor;

use super::{BenchConfig, median_sorted, percentile_sorted};

const COLUMNS: usize = 80;
const LINES: usize = 24;
const VTEBENCH_MIN_BYTES: usize = 1_048_576;
const PTY_CHUNK_BYTES: usize = 4_096;
const VTEBENCH_REVISION: &str = "ead80032e57dee2e75f0b51f2ea67528647d9944";
const KITTY_REVISION: &str = "v0.46.1 (7f3e7d5192c37cd4ceecd062a892ec9b34b3dc1a)";

struct ParserWorkload {
    suite: &'static str,
    name: &'static str,
    description: &'static str,
    setup: Vec<u8>,
    payload: Vec<u8>,
}

impl ParserWorkload {
    fn new(
        suite: &'static str,
        name: &'static str,
        description: &'static str,
        setup: Vec<u8>,
        payload: Vec<u8>,
    ) -> Self {
        Self { suite, name, description, setup, payload }
    }

    fn qualified_name(&self) -> String {
        format!("{}/{}", self.suite, self.name)
    }
}

struct ParserDimensions;

impl Dimensions for ParserDimensions {
    fn total_lines(&self) -> usize {
        LINES
    }

    fn screen_lines(&self) -> usize {
        LINES
    }

    fn columns(&self) -> usize {
        COLUMNS
    }
}

struct ParserPath {
    processor: Processor,
    term: Term<VoidListener>,
}

impl ParserPath {
    fn new() -> Self {
        Self {
            processor: Processor::new(),
            term: Term::new(TermConfig::default(), &ParserDimensions, VoidListener),
        }
    }

    fn prepare(&mut self, setup: &[u8]) {
        // Match both upstream runners: reset terminal state before applying untimed setup.
        self.processor.advance(&mut self.term, b"\x1b]\x1b\\\x1bc");
        self.processor.advance(&mut self.term, setup);
    }

    fn parse(&mut self, payload: &[u8], chunk_bytes: usize) -> usize {
        if payload.len() <= chunk_bytes {
            self.processor.advance(&mut self.term, payload);
        } else {
            for chunk in payload.chunks(chunk_bytes) {
                self.processor.advance(&mut self.term, chunk);
            }
        }

        let cursor = self.term.grid().cursor.point;
        payload.len()
            ^ (cursor.line.0 as usize).rotate_left(7)
            ^ cursor.column.0.rotate_left(13)
            ^ self.term.grid().display_offset().rotate_left(19)
            ^ (self.term.mode().bits() as usize).rotate_left(23)
    }
}

pub(super) fn report(config: &BenchConfig) {
    let workloads = workloads();
    validate_workloads(&workloads);
    let selected = workloads
        .iter()
        .filter(|workload| config.includes("external parser", &workload.qualified_name()))
        .collect::<Vec<_>>();
    if selected.is_empty() {
        return;
    }

    println!();
    println!("## Ported vtebench and Kitty Parser Workloads");
    println!();
    println!(
        "These are direct in-memory CuTTY parser/terminal-state measurements. They exclude PTY, \
         shell, window-server, renderer, GPU, and compositor time. Bulk passes each payload in \
         one call; 4 KiB mode models bounded PTY reads. Setup and terminal reset are not timed."
    );
    println!();
    println!("- vtebench source revision: `{VTEBENCH_REVISION}` (all 12 benchmark directories)");
    println!("- Kitty source revision: `{KITTY_REVISION}` (all 5 benchmark families)");
    println!(
        "- vtebench payloads use its default 1 MiB minimum and exact 80×24 control-flow \
         semantics; large recorded fixtures are deterministic generated equivalents"
    );
    println!("- Kitty random inputs are replaced by a fixed-seed generator for reproducibility");
    println!(
        "- CuTTY does not implement Kitty graphics; the images case measures scanning and \
         ignoring the graphics APC stream, not image decoding or display"
    );
    println!();
    println!("### External Workload Inventory");
    println!();
    println!("| suite | upstream workload | payload bytes | setup bytes | scenario |");
    println!("|---|---|---:|---:|---|");
    for workload in &selected {
        println!(
            "| {} | {} | {} | {} | {} |",
            workload.suite,
            workload.name,
            workload.payload.len(),
            workload.setup.len(),
            workload.description
        );
    }

    println!();
    println!("### External Parser Measurements");
    println!();
    println!(
        "Chunk overhead is paired `(4 KiB time - bulk time) / bulk time`; positive means chunking \
         was slower. The range is p10–p90 across paired samples."
    );
    println!();
    println!(
        "| suite | workload | bulk µs | bulk MiB/s | 4 KiB µs | 4 KiB MiB/s | chunk overhead | \
         p10–p90 | iterations |"
    );
    println!("|---|---|---:|---:|---:|---:|---:|---:|---:|");
    for workload in selected {
        report_parser_pair(workload, config);
    }
}

fn report_parser_pair(workload: &ParserWorkload, config: &BenchConfig) {
    let mut bulk = ParserPath::new();
    let mut chunked = ParserPath::new();

    for _ in 0..config.warmups {
        black_box(timed_parse_once(&mut bulk, workload, usize::MAX));
        black_box(timed_parse_once(&mut chunked, workload, PTY_CHUNK_BYTES));
    }

    let bulk_probe = timed_parse_once(&mut bulk, workload, usize::MAX).0;
    let chunked_probe = timed_parse_once(&mut chunked, workload, PTY_CHUNK_BYTES).0;
    let probe = bulk_probe.min(chunked_probe).max(1.0);
    let iterations = ((config.target.as_nanos() as f64 / probe) as usize).clamp(1, 256);
    let mut bulk_samples = Vec::with_capacity(config.samples);
    let mut chunked_samples = Vec::with_capacity(config.samples);
    let mut work: usize = 0;

    for sample in 0..config.samples {
        if sample % 2 == 0 {
            let result = timed_parse_iterations(&mut bulk, workload, usize::MAX, iterations);
            bulk_samples.push(result.0);
            work ^= result.1;
            let result =
                timed_parse_iterations(&mut chunked, workload, PTY_CHUNK_BYTES, iterations);
            chunked_samples.push(result.0);
            work ^= result.1;
        } else {
            let result =
                timed_parse_iterations(&mut chunked, workload, PTY_CHUNK_BYTES, iterations);
            chunked_samples.push(result.0);
            work ^= result.1;
            let result = timed_parse_iterations(&mut bulk, workload, usize::MAX, iterations);
            bulk_samples.push(result.0);
            work ^= result.1;
        }
    }
    black_box(work);

    let mut overheads = bulk_samples
        .iter()
        .zip(&chunked_samples)
        .map(|(bulk, chunked)| 100.0 * (chunked - bulk) / bulk)
        .collect::<Vec<_>>();
    overheads.sort_unstable_by(f64::total_cmp);
    bulk_samples.sort_unstable_by(f64::total_cmp);
    chunked_samples.sort_unstable_by(f64::total_cmp);

    let bulk_ns = median_sorted(&bulk_samples) / iterations as f64;
    let chunked_ns = median_sorted(&chunked_samples) / iterations as f64;
    let mib = workload.payload.len() as f64 / (1024.0 * 1024.0);
    let bulk_rate = mib / (bulk_ns / 1_000_000_000.0);
    let chunked_rate = mib / (chunked_ns / 1_000_000_000.0);
    println!(
        "| {} | {} | {:.2} | {:.1} | {:.2} | {:.1} | {:+.1}% | {:+.1}%–{:+.1}% | {} |",
        workload.suite,
        workload.name,
        bulk_ns / 1_000.0,
        bulk_rate,
        chunked_ns / 1_000.0,
        chunked_rate,
        median_sorted(&overheads),
        percentile_sorted(&overheads, 0.1),
        percentile_sorted(&overheads, 0.9),
        iterations,
    );
}

fn timed_parse_once(
    path: &mut ParserPath,
    workload: &ParserWorkload,
    chunk_bytes: usize,
) -> (f64, usize) {
    path.prepare(&workload.setup);
    let start = Instant::now();
    let work = black_box(path.parse(&workload.payload, chunk_bytes));
    (start.elapsed().as_nanos() as f64, work)
}

fn timed_parse_iterations(
    path: &mut ParserPath,
    workload: &ParserWorkload,
    chunk_bytes: usize,
    iterations: usize,
) -> (f64, usize) {
    let mut elapsed = Duration::ZERO;
    let mut work: usize = 0;
    for _ in 0..iterations {
        path.prepare(&workload.setup);
        let start = Instant::now();
        work = work.wrapping_add(black_box(path.parse(&workload.payload, chunk_bytes)));
        elapsed += start.elapsed();
    }
    (elapsed.as_nanos() as f64, work)
}

fn workloads() -> Vec<ParserWorkload> {
    let scrolling_setup = repeat_bytes(b"y\n", 100_001 * 2);
    vec![
        ParserWorkload::new(
            "vtebench",
            "cursor_motion",
            "Cursor-addressed writes around nested viewport rectangles",
            Vec::new(),
            vte_cursor_motion(),
        ),
        ParserWorkload::new(
            "vtebench",
            "dense_cells",
            "Full-screen writes with indexed foreground/background and bold/italic/underline",
            b"\x1b[?1049h".to_vec(),
            vte_dense_cells(),
        ),
        ParserWorkload::new(
            "vtebench",
            "light_cells",
            "Repeated full-screen plain ASCII updates without scrolling",
            b"\x1b[?1049h".to_vec(),
            vte_light_cells(),
        ),
        ParserWorkload::new(
            "vtebench",
            "medium_cells",
            "Escape-heavy Vim-style recorded-session equivalent",
            Vec::new(),
            vte_medium_cells(false),
        ),
        ParserWorkload::new(
            "vtebench",
            "scrolling_bottom_region",
            "Line-feed scrolling in rows 1–23",
            b"\x1b[?1049h\x1b[1;23r".to_vec(),
            vte_scrolling(),
        ),
        ParserWorkload::new(
            "vtebench",
            "scrolling_bottom_small_region",
            "Line-feed scrolling in rows 1–12",
            b"\x1b[?1049h\x1b[1;12r".to_vec(),
            vte_scrolling(),
        ),
        ParserWorkload::new(
            "vtebench",
            "scrolling_fullscreen",
            "Full-width A–Z lines scrolling the primary screen and scrollback",
            scrolling_setup.clone(),
            vte_scrolling_fullscreen(),
        ),
        ParserWorkload::new(
            "vtebench",
            "scrolling_top_region",
            "Line-feed scrolling in rows 2–24",
            b"\x1b[?1049h\x1b[2;24r".to_vec(),
            vte_scrolling(),
        ),
        ParserWorkload::new(
            "vtebench",
            "scrolling_top_small_region",
            "Line-feed scrolling in rows 12–24",
            b"\x1b[?1049h\x1b[12;24r".to_vec(),
            vte_scrolling(),
        ),
        ParserWorkload::new(
            "vtebench",
            "scrolling",
            "Short line-feed writes scrolling a full primary screen with full scrollback",
            scrolling_setup,
            vte_scrolling(),
        ),
        ParserWorkload::new(
            "vtebench",
            "sync_medium_cells",
            "Escape-heavy Vim-style updates using synchronized-output boundaries",
            Vec::new(),
            vte_medium_cells(true),
        ),
        ParserWorkload::new(
            "vtebench",
            "unicode",
            "Large mixed-script, combining-mark, symbol, CJK, and emoji stream",
            b"\x1b[?1049h".to_vec(),
            vte_unicode(),
        ),
        ParserWorkload::new(
            "kitty",
            "ascii",
            "Only printable ASCII plus newline and tab controls",
            kitty_setup(),
            kitty_ascii(),
        ),
        ParserWorkload::new(
            "kitty",
            "unicode",
            "Repeated CJK, emoji, symbols, accents, and combining marks",
            kitty_setup(),
            kitty_unicode(),
        ),
        ParserWorkload::new(
            "kitty",
            "csi",
            "Sparse text interleaved with SGR, cursor, erase, and repeat CSI commands",
            kitty_setup(),
            kitty_csi(),
        ),
        ParserWorkload::new(
            "kitty",
            "images",
            "Uncompressed 1024×1024 RGBA Kitty graphics transmit followed by delete",
            kitty_setup(),
            kitty_images(),
        ),
        ParserWorkload::new(
            "kitty",
            "long_escape_codes",
            "1,024 ignored OSC 6 commands with 8,024-byte printable payloads",
            kitty_setup(),
            kitty_long_escape_codes(),
        ),
    ]
}

fn vte_cursor_motion() -> Vec<u8> {
    let mut output = String::new();
    for character in b'A'..=b'Z' {
        let (mut column_start, mut column_end) = (1, COLUMNS);
        let (mut line_start, mut line_end) = (1, LINES);
        loop {
            let (mut column, mut line) = (column_start, line_start);
            while column < column_end {
                write!(output, "\x1b[{line};{column}H{}", character as char).unwrap();
                column += 1;
            }
            while line < line_end {
                write!(output, "\x1b[{line};{column}H{}", character as char).unwrap();
                line += 1;
            }
            while column > column_start {
                write!(output, "\x1b[{line};{column}H{}", character as char).unwrap();
                column -= 1;
            }
            while line > line_start {
                write!(output, "\x1b[{line};{column}H{}", character as char).unwrap();
                line -= 1;
            }

            column_start += 1;
            line_start += 1;
            column_end -= 1;
            line_end -= 1;
            if column_start > column_end || line_start > line_end {
                break;
            }
        }
    }
    repeat_to_min(output.into_bytes(), VTEBENCH_MIN_BYTES)
}

fn vte_dense_cells() -> Vec<u8> {
    let mut output = String::new();
    for (offset, character) in (b'A'..=b'Z').enumerate() {
        output.push_str("\x1b[H");
        for line in 1..=LINES {
            for column in 1..=COLUMNS {
                let index = line + column + offset;
                let foreground = index % 156 + 100;
                let background = 255 - index % 156 + 100;
                write!(
                    output,
                    "\x1b[38;5;{foreground};48;5;{background};1;3;4m{}",
                    character as char
                )
                .unwrap();
            }
        }
    }
    repeat_to_min(output.into_bytes(), VTEBENCH_MIN_BYTES)
}

fn vte_light_cells() -> Vec<u8> {
    let mut output = Vec::with_capacity(26 * (COLUMNS * LINES + 3));
    for character in b'A'..=b'Z' {
        output.extend_from_slice(b"\x1b[H");
        output.extend(std::iter::repeat_n(character, COLUMNS * LINES));
    }
    repeat_to_min(output, VTEBENCH_MIN_BYTES)
}

fn vte_medium_cells(synchronized: bool) -> Vec<u8> {
    let target = if synchronized { 185_737 } else { 178_345 };
    let mut output = Vec::with_capacity(target + 512);
    let mut frame: usize = 0;
    while output.len() < target {
        if synchronized {
            output.extend_from_slice(b"\x1b[?2026h");
        }
        output.extend_from_slice(b"\x1b[?1049h\x1b[H\x1b[2J\x1b[?25l");
        for line in 1..=LINES {
            let foreground = 31 + line % 7;
            let background = 40 + (line + frame) % 7;
            let row = format!(
                "\x1b[{line};1H\x1b[{foreground};{background};1m{:02} fn render_frame_{frame}() \
                 {{ let value = 0x{:08x}; }}\x1b[K",
                line,
                frame.wrapping_mul(0x9e37_79b9_usize)
            );
            output.extend_from_slice(row.as_bytes());
        }
        output.extend_from_slice(
            b"\x1b[24;1H\x1b[7m NORMAL  cutty/src/display/mod.rs  100% \x1b[K\x1b[m",
        );
        output.extend_from_slice(b"\x1b[?25h");
        if synchronized {
            output.extend_from_slice(b"\x1b[?2026l");
        }
        frame += 1;
    }
    repeat_to_min(output, VTEBENCH_MIN_BYTES)
}

fn vte_scrolling() -> Vec<u8> {
    repeat_bytes(b"y\n", VTEBENCH_MIN_BYTES)
}

fn vte_scrolling_fullscreen() -> Vec<u8> {
    let mut output = Vec::with_capacity(26 * (COLUMNS + 1));
    for character in b'A'..=b'Z' {
        output.extend(std::iter::repeat_n(character, COLUMNS));
        output.push(b'\n');
    }
    repeat_to_min(output, VTEBENCH_MIN_BYTES)
}

fn vte_unicode() -> Vec<u8> {
    let corpus = concat!(
        "Latin café naïve — Ελληνικά Ω∞ — Кириллица Ж — العربية مرحبا — ",
        "देवनागरी नमस्ते — 中文漢字 — 日本語かな — 한글 — 😀😛😇🦀👍 — ",
        "a\u{0301} e\u{0327} o\u{0302}\u{0307} →⇒•·°±−×÷¼½¾\n",
    );
    let mut output = Vec::with_capacity(138_512);
    while output.len() < 138_296 {
        output.extend_from_slice(corpus.as_bytes());
    }
    repeat_to_min(output, VTEBENCH_MIN_BYTES)
}

fn kitty_setup() -> Vec<u8> {
    b"\x1b[?1049h\x1b[?25l\x1b[m\x1b[H\x1b[2J\x1b[?2026h".to_vec()
}

fn kitty_ascii() -> Vec<u8> {
    const ALPHABET: &[u8] =
        b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ  `~!@#$%^&*()_+-=[]{}\\|;:'\",<.>/?\n\t";
    kitty_finish(random_bytes(1024 * 2048 + 13, ALPHABET, 0x4b49_5454_5900_0001))
}

fn kitty_unicode() -> Vec<u8> {
    // Match the byte length of Kitty 0.46.1's two static corpora repeated 1,024 times.
    const UPSTREAM_BYTES: usize = 1_895_424;
    let corpus = concat!(
        "旦海司有幼雞讀松鼻種比門真目怪少，位香法士錯乙音造活羽詞。\n",
        "飯躲裝個哥害共買去隻把氣年，快石牙飽知唱想土人。\n",
        "‘’“”‹›«» 😀😛😇😈😉😍😎👍👎 —–§¶†‡©®™ →⇒•·°±−×÷¼½¾\n",
        "ÀÁÂÃÄÅÆÇÈÉÊË αΩ∞ ū\u{0300} n\u{0302} H\u{0328} a\u{0320} ",
        "X\u{0321}\u{0310}\u{0353}\n\t",
    );
    kitty_finish(repeat_utf8_to_exact(corpus, UPSTREAM_BYTES))
}

fn kitty_csi() -> Vec<u8> {
    const TARGET: usize = 1024 * 1024 + 17;
    const TEXT: &[u8] =
        b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ  `~!@#$%^&*()_+-=[]{}\\|;:'\",<.>/?\n\t";
    let mut rng = Lcg::new(0x4b49_5454_5900_0002);
    let mut output = Vec::with_capacity(TARGET + 96);
    while output.len() < TARGET {
        match rng.next_usize(100) {
            0..=9 => {
                let length = rng.next_usize(72) + 1;
                for _ in 0..length {
                    output.push(TEXT[rng.next_usize(TEXT.len())]);
                }
            },
            30..=39 => output.extend_from_slice(b"\x1b[1;2;3;4:3;31m"),
            40..=49 => output.extend_from_slice(b"\x1b[38:5:24;48:2:125:136:147m"),
            50..=59 => output.extend_from_slice(b"\x1b[58;5;44;2m"),
            60..=79 => output.extend_from_slice(b"\x1b[m\x1b[10A\x1b[3E\x1b[2K"),
            _ => output.extend_from_slice(b"\x1b[39m\x1b[10`a\x1b[100b\x1b[?1l"),
        }
    }
    output.extend_from_slice(b"\x1b[m");
    kitty_finish(output)
}

fn kitty_images() -> Vec<u8> {
    const RGBA_BYTES: usize = 4 * 1024 * 1024;
    const ENCODED_BYTES: usize = RGBA_BYTES.div_ceil(3) * 4;
    const GRAPHICS_CHUNK: usize = 4_096;
    let mut encoded = vec![b'A'; ENCODED_BYTES];
    let remainder = RGBA_BYTES % 3;
    if remainder != 0 {
        let padding = 3 - remainder;
        for byte in encoded.iter_mut().rev().take(padding) {
            *byte = b'=';
        }
    }

    let mut output = Vec::with_capacity(ENCODED_BYTES + ENCODED_BYTES / GRAPHICS_CHUNK * 16);
    for (index, chunk) in encoded.chunks(GRAPHICS_CHUNK).enumerate() {
        if index == 0 {
            output.extend_from_slice(b"\x1b_Ga=t,f=32,s=1024,v=1024,i=12345,q=2,m=1;");
        } else if index + 1 == encoded.len().div_ceil(GRAPHICS_CHUNK) {
            output.extend_from_slice(b"\x1b_Gm=0;");
        } else {
            output.extend_from_slice(b"\x1b_Gm=1;");
        }
        output.extend_from_slice(chunk);
        output.extend_from_slice(b"\x1b\\");
    }
    output.extend_from_slice(b"\x1b_Ga=d,d=i,i=12345,q=2\x1b\\");
    kitty_finish(output)
}

fn kitty_long_escape_codes() -> Vec<u8> {
    const PRINTABLE: &[u8] =
        b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ  `~!@#$%^&*()_+-=[]{}\\|;:'\",<.>/?";
    let body = random_bytes(8_024, PRINTABLE, 0x4b49_5454_5900_0003);
    let mut output = Vec::with_capacity((body.len() + 5) * 1024);
    for _ in 0..1024 {
        output.extend_from_slice(b"\x1b]6;");
        output.extend_from_slice(&body);
        output.push(0x07);
    }
    kitty_finish(output)
}

fn kitty_finish(mut payload: Vec<u8>) -> Vec<u8> {
    // Kitty ends each timed repetition by resuming synchronized output before waiting on DSR.
    payload.extend_from_slice(b"\x1b[?2026l");
    payload
}

fn repeat_to_min(seed: Vec<u8>, minimum: usize) -> Vec<u8> {
    assert!(!seed.is_empty());
    let repetitions = minimum.div_ceil(seed.len());
    let mut output = Vec::with_capacity(seed.len() * repetitions);
    for _ in 0..repetitions {
        output.extend_from_slice(&seed);
    }
    output
}

fn repeat_bytes(seed: &[u8], minimum: usize) -> Vec<u8> {
    repeat_to_min(seed.to_vec(), minimum)
}

fn repeat_utf8_to_exact(seed: &str, length: usize) -> Vec<u8> {
    let mut output = Vec::with_capacity(length);
    while output.len() + seed.len() <= length {
        output.extend_from_slice(seed.as_bytes());
    }
    output.resize(length, b' ');
    output
}

fn random_bytes(length: usize, alphabet: &[u8], seed: u64) -> Vec<u8> {
    let mut rng = Lcg::new(seed);
    (0..length).map(|_| alphabet[rng.next_usize(alphabet.len())]).collect()
}

struct Lcg(u64);

impl Lcg {
    fn new(seed: u64) -> Self {
        Self(seed)
    }

    fn next_usize(&mut self, upper_bound: usize) -> usize {
        self.0 = self.0.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
        ((self.0 >> 32) as usize) % upper_bound
    }
}

fn validate_workloads(workloads: &[ParserWorkload]) {
    assert_eq!(workloads.iter().filter(|workload| workload.suite == "vtebench").count(), 12);
    assert_eq!(workloads.iter().filter(|workload| workload.suite == "kitty").count(), 5);
    let mut names = std::collections::HashSet::new();
    for workload in workloads {
        assert!(names.insert(workload.qualified_name()));
        assert!(!workload.payload.is_empty());
        if workload.suite == "vtebench" {
            assert!(workload.payload.len() >= VTEBENCH_MIN_BYTES);
        }
    }
}

#[test]
fn all_external_parser_workloads_are_valid() {
    let workloads = workloads();
    validate_workloads(&workloads);

    // Exercise every complete stream and its terminal-state transitions without timing them.
    let mut parser = ParserPath::new();
    for workload in &workloads {
        parser.prepare(&workload.setup);
        black_box(parser.parse(&workload.payload, PTY_CHUNK_BYTES));
    }
}
