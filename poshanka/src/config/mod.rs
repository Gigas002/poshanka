#![allow(dead_code)]

use std::path::{Path, PathBuf};

use serde::Deserialize;

/// Top-level config (`config.toml`).
#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub paths: Paths,
    pub stack: Stack,
    pub placement: Placement,
    pub queue: Queue,
    pub timeouts: Timeouts,
    pub layer: LayerConfig,
    pub events: Option<Events>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Paths {
    pub theme: String,
    #[serde(default)]
    pub overrides: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Stack {
    pub max: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Placement {
    pub anchor: String,
    pub gap: u32,
    pub margin: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Queue {
    pub history: bool,
    pub max: u32,
    pub sort: SortBy,
    pub order: SortOrder,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SortBy {
    Time,
    Priority,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SortOrder {
    Asc,
    Desc,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Timeouts {
    pub ignore: bool,
    pub default: u64,
    pub low: u64,
    pub normal: u64,
    pub critical: u64,
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

#[derive(Debug, Clone, Deserialize, Default)]
pub struct Events {
    pub on_button_left: Option<String>,
    pub on_button_middle: Option<String>,
    pub on_button_right: Option<String>,
    pub on_notify: Option<String>,
    pub on_touch: Option<String>,
}

/// Override fragment config (`<fragment>/config.toml`).
#[derive(Debug, Clone, Deserialize)]
pub struct FragmentConfig {
    #[serde(rename = "override")]
    pub override_meta: OverrideMeta,
    pub paths: Option<FragmentPaths>,
    pub events: Option<Events>,
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
