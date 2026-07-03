# poshanka — implementation plan

This document is the **human roadmap** and **agent playbook** for **poshanka**: a **Wayland popup renderer** for the [notred](https://github.com/Gigas002/notred) notification platform — **behaviorally inspired mainly by [mako](https://github.com/emersion/mako)** (with [dunst](https://dunst-project.org/) as a secondary reference where overlap is small), using **Cairo + Pango** for drawing, **no** heavyweight UI toolkits, and **no** in-process notification queue or Freedesktop D-Bus server.

**poshanka does not own `org.freedesktop.Notifications`.** [notred](https://github.com/Gigas002/notred) is the session host (FDN + queue + timeouts + optional history). poshanka is an **external subscriber** that paints corner cards and forwards user input via **`notredctl`** only — the same integration model [notred](https://github.com/Gigas002/notred) uses for [notred-tui](https://github.com/Gigas002/notred) and third-party clients.

**Workspace structure, settings flow, testing, features, dependencies, and quality gates** are defined in [ARCHITECTURE.md](./ARCHITECTURE.md). Follow that document for every change; this plan covers **product behavior, crate split, config schema, notredctl integration, and phased delivery** only.

**Integration references:**

| Project | Role for poshanka |
| ------- | ----------------- |
| [notred](https://github.com/Gigas002/notred) | FDN daemon, queue, timeouts, `[events]` shell, `ActionInvoked` / `NotificationClosed` |
| **`notredctl`** | **Only supported connector** — `subscribe`, `list`, `close`, `activate`, `reload`, … ([IPC.md](https://github.com/Gigas002/notred/blob/master/docs/IPC.md) is for ctl/daemon authors; UI authors use `notredctl --help`) |
| [abar](https://github.com/Gigas002/abar) | **Exec-handler pattern** for streaming external state — see [tray.sh](https://github.com/Gigas002/abar/blob/master/examples/scripts/tray/tray.sh) (`trayctl subscribe` loop → JSON lines on stdout) |

This repo must **not** vendor or depend on `notred` / `libnotred`, `abar` / `libabar`, or open `notred.sock` directly. Copy **patterns** into `libposhanka`; spawn **`notredctl`** (or a user script that only wraps `notredctl`).

**Reference configs (source of truth for poshanka schemas — update examples first, then this doc):**

- `examples/config.toml` — placement, stack layout, layer shell, notredctl wiring.
- `examples/theme.toml` — base visual theme; fragments patch tables (e.g. `examples/urgency/*/theme.toml`).
- `examples/apps/<name>/theme.toml` — optional `[override]` fragments (app or urgency), visual only.

**notred config** (`$XDG_CONFIG_HOME/notred/notred.toml` in the notred repo) owns queue policy, timeouts, and `[events]` — not poshanka.

---

## 1. Goals and constraints

### 1.1 Goals

- **Wayland-native popups**: `zwlr_layer_shell_v1` overlay surfaces; anchor, margins, keyboard interactivity `none`, fractional scale / buffer scale where supported.
- **Cairo + Pango**: measure and paint notification **cards** on an **image buffer** (shm) per surface (`cairo-rs`, `pango`, `pangocairo`); gtk-rs stack versions aligned within one minor.
- **Mako-like UX**: corner stack, gap, urgency-driven look, tap-to-dismiss or whole-card activation — **behavioral** reference, not a mako/dunst config clone. **Action buttons are never drawn** — whole-card tap + `notredctl activate` only.
- **notredctl subscriber**: live state from `notredctl subscribe` (NDJSON events on stdout); mutations via `notredctl close`, `activate`, etc. Reconnect loop like [abar `tray.sh`](https://github.com/Gigas002/abar/blob/master/examples/scripts/tray/tray.sh).
- **Config discovery**: XDG-style `$XDG_CONFIG_HOME/poshanka/config.toml`, theme from `paths.theme`, plus `--config` on the binary.
- **Deploy model**: user runs **`notred`** (systemd user unit or session autostart) **and** **`poshanka`** (graphical subscriber). Control plane for operators: **`notredctl`**, not a poshanka-specific ctl binary.

### 1.2 Crate split and runtime (poshanka-specific)

- **`libposhanka`** — notification view model (from `notredctl` JSON), render, Wayland surfaces, `notredctl` child-process I/O (subscribe + one-shot commands). **No** `clap`, **no** `toml`, **no** `zbus`, **no** FDN server.
- **`poshanka`** — binary: config/theme, `Settings`, run loop; wires `libposhanka` only.
- **No `poshankactl`** — removed from scope; use upstream **`notredctl`**.

**Threading:** `notredctl subscribe` runs as a **child process** with stdout parsed on a dedicated reader thread (or async task that signals the Wayland thread). One-shot `notredctl close` / `activate` via `std::process::Command` — never block the Wayland `poll` loop. The Wayland client loop stays **synchronous** on the main thread (`poll` + nonblocking dispatch, wakeup `UnixStream`).

**Step sizing:** small PR-sized phases with explicit **Verify** blocks (quality gates per [ARCHITECTURE.md §8](./ARCHITECTURE.md#8-quality-gates--required-before-every-commit)).

### 1.3 Non-goals / deferred

- **No** FDN D-Bus server, queue, or timeout engine in this repo — [notred](https://github.com/Gigas002/notred).
- **No** `poshankactl`, custom Unix socket client, or `libnotred` dependency.
- **No** GTK/Qt/iced notification applets; **no** full notification center / history browser (use [notred-tui](https://github.com/Gigas002/notred) + `notredctl list-history` when history is enabled).
- **No** pixel-perfect mako clone; deliberate divergences in §1.5.
- **No** dunst-only features mako does not have — history UI, inhibition, inline `image-data`, daemon sound, dunst rule scripts.
- **Deferred (mako parity, pixels or subscriber-only):** icons (Phase 5), **`[progress]`** bar (theme schema exists), optional **body markup** if advertised jointly with notred, richer **override criteria** for theme fragments.

### 1.4 Definitions

- **Notification (view)**: one item from a `notredctl` `update` event / `list` snapshot (`MinimalNotification` shape in [notred IPC](https://github.com/Gigas002/notred/blob/master/docs/IPC.md)).
- **Card**: rendered representation on a Wayland surface. **No action button row, ever** — whole-card tap → `notredctl activate`.
- **Stack**: ordered visible cards at a screen corner; membership and dismissals come from **notred** via subscribe snapshots.
- **Surface strategy (v0)**: **one layer-shell surface per notification** for hit-testing; revisit single-surface stack if compositor overhead hurts.
- **IPC**: poshanka talks to **notred** only through **`notredctl`** subprocesses — never `notred.sock`, never session D-Bus for notifications.

### 1.5 Behavioral reference (mako primary, dunst secondary)

| Area | Follow **mako** | Notes |
| ---- | ----------------- | ----- |
| Platform | Wayland layer-shell popups | FDN lives in **notred**, not poshanka |
| Config shape | Global theme + **criteria/override fragments** for look | Behavior/timeouts/`[events]` in **notred** config |
| Interaction | Tap card to dismiss or activate | poshanka → `notredctl`; notred emits FDN signals |
| Look | Theme tables (colors, layout, Pango templates) | poshanka `examples/theme.toml` |
| Progress | `over` / `source` bar | deferred; data from notred hints when wired |
| Actions UI | Mako *can* show buttons; **poshanka never does** | `notredctl activate` |
| Control CLI | **`notredctl`** (`reload`, `close-all`, `pause`, …) | not a poshanka binary |

---

## 2. Crate layout (poshanka-specific)

Generic workspace layout: [ARCHITECTURE.md §2](./ARCHITECTURE.md#2-repository-layout).

```text
libposhanka/
  src/
    model/           # view types mapped from notredctl JSON
    render/          # cairo+pango: measure card, paint card
    icon/            # icon hint from JSON (Phase 5)
    wayland/         # layer_shell, per-notification surfaces, pointer
    notred/          # spawn notredctl, parse NDJSON, run ctl commands
poshanka/
  src/
    cli/             # --config
    config/          # poshanka TOML only (visual + placement + notred wiring)
    theme/
    settings/
    logger/
    app/             # Settings → libposhanka::run_subscriber
examples/
  scripts/
    notred-subscribe.sh   # abar tray.sh analogue — notredctl subscribe + reconnect
```

**Crate boundaries**

- `libposhanka`: render + Wayland + `notredctl` I/O; no config parsers.
- `poshanka`: TOML, `Settings`, subscriber entrypoint.
- External runtime deps: **`notred`** daemon + **`notredctl`** on `$PATH` (document in README; optional `examples/notred.service` pointer to notred repo).

**Feature passthrough:** optional `icons`, `markup`, etc. on `libposhanka`; `poshanka` binary passes features through. **No `dbus` feature** — D-Bus is notred's concern.

---

## 3. Config split: poshanka vs notred

### 3.1 What lives where

| Concern | Owner | Config |
| ------- | ----- | ------ |
| FDN, queue, `max_visible`, timeouts, pause | **notred** | `$XDG_CONFIG_HOME/notred/notred.toml` |
| `[events]` shell, `ActionInvoked` ordering | **notred** | same |
| Override fragments (behavior) | **notred** | `paths.overrides` in notred config |
| Placement, gap, layer-shell anchor/layer | **poshanka** | `examples/config.toml` |
| Card look (font, colors, layout, templates) | **poshanka** | `examples/theme.toml` + fragments |
| Visual override per app/urgency | **poshanka** | `examples/apps/*/theme.toml`, `examples/urgency/*/theme.toml` |
| `notredctl` path / subscribe wrapper | **poshanka** | `[notred]` section |

**Rule:** if it affects **D-Bus apps** (timeout, dismiss reason, capabilities, signals), it belongs in **notred**. If it affects **pixels only**, it belongs in **poshanka**.

### 3.2 poshanka `config.toml`

| Section | Role |
| ------- | ---- |
| `[paths]` | `theme`; `overrides` — ordered theme fragment paths (relative to config dir) |
| `[notred]` | `ctl` (default `notredctl`); optional `subscribe_exec` wrapper script; optional `socket` passed as `notredctl --socket …` |
| `[stack]` | `gap`, visual stacking policy for surfaces poshanka paints (not queue cap — that is notred) |
| `[placement]` | `anchor`, `margin` |
| `[layer]` | layer-shell `layer`, optional `output` |

**`[override]`** (in theme fragments only)

- `type` — `app` \| `urgency`
- `name` / `level` — match `app_id` / urgency from subscribe JSON

Merge policy: **first matching `[override]` wins** for app/urgency; fragment theme keys replace same keys in merged theme.

### 3.3 `theme.toml`

| Section | Keys (representative) |
| ------- | --------------------- |
| `[font]` | `name`, `size` |
| `[colors]` | `background`, `foreground`, `border`, `progress` |
| `[layout]` | `width`, `height` (max), `padding`, `margin` |
| `[border]` | `size`, `radius` |
| `[text]` | `alignment`; `summary`, `body`, … — Pango templates with `{summary}`, `{body}`, … |
| `[icons]` | `size`, `position`, `theme` |
| `[progress]` | `mode` — `over` \| `source` (deferred) |

### 3.4 notredctl JSON → internal view

Map fields from `notredctl list` / `subscribe` `update` items (see [notred IPC](https://github.com/Gigas002/notred/blob/master/docs/IPC.md)):

| JSON field | poshanka use | v0 |
| ---------- | ------------ | -- |
| `id` | surface key, `notredctl close` / `activate` | yes |
| `app_id`, `summary`, `body` | text templates | yes |
| `urgency` | theme override + colors | yes |
| `timeout_ms` | display only (timer in notred) | yes |
| `icon` | icon column | Phase 5 |
| `has_actions` | whole-card tap → `activate` vs `close` | Phase 4 |

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

1. Spawns `subscribe_exec` from config (abar-style), **or**
2. Spawns `notredctl subscribe` directly with the same reconnect loop in Rust.

**Contract:** one NDJSON object per line on child stdout. Handle `{"type":"event","event":{"kind":"update","items":[…]}}` — refresh local view from `items`. On `reload` event, re-read poshanka config/theme from disk (notred config reload is `notredctl reload`, separate).

**Initial sync:** on startup, run `notredctl list` once before or after subscribe attaches, so cards appear even if no event fired yet.

### 5.3 User actions (poshanka → notredctl)

| User gesture | Command | Notes |
| ------------ | ------- | ----- |
| Tap card, `has_actions` | `notredctl activate <id> [key]` | prefer `default` key; notred runs `[events]` + FDN |
| Tap card, no actions | `notredctl close <id>` | dismiss |
| (none in poshanka) | `notredctl reload` | operator / keybind via shell |
| (none in poshanka) | `notredctl pause` / `unpause` | operator |

**Do not** emit D-Bus signals or run `[events]` shell from poshanka — notred owns that pipeline.

### 5.4 Process model

| Process | Role |
| ------- | ---- |
| **`notred`** | Single FDN owner; must be running before poshanka shows cards |
| **`poshanka`** | Subscriber UI; exits if subscribe child dies permanently (configurable retry like tray.sh) |
| **`notredctl`** | Stateless CLI; poshanka spawns per command + one long-lived subscribe child |

**Single instance:** notred enforces FDN bus name. Multiple **poshanka** instances are undefined — document “one graphical subscriber per session” for v0.

### 5.5 Wayland

- `wayland-client`, `wayland-protocols-wlr` (`wlr-layer-shell-unstable-v1`).
- Layer: **overlay**; anchor from config; keyboard interactivity **none**.
- Pointer: whole-card click per §5.3.
- Seat: pointer required for v0.

---

## 6. Module catalog (`libposhanka`)

| Module | Responsibility |
| ------ | -------------- |
| `model` | `NotificationView`, `Urgency`, `CardStyle`, map from ctl JSON |
| `render` | `measure_card`, `paint_card` → pixel buffer |
| `wayland` | Globals, surfaces, SHM, pointer → ctl commands |
| `notred` | Child `subscribe`, parse NDJSON, `run_ctl(&["close", id])` helpers |
| `icon` | Resolve `icon` from JSON (Phase 5) |

---

## 7. CI notes (poshanka-specific)

Quality gates: [ARCHITECTURE.md §6–§8](./ARCHITECTURE.md#6-testing-and-coverage).

- Workspace members: **`libposhanka`**, **`poshanka`** only (drop `poshankactl` when migration lands).
- **libcairo2-dev**, **libpango1.0-dev** for render tests.
- **notred** + **notredctl** required for integration/manual tests — install from [notred](https://github.com/Gigas002/notred) or CI services block; unit tests mock NDJSON fixtures from `examples/notred-fixtures/`.
- Headless: queue diff, JSON parse, render pixels — no compositor, no live notred.

---

## 8. Phased steps

### Phase 0 — Workspace + hygiene + empty vertical slice ✅

Completed under the **pre-notred** plan (three crates, Wayland color rect, config/theme load). Artifacts to revisit in Phase 1b: **`poshankactl/`** stub, D-Bus-oriented descriptions in manifests.

### Phase 1 — Config + theme + runtime spec ✅

Completed: serde for `examples/**`, override merge, `Settings` → `DaemonSpec` / `CardStyle`. **Follow-up in Phase 1b:** trim config schema to visual-only; add `[notred]` section; move timeout/queue/events docs to notred.

### Phase 1b — notred pivot (migration)

- [ ] Remove **`poshankactl/`** from workspace; delete crate tree.
- [ ] Update root `Cargo.toml` description (“Wayland subscriber for notred”).
- [ ] Add `examples/scripts/notred-subscribe.sh` (tray.sh pattern).
- [ ] Add `examples/notred-fixtures/*.jsonl` golden lines for subscribe/list parsing tests.
- [ ] Strip any D-Bus / `zbus` deps and plan-only modules from `libposhanka`.
- [ ] Rename `DaemonSpec` → `SubscriberSpec` (or equivalent) — placement/layer/notred wiring only.
- [ ] Document two-process setup in README sketch: `notred` + `poshanka`.

**Verify**: workspace builds with two members; `rg poshankactl` / `zbus` clean; fixture parse tests green.

### Phase 2 — Render core (Cairo + Pango)

- [ ] `color`, `render/font`, rounded rect, BGRA buffer (port from [abar](https://github.com/Gigas002/abar) if useful).
- [ ] `measure_card` / `paint_card` with summary + body (placeholder icon).
- [ ] Headless render tests.

**Verify**: `libposhanka` render tests without compositor or notred.

### Phase 3 — notredctl subscriber loop

- [ ] `notred/`: spawn subscribe child, parse NDJSON, reconnect with backoff.
- [ ] `notredctl list` on startup; map JSON → `model::NotificationView`.
- [ ] Unit tests with golden fixtures; optional `#[ignore]` test with live notred.

**Verify**: manual — `notred` running, `poshanka` logs parsed item count on `notify-send`.

### Phase 4 — Wayland surfaces + sync

- [ ] One layer surface per notification id from subscribe snapshots.
- [ ] SHM resize on configure; paint via Phase 2.
- [ ] Diff `update` items → create/destroy/reposition surfaces.
- [ ] Wakeup pipe + `poll` loop.

**Verify**: manual on Hyprland — `notify-send` shows themed card via notred + poshanka.

### Phase 5 — Pointer + activate/close

- [ ] Whole-card click → `notredctl close <id>` or `activate <id>`.
- [ ] Surfaces removed when id absent from next `update`.

**Verify**: tap dismisses; `notify-send --action` + tap → `dbus-monitor` shows `ActionInvoked` from **notred**.

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
- [ ] Subscribe feed via **`notredctl`** only (direct spawn or `subscribe_exec` script).
- [ ] Card tap → `notredctl activate` or `close`; FDN signals originate from **notred**.
- [ ] **No** GTK/iced; Cairo+Pango path live.
- [ ] **No** `poshankactl`, **no** `zbus`, **no** `notred.sock` in poshanka.
- [ ] CI green per [ARCHITECTURE.md §8](./ARCHITECTURE.md#8-quality-gates--required-before-every-commit).

---

## 10. Stack dependencies (poshanka-specific)

Generic policy: [ARCHITECTURE.md §7](./ARCHITECTURE.md#7-dependencies).

| Area | Crates / notes |
| ---- | -------------- |
| Graphics | `cairo-rs`, `pango`, `pangocairo` — one gtk-rs minor |
| Wayland | `wayland-client`, `wayland-protocols-wlr` |
| JSON | `serde`, `serde_json` — parse notredctl stdout |
| External binaries | **`notred`**, **`notredctl`** — not Cargo deps; required at runtime |

---

## 11. Pattern checklist (abar tray → poshanka notred)

| abar concern | poshanka module |
| ------------ | --------------- |
| `[tray].exec` long-lived script | `[notred].subscribe_exec` or built-in reconnect loop |
| `trayctl subscribe` stdout JSON | `notredctl subscribe` NDJSON → `notred/` parser |
| Tray item click → external action | Card click → `notredctl activate` / `close` |
| Hex RGBA → buffer | `libposhanka/src/color/` |
| Font / rounded rects | `libposhanka/src/render/` |
| SHM lifecycle | `libposhanka/src/wayland/` |
| Settings boundary | `poshanka/src/settings/` → `SubscriberSpec` + `CardStyle` |
| Poll + wakeup | `libposhanka/src/wayland/` |

**Never** add `libnotred`, `libabar`, or a custom socket client as dependencies.

---

## 12. Document maintenance

Update this plan when subscriber behavior, config schema, or notredctl command usage changes. Update [ARCHITECTURE.md](./ARCHITECTURE.md) for workspace-wide conventions. For poshanka config: `examples/*.toml` first, then this doc. For FDN/queue/timeouts: [notred](https://github.com/Gigas002/notred) docs only.

---

## Revision history

| Date | Change |
| ---- | ------ |
| 2026-05-18 | Initial poshanka plan (monolithic daemon model) |
| 2026-06-02 | `examples/` config system; mako primary reference |
| 2026-07-03 | Trim duplication; [ARCHITECTURE.md](./ARCHITECTURE.md) structural source of truth |
| 2026-07-03 | **notred pivot:** poshanka = Wayland subscriber via **`notredctl`**; drop FDN/`poshankactl`; abar tray exec pattern |
