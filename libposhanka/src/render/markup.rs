//! Parser for the subset of [Pango markup] used by notification templates and
//! `body_markup` bodies, producing plain text plus styled byte ranges that
//! [`super::font::FontContext`] feeds into a parley layout.
//!
//! Supported tags: `b`, `i`, `u`, `s`, `tt`, `big`, `small`, `sub`, `sup`,
//! `span` and the optional `markup` root. Supported `span` attributes:
//! `font`/`font_desc`, `font_family`/`face`, `font_size`/`size`,
//! `font_style`/`style`, `font_weight`/`weight`, `foreground`/`fgcolor`/`color`,
//! `alpha`/`fgalpha`, `underline`, `underline_color`, `strikethrough` and
//! `strikethrough_color`. Unknown attributes are ignored; unknown tags,
//! unbalanced tags and malformed entities are errors, like in Pango.
//!
//! [Pango markup]: https://docs.gtk.org/Pango/pango_markup.html

use std::ops::Range;

/// Pango sizes are in points at 96 DPI (`pangocairo`'s default resolution).
pub const PT_TO_PX: f32 = 96.0 / 72.0;

/// Pango's `PANGO_SCALE_LARGE` — the step used by `big`/`small`/`larger`/`smaller`.
const SCALE_STEP: f32 = 1.2;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkupError(pub String);

impl std::fmt::Display for MarkupError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Straight-alpha RGBA override for glyphs; `None` channels fall back to the
/// card's foreground color at paint time.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct TextBrush {
    pub rgb: Option<[u8; 3]>,
    pub alpha: Option<u8>,
}

impl TextBrush {
    pub fn resolve(self, foreground_rgba: [u8; 4]) -> [u8; 4] {
        let [r, g, b] =
            self.rgb
                .unwrap_or([foreground_rgba[0], foreground_rgba[1], foreground_rgba[2]]);
        [r, g, b, self.alpha.unwrap_or(foreground_rgba[3])]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontSlant {
    Normal,
    Italic,
    Oblique,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shift {
    Sub,
    Super,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Attr {
    Family(String),
    /// Absolute size in pixels.
    Size(f32),
    Weight(f32),
    Slant(FontSlant),
    Brush(TextBrush),
    Underline(bool),
    UnderlineBrush(TextBrush),
    Strikethrough(bool),
    StrikethroughBrush(TextBrush),
    Shift(Shift),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Span {
    pub range: Range<usize>,
    pub attr: Attr,
}

/// Parsed markup: plain text plus attribute spans in document (open-tag)
/// order, so later (inner) spans override earlier (outer) ones.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RichText {
    pub text: String,
    pub spans: Vec<Span>,
}

/// Parsed Pango font description (`"Noto Sans Bold Italic 12"`).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct FontDesc {
    pub family: Option<String>,
    pub weight: Option<f32>,
    pub slant: Option<FontSlant>,
    pub size_pt: Option<f32>,
}

impl FontDesc {
    /// Parse a Pango font description string: an optional comma-separated
    /// family list, followed by optional style words and an optional size.
    pub fn parse(desc: &str) -> Self {
        let mut out = Self::default();
        let mut words: Vec<&str> = desc.split_whitespace().collect();

        if let Some(last) = words.last() {
            let num = last.strip_suffix("px").unwrap_or(last);
            if let Ok(size) = num.parse::<f32>()
                && size > 0.0
            {
                out.size_pt = Some(if last.ends_with("px") {
                    size / PT_TO_PX
                } else {
                    size
                });
                words.pop();
            }
        }

        while let Some(last) = words.last() {
            let lower = last.to_ascii_lowercase();
            if let Some(weight) = parse_weight(&lower) {
                out.weight.get_or_insert(weight);
            } else if let Some(slant) = parse_slant(&lower) {
                out.slant.get_or_insert(slant);
            } else if !matches!(
                lower.as_str(),
                "regular" | "roman" | "condensed" | "expanded" | "small-caps"
            ) {
                break;
            }
            words.pop();
        }

        let family = words.join(" ");
        let family = family.trim().trim_end_matches(',').trim();
        if !family.is_empty() {
            out.family = Some(family.to_string());
        }
        out
    }
}

/// Check that `markup` is well-formed without keeping the result.
pub fn validate(markup: &str) -> Result<(), MarkupError> {
    parse(markup, 1.0).map(|_| ())
}

/// Parse `markup` into text and spans. `base_px` is the layout's default font
/// size, needed to resolve relative sizes (`big`, `smaller`, `150%`, …).
pub fn parse(markup: &str, base_px: f32) -> Result<RichText, MarkupError> {
    let mut parser = Parser {
        src: markup,
        pos: 0,
        out: RichText::default(),
        stack: Vec::new(),
        base_px,
    };
    parser.run()?;
    Ok(parser.out)
}

struct Open {
    name: String,
    /// Indices into `out.spans` whose ranges end at this tag's close.
    spans: Vec<usize>,
    size_px: f32,
    brush: TextBrush,
}

struct Parser<'a> {
    src: &'a str,
    pos: usize,
    out: RichText,
    stack: Vec<Open>,
    base_px: f32,
}

impl Parser<'_> {
    fn run(&mut self) -> Result<(), MarkupError> {
        while self.pos < self.src.len() {
            let rest = &self.src[self.pos..];
            if let Some(comment) = rest.strip_prefix("<!--") {
                let end = comment
                    .find("-->")
                    .ok_or_else(|| err("unterminated comment"))?;
                self.pos += 4 + end + 3;
            } else if let Some(close) = rest.strip_prefix("</") {
                let end = close.find('>').ok_or_else(|| err("unterminated tag"))?;
                let name = close[..end].trim();
                self.pos += 2 + end + 1;
                self.close(name)?;
            } else if rest.starts_with('<') {
                self.open_tag()?;
            } else if rest.starts_with('&') {
                let end = rest.find(';').ok_or_else(|| err("unterminated entity"))?;
                let ch = decode_entity(&rest[1..end])?;
                self.out.text.push(ch);
                self.pos += end + 1;
            } else {
                let end = rest.find(['<', '&']).unwrap_or(rest.len());
                self.out.text.push_str(&rest[..end]);
                self.pos += end;
            }
        }
        match self.stack.last() {
            Some(open) => Err(err(&format!("unclosed <{}>", open.name))),
            None => Ok(()),
        }
    }

    fn open_tag(&mut self) -> Result<(), MarkupError> {
        let body_start = self.pos + 1;
        let rest = &self.src[body_start..];
        let name_len = rest
            .find(|c: char| c.is_whitespace() || c == '>' || c == '/')
            .ok_or_else(|| err("unterminated tag"))?;
        let name = rest[..name_len].to_string();
        if name.is_empty() {
            return Err(err("empty tag name"));
        }

        let mut i = name_len;
        let mut attrs = Vec::new();
        let self_closing = loop {
            let tail = &rest[i..];
            let trimmed = tail.trim_start();
            i += tail.len() - trimmed.len();
            if trimmed.starts_with("/>") {
                i += 2;
                break true;
            }
            if trimmed.starts_with('>') {
                i += 1;
                break false;
            }
            let eq = trimmed
                .find('=')
                .ok_or_else(|| err("malformed attribute"))?;
            let key = trimmed[..eq].trim().to_string();
            let after = trimmed[eq + 1..].trim_start();
            let quote = after
                .chars()
                .next()
                .filter(|c| *c == '"' || *c == '\'')
                .ok_or_else(|| err("unquoted attribute value"))?;
            let close = after[1..]
                .find(quote)
                .ok_or_else(|| err("unterminated attribute value"))?;
            let value = decode_entities(&after[1..1 + close])?;
            i += (trimmed.len() - after.len()) + 1 + close + 1;
            attrs.push((key, value));
        };
        self.pos = body_start + i;

        self.push_open(name.clone(), &attrs)?;
        if self_closing {
            self.close(&name)?;
        }
        Ok(())
    }

    fn push_open(&mut self, name: String, attrs: &[(String, String)]) -> Result<(), MarkupError> {
        let (parent_px, parent_brush) = self
            .stack
            .last()
            .map_or((self.base_px, TextBrush::default()), |o| {
                (o.size_px, o.brush)
            });
        let mut open = Open {
            name,
            spans: Vec::new(),
            size_px: parent_px,
            brush: parent_brush,
        };

        match open.name.as_str() {
            "markup" => {}
            "b" => self.add(&mut open, Attr::Weight(700.0)),
            "i" => self.add(&mut open, Attr::Slant(FontSlant::Italic)),
            "u" => self.add(&mut open, Attr::Underline(true)),
            "s" => self.add(&mut open, Attr::Strikethrough(true)),
            "tt" => self.add(&mut open, Attr::Family("monospace".into())),
            "big" => self.set_size(&mut open, parent_px * SCALE_STEP),
            "small" => self.set_size(&mut open, parent_px / SCALE_STEP),
            "sub" | "sup" => {
                let shift = if open.name == "sub" {
                    Shift::Sub
                } else {
                    Shift::Super
                };
                self.add(&mut open, Attr::Shift(shift));
                self.set_size(&mut open, parent_px / SCALE_STEP);
            }
            "span" => self.span_attrs(&mut open, attrs),
            other => return Err(err(&format!("unknown tag <{other}>"))),
        }

        self.stack.push(open);
        Ok(())
    }

    fn span_attrs(&mut self, open: &mut Open, attrs: &[(String, String)]) {
        let parent_px = open.size_px;
        let mut brush = open.brush;
        let mut brush_set = false;

        for (key, value) in attrs {
            let lower = value.trim().to_ascii_lowercase();
            match key.as_str() {
                "font" | "font_desc" => {
                    let desc = FontDesc::parse(value);
                    if let Some(family) = desc.family {
                        self.add(open, Attr::Family(family));
                    }
                    if let Some(weight) = desc.weight {
                        self.add(open, Attr::Weight(weight));
                    }
                    if let Some(slant) = desc.slant {
                        self.add(open, Attr::Slant(slant));
                    }
                    if let Some(pt) = desc.size_pt {
                        self.set_size(open, pt * PT_TO_PX);
                    }
                }
                "font_family" | "face" => self.add(open, Attr::Family(value.clone())),
                "font_size" | "size" => {
                    if let Some(px) = parse_size(&lower, self.base_px, parent_px) {
                        self.set_size(open, px);
                    }
                }
                "font_style" | "style" => {
                    if let Some(slant) = parse_slant(&lower) {
                        self.add(open, Attr::Slant(slant));
                    }
                }
                "font_weight" | "weight" => {
                    if let Some(weight) = parse_weight(&lower) {
                        self.add(open, Attr::Weight(weight));
                    }
                }
                "foreground" | "fgcolor" | "color" => {
                    if let Some([r, g, b, a]) = parse_color(&lower) {
                        brush.rgb = Some([r, g, b]);
                        if a != 255 {
                            brush.alpha = Some(a);
                        }
                        brush_set = true;
                    }
                }
                "alpha" | "fgalpha" => {
                    if let Some(a) = parse_alpha(&lower) {
                        brush.alpha = Some(a);
                        brush_set = true;
                    }
                }
                "underline" => self.add(
                    open,
                    Attr::Underline(!matches!(lower.as_str(), "none" | "false")),
                ),
                "strikethrough" => self.add(open, Attr::Strikethrough(lower == "true")),
                "underline_color" => {
                    if let Some([r, g, b, a]) = parse_color(&lower) {
                        let deco = TextBrush {
                            rgb: Some([r, g, b]),
                            alpha: Some(a),
                        };
                        self.add(open, Attr::UnderlineBrush(deco));
                    }
                }
                "strikethrough_color" => {
                    if let Some([r, g, b, a]) = parse_color(&lower) {
                        let deco = TextBrush {
                            rgb: Some([r, g, b]),
                            alpha: Some(a),
                        };
                        self.add(open, Attr::StrikethroughBrush(deco));
                    }
                }
                _ => tracing::trace!(attribute = %key, "ignoring unsupported span attribute"),
            }
        }

        if brush_set {
            open.brush = brush;
            self.add(open, Attr::Brush(brush));
        }
    }

    fn set_size(&mut self, open: &mut Open, px: f32) {
        open.size_px = px;
        self.add(open, Attr::Size(px));
    }

    fn add(&mut self, open: &mut Open, attr: Attr) {
        let start = self.out.text.len();
        open.spans.push(self.out.spans.len());
        self.out.spans.push(Span {
            range: start..start,
            attr,
        });
    }

    fn close(&mut self, name: &str) -> Result<(), MarkupError> {
        let open = self
            .stack
            .pop()
            .ok_or_else(|| err(&format!("unexpected </{name}>")))?;
        if open.name != name {
            return Err(err(&format!("<{}> closed by </{name}>", open.name)));
        }
        let end = self.out.text.len();
        for idx in open.spans {
            self.out.spans[idx].range.end = end;
        }
        Ok(())
    }
}

fn err(msg: &str) -> MarkupError {
    MarkupError(msg.to_string())
}

fn decode_entities(s: &str) -> Result<String, MarkupError> {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        let tail = &rest[amp + 1..];
        let end = tail.find(';').ok_or_else(|| err("unterminated entity"))?;
        out.push(decode_entity(&tail[..end])?);
        rest = &tail[end + 1..];
    }
    out.push_str(rest);
    Ok(out)
}

fn decode_entity(name: &str) -> Result<char, MarkupError> {
    let ch = match name {
        "amp" => Some('&'),
        "lt" => Some('<'),
        "gt" => Some('>'),
        "quot" => Some('"'),
        "apos" => Some('\''),
        _ => name.strip_prefix('#').and_then(|num| {
            let code = match num.strip_prefix(['x', 'X']) {
                Some(hex) => u32::from_str_radix(hex, 16).ok(),
                None => num.parse().ok(),
            };
            code.and_then(char::from_u32)
        }),
    };
    ch.ok_or_else(|| err(&format!("unknown entity &{name};")))
}

fn parse_weight(s: &str) -> Option<f32> {
    let weight = match s {
        "thin" => 100.0,
        "ultralight" | "extralight" => 200.0,
        "light" => 300.0,
        "semilight" => 350.0,
        "book" => 380.0,
        "normal" => 400.0,
        "medium" => 500.0,
        "semibold" | "demibold" => 600.0,
        "bold" => 700.0,
        "ultrabold" | "extrabold" => 800.0,
        "heavy" | "black" => 900.0,
        "ultraheavy" => 1000.0,
        _ => s
            .parse()
            .ok()
            .filter(|w: &f32| (1.0..=1000.0).contains(w))?,
    };
    Some(weight)
}

fn parse_slant(s: &str) -> Option<FontSlant> {
    match s {
        "normal" => Some(FontSlant::Normal),
        "italic" => Some(FontSlant::Italic),
        "oblique" => Some(FontSlant::Oblique),
        _ => None,
    }
}

/// Resolve a Pango `size` value to pixels. Absolute keywords scale the
/// layout's base size (as Pango does); `smaller`/`larger`/`N%` scale the
/// enclosing span's size; bare numbers are in Pango units (1/1024 pt).
fn parse_size(s: &str, base_px: f32, parent_px: f32) -> Option<f32> {
    let keyword = match s {
        "xx-small" => Some(base_px / SCALE_STEP.powi(3)),
        "x-small" => Some(base_px / SCALE_STEP.powi(2)),
        "small" => Some(base_px / SCALE_STEP),
        "medium" => Some(base_px),
        "large" => Some(base_px * SCALE_STEP),
        "x-large" => Some(base_px * SCALE_STEP.powi(2)),
        "xx-large" => Some(base_px * SCALE_STEP.powi(3)),
        "smaller" => Some(parent_px / SCALE_STEP),
        "larger" => Some(parent_px * SCALE_STEP),
        _ => None,
    };
    if keyword.is_some() {
        return keyword;
    }

    let px = if let Some(pct) = s.strip_suffix('%') {
        parent_px * pct.trim().parse::<f32>().ok()? / 100.0
    } else if let Some(pt) = s.strip_suffix("pt") {
        pt.trim().parse::<f32>().ok()? * PT_TO_PX
    } else if let Some(px) = s.strip_suffix("px") {
        px.trim().parse::<f32>().ok()?
    } else {
        s.parse::<f32>().ok()? / 1024.0 * PT_TO_PX
    };
    (px > 0.0).then_some(px)
}

/// `"50%"` or a 16-bit value (`0..=65535`), as Pango accepts for `alpha`.
fn parse_alpha(s: &str) -> Option<u8> {
    if let Some(pct) = s.strip_suffix('%') {
        let pct: f32 = pct.trim().parse().ok()?;
        return Some((pct.clamp(0.0, 100.0) * 2.55).round() as u8);
    }
    let raw: u32 = s.parse().ok()?;
    Some((raw.min(65535) / 257) as u8)
}

/// `#rgb`, `#rgba`, `#rrggbb`, `#rrggbbaa`, `#rrrrggggbbbb` or a basic color
/// name → straight RGBA.
fn parse_color(s: &str) -> Option<[u8; 4]> {
    if let Some(hex) = s.strip_prefix('#') {
        if !hex.chars().all(|c| c.is_ascii_hexdigit()) {
            return None;
        }
        let channel = |i: usize, width: usize| -> Option<u8> {
            let v = u32::from_str_radix(hex.get(i * width..(i + 1) * width)?, 16).ok()?;
            Some(match width {
                1 => (v * 17) as u8,
                2 => v as u8,
                _ => (v >> 8) as u8,
            })
        };
        return match hex.len() {
            3 => Some([channel(0, 1)?, channel(1, 1)?, channel(2, 1)?, 255]),
            4 => Some([
                channel(0, 1)?,
                channel(1, 1)?,
                channel(2, 1)?,
                channel(3, 1)?,
            ]),
            6 => Some([channel(0, 2)?, channel(1, 2)?, channel(2, 2)?, 255]),
            8 => Some([
                channel(0, 2)?,
                channel(1, 2)?,
                channel(2, 2)?,
                channel(3, 2)?,
            ]),
            12 => Some([channel(0, 4)?, channel(1, 4)?, channel(2, 4)?, 255]),
            _ => None,
        };
    }

    let rgb = match s {
        "black" => [0, 0, 0],
        "white" => [255, 255, 255],
        "red" => [255, 0, 0],
        "green" => [0, 255, 0],
        "blue" => [0, 0, 255],
        "yellow" => [255, 255, 0],
        "cyan" => [0, 255, 255],
        "magenta" => [255, 0, 255],
        "gray" | "grey" => [190, 190, 190],
        "orange" => [255, 165, 0],
        "purple" => [160, 32, 240],
        "brown" => [165, 42, 42],
        "pink" => [255, 192, 203],
        _ => return None,
    };
    Some([rgb[0], rgb[1], rgb[2], 255])
}
