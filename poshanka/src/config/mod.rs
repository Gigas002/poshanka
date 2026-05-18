use std::path::{Path, PathBuf};

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub base: Base,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Base {
    pub font_name: String,
    pub font_size: Option<f64>,
    pub theme: Option<String>,
}

impl Config {
    pub fn load(path: &Path) -> Result<Self, crate::error::Error> {
        let raw = std::fs::read_to_string(path).map_err(|source| crate::error::Error::Io {
            path: path.to_path_buf(),
            source,
        })?;
        toml::from_str(&raw)
            .map_err(|err| crate::error::Error::Config(format!("{}: {err}", path.display())))
    }
}

pub fn config_dir() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .unwrap_or_else(|| PathBuf::from(".config"))
        .join("poshanka")
}

pub fn default_config_path() -> PathBuf {
    config_dir().join("config.toml")
}

#[cfg(test)]
mod tests;
