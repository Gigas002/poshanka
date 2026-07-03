# poshanka

Wayland popup subscriber for [notred](https://github.com/Gigas002/notred) — paints corner notification cards from `notredctl` state.

**poshanka does not own `org.freedesktop.Notifications`.** [notred](https://github.com/Gigas002/notred) is the session host (FDN, queue, timeouts, `[events]`). poshanka is an external subscriber that renders cards and forwards user input via **`notredctl`** only.

## Two-process setup

Run both processes in your graphical session:

1. **`notred`** — notification daemon (FDN + queue). Install and configure from the [notred](https://github.com/Gigas002/notred) repo (`$XDG_CONFIG_HOME/notred/notred.toml`).
2. **`poshanka`** — Wayland layer-shell subscriber. Reads `$XDG_CONFIG_HOME/poshanka/config.toml` and theme files; subscribes to notification state via `notredctl`.

Control plane for operators: **`notredctl`** (`list`, `close`, `activate`, `reload`, …) — there is no poshanka-specific ctl binary.

## Development

```sh
cargo build --workspace
cargo test --workspace

# Run against example config (requires Wayland + layer-shell compositor + notred running):
poshanka --config examples/config.toml
```

Example subscribe wrapper (abar `tray.sh` pattern): `examples/scripts/notred-subscribe.sh`.
