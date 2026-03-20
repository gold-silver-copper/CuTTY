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
use vello::peniko::Color;

use cutty_terminal::term::cell::Flags;

use crate::config::font::Font;
use crate::display::color::Rgb;
use crate::display::content::RenderableCell;

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
enum LayoutTextKey {
    Char(char),
    String(Box<str>),
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct LayoutKey {
    text: LayoutTextKey,
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

    pub fn shape_cell(&mut self, cell: &RenderableCell) -> Option<Arc<Layout<()>>> {
        if cell.flags.contains(Flags::HIDDEN) {
            return None;
        }

        let variant = font_variant(cell.flags);
        if let Some(extra) = cell.extra.as_ref().and_then(|extra| extra.zerowidth.as_ref()) {
            let mut text = String::with_capacity(1 + extra.len());
            text.push(cell.character);
            text.extend(extra.iter().copied());
            Some(self.shape_text(text, variant))
        } else {
            Some(self.shape_char(cell.character, variant))
        }
    }

    #[cfg(test)]
    pub fn shape_string(
        &mut self,
        text: impl Into<String>,
        bold: bool,
        italic: bool,
    ) -> Option<Arc<Layout<()>>> {
        let text = text.into();
        if text.is_empty() {
            return None;
        }

        let variant = font_variant_from_style(bold, italic);
        if let Some(character) = single_char(&text) {
            Some(self.shape_char(character, variant))
        } else {
            Some(self.shape_text(text, variant))
        }
    }

    pub fn shape_character(
        &mut self,
        character: char,
        bold: bool,
        italic: bool,
    ) -> Arc<Layout<()>> {
        self.shape_char(character, font_variant_from_style(bold, italic))
    }

    fn shape_char(&mut self, character: char, variant: FontVariant) -> Arc<Layout<()>> {
        let mut buffer = [0; 4];
        let text = character.encode_utf8(&mut buffer);
        self.ensure_fontique_fallbacks(text);

        let key = LayoutKey {
            text: LayoutTextKey::Char(character),
            variant,
            font_size_bits: self.font.size().as_px().to_bits(),
        };
        if let Some(layout) = self.cache.get(&key) {
            return Arc::clone(layout);
        }

        self.build_and_cache_layout(key, text, variant)
    }

    fn shape_text(&mut self, text: String, variant: FontVariant) -> Arc<Layout<()>> {
        self.ensure_fontique_fallbacks(&text);

        let key = LayoutKey {
            text: LayoutTextKey::String(text.clone().into_boxed_str()),
            variant,
            font_size_bits: self.font.size().as_px().to_bits(),
        };
        if let Some(layout) = self.cache.get(&key) {
            return Arc::clone(layout);
        }

        self.build_and_cache_layout(key, &text, variant)
    }

    fn build_and_cache_layout(
        &mut self,
        key: LayoutKey,
        text: &str,
        variant: FontVariant,
    ) -> Arc<Layout<()>> {
        let family = Arc::clone(&self.family_stacks[variant.as_index()]);
        let (font_style, font_weight) = font_style(variant);

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

    #[cfg(test)]
    fn cache_len(&self) -> usize {
        self.cache.len()
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

            changed |= self.seed_fontique_fallbacks(key, character);
        }

        if changed {
            self.checked_fallbacks.clear();
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

    fn seed_fontique_fallbacks(&mut self, key: FallbackKey, character: char) -> bool {
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
    const fn as_index(self) -> usize {
        match self {
            Self::Normal => 0,
            Self::Bold => 1,
            Self::Italic => 2,
            Self::BoldItalic => 3,
        }
    }
}

#[cfg(test)]
fn single_char(text: &str) -> Option<char> {
    let mut chars = text.chars();
    let first = chars.next()?;
    chars.next().is_none().then_some(first)
}

fn font_variant(flags: Flags) -> FontVariant {
    font_variant_from_style(
        flags.intersects(Flags::BOLD | Flags::DIM_BOLD),
        flags.contains(Flags::ITALIC),
    )
}

fn font_variant_from_style(bold: bool, italic: bool) -> FontVariant {
    match (bold, italic) {
        (true, true) => FontVariant::BoldItalic,
        (true, false) => FontVariant::Bold,
        (false, true) => FontVariant::Italic,
        (false, false) => FontVariant::Normal,
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

pub fn color_from_rgb(color: Rgb) -> Color {
    Color::from_rgb8(color.r, color.g, color.b)
}

fn family_stacks_for_font(font: &Font) -> [Arc<[FontFamily<'static>]>; 4] {
    array::from_fn(|index| {
        let variant = match index {
            0 => FontVariant::Normal,
            1 => FontVariant::Bold,
            2 => FontVariant::Italic,
            3 => FontVariant::BoldItalic,
            _ => unreachable!("font variant index out of range"),
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
                FontFamily::Generic(family) => {
                    push_family_name(families, FontFamily::Generic(family))
                },
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

fn text_locale() -> Option<String> {
    ["LC_ALL", "LC_CTYPE", "LANG"]
        .into_iter()
        .find_map(|key| env::var(key).ok())
        .and_then(|value| normalize_locale(&value))
}

fn normalize_locale(locale: &str) -> Option<String> {
    let locale = locale.trim();
    if locale.is_empty() || matches!(locale, "C" | "POSIX") {
        return None;
    }

    let locale = locale.split_once('.').map(|(locale, _)| locale).unwrap_or(locale);
    let locale = locale.split_once('@').map(|(locale, _)| locale).unwrap_or(locale);
    let locale = locale.replace('_', "-");

    if locale.is_empty() { None } else { Some(locale) }
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

#[cfg(test)]
mod tests {
    use std::borrow::Cow;
    use std::sync::Arc;

    use cutty_terminal::index::Point;
    use cutty_terminal::term::cell::Flags;
    use parley::fontique::{FallbackKey, Script};
    use parley::layout::PositionedLayoutItem;
    use parley::{FontFamily, GenericFamily};

    use super::{
        FontVariant, TextSystem, family_name_sort_key, font_family_stack, fontique_script_for_char,
        normalize_locale, push_configured_family_names,
    };
    use crate::config::font::Font;
    use crate::display::color::Rgb;
    use crate::display::content::RenderableCell;

    #[test]
    fn font_update_recomputes_metrics() {
        let mut text = TextSystem::new(Font::default());
        let original = text.metrics();
        let updated_font = Font::default().with_size(crate::config::font::FontSize::from_px(22.0));

        text.update_font(updated_font);

        let updated = text.metrics();
        assert!(updated.cell_width >= original.cell_width);
        assert!(updated.cell_height >= original.cell_height);
    }

    #[test]
    fn single_character_layouts_are_reused_without_cloning_the_layout() {
        let mut text = TextSystem::new(Font::default());

        let first = text.shape_string("x", false, false).unwrap();
        let second = text.shape_string("x", false, false).unwrap();

        assert!(Arc::ptr_eq(&first, &second));
        assert_eq!(text.cache_len(), 1);
    }

    #[test]
    fn repeated_multicharacter_layouts_share_cached_layout() {
        let mut text = TextSystem::new(Font::default());

        let first = text.shape_string("hello", false, false).unwrap();
        let second = text.shape_string("hello", false, false).unwrap();

        assert!(Arc::ptr_eq(&first, &second));
        assert_eq!(text.cache_len(), 1);
    }

    #[test]
    fn direct_character_shaping_reuses_cached_layout() {
        let mut text = TextSystem::new(Font::default());

        let first = text.shape_character('x', false, false);
        let second = text.shape_character('x', false, false);

        assert!(Arc::ptr_eq(&first, &second));
        assert_eq!(text.cache_len(), 1);
    }

    #[test]
    fn foreground_color_does_not_split_shape_cache() {
        let mut text = TextSystem::new(Font::default());
        let white = Rgb::new(255, 255, 255);
        let red = Rgb::new(255, 0, 0);
        let base_cell = RenderableCell {
            character: 'x',
            point: Point::default(),
            fg: white,
            bg: Rgb::default(),
            bg_alpha: 0.0,
            underline: Rgb::default(),
            flags: Flags::empty(),
            extra: None,
        };
        let red_cell = RenderableCell { fg: red, ..base_cell.clone() };

        let first = text.shape_cell(&base_cell).unwrap();
        let second = text.shape_cell(&red_cell).unwrap();

        assert!(Arc::ptr_eq(&first, &second));
        assert_eq!(text.cache_len(), 1);
    }

    #[test]
    fn css_family_lists_are_preserved_before_terminal_fallbacks() {
        let mut families = Vec::new();

        push_configured_family_names(&mut families, "'SF Mono', monospace, 'Noto Sans Symbols 2'");

        assert_eq!(families, vec![
            FontFamily::Named(Cow::Owned(String::from("SF Mono"))),
            GenericFamily::Monospace.into(),
            FontFamily::Named(Cow::Owned(String::from("Noto Sans Symbols 2"))),
        ]);
    }

    #[test]
    fn locale_normalization_strips_encoding_and_uses_bcp47_separators() {
        assert_eq!(normalize_locale("ja_JP.UTF-8"), Some(String::from("ja-JP")));
        assert_eq!(
            normalize_locale("zh_Hans_CN@calendar=gregorian"),
            Some(String::from("zh-Hans-CN"))
        );
        assert_eq!(normalize_locale("C"), None);
    }

    #[test]
    fn character_scripts_are_mapped_to_fontique_tags() {
        assert_eq!(fontique_script_for_char('今'), Some(Script::from("Hani")));
        assert_eq!(fontique_script_for_char('あ'), Some(Script::from("Hira")));
        assert_eq!(fontique_script_for_char('한'), Some(Script::from("Hang")));
        assert_eq!(fontique_script_for_char('!'), None);
    }

    #[test]
    fn hidden_family_names_sort_after_visible_names() {
        assert!(
            family_name_sort_key("Apple SD Gothic Neo")
                < family_name_sort_key(".Apple SD Gothic Neo")
        );
    }

    #[test]
    fn invalid_css_family_spec_falls_back_to_literal_name() {
        let mut families = Vec::new();

        push_configured_family_names(&mut families, "'broken");

        assert_eq!(families, vec![FontFamily::Named(Cow::Owned(String::from("broken")))]);
    }

    #[test]
    fn variant_family_stack_deduplicates_configured_and_generic_fallbacks() {
        let families = font_family_stack(&Font::default(), FontVariant::Bold);

        assert_eq!(
            families.iter().filter(|family| **family == GenericFamily::Monospace.into()).count(),
            1
        );
        assert!(families.contains(&GenericFamily::UiMonospace.into()));
        assert!(families.contains(&GenericFamily::SystemUi.into()));
        assert!(families.contains(&GenericFamily::Emoji.into()));
        assert!(!families.is_empty());
    }

    #[test]
    fn untracked_han_locale_falls_back_to_script_default_key() {
        let mut text = TextSystem::new(Font::default());
        text.locale = Some(String::from("en-US"));

        let key = text.fallback_key_for_char('今').expect("han fallback key");

        assert_eq!(key, FallbackKey::from(Script::from("Hani")));
        assert!(key.is_tracked());
        assert!(key.is_default());
    }

    fn selected_run_font<'a>(text: &'a mut TextSystem, content: &str) -> Option<parley::FontData> {
        let layout = text.shape_string(content.to_owned(), false, false)?;
        layout.lines().find_map(|line| {
            line.items().find_map(|item| match item {
                PositionedLayoutItem::GlyphRun(glyph_run) => Some(glyph_run.run().font().clone()),
                _ => None,
            })
        })
    }

    fn selected_family_name(text: &mut TextSystem, content: &str) -> Option<String> {
        let target = selected_run_font(text, content)?;
        let family_names =
            text.font_cx.collection.family_names().map(str::to_owned).collect::<Vec<_>>();
        for family_name in family_names {
            let Some(family_id) = text.font_cx.collection.family_id(&family_name) else {
                continue;
            };
            let Some(family) = text.font_cx.collection.family(family_id) else {
                continue;
            };
            if family.fonts().iter().any(|font| {
                font.index() == target.index
                    && font
                        .load(Some(&mut text.font_cx.source_cache))
                        .is_some_and(|data| data.as_ref() == target.data.as_ref())
            }) {
                return Some(family.name().to_owned());
            }
        }
        None
    }

    fn family_glyph_id_for(
        text: &mut TextSystem,
        family_name: &str,
        character: char,
    ) -> Option<u32> {
        let family_id = text.font_cx.collection.family_id(family_name)?;
        let family = text.font_cx.collection.family(family_id)?;
        family.fonts().iter().find_map(|font| {
            let data = font.load(Some(&mut text.font_cx.source_cache))?;
            let charmap = font.charmap_index().charmap(data.as_ref())?;
            charmap.map(character)
        })
    }

    fn first_layout_glyph_id(text: &mut TextSystem, content: &str) -> Option<u32> {
        let layout = text.shape_string(content.to_owned(), false, false)?;
        layout.lines().find_map(|line| {
            line.items().find_map(|item| match item {
                PositionedLayoutItem::GlyphRun(glyph_run) => {
                    glyph_run.glyphs().next().map(|glyph| glyph.id)
                },
                _ => None,
            })
        })
    }

    #[test]
    #[ignore = "diagnostic helper"]
    fn debug_cjk_sample_glyphs() {
        let mut text = TextSystem::new(Font::default());
        for character in "今語漢あア한글".chars() {
            let family = selected_family_name(&mut text, &character.to_string());
            let layout = first_layout_glyph_id(&mut text, &character.to_string());
            let family_glyph = family
                .as_deref()
                .and_then(|family_name| family_glyph_id_for(&mut text, family_name, character));
            let square = family
                .as_deref()
                .and_then(|family_name| family_glyph_id_for(&mut text, family_name, '□'));
            println!(
                "char={character:?} family={family:?} layout={layout:?} \
                 family_glyph={family_glyph:?} square={square:?}"
            );
        }
    }
}
