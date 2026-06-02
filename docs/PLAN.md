# poshanka — Rust architecture + implementation plan

This document is the **human roadmap** and **agent playbook** for **poshanka**: a minimal Wayland-native notification daemon, **behaviorally inspired mainly by [mako](https://github.com/emersion/mako)** (with [dunst](https://dunst-project.org/) as a secondary reference where overlap is small), using **Cairo + Pango** for drawing, **no** heavyweight UI toolkits, and **no** in-daemon settings panels or notification-center history UI — user-visible side effects are **spawn external commands** (shell runner) or **D-Bus protocol responses** only.

It mirrors the **execution discipline** of the sibling [abar](https://github.com/Gigas002/abar) project and WAU-style plans:

- Library-first crate split, small verifiable phases, strict quality gates (fmt, clippy `-D warnings` with feature matrix, tests, `cargo doc`, `typos`, `cargo deny`).
- **Directory modules** with **sibling `tests.rs`** — tests never live in the same file as logic (same rule as WAU §2.0).
- **Per-integration Cargo features** so minimal installs and CI do not bitrot optional code paths.

**Architectural reference (external):** [abar](https://github.com/Gigas002/abar) uses the same stack (layer shell, Cairo/Pango, Tokio, zbus). This repo must **not** vendor or depend on `abar` / `libabar`. When implementing poshanka, **copy patterns** into `libposhanka`, not crates.

**Reference configs (source of truth for schemas — update examples first, then this doc):**

- `examples/config.toml` — global daemon behavior; `paths.overrides` lists fragment paths.
- `examples/theme.toml` — base visual theme; fragments patch tables (e.g. `examples/urgency/*/theme.toml`).
- `examples/apps/<name>/config.toml` — optional `[override]` fragments (app or urgency), same schema as root config for overridable sections.

---

## 1. Goals and constraints

### 1.1 Goals

- **Minimal surface area**: smallest useful notification daemon; optional behavior behind **compile-time** `features` where practical (not for the core D-Bus server — see §5.1).
- **Wayland-native**: `zwlr_layer_shell_v1` overlay surfaces; correct anchor, margins, keyboard interactivity `none`, fractional scale / buffer scale where supported.
- **Cairo + Pango**: measure and paint notification **cards** on an **image buffer** (shm) per surface or per combined overlay (`cairo-rs`, `pango`, `pangocairo`); keep gtk-rs stack versions aligned within one minor.
- **Freedesktop notifications**: session-bus **`org.freedesktop.Notifications`** via **native Rust [`zbus`](https://crates.io/crates/zbus)** — **no** `libdbus` / `dbus-glib`.
- **Mako-like UX defaults**: corner stack, gap, max visible, urgency-driven look/timeouts, tap-to-dismiss or whole-card activation — **behavioral** reference, not a mako/dunst config clone (poshanka uses its own `examples/` schema). **Action buttons are never drawn** (unlike mako’s optional action UI) — whole-card tap + `ActionInvoked` only.
- **Config discovery**: XDG-style resolution (e.g. `$XDG_CONFIG_HOME/poshanka/config.toml`, theme under `.../poshanka/themes/`), plus `--config` / `--theme` on the binary.
- **Control IPC**: same session bus as notifications — a **vendor D-Bus interface** for **`poshankactl`** (makoctl parity: reload, close-all, pause, …), not a second Unix-socket protocol in v0 (see §5).

### 1.2 Discipline (non-negotiable, WAU-style)

- **Library-first**: **`libposhanka`** — notification model, queue/timeouts, render, Wayland surfaces, D-Bus **server** glue (testable without TOML); **`poshanka`** — daemon crate (config/theme, `Settings`, run loop); **`poshankactl`** — separate **crate** (makoctl parity), thin zbus **client** only.
- **`poshanka` contains no domain logic** beyond wiring; **`libposhanka` does not depend on clap** or **toml** and does not assume a specific logger implementation beyond `tracing`.
- **Tokio for async work**: use **`tokio`** for D-Bus I/O, timers (auto-dismiss), and `tokio::process` for user commands. The Wayland client loop stays **synchronous** on the main thread (`poll` + nonblocking dispatch, wakeup `UnixStream`); never block it on subprocess or socket I/O — offload with `tokio::spawn`.
- **Step sizing**: small PR-sized phases with explicit **Verify** blocks.
- **Feature matrix in CI**: default, `--all-features`, `--no-default-features` (core must still build — define explicitly in Phase 0: queue + render types without live D-Bus or Wayland if needed).
- **Naming**: short, descriptive; prefer clarity over abstraction depth.
- **Code comments**: describe current behavior only (invariants, protocol steps, non-obvious effects). No roadmap phase labels, session/chat context, or long rationale unrelated to reading the code.

### 1.3 Non-goals / deferred

- **No** GTK/Qt/iced/winit notification applets; **no** full notification center / history browser.
- **No** pixel-perfect mako clone; target is **similar** stacking, colors, and timeouts from `examples/`, with deliberate divergences (§1.5).
- **No** X11 or DBus-less “demo mode” in the first milestone binary (stub builds in `libposhanka` for CI only).
- **No dunst-only (or generic FDN) features mako does not have** — e.g. dunst rule **scripts**, notification **history** UI, **inhibition**, inline **`image-data`** in the body, daemon-owned **sound** playback. If mako does not do it, poshanka does not roadmap it.
- **Deferred = mako parity gaps only** — work not yet done in the phased plan but already in mako’s behavior, mainly: **icons** (Phase 6), **timeouts/urgency/stack** polish (Phase 7), **`[progress]`** + `value` hint (theme schema exists; render when implemented), optional **body markup** if we advertise `body-markup` like mako. Extra **criteria** keys matching mako sections (`category`, `desktop-entry`, …) may extend `[override]` later; not a separate dunst-style rules engine.

### 1.4 Definitions

- **Notification**: one client message (`Notify`) with id, summary, body, urgency, timeout, optional icon, optional actions.
- **Card**: rendered representation of one notification (rounded rect, text, optional icon). **No action button row, ever** — client actions use whole-card tap + `ActionInvoked`.
- **Stack**: ordered list of visible cards at a screen corner; older entries shift or drop per `max_visible` policy.
- **Surface strategy (v0)**: prefer **one layer-shell surface per notification** for simpler hit-testing and independent timeouts; revisit a single-surface stack if compositor overhead becomes an issue (document in phase notes).
- **IPC (two planes)**:
  - **Notification plane** — standard `org.freedesktop.Notifications` (apps, `notify-send`).
  - **Control plane** — poshanka-specific D-Bus methods for **`poshankactl`** talking to the **already-running** daemon (mako: `makoctl`; dunst: `dunstctl` / `org.dunstproject.cmd0`).

### 1.5 Behavioral reference (mako primary, dunst secondary)

| Area | Follow **mako** | Notes / **dunst** where different |
| ---- | ----------------- | --------------------------------- |
| Platform | Wayland layer-shell popups, minimal chrome | Dunst is often X11-era in docs; both use FDN D-Bus. |
| Config shape | Global defaults + **criteria/override fragments** (mako `[app-name=…]` sections → our `[override]` + `paths.overrides`) | Dunst uses monolithic `dunstrc` + rules. |
| Interaction | Tap card to dismiss; optional shell side effects via config | Dunst adds richer mouse enums (`close_current`, `do_action`, …) — we use `[events]` shell + **always** `ActionInvoked` when client sent actions. |
| Look | Theme tables (colors, layout, border, Pango text templates) | Dunst `format` string — we use per-field `{summary}` templates. |
| Progress | `over` / `source` bar compositing | Dunst separate progress-bar widget — we align with mako when implemented. |
| Actions UI | Mako *can* show buttons; **poshanka never does** | Both support FDN `actions` on the bus. |
| Control CLI | **`poshankactl`** — external ctl for reload, dismiss, pause (`makoctl` parity) | Dunst `dunstctl` / `org.dunstproject.cmd0` — same D-Bus role, different binary name. |

When in doubt during implementation, prefer **mako** behavior and config ergonomics; cite dunst only for FDN/control-plane precedent, not for feature scope.

---

## 2. Repository layout (target)

```text
poshanka/                      # workspace root (repo name)
  Cargo.toml                   # workspace members: libposhanka, poshanka, poshankactl
  Cargo.lock                   # committed
  deny.toml
  examples/
    config.toml
    theme.toml
    urgency/…/config.toml
    urgency/…/theme.toml
    apps/<name>/config.toml
    apps/<name>/theme.toml
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
      dbus/                    # zbus servers (daemon side only)
        notifications/         # Notify, CloseNotification, signals
        control/               # org.poshanka.Daemon1 server (reload, close-all, pause, …)
      spawn/                   # Tokio runtime + sh -c (action / close hooks from config)
  poshanka/
    Cargo.toml                 # [[bin]] poshanka; clap, toml, tracing-subscriber
    src/
      main.rs                  # daemon entry
      error.rs                 # config/theme/file validation (thiserror)
      config/
        mod.rs
        tests.rs
      theme/
        mod.rs
        tests.rs
      settings/
        mod.rs                 # merged view: cli > env > config
        tests.rs
      app/
        mod.rs                 # load Settings → libposhanka::run_daemon
  poshankactl/
    Cargo.toml                 # [[bin]] poshankactl; clap, zbus, tracing-subscriber
    src/
      main.rs                  # control client entry (makoctl parity)
      cli/
        mod.rs                 # subcommands: ping, reload, close-all, …
        tests.rs
      dbus/
        mod.rs                 # zbus proxy → org.poshanka.Daemon1
        tests.rs
  docs/
    PLAN.md                    # this file
  .github/workflows/           # CI: all workspace members
```

**Crate boundary rules**

- `libposhanka` has **no** `clap`, **no** `toml`, **no** config/theme parsers; **no** `println!` (use `tracing`). Hosts D-Bus **servers** only (notifications + control).
- `poshanka` loads TOML, builds `Settings` / `DaemonSpec`, runs the daemon. **No** control subcommands.
- `poshankactl` is a **thin client**: `clap` + zbus proxy to `org.poshanka.Daemon1` only. **No** Wayland, **no** config/theme load (except flags that only affect the ctl process, if any). May depend on `libposhanka` only for shared constants/types if useful — not on `poshanka` the crate.
- After `Settings` is built in `poshanka`, only plain structs cross into `libposhanka::run_daemon` — avoid threading raw `clap` types through the library.

**Feature passthrough:** `poshanka` features are **thin passthroughs** to `libposhanka`, e.g. `cargo install poshanka --no-default-features --features "dbus,icons"`. `poshankactl` needs **`dbus`** (zbus) only; install via `cargo install --path poshankactl` or package both binaries from the workspace.

---

## 3. Data model and config

Schemas are defined by **`examples/`** (see `examples/config.toml`, `examples/theme.toml`, and override fragments). Phase 1 implements serde + merge; behavior below is the contract agreed before implementation.

### 3.1 Config file roles

| File | Role |
| ---- | ---- |
| `examples/config.toml` | Global defaults: paths, stack, placement, queue, timeouts, layer, `[events]`. |
| `examples/theme.toml` | Base card look: font, colors, layout, border, text templates, icons, progress. |
| `examples/<fragment>/config.toml` | Patch overridable sections; may include `[override]` metadata. |
| `examples/<fragment>/theme.toml` | Patch theme tables only (e.g. urgency colors). |

**`[paths]`**

- `theme` — base theme file (relative to config directory).
- `overrides` — ordered list of fragment paths (relative to main config directory). Merge policy in Phase 1: **first matching `[override]` wins** for app/urgency (document tie-breaking in code); fragment fields **replace** the same keys in merged config/theme.

**`[override]`** (in fragments only)

- `type` — `app` \| `urgency`.
- `name` — required when `type = "app"` (matches `Notify` `app_name`).
- `level` — required when `type = "urgency"` (`low` \| `normal` \| `critical`).

**`[stack]`** — `max` visible notifications (global cap).

**`[placement]`** — `anchor`, `gap`, `margin` (outer margin on the anchored corner).

**`[queue]`** — `history`, `max`, `sort`, `order` (history UI deferred; store policy in Phase 7+).

**`[timeouts]`** — `ignore` (ignore client timeout hints), `default`, `low`, `normal`, `critical` (ms; `0` = persist until dismissed — document in Phase 7).

**`[layer]`** — layer-shell `layer`, optional `output` name.

**Not in user config (implementation only)**

- **`GetServerInformation`** — hardcoded in binary (`name`, `vendor`, `version`, `spec_version`). No `[server]` table.
- **`GetCapabilities`** — computed from what is implemented (e.g. `body`, `actions` when ActionInvoked is live). No `[capabilities]` table in TOML.

### 3.2 `[events]` — card click and notify hooks

User-side shell hooks on the **notification card**. **Action buttons are not planned** (no UI, no hit regions, not post-v0). Client actions come from the `Notify` `actions` array and whole-card tap only.

**Hard policy (click on card, per button: `on_button_left` / `on_button_middle` / `on_button_right` / `on_touch`):**

1. If the notification has **actions** from the client → **always** emit **`ActionInvoked`** on D-Bus (prefer key `"default"` when present, else document fallback for a single action). This is **never skipped** because of `[events]` shell commands (wayshot and similar clients may block in `wait_for_action`).
2. If an **`[events]`** key is set for that button → also run `sh -c '<command>'` (**additive**). Order: **shell first**, then **`ActionInvoked`**, then dismiss popup (exact dismiss reason in Phase 5).
3. If **no actions** on the notification and no shell key → **dismiss** (`NotificationClosed`, user reason).
4. If **no actions** but shell key is set → run shell only, then dismiss.

**`on_notify`** — optional `sh -c` when a notification is **shown** (not a click). Independent of `ActionInvoked`.

**No custom hint parameters** — do not read per-notification paths or other client hints for `[events]` substitution (wayshot keeps per-shot paths in its own process via `ActionInvoked`).

### 3.3 `theme.toml`

| Section | Keys (representative) |
| ------- | --------------------- |
| `[font]` | `name`, `size` |
| `[colors]` | `background`, `foreground`, `border`, `progress` |
| `[layout]` | `width`, `height` (max), `padding`, `margin` |
| `[border]` | `size`, `radius` |
| `[text]` | `alignment`; `summary`, `body`, optional `app`, `id` — Pango markup templates with `{summary}`, `{body}`, … (escape client text before substitute; `parse_markup` in render). |
| `[icons]` | `size` (≤0 off), `position`, `theme` |
| `[progress]` | `mode` — `over` \| `source` (mako-style compositing; data from `value` hint when progress is implemented). |

Urgency color patches live in fragments (e.g. `examples/urgency/critical/theme.toml`), not necessarily in base theme.

### 3.4 D-Bus → internal model mapping

| D-Bus `Notify` arg / hint | Internal field | v0 support |
| ------------------------- | -------------- | ------------ |
| `app_name`, `replaces_id` | metadata | yes |
| `app_icon` | icon hint (name or path) | path + name in Phase 6 |
| `summary`, `body` | text | yes (plain text) |
| `actions` | `Vec<Action>` (stored for `ActionInvoked`; **never** rendered as buttons) | Phase 5 |
| `hints` urgency | `Urgency` | yes |
| `timeout` | override or config default | Phase 7 |
| `image-data` / inline body images | — | **not planned** (mako uses icon column, not dunst-style inline images) |
| `category`, `desktop-entry` | match criteria for overrides | extend `[override]` when needed |
| `body-markup` | body | deferred §1.3 only if mako parity |
| custom hint substitution for `[events]` | — | **not planned** |

**`GetCapabilities`**: advertise only implemented behavior (e.g. `body`, `actions` once `ActionInvoked` on card tap works; `icon-static` after Phase 6). Do not claim `body-markup` until implemented.

---

## 4. Rendering and UI

### 4.1 Cairo + Pango pipeline

- **Measure**: summary + body from `[text]` templates (Pango markup); optional icon column. **No action row.**
- **Draw**: rounded rect fill + border; clip/wrap body with Pango; ellipsis on single-line summary if needed.
- **Buffer**: ARGB32 premultiplied or BGRA — pick one (`parse_hex_rgba_to_bgra` style) and document once.
- **Upload**: `wl_shm` pool per surface resize; full card redraw acceptable for **v0**.

### 4.2 Layout within a card

```text
┌─────────────────────────────────────┐
│ [icon]  Summary                     │
│         Body (wrapped)              │
└─────────────────────────────────────┘
     entire card = one click target (v0)
```

- Icon column omitted when no icon resolved.
- Whole card is the only click target → §3.2 `[events]` + `ActionInvoked` when client sent actions. No per-action button hit regions at any time.

### 4.3 Stack placement

- Compute screen position from output geometry + `placement.anchor` + cumulative card heights + `gap`.
- On new notification or dismiss: reposition all surfaces (v0 may destroy/recreate surfaces — keep logic in `queue` + `wayland`).

---

## 5. Wayland and IPC policy

**Yes — poshanka needs IPC.** D-Bus on the session bus **is** IPC; the plan treats it as the **only** v0 wire protocol (no private Unix-socket control protocol). Two logical interfaces on the **same** object path (mako/dunst pattern).

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

Daemon-only operations are **not** part of the Freedesktop spec. Expose a **vendor interface** on the same object path (`makoctl` / dunst `org.dunstproject.cmd0` precedent). Poshanka v0:

| Item | Value |
| ---- | ----- |
| Interface | **`org.poshanka.Daemon1`** (versioned suffix; bump if breaking) |
| Path | `/org/freedesktop/Notifications` (same object as notifications) |
| Client | **`poshankactl`** binary (`clap` + **`zbus`** proxy on `org.poshanka.Daemon1`) — no `libdbus`. Daemon binary **`poshanka`** does not implement control subcommands. |

**Methods (v0 minimum):**

| Method | Purpose |
| ------ | ------- |
| `Ping` | Health check; fails if daemon not running |
| `CloseAll` | Dismiss every visible notification (emit `NotificationClosed` per id) |
| `Close` | `CloseNotification` by id (control path for CLI) |
| `Reload` | Re-read config/theme from disk, apply without restart |
| `Pause` / `Unpause` | Stop showing new `Notify` (queue or drop); mako/dunst parity |

**Deferred (post-v0, only if mako parity):** e.g. `makoctl`-style **modes** — not dunst `HistoryList` / `RuleList` / inhibition.

**Why not a Unix socket?** Avoids a second protocol, second security story, and duplicate wakeups; **zbus-only** on the session bus. Revisit only if a concrete integrator requires it.

### 5.3 Wayland (core)

- `wayland-client`, `wayland-protocols-wlr` (`wlr-layer-shell-unstable-v1`).
- Layer: **overlay**; anchor from config; keyboard interactivity **none**.
- Pointer: card click per §3.2 — optional `[events]` shell, then **`ActionInvoked` if actions present**, else dismiss.
- Seat: pointer required; keyboard not required for v0.

### 5.4 Process model and single instance

- **`poshanka`** (daemon): request bus name `org.freedesktop.Notifications`, run Wayland loop until exit. No control subcommands on this binary (mako / makoctl split).
- **`poshankactl`** (control client crate, **makoctl parity**): connect to session bus, call `org.poshanka.Daemon1` on the running daemon; exit non-zero if name not owned or method fails. **Third workspace crate** `poshankactl/` (not a second binary inside `poshanka/`).
- Optional: write **`$XDG_RUNTIME_DIR/poshanka/pid`** for human debugging only — **not** authoritative for locking (bus name is).

**`poshankactl` v0 commands (minimum, mirror makoctl scope where applicable):**

| Command | Maps to | Notes |
| ------- | ------- | ----- |
| `poshankactl ping` | `Ping` | Health check |
| `poshankactl reload` | `Reload` | Re-read config/theme |
| `poshankactl close-all` | `CloseAll` | Dismiss all |
| `poshankactl close <id>` | `Close` | Dismiss one |
| `poshankactl pause` / `unpause` | `Pause` / `Unpause` | Stop/show new notifications |

Post-v0 (mako parity only): e.g. `poshankactl mode …` when modes are implemented.

### 5.5 Optional features (post-core)

| Feature (example) | Responsibility |
| ----------------- | -------------- |
| `icons` | FreeDesktop name + filesystem path icons |
| `svg` | SVG via `resvg` (optional polish) |
| `markup` | `body-markup` + Pango body (only if mako parity) |

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
- Integration tests under `poshanka` (config/theme TOML), `poshankactl` (ctl client), and `libposhanka/tests/` (queue ordering, render pixel samples, dbus with `zbus` test bus if feasible).

### 7.2 CI

Workflows must target **`libposhanka`**, **`poshanka`**, and **`poshankactl`** (`cargo test --workspace`, etc.). Install **libcairo2-dev**, **libpango1.0-dev** for the daemon path. D-Bus tests: document headless strategy (dbus-test-runner or skip with issue link) in Phase 4.

---

## 8. Phased steps

### Phase 0 — Workspace + hygiene + empty vertical slice

- [x] **Purge abar from the repo** (complete before other Phase 0 work):
  - [x] Delete the `abar/` tree (vendored copy of the sibling bar project).
  - [x] Delete `docs/ABAR_PLAN.md` (lives in the [abar](https://github.com/Gigas002/abar) repo, not here).
  - [x] Fix root `Cargo.toml`: workspace `package` metadata for **poshanka** (homepage, repository, description, keywords — no abar URLs).
  - [x] Root `Cargo.toml` **`members`**: `["libposhanka", "poshanka", "poshankactl"]` (three crates; see §2).
  - [x] Update `.github/workflows/*` so every `cargo -p`, archive name, Codecov flag, and deploy/publish step references **`poshanka`** / **`libposhanka`** only.
  - [x] Grep tracked files: `rg -i 'abar|libabar' --glob '!docs/PLAN.md'` returns **no matches** (this plan may link to the external sibling repo only).
- [x] Scaffold `libposhanka` + `poshanka` with tracing in daemon binary.
- [x] Scaffold **`poshankactl/`** crate (stub `main`, workspace member).
- [x] `libposhanka`: connect Wayland, bind layer shell, show **one** solid-color overlay rect (theme background) — no text.
- [x] `poshanka`: load minimal `config.toml` / `theme.toml` (font + colors only); exit with structured error on missing files if strict.
- [x] Populate **`deny.toml`** license allow list for Wayland stack crates (cairo/pango added in Phase 2).

**Verify**: all gates in §7; `rg -i 'abar|libabar' --glob '!docs/PLAN.md'` empty; manual run on Hyprland (or any layer-shell compositor).

### Phase 1 — Config + theme + runtime spec

- [x] Serde models matching `examples/config.toml`, `examples/theme.toml`, and fragment overrides (`[override]`, `[paths].overrides`, `[events]`, theme tables).
- [x] Load + merge override fragments; resolve `theme` paths relative to config directory.
- [ ] XDG path resolution + `--config` / `--theme` (`clap`).
- [ ] `Settings::resolve` → `DaemonSpec` + `CardStyle` plain structs for `libposhanka` (include resolved `[events]` per matched override).

**Verify**: unit tests deserialize all `examples/**` configs/themes; merge smoke tests; no Wayland required.

### Phase 2 — Render core (Cairo + Pango)

- [ ] Implement `color`, `render/font`, rounded rect, BGRA buffer (port from [abar](https://github.com/Gigas002/abar) if useful).
- [ ] `measure_card` / `paint_card` with summary + body only (placeholder icon).
- [ ] Headless tests: non-transparent pixels in card bbox; text layout sanity.

**Verify**: `libposhanka` render tests pass without compositor.

### Phase 3 — Wayland surfaces + card click (pre–D-Bus)

- [ ] One layer surface per notification card (or documented alternative).
- [ ] SHM buffer resize on configure; paint via Phase 2.
- [ ] Pointer: whole-card click → dismiss only (queue removes, surface destroyed); `ActionInvoked` wired in Phase 5.
- [ ] Wakeup pipe + `poll` loop (nonblocking Wayland dispatch).

**Verify**: manual show/hide with a test harness calling `libposhanka` directly (pre-D-Bus).

### Phase 4 — D-Bus server + queue (Notify path)

- [ ] `dbus/notifications/`: request name `org.freedesktop.Notifications`, register object, implement `Notify`, `GetCapabilities`, `GetServerInformation`.
- [ ] `queue/`: assign ids, stack ordering, `replaces_id`.
- [ ] `Notify` → enqueue → create surface → paint.
- [ ] `CloseNotification` + emit `NotificationClosed`.
- [ ] `Ping` on control interface only (proves bus registration + client path); full control methods in Phase 4b.

**Verify**: `dbus-send` / `notify-send` manual test; unit tests for id/replace logic; `poshankactl ping` succeeds while daemon runs.

### Phase 4b — Control plane + `poshankactl` crate (makoctl parity)

- [ ] `libposhanka/src/dbus/control/`: implement `org.poshanka.Daemon1` server (`CloseAll`, `Close`, `Reload`, `Pause`, `Unpause`).
- [ ] Map control calls to `mpsc` commands handled on Wayland thread (same as `Notify`).
- [ ] **`poshankactl/`** crate: `ping`, `reload`, `close-all`, `close <id>`, `pause`, `unpause` via zbus proxy (`poshankactl/src/dbus/`, `poshankactl/src/cli/`).
- [ ] Second **`poshanka`** instance: fail fast if bus name already owned (clear error message). Control traffic uses **`poshankactl`** only, not `poshanka`.

**Verify**: integration test with zbus test bus or documented manual script; `poshankactl reload` picks up theme change without restart.

### Phase 5 — Client actions (protocol only; no button UI, ever)

- [ ] Parse and store `actions` from `Notify` (for `ActionInvoked` only — **no** action button rendering, now or later).
- [ ] Card click: run optional `[events]` shell for that button, then **always** emit `ActionInvoked` when actions exist (§3.2); pick action key (`default` preferred).
- [ ] `on_notify` → spawn shell when notification is shown.
- [ ] Dismiss after handling; correct `NotificationClosed` / `ActionInvoked` ordering for clients (e.g. wayshot).

**Verify**: `notify-send` / wayshot with `--action`; `dbus-monitor` shows `ActionInvoked`; optional `[events]` shell runs before signal.

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

### Post-first-release (optional, mako-aligned only)

- [ ] **Progress** rendering (`value` hint + theme `[progress]` `over`/`source`) if not finished in v0.
- [ ] **body-markup** only if we match mako’s markup handling and advertise the capability.
- [ ] Richer **override criteria** (mako-style `category`, `desktop-entry`, …) — still fragment-based, not dunst scripts.
- [ ] **Modes** / DND-style visibility (mako `makoctl mode` parity) if needed.
- [ ] Single-surface stack optimization if profiling warrants it.

Do **not** add post-v1 items for dunst history, inhibition, `image-data` body images, or sound daemon unless mako gains them first.

---

## 9. Definition of done (v0 / first working draft)

- [ ] `notify-send` displays stacked notifications on Wayland with theme from `examples/theme.toml`.
- [ ] Placement, gaps, and `max_visible` behave per config.
- [ ] Urgency colors and timeouts work; persistent notifications (`timeout = -1`) stay until dismissed.
- [ ] Card tap emits `ActionInvoked` when client sent actions (§3.2); `[events]` shell is additive; dismiss emits `NotificationClosed` with correct reason codes.
- [ ] **No** GTK/iced; Cairo+Pango path is live.
- [ ] **zbus** only for session D-Bus (notification + control); no libdbus; no v0 Unix control socket.
- [ ] **`poshankactl`** works against a running daemon (`reload`, `close-all`, `pause`, …).
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
| 2026-06-02 | §3: `examples/` config system — override fragments, `[events]`, theme tables, action buttons not planned (card tap + `ActionInvoked` only), no user `[server]`/`[capabilities]`/hint params |
| 2026-06-02 | §1.5: mako as primary behavioral reference; dunst secondary |
| 2026-06-02 | §5.4 / Phase 4b: **`poshankactl`** crate (makoctl parity), third workspace member beside **`poshanka`** / **`libposhanka`** |
