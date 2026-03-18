use std::borrow::Cow;
use std::collections::HashMap;

use parley::layout::PositionedLayoutItem;
use parley::{
    Alignment, AlignmentOptions, FontContext, FontFamily, FontFamilyName,
    FontStyle as ParleyFontStyle, FontWeight, GenericFamily, Layout, LayoutContext, LineHeight,
    StyleProperty,
};
use vello::peniko::{Brush, Color};

use alacritty_terminal::term::cell::Flags;

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
struct LayoutKey {
    text: String,
    variant: FontVariant,
    fg: (u8, u8, u8),
    font_size_bits: u32,
}

pub struct TextSystem {
    font: Font,
    font_cx: FontContext,
    layout_cx: LayoutContext<Brush>,
    metrics: TextMetrics,
    cache: HashMap<LayoutKey, Layout<Brush>>,
}

impl TextSystem {
    pub fn new(font: Font) -> Self {
        let mut text_system = Self {
            font,
            font_cx: FontContext::default(),
            layout_cx: LayoutContext::default(),
            metrics: TextMetrics::default(),
            cache: HashMap::new(),
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

    pub fn shape_cell(&mut self, cell: &RenderableCell) -> Option<Layout<Brush>> {
        let text = cell_text(cell);
        if text.is_empty() || cell.flags.contains(Flags::HIDDEN) {
            return None;
        }

        let variant = font_variant(cell.flags);
        Some(self.shape_text(text, variant, cell.fg))
    }

    pub fn shape_string(
        &mut self,
        text: impl Into<String>,
        fg: Rgb,
        bold: bool,
        italic: bool,
    ) -> Option<Layout<Brush>> {
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
        Some(self.shape_text(text, variant, fg))
    }

    fn shape_text(&mut self, text: String, variant: FontVariant, fg: Rgb) -> Layout<Brush> {
        let key = LayoutKey {
            text: text.clone(),
            variant,
            fg: fg.as_tuple(),
            font_size_bits: self.font.size().as_px().to_bits(),
        };
        if let Some(layout) = self.cache.get(&key) {
            return layout.clone();
        }

        let family = self.font_family(variant);
        let (font_style, font_weight) = font_style(variant);

        let mut builder = self.layout_cx.ranged_builder(&mut self.font_cx, &text, 1.0, true);
        builder.push_default(StyleProperty::FontFamily(family));
        builder.push_default(StyleProperty::FontSize(self.font.size().as_px()));
        builder.push_default(StyleProperty::FontStyle(font_style));
        builder.push_default(StyleProperty::FontWeight(font_weight));
        builder.push_default(LineHeight::Absolute(self.metrics.cell_height));
        builder.push_default(StyleProperty::Brush(Brush::Solid(color_from_rgb(fg))));

        let mut layout = builder.build(&text);
        layout.break_all_lines(None);
        layout.align(None, Alignment::Start, AlignmentOptions::default());

        self.cache.insert(key, layout.clone());
        layout
    }

    fn measure_metrics(&mut self) -> TextMetrics {
        let sample = "M";
        let family = self.font_family(FontVariant::Normal);
        let font_size = self.font.size().as_px();
        let mut builder = self.layout_cx.ranged_builder(&mut self.font_cx, sample, 1.0, true);
        builder.push_default(StyleProperty::FontFamily(family));
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

    fn font_family(&self, variant: FontVariant) -> FontFamily<'static> {
        FontFamily::List(Cow::Owned(font_family_stack(&self.font, variant)))
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

fn cell_text(cell: &RenderableCell) -> String {
    let mut text = String::new();
    text.push(cell.character);
    if let Some(extra) = cell.extra.as_ref() {
        if let Some(zerowidth) = extra.zerowidth.as_ref() {
            text.extend(zerowidth.iter().copied());
        }
    }
    text
}

pub fn color_from_rgb(color: Rgb) -> Color {
    Color::from_rgb8(color.r, color.g, color.b)
}

fn font_family_stack(font: &Font, variant: FontVariant) -> Vec<FontFamilyName<'static>> {
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

fn push_configured_family_names(
    families: &mut Vec<FontFamilyName<'static>>,
    spec: impl AsRef<str>,
) {
    let spec = spec.as_ref().trim();
    if spec.is_empty() {
        return;
    }

    match FontFamilyName::parse_css_list(spec).collect::<Result<Vec<_>, _>>() {
        Ok(parsed) if !parsed.is_empty() => {
            for family in parsed {
                push_family_name(families, family.into_owned());
            }
        },
        _ => push_family_name(families, FontFamilyName::named(spec).into_owned()),
    }
}

fn push_family_name(families: &mut Vec<FontFamilyName<'static>>, family: FontFamilyName<'static>) {
    if !families.contains(&family) {
        families.push(family);
    }
}

#[cfg(test)]
mod tests {
    use parley::{FontFamilyName, GenericFamily};

    use super::{FontVariant, TextSystem, font_family_stack, push_configured_family_names};
    use crate::config::font::Font;

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
    fn css_family_lists_are_preserved_before_terminal_fallbacks() {
        let mut families = Vec::new();

        push_configured_family_names(&mut families, "'SF Mono', monospace, 'Noto Sans Symbols 2'");

        assert_eq!(families, vec![
            FontFamilyName::named("SF Mono").into_owned(),
            GenericFamily::Monospace.into(),
            FontFamilyName::named("Noto Sans Symbols 2").into_owned(),
        ]);
    }

    #[test]
    fn invalid_css_family_spec_falls_back_to_literal_name() {
        let mut families = Vec::new();

        push_configured_family_names(&mut families, "'broken");

        assert_eq!(families, vec![FontFamilyName::named("'broken").into_owned()]);
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
