# poshanka

Wayland popup subscriber

![](examples/preview.png)

## Requirements

- A Wayland compositor with `wlr-layer-shell-unstable-v1`
- [notred](https://github.com/Gigas002/notred) or similar FDN daemon poshanka subscribes to, with its `notredctl` connector CLI on `$PATH`
- `fontconfig` (system font discovery; rendering itself is pure Rust via [vello_cpu](https://github.com/linebender/vello) + [parley](https://github.com/linebender/parley))

## Configuration

poshanka reads config from `$XDG_CONFIG_HOME/poshanka/` (falls back to `~/.config/poshanka/` if `$XDG_CONFIG_HOME` is unset), or from `--config <path>`:

| File               | Role                                                                          |
| ------------------ | ----------------------------------------------------------------------------- |
| `config.toml`      | Placement, stack gap, layer-shell anchor/layer, `[provider]` wiring           |
| `theme.toml`       | Card look — font, colors, layout, Pango-markup templates (path from `[paths].theme`) |
| override fragments | Per-app / per-urgency theme patches (paths from `[paths].overrides`)          |

When notred reloads (`notredctl reload`), poshanka re-reads its theme and override fragments (including the `[paths]` list of them). `[placement]`, `[stack]`, `[layer]` and `[provider]` are read at start-up only; restart poshanka after changing them.

Keys poshanka doesn't know are logged as warnings when the files are read (at start-up and on reload), so a misspelled key shows up in the log instead of silently doing nothing.

Start from the reference files under `examples/`: copy `examples/config.toml`, `examples/theme.toml`, and (optionally) `examples/apps/`, `examples/urgency/`, and `examples/scripts/notred-subscribe.sh` into `$XDG_CONFIG_HOME/poshanka/`, adjusting `[provider].exec` if you relocate the subscribe script.
