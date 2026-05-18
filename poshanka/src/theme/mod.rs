use std::path::{Path, PathBuf};

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct Theme {
    pub base: ThemeBase,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ThemeBase {
    pub background_color: String,
    #[allow(dead_code)]
    pub foreground_color: Option<String>,
}

impl Theme {
    pub fn load(path: &Path) -> Result<Self, crate::error::Error> {
        let raw = std::fs::read_to_string(path).map_err(|source| crate::error::Error::Io {
            path: path.to_path_buf(),
            source,
        })?;
        toml::from_str(&raw)
            .map_err(|err| crate::error::Error::Theme(format!("{}: {err}", path.display())))
    }
}

/// Resolve a theme path relative to the config file's directory or `…/poshanka/themes/`.
pub fn resolve_path(config_path: &Path, theme_name: &str) -> PathBuf {
    let theme_path = Path::new(theme_name);
    if theme_path.is_absolute() {
        return theme_path.to_path_buf();
    }

    if let Some(parent) = config_path.parent() {
        let beside = parent.join(theme_name);
        if beside.is_file() {
            return beside;
        }
    }

    crate::config::config_dir().join("themes").join(theme_name)
}

#[cfg(test)]
mod tests;
