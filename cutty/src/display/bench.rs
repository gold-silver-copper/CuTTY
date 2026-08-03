//! Headless microbenchmarks for terminal parsing and scene construction.
//!
//! Run through `scripts/headless-render-bench.sh`. This deliberately lives in the display module
//! so it exercises the real private Parley/Vello paths without creating a window or GPU device.

use std::collections::{BTreeMap, HashSet};
use std::env;
use std::hint::black_box;
use std::time::{Duration, Instant};

use cutty_terminal::index::{Column, Point};
use cutty_terminal::term::cell::Flags;
use unicode_width::UnicodeWidthChar;
use vello::Scene;

use super::color::Rgb;
use super::content::{RenderableCell, RenderableCellExtra};
use super::text::TextSystem;
use super::{Display, GlyphBatch, SizeInfo};
use crate::config::font::Font;

mod external;

const STANDARD_COLUMNS: usize = 80;
const STANDARD_LINES: usize = 24;

struct Workload {
    name: &'static str,
    description: &'static str,
    columns: usize,
    lines: usize,
    cells: Vec<RenderableCell>,
}

impl Workload {
    fn new(
        name: &'static str,
        description: &'static str,
        columns: usize,
        lines: usize,
        cells: Vec<RenderableCell>,
    ) -> Self {
        Self { name, description, columns, lines, cells }
    }

    fn size_info(&self) -> SizeInfo {
        SizeInfo::new(
            self.columns as f32 * 10.0,
            self.lines as f32 * 20.0,
            10.0,
            20.0,
            0.0,
            0.0,
            false,
        )
    }

    fn text_cells(&self) -> usize {
        self.cells.iter().filter(|cell| super::cell_has_visible_text(cell)).count()
    }

    fn background_cells(&self) -> usize {
        self.cells.iter().filter(|cell| cell.bg_alpha > 0.0).count()
    }
}

struct ScenePath {
    scene: Scene,
    text: TextSystem,
    glyph_batch: GlyphBatch,
}

impl ScenePath {
    fn new() -> Self {
        Self {
            scene: Scene::new(),
            text: TextSystem::new(Font::default()),
            glyph_batch: GlyphBatch::default(),
        }
    }

    fn glyphs_unbatched(&mut self, cells: &[RenderableCell], size: SizeInfo) -> usize {
        self.scene.reset();
        for cell in cells {
            Display::paint_cell_text(&mut self.scene, &mut self.text, size, cell);
        }
        self.scene.encoding().resources.glyph_runs.len()
    }

    fn glyphs_batched(&mut self, cells: &[RenderableCell], size: SizeInfo) -> usize {
        self.scene.reset();
        Display::paint_cell_texts(
            &mut self.scene,
            &mut self.glyph_batch,
            &mut self.text,
            size,
            cells,
        );
        self.scene.encoding().resources.glyph_runs.len()
    }

    fn backgrounds_unbatched(&mut self, cells: &[RenderableCell], size: SizeInfo) -> usize {
        self.scene.reset();
        for cell in cells {
            Display::paint_cell_background(&mut self.scene, cell, size);
        }
        self.scene.encoding().n_paths as usize
    }

    fn backgrounds_coalesced(&mut self, cells: &[RenderableCell], size: SizeInfo) -> usize {
        self.scene.reset();
        Display::paint_cell_backgrounds(&mut self.scene, cells, size)
    }

    fn optimized_into(&mut self, cells: &[RenderableCell], size: SizeInfo) -> usize {
        self.scene.reset();
        Display::paint_cell_backgrounds(&mut self.scene, cells, size);
        Display::paint_cell_texts(
            &mut self.scene,
            &mut self.glyph_batch,
            &mut self.text,
            size,
            cells,
        );
        scene_work(&self.scene)
    }

    fn optimized_fresh_scene(&mut self, cells: &[RenderableCell], size: SizeInfo) -> usize {
        let mut scene = Scene::new();
        Display::paint_cell_backgrounds(&mut scene, cells, size);
        Display::paint_cell_texts(&mut scene, &mut self.glyph_batch, &mut self.text, size, cells);
        scene_work(&scene)
    }
}

struct LegacyFramePath {
    text: TextSystem,
}

impl LegacyFramePath {
    fn new() -> Self {
        Self { text: TextSystem::new(Font::default()) }
    }

    fn render(&mut self, source: &[RenderableCell], size: SizeInfo) -> usize {
        let grid_cells = source.to_vec();
        let mut prepared_cells = Vec::with_capacity(grid_cells.len());
        prepared_cells.extend(grid_cells);

        let mut scene = Scene::new();
        for cell in &prepared_cells {
            Display::paint_cell_background(&mut scene, cell, size);
            if let Some(layout) = self.text.shape_cell_legacy(cell) {
                Display::paint_layout(
                    &mut scene,
                    &layout,
                    self.text.metrics(),
                    size,
                    cell.point.line,
                    cell.point.column.0,
                    cell.fg,
                );
            }
        }
        scene_work(&scene) + prepared_cells.len()
    }

    fn render_cold(&mut self, source: &[RenderableCell], size: SizeInfo) -> usize {
        self.text.reset_caches_for_benchmark();
        self.render(source, size)
    }
}

struct OptimizedFramePath {
    scene: Scene,
    text: TextSystem,
    glyph_batch: GlyphBatch,
    cells: Vec<RenderableCell>,
}

impl OptimizedFramePath {
    fn new() -> Self {
        Self {
            scene: Scene::new(),
            text: TextSystem::new(Font::default()),
            glyph_batch: GlyphBatch::default(),
            cells: Vec::new(),
        }
    }

    fn render(&mut self, source: &[RenderableCell], size: SizeInfo) -> usize {
        self.cells.clear();
        self.cells.extend_from_slice(source);
        self.scene.reset();
        Display::paint_cell_backgrounds(&mut self.scene, &self.cells, size);
        Display::paint_cell_texts(
            &mut self.scene,
            &mut self.glyph_batch,
            &mut self.text,
            size,
            &self.cells,
        );
        scene_work(&self.scene) + self.cells.len()
    }

    fn render_cold(&mut self, source: &[RenderableCell], size: SizeInfo) -> usize {
        self.text.reset_caches_for_benchmark();
        self.render(source, size)
    }
}

#[test]
#[ignore = "manual headless performance harness"]
fn render_perf() {
    let workloads = workloads();
    let config = BenchConfig::from_env();
    validate_workloads(&workloads);

    println!("# CuTTY Headless Performance Benchmark");
    println!();
    println!(
        "Paired release-mode measurements with {} samples, {} warmup pairs, and approximately {} \
         ms per sample per side.",
        config.samples,
        config.warmups,
        config.target.as_millis()
    );
    println!("Positive impact means the optimized path is faster. The range is p10–p90.");
    println!();
    println!("## Workloads");
    println!();
    println!("| workload | grid | render cells | text cells | background cells | scenario |");
    println!("|---|---:|---:|---:|---:|---|");
    for workload in &workloads {
        println!(
            "| {} | {}×{} | {} | {} | {} | {} |",
            workload.name,
            workload.columns,
            workload.lines,
            workload.cells.len(),
            workload.text_cells(),
            workload.background_cells(),
            workload.description
        );
    }
    println!();
    println!("## Measurements");
    println!();
    println!("Work counts are operation-specific and should only be compared within one row.");
    println!();
    println!(
        "| improvement | workload | baseline µs | optimized µs | saved µs | paired impact | \
         p10–p90 | iterations | baseline work | optimized work |"
    );
    println!("|---|---|---:|---:|---:|---:|---:|---:|---:|---:|");

    let mut summary = BTreeMap::<&'static str, Vec<f64>>::new();

    for workload in &workloads {
        let size = workload.size_info();
        let mut baseline = ScenePath::new();
        let mut optimized = ScenePath::new();
        record_applicable_impact(
            &mut summary,
            "glyph batching",
            workload.text_cells() > 0,
            report_pair(
                "glyph batching",
                workload.name,
                &config,
                || baseline.glyphs_unbatched(&workload.cells, size),
                || optimized.glyphs_batched(&workload.cells, size),
            ),
        );
    }

    for workload in &workloads {
        let size = workload.size_info();
        let mut baseline = ScenePath::new();
        let mut optimized = ScenePath::new();
        record_applicable_impact(
            &mut summary,
            "background coalescing",
            workload.background_cells() > 0,
            report_pair(
                "background coalescing",
                workload.name,
                &config,
                || baseline.backgrounds_unbatched(&workload.cells, size),
                || optimized.backgrounds_coalesced(&workload.cells, size),
            ),
        );
    }

    for workload in &workloads {
        let mut baseline = TextSystem::new(Font::default());
        let mut optimized = TextSystem::new(Font::default());
        record_impact(
            &mut summary,
            "whitespace skip",
            report_pair(
                "whitespace skip",
                workload.name,
                &config,
                || shape_cells(&mut baseline, &workload.cells, ShapeMode::NoWhitespaceSkip),
                || shape_cells(&mut optimized, &workload.cells, ShapeMode::Optimized),
            ),
        );
    }

    for workload in &workloads {
        let size = workload.size_info();
        let mut baseline = ScenePath::new();
        let mut optimized = ScenePath::new();
        record_impact(
            &mut summary,
            "retained scene",
            report_pair(
                "retained scene",
                workload.name,
                &config,
                || baseline.optimized_fresh_scene(&workload.cells, size),
                || optimized.optimized_into(&workload.cells, size),
            ),
        );
    }

    for workload in &workloads {
        let mut baseline = TextSystem::new(Font::default());
        let mut optimized = TextSystem::new(Font::default());
        record_applicable_impact(
            &mut summary,
            "cache lookup first",
            workload.text_cells() > 0,
            report_pair(
                "cache lookup first",
                workload.name,
                &config,
                || shape_cells(&mut baseline, &workload.cells, ShapeMode::FallbackFirst),
                || shape_cells(&mut optimized, &workload.cells, ShapeMode::Optimized),
            ),
        );
    }

    for workload in &workloads {
        let mut reused = Vec::new();
        record_impact(
            &mut summary,
            "scratch vector reuse",
            report_pair(
                "scratch vector reuse",
                workload.name,
                &config,
                || prepare_cells_fresh(&workload.cells),
                || prepare_cells_reused(&workload.cells, &mut reused),
            ),
        );
    }

    for workload in &workloads {
        let size = workload.size_info();
        let mut baseline = LegacyFramePath::new();
        let mut optimized = OptimizedFramePath::new();
        record_impact(
            &mut summary,
            "all six combined",
            report_pair(
                "all six combined",
                workload.name,
                &config,
                || baseline.render(&workload.cells, size),
                || optimized.render(&workload.cells, size),
            ),
        );
    }

    for workload in &workloads {
        let size = workload.size_info();
        let mut baseline = LegacyFramePath::new();
        let mut optimized = OptimizedFramePath::new();
        record_impact(
            &mut summary,
            "cold-cache full frame",
            report_pair(
                "cold-cache full frame",
                workload.name,
                &config,
                || baseline.render_cold(&workload.cells, size),
                || optimized.render_cold(&workload.cells, size),
            ),
        );
    }

    println!();
    println!("## Unweighted Workload Summary");
    println!();
    println!(
        "Zero-work controls are excluded from summaries for glyph batching, background \
         coalescing, and cache lookup."
    );
    println!();
    println!("| improvement | cases | faster | neutral | slower | median impact | worst | best |");
    println!("|---|---:|---:|---:|---:|---:|---:|---:|");
    for (improvement, mut impacts) in summary {
        impacts.sort_unstable_by(f64::total_cmp);
        let faster = impacts.iter().filter(|impact| **impact > 1.0).count();
        let slower = impacts.iter().filter(|impact| **impact < -1.0).count();
        let neutral = impacts.len() - faster - slower;
        println!(
            "| {improvement} | {} | {faster} | {neutral} | {slower} | {:+.1}% | {:+.1}% | {:+.1}% \
             |",
            impacts.len(),
            median_sorted(&impacts),
            impacts[0],
            impacts[impacts.len() - 1]
        );
    }
    external::report(&config);
    println!("CUTTY_BENCHMARK_END");
}

/// Absolute measurements of CuTTY's current native frame path.
///
/// Unlike [`render_perf`], this has no synthetic baseline. It exists so the exact same workload
/// suite can be compiled at two CuTTY revisions and compared across their native implementations.
#[test]
#[ignore = "manual cross-build headless performance harness"]
fn native_perf() {
    let workloads = workloads();
    let config = BenchConfig::from_env();
    validate_workloads(&workloads);

    println!("# CuTTY Native Headless Performance Benchmark");
    println!();
    println!("Build: `{}`", env!("VERSION"));
    println!(
        "Release-mode measurements with {} samples, {} warmups, and approximately {} ms per \
         sample.",
        config.samples,
        config.warmups,
        config.target.as_millis()
    );
    println!();
    println!("## Native Scene Measurements");
    println!();
    println!("| workload | grid | cells | median µs | p10–p90 µs | iterations | work | scenario |");
    println!("|---|---:|---:|---:|---:|---:|---:|---|");

    for workload in &workloads {
        if !config.includes("native frame", workload.name) {
            continue;
        }
        let size = workload.size_info();
        let mut path = OptimizedFramePath::new();
        report_native_frame(workload, &config, || path.render(&workload.cells, size));
    }

    external::report(&config);
    println!("CUTTY_NATIVE_BENCHMARK_END");
}

fn report_native_frame(
    workload: &Workload,
    config: &BenchConfig,
    mut render: impl FnMut() -> usize,
) {
    for _ in 0..config.warmups {
        black_box(render());
    }

    let probe = timed_once(&mut render).max(Duration::from_nanos(1));
    let iterations = (config.target.as_nanos() / probe.as_nanos()).clamp(1, 50_000) as usize;
    let mut samples = (0..config.samples)
        .map(|_| timed_iterations(&mut render, iterations) / iterations as f64)
        .collect::<Vec<_>>();
    samples.sort_unstable_by(f64::total_cmp);
    let work = black_box(render());
    println!(
        "| {} | {}×{} | {} | {:.2} | {:.2}–{:.2} | {} | {} | {} |",
        workload.name,
        workload.columns,
        workload.lines,
        workload.cells.len(),
        median_sorted(&samples) / 1_000.0,
        percentile_sorted(&samples, 0.1) / 1_000.0,
        percentile_sorted(&samples, 0.9) / 1_000.0,
        iterations,
        work,
        workload.description,
    );
}

#[derive(Clone, Copy)]
enum ShapeMode {
    Optimized,
    NoWhitespaceSkip,
    FallbackFirst,
}

fn shape_cells(text: &mut TextSystem, cells: &[RenderableCell], mode: ShapeMode) -> usize {
    cells
        .iter()
        .filter_map(|cell| match mode {
            ShapeMode::Optimized => text.shape_cell(cell),
            ShapeMode::NoWhitespaceSkip => text.shape_cell_without_whitespace_skip(cell),
            ShapeMode::FallbackFirst => text.shape_cell_without_cache_first(cell),
        })
        .map(|layout| layout.len())
        .sum()
}

fn prepare_cells_fresh(source: &[RenderableCell]) -> usize {
    let grid_cells = source.to_vec();
    let mut prepared_cells = Vec::with_capacity(grid_cells.len());
    prepared_cells.extend(grid_cells);
    black_box(prepared_cells).len()
}

fn prepare_cells_reused(source: &[RenderableCell], cells: &mut Vec<RenderableCell>) -> usize {
    cells.clear();
    cells.extend_from_slice(source);
    black_box(&*cells).len()
}

fn scene_work(scene: &Scene) -> usize {
    let encoding = scene.encoding();
    encoding.path_tags.len()
        + encoding.draw_tags.len()
        + encoding.resources.glyphs.len()
        + encoding.resources.glyph_runs.len()
}

struct BenchConfig {
    samples: usize,
    warmups: usize,
    target: Duration,
    filter: Option<String>,
}

impl BenchConfig {
    fn from_env() -> Self {
        let samples =
            env::var("CUTTY_PERF_SAMPLES").ok().and_then(|value| value.parse().ok()).unwrap_or(11);
        let warmups =
            env::var("CUTTY_PERF_WARMUPS").ok().and_then(|value| value.parse().ok()).unwrap_or(5);
        let target_ms = env::var("CUTTY_PERF_TARGET_MS")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(20);
        let filter = env::var("CUTTY_PERF_FILTER").ok();
        Self {
            samples: samples.max(3),
            warmups,
            target: Duration::from_millis(target_ms.max(1)),
            filter,
        }
    }

    fn includes(&self, improvement: &str, workload: &str) -> bool {
        self.filter.as_ref().is_none_or(|filter| {
            let filter = filter.to_ascii_lowercase();
            improvement.to_ascii_lowercase().contains(&filter)
                || workload.to_ascii_lowercase().contains(&filter)
                || format!("{improvement}/{workload}").to_ascii_lowercase().contains(&filter)
        })
    }
}

fn report_pair<B, O>(
    improvement: &str,
    workload: &str,
    config: &BenchConfig,
    mut baseline: B,
    mut optimized: O,
) -> Option<f64>
where
    B: FnMut() -> usize,
    O: FnMut() -> usize,
{
    if !config.includes(improvement, workload) {
        return None;
    }

    for _ in 0..config.warmups {
        black_box(baseline());
        black_box(optimized());
    }

    let baseline_probe = timed_once(&mut baseline);
    let optimized_probe = timed_once(&mut optimized);
    let probe = baseline_probe.min(optimized_probe).max(Duration::from_nanos(1));
    let iterations = (config.target.as_nanos() / probe.as_nanos()).clamp(1, 50_000) as usize;
    let mut baseline_samples = Vec::with_capacity(config.samples);
    let mut optimized_samples = Vec::with_capacity(config.samples);

    for sample in 0..config.samples {
        if sample % 2 == 0 {
            baseline_samples.push(timed_iterations(&mut baseline, iterations));
            optimized_samples.push(timed_iterations(&mut optimized, iterations));
        } else {
            optimized_samples.push(timed_iterations(&mut optimized, iterations));
            baseline_samples.push(timed_iterations(&mut baseline, iterations));
        }
    }

    let mut impacts = baseline_samples
        .iter()
        .zip(&optimized_samples)
        .map(|(baseline, optimized)| 100.0 * (baseline - optimized) / baseline)
        .collect::<Vec<_>>();
    impacts.sort_unstable_by(f64::total_cmp);
    baseline_samples.sort_unstable_by(f64::total_cmp);
    optimized_samples.sort_unstable_by(f64::total_cmp);
    let impact = median_sorted(&impacts);
    let impact_low = percentile_sorted(&impacts, 0.1);
    let impact_high = percentile_sorted(&impacts, 0.9);
    let baseline_ns = median_sorted(&baseline_samples) / iterations as f64;
    let optimized_ns = median_sorted(&optimized_samples) / iterations as f64;
    let baseline_work = black_box(baseline());
    let optimized_work = black_box(optimized());
    println!(
        "| {improvement} | {workload} | {:.2} | {:.2} | {:+.2} | {impact:+.1}% | \
         {impact_low:+.1}%–{impact_high:+.1}% | {iterations} | {baseline_work} | {optimized_work} \
         |",
        baseline_ns / 1_000.0,
        optimized_ns / 1_000.0,
        (baseline_ns - optimized_ns) / 1_000.0,
    );
    Some(impact)
}

fn record_impact(
    summary: &mut BTreeMap<&'static str, Vec<f64>>,
    improvement: &'static str,
    impact: Option<f64>,
) {
    if let Some(impact) = impact {
        summary.entry(improvement).or_default().push(impact);
    }
}

fn record_applicable_impact(
    summary: &mut BTreeMap<&'static str, Vec<f64>>,
    improvement: &'static str,
    applicable: bool,
    impact: Option<f64>,
) {
    if applicable {
        record_impact(summary, improvement, impact);
    }
}

fn timed_once(function: &mut impl FnMut() -> usize) -> Duration {
    let start = Instant::now();
    black_box(function());
    start.elapsed()
}

fn timed_iterations(function: &mut impl FnMut() -> usize, iterations: usize) -> f64 {
    let start = Instant::now();
    for _ in 0..iterations {
        black_box(function());
    }
    start.elapsed().as_nanos() as f64
}

fn median_sorted(samples: &[f64]) -> f64 {
    samples[samples.len() / 2]
}

fn percentile_sorted(samples: &[f64], percentile: f64) -> f64 {
    let index = ((samples.len() - 1) as f64 * percentile).round() as usize;
    samples[index]
}

fn workloads() -> Vec<Workload> {
    vec![
        Workload::new(
            "dense_ascii",
            "Fully occupied ASCII viewport with one text style",
            STANDARD_COLUMNS,
            STANDARD_LINES,
            dense_ascii(STANDARD_COLUMNS, STANDARD_LINES),
        ),
        Workload::new(
            "large_ascii",
            "Large fully occupied ASCII viewport",
            240,
            67,
            dense_ascii(240, 67),
        ),
        Workload::new(
            "sparse_shell",
            "Typical prompt and command output with empty screen space",
            120,
            35,
            sparse_shell(120, 35),
        ),
        Workload::new(
            "source_code",
            "Source code with indentation and syntax-like foreground colors",
            120,
            40,
            source_code(120, 40),
        ),
        Workload::new(
            "log_stream",
            "Large timestamped log viewport with colored severity fields",
            160,
            50,
            log_stream(160, 50),
        ),
        Workload::new(
            "unicode",
            "Mixed scripts, symbols, emoji, and wide characters",
            STANDARD_COLUMNS,
            STANDARD_LINES,
            unicode(STANDARD_COLUMNS, STANDARD_LINES),
        ),
        Workload::new(
            "wide_cjk",
            "Viewport dominated by double-width CJK and emoji glyphs",
            STANDARD_COLUMNS,
            STANDARD_LINES,
            wide_cjk(STANDARD_COLUMNS, STANDARD_LINES),
        ),
        Workload::new(
            "combining_marks",
            "ASCII bases with one or more zero-width combining marks",
            STANDARD_COLUMNS,
            STANDARD_LINES,
            combining_marks(STANDARD_COLUMNS, STANDARD_LINES),
        ),
        Workload::new(
            "colored_tui",
            "TUI-style text over contiguous colored background bands",
            STANDARD_COLUMNS,
            STANDARD_LINES,
            colored_tui(STANDARD_COLUMNS, STANDARD_LINES),
        ),
        Workload::new(
            "blank_selection",
            "Whitespace-only selection with one solid background",
            STANDARD_COLUMNS,
            STANDARD_LINES,
            blank_selection(STANDARD_COLUMNS, STANDARD_LINES),
        ),
        Workload::new(
            "fragmented_backgrounds",
            "Worst-case alternating background colors with no mergeable runs",
            STANDARD_COLUMNS,
            STANDARD_LINES,
            fragmented_backgrounds(STANDARD_COLUMNS, STANDARD_LINES),
        ),
        Workload::new(
            "decorated_grid",
            "Dense mixed text decorations and translucent row backgrounds",
            STANDARD_COLUMNS,
            STANDARD_LINES,
            decorated_grid(STANDARD_COLUMNS, STANDARD_LINES),
        ),
        Workload::new(
            "alternating_styles",
            "Worst-case foreground and font-style changes on every cell",
            STANDARD_COLUMNS,
            STANDARD_LINES,
            alternating_styles(STANDARD_COLUMNS, STANDARD_LINES),
        ),
    ]
}

fn validate_workloads(workloads: &[Workload]) {
    let mut names = HashSet::new();
    for workload in workloads {
        assert!(names.insert(workload.name), "duplicate workload name: {}", workload.name);
        assert!(workload.columns > 0 && workload.lines > 0, "empty grid: {}", workload.name);
        assert!(!workload.cells.is_empty(), "empty workload: {}", workload.name);

        let mut points = HashSet::with_capacity(workload.cells.len());
        let mut previous = None;
        for cell in &workload.cells {
            let point = (cell.point.line, cell.point.column.0);
            assert!(point.0 < workload.lines, "line outside {}: {:?}", workload.name, point);
            assert!(point.1 < workload.columns, "column outside {}: {:?}", workload.name, point);
            assert!(points.insert(point), "duplicate point in {}: {:?}", workload.name, point);
            assert!(
                previous.is_none_or(|previous| previous < point),
                "unsorted workload: {}",
                workload.name
            );
            assert!(cell.bg_alpha.is_finite() && (0.0..=1.0).contains(&cell.bg_alpha));
            if cell.flags.contains(Flags::WIDE_CHAR) {
                assert!(
                    point.1 + 1 < workload.columns,
                    "wide cell outside {}: {:?}",
                    workload.name,
                    point
                );
            }
            previous = Some(point);
        }

        let size = workload.size_info();
        let mut unbatched = ScenePath::new();
        let mut batched = ScenePath::new();
        let unbatched_runs = unbatched.glyphs_unbatched(&workload.cells, size);
        let unbatched_glyphs = unbatched.scene.encoding().resources.glyphs.len();
        let batched_runs = batched.glyphs_batched(&workload.cells, size);
        let batched_glyphs = batched.scene.encoding().resources.glyphs.len();
        assert_eq!(unbatched_glyphs, batched_glyphs, "glyph count changed in {}", workload.name);
        assert!(batched_runs <= unbatched_runs, "glyph runs increased in {}", workload.name);

        let unbatched_backgrounds = unbatched.backgrounds_unbatched(&workload.cells, size);
        let coalesced_backgrounds = batched.backgrounds_coalesced(&workload.cells, size);
        assert!(
            coalesced_backgrounds <= unbatched_backgrounds,
            "background fills increased in {}",
            workload.name
        );
    }
}

fn dense_ascii(columns: usize, lines: usize) -> Vec<RenderableCell> {
    grid(columns, lines, |line, column| {
        let characters = b"abcdefghijklmnopqrstuvwxyz0123456789[]{}()<>/\\";
        let character = characters[(line * columns + column) % characters.len()] as char;
        (character, Rgb::default(), 0.0, Flags::empty())
    })
}

fn sparse_shell(columns: usize, lines: usize) -> Vec<RenderableCell> {
    let rows = [
        "~/src/cutty main ❯ cargo test --workspace",
        "   Compiling cutty_terminal v0.25.1",
        "   Compiling cutty_config_derive v0.2.5",
        "   Compiling cutty v0.18.7",
        "    Finished test profile [unoptimized + debuginfo] target(s) in 3.42s",
        "     Running unittests src/main.rs",
        "",
        "running 108 tests",
        "test display::tests::adjacent_matching_backgrounds_are_coalesced ... ok",
        "test display::text::tests::tabs_are_not_shaped_as_visible_glyphs ... ok",
        "test display::text::tests::single_character_layouts_are_reused ... ok",
        "test result: ok. 108 passed; 0 failed; 0 ignored; finished in 1.27s",
        "",
        "~/src/cutty main ❯ git status --short",
        " M cutty/src/display/mod.rs",
        " M cutty/src/display/text.rs",
        "?? cutty/src/display/bench.rs",
        "",
        "warning: benchmark output can vary with system load",
        "~/src/cutty main ❯",
    ];
    let mut cells = Vec::new();
    for (line, row) in rows.iter().copied().enumerate().take(lines) {
        push_text_line(&mut cells, line, columns, row, |column, _| {
            let (fg, flags) = if row.contains("❯") {
                (Rgb::new(90, 220, 140), Flags::BOLD)
            } else if row.starts_with("warning") {
                (Rgb::new(245, 190, 70), Flags::empty())
            } else if row.contains("... ok") {
                (Rgb::new(150, 200, 150), Flags::empty())
            } else if column < 12 {
                (Rgb::new(145, 155, 170), Flags::empty())
            } else {
                (Rgb::new(225, 225, 225), Flags::empty())
            };
            (fg, flags)
        });
    }
    cells
}

fn source_code(columns: usize, lines: usize) -> Vec<RenderableCell> {
    let rows = [
        "fn paint_cells(scene: &mut Scene, cells: &[RenderableCell]) {",
        "    let mut pending = None;",
        "    for cell in cells.iter().filter(|cell| cell.visible()) {",
        "        if let Some(batch) = pending.as_mut() {",
        "            batch.push(cell);",
        "        } else {",
        "            pending = Some(Batch::new(cell));",
        "        }",
        "    }",
        "}",
    ];
    let mut cells = Vec::new();
    for line in 0..lines {
        let row = rows[line % rows.len()];
        push_text_line(&mut cells, line, columns, row, |column, character| {
            let fg = if character.is_ascii_digit() {
                Rgb::new(215, 160, 105)
            } else if "&[](){}|".contains(character) {
                Rgb::new(120, 185, 235)
            } else if column < 8 && !character.is_ascii_punctuation() {
                Rgb::new(195, 140, 230)
            } else if character.is_ascii_punctuation() {
                Rgb::new(150, 160, 175)
            } else {
                Rgb::new(220, 225, 230)
            };
            (fg, Flags::empty())
        });
    }
    cells
}

fn log_stream(columns: usize, lines: usize) -> Vec<RenderableCell> {
    let levels = ["TRACE", "DEBUG", "INFO ", "WARN ", "ERROR"];
    let mut cells = Vec::new();
    for line in 0..lines {
        let level = levels[line % levels.len()];
        let row = format!(
            "2026-08-02T15:{:02}:{:02}.{:03}Z {level} worker={} request={} duration={}µs \
             message=\"frame rendered\"",
            (line / 60) % 60,
            line % 60,
            (line * 37) % 1000,
            line % 12,
            10_000 + line,
            120 + (line * 17) % 900,
        );
        push_text_line(&mut cells, line, columns, &row, |column, character| {
            let fg = if column < 24 {
                Rgb::new(120, 130, 145)
            } else if column < 30 {
                match level.trim() {
                    "ERROR" => Rgb::new(245, 95, 95),
                    "WARN" => Rgb::new(245, 190, 70),
                    "INFO" => Rgb::new(105, 205, 145),
                    _ => Rgb::new(120, 175, 220),
                }
            } else if character == '=' {
                Rgb::new(170, 125, 215)
            } else if character.is_ascii_digit() {
                Rgb::new(215, 160, 105)
            } else {
                Rgb::new(210, 215, 225)
            };
            (fg, Flags::empty())
        });
    }
    cells
}

fn unicode(columns: usize, lines: usize) -> Vec<RenderableCell> {
    let characters = ['λ', 'Ж', '界', 'あ', '한', 'é', '→', '●', '🦀'];
    let mut cells = Vec::with_capacity(columns * lines);
    for line in 0..lines {
        let mut column = 0;
        while column < columns {
            let character = characters[(line + column) % characters.len()];
            let width = character.width().unwrap_or(1).max(1).min(columns - column);
            let flags = if width == 2 { Flags::WIDE_CHAR } else { Flags::empty() };
            cells.push(cell(line, column, character, Rgb::default(), 0.0, flags));
            column += width;
        }
    }
    cells
}

fn wide_cjk(columns: usize, lines: usize) -> Vec<RenderableCell> {
    let characters = ['界', '語', '漢', '字', '한', '글', '🦀', '🙂'];
    let mut cells = Vec::with_capacity(columns * lines / 2);
    for line in 0..lines {
        let mut column = 0;
        while column + 1 < columns {
            let character = characters[(line + column / 2) % characters.len()];
            cells.push(cell(line, column, character, Rgb::default(), 0.0, Flags::WIDE_CHAR));
            column += 2;
        }
    }
    cells
}

fn combining_marks(columns: usize, lines: usize) -> Vec<RenderableCell> {
    let accents = [['\u{0301}', '\u{0308}'], ['\u{0327}', '\u{0304}'], ['\u{0302}', '\u{0307}']];
    let mut cells = grid(columns, lines, |line, column| {
        let character = (b'a' + ((line * columns + column) % 26) as u8) as char;
        (character, Rgb::default(), 0.0, Flags::empty())
    });
    for (index, cell) in cells.iter_mut().enumerate() {
        let marks = accents[index % accents.len()];
        cell.extra = Some(Box::new(RenderableCellExtra {
            zerowidth: Some(if index % 4 == 0 { marks.to_vec() } else { vec![marks[0]] }),
            hyperlink: None,
        }));
    }
    cells
}

fn colored_tui(columns: usize, lines: usize) -> Vec<RenderableCell> {
    grid(columns, lines, |line, column| {
        let band = (column / 8 + line / 4) as u8;
        let bg = Rgb::new(
            20_u8.saturating_add(band.saturating_mul(9)),
            35_u8.saturating_add(band.saturating_mul(5)),
            55_u8.saturating_add(band.saturating_mul(3)),
        );
        let character = if column % 11 == 0 { '│' } else { (b'a' + (column % 26) as u8) as char };
        (character, bg, 1.0, Flags::empty())
    })
}

fn blank_selection(columns: usize, lines: usize) -> Vec<RenderableCell> {
    grid(columns, lines, |_, _| (' ', Rgb::new(55, 65, 80), 1.0, Flags::empty()))
}

fn fragmented_backgrounds(columns: usize, lines: usize) -> Vec<RenderableCell> {
    grid(columns, lines, |line, column| {
        let bg = if (line + column) % 2 == 0 { Rgb::new(35, 55, 85) } else { Rgb::new(85, 45, 55) };
        (' ', bg, 1.0, Flags::empty())
    })
}

fn decorated_grid(columns: usize, lines: usize) -> Vec<RenderableCell> {
    grid(columns, lines, |line, column| {
        let mut flags = match (line + column) % 4 {
            0 => Flags::BOLD,
            1 => Flags::ITALIC,
            2 => Flags::BOLD | Flags::ITALIC,
            _ => Flags::empty(),
        };
        flags |= match line % 4 {
            0 => Flags::UNDERLINE,
            1 => Flags::UNDERCURL,
            2 => Flags::STRIKEOUT,
            _ => Flags::empty(),
        };
        let bg = Rgb::new(25 + (line % 3) as u8 * 12, 25, 35);
        ((b'!' + ((line * columns + column) % 90) as u8) as char, bg, 0.8, flags)
    })
}

fn alternating_styles(columns: usize, lines: usize) -> Vec<RenderableCell> {
    let mut cells = grid(columns, lines, |line, column| {
        let character = (b'a' + ((line * columns + column) % 26) as u8) as char;
        let flags = if (line + column) % 2 == 0 { Flags::BOLD } else { Flags::ITALIC };
        (character, Rgb::default(), 0.0, flags)
    });
    for cell in &mut cells {
        cell.fg = if (cell.point.line + cell.point.column.0) % 2 == 0 {
            Rgb::new(240, 130, 130)
        } else {
            Rgb::new(125, 185, 245)
        };
    }
    cells
}

fn push_text_line(
    cells: &mut Vec<RenderableCell>,
    line: usize,
    columns: usize,
    text: &str,
    mut style: impl FnMut(usize, char) -> (Rgb, Flags),
) {
    let mut column = 0;
    for character in text.chars() {
        if column >= columns {
            break;
        }
        let width = character.width().unwrap_or(1).max(1);
        if column + width > columns {
            break;
        }
        if character != ' ' && character != '\t' {
            let (fg, flags) = style(column, character);
            let wide = if width == 2 { Flags::WIDE_CHAR } else { Flags::empty() };
            let mut renderable = cell(line, column, character, Rgb::default(), 0.0, flags | wide);
            renderable.fg = fg;
            renderable.underline = fg;
            cells.push(renderable);
        }
        column += width;
    }
}

fn grid(
    columns: usize,
    lines: usize,
    mut make: impl FnMut(usize, usize) -> (char, Rgb, f32, Flags),
) -> Vec<RenderableCell> {
    let mut cells = Vec::with_capacity(columns * lines);
    for line in 0..lines {
        for column in 0..columns {
            let (character, bg, bg_alpha, flags) = make(line, column);
            cells.push(cell(line, column, character, bg, bg_alpha, flags));
        }
    }
    cells
}

fn cell(
    line: usize,
    column: usize,
    character: char,
    bg: Rgb,
    bg_alpha: f32,
    flags: Flags,
) -> RenderableCell {
    RenderableCell {
        character,
        point: Point::new(line, Column(column)),
        fg: Rgb::new(225, 225, 225),
        bg,
        bg_alpha,
        underline: Rgb::new(225, 225, 225),
        flags,
        extra: None,
    }
}

#[test]
fn expanded_benchmark_workloads_are_valid() {
    let workloads = workloads();
    assert_eq!(workloads.len(), 13);
    validate_workloads(&workloads);
}
