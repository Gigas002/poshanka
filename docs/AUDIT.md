# Audit

A whole-application audit of poshanka and libposhanka (2026-11-17) in five
parts: bugs (§2), performance (§3), security and robustness (§4), usability
(§5), and code quality, tests and project hygiene (§6). §7 lists what was left
open, and §8 is the checklist to re-run the audit.

Each finding says what was found, how serious it is, and what was done:
**fixed** (with the commit), **accepted** (with the reason), or **OK**
(checked, nothing to do). Every fix is its own commit, with a regression test
where one can be written.

Severity: **high** (cards missing or a crash in normal use), **medium**
(wrong behaviour in a reachable case, an ignored or rejected setting), **low**
(hardening, polish, or needs an unusual setup).

## 1. Method

Every source file was read in full. Each finding was then reproduced with the
release build in a private headless sway (`WLR_BACKENDS=headless`, its own
runtime and config directories), so the desktop's compositor and its poshanka
were never touched. Notifications came from a FIFO-backed feed script, clicks
from sway's virtual pointer, provider calls were recorded by a stand-in
`notredctl`, the protocol was read with `WAYLAND_DEBUG=1`, and the screen with
`grim`. notred's subscribe output (as fixed in its own audit) was the reference
for the feed format.

Builds checked: default (`icons`), `--no-default-features`, `--all-features`.
Static checks: `cargo clippy` at the three gate levels, `cargo deny check`,
`typos`, `cargo llvm-cov`.

## 2. Bugs

| Area | Finding | Status |
| ---- | ------- | ------ |
| Lone card invisible | sway configures a new layer surface twice; the second configure found no pending frame and painted a transparent buffer. The first notification after an empty screen never showed (until an update of the same size). | Fixed, high (9834fa6) |
| Burst blanks the stack | When two updates resized the stack before sway answered the first, the late configure took the newest frame, found the size wrong and painted a transparent buffer; the whole stack stayed blank until the next update. sway never clamps the size; it only answers late. | Fixed, high (6ca0a00) |
| Buffer leak | Each repaint dropped the previous `wl_buffer` and `wl_shm_pool` proxies without destroying them (wayland-client doesn't on drop): 20 updates, 20 of each alive in the compositor with their memory mapped, for poshanka's lifetime. Closed surfaces were forgotten, not destroyed. | Fixed, high (f29a348) |
| Feed restart delay | The restart delay doubled after every exit of the feed script and never reset: after five notred restarts every reconnection took 30 s (runs measured at 0, 4, 9, 16, 27, 46 s). Now 1 s after any run that delivered. | Fixed, medium (49f6195) |
| Full feed channel | 64 queued updates ended the run (`try_send`) and left the script running unread. The feed now waits for room and kills its script on error. | Fixed, low (faf79f5) |
| Template placeholders | `{body}`, `{id}` etc. inside a notification's own text were filled in too: "type {body}" showed the body. | Fixed, low (601591e) |
| `<a>`/`<img>` bodies | notred sends body markup by default, and the spec allows `<a href>` and `<img>`; the parser rejected them, so the body was shown escaped with tags and URLs. | Fixed, medium (64d50ff) |
| `#RRGGBB` colors | Documented in `examples/theme.toml` but rejected: poshanka refused to start. | Fixed, medium (245a2a7) |
| Unknown config keys | Ignored silently in config, theme and fragments. Now logged as warnings, at start-up and on reload. | Fixed, medium (2497bad) |
| No shutdown | `SIGTERM`/`SIGINT` killed poshanka mid-frame. Now surfaces are destroyed and it exits 0. | Fixed, low (329abc7) |
| Clicks | Left click closes, or activates with actions; right/middle report input; clicks between cards and on the right card of a bottom stack checked. | OK |
| Overrides, reload, output filter | Urgency overrides, theme reload (a broken reload keeps the old theme), `[layer].output` matching. | OK |
| notred 0.2.0 | Run end to end against notred after its audit (private bus, real `notredctl subscribe`): links in bodies, clicks on cards without actions, with only a named action, with a `default` action, and right clicks all behave; its new `resident` field is ignored. The README now describes clicks. | OK, docs (03d8185) |

## 3. Performance

| Area | Finding | Status |
| ---- | ------- | ------ |
| Avatar-heavy updates | notred re-sends every icon with each update (see its audit). With 20 × 256 px avatars poshanka took 50 ms CPU for one 7 MB update and 425 ms (170 ms CPU) for the whole 70 MB stream of 20 updates; queued updates are coalesced into one repaint. Fine as is; see §7. | Accepted |
| Huge text | `<span size="2000pt">` and 30 nested `<big>` in a body cost no measurable CPU: layout is limited to the card. No cap needed. | OK |
| Idle | No timers or polling when nothing changes; RSS 6.5 MB (your running instance after 27 h). | OK |

## 4. Security and robustness

| Area | Finding | Status |
| ---- | ------- | ------ |
| `image-data` decoding | A one-channel image indexed past the pixel buffer and panicked the Wayland thread: one notification from any app on the bus ended poshanka (Hyprland's `exec-once` doesn't restart it). Layouts are now checked first; notred also filters them since its audit. | Fixed, high (115ce63) |
| Orphaned feed script | After poshanka died the script (and `notredctl subscribe`) ran on until its next write. Now `PR_SET_PDEATHSIG` stops it. | Fixed, low (4adc9bc) |
| Huge log lines | A bad feed line was logged whole (megabytes with avatars); now its first 200 bytes. | Fixed, low (1e43683) |
| Provider commands | Run as argv (`notredctl close <id>`), never through a shell; ids are numbers. | OK |
| Icon paths | `icon.path` from a notification is read (PNG/SVG) like any icon; nothing leaves the screen. Huge files cost memory, as in any image viewer. | Accepted |
| shm buffers | Created with `tempfile_in("/dev/shm")` (unlinked, mode 0600). | OK |
| Dependencies | `cargo deny check` passes. | OK |

## 5. Usability

| Area | Finding | Status |
| ---- | ------- | ------ |
| Reload scope | A reload re-reads theme and fragments only; the README now says placement, stack, layer and provider need a restart. | Fixed, docs (8ce2856) |
| Tall stacks | Cards beyond the screen edge are drawn off screen (sway lets the surface grow). notred's `[queue].max_visible` limits the stack. | Accepted |

## 6. Code quality, tests and hygiene

| Area | Finding | Status |
| ---- | ------- | ------ |
| Untested Wayland loop | The Wayland loop, feed thread, subscriber and binary had no tests (0 %). `poshanka/tests/live.rs` runs the binary in a headless sway: drawing (pixels via `grim`), clicks via a virtual pointer, buffer lifetimes via `WAYLAND_DEBUG`, reload, overrides, placement, the feed's restarts and shutdown. CI installs sway and grim (`POSHANKA_LIVE_TESTS=1`). | Fixed (34e80f5, 3adf89b) |
| Coverage | 56.7 % → 81.3 % of lines (`cargo llvm-cov --workspace --all-features`, with sway and grim installed). | Fixed |
| Cargo warnings | Three manifest warnings on every build. | Fixed (4bba10c) |
| Dependencies | parley 0.13, vello_cpu 0.4 and resvg 0.50 (the rest were current); cards render pixel-identical. | Fixed (0c04f6b) |
| Gates | fmt, typos, deny, clippy and tests at the three feature levels, and docs, pass on every commit. | OK |

## 7. Left open

- **`[layout].margin` does nothing.** The theme key is read into
  `CardStyle::margin` and never used (spacing comes from `[stack].gap` and
  `[placement].margin`). Either implement it (an outer margin per card, as in
  mako) or drop it; both change what existing themes do, so it is a decision
  for the maintainer. Your theme sets it to 5.
- **Avatar stream size (§3).** Sending each icon once and referring to it by
  id would cut notred's stream from quadratic to linear, but poshanka copes
  with 20 large avatars well, so the protocol change isn't needed now.
- **Stale cards while the provider is down.** When notred stops, the cards it
  sent stay until the feed reconnects; clicking them fails harmlessly.
- **Cards off screen (§5).** A stack taller than the output could be cut to
  the cards that fit, using the output's size.

## 8. Re-running the audit

```sh
# Gates (docs/ARCHITECTURE.md §8); live tests need sway and grim
cargo fmt --all -- --check && typos && cargo deny check
for f in --no-default-features "" --all-features; do
  cargo clippy --workspace --all-targets $f -- -D warnings
  POSHANKA_LIVE_TESTS=1 cargo test --workspace $f
done
cargo doc --workspace --no-deps

# Coverage (system LLVM tools on Arch: LLVM_COV / LLVM_PROFDATA)
POSHANKA_LIVE_TESTS=1 LLVM_COV=/usr/bin/llvm-cov LLVM_PROFDATA=/usr/bin/llvm-profdata \
  cargo llvm-cov --workspace --all-features --summary-only
```

To try a change by hand without touching the desktop, start a headless sway
with a private runtime directory (`XDG_RUNTIME_DIR=$(mktemp -d)
WLR_BACKENDS=headless WLR_RENDERER=pixman sway`), point `WAYLAND_DISPLAY` at
it, run `poshanka --config <copy>` with a feed script that reads a FIFO, and
look with `grim`. `poshanka/tests/common/mod.rs` does exactly this.
