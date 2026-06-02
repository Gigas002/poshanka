#![allow(dead_code)]

use std::path::{Path, PathBuf};

use serde::Deserialize;

/// Top-level theme file (`theme.toml`).
#[derive(Debug, Clone, Deserialize)]
pub struct Theme {
    pub font: Font,
    pub colors: Colors,
    pub layout: Layout,
    pub border: Border,
    pub text: Text,
    pub icons: Icons,
    pub progress: Progress,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Font {
    pub name: String,
    pub size: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Colors {
    pub background: String,
    pub foreground: String,
    pub border: String,
    pub progress: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Layout {
    pub width: u32,
    /// Maximum card height in pixels.
    pub height: u32,
    pub padding: u32,
    pub margin: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Border {
    pub size: u32,
    pub radius: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Text {
    pub alignment: TextAlignment,
    pub summary: String,
    pub body: String,
    pub app: Option<String>,
    pub id: Option<String>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TextAlignment {
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Icons {
    /// ≤0 disables icon rendering.
    pub size: i32,
    pub position: IconPosition,
    pub theme: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum IconPosition {
    Left,
    Right,
    Top,
    Bottom,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Progress {
    pub mode: ProgressMode,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ProgressMode {
    Over,
    Source,
}

/// Partial theme used in override fragments (all sections optional, colors patch-by-key).
#[derive(Debug, Clone, Deserialize, Default)]
pub struct FragmentTheme {
    pub font: Option<Font>,
    pub colors: Option<FragmentColors>,
    pub layout: Option<Layout>,
    pub border: Option<Border>,
    pub text: Option<Text>,
    pub icons: Option<Icons>,
    pub progress: Option<Progress>,
}

/// Partial color table — each key is optional so urgency fragments can patch
/// only the keys they care about (e.g. `background` + `border` only).
#[derive(Debug, Clone, Deserialize, Default)]
pub struct FragmentColors {
    pub background: Option<String>,
    pub foreground: Option<String>,
    pub border: Option<String>,
    pub progress: Option<String>,
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

impl Theme {
    /// Apply `frag` on top of `self`, returning a new merged [`Theme`].
    ///
    /// Colors are patched key-by-key (only the keys present in `frag.colors` replace
    /// the corresponding base keys).  All other sections are replaced whole if present.
    pub fn apply_fragment(&self, frag: &FragmentTheme) -> Theme {
        Theme {
            font: frag.font.clone().unwrap_or_else(|| self.font.clone()),
            colors: match &frag.colors {
                None => self.colors.clone(),
                Some(fc) => Colors {
                    background: fc
                        .background
                        .clone()
                        .unwrap_or_else(|| self.colors.background.clone()),
                    foreground: fc
                        .foreground
                        .clone()
                        .unwrap_or_else(|| self.colors.foreground.clone()),
                    border: fc
                        .border
                        .clone()
                        .unwrap_or_else(|| self.colors.border.clone()),
                    progress: fc
                        .progress
                        .clone()
                        .unwrap_or_else(|| self.colors.progress.clone()),
                },
            },
            layout: frag.layout.clone().unwrap_or_else(|| self.layout.clone()),
            border: frag.border.clone().unwrap_or_else(|| self.border.clone()),
            text: frag.text.clone().unwrap_or_else(|| self.text.clone()),
            icons: frag.icons.clone().unwrap_or_else(|| self.icons.clone()),
            progress: frag
                .progress
                .clone()
                .unwrap_or_else(|| self.progress.clone()),
        }
    }
}

impl FragmentTheme {
    pub fn load(path: &Path) -> Result<Self, crate::error::Error> {
        let raw = std::fs::read_to_string(path).map_err(|source| crate::error::Error::Io {
            path: path.to_path_buf(),
            source,
        })?;
        toml::from_str(&raw)
            .map_err(|err| crate::error::Error::Theme(format!("{}: {err}", path.display())))
    }
}

/// Resolve a theme path: absolute paths are returned as-is; relative paths are
/// resolved beside the config file, falling back to `…/poshanka/themes/<name>`.
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
