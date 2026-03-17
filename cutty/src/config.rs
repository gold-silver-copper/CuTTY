use std::borrow::Cow;

use anyhow::{Result, bail};
use parley::{FontFamily, FontFamilyName, GenericFamily};
use serde::Deserialize;

/// User-configurable settings for the bundled desktop terminal app.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct AppConfig {
    pub font: FontConfig,
    pub window: WindowConfig,
    pub terminal: TerminalConfig,
}

impl AppConfig {
    pub fn validate(&self) -> Result<()> {
        self.font.validate()?;
        self.window.validate()?;
        self.terminal.validate()?;
        Ok(())
    }
}

/// Font selection and metrics for terminal text rendering.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct FontConfig {
    pub families: Vec<String>,
    pub size: f32,
    pub line_height: f32,
}

impl Default for FontConfig {
    fn default() -> Self {
        Self {
            families: vec!["monospace".to_owned()],
            size: 18.0,
            line_height: 1.25,
        }
    }
}

impl FontConfig {
    pub fn validate(&self) -> Result<()> {
        if self.families.is_empty() {
            bail!("font.families must include at least one family");
        }
        if self.families.iter().any(|family| family.trim().is_empty()) {
            bail!("font.families cannot contain empty entries");
        }
        if !self.size.is_finite() || self.size <= 0.0 {
            bail!("font.size must be a positive finite number");
        }
        if !self.line_height.is_finite() || self.line_height <= 0.0 {
            bail!("font.line_height must be a positive finite number");
        }
        Ok(())
    }

    pub(crate) fn family_stack(&self) -> FontFamily<'static> {
        let families = if self.families.is_empty() {
            vec![FontFamilyName::Generic(GenericFamily::Monospace)]
        } else {
            self.families
                .iter()
                .map(|family| {
                    FontFamilyName::parse(family)
                        .unwrap_or_else(|| FontFamilyName::named(family.as_str()))
                        .into_owned()
                })
                .collect()
        };

        FontFamily::List(Cow::Owned(families))
    }
}

/// Initial window size for the bundled desktop terminal app.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct WindowConfig {
    pub width: u32,
    pub height: u32,
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            width: 1180,
            height: 760,
        }
    }
}

impl WindowConfig {
    pub fn validate(&self) -> Result<()> {
        if self.width == 0 {
            bail!("window.width must be greater than zero");
        }
        if self.height == 0 {
            bail!("window.height must be greater than zero");
        }
        Ok(())
    }
}

/// PTY and scrollback settings for the bundled desktop terminal app.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct TerminalConfig {
    pub scrollback: usize,
    pub shell: Option<String>,
}

impl Default for TerminalConfig {
    fn default() -> Self {
        Self {
            scrollback: 5_000,
            shell: None,
        }
    }
}

impl TerminalConfig {
    pub fn validate(&self) -> Result<()> {
        if let Some(shell) = &self.shell {
            if shell.trim().is_empty() {
                bail!("terminal.shell cannot be empty");
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::AppConfig;

    #[test]
    fn config_file_overrides_font_settings_and_keeps_defaults() {
        let config: AppConfig = toml::from_str(
            r#"
            [font]
            families = ["JetBrainsMono Nerd Font Mono", "Symbols Nerd Font Mono", "monospace"]
            size = 20.0
            "#,
        )
        .expect("config should parse");

        assert_eq!(
            config.font.families,
            vec![
                "JetBrainsMono Nerd Font Mono".to_owned(),
                "Symbols Nerd Font Mono".to_owned(),
                "monospace".to_owned(),
            ]
        );
        assert_eq!(config.font.size, 20.0);
        assert_eq!(config.font.line_height, 1.25);
        assert_eq!(config.window.width, 1180);
        assert_eq!(config.terminal.scrollback, 5_000);
    }

    #[test]
    fn validation_rejects_empty_font_stack() {
        let config: AppConfig = toml::from_str(
            r#"
            [font]
            families = []
            "#,
        )
        .expect("config should parse");

        assert!(config.validate().is_err());
    }
}
