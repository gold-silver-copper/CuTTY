use std::{env, fs, path::PathBuf};

use anyhow::{Context, Result, bail};
use cutty::{AppConfig, run_terminal_with_config};
use directories::BaseDirs;

fn main() -> Result<()> {
    run_terminal_with_config(load_config()?)
}

fn load_config() -> Result<AppConfig> {
    let from_env = env::var_os("CUTTY_CONFIG").is_some();
    let Some(path) = config_path() else {
        return Ok(AppConfig::default());
    };

    if !path.exists() {
        if from_env {
            bail!("cutty config file does not exist: {}", path.display());
        }
        return Ok(AppConfig::default());
    }

    let contents = fs::read_to_string(&path)
        .with_context(|| format!("failed to read cutty config at {}", path.display()))?;
    let config: AppConfig = toml::from_str(&contents)
        .with_context(|| format!("failed to parse cutty config at {}", path.display()))?;
    config
        .validate()
        .with_context(|| format!("invalid cutty config at {}", path.display()))?;
    Ok(config)
}

fn config_path() -> Option<PathBuf> {
    if let Some(path) = env::var_os("CUTTY_CONFIG") {
        return Some(PathBuf::from(path));
    }

    Some(
        BaseDirs::new()?
            .config_dir()
            .join("cutty")
            .join("config.toml"),
    )
}
