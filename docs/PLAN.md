# poshanka — implementation plan

This document is the **human roadmap** and **agent playbook** for **poshanka**: a **Wayland popup renderer** for the [notred](https://github.com/Gigas002/notred) notification platform — **behaviorally inspired mainly by [mako](https://github.com/emersion/mako)** (with [dunst](https://dunst-project.org/) as a secondary reference where overlap is small), using **Cairo + Pango** for drawing, **no** heavyweight UI toolkits, and **no** in-process notification queue or Freedesktop D-Bus server.

**poshanka does not own `org.freedesktop.Notifications`.** [notred](https://github.com/Gigas002/notred) is the session host (FDN + queue + timeouts + optional history). poshanka is an **external subscriber** that paints corner cards and forwards user input via **`notredctl`** only — the same integration model [notred](https://github.com/Gigas002/notred) uses for [notred-tui](https://github.com/Gigas002/notred) and third-party clients.

**Workspace structure, settings flow, testing, features, dependencies, and quality gates** are defined in [ARCHITECTURE.md](./ARCHITECTURE.md). Follow that document for every change; this plan covers **product behavior, crate split, config schema, notredctl integration, and phased delivery** only.

**Integration references:**

| Project                                      | Role for poshanka                                                                                                                                                                                                        |
| -------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| [notred](https://github.com/Gigas002/notred) | FDN daemon, queue, timeouts, `[events]` shell, `ActionInvoked` / `NotificationClosed`                                                                                                                                    |
| **`notredctl`**                              | **Only supported connector** — `subscribe`, `list`, `close`, `activate`, `reload`, … ([IPC.md](https://github.com/Gigas002/notred/blob/master/docs/IPC.md) is for ctl/daemon authors; UI authors use `notredctl --help`) |
| [abar](https://github.com/Gigas002/abar)     | **Exec-handler pattern** for streaming external state — see [tray.sh](https://github.com/Gigas002/abar/blob/master/examples/scripts/tray/tray.sh) (`trayctl subscribe` loop → JSON lines on stdout)                      |

This repo must **not** vendor or depend on `notred` / `libnotred`, `abar` / `libabar`, or open `notred.sock` directly. Copy **patterns** into `libposhanka`; spawn **`notredctl`** (or a user script that only wraps `notredctl`).

### abar / trayd coexistence (mandatory model)

poshanka and notred must split work **exactly like** abar and trayd: independent repos, zero Cargo coupling, integration only through a **connector CLI** and optional wrapper scripts.

| Concern           | abar + trayd                                         | poshanka + notred                                           |
| ----------------- | ---------------------------------------------------- | ----------------------------------------------------------- |
| Daemon            | trayd (SNI state)                                    | notred (FDN + queue)                                        |
| Connector         | trayctl                                              | notredctl                                                   |
| Pixels            | abar                                                 | poshanka                                                    |
| State into UI     | `trayctl subscribe` → `tray.sh` → abar exec          | `notredctl subscribe` → `notred-subscribe.sh` → poshanka    |
| Swapping provider | Replace trayd + trayctl; keep abar + script contract | Replace notred + notredctl; keep poshanka + NDJSON contract |

**Where behavior differs from abar (on purpose):** abar runs click scripts from **abar config** (`[tray].on_left_click` → `tray-menu.sh`). poshanka **must not** run dismiss/activate/hook scripts from poshanka config. Pointer gestures → **`notredctl input <id> <event_kind>`** (or v0 whole-card `activate` / `close` shortcuts). **`[events].on_button_left` and friends live in notred config** — see [notred PLAN §5.2](https://github.com/Gigas002/notred/blob/master/docs/PLAN.md#52-events-hooks-and-override-fragments).

**poshanka is a dumb view:** paint cards, diff subscribe snapshots, forward `id` + gesture kind. Policy, hooks, FDN signals = **notred only**.

**Reference configs (source of truth for poshanka schemas — update examples first, then this doc):**

- `examples/config.toml` — placement, stack layout, layer shell, `[provider]` wiring.
- `examples/theme.toml` — base visual theme; fragments patch tables (e.g. `examples/urgency/*/theme.toml`).
- `examples/apps/<name>/theme.toml` — optional `[override]` fragments (app or urgency), visual only.

**notred config** (`$XDG_CONFIG_HOME/notred/notred.toml`) owns queue policy, timeouts, and `[events]` — not poshanka. **Fragment paths mirror poshanka:** `apps/some_app/config.toml` with nested `urgency/*/config.toml` for per-app behavior overrides (see [notred PLAN §5.2](https://github.com/Gigas002/notred/blob/master/docs/PLAN.md#52-events-hooks-and-override-fragments-poshanka-parity-layout)).

---

## 1. Goals and constraints

### 1.1 Goals

- **Wayland-native popups**: `zwlr_layer_shell_v1` overlay surfaces; anchor, margins, keyboard interactivity `none`, fractional scale / buffer scale where supported.
- **Cairo + Pango**: measure and paint notification **cards** on an **image buffer** (shm) per surface (`cairo-rs`, `pango`, `pangocairo`); gtk-rs stack versions aligned within one minor.
- **Mako-like UX**: corner stack, gap, urgency-driven look, tap-to-dismiss or whole-card activation — **behavioral** reference, not a mako/dunst config clone. **Action buttons are never drawn** — gestures reported to notred; hooks run in **notred**, not poshanka.
- **notredctl subscriber**: live state from `notredctl subscribe` (NDJSON events on stdout); mutations via `notredctl close`, `activate`, etc. Reconnect loop like [abar `tray.sh`](https://github.com/Gigas002/abar/blob/master/examples/scripts/tray/tray.sh).
- **Config discovery**: XDG-style `$XDG_CONFIG_HOME/poshanka/config.toml`, theme from `paths.theme`, plus `--config` on the binary.
- **Deploy model**: user runs **`notred`** (systemd user unit or session autostart) **and** **`poshanka`** (graphical subscriber). Control plane for operators: **`notredctl`**, not a poshanka-specific ctl binary.

### 1.2 Crate split and runtime (poshanka-specific)

- **`libposhanka`** — notification view model (from provider feed NDJSON), render, Wayland surfaces, provider child-process I/O (feed script + optional one-shot command). **No** `clap`, **no** `toml`, **no** `zbus`, **no** FDN server. **No provider names in code** — same rule as `libabar` never mentions trayd.
- **`poshanka`** — binary: config/theme, `Settings`, run loop; wires `libposhanka` only.
- **No `poshankactl`** — removed from scope; use upstream **`notredctl`**.

**Threading:** `notredctl subscribe` runs as a **child process** with stdout parsed on a dedicated reader thread (or async task that signals the Wayland thread). One-shot `notredctl close` / `activate` via `std::process::Command` — never block the Wayland `poll` loop. The Wayland client loop stays **synchronous** on the main thread (`poll` + nonblocking dispatch, wakeup `UnixStream`).

**Step sizing:** small PR-sized phases with explicit **Verify** blocks (quality gates per [ARCHITECTURE.md §8](./ARCHITECTURE.md#8-quality-gates--required-before-every-commit)).

### 1.3 Non-goals / deferred

- **No** FDN D-Bus server, queue, or timeout engine in this repo — [notred](https://github.com/Gigas002/notred).
- **No** `poshankactl`, custom Unix socket client, or `libnotred` dependency.
- **No** `[events]` or `on_button_*` in poshanka config — ever (notred owns hooks).
- **No** GTK/Qt/iced notification applets; **no** full notification center / history browser (use [notred-tui](https://github.com/Gigas002/notred) + `notredctl list-history` when history is enabled).
- **No** pixel-perfect mako clone; deliberate divergences in §1.5.
- **No** dunst-only features mako does not have — history UI, inhibition, inline `image-data`, daemon sound, dunst rule scripts.
- **Deferred (mako parity, pixels or subscriber-only):** icons (Phase 5), **`[progress]`** bar (theme schema exists), optional **body markup** if advertised jointly with notred, richer **override criteria** for theme fragments.

### 1.4 Definitions

- **Notification (view)**: one item from a `notredctl` `update` event / `list` snapshot (`MinimalNotification` shape in [notred IPC](https://github.com/Gigas002/notred/blob/master/docs/IPC.md)).
- **Card**: rendered representation on a Wayland surface. **No action button row, ever** — whole-card or per-region pointer → `notredctl` (not local shell).
- **Stack**: ordered visible cards at a screen corner; membership and dismissals come from **notred** via subscribe snapshots.
- **Surface strategy (v0)**: **one layer-shell surface per notification** for hit-testing; revisit single-surface stack if compositor overhead hurts.
- **IPC**: poshanka talks to **notred** only through **`notredctl`** subprocesses — never `notred.sock`, never session D-Bus for notifications.

### 1.5 Behavioral reference (mako primary, dunst secondary)

| Area         | Follow **mako**                                         | Notes                                                                  |
| ------------ | ------------------------------------------------------- | ---------------------------------------------------------------------- |
| Platform     | Wayland layer-shell popups                              | FDN lives in **notred**, not poshanka                                  |
| Config shape | Global theme + **criteria/override fragments** for look | Behavior/timeouts/`[events]` in **notred** config                      |
| Interaction  | Tap card to dismiss or activate                         | poshanka → `notredctl input` / shortcuts; notred runs `[events]` + FDN |
| Look         | Theme tables (colors, layout, Pango templates)          | poshanka `examples/theme.toml`                                         |
| Progress     | `over` / `source` bar                                   | deferred; data from notred hints when wired                            |
| Actions UI   | Mako _can_ show buttons; **poshanka never does**        | `notredctl input` / `activate` — hooks in **notred**                   |
| Control CLI  | **`notredctl`** (`reload`, `close-all`, `pause`, …)     | not a poshanka binary                                                  |

---

## 2. Crate layout (poshanka-specific)

Generic workspace layout: [ARCHITECTURE.md §2](./ARCHITECTURE.md#2-repository-layout).

```text
libposhanka/
  src/
    model/           # view types mapped from provider feed JSON
    feed/            # parse NDJSON from feed script stdout (no provider names)
    render/          # cairo+pango: measure card, paint card
    icon/            # icon hint from JSON (Phase 5)
    wayland/         # layer_shell, per-notification surfaces, pointer
poshanka/
  src/
    cli/             # --config
    config/          # poshanka TOML only (visual + placement + [provider] wiring)
    theme/
    settings/
    logger/
    app/             # Settings → libposhanka::run_subscriber
examples/
  scripts/
    notred-subscribe.sh   # reference script — names notredctl here only
  feed-fixtures/          # golden NDJSON lines for feed parser tests
```

**Crate boundaries**

- `libposhanka`: render + Wayland + feed NDJSON parsing; **zero** provider/daemon names in source.
- `poshanka`: TOML, `Settings`, subscriber entrypoint; `[provider].exec` / `.command` point at user scripts/CLIs.
- External runtime: whatever your `[provider]` script wraps (e.g. [notred](https://github.com/Gigas002/notred) + `notredctl` in `examples/`).

**Feature passthrough:** optional `icons`, `markup`, etc. on `libposhanka`; `poshanka` binary passes features through. **No `dbus` feature** — D-Bus is notred's concern.

---

## 3. Config split: poshanka vs notred

### 3.1 What lives where

| Concern                                     | Owner        | Config                                                                                                                                           |
| ------------------------------------------- | ------------ | ------------------------------------------------------------------------------------------------------------------------------------------------ |
| FDN, queue, `max_visible`, timeouts, pause  | **notred**   | `$XDG_CONFIG_HOME/notred/notred.toml`                                                                                                            |
| `[events]` shell, `ActionInvoked` ordering  | **notred**   | same                                                                                                                                             |
| Override fragments (behavior)               | **notred**   | `paths.overrides` — **same directory tree as poshanka** (`apps/<name>/config.toml`, nested `urgency/*/config.toml`); `[events]` instead of theme |
| Placement, gap, layer-shell anchor/layer    | **poshanka** | `examples/config.toml`                                                                                                                           |
| Card look (font, colors, layout, templates) | **poshanka** | `examples/theme.toml` + fragments                                                                                                                |
| Visual override per app/urgency             | **poshanka** | `examples/apps/*/theme.toml`, `examples/urgency/*/theme.toml`                                                                                    |
| Provider feed script / one-shot CLI path    | **poshanka** | `[provider]` section                                                                                                                             |

**Rule:** if it affects **D-Bus apps** (timeout, dismiss reason, capabilities, signals), it belongs in **notred**. If it affects **pixels only**, it belongs in **poshanka**.

### 3.2 poshanka `config.toml`

| Section       | Role                                                                                                                                                   |
| ------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `[paths]`     | `theme`; `overrides` — ordered theme fragment paths (relative to config dir)                                                                           |
| `[provider]`  | `exec` — long-running feed script (abar `[tray].exec` analogue); optional `command` for one-shot RPC CLI; optional `socket` forwarded by script/binary |
| `[stack]`     | `gap`, visual stacking policy for surfaces poshanka paints (not queue cap — that is notred)                                                            |
| `[placement]` | `anchor`, `margin`                                                                                                                                     |
| `[layer]`     | layer-shell `layer`, optional `output`                                                                                                                 |

**`[override]`** (in theme fragments only)

- `type` — `app` \| `urgency`
- `name` / `level` — match `app_id` / urgency from subscribe JSON

Merge policy: **first matching `[override]` wins** for app/urgency; fragment theme keys replace same keys in merged theme.

### 3.3 `theme.toml`

| Section      | Keys (representative)                                                             |
| ------------ | --------------------------------------------------------------------------------- |
| `[font]`     | `name`, `size`                                                                    |
| `[colors]`   | `background`, `foreground`, `border`, `progress`                                  |
| `[layout]`   | `width`, `height` (max), `padding`, `margin`                                      |
| `[border]`   | `size`, `radius`                                                                  |
| `[text]`     | `alignment`; `summary`, `body`, … — Pango templates with `{summary}`, `{body}`, … |
| `[icons]`    | `size`, `position`, `theme`                                                       |
| `[progress]` | `mode` — `over` \| `source` (deferred)                                            |

### 3.4 notredctl JSON → internal view

Map fields from `notredctl list` / `subscribe` `update` items (see [notred IPC](https://github.com/Gigas002/notred/blob/master/docs/IPC.md)):

| JSON field                  | poshanka use                                | v0      |
| --------------------------- | ------------------------------------------- | ------- |
| `id`                        | surface key, `notredctl close` / `activate` | yes     |
| `app_id`, `summary`, `body` | text templates                              | yes     |
| `urgency`                   | theme override + colors                     | yes     |
| `timeout_ms`                | display only (timer in notred)              | yes     |
| `icon`                      | icon column                                 | Phase 5 |
| `has_actions`               | whole-card tap → `activate` vs `close`      | Phase 4 |

Do not re-parse raw FDN hints in poshanka — notred normalizes payloads for subscribers.

---

## 4. Rendering and UI

(Unchanged from prior plan — pixels only.)

### 4.1 Cairo + Pango pipeline

- **Measure**: summary + body from `[text]` templates; optional icon. **No action row.**
- **Draw**: rounded rect, border, Pango wrap/ellipsis.
- **Buffer**: BGRA (document once in code).
- **Upload**: `wl_shm` per surface; full redraw acceptable for v0.

### 4.2 Card layout

```text
┌─────────────────────────────────────┐
│ [icon]  Summary                     │
│         Body (wrapped)              │
└─────────────────────────────────────┘
     entire card = one click target (v0)
```

### 4.3 Stack placement

- Position from output geometry + `placement.anchor` + cumulative card heights + `[stack].gap`.
- On subscribe `update`: diff ids → create/destroy/reposition surfaces.

---

## 5. notredctl integration (abar exec-handler pattern)

### 5.1 Architecture

```text
Apps ──FDN──► notred ◄──socket── notredctl ◄──spawn── poshanka
                    │                              │
                    │                              └── Wayland cards
                    └── queue, timeouts, signals
```

### 5.2 Subscribe feed (like abar `[tray].exec`)

[abar](https://github.com/Gigas002/abar) tray module runs a long-lived script that streams JSON:

```bash
# examples/scripts/notred-subscribe.sh — reference wrapper
while true; do
    notredctl subscribe
    sleep 3
done
```

poshanka either:

1. Spawns `[provider].exec` from config (abar `[tray].exec` style), **or**
2. Spawns the provider subscribe command directly with the same reconnect loop in Rust.

**Contract:** one NDJSON object per line on child stdout. Handle `{"type":"event","event":{"kind":"update","items":[…]}}` — refresh local view from `items`. On `reload` event, re-read poshanka config/theme from disk (provider config reload is a separate operator command).

**Initial sync:** on startup, run `[provider].command list` once before or after subscribe attaches, so cards appear even if no event fired yet.

### 5.3 User actions (poshanka → notredctl only)

poshanka receives pointer events on its Wayland surfaces; **notred never sees raw Wayland events**. poshanka sends **notification `id` + event kind** (or semantic shortcuts below) — **never** shell commands, hook argv, app-specific policy, or `[events]` text.

**Two layers (both end in notred):**

| Layer                           | When                                       | poshanka sends                            | notred does                                                        |
| ------------------------------- | ------------------------------------------ | ----------------------------------------- | ------------------------------------------------------------------ |
| **A — Gesture report** (target) | Per-button / touch / any pointer binding   | `notredctl input <id> <event_kind>`       | Match `[events].on_button_*` / `on_touch`; default policy if unset |
| **B — v0 whole-card shortcut**  | Single hit target per card (Phase 5 early) | `notredctl activate <id>` or `close <id>` | Same as notred-tui: semantic RPC + `on_action` where applicable    |

Layer **B** is a **poshanka convenience** for cards without per-button regions — not a second config system. Layer **A** is required for mako/dunst-style `on_button_left` parity — **landed in notred** ([Phase 6](https://github.com/Gigas002/notred/blob/master/docs/PLAN.md#phase-6--subscriber-input-events--events-hooks), merged).

| User gesture                                        | Command                             | Notes                                      |
| --------------------------------------------------- | ----------------------------------- | ------------------------------------------ |
| Primary tap whole card, `has_actions` (v0 shortcut) | `notredctl activate <id> [key]`     | Layer B; prefer `default` key              |
| Primary tap whole card, no actions (v0 shortcut)    | `notredctl close <id>`              | Layer B; dismiss                           |
| Left / middle / right / touch on card               | `notredctl input <id> <event_kind>` | Layer A — **preferred** once Phase 5 lands |
| (none in poshanka)                                  | `notredctl reload`                  | operator / keybind via shell               |
| (none in poshanka)                                  | `notredctl pause` / `unpause`       | operator                                   |

**Event kinds** (wire protocol — align with notred IPC): `button_left`, `button_middle`, `button_right`, `touch`. Map from Wayland `wl_pointer` button events in `libposhanka/src/wayland/`. Do **not** invent aliases like `left_button_click` in poshanka — use the enum notred documents.

**Forbidden in poshanka:**

- Reading `$XDG_CONFIG_HOME/notred/` for hook scripts
- Spawning user shell on click (abar `on_left_click` pattern)
- Emitting D-Bus `ActionInvoked` / `NotificationClosed` directly

**Do not** run `[events]` shell from poshanka — notred owns that pipeline.

### 5.4 Process model

| Process         | Role                                                                                       |
| --------------- | ------------------------------------------------------------------------------------------ |
| **`notred`**    | Single FDN owner; must be running before poshanka shows cards                              |
| **`poshanka`**  | Subscriber UI; exits if subscribe child dies permanently (configurable retry like tray.sh) |
| **`notredctl`** | Stateless CLI; poshanka spawns per command + one long-lived subscribe child                |

**Single instance:** notred enforces FDN bus name. Multiple **poshanka** instances are undefined — document “one graphical subscriber per session” for v0.

### 5.5 Wayland

- `wayland-client`, `wayland-protocols-wlr` (`wlr-layer-shell-unstable-v1`).
- Layer: **overlay**; anchor from config; keyboard interactivity **none**.
- Pointer: map seat events → provider command per §5.3. Ship **Layer B** whole-card shortcuts first, then **Layer A** per-button `input` (notred upstream ready).
- Seat: pointer required for v0.

### 5.6 Upstream — notred Phase 6 (`input` RPC) ✅ merged

**Done in [notred](https://github.com/Gigas002/notred)** (Phases 5–6 on `master`). poshanka is unblocked for Phase 5 pointer work.

- [x] **`notredctl input <id> <event_kind>`** — CLI + socket IPC; golden fixtures in `examples/ipc-examples/`.
- [x] **Daemon handler** — override fragments; `on_button_left`, `on_button_middle`, `on_button_right`, `on_touch`.
- [x] **Precedence rules** — `input` vs `activate` / `close` shortcuts ([IPC.md](https://github.com/Gigas002/notred/blob/master/docs/IPC.md)).
- [x] **FDN side effects** — correct signals when hooks or default policy dismiss/activate.

**poshanka next:** Phase 5 — Wayland pointer → `[provider].command` (`close` / `activate` / `input`).

---

## 6. Module catalog (`libposhanka`)

| Module    | Responsibility                                                          |
| --------- | ----------------------------------------------------------------------- |
| `model`   | `NotificationView`, `Urgency`, `CardStyle`, map from feed JSON          |
| `feed`    | Parse NDJSON lines from feed script stdout (`FeedMessage`, `FeedEvent`) |
| `render`  | `measure_card`, `paint_card` → pixel buffer                             |
| `wayland` | Globals, surfaces, SHM, pointer → provider command spawn                |
| `icon`    | Resolve `icon` from JSON (Phase 5)                                      |

---

## 7. CI notes (poshanka-specific)

Quality gates: [ARCHITECTURE.md §6–§8](./ARCHITECTURE.md#6-testing-and-coverage).

- Workspace members: **`libposhanka`**, **`poshanka`** only.
- **libcairo2-dev**, **libpango1.0-dev** for render tests.
- **notred** + **notredctl** required for integration/manual tests with the reference script — install from [notred](https://github.com/Gigas002/notred) or CI services block; unit tests mock NDJSON fixtures from `examples/feed-fixtures/`.
- Headless: queue diff, JSON parse, render pixels — no compositor, no live notred.

---

## 8. Phased steps

**Status (2026-07-23):** Phases **0–4** and **1b** done. **Phase 5** is next (pointer input). No upstream blockers.

### Phase 0 — Workspace + hygiene + empty vertical slice ✅

Completed under the **pre-notred** plan (three crates, Wayland color rect, config/theme load).

### Phase 1 — Config + theme + runtime spec ✅

Completed: serde for `examples/**`, override merge, `Settings` → `CardStyle` (runtime spec renamed to `SubscriberSpec` in Phase 1b).

### Phase 1b — notred pivot (migration) ✅

- [x] Remove **`poshankactl/`** from workspace; delete crate tree.
- [x] Update root `Cargo.toml` description (“Wayland subscriber for notred”).
- [x] Add `examples/scripts/notred-subscribe.sh` (tray.sh pattern).
- [x] Add `examples/feed-fixtures/*.jsonl` golden lines for subscribe/list parsing tests.
- [x] `libposhanka/src/feed/` — generic NDJSON parser; **no notred/trayd names in lib code**.
- [x] Config `[provider]` with `exec` / `command` / `socket` (notred only in `examples/scripts/notred-subscribe.sh`).
- [x] Document two-process setup in README sketch: `notred` + `poshanka`.

**Verify**: workspace builds with two members; `rg poshankactl` / `zbus` clean; fixture parse tests green.

### Phase 2 — Render core (Cairo + Pango) ✅

- [x] `color`, `render/font`, rounded rect, BGRA buffer (ported from [abar](https://github.com/Gigas002/abar)).
- [x] `measure_card` / `paint_card` with summary + body (placeholder icon).
- [x] Headless render tests.

**Verify**: `libposhanka` render tests without compositor or provider daemon.

### Phase 3 — Provider feed subscriber loop ✅

- [x] Spawn `[provider].exec` child; parse NDJSON from stdout; reconnect with backoff (abar exec-handler pattern).
- [x] One-shot `[provider].command list` on startup; map JSON → `model::NotificationView` (wire NDJSON + CLI JSON array).
- [x] `feed/` helpers for one-shot RPC spawn (`close`, `activate`, `input`, …) via `[provider].command` + optional `socket`.
- [x] Unit tests with golden fixtures; `#[ignore]` integration test in `libposhanka/tests/live_provider.rs`.

**Verify**: manual — notred running, poshanka logs `initial provider list` / `feed update` on `notify-send`.

### Phase 4 — Wayland surfaces + stack sync ✅

- [x] One layer-shell surface per notification `id` from subscribe snapshots.
- [x] SHM resize on configure; paint via Phase 2 `paint_card`.
- [x] Diff `update` items → create / destroy / reposition surfaces; `[stack].gap` + `placement`.
- [x] Wakeup pipe + `poll` loop; reload event re-reads poshanka config/theme.

**Verify**: manual on Hyprland — `notify-send` shows themed card via notred + poshanka. (Automated: `libposhanka/src/wayland/tests.rs` covers corner/margin/stack-offset geometry; `poshanka/src/app/tests.rs` covers override-merged `style_for` + `reload` against disk fixtures.)

**Notes:**

- `libposhanka::wayland::StyleSource` is the seam between the (provider-agnostic) Wayland card stack and poshanka's TOML theme/override merging — `poshanka::app::style::OverrideStyleSource` implements it, re-reading config/theme/overrides from disk on `reload()`.
- `[layer].output` is parsed into `SubscriberSpec` but not yet bound to a specific `wl_output`; surfaces are created without an explicit output (compositor default). Deferred — not required by this phase's checklist.
- Phase 0's single solid-color `OverlaySpec` overlay is removed; the card stack now always paints real notification content.

### Phase 5 — Pointer input (Layer B + Layer A)

**Upstream ready** — notred `input` RPC merged. Deliver in two slices if useful; both use `[provider].command` only.

**5a — Whole-card shortcuts (Layer B)**

- [ ] `wl_pointer` on card surfaces; single hit region per card.
- [ ] Primary tap → `command close <id>` or `activate <id>` per `has_actions` (non-blocking spawn).
- [ ] Surfaces removed when `id` absent from next `update`.

**5b — Per-gesture `input` (Layer A)**

- [ ] Map pointer button events → `command input <id> button_left|button_middle|button_right|touch`.
- [ ] Right/middle click runs `on_button_*` from **notred** config — verify with hook in notred `examples/`.

**Verify (5a):** tap dismisses; `notify-send --action` + tap → `ActionInvoked` from **notred** only.

**Verify (5b):** `on_button_right` hook in notred config fires on right-click; poshanka config unchanged.

### Phase 6 — Icons

- [ ] `icon/`: use `icon.name` / `icon.path` from JSON; PNG → Cairo.
- [ ] Feature `icons` (default on for binary).

**Verify**: `notify-send -i`; fixture tests.

### Phase 7 — Polish + first release

- [ ] README: install **notred** + **notredctl**, poshanka config paths, `notify-send` smoke test.
- [ ] CHANGELOG; tag **v0.1.0**.
- [ ] Optional: sample systemd user units (notred from upstream + poshanka).

**Verify**: [ARCHITECTURE.md §8](./ARCHITECTURE.md#8-quality-gates--required-before-every-commit) gates; dogfood with common apps.

### Post-first-release (optional)

- [ ] Progress bar (`value` hint via notred JSON when available).
- [ ] body-markup (coordinate capability with notred).
- [ ] Richer theme override criteria (`category`, `desktop-entry`, …).
- [ ] Single-surface stack optimization.

---

## 9. Definition of done (v0)

- [ ] **`notred`** owns FDN; **`notify-send`** reaches cards when **`poshanka`** is running.
- [ ] Theme from `examples/theme.toml`; placement and gap per poshanka config.
- [ ] Subscribe feed via `[provider].exec` / `command` only (reference script wraps notredctl).
- [ ] Card tap → provider `activate` / `close` (5a) and/or `input` (5b); FDN signals originate from **notred** only.
- [ ] **No** `[events]` or click hooks in poshanka config.
- [ ] **No** GTK/iced; Cairo+Pango path live.
- [x] **No** `poshankactl`, **no** `zbus`, **no** `notred.sock` in poshanka.
- [ ] CI green per [ARCHITECTURE.md §8](./ARCHITECTURE.md#8-quality-gates--required-before-every-commit).

---

## 10. Stack dependencies (poshanka-specific)

Generic policy: [ARCHITECTURE.md §7](./ARCHITECTURE.md#7-dependencies).

| Area              | Crates / notes                                                      |
| ----------------- | ------------------------------------------------------------------- |
| Graphics          | `cairo-rs`, `pango`, `pangocairo` — one gtk-rs minor                |
| Wayland           | `wayland-client`, `wayland-protocols-wlr`                           |
| JSON              | `serde`, `serde_json` — parse notredctl stdout                      |
| External binaries | **`notred`**, **`notredctl`** — not Cargo deps; required at runtime |

---

## 11. Pattern checklist (abar tray → poshanka notred)

| abar / trayd concern                  | poshanka / notred analogue                                | Owner                             |
| ------------------------------------- | --------------------------------------------------------- | --------------------------------- |
| `[tray].exec` long-lived script       | `[provider].exec` (e.g. `notred-subscribe.sh`)            | poshanka spawns                   |
| `trayctl subscribe` stdout JSON       | provider subscribe NDJSON → `feed/` parser                | poshanka parses                   |
| `[tray].on_left_click` → shell script | **`[events].on_button_left` in notred config**            | **notred** runs hook              |
| Tray click → `trayctl` RPC            | Card gesture → `command input` / `activate` / `close`     | poshanka sends; provider executes |
| `feed_id` appends context to handler  | `id` + `event_kind` on `input` RPC                        | notred resolves context           |
| Hex RGBA → buffer                     | `libposhanka/src/color/`                                  | poshanka                          |
| Font / rounded rects                  | `libposhanka/src/render/`                                 | poshanka                          |
| SHM lifecycle                         | `libposhanka/src/wayland/`                                | poshanka                          |
| Settings boundary                     | `poshanka/src/settings/` → `SubscriberSpec` + `CardStyle` | poshanka                          |
| Poll + wakeup                         | `libposhanka/src/wayland/`                                | poshanka                          |
| Queue / timeouts / FDN                | (no abar equivalent)                                      | **notred** only                   |

**Never** add `libnotred`, `libabar`, or a custom socket client as dependencies. **Never** put `on_button_*` or `[events]` in poshanka TOML.

---

## 12. Document maintenance

Update this plan when subscriber behavior, config schema, or notredctl command usage changes. **Mark completed phase steps with `[x]` and a ✅ on the phase heading.** Update [ARCHITECTURE.md](./ARCHITECTURE.md) for workspace-wide conventions. For poshanka config: `examples/*.toml` first, then this doc. For FDN/queue/timeouts: [notred](https://github.com/Gigas002/notred) docs only.

---

## Revision history

| Date       | Change                                                                                                                   |
| ---------- | ------------------------------------------------------------------------------------------------------------------------ |
| 2026-05-18 | Initial poshanka plan (monolithic daemon model)                                                                          |
| 2026-06-02 | `examples/` config system; mako primary reference                                                                        |
| 2026-07-03 | Trim duplication; [ARCHITECTURE.md](./ARCHITECTURE.md) structural source of truth                                        |
| 2026-07-03 | **notred pivot:** poshanka = Wayland subscriber via **`notredctl`**; drop FDN/`poshankactl`; abar tray exec pattern      |
| 2026-07-03 | **abar/trayd coexistence** § intro; Layer A/B input model; events **never** in poshanka config                           |
| 2026-07-03 | **Phase 2 ✅** render core; **generic `[provider]`** in libposhanka (no notred in lib code)                              |
| 2026-07-03 | **notred merged** (Phases 5–6): unblock poshanka Phase 5 pointer; merge 5a/5b under single phase                         |
| 2026-07-23 | **Phase 4 ✅** per-notification layer-shell surfaces + stack sync; `wayland::StyleSource` seam for theme/override reload |
