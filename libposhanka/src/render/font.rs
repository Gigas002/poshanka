use pango::prelude::*;
use pango::{FontDescription, Layout};
use pangocairo::FontMap as CairoFontMap;

use crate::error::PoshankaError;

pub struct FontContext {
    layout: Layout,
}

impl FontContext {
    pub fn new(font_name: &str, font_size: f64) -> Result<Self, PoshankaError> {
        let mut desc = FontDescription::from_string(font_name);
        let size = (font_size * f64::from(pango::SCALE)).round() as i32;
        desc.set_size(size);
        let map = CairoFontMap::new();
        let context = map.create_context();
        context.set_font_description(&desc);
        let layout = Layout::new(&context);
        Ok(Self { layout })
    }

    pub fn measure_markup(&self, markup: &str, width_px: f64, max_height_px: Option<f64>) -> (f64, f64) {
        self.layout.set_markup(markup);
        self.layout
            .set_width((width_px * f64::from(pango::SCALE)).round() as i32);
        self.layout.set_wrap(pango::WrapMode::WordChar);
        if let Some(max_h) = max_height_px {
            self.layout
                .set_height((max_h * f64::from(pango::SCALE)).round() as i32);
            self.layout.set_ellipsize(pango::EllipsizeMode::End);
        } else {
            self.layout.set_height(-1);
            self.layout.set_ellipsize(pango::EllipsizeMode::End);
        }
        let (w, h) = self.layout.size();
        (
            f64::from(w) / f64::from(pango::SCALE),
            f64::from(h) / f64::from(pango::SCALE),
        )
    }

    pub fn layout(&self) -> &Layout {
        &self.layout
    }

    pub fn set_alignment(&self, align: pango::Alignment) {
        self.layout.set_alignment(align);
    }
}
