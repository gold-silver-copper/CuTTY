#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Font {
    pub offset: FontOffset,
    pub glyph_offset: FontOffset,
    pub normal: FontDescription,
    pub bold: Option<FontDescription>,
    pub italic: Option<FontDescription>,
    pub bold_italic: Option<FontDescription>,
    pub size: FontSize,
}

impl Default for Font {
    fn default() -> Self {
        Self {
            offset: FontOffset::default(),
            glyph_offset: FontOffset::default(),
            normal: FontDescription::default(),
            bold: None,
            italic: None,
            bold_italic: None,
            size: FontSize::new(11.25),
        }
    }
}

impl Font {
    pub fn with_size(self, size: FontSize) -> Self {
        Self { size, ..self }
    }

    pub fn size(&self) -> FontSize {
        self.size
    }

    pub fn normal(&self) -> &FontDescription {
        &self.normal
    }

    pub fn bold(&self) -> FontDescription {
        self.bold.clone().unwrap_or_else(|| self.normal.clone())
    }

    pub fn italic(&self) -> FontDescription {
        self.italic.clone().unwrap_or_else(|| self.normal.clone())
    }

    pub fn bold_italic(&self) -> FontDescription {
        self.bold_italic.clone().unwrap_or_else(|| self.bold().with_style(self.italic().style))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FontDescription {
    pub family: String,
    pub style: Option<String>,
}

impl Default for FontDescription {
    fn default() -> Self {
        Self { family: "FiraMono Nerd Font".into(), style: None }
    }
}

impl FontDescription {
    pub fn with_style(mut self, style: Option<String>) -> Self {
        self.style = style;
        self
    }
}

#[derive(Debug, Default, Copy, Clone, PartialEq, Eq)]
pub struct FontOffset {
    pub x: i8,
    pub y: i8,
}

#[derive(Debug, Copy, Clone, PartialOrd, PartialEq)]
pub struct FontSize(f32);

impl Eq for FontSize {}

impl FontSize {
    pub const fn new(size: f32) -> Self {
        Self(size)
    }

    pub const fn from_px(size: f32) -> Self {
        Self(size)
    }

    pub const fn as_px(self) -> f32 {
        self.0
    }

    pub const fn scale(self, factor: f32) -> Self {
        Self(self.0 * factor)
    }
}
