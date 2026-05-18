# poshanka — Rust architecture + implementation plan

This document is the **human roadmap** and **agent playbook** for **poshanka**: a minimal Wayland-native notification daemon (dunst / mako inspired) using **Cairo + Pango** for drawing, **no** heavyweight UI toolkits, and **no** in-daemon settings panels or notification-center history UI — user-visible side effects are **spawn external commands** (shell runner) or **D-Bus protocol responses** only.

It mirrors the **execution discipline** of the sibling [abar](https://github.com/Gigas002/abar) project and WAU-style plans:

- Library-first crate split, small verifiable phases, strict quality gates (fmt, clippy `-D warnings` with feature matrix, tests, `cargo doc`, `typos`, `cargo deny`).
- **Directory modules** with **sibling `tests.rs`** — tests never live in the same file as logic (same rule as WAU §2.0).
- **Per-integration Cargo features** so minimal installs and CI do not bitrot optional code paths.

**Architectural reference (external):** [abar](https://github.com/Gigas002/abar) uses the same stack (layer shell, Cairo/Pango, Tokio, zbus). This repo must **not** vendor or depend on `abar` / `libabar`. When implementing poshanka, **copy patterns** into `libposhanka`, not crates.

**Reference configs (source of truth for schemas — create/update before each phase that touches config):**

- `examples/config.toml` — placement, timeouts, queue limits, optional action command templates.
- `examples/theme.toml` — RGBA colors per urgency, card geometry, typography.

---

## 1. Goals and constraints

### 1.1 Goals

- **Minimal surface area**: smallest useful notification daemon; optional behavior behind **compile-time** `features` where practical (not for the core D-Bus server — see §5.1).
- **Wayland-native**: `zwlr_layer_shell_v1` overlay surfaces; correct anchor, margins, keyboard interactivity `none`, fractional scale / buffer scale where supported.
- **Cairo + Pango**: measure and paint notification **cards** on an **image buffer** (shm) per surface or per combined overlay (`cairo-rs`, `pango`, `pangocairo`); keep gtk-rs stack versions aligned within one minor.
- **Freedesktop notifications**: session-bus **`org.freedesktop.Notifications`** via **native Rust [`zbus`](https://crates.io/crates/zbus)** — **no** `libdbus` / `dbus-glib`.
- **Dunst-like UX defaults**: corner stack (e.g. top-right), vertical gap, max visible count, urgency-based colors/timeouts, pointer dismiss, optional action buttons — **behavioral** reference, not a config-format clone.
- **Config discovery**: XDG-style resolution (e.g. `$XDG_CONFIG_HOME/poshanka/config.toml`, theme under `.../poshanka/themes/`), plus `--config` / `--theme` on the binary.
- **Control IPC**: same session bus as notifications — a **vendor D-Bus interface** for the `poshanka` CLI (reload, close-all, pause, …), not a second Unix-socket protocol in v0 (see §5).

### 1.2 Discipline (non-negotiable, WAU-style)

- **Library-first**: **`libposhanka`** — notification model, queue/timeouts, render, Wayland surfaces, D-Bus server glue that is testable without TOML; **`poshanka`** — `main` (tracing, CLI, read config/theme TOML, run loop).
- **`poshanka` contains no domain logic** beyond wiring; **`libposhanka` does not depend on clap** or **toml** and does not assume a specific logger implementation beyond `tracing`.
- **Tokio for async work**: use **`tokio`** for D-Bus I/O, timers (auto-dismiss), and `tokio::process` for user commands. The Wayland client loop stays **synchronous** on the main thread (`poll` + nonblocking dispatch, wakeup `UnixStream`); never block it on subprocess or socket I/O — offload with `tokio::spawn`.
- **Step sizing**: small PR-sized phases with explicit **Verify** blocks.
- **Feature matrix in CI**: default, `--all-features`, `--no-default-features` (core must still build — define explicitly in Phase 0: queue + render types without live D-Bus or Wayland if needed).
- **Naming**: short, descriptive; prefer clarity over abstraction depth.
- **Code comments**: describe current behavior only (invariants, protocol steps, non-obvious effects). No roadmap phase labels, session/chat context, or long rationale unrelated to reading the code.

### 1.3 Non-goals / deferred

- **No** GTK/Qt/iced/winit notification applets; **no** full notification center / history browser.
- **No** pixel-perfect dunst clone; target is **similar** stacking, colors, and timeouts from examples.
- **No** X11 or DBus-less “demo mode” in the first milestone binary (stub builds in `libposhanka` for CI only).
- **Post-v0**: dunst-style **rules** engine, inline **images** (`image-data` / `image-path` hints), **progress** bars, sound via spawn, `body-markup` (Pango markup), inhibition (`inhibit` hint), per-app scripting — each behind a feature when added.

### 1.4 Definitions

- **Notification**: one client message (`Notify`) with id, summary, body, urgency, timeout, optional icon, optional actions.
- **Card**: rendered representation of one notification (rounded rect, text, optional icon strip, optional action row).
- **Stack**: ordered list of visible cards at a screen corner; older entries shift or drop per `max_visible` policy.
- **Surface strategy (v0)**: prefer **one layer-shell surface per notification** for simpler hit-testing and independent timeouts; revisit a single-surface stack if compositor overhead becomes an issue (document in phase notes).
- **IPC (two planes)**:
  - **Notification plane** — standard `org.freedesktop.Notifications` (apps, `notify-send`).
  - **Control plane** — poshanka-specific D-Bus methods for the CLI talking to the **already-running** daemon (dunst: `org.dunstproject.cmd0` on the same object path).

---

## 2. Repository layout (target)

```text
poshanka/                      # workspace root
  Cargo.toml                   # workspace members: libposhanka, poshanka
  Cargo.lock                   # committed
  deny.toml
  examples/
    config.toml
    theme.toml
  libposhanka/
    Cargo.toml                 # features defined here
    src/
      lib.rs
      error.rs                 # thiserror (Wayland / SHM / render / async only)
      model/                   # Notification, Urgency, Action, StackAnchor, RuntimeSpec
      queue/                   # ids, enqueue, dismiss, timeout scheduling hooks
      render/                  # cairo+pango: measure card, paint card
      icon/                    # app-icon hint: path + freedesktop name (Phase 6)
      wayland/                 # compositor, layer_shell, per-notification surfaces, pointer
      dbus/                    # zbus: freedesktop Notifications + control interface
        notifications/         # Notify, CloseNotification, signals
        control/               # reload, close-all, pause, … (CLI client + server)
      spawn/                   # Tokio runtime + sh -c (action / close hooks from config)
  poshanka/
    Cargo.toml                 # clap, toml, tracing-subscriber; feature passthrough
    src/
      main.rs                  # clap: default = daemon; subcommands = control client
      error.rs                 # config/theme/file validation (thiserror)
      config/
        mod.rs
        tests.rs
      theme/
        mod.rs
        tests.rs
      cli/
        mod.rs
        tests.rs
      settings/
        mod.rs                 # merged view: cli > env > config
        tests.rs
      app/
        mod.rs                 # load Settings → libposhanka::run_daemon
  docs/
    PLAN.md                    # this file
  .github/workflows/           # CI matrices (default / all-features / no-default-features)
```

**Crate boundary rules**

- `libposhanka` has **no** `clap`, **no** `toml`, **no** config/theme parsers; **no** `println!` (use `tracing`).
- After `Settings` (or `DaemonSpec`) is built in `poshanka`, only plain structs cross into `libposhanka::run_daemon` — avoid threading raw `clap` types through the library.

**Feature passthrough:** `poshanka` features are **thin passthroughs** to `libposhanka`, e.g. `cargo install poshanka --no-default-features --features "dbus,icons"`.

---

## 3. Data model and config

### 3.1 `config.toml` (see `examples/config.toml` — to be authored in Phase 1)

**Intent**

- **`[base]`**: `font_name`, `font_size`, `theme` (filename or path relative to themes dir).
- **`[placement]`**: `anchor` (`top-right` | `top-left` | `bottom-right` | `bottom-left`), `margin_x`, `margin_y`, `gap` between cards, `max_visible` (drop or expire oldest when exceeded — document choice in Phase 7).
- **`[timeouts]`**: default milliseconds per urgency (`low`, `normal`, `critical`); `persist` sentinel for “until closed” (maps from D-Bus timeout `-1`).
- **`[server]`**: `app_name`, `app_icon` for `GetServerInformation` (Freedesktop spec).
- **`[commands]`** (optional): `on_action` / `on_close` shell templates with placeholders (`{action_key}`, `{id}`, `{app_name}`) — executed via `spawn`; failures logged only.
- **`[ignore]`** (optional, post-v0): reserved for rules — do not implement in v0.

**Invariants**

- Unknown keys: ignored by serde unless we add explicit handling later.
- Missing required `font_name` / placement fields: use documented defaults in `Settings::resolve`, not silent failure, unless we choose strict mode — **pick one in Phase 1** and test it.

### 3.2 `theme.toml` (see `examples/theme.toml` — Phase 1)

**Intent**

- Global `background_color`, `foreground_color`, `border_color` (RGBA hex).
- **`[urgency.low]` / `[urgency.normal]` / `[urgency.critical]`**: optional background/border overrides.
- **Geometry**: `card_padding_x/y`, `card_radius`, `max_width`, `icon_size`, action button padding.
- **`scale_factor`**: deferred — use compositor fractional scale from Wayland when available.

### 3.3 D-Bus → internal model mapping

| D-Bus `Notify` arg / hint | Internal field | v0 support |
| ------------------------- | -------------- | ------------ |
| `app_name`, `replaces_id` | metadata | yes |
| `app_icon` | icon hint (name or path) | path + name in Phase 6 |
| `summary`, `body` | text | yes (plain text) |
| `actions` | `Vec<Action>` | Phase 5 |
| `hints` urgency | `Urgency` | yes |
| `timeout` | override or config default | Phase 7 |
| `image-*`, `category`, markup | — | deferred §1.3 |

Implement **`GetCapabilities`** conservatively: advertise only what is implemented (e.g. `body`, `actions`, `icon-static` after Phase 6). Do not claim `body-markup` until implemented.

---

## 4. Rendering and UI

### 4.1 Cairo + Pango pipeline

- **Measure**: summary (bold or larger font), body (wrapped to `max_width`), optional icon column, optional action row.
- **Draw**: rounded rect fill + border; clip/wrap body with Pango; ellipsis on single-line summary if needed.
- **Buffer**: ARGB32 premultiplied or BGRA — pick one (`parse_hex_rgba_to_bgra` style) and document once.
- **Upload**: `wl_shm` pool per surface resize; full card redraw acceptable for **v0**.

### 4.2 Layout within a card

```text
┌─────────────────────────────────────┐
│ [icon]  Summary (single line)       │
│         Body (wrapped)              │
│         [ Action1 ] [ Action2 ]     │
└─────────────────────────────────────┘
```

- Icon column omitted when no icon resolved.
- Actions: horizontal row of text buttons (not GTK widgets); pointer hit regions computed in `wayland` / `hit_test` sibling module.

### 4.3 Stack placement

- Compute screen position from output geometry + `placement.anchor` + cumulative card heights + `gap`.
- On new notification or dismiss: reposition all surfaces (v0 may destroy/recreate surfaces — keep logic in `queue` + `wayland`).

---

## 5. Wayland and IPC policy

**Yes — poshanka needs IPC.** D-Bus on the session bus **is** IPC; the plan treats it as the **only** v0 wire protocol (no private Unix-socket control protocol like some older tools). Two logical interfaces on the **same** connection object path, same as dunst.

### 5.1 D-Bus — notification plane (core)

- Well-known **bus name**: **`org.freedesktop.Notifications`** (session bus). Owning this name is the **single-instance** guard: a second `poshanka` process must not become a second daemon (see §5.4).
- Object path: **`/org/freedesktop/Notifications`**
- Interface: **`org.freedesktop.Notifications`**
- Methods (v0): `Notify`, `CloseNotification`, `GetCapabilities`, `GetServerInformation`.
- Signals (v0): `NotificationClosed`, `ActionInvoked`.
- **zbus** only; code under `libposhanka/src/dbus/notifications/`.
- **Cargo feature `dbus`** (default **on** for `poshanka` binary): links zbus. For `--no-default-features` CI, `libposhanka` must still compile (queue + render unit tests; dbus module gated with `#[cfg(feature = "dbus")]`).

**Threading:** D-Bus callbacks run on Tokio; enqueue/dequeue and “request redraw” via `mpsc` + wakeup fd into the Wayland thread. Control-plane calls use the **same** channel types (e.g. `ControlRequest::ReloadConfig`).

### 5.2 D-Bus — control plane (core for CLI)

Daemon-only operations are **not** part of the Freedesktop spec. Expose a **vendor interface** on the same object path (dunst precedent: `org.dunstproject.cmd0`). Poshanka v0:

| Item | Value |
| ---- | ----- |
| Interface | **`org.poshanka.Daemon1`** (versioned suffix; bump if breaking) |
| Path | `/org/freedesktop/Notifications` (same object as notifications) |
| Client | `poshanka` binary **subcommands** (`clap`), implemented in `poshanka` with **`zbus`** proxy — no `libdbus` |

**Methods (v0 minimum):**

| Method | Purpose |
| ------ | ------- |
| `Ping` | Health check; fails if daemon not running |
| `CloseAll` | Dismiss every visible notification (emit `NotificationClosed` per id) |
| `Close` | `CloseNotification` by id (control path for CLI) |
| `Reload` | Re-read config/theme from disk, apply without restart |
| `Pause` / `Unpause` | Stop showing new `Notify` (queue or drop); dunst parity |

**Deferred (post-v0):** `HistoryList`, `RuleList`, inhibition — do not block v0.

**Why not a Unix socket?** Avoids a second protocol, second security story, and duplicate wakeups; **zbus-only** on the session bus. Revisit only if a concrete integrator requires it.

### 5.3 Wayland (core)

- `wayland-client`, `wayland-protocols-wlr` (`wlr-layer-shell-unstable-v1`).
- Layer: **overlay**; anchor from config; keyboard interactivity **none**.
- Pointer: click on card → default dismiss; click on action → `ActionInvoked` + optional spawn template.
- Seat: pointer required; keyboard not required for v0.

### 5.4 Process model and single instance

- **Daemon mode** (default): `poshanka` — request bus name `org.freedesktop.Notifications`, run Wayland loop until exit.
- **Client mode**: `poshanka close-all`, `poshanka reload`, … — connect to session bus, call `org.poshanka.Daemon1` on the running daemon; exit non-zero if name not owned or method fails.
- **No separate `poshankactl` binary in v0** unless packaging demands it later; one crate, two clap entry paths.
- Optional: write **`$XDG_RUNTIME_DIR/poshanka/pid`** for human debugging only — **not** authoritative for locking (bus name is).

### 5.5 Optional features (post-core)

| Feature (example) | Responsibility |
| ----------------- | -------------- |
| `icons` | FreeDesktop name + filesystem path icons |
| `svg` | SVG via `resvg` (optional polish) |
| `markup` | `body-markup` capability + Pango markup body |
| `rules` | dunst-like match overrides |

---

## 6. Module catalog (`libposhanka`)

Each directory: `mod.rs` + **`tests.rs`**.

| Module | Responsibility |
| ------ | -------------- |
| `model` | `Notification`, `Urgency`, `Action`, `DaemonSpec`, `CardStyle` |
| `queue` | Monotonic ids, stack order, replace-by-id, timeout registration |
| `render` | `measure_card`, `paint_card` → pixel buffer |
| `wayland` | Globals, surfaces, configure, buffer attach, pointer dispatch |
| `dbus` | `notifications/` + `control/` zbus servers; shared Tokio + `mpsc` to Wayland thread |
| `icon` | Resolve `app_icon` hint (Phase 6) |
| `spawn` | Shared Tokio runtime + `sh -c` |

---

## 7. Quality gates

Whenever a phase is marked complete:

- `cargo fmt --check`
- `typos`
- `cargo deny check licenses` (populate `deny.toml` allow list before enforcing in CI)
- `cargo clippy --workspace --all-targets --no-default-features -- -D warnings`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `cargo test --workspace --no-default-features`
- `cargo test --workspace --all-features`
- `cargo doc --workspace --no-deps`

### 7.1 Test discipline

- Unit tests in **`tests.rs`** next to `mod.rs`.
- Integration tests under `poshanka` (config/theme TOML) and `libposhanka/tests/` (queue ordering, render pixel samples, dbus with `zbus` test bus if feasible).

### 7.2 CI

Workflows must target **`libposhanka`** / **`poshanka`** only (install **libcairo2-dev**, **libpango1.0-dev**). D-Bus tests: document headless strategy (dbus-test-runner or skip with issue link) in Phase 4.

---

## 8. Phased steps

### Phase 0 — Workspace + hygiene + empty vertical slice

- [x] **Purge abar from the repo** (complete before other Phase 0 work):
  - [x] Delete the `abar/` tree (vendored copy of the sibling bar project).
  - [x] Delete `docs/ABAR_PLAN.md` (lives in the [abar](https://github.com/Gigas002/abar) repo, not here).
  - [x] Fix root `Cargo.toml`: `members = ["libposhanka", "poshanka"]`; workspace `package` metadata for **poshanka** (homepage, repository, description, keywords — no abar URLs).
  - [x] Update `.github/workflows/*` so every `cargo -p`, archive name, Codecov flag, and deploy/publish step references **`poshanka`** / **`libposhanka`** only.
  - [x] Grep tracked files: `rg -i 'abar|libabar' --glob '!docs/PLAN.md'` returns **no matches** (this plan may link to the external sibling repo only).
- [x] Scaffold `libposhanka` + `poshanka` with tracing in binary only.
- [x] `libposhanka`: connect Wayland, bind layer shell, show **one** solid-color overlay rect (theme background) — no text.
- [x] `poshanka`: load minimal `config.toml` / `theme.toml` (font + colors only); exit with structured error on missing files if strict.
- [x] Populate **`deny.toml`** license allow list for Wayland stack crates (cairo/pango added in Phase 2).

**Verify**: all gates in §7; `rg -i 'abar|libabar' --glob '!docs/PLAN.md'` empty; manual run on Hyprland (or any layer-shell compositor).

### Phase 1 — Config + theme + runtime spec

- [ ] Serde models for `examples/config.toml` / `examples/theme.toml` (placement, timeouts, server info, urgency colors).
- [ ] XDG path resolution + `--config` / `--theme` (`clap`).
- [ ] `Settings::resolve` → `DaemonSpec` + `CardStyle` plain structs for `libposhanka`.

**Verify**: unit tests deserialize examples; no Wayland required.

### Phase 2 — Render core (Cairo + Pango)

- [ ] Implement `color`, `render/font`, rounded rect, BGRA buffer (port from [abar](https://github.com/Gigas002/abar) if useful).
- [ ] `measure_card` / `paint_card` with summary + body only (placeholder icon/actions).
- [ ] Headless tests: non-transparent pixels in card bbox; text layout sanity.

**Verify**: `libposhanka` render tests pass without compositor.

### Phase 3 — Wayland surfaces + pointer dismiss

- [ ] One layer surface per notification card (or documented alternative).
- [ ] SHM buffer resize on configure; paint via Phase 2.
- [ ] Pointer: click outside action areas → dismiss (queue removes, surface destroyed).
- [ ] Wakeup pipe + `poll` loop (nonblocking Wayland dispatch).

**Verify**: manual show/hide with a test harness calling `libposhanka` directly (pre-D-Bus).

### Phase 4 — D-Bus server + queue (Notify path)

- [ ] `dbus/notifications/`: request name `org.freedesktop.Notifications`, register object, implement `Notify`, `GetCapabilities`, `GetServerInformation`.
- [ ] `queue/`: assign ids, stack ordering, `replaces_id`.
- [ ] `Notify` → enqueue → create surface → paint.
- [ ] `CloseNotification` + emit `NotificationClosed`.
- [ ] `Ping` on control interface only (proves bus registration + client path); full control methods in Phase 4b.

**Verify**: `dbus-send` / `notify-send` manual test; unit tests for id/replace logic; `poshanka ping` (or equivalent) succeeds while daemon runs.

### Phase 4b — Control plane + CLI client

- [ ] `dbus/control/`: implement `org.poshanka.Daemon1` (`CloseAll`, `Close`, `Reload`, `Pause`, `Unpause`).
- [ ] Map control calls to `mpsc` commands handled on Wayland thread (same as `Notify`).
- [ ] `poshanka` **clap** subcommands: `close-all`, `close <id>`, `reload`, `pause`, `unpause`, `ping`.
- [ ] Second `poshanka` without subcommand: fail fast if bus name already taken (clear error message).

**Verify**: integration test with zbus test bus or documented manual script; `poshanka reload` picks up theme change without restart.

### Phase 5 — Actions

- [ ] Parse `actions` array; render action row; pointer hit-test per button.
- [ ] Emit `ActionInvoked`; optional `[commands].on_action` spawn.
- [ ] Default action / middle-click behavior: document and keep minimal (dunst: often dismiss — pick one).

**Verify**: `notify-send` with `--action` flags; signal observed via `dbus-monitor`.

### Phase 6 — Icons

- [ ] `icon/`: `app_icon` as Freedesktop name or absolute path; PNG → Cairo.
- [ ] Feature `icons` (default on for binary); fail startup or degrade gracefully — **document in examples**.
- [ ] Update `GetCapabilities` to include `icon-static` when enabled.

**Verify**: fixture icon theme tests; manual `notify-send -i`.

### Phase 7 — Timeouts, urgency, stack limits

- [ ] Map urgency → theme colors + default timeout from config.
- [ ] Tokio timers: auto-dismiss → `NotificationClosed` reason timeout.
- [ ] `max_visible`: drop oldest or reject new — document policy.
- [ ] Honor `Notify` timeout override (`-1` persist).

**Verify**: unit tests for timeout math; manual short/long notifications.

### Phase 8 — Polish + first release

- [ ] README: deps, `XDG_CONFIG_HOME`, `dbus-send` examples, compositor requirements.
- [ ] CHANGELOG; tag **v0.1.0**.
- [ ] Desktop file / dbus service activation — **optional** if scope creep; otherwise document `exec poshanka` in README.

**Verify**: full §7 gates + dogfood with common apps (fcitx, browsers, etc.).

### Post-first-release

- [ ] **Rules** (`[ignore]`, match app_name/urgency) — feature `rules`.
- [ ] **body-markup**, **image-data**, **progress**, **inhibit** hints — separate features.
- [ ] Single-surface stack optimization if profiling warrants it.

---

## 9. Definition of done (v0 / first working draft)

- [ ] `notify-send` displays stacked notifications on Wayland with theme from `examples/theme.toml`.
- [ ] Placement, gaps, and `max_visible` behave per config.
- [ ] Urgency colors and timeouts work; persistent notifications (`timeout = -1`) stay until dismissed.
- [ ] Actions emit `ActionInvoked`; dismiss emits `NotificationClosed` with correct reason codes.
- [ ] **No** GTK/iced; Cairo+Pango path is live.
- [ ] **zbus** only for session D-Bus (notification + control); no libdbus; no v0 Unix control socket.
- [ ] CLI control subcommands work against a running daemon (`reload`, `close-all`, `pause`).
- [ ] CI green on default / all-features / no-default-features; docs build.

---

## 10. Dependency policy

- **Edition**: `2024`.
- **Versions**: `x.y` or `x` in manifests; lockfile committed.
- **Async runtime**: **`tokio`** in **`libposhanka`** — lean features (`rt-multi-thread`, `process`, `time`, `macros`).
- **D-Bus**: **`zbus`** with default feature on shipped binary; justify in PR.
- **Graphics**: `cairo-rs`, `pango`, `pangocairo` aligned to one gtk-rs minor.
- **Wayland**: `wayland-client`, `wayland-protocols-wlr` — pin versions in workspace `Cargo.toml`.

---

## 11. Implementation pattern checklist

When porting from the external [abar](https://github.com/Gigas002/abar) repo, map concerns as follows:

| Concern | poshanka module |
| ------- | --------------- |
| Hex RGBA → buffer | `libposhanka/src/color/` |
| Font metrics | `libposhanka/src/render/` |
| Rounded rects | `libposhanka/src/render/` |
| SHM buffer lifecycle | `libposhanka/src/wayland/` (per-notification surface) |
| Async shell commands | `libposhanka/src/spawn/` |
| Settings boundary | `poshanka/src/settings/` → `DaemonSpec` |
| Error split | lib vs bin `error.rs` |
| Poll + wakeup | `libposhanka/src/wayland/` |

**Never** add the sibling bar library as a Cargo dependency.

---

## 12. Document maintenance

Update this plan when:

- feature set or D-Bus surface changes
- examples change — update `examples/*.toml` first, then this doc
- surface strategy (one vs many `wl_surface`) changes

---

## Revision history

| Date       | Change                                                                 |
| ---------- | ---------------------------------------------------------------------- |
| 2026-05-18 | Initial poshanka plan (WAU-style discipline + dunst goals) |
| 2026-05-18 | §5 IPC: notification vs control D-Bus planes; Phase 4b; single-instance via bus name |
| 2026-05-18 | Phase 0: purge all `abar` / `libabar` from repo; external abar link only |
