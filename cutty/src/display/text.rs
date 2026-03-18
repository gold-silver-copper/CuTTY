use std::borrow::Cow;
use std::sync::Arc;

use ahash::AHashMap;
use parley::layout::PositionedLayoutItem;
use parley::{
    Alignment, AlignmentOptions, FontContext, FontFamily, FontStack, FontStyle as ParleyFontStyle,
    FontWeight, GenericFamily, Layout, LayoutContext, LineHeight, StyleProperty,
};
use vello::peniko::{Brush, Color};

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
    fg: (u8, u8, u8),
    font_size_bits: u32,
}

pub struct TextSystem {
    font: Font,
    font_cx: FontContext,
    layout_cx: LayoutContext<Brush>,
    metrics: TextMetrics,
    cache: AHashMap<LayoutKey, Arc<Layout<Brush>>>,
}

impl TextSystem {
    pub fn new(font: Font) -> Self {
        let mut text_system = Self {
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
        self.metrics = self.measure_metrics();
        self.cache.clear();
    }

    pub fn clear_cache(&mut self) {
        self.cache.clear();
    }

    pub fn shape_cell(&mut self, cell: &RenderableCell) -> Option<Arc<Layout<Brush>>> {
        if cell.flags.contains(Flags::HIDDEN) {
            return None;
        }

        let variant = font_variant(cell.flags);
        if let Some(extra) = cell.extra.as_ref().and_then(|extra| extra.zerowidth.as_ref()) {
            let mut text = String::with_capacity(1 + extra.len());
            text.push(cell.character);
            text.extend(extra.iter().copied());
            Some(self.shape_text(text, variant, cell.fg))
        } else {
            Some(self.shape_char(cell.character, variant, cell.fg))
        }
    }

    pub fn shape_string(
        &mut self,
        text: impl Into<String>,
        fg: Rgb,
        bold: bool,
        italic: bool,
    ) -> Option<Arc<Layout<Brush>>> {
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
        if text.chars().count() == 1 {
            Some(self.shape_char(text.chars().next().unwrap(), variant, fg))
        } else {
            Some(self.shape_text(text, variant, fg))
        }
    }

    fn shape_char(&mut self, character: char, variant: FontVariant, fg: Rgb) -> Arc<Layout<Brush>> {
        let key = LayoutKey {
            text: LayoutTextKey::Char(character),
            variant,
            fg: fg.as_tuple(),
            font_size_bits: self.font.size().as_px().to_bits(),
        };
        if let Some(layout) = self.cache.get(&key) {
            return Arc::clone(layout);
        }

        let mut buffer = [0; 4];
        let text = character.encode_utf8(&mut buffer);
        self.build_and_cache_layout(key, text, variant, fg)
    }

    fn shape_text(&mut self, text: String, variant: FontVariant, fg: Rgb) -> Arc<Layout<Brush>> {
        let key = LayoutKey {
            text: LayoutTextKey::String(text.clone().into_boxed_str()),
            variant,
            fg: fg.as_tuple(),
            font_size_bits: self.font.size().as_px().to_bits(),
        };
        if let Some(layout) = self.cache.get(&key) {
            return Arc::clone(layout);
        }

        self.build_and_cache_layout(key, &text, variant, fg)
    }

    fn build_and_cache_layout(
        &mut self,
        key: LayoutKey,
        text: &str,
        variant: FontVariant,
        fg: Rgb,
    ) -> Arc<Layout<Brush>> {
        let family = self.font_family(variant);
        let (font_style, font_weight) = font_style(variant);

        let mut builder = self.layout_cx.ranged_builder(&mut self.font_cx, text, 1.0, true);
        builder.push_default(family);
        builder.push_default(StyleProperty::FontSize(self.font.size().as_px()));
        builder.push_default(StyleProperty::FontStyle(font_style));
        builder.push_default(StyleProperty::FontWeight(font_weight));
        builder.push_default(LineHeight::Absolute(self.metrics.cell_height));
        builder.push_default(StyleProperty::Brush(Brush::Solid(color_from_rgb(fg))));

        let mut layout = builder.build(text);
        layout.break_all_lines(None);
        layout.align(None, Alignment::Start, AlignmentOptions::default());

        let layout = Arc::new(layout);
        self.cache.insert(key, Arc::clone(&layout));
        layout
    }

    fn measure_metrics(&mut self) -> TextMetrics {
        let sample = "M";
        let family = self.font_family(FontVariant::Normal);
        let font_size = self.font.size().as_px();
        let mut builder = self.layout_cx.ranged_builder(&mut self.font_cx, sample, 1.0, true);
        builder.push_default(family);
        builder.push_default(StyleProperty::FontSize(font_size));
        builder.push_default(StyleProperty::Brush(Brush::Solid(Color::WHITE)));

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

    fn font_family(&self, variant: FontVariant) -> FontStack<'static> {
        FontStack::List(Cow::Owned(font_family_stack(&self.font, variant)))
    }

    #[cfg(test)]
    fn cache_len(&self) -> usize {
        self.cache.len()
    }
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

    use parley::{FontFamily, GenericFamily};

    use super::{FontVariant, TextSystem, font_family_stack, push_configured_family_names};
    use crate::config::font::Font;
    use crate::display::color::Rgb;

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
        let fg = Rgb::new(255, 255, 255);

        let first = text.shape_string("x", fg, false, false).unwrap();
        let second = text.shape_string("x", fg, false, false).unwrap();

        assert!(Arc::ptr_eq(&first, &second));
        assert_eq!(text.cache_len(), 1);
    }

    #[test]
    fn repeated_multicharacter_layouts_share_cached_layout() {
        let mut text = TextSystem::new(Font::default());
        let fg = Rgb::new(255, 255, 255);

        let first = text.shape_string("hello", fg, false, false).unwrap();
        let second = text.shape_string("hello", fg, false, false).unwrap();

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
