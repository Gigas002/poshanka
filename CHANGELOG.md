# Changelog

## Unreleased

### Changed

- Rendering migrated from Cairo/Pango to pure-Rust [vello_cpu](https://github.com/linebender/vello) (rasterization) and [parley](https://github.com/linebender/parley) (text layout); the only remaining system dependency is fontconfig for font discovery.
- Pango markup is now parsed in-crate. Supported: `b`, `i`, `u`, `s`, `tt`, `big`, `small`, `sub`, `sup`, `span` (font/family/size/style/weight/foreground/alpha/underline/strikethrough attributes). Other `span` attributes (e.g. `background`) are ignored.
- `libposhanka`: `load_icon_surface`/`load_icon_surface_from_raw` are now `load_icon_pixmap`/`load_icon_pixmap_from_raw` and return `vello_cpu::Pixmap`.

## [0.1.0] - 2026-09-04
