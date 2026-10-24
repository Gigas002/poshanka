use std::borrow::Cow;
use std::cell::RefCell;
use std::ops::Range;

use parley::{
    Alignment, AlignmentOptions, FontFamily, FontFamilyName, FontStyle, FontWeight, GenericFamily,
    Layout, LayoutContext, OverflowWrap, StyleProperty, VerticalAlign, WhiteSpaceCollapse,
};

use crate::error::PoshankaError;
use crate::model::TextAlign;

use super::markup::{self, Attr, FontDesc, FontSlant, RichText, Shift, TextBrush};

pub type TextLayout = Layout<TextBrush>;

const ELLIPSIS: &str = "…";

thread_local! {
    /// System font discovery (fontconfig scan) is expensive; do it once per
    /// thread and hand each `FontContext` a clone of the populated collection.
    static SYSTEM_FONTS: parley::FontContext = parley::FontContext::new();
}

/// Default font plus parley's font database and layout scratch space.
///
/// Uses interior mutability so measuring and painting can share one context
/// through `&self`, like the Pango layout it replaces.
pub struct FontContext {
    families: Vec<FontFamilyName<'static>>,
    weight: f32,
    slant: FontSlant,
    size_px: f32,
    fonts: RefCell<parley::FontContext>,
    layouts: RefCell<LayoutContext<TextBrush>>,
}

impl FontContext {
    /// `font_name` is a Pango-style font description (`"Noto Sans"`,
    /// `"Monospace Bold"`, `"Iosevka, Symbols Nerd Font"`); `font_size` is in
    /// points and overrides any size in the description.
    pub fn new(font_name: &str, font_size: f64) -> Result<Self, PoshankaError> {
        let desc = FontDesc::parse(font_name);
        Ok(Self {
            families: family_list(desc.family.as_deref().unwrap_or("sans-serif")),
            weight: desc.weight.unwrap_or(FontWeight::NORMAL.value()),
            slant: desc.slant.unwrap_or(FontSlant::Normal),
            size_px: font_size as f32 * markup::PT_TO_PX,
            fonts: RefCell::new(SYSTEM_FONTS.with(Clone::clone)),
            layouts: RefCell::new(LayoutContext::new()),
        })
    }

    pub fn measure_markup(
        &self,
        markup: &str,
        width_px: f64,
        max_height_px: Option<f64>,
    ) -> (f64, f64) {
        let layout = self.layout_markup(markup, width_px, max_height_px, &TextAlign::Left);
        (f64::from(layout.width()), f64::from(layout.height()))
    }

    /// Lay out `markup` wrapped to `width_px`. When `max_height_px` is set,
    /// lines past it are dropped and the last kept line ends in an ellipsis
    /// (at least one line is always kept), matching Pango's
    /// `EllipsizeMode::End` with a positive layout height.
    pub fn layout_markup(
        &self,
        markup: &str,
        width_px: f64,
        max_height_px: Option<f64>,
        align: &TextAlign,
    ) -> TextLayout {
        let rich = markup::parse(markup, self.size_px).unwrap_or_else(|e| {
            tracing::debug!(error = %e, "invalid markup; rendering as plain text");
            RichText {
                text: markup.to_string(),
                spans: Vec::new(),
            }
        });
        let width = width_px.max(1.0) as f32;
        let layout = self.build(&rich, width, align);

        let Some(max_h) = max_height_px else {
            return layout;
        };
        let max_h = max_h as f32 + 0.01;
        let mut fitting = 0;
        let mut used = 0.0;
        for line in layout.lines() {
            used += line.metrics().line_height;
            if used > max_h {
                break;
            }
            fitting += 1;
        }
        let keep = fitting.max(1);
        if keep >= layout.len() {
            return layout;
        }

        // Binary-search the longest prefix (ending inside the last kept line)
        // that still fits in `keep` lines once the ellipsis is appended.
        let last = layout.get(keep - 1).map_or(0..0, |l| l.text_range());
        let bounds: Vec<usize> = (last.start..=last.end)
            .filter(|i| rich.text.is_char_boundary(*i))
            .collect();
        let (mut lo, mut hi) = (0, bounds.len() - 1);
        let mut best = None;
        while lo <= hi {
            let mid = (lo + hi) / 2;
            let candidate = self.build(&truncate(&rich, bounds[mid]), width, align);
            if candidate.len() <= keep {
                best = Some(candidate);
                lo = mid + 1;
            } else if mid == 0 {
                break;
            } else {
                hi = mid - 1;
            }
        }
        best.unwrap_or_else(|| self.build(&truncate(&rich, last.start), width, align))
    }

    fn build(&self, rich: &RichText, width: f32, align: &TextAlign) -> TextLayout {
        let mut fonts = self.fonts.borrow_mut();
        let mut layouts = self.layouts.borrow_mut();
        let mut builder = layouts.ranged_builder(&mut fonts, &rich.text, 1.0, true);

        builder.push_default(StyleProperty::FontFamily(FontFamily::List(Cow::Borrowed(
            &self.families,
        ))));
        builder.push_default(StyleProperty::FontSize(self.size_px));
        builder.push_default(StyleProperty::FontWeight(FontWeight::new(self.weight)));
        builder.push_default(StyleProperty::FontStyle(font_style(self.slant)));
        builder.push_default(StyleProperty::Brush(TextBrush::default()));
        builder.push_default(StyleProperty::WhiteSpaceCollapse(
            WhiteSpaceCollapse::Preserve,
        ));
        builder.push_default(StyleProperty::OverflowWrap(OverflowWrap::Anywhere));

        // Owned family lists must outlive the builder's borrowed properties.
        let span_families: Vec<Option<Vec<FontFamilyName<'static>>>> = rich
            .spans
            .iter()
            .map(|span| match &span.attr {
                Attr::Family(name) => Some(family_list(name)),
                _ => None,
            })
            .collect();

        for (span, families) in rich.spans.iter().zip(&span_families) {
            if span.range.is_empty() {
                continue;
            }
            let range = span.range.clone();
            let prop = match (&span.attr, families) {
                (Attr::Family(_), Some(list)) => {
                    StyleProperty::FontFamily(FontFamily::List(Cow::Borrowed(list)))
                }
                (Attr::Family(_), None) => continue,
                (Attr::Size(px), _) => StyleProperty::FontSize(*px),
                (Attr::Weight(w), _) => StyleProperty::FontWeight(FontWeight::new(*w)),
                (Attr::Slant(slant), _) => StyleProperty::FontStyle(font_style(*slant)),
                (Attr::Brush(brush), _) => StyleProperty::Brush(*brush),
                (Attr::Underline(on), _) => StyleProperty::Underline(*on),
                (Attr::UnderlineBrush(brush), _) => StyleProperty::UnderlineBrush(Some(*brush)),
                (Attr::Strikethrough(on), _) => StyleProperty::Strikethrough(*on),
                (Attr::StrikethroughBrush(brush), _) => {
                    StyleProperty::StrikethroughBrush(Some(*brush))
                }
                (Attr::Shift(Shift::Sub), _) => StyleProperty::VerticalAlign(VerticalAlign::SUB),
                (Attr::Shift(Shift::Super), _) => {
                    StyleProperty::VerticalAlign(VerticalAlign::SUPER)
                }
            };
            builder.push(prop, range);
        }

        let mut layout = builder.build(&rich.text);
        layout.break_all_lines(Some(width));
        layout.align(text_alignment(align), AlignmentOptions::default());
        layout
    }
}

/// Cut `rich` at byte `end` (trailing whitespace dropped) and append an
/// ellipsis that inherits the style of the text right before the cut.
fn truncate(rich: &RichText, end: usize) -> RichText {
    let end = rich.text[..end].trim_end().len();
    let mut text = rich.text[..end].to_string();
    text.push_str(ELLIPSIS);
    let new_end = text.len();

    let spans = rich
        .spans
        .iter()
        .filter(|span| span.range.start < end || span.range.start == span.range.end)
        .map(|span| {
            let mut span = span.clone();
            span.range = clip(&span.range, end, new_end);
            span
        })
        .collect();
    RichText { text, spans }
}

fn clip(range: &Range<usize>, end: usize, new_end: usize) -> Range<usize> {
    let start = range.start.min(end);
    if range.end >= end && range.start < end {
        start..new_end
    } else {
        start..range.end.min(end)
    }
}

/// Split a Pango-style comma-separated family list, mapping fontconfig's
/// generic aliases to CSS generic families and ending with `sans-serif` as a
/// last resort.
fn family_list(families: &str) -> Vec<FontFamilyName<'static>> {
    let mut list: Vec<FontFamilyName<'static>> = families
        .split(',')
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(|name| match name.to_ascii_lowercase().as_str() {
            "sans" | "sans-serif" => GenericFamily::SansSerif.into(),
            "serif" => GenericFamily::Serif.into(),
            "mono" | "monospace" => GenericFamily::Monospace.into(),
            "system-ui" => GenericFamily::SystemUi.into(),
            "emoji" => GenericFamily::Emoji.into(),
            _ => FontFamilyName::Named(Cow::Owned(name.to_string())),
        })
        .collect();
    let fallback: FontFamilyName<'static> = GenericFamily::SansSerif.into();
    if !list.contains(&fallback) {
        list.push(fallback);
    }
    list
}

fn font_style(slant: FontSlant) -> FontStyle {
    match slant {
        FontSlant::Normal => FontStyle::Normal,
        FontSlant::Italic => FontStyle::Italic,
        FontSlant::Oblique => FontStyle::Oblique(None),
    }
}

fn text_alignment(align: &TextAlign) -> Alignment {
    match align {
        TextAlign::Left => Alignment::Left,
        TextAlign::Center => Alignment::Center,
        TextAlign::Right => Alignment::Right,
    }
}
