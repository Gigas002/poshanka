#![allow(dead_code)]

use std::path::{Path, PathBuf};

use serde::Deserialize;

/// Top-level config (`config.toml`).
#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub paths: Paths,
    pub provider: ProviderConfig,
    pub stack: Stack,
    pub placement: Placement,
    pub layer: LayerConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Paths {
    pub theme: String,
    #[serde(default)]
    pub overrides: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProviderConfig {
    /// Long-running feed script (abar `[tray].exec` analogue). Prints NDJSON on stdout.
    pub exec: Option<String>,
    /// Optional CLI for one-shot provider commands (`close`, `activate`, `input`, …).
    pub command: Option<String>,
    /// Optional socket path forwarded to the provider CLI by the binary/script.
    pub socket: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Stack {
    pub gap: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Placement {
    pub anchor: String,
    pub margin: u32,
}

/// The `[layer]` table. Field name `layer` inside matches the TOML key.
#[derive(Debug, Clone, Deserialize)]
pub struct LayerConfig {
    pub layer: LayerShell,
    pub output: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum LayerShell {
    Background,
    Bottom,
    Top,
    Overlay,
}

/// Override fragment config (`<fragment>/config.toml`).
#[derive(Debug, Clone, Deserialize)]
pub struct FragmentConfig {
    #[serde(rename = "override")]
    pub override_meta: OverrideMeta,
    pub paths: Option<FragmentPaths>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OverrideMeta {
    #[serde(rename = "type")]
    pub kind: OverrideType,
    /// Required when `kind == App`.
    pub name: Option<String>,
    /// Required when `kind == Urgency`.
    pub level: Option<UrgencyLevel>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum OverrideType {
    App,
    Urgency,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum UrgencyLevel {
    Low,
    Normal,
    Critical,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FragmentPaths {
    pub theme: Option<String>,
    /// Nested urgency overrides; valid only inside an `app` fragment.
    #[serde(default)]
    pub overrides: Vec<String>,
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

impl FragmentConfig {
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
    config_dir_from_env(
        std::env::var_os("XDG_CONFIG_HOME"),
        std::env::var_os("HOME"),
    )
}

/// Pure resolution logic; accepts env values directly so tests don't touch the process env.
pub(crate) fn config_dir_from_env(
    xdg_config_home: Option<impl AsRef<std::ffi::OsStr>>,
    home: Option<impl AsRef<std::ffi::OsStr>>,
) -> PathBuf {
    xdg_config_home
        .map(|s| std::ffi::OsString::from(s.as_ref()))
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .or_else(|| home.map(|h| PathBuf::from(h.as_ref()).join(".config")))
        .unwrap_or_else(|| PathBuf::from(".config"))
        .join("poshanka")
}

pub fn default_config_path() -> PathBuf {
    config_dir().join("config.toml")
}

#[cfg(test)]
mod tests;
