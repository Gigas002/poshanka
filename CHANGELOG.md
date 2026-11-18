# Changelog

## [0.2.0] - Unreleased

### Changed

- Rendering migrated from Cairo/Pango to pure-Rust [vello_cpu](https://github.com/linebender/vello) (rasterization) and [parley](https://github.com/linebender/parley) (text layout); the only remaining system dependency is fontconfig for font discovery.
- Pango markup is now parsed in-crate. Supported: `b`, `i`, `u`, `s`, `tt`, `big`, `small`, `sub`, `sup`, `span` (font/family/size/style/weight/foreground/alpha/underline/strikethrough attributes). Other `span` attributes (e.g. `background`) are ignored.
- `libposhanka`: `load_icon_surface`/`load_icon_surface_from_raw` are now `load_icon_pixmap`/`load_icon_pixmap_from_raw` and return `vello_cpu::Pixmap`.
- `libposhanka`: `SubscriberRun` has a new `stop` field and `run_overlay` a new `stop` parameter: a descriptor that ends the loop when it becomes readable.
- Updated parley to 0.13, vello_cpu to 0.4 and resvg to 0.50; cards render the same.
- `[layout].margin` in the theme now does something (it was read and ignored): transparent space around each card, outside its click area. It adds to `[stack].gap` between cards and to `[placement].margin` at the screen edges, and since it is part of the card's style, app and urgency overrides can set it per card. Existing themes that set it (the example's `10`) now space their cards further apart; set it to `0` for the old layout.

### Fixed

- A card shown on an empty screen could vanish right away: when the compositor configured the new surface a second time (sway does, right after mapping it), poshanka answered with a blank buffer, so a lone notification was never visible until the next update. The last frame is now repainted instead.
- Every repaint leaked a Wayland buffer and its shared-memory pool: poshanka dropped them without destroying them, so the compositor kept their memory for as long as poshanka ran (a card's worth for each update). Pools are now destroyed right after use and buffers once the compositor releases them; surfaces the compositor closes are destroyed too.
- Notifications arriving in quick succession could leave the whole stack blank until the next update: when the stack was resized again before the compositor answered the previous resize, that late answer took the newest frame and, its size not matching, painted a transparent buffer instead. Late answers to older sizes are now skipped.
- The delay before restarting the provider feed script only ever grew (1, 2, 4 … 30 s) for poshanka's whole lifetime, so after a few notred restarts every reconnection took 30 s, and notifications sent in between never showed. It now starts over at 1 s after any run that delivered messages.
- When updates queued up faster than they were painted (64 waiting), the feed gave up and restarted the script instead of waiting, and the abandoned script kept running. The feed now waits for room, and stops its script if it does end early.
- The provider feed script is stopped when poshanka ends, however it ends; it used to keep running (with its `notredctl subscribe`) until its next write failed.
- Placeholders written in a notification's own text were filled in too: a summary containing `{body}` showed the body there, and a body containing `{app_id}` or `{id}` showed those. Templates are now filled in one pass.
- Bodies using the Desktop Notifications markup for links (`<a href>`) or images (`<img>`) were shown as raw text with their tags, because the whole body fell back to plain text. Links now show their text and images their `alt` text.
- Theme colors written as `#RRGGBB`, which `examples/theme.toml` documents, were rejected and poshanka refused to start; they are now read as opaque.
- Unknown keys in `config.toml`, the theme and override fragments (such as `[paths] override` for `overrides`) were silently ignored; each one is now logged as a warning.
- A feed line that fails to parse is logged by its first 200 bytes; a line with image data used to be logged whole, megabytes at a time.
- poshanka exits cleanly on `SIGTERM` and `SIGINT` (`systemctl --user stop`, Ctrl+C): it removes its surfaces, stops the feed script and exits with status 0, instead of being killed mid-frame.

### Security

- Raw `image-data` icons are checked before decoding: 8 bits per sample, 3 channels (4 with alpha), a rowstride of at least one row, and enough bytes for every row. A notification with a one- or two-channel image used to crash poshanka (any app can send one); others were decoded from the wrong bytes. Such icons are now skipped for the next icon source.

## [0.1.0] - 2026-09-04
