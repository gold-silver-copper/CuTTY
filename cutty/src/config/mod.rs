use std::fmt::{self, Display, Formatter};
use std::path::{Path, PathBuf};
use std::result::Result as StdResult;
use std::{env, fs, io};

use log::{debug, error, info};
use serde::Deserialize;
use toml::de::Error as TomlError;
use toml::{Table, Value};

pub mod bell;
pub mod color;
pub mod cursor;
pub mod debug;
pub mod font;
pub mod general;
pub mod monitor;
pub mod scrolling;
pub mod selection;
pub mod serde_utils;
pub mod terminal;
pub mod ui_config;
pub mod window;

mod bindings;
mod mouse;

use crate::cli::Options;
#[cfg(test)]
pub use crate::config::bindings::Binding;
pub use crate::config::bindings::{
    Action, BindingKey, BindingMode, KeyBinding, MouseAction, MouseEvent, SearchAction, ViAction,
};
pub use crate::config::ui_config::UiConfig;
use crate::logging::LOG_TARGET_CONFIG;

/// Maximum number of depth for the configuration file imports.
pub const IMPORT_RECURSION_LIMIT: usize = 5;

/// Result from config loading.
pub type Result<T> = std::result::Result<T, Error>;

/// Errors occurring during config loading.
#[derive(Debug)]
pub enum Error {
    /// Couldn't read $HOME environment variable.
    ReadingEnvHome(env::VarError),

    /// io error reading file.
    Io(io::Error),

    /// Invalid toml.
    Toml(TomlError),
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::ReadingEnvHome(err) => err.source(),
            Error::Io(err) => err.source(),
            Error::Toml(err) => err.source(),
        }
    }
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Error::ReadingEnvHome(err) => {
                write!(f, "Unable to read $HOME environment variable: {err}")
            },
            Error::Io(err) => write!(f, "Error reading config file: {err}"),
            Error::Toml(err) => write!(f, "Config error: {err}"),
        }
    }
}

impl From<env::VarError> for Error {
    fn from(val: env::VarError) -> Self {
        Error::ReadingEnvHome(val)
    }
}

impl From<io::Error> for Error {
    fn from(val: io::Error) -> Self {
        Error::Io(val)
    }
}

impl From<TomlError> for Error {
    fn from(val: TomlError) -> Self {
        Error::Toml(val)
    }
}

/// Load the configuration file.
pub fn load(options: &mut Options) -> UiConfig {
    let config_path = options.config_file.clone().or_else(|| installed_config("toml"));

    // Load the config using the following fallback behavior:
    //  - Config path + CLI overrides
    //  - CLI overrides
    //  - Default
    let mut config = config_path
        .as_ref()
        .and_then(|config_path| load_from(config_path).ok())
        .unwrap_or_else(|| {
            let mut config = UiConfig::default();
            match config_path {
                Some(config_path) => config.config_paths.push(config_path),
                None => info!(target: LOG_TARGET_CONFIG, "No config file found; using default"),
            }
            config
        });

    after_loading(&mut config, options);

    config
}

/// Attempt to reload the configuration file.
pub fn reload(config_path: &Path, options: &mut Options) -> Result<UiConfig> {
    debug!("Reloading configuration file: {config_path:?}");

    // Load config, propagating errors.
    let mut config = load_from(config_path)?;

    after_loading(&mut config, options);

    Ok(config)
}

/// Modifications after the `UiConfig` object is created.
fn after_loading(config: &mut UiConfig, options: &mut Options) {
    // Override config with CLI options.
    options.override_config(config);
}

/// Load configuration file and log errors.
fn load_from(path: &Path) -> Result<UiConfig> {
    match read_config(path) {
        Ok(config) => Ok(config),
        Err(Error::Io(io)) if io.kind() == io::ErrorKind::NotFound => {
            error!(target: LOG_TARGET_CONFIG, "Unable to load config {path:?}: File not found");
            Err(Error::Io(io))
        },
        Err(err) => {
            error!(target: LOG_TARGET_CONFIG, "Unable to load config {path:?}: {err}");
            Err(err)
        },
    }
}

/// Deserialize configuration file from path.
fn read_config(path: &Path) -> Result<UiConfig> {
    let mut config_paths = Vec::new();
    let config_value = parse_config(path, &mut config_paths, IMPORT_RECURSION_LIMIT)?;

    // Deserialize to concrete type.
    let mut config = UiConfig::deserialize(config_value)?;
    config.config_paths = config_paths;

    Ok(config)
}

/// Deserialize all configuration files as generic Value.
fn parse_config(
    path: &Path,
    config_paths: &mut Vec<PathBuf>,
    recursion_limit: usize,
) -> Result<Value> {
    config_paths.push(path.to_owned());

    // Deserialize the configuration file.
    let config = deserialize_config(path)?;

    // Merge config with imports.
    let imports = load_imports(&config, path, config_paths, recursion_limit);
    Ok(serde_utils::merge(imports, config))
}

/// Deserialize a configuration file.
pub fn deserialize_config(path: &Path) -> Result<Value> {
    let mut contents = fs::read_to_string(path)?;

    // Remove UTF-8 BOM.
    if contents.starts_with('\u{FEFF}') {
        contents = contents.split_off(3);
    }

    deserialize_toml_config(&contents).map_err(Into::into)
}

fn deserialize_toml_config(contents: &str) -> std::result::Result<Value, TomlError> {
    let mut config: Value = toml::from_str(contents)?;
    normalize_legacy_toml(&mut config);
    Ok(config)
}

fn normalize_legacy_toml(config: &mut Value) {
    let Some(table) = config.as_table_mut() else {
        return;
    };

    move_legacy_value(table, "draw_bold_text_with_bright_colors", &[
        "colors",
        "draw_bold_text_with_bright_colors",
    ]);
    move_legacy_value(table, "key_bindings", &["keyboard", "bindings"]);
    move_legacy_value(table, "mouse_bindings", &["mouse", "bindings"]);
    move_legacy_value(table, "live_config_reload", &["general", "live_config_reload"]);
    move_legacy_value(table, "working_directory", &["general", "working_directory"]);
    move_legacy_value(table, "ipc_socket", &["general", "ipc_socket"]);
    move_legacy_value(table, "import", &["general", "import"]);
    move_legacy_value(table, "shell", &["terminal", "shell"]);

    rename_nested_value(table, &["colors", "cursor"], "text", "foreground");
    rename_nested_value(table, &["colors", "cursor"], "cursor", "background");
    rename_nested_value(table, &["colors", "vi_mode_cursor"], "text", "foreground");
    rename_nested_value(table, &["colors", "vi_mode_cursor"], "cursor", "background");
    rename_nested_value(table, &["colors", "selection"], "text", "foreground");
    rename_nested_value(table, &["colors", "selection"], "cursor", "background");
}

fn move_legacy_value(table: &mut Table, origin: &str, target: &[&str]) {
    let Some(value) = table.remove(origin) else {
        return;
    };

    insert_if_missing(table, target, value);
}

fn insert_if_missing(table: &mut Table, path: &[&str], value: Value) {
    let Some((segment, rest)) = path.split_first() else {
        return;
    };

    if rest.is_empty() {
        table.entry((*segment).to_owned()).or_insert(value);
        return;
    }

    let entry = table.entry((*segment).to_owned()).or_insert_with(|| Value::Table(Table::new()));
    let Value::Table(next) = entry else {
        return;
    };

    insert_if_missing(next, rest, value);
}

fn rename_nested_value(table: &mut Table, path: &[&str], origin: &str, target: &str) {
    let Some(target_table) = table_at_mut(table, path) else {
        return;
    };

    let Some(value) = target_table.remove(origin) else {
        return;
    };

    target_table.entry(target.to_owned()).or_insert(value);
}

fn table_at_mut<'a>(table: &'a mut Table, path: &[&str]) -> Option<&'a mut Table> {
    let Some((segment, rest)) = path.split_first() else {
        return Some(table);
    };

    let Value::Table(next) = table.get_mut(*segment)? else {
        return None;
    };

    table_at_mut(next, rest)
}

/// Load all referenced configuration files.
fn load_imports(
    config: &Value,
    base_path: &Path,
    config_paths: &mut Vec<PathBuf>,
    recursion_limit: usize,
) -> Value {
    // Get paths for all imports.
    let import_paths = match imports(config, base_path, recursion_limit) {
        Ok(import_paths) => import_paths,
        Err(err) => {
            error!(target: LOG_TARGET_CONFIG, "{err}");
            return Value::Table(Table::new());
        },
    };

    // Parse configs for all imports recursively.
    let mut merged = Value::Table(Table::new());
    for import_path in import_paths {
        let path = match import_path {
            Ok(path) => path,
            Err(err) => {
                error!(target: LOG_TARGET_CONFIG, "{err}");
                continue;
            },
        };

        match parse_config(&path, config_paths, recursion_limit - 1) {
            Ok(config) => merged = serde_utils::merge(merged, config),
            Err(Error::Io(io)) if io.kind() == io::ErrorKind::NotFound => {
                info!(target: LOG_TARGET_CONFIG, "Config import not found:\n  {:?}", path.display());
                continue;
            },
            Err(err) => {
                error!(target: LOG_TARGET_CONFIG, "Unable to import config {path:?}: {err}")
            },
        }
    }

    merged
}

/// Get all import paths for a configuration.
pub fn imports(
    config: &Value,
    base_path: &Path,
    recursion_limit: usize,
) -> StdResult<Vec<StdResult<PathBuf, String>>, String> {
    let imports =
        config.get("general").and_then(|g| g.get("import")).or_else(|| config.get("import"));
    let imports = match imports {
        Some(Value::Array(imports)) => imports,
        Some(_) => return Err("Invalid import type: expected a sequence".into()),
        None => return Ok(Vec::new()),
    };

    // Limit recursion to prevent infinite loops.
    if !imports.is_empty() && recursion_limit == 0 {
        return Err("Exceeded maximum configuration import depth".into());
    }

    let mut import_paths = Vec::new();

    for import in imports {
        let path = match import {
            Value::String(path) => PathBuf::from(path),
            _ => {
                import_paths.push(Err("Invalid import element type: expected path string".into()));
                continue;
            },
        };

        let normalized = normalize_import(base_path, path);

        import_paths.push(Ok(normalized));
    }

    Ok(import_paths)
}

/// Normalize import paths.
pub fn normalize_import(base_config_path: &Path, import_path: impl Into<PathBuf>) -> PathBuf {
    let mut import_path = import_path.into();

    // Resolve paths relative to user's home directory.
    if let (Ok(stripped), Some(home_dir)) = (import_path.strip_prefix("~/"), home::home_dir()) {
        import_path = home_dir.join(stripped);
    }

    if import_path.is_relative()
        && let Some(base_config_dir) = base_config_path.parent()
    {
        import_path = base_config_dir.join(import_path)
    }

    import_path
}

/// Get the location of the first found default config file paths
/// according to the following order:
///
/// 1. $XDG_CONFIG_HOME/cutty/cutty.toml
/// 2. $XDG_CONFIG_HOME/cutty.toml
/// 3. $HOME/.config/cutty/cutty.toml
/// 4. $HOME/.cutty.toml
/// 5. /etc/cutty/cutty.toml
#[cfg(not(windows))]
pub fn installed_config(suffix: &str) -> Option<PathBuf> {
    let file_name = format!("cutty.{suffix}");

    // Try using XDG location by default.
    xdg::BaseDirectories::with_prefix("cutty")
        .find_config_file(&file_name)
        .or_else(|| xdg::BaseDirectories::new().find_config_file(&file_name))
        .or_else(|| {
            if let Ok(home) = env::var("HOME") {
                // Fallback path: $HOME/.config/cutty/cutty.toml.
                let fallback = PathBuf::from(&home).join(".config/cutty").join(&file_name);
                if fallback.exists() {
                    return Some(fallback);
                }
                // Fallback path: $HOME/.cutty.toml.
                let hidden_name = format!(".{file_name}");
                let fallback = PathBuf::from(&home).join(hidden_name);
                if fallback.exists() {
                    return Some(fallback);
                }
            }

            let fallback = PathBuf::from("/etc/cutty").join(&file_name);
            fallback.exists().then_some(fallback)
        })
}

#[cfg(windows)]
pub fn installed_config(suffix: &str) -> Option<PathBuf> {
    let file_name = format!("cutty.{suffix}");
    dirs::config_dir().map(|path| path.join("cutty").join(file_name)).filter(|new| new.exists())
}

#[cfg(test)]
mod tests {
    use super::*;

    use cutty_terminal::tty::Shell;

    use crate::display::color::CellRgb;

    #[test]
    fn empty_config() {
        toml::from_str::<UiConfig>("").unwrap();
    }

    #[test]
    fn legacy_root_level_imports_are_loaded() {
        let config = deserialize_toml_config(r#"import = ["theme.toml"]"#).unwrap();
        let base = Path::new("/tmp/cutty.toml");

        let import_paths = imports(&config, base, IMPORT_RECURSION_LIMIT).unwrap();
        assert_eq!(import_paths.len(), 1);
        assert_eq!(import_paths[0].as_ref().unwrap(), &PathBuf::from("/tmp/theme.toml"));
    }

    #[test]
    fn general_imports_are_loaded() {
        let config: Value = toml::from_str(
            r#"
            [general]
            import = ["theme.toml"]
            "#,
        )
        .unwrap();
        let base = Path::new("/tmp/cutty.toml");

        let import_paths = imports(&config, base, IMPORT_RECURSION_LIMIT).unwrap();
        assert_eq!(import_paths.len(), 1);
        assert_eq!(import_paths[0].as_ref().unwrap(), &PathBuf::from("/tmp/theme.toml"));
    }

    #[test]
    fn bundled_example_config_is_valid() {
        toml::from_str::<UiConfig>(include_str!("../../../extra/cutty.example.toml")).unwrap();
    }

    #[test]
    fn bundled_daily_config_is_valid() {
        toml::from_str::<UiConfig>(include_str!("../../../extra/cutty.daily.toml")).unwrap();
    }

    #[test]
    fn legacy_alacritty_toml_is_supported() {
        let config = deserialize_toml_config(
            r#"
            shell = "/bin/zsh"
            working_directory = "/tmp/legacy"
            live_config_reload = false
            ipc_socket = false
            draw_bold_text_with_bright_colors = true
            import = ["theme.toml"]
            key_bindings = [
                { key = "Back", chars = "\u007f" },
                { key = "Key0", mods = "Control", action = "ResetFontSize" },
            ]

            [colors.cursor]
            text = "CellBackground"
            cursor = "CellForeground"
            "#,
        )
        .unwrap();
        let config = UiConfig::deserialize(config).unwrap();

        let pty = config.pty_config();
        assert_eq!(pty.shell, Some(Shell::new(String::from("/bin/zsh"), Vec::new())));
        assert_eq!(pty.working_directory, Some(PathBuf::from("/tmp/legacy")));
        assert!(!config.live_config_reload());
        #[cfg(unix)]
        assert!(!config.ipc_socket());
        assert!(config.colors.draw_bold_text_with_bright_colors);
        assert_eq!(config.colors.cursor.foreground, CellRgb::CellBackground);
        assert_eq!(config.colors.cursor.background, CellRgb::CellForeground);
        assert!(config.key_bindings().iter().any(|binding| {
            binding.trigger
                == crate::config::bindings::BindingKey::Keycode {
                    key: winit::keyboard::Key::Named(winit::keyboard::NamedKey::Backspace),
                    location: crate::config::bindings::KeyLocation::Any,
                }
        }));
        assert!(config.key_bindings().iter().any(|binding| {
            binding.action == Action::ResetFontSize
                && binding.trigger
                    == crate::config::bindings::BindingKey::Keycode {
                        key: winit::keyboard::Key::Character("0".into()),
                        location: crate::config::bindings::KeyLocation::Standard,
                    }
        }));
    }
}
