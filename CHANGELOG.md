# Changelog

## Unreleased

### Changed

- Rendering migrated from Cairo/Pango to pure-Rust [vello_cpu](https://github.com/linebender/vello) (rasterization) and [parley](https://github.com/linebender/parley) (text layout); the only remaining system dependency is fontconfig for font discovery.
- Pango markup is now parsed in-crate. Supported: `b`, `i`, `u`, `s`, `tt`, `big`, `small`, `sub`, `sup`, `span` (font/family/size/style/weight/foreground/alpha/underline/strikethrough attributes). Other `span` attributes (e.g. `background`) are ignored.
- `libposhanka`: `load_icon_surface`/`load_icon_surface_from_raw` are now `load_icon_pixmap`/`load_icon_pixmap_from_raw` and return `vello_cpu::Pixmap`.

### Fixed

- A card shown on an empty screen could vanish right away: when the compositor configured the new surface a second time (sway does, right after mapping it), poshanka answered with a blank buffer, so a lone notification was never visible until the next update. The last frame is now repainted instead.
- Every repaint leaked a Wayland buffer and its shared-memory pool: poshanka dropped them without destroying them, so the compositor kept their memory for as long as poshanka ran (a card's worth for each update). Pools are now destroyed right after use and buffers once the compositor releases them; surfaces the compositor closes are destroyed too.

## [0.1.0] - 2026-09-04
