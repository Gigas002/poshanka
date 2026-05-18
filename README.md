# poshanka

Minimalistic wayland-native notification daemon, inspired by dunst.

## Phase 0 (dev)

```sh
cargo build --workspace
cargo test --workspace

# Run against example config (requires Wayland + layer-shell compositor):
poshanka --config examples/config.toml --theme examples/theme.toml
```
