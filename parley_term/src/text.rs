use std::borrow::Cow;
use std::sync::Arc;
use std::{array, env};

use ahash::{AHashMap, AHashSet};
use parley::fontique::{FallbackKey, FamilyId};
use parley::layout::PositionedLayoutItem;
use parley::swash::text::Codepoint as _;
use parley::{
    Alignment, AlignmentOptions, FontContext, FontFamily, FontStack, FontStyle as ParleyFontStyle,
    FontWeight, GenericFamily, Layout, LayoutContext, LineHeight, StyleProperty,
};

use crate::font::Font;
use crate::grid::{CellFlags, TerminalCell};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TextMetrics {
    pub cell_width: f32,
    pub cell_height: f32,
    pub baseline: f32,
    pub descent: f32,
    pub underline_position: f32,
    pub underline_thickness: f32,
    pub strikeout_position: f32,
    pub strikeout_thickness: f32,
    pub glyph_offset_x: f32,
    pub glyph_offset_y: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum FontVariant {
    Normal,
    Bold,
    Italic,
    BoldItalic,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct LayoutKey {
    text: Box<str>,
    variant: FontVariant,
    font_size_bits: u32,
}

pub struct TextSystem {
    font: Font,
    font_cx: FontContext,
    layout_cx: LayoutContext<()>,
    metrics: TextMetrics,
    locale: Option<String>,
    fallback_search_families: Arc<[FamilyId]>,
    checked_fallbacks: AHashSet<(FallbackKey, char)>,
    family_stacks: [Arc<[FontFamily<'static>]>; 4],
    cache: AHashMap<LayoutKey, Arc<Layout<()>>>,
}

impl TextSystem {
    pub fn new(font: Font) -> Self {
        let mut font_cx = FontContext::default();
        let fallback_search_families = fallback_search_families(&mut font_cx);
        let mut text_system = Self {
            family_stacks: family_stacks_for_font(&font),
            font,
            font_cx,
            layout_cx: LayoutContext::default(),
            metrics: TextMetrics::default(),
            locale: text_locale(),
            fallback_search_families,
            checked_fallbacks: AHashSet::default(),
            cache: AHashMap::new(),
        };
        text_system.metrics = text_system.measure_metrics();
        text_system
    }

    pub fn metrics(&self) -> TextMetrics {
        self.metrics
    }

    pub fn update_font(&mut self, font: Font) {
        self.font = font;
        self.family_stacks = family_stacks_for_font(&self.font);
        self.metrics = self.measure_metrics();
        self.checked_fallbacks.clear();
        self.cache.clear();
    }

    pub fn shape_cell(&mut self, cell: &TerminalCell) -> Option<Arc<Layout<()>>> {
        if cell.flags.contains(CellFlags::HIDDEN) || cell.text.is_empty() {
            return None;
        }

        Some(self.shape_text(cell.text.clone(), font_variant(cell.flags)))
    }

    pub fn shape_character(
        &mut self,
        character: char,
        bold: bool,
        italic: bool,
    ) -> Arc<Layout<()>> {
        let mut buffer = [0; 4];
        let text = character.encode_utf8(&mut buffer);
        self.shape_text(text, font_variant_from_style(bold, italic))
    }

    fn shape_text(&mut self, text: impl Into<Box<str>>, variant: FontVariant) -> Arc<Layout<()>> {
        let text = text.into();
        self.ensure_fontique_fallbacks(&text);

        let key = LayoutKey {
            text: text.clone(),
            variant,
            font_size_bits: self.font.size().as_px().to_bits(),
        };
        if let Some(layout) = self.cache.get(&key) {
            return Arc::clone(layout);
        }

        self.build_and_cache_layout(key)
    }

    fn build_and_cache_layout(&mut self, key: LayoutKey) -> Arc<Layout<()>> {
        let family = Arc::clone(&self.family_stacks[key.variant.as_index()]);
        let (font_style, font_weight) = font_style(key.variant);
        let text = key.text.as_ref();

        let mut builder = self.layout_cx.ranged_builder(&mut self.font_cx, text, 1.0, true);
        builder.push_default(FontStack::from(&family[..]));
        builder.push_default(StyleProperty::FontSize(self.font.size().as_px()));
        builder.push_default(StyleProperty::FontStyle(font_style));
        builder.push_default(StyleProperty::FontWeight(font_weight));
        builder.push_default(StyleProperty::Locale(self.locale.as_deref()));
        builder.push_default(LineHeight::Absolute(self.metrics.cell_height));

        let mut layout = builder.build(text);
        layout.break_all_lines(None);
        layout.align(None, Alignment::Start, AlignmentOptions::default());

        let layout = Arc::new(layout);
        self.cache.insert(key, Arc::clone(&layout));
        layout
    }

    fn measure_metrics(&mut self) -> TextMetrics {
        let sample = "M";
        let family = Arc::clone(&self.family_stacks[FontVariant::Normal.as_index()]);
        let font_size = self.font.size().as_px();
        let mut builder = self.layout_cx.ranged_builder(&mut self.font_cx, sample, 1.0, true);
        builder.push_default(FontStack::from(&family[..]));
        builder.push_default(StyleProperty::FontSize(font_size));
        builder.push_default(StyleProperty::Locale(self.locale.as_deref()));

        let mut layout = builder.build(sample);
        layout.break_all_lines(None);
        layout.align(None, Alignment::Start, AlignmentOptions::default());

        let line = layout.lines().next().expect("sample line");
        let run_metrics = line
            .items()
            .find_map(|item| match item {
                PositionedLayoutItem::GlyphRun(glyph_run) => Some(*glyph_run.run().metrics()),
                _ => None,
            })
            .unwrap_or_default();

        TextMetrics {
            cell_width: (layout.full_width() + f32::from(self.font.offset.x)).floor().max(1.0),
            cell_height: (line.metrics().line_height + f32::from(self.font.offset.y))
                .floor()
                .max(1.0),
            baseline: line.metrics().baseline,
            descent: line.metrics().descent,
            underline_position: run_metrics.underline_offset,
            underline_thickness: run_metrics.underline_size.max(1.0),
            strikeout_position: run_metrics.strikethrough_offset,
            strikeout_thickness: run_metrics.strikethrough_size.max(1.0),
            glyph_offset_x: f32::from(self.font.glyph_offset.x),
            glyph_offset_y: f32::from(self.font.glyph_offset.y),
        }
    }

    fn ensure_fontique_fallbacks(&mut self, text: &str) {
        let mut changed = false;

        for character in text.chars() {
            let Some(key) = self.fallback_key_for_char(character) else {
                continue;
            };

            if !self.checked_fallbacks.insert((key, character)) {
                continue;
            }

            if self.fallbacks_support_character(key, character) {
                continue;
            }

            changed |= self.add_matching_fallbacks(key, character);
        }

        if changed {
            self.cache.clear();
        }
    }

    fn fallback_key_for_char(&self, character: char) -> Option<FallbackKey> {
        let script = fontique_script_for_char(character)?;
        let localized = self.locale.as_deref().map(|locale| FallbackKey::from((script, locale)));
        match localized {
            Some(key) if key.is_tracked() => Some(key),
            _ => Some(FallbackKey::from(script)),
        }
    }

    fn fallbacks_support_character(&mut self, key: FallbackKey, character: char) -> bool {
        let fallback_families = self.font_cx.collection.fallback_families(key).collect::<Vec<_>>();
        let mut buffer = [0; 4];
        let character_text = character.encode_utf8(&mut buffer);
        fallback_families
            .into_iter()
            .any(|family_id| self.family_supports_text(family_id, character_text))
    }

    fn add_matching_fallbacks(&mut self, key: FallbackKey, character: char) -> bool {
        let fallback_families = self.find_fallback_families(key.script(), character);
        if fallback_families.is_empty() {
            return false;
        }

        self.font_cx.collection.append_fallbacks(key, fallback_families.into_iter())
    }

    fn find_fallback_families(
        &mut self,
        script: parley::fontique::Script,
        character: char,
    ) -> Vec<FamilyId> {
        let mut character_buffer = [0; 4];
        let character_text = character.encode_utf8(&mut character_buffer);
        let sample_text = script.sample().unwrap_or(character_text);
        let use_sample_text = sample_text != character_text;
        let search_families = Arc::clone(&self.fallback_search_families);

        let mut preferred = Vec::new();
        let mut fallback_only = Vec::new();
        for &family_id in search_families.iter() {
            if !self.family_supports_text(family_id, character_text) {
                continue;
            }

            if use_sample_text && self.family_supports_text(family_id, sample_text) {
                preferred.push(family_id);
            } else {
                fallback_only.push(family_id);
            }
        }

        preferred.extend(fallback_only);
        preferred
    }

    fn family_supports_text(&mut self, family_id: FamilyId, text: &str) -> bool {
        let Some(family) = self.font_cx.collection.family(family_id) else {
            return false;
        };

        family.fonts().iter().any(|font| {
            let Some(data) = font.load(Some(&mut self.font_cx.source_cache)) else {
                return false;
            };
            let Some(charmap) = font.charmap_index().charmap(data.as_ref()) else {
                return false;
            };

            text.chars()
                .all(|character| charmap.map(character).is_some_and(|glyph_id| glyph_id != 0))
        })
    }
}

impl FontVariant {
    fn as_index(self) -> usize {
        match self {
            Self::Normal => 0,
            Self::Bold => 1,
            Self::Italic => 2,
            Self::BoldItalic => 3,
        }
    }
}

fn font_variant(flags: CellFlags) -> FontVariant {
    font_variant_from_style(
        flags.contains(CellFlags::BOLD),
        flags.contains(CellFlags::ITALIC),
    )
}

fn font_variant_from_style(bold: bool, italic: bool) -> FontVariant {
    match (
        bold,
        italic,
    ) {
        (false, false) => FontVariant::Normal,
        (true, false) => FontVariant::Bold,
        (false, true) => FontVariant::Italic,
        (true, true) => FontVariant::BoldItalic,
    }
}

fn font_style(variant: FontVariant) -> (ParleyFontStyle, FontWeight) {
    match variant {
        FontVariant::Normal => (ParleyFontStyle::Normal, FontWeight::NORMAL),
        FontVariant::Bold => (ParleyFontStyle::Normal, FontWeight::BOLD),
        FontVariant::Italic => (ParleyFontStyle::Italic, FontWeight::NORMAL),
        FontVariant::BoldItalic => (ParleyFontStyle::Italic, FontWeight::BOLD),
    }
}

fn family_stacks_for_font(font: &Font) -> [Arc<[FontFamily<'static>]>; 4] {
    array::from_fn(|index| {
        let variant = match index {
            0 => FontVariant::Normal,
            1 => FontVariant::Bold,
            2 => FontVariant::Italic,
            3 => FontVariant::BoldItalic,
            _ => unreachable!("font variant index"),
        };
        Arc::from(font_family_stack(font, variant))
    })
}

fn font_family_stack(font: &Font, variant: FontVariant) -> Vec<FontFamily<'static>> {
    let mut families = Vec::new();
    push_configured_family_names(&mut families, variant_family_spec(font, variant));

    if variant != FontVariant::Normal {
        push_configured_family_names(&mut families, &font.normal().family);
    }

    push_family_name(&mut families, GenericFamily::UiMonospace.into());
    push_family_name(&mut families, GenericFamily::Monospace.into());
    push_family_name(&mut families, GenericFamily::SystemUi.into());
    push_family_name(&mut families, GenericFamily::Emoji.into());

    families
}

fn variant_family_spec(font: &Font, variant: FontVariant) -> Cow<'_, str> {
    match variant {
        FontVariant::Normal => Cow::Borrowed(&font.normal().family),
        FontVariant::Bold => Cow::Owned(font.bold().family),
        FontVariant::Italic => Cow::Owned(font.italic().family),
        FontVariant::BoldItalic => Cow::Owned(font.bold_italic().family),
    }
}

fn fallback_search_families(font_cx: &mut FontContext) -> Arc<[FamilyId]> {
    let mut families = Vec::new();
    let mut seen = AHashSet::default();

    for generic_family in [
        GenericFamily::UiMonospace,
        GenericFamily::Monospace,
        GenericFamily::SystemUi,
        GenericFamily::Emoji,
    ] {
        for family_id in font_cx.collection.generic_families(generic_family) {
            if seen.insert(family_id) {
                families.push(family_id);
            }
        }
    }

    let mut family_names = font_cx.collection.family_names().map(str::to_owned).collect::<Vec<_>>();
    family_names.sort_unstable_by_key(|family_name| family_name_sort_key(family_name));
    family_names.dedup();

    for family_name in family_names {
        let Some(family_id) = font_cx.collection.family_id(&family_name) else {
            continue;
        };
        if seen.insert(family_id) {
            families.push(family_id);
        }
    }

    Arc::from(families)
}

fn family_name_sort_key(family_name: &str) -> (bool, String) {
    (family_name.starts_with('.'), family_name.to_ascii_lowercase())
}

fn push_configured_family_names(families: &mut Vec<FontFamily<'static>>, spec: impl AsRef<str>) {
    let spec = spec.as_ref().trim();
    if spec.is_empty() {
        return;
    }

    let parsed = FontFamily::parse_list(spec).collect::<Vec<_>>();
    if parsed.is_empty() {
        push_family_name(families, named_family(spec));
    } else {
        for family in parsed {
            match family {
                FontFamily::Named(name) => push_family_name(families, named_family(name.as_ref())),
                FontFamily::Generic(family) => push_family_name(families, FontFamily::Generic(family)),
            }
        }
    }
}

fn named_family(name: impl AsRef<str>) -> FontFamily<'static> {
    FontFamily::Named(Cow::Owned(name.as_ref().to_owned()))
}

fn push_family_name(families: &mut Vec<FontFamily<'static>>, family: FontFamily<'static>) {
    if !families.contains(&family) {
        families.push(family);
    }
}

fn fontique_script_for_char(character: char) -> Option<parley::fontique::Script> {
    let tag = character.script().to_opentype();
    let mut bytes = [(tag >> 24) as u8, (tag >> 16) as u8, (tag >> 8) as u8, tag as u8];
    bytes[0] = bytes[0].to_ascii_uppercase();
    bytes[1] = bytes[1].to_ascii_lowercase();
    bytes[2] = bytes[2].to_ascii_lowercase();
    bytes[3] = bytes[3].to_ascii_lowercase();
    let script = parley::fontique::Script(bytes);
    (!matches!(&script.0, b"Zyyy" | b"Zinh" | b"Zzzz")).then_some(script)
}

fn text_locale() -> Option<String> {
    env::var("LC_ALL")
        .ok()
        .or_else(|| env::var("LC_CTYPE").ok())
        .or_else(|| env::var("LANG").ok())
}
