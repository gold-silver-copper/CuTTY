use std::array;
use std::borrow::Cow;
use std::sync::Arc;

use ahash::AHashMap;
use parley::layout::PositionedLayoutItem;
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
    family_stacks: [Arc<[FontFamily<'static>]>; 4],
    cache: AHashMap<LayoutKey, Arc<Layout<()>>>,
}

impl TextSystem {
    pub fn new(font: Font) -> Self {
        let mut text_system = Self {
            family_stacks: family_stacks_for_font(&font),
            font,
            font_cx: FontContext::default(),
            layout_cx: LayoutContext::default(),
            metrics: TextMetrics::default(),
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
        self.cache.clear();
    }

    pub fn clear_cache(&mut self) {
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

        let variant = match (bold, italic) {
            (true, true) => FontVariant::BoldItalic,
            (true, false) => FontVariant::Bold,
            (false, true) => FontVariant::Italic,
            (false, false) => FontVariant::Normal,
        };
        if let Some(character) = single_char(&text) {
            Some(self.shape_char(character, variant))
        } else {
            Some(self.shape_text(text, variant))
        }
    }

    fn shape_char(&mut self, character: char, variant: FontVariant) -> Arc<Layout<()>> {
        let key = LayoutKey {
            text: LayoutTextKey::Char(character),
            variant,
            font_size_bits: self.font.size().as_px().to_bits(),
        };
        if let Some(layout) = self.cache.get(&key) {
            return Arc::clone(layout);
        }

        let mut buffer = [0; 4];
        let text = character.encode_utf8(&mut buffer);
        self.build_and_cache_layout(key, text, variant)
    }

    fn shape_text(&mut self, text: String, variant: FontVariant) -> Arc<Layout<()>> {
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

fn single_char(text: &str) -> Option<char> {
    let mut chars = text.chars();
    let first = chars.next()?;
    chars.next().is_none().then_some(first)
}

fn font_variant(flags: Flags) -> FontVariant {
    match (flags.intersects(Flags::BOLD | Flags::DIM_BOLD), flags.contains(Flags::ITALIC)) {
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

#[cfg(test)]
mod tests {
    use std::borrow::Cow;
    use std::sync::Arc;

    use cutty_terminal::index::Point;
    use cutty_terminal::term::cell::Flags;
    use parley::{FontFamily, GenericFamily};

    use super::{FontVariant, TextSystem, font_family_stack, push_configured_family_names};
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
}
