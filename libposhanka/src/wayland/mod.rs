//! `zwlr_layer_shell_v1` overlay: one Wayland surface per notification `id`,
//! replicated on every connected output (or a single named output when
//! `[layer].output` is set).
//!
//! Each visible notification (per the provider feed snapshot) gets its own
//! layer-shell surface, sized and painted via [`crate::render::paint_card`].
//! Surfaces are created/destroyed as the feed's `id` set changes, and
//! repositioned along the configured corner using `[stack].gap` +
//! `[placement].margin`.

use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::Write;
use std::os::fd::AsFd;
use std::os::unix::net::UnixStream;
use std::sync::mpsc;

use rustix::event::{PollFd, PollFlags, poll};
use tracing::{debug, info, warn};
use wayland_client::protocol::{
    wl_buffer, wl_compositor, wl_output, wl_pointer, wl_registry, wl_seat, wl_shm, wl_shm_pool,
    wl_surface, wl_touch,
};
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle};
use wayland_protocols_wlr::layer_shell::v1::client::{
    zwlr_layer_shell_v1::{Layer, ZwlrLayerShellV1},
    zwlr_layer_surface_v1::{self, KeyboardInteractivity, ZwlrLayerSurfaceV1},
};

mod anchor;
mod stack;

use anchor::Corner;
use stack::stack_offsets;

use crate::error::PoshankaError;
use crate::feed::{self, FeedSignal, NotificationState, ProviderSpec};
use crate::model::{CardStyle, NotificationView, SubscriberSpec};
use crate::render::{FontContext, Frame, paint_card};

/// Linux input event codes for pointer buttons (`linux/input-event-codes.h`).
const BTN_LEFT: u32 = 0x110;
const BTN_RIGHT: u32 = 0x111;
const BTN_MIDDLE: u32 = 0x112;

/// Resolves the effective [`CardStyle`] for a notification, and reacts to a
/// provider `reload` event by re-reading whatever config/theme backs it.
///
/// `libposhanka` never parses TOML or reads config paths (see
/// ARCHITECTURE.md §1.1) — the `poshanka` binary implements this trait to
/// apply theme + override-fragment merging over `app_id` / urgency.
pub trait StyleSource {
    /// Resolve the merged style for `notification` (app/urgency overrides applied).
    fn style_for(&mut self, notification: &NotificationView) -> CardStyle;

    /// Re-read config/theme from disk after a provider `reload` event.
    ///
    /// Implementations should log and keep the previous style on failure
    /// rather than propagating an error into the Wayland loop.
    fn reload(&mut self);
}

/// Provider feed wired into the Wayland poll loop (wakeup fd + parsed signal channel).
pub struct FeedHandle {
    pub wakeup: UnixStream,
    pub rx: mpsc::Receiver<FeedSignal>,
}

/// Runs the per-notification card stack until the compositor connection closes
/// or an unrecoverable protocol error occurs.
pub fn run_overlay(
    stack_spec: SubscriberSpec,
    initial: Vec<NotificationView>,
    feed: Option<FeedHandle>,
    style_source: Box<dyn StyleSource>,
    provider: ProviderSpec,
) -> Result<(), PoshankaError> {
    let conn = Connection::connect_to_env()?;
    let display = conn.display();
    let mut event_queue = conn.new_event_queue();
    let qh = event_queue.handle();

    display.get_registry(&qh, ());

    let mut notifications = NotificationState::default();
    notifications.replace(initial);

    let mut state = AppState {
        running: true,
        corner: Corner::parse(&stack_spec.anchor),
        base_margin: stack_spec.margin,
        gap: stack_spec.stack_gap,
        layer: parse_layer(&stack_spec.layer),
        notifications,
        feed,
        style_source,
        provider,
        dirty: true,
        compositor: None,
        shm: None,
        layer_shell: None,
        seat: None,
        pointer: None,
        touch: None,
        pointer_focus: None,
        output_filter: stack_spec.output.clone(),
        outputs: HashMap::new(),
        surfaces: HashMap::new(),
    };

    loop {
        event_queue
            .flush()
            .map_err(|e| PoshankaError::WaylandProtocol(format!("flush failed: {e}")))?;

        event_queue
            .dispatch_pending(&mut state)
            .map_err(|e| PoshankaError::WaylandProtocol(format!("dispatch failed: {e}")))?;

        if !state.running {
            break;
        }

        state.drain_feed();
        state.try_sync_stack(&qh);

        let Some(read_guard) = event_queue.prepare_read() else {
            continue;
        };

        let wayland_fd = read_guard.connection_fd();
        let mut pollfds = vec![PollFd::from_borrowed_fd(wayland_fd, PollFlags::IN)];
        if let Some(feed) = state.feed.as_mut() {
            pollfds.push(PollFd::from_borrowed_fd(feed.wakeup.as_fd(), PollFlags::IN));
        }

        loop {
            match poll(&mut pollfds, None) {
                Ok(0) => continue,
                Ok(_) => break,
                Err(e) => {
                    return Err(PoshankaError::WaylandProtocol(format!("poll failed: {e}")));
                }
            }
        }

        // `poll` may have woken us for the feed's wakeup fd alone, with nothing
        // to read on the Wayland socket; `read` reports that as `WouldBlock`,
        // not a protocol failure, since we only prepared the read speculatively.
        match read_guard.read() {
            Ok(_) => {}
            Err(wayland_client::backend::WaylandError::Io(e))
                if e.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(e) => {
                return Err(PoshankaError::WaylandProtocol(format!("read failed: {e}")));
            }
        }

        event_queue
            .dispatch_pending(&mut state)
            .map_err(|e| PoshankaError::WaylandProtocol(format!("dispatch failed: {e}")))?;

        state.drain_feed();
        state.try_sync_stack(&qh);

        if !state.running {
            break;
        }
    }

    Ok(())
}

fn parse_layer(layer: &str) -> Layer {
    match layer.trim().to_ascii_lowercase().as_str() {
        "background" => Layer::Background,
        "bottom" => Layer::Bottom,
        "top" => Layer::Top,
        _ => Layer::Overlay,
    }
}

struct AppState {
    running: bool,
    corner: Corner,
    base_margin: u32,
    gap: u32,
    layer: Layer,
    notifications: NotificationState,
    feed: Option<FeedHandle>,
    style_source: Box<dyn StyleSource>,
    /// `[provider].command` wiring used to fire non-blocking `close` /
    /// `activate` / `input` calls in response to pointer and touch gestures.
    provider: ProviderSpec,
    /// Set whenever the notification list (or, on reload, its styling) may
    /// have changed and the surface stack needs re-syncing.
    dirty: bool,
    compositor: Option<wl_compositor::WlCompositor>,
    shm: Option<wl_shm::WlShm>,
    layer_shell: Option<ZwlrLayerShellV1>,
    seat: Option<wl_seat::WlSeat>,
    pointer: Option<wl_pointer::WlPointer>,
    touch: Option<wl_touch::WlTouch>,
    /// Notification `id` of the card currently under the pointer, if any.
    pointer_focus: Option<u32>,
    /// `[layer].output` config: empty means "every connected output";
    /// otherwise the `wl_output` name to match (see [`OutputEntry`]).
    output_filter: String,
    /// Known `wl_output` globals, keyed by `wl_registry` name.
    outputs: HashMap<u32, OutputEntry>,
    /// Live card surfaces, keyed by `(wl_registry` output name`, notification id)`
    /// so each notification gets one surface per eligible output.
    surfaces: HashMap<(u32, u32), CardSurface>,
}

/// A bound `wl_output` global and its name (from the `wl_output::Event::Name`
/// event, `wl_output` version ≥ 4), used to match `[layer].output`.
struct OutputEntry {
    output: wl_output::WlOutput,
    name: Option<String>,
}

/// Per-notification layer-shell surface and its SHM buffer state.
struct CardSurface {
    surface: wl_surface::WlSurface,
    layer_surface: ZwlrLayerSurfaceV1,
    /// Last size requested via `set_size` (awaiting or matching a configure).
    requested_size: (u32, u32),
    /// Whether at least one configure has been acked for `requested_size`.
    configured: bool,
    /// Frame content to paint once the next configure is acked.
    pending_frame: Option<Frame>,
    /// Solid-color fallback used if the compositor configures an unexpected size.
    fallback_bgra: [u8; 4],
    margin: (i32, i32, i32, i32),
    pool_file: Option<File>,
    pool: Option<wl_shm_pool::WlShmPool>,
    buffer: Option<wl_buffer::WlBuffer>,
}

impl AppState {
    fn drain_feed(&mut self) {
        let Some(feed) = self.feed.as_mut() else {
            return;
        };
        crate::feed::drain_wakeup(&mut feed.wakeup);

        let mut changed = false;
        while let Ok(signal) = feed.rx.try_recv() {
            changed = true;
            match signal {
                FeedSignal::Items(items) => {
                    self.notifications.replace(items);
                    info!(count = self.notifications.len(), "feed update");
                }
                FeedSignal::Reload => {
                    info!("feed reload event");
                    self.style_source.reload();
                }
            }
        }
        if changed {
            self.dirty = true;
        }
    }

    fn globals_ready(&self) -> bool {
        self.compositor.is_some() && self.shm.is_some() && self.layer_shell.is_some()
    }

    /// Whether `entry` should get a card stack: every output when
    /// `[layer].output` is empty, otherwise only the output whose
    /// `wl_output::Event::Name` matches it exactly.
    fn output_matches(&self, entry: &OutputEntry) -> bool {
        if self.output_filter.is_empty() {
            return true;
        }
        entry.name.as_deref() == Some(self.output_filter.as_str())
    }

    /// `wl_registry` names of currently eligible outputs (see [`Self::output_matches`]).
    fn eligible_output_keys(&self) -> Vec<u32> {
        self.outputs
            .iter()
            .filter(|(_, entry)| self.output_matches(entry))
            .map(|(key, _)| *key)
            .collect()
    }

    /// Diff the current notification snapshot (across every eligible output)
    /// against live surfaces, then create/destroy/reposition/repaint as needed.
    fn try_sync_stack(&mut self, qh: &QueueHandle<Self>) {
        if !self.dirty || !self.globals_ready() {
            return;
        }
        self.dirty = false;

        let items: Vec<NotificationView> = self.notifications.items().to_vec();
        let output_keys = self.eligible_output_keys();
        let keep_keys: HashSet<(u32, u32)> = output_keys
            .iter()
            .flat_map(|&output_key| items.iter().map(move |v| (output_key, v.id)))
            .collect();

        let stale: Vec<(u32, u32)> = self
            .surfaces
            .keys()
            .copied()
            .filter(|key| !keep_keys.contains(key))
            .collect();
        for key in stale {
            if let Some(card) = self.surfaces.remove(&key) {
                card.destroy();
            }
        }

        let mut rendered = Vec::with_capacity(items.len());
        for view in &items {
            let style = self.style_source.style_for(view);
            match render_frame(&style, view) {
                Ok(frame) => rendered.push((view.id, style.background_bgra, frame)),
                Err(err) => {
                    warn!(id = view.id, %err, "failed to render notification card; skipping");
                }
            }
        }

        let heights: Vec<u32> = rendered.iter().map(|(_, _, frame)| frame.height).collect();
        let offsets = stack_offsets(&heights, self.gap, self.base_margin);

        // Bound checks in `globals_ready` guarantee these clones succeed.
        let compositor = self.compositor.clone().expect("compositor bound");
        let layer_shell = self.layer_shell.clone().expect("layer shell bound");
        let shm = self.shm.clone().expect("shm bound");

        for ((id, fallback_bgra, frame), offset) in rendered.into_iter().zip(offsets) {
            let margin = self.corner.margins(self.base_margin, offset);
            for &output_key in &output_keys {
                let Some(output) = self.outputs.get(&output_key).map(|e| e.output.clone()) else {
                    continue;
                };
                self.upsert_surface(
                    qh,
                    output_key,
                    &output,
                    id,
                    frame.clone(),
                    fallback_bgra,
                    margin,
                    &compositor,
                    &layer_shell,
                    &shm,
                );
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn upsert_surface(
        &mut self,
        qh: &QueueHandle<Self>,
        output_key: u32,
        output: &wl_output::WlOutput,
        id: u32,
        frame: Frame,
        fallback_bgra: [u8; 4],
        margin: (i32, i32, i32, i32),
        compositor: &wl_compositor::WlCompositor,
        layer_shell: &ZwlrLayerShellV1,
        shm: &wl_shm::WlShm,
    ) {
        let key = (output_key, id);
        if let Some(card) = self.surfaces.get_mut(&key) {
            card.fallback_bgra = fallback_bgra;
            let size_changed = card.requested_size != (frame.width, frame.height);
            let margin_changed = card.margin != margin;

            if margin_changed {
                card.margin = margin;
                let (top, right, bottom, left) = margin;
                card.layer_surface.set_margin(top, right, bottom, left);
            }

            if size_changed {
                card.requested_size = (frame.width, frame.height);
                card.configured = false;
                card.layer_surface.set_size(frame.width, frame.height);
                card.pending_frame = Some(frame);
                card.surface.commit();
            } else if card.configured {
                if let Err(err) = card.paint_frame(shm, qh, &frame) {
                    warn!(id, %err, "failed to repaint notification card");
                }
            } else {
                card.pending_frame = Some(frame);
                if margin_changed {
                    card.surface.commit();
                }
            }
            return;
        }

        let surface = compositor.create_surface(qh, key);
        let layer_surface = layer_shell.get_layer_surface(
            &surface,
            Some(output),
            self.layer,
            "poshanka".into(),
            qh,
            key,
        );

        layer_surface.set_anchor(self.corner.anchor_bits());
        let (top, right, bottom, left) = margin;
        layer_surface.set_margin(top, right, bottom, left);
        layer_surface.set_keyboard_interactivity(KeyboardInteractivity::None);
        layer_surface.set_size(frame.width, frame.height);
        surface.commit();

        self.surfaces.insert(
            key,
            CardSurface {
                surface,
                layer_surface,
                requested_size: (frame.width, frame.height),
                configured: false,
                pending_frame: Some(frame),
                fallback_bgra,
                margin,
                pool_file: None,
                pool: None,
                buffer: None,
            },
        );
    }

    fn on_configure(
        &mut self,
        key: (u32, u32),
        layer_surface: &ZwlrLayerSurfaceV1,
        serial: u32,
        width: u32,
        height: u32,
        qh: &QueueHandle<Self>,
    ) {
        layer_surface.ack_configure(serial);

        let Some(shm) = self.shm.clone() else {
            return;
        };
        let Some(card) = self.surfaces.get_mut(&key) else {
            return;
        };

        let width = width.max(1);
        let height = height.max(1);
        card.configured = true;
        let id = key.1;

        let result = match card.pending_frame.take() {
            Some(frame) if frame.width == width && frame.height == height => {
                card.paint_frame(&shm, qh, &frame)
            }
            Some(frame) => {
                warn!(
                    id,
                    requested_w = frame.width,
                    requested_h = frame.height,
                    configured_w = width,
                    configured_h = height,
                    "compositor configured a different size than requested; using solid fallback"
                );
                card.paint_fallback(&shm, qh, width, height)
            }
            None => card.paint_fallback(&shm, qh, width, height),
        };

        if let Err(err) = result {
            warn!(id, %err, "failed to paint notification card");
        }
    }

    fn on_closed(&mut self, key: (u32, u32)) {
        let id = key.1;
        debug!(id, output = key.0, "layer surface closed by compositor");
        self.surfaces.remove(&key);
        if self.pointer_focus == Some(id) {
            self.pointer_focus = None;
        }
    }

    /// Layer B — whole-card shortcut for a primary-button click: `activate`
    /// when the notification advertises actions, otherwise `close`.
    fn handle_pointer_click(&self, id: u32, button: u32) {
        match button {
            BTN_LEFT => {
                let has_actions = self
                    .notifications
                    .items()
                    .iter()
                    .any(|item| item.id == id && item.has_actions);
                if has_actions {
                    self.spawn_provider_command(ProviderAction::Activate(id));
                } else {
                    self.spawn_provider_command(ProviderAction::Close(id));
                }
            }
            BTN_RIGHT => self.spawn_provider_command(ProviderAction::Input(id, "button_right")),
            BTN_MIDDLE => self.spawn_provider_command(ProviderAction::Input(id, "button_middle")),
            _ => {}
        }
    }

    /// Layer A — report a touch tap as a generic `input` gesture; notred
    /// resolves `on_touch` (or default policy) from its own config.
    fn handle_touch_down(&self, id: u32) {
        self.spawn_provider_command(ProviderAction::Input(id, "touch"));
    }

    /// Fire a `[provider].command` mutation on a dedicated thread so a slow
    /// or hung provider CLI never blocks the Wayland poll loop.
    fn spawn_provider_command(&self, action: ProviderAction) {
        let provider = self.provider.clone();
        std::thread::spawn(move || {
            let (id, result) = match action {
                ProviderAction::Close(id) => (id, feed::close(&provider, id)),
                ProviderAction::Activate(id) => (id, feed::activate(&provider, id, None)),
                ProviderAction::Input(id, kind) => (id, feed::input(&provider, id, kind)),
            };
            if let Err(err) = result {
                warn!(id, %err, "provider command failed");
            }
        });
    }
}

/// A gesture-triggered `[provider].command` mutation, dispatched off the
/// Wayland thread by [`AppState::spawn_provider_command`].
enum ProviderAction {
    /// Layer B whole-card shortcut: dismiss a notification without actions.
    Close(u32),
    /// Layer B whole-card shortcut: run the default action.
    Activate(u32),
    /// Layer A gesture report: `button_left` | `button_middle` | `button_right` | `touch`.
    Input(u32, &'static str),
}

impl CardSurface {
    fn paint_frame(
        &mut self,
        shm: &wl_shm::WlShm,
        qh: &QueueHandle<AppState>,
        frame: &Frame,
    ) -> Result<(), PoshankaError> {
        self.write_buffer(
            shm,
            qh,
            frame.width,
            frame.height,
            frame.stride,
            &frame.data,
        )
    }

    fn paint_fallback(
        &mut self,
        shm: &wl_shm::WlShm,
        qh: &QueueHandle<AppState>,
        width: u32,
        height: u32,
    ) -> Result<(), PoshankaError> {
        let stride = width.saturating_mul(4) as i32;
        let size = (stride as u64).saturating_mul(u64::from(height));
        let mut data = vec![0u8; size as usize];
        for chunk in data.chunks_exact_mut(4) {
            chunk.copy_from_slice(&self.fallback_bgra);
        }
        self.write_buffer(shm, qh, width, height, stride, &data)
    }

    fn write_buffer(
        &mut self,
        shm: &wl_shm::WlShm,
        qh: &QueueHandle<AppState>,
        width: u32,
        height: u32,
        stride: i32,
        data: &[u8],
    ) -> Result<(), PoshankaError> {
        let size = (stride as u64).saturating_mul(u64::from(height));

        self.buffer.take();
        self.pool.take();
        self.pool_file.take();

        let mut file = tempfile::tempfile_in("/dev/shm").map_err(|source| PoshankaError::Io {
            path: std::path::PathBuf::from("/dev/shm"),
            source,
        })?;
        file.write_all(data).map_err(|source| PoshankaError::Io {
            path: std::path::PathBuf::from("/dev/shm"),
            source,
        })?;
        file.flush().map_err(|source| PoshankaError::Io {
            path: std::path::PathBuf::from("/dev/shm"),
            source,
        })?;

        let pool = shm.create_pool(file.as_fd(), size as i32, qh, ());
        let buffer = pool.create_buffer(
            0,
            width as i32,
            height as i32,
            stride,
            wl_shm::Format::Argb8888,
            qh,
            (),
        );

        self.surface.attach(Some(&buffer), 0, 0);
        self.surface
            .damage_buffer(0, 0, width as i32, height as i32);
        self.surface.commit();

        self.pool_file = Some(file);
        self.pool = Some(pool);
        self.buffer = Some(buffer);
        Ok(())
    }

    fn destroy(self) {
        self.layer_surface.destroy();
        self.surface.destroy();
    }
}

fn render_frame(
    style: &CardStyle,
    notification: &NotificationView,
) -> Result<Frame, PoshankaError> {
    let font = FontContext::new(&style.font_name, style.font_size)?;
    paint_card(style, notification, &font)
}

impl Dispatch<wl_registry::WlRegistry, ()> for AppState {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        match event {
            wl_registry::Event::Global {
                name,
                interface,
                version,
            } => match interface.as_str() {
                "wl_compositor" => {
                    let compositor = registry.bind::<wl_compositor::WlCompositor, _, _>(
                        name,
                        5.min(version),
                        qh,
                        (),
                    );
                    state.compositor = Some(compositor);
                    state.try_sync_stack(qh);
                }
                "wl_shm" => {
                    let shm = registry.bind::<wl_shm::WlShm, _, _>(name, 1.min(version), qh, ());
                    state.shm = Some(shm);
                    state.try_sync_stack(qh);
                }
                "zwlr_layer_shell_v1" => {
                    let shell =
                        registry.bind::<ZwlrLayerShellV1, _, _>(name, 4.min(version), qh, ());
                    state.layer_shell = Some(shell);
                    state.try_sync_stack(qh);
                }
                "wl_seat" => {
                    let seat = registry.bind::<wl_seat::WlSeat, _, _>(name, 7.min(version), qh, ());
                    state.seat = Some(seat);
                }
                "wl_output" => {
                    let bound_version = 4.min(version);
                    if bound_version < 4 && !state.output_filter.is_empty() {
                        warn!(
                            name,
                            bound_version,
                            "compositor's wl_output does not support Name (needs v4); \
                             `[layer].output` filter cannot match this output"
                        );
                    }
                    let output =
                        registry.bind::<wl_output::WlOutput, _, _>(name, bound_version, qh, name);
                    state
                        .outputs
                        .insert(name, OutputEntry { output, name: None });
                    // No filter configured: this output is eligible immediately,
                    // without waiting for a `Name` event.
                    if state.output_filter.is_empty() {
                        state.dirty = true;
                    }
                    state.try_sync_stack(qh);
                }
                _ => {}
            },
            wl_registry::Event::GlobalRemove { name } if state.outputs.remove(&name).is_some() => {
                let stale: Vec<(u32, u32)> = state
                    .surfaces
                    .keys()
                    .copied()
                    .filter(|(output_key, _)| *output_key == name)
                    .collect();
                for key in stale {
                    if let Some(card) = state.surfaces.remove(&key) {
                        card.destroy();
                    }
                }
            }
            _ => {}
        }
    }
}

impl Dispatch<wl_output::WlOutput, u32> for AppState {
    fn event(
        state: &mut Self,
        _output: &wl_output::WlOutput,
        event: wl_output::Event,
        registry_name: &u32,
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        let wl_output::Event::Name { name } = event else {
            return;
        };
        let Some(entry) = state.outputs.get_mut(registry_name) else {
            return;
        };
        entry.name = Some(name);
        state.dirty = true;
        state.try_sync_stack(qh);
    }
}

impl Dispatch<wl_seat::WlSeat, ()> for AppState {
    fn event(
        state: &mut Self,
        seat: &wl_seat::WlSeat,
        event: wl_seat::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        let wl_seat::Event::Capabilities { capabilities } = event else {
            return;
        };
        let Ok(capabilities) = capabilities.into_result() else {
            return;
        };

        if capabilities.contains(wl_seat::Capability::Pointer) && state.pointer.is_none() {
            state.pointer = Some(seat.get_pointer(qh, ()));
        }
        if capabilities.contains(wl_seat::Capability::Touch) && state.touch.is_none() {
            state.touch = Some(seat.get_touch(qh, ()));
        }
    }
}

impl Dispatch<wl_pointer::WlPointer, ()> for AppState {
    fn event(
        state: &mut Self,
        _pointer: &wl_pointer::WlPointer,
        event: wl_pointer::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            wl_pointer::Event::Enter { surface, .. } => {
                state.pointer_focus = surface.data::<(u32, u32)>().map(|key| key.1);
            }
            wl_pointer::Event::Leave { .. } => {
                state.pointer_focus = None;
            }
            wl_pointer::Event::Button {
                button,
                state: button_state,
                ..
            } => {
                let Ok(button_state) = button_state.into_result() else {
                    return;
                };
                if button_state != wl_pointer::ButtonState::Released {
                    return;
                }
                if let Some(id) = state.pointer_focus {
                    state.handle_pointer_click(id, button);
                }
            }
            _ => {}
        }
    }
}

impl Dispatch<wl_touch::WlTouch, ()> for AppState {
    fn event(
        state: &mut Self,
        _touch: &wl_touch::WlTouch,
        event: wl_touch::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_touch::Event::Down { surface, .. } = event
            && let Some(id) = surface.data::<(u32, u32)>().map(|key| key.1)
        {
            state.handle_touch_down(id);
        }
    }
}

impl Dispatch<ZwlrLayerSurfaceV1, (u32, u32)> for AppState {
    fn event(
        state: &mut Self,
        layer_surface: &ZwlrLayerSurfaceV1,
        event: zwlr_layer_surface_v1::Event,
        data: &(u32, u32),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        let key = *data;
        match event {
            zwlr_layer_surface_v1::Event::Configure {
                serial,
                width,
                height,
            } => state.on_configure(key, layer_surface, serial, width, height, qh),
            zwlr_layer_surface_v1::Event::Closed => state.on_closed(key),
            _ => {}
        }
    }
}

impl Dispatch<wl_surface::WlSurface, (u32, u32)> for AppState {
    fn event(
        _: &mut Self,
        _: &wl_surface::WlSurface,
        _: wl_surface::Event,
        _: &(u32, u32),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

wayland_client::delegate_noop!(AppState: ignore wl_compositor::WlCompositor);
wayland_client::delegate_noop!(AppState: ignore wl_shm::WlShm);
wayland_client::delegate_noop!(AppState: ignore wl_shm_pool::WlShmPool);
wayland_client::delegate_noop!(AppState: ignore wl_buffer::WlBuffer);
wayland_client::delegate_noop!(AppState: ignore ZwlrLayerShellV1);

#[cfg(test)]
mod tests;
