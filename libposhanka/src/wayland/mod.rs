//! `zwlr_layer_shell_v1` overlay: one Wayland surface per notification `id`.
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
    wl_buffer, wl_compositor, wl_registry, wl_shm, wl_shm_pool, wl_surface,
};
use wayland_client::{Connection, Dispatch, QueueHandle};
use wayland_protocols_wlr::layer_shell::v1::client::{
    zwlr_layer_shell_v1::{Layer, ZwlrLayerShellV1},
    zwlr_layer_surface_v1::{self, KeyboardInteractivity, ZwlrLayerSurfaceV1},
};

mod anchor;
mod stack;

use anchor::Corner;
use stack::stack_offsets;

use crate::error::PoshankaError;
use crate::feed::{FeedSignal, NotificationState};
use crate::model::{CardStyle, NotificationView, SubscriberSpec};
use crate::render::{FontContext, Frame, paint_card};

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
        dirty: true,
        compositor: None,
        shm: None,
        layer_shell: None,
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

        read_guard
            .read()
            .map_err(|e| PoshankaError::WaylandProtocol(format!("read failed: {e}")))?;

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
    /// Set whenever the notification list (or, on reload, its styling) may
    /// have changed and the surface stack needs re-syncing.
    dirty: bool,
    compositor: Option<wl_compositor::WlCompositor>,
    shm: Option<wl_shm::WlShm>,
    layer_shell: Option<ZwlrLayerShellV1>,
    surfaces: HashMap<u32, CardSurface>,
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

    /// Diff the current notification snapshot against live surfaces, then
    /// create/destroy/reposition/repaint as needed.
    fn try_sync_stack(&mut self, qh: &QueueHandle<Self>) {
        if !self.dirty || !self.globals_ready() {
            return;
        }
        self.dirty = false;

        let items: Vec<NotificationView> = self.notifications.items().to_vec();
        let keep_ids: HashSet<u32> = items.iter().map(|v| v.id).collect();

        let stale: Vec<u32> = self
            .surfaces
            .keys()
            .copied()
            .filter(|id| !keep_ids.contains(id))
            .collect();
        for id in stale {
            if let Some(card) = self.surfaces.remove(&id) {
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
            self.upsert_surface(
                qh,
                id,
                frame,
                fallback_bgra,
                margin,
                &compositor,
                &layer_shell,
                &shm,
            );
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn upsert_surface(
        &mut self,
        qh: &QueueHandle<Self>,
        id: u32,
        frame: Frame,
        fallback_bgra: [u8; 4],
        margin: (i32, i32, i32, i32),
        compositor: &wl_compositor::WlCompositor,
        layer_shell: &ZwlrLayerShellV1,
        shm: &wl_shm::WlShm,
    ) {
        if let Some(card) = self.surfaces.get_mut(&id) {
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

        let surface = compositor.create_surface(qh, id);
        let layer_surface =
            layer_shell.get_layer_surface(&surface, None, self.layer, "poshanka".into(), qh, id);

        layer_surface.set_anchor(self.corner.anchor_bits());
        let (top, right, bottom, left) = margin;
        layer_surface.set_margin(top, right, bottom, left);
        layer_surface.set_keyboard_interactivity(KeyboardInteractivity::None);
        layer_surface.set_size(frame.width, frame.height);
        surface.commit();

        self.surfaces.insert(
            id,
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
        id: u32,
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
        let Some(card) = self.surfaces.get_mut(&id) else {
            return;
        };

        let width = width.max(1);
        let height = height.max(1);
        card.configured = true;

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

    fn on_closed(&mut self, id: u32) {
        debug!(id, "layer surface closed by compositor");
        self.surfaces.remove(&id);
    }
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
        let wl_registry::Event::Global {
            name,
            interface,
            version,
        } = event
        else {
            return;
        };

        match interface.as_str() {
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
                let shell = registry.bind::<ZwlrLayerShellV1, _, _>(name, 4.min(version), qh, ());
                state.layer_shell = Some(shell);
                state.try_sync_stack(qh);
            }
            _ => {}
        }
    }
}

impl Dispatch<ZwlrLayerSurfaceV1, u32> for AppState {
    fn event(
        state: &mut Self,
        layer_surface: &ZwlrLayerSurfaceV1,
        event: zwlr_layer_surface_v1::Event,
        data: &u32,
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        let id = *data;
        match event {
            zwlr_layer_surface_v1::Event::Configure {
                serial,
                width,
                height,
            } => state.on_configure(id, layer_surface, serial, width, height, qh),
            zwlr_layer_surface_v1::Event::Closed => state.on_closed(id),
            _ => {}
        }
    }
}

impl Dispatch<wl_surface::WlSurface, u32> for AppState {
    fn event(
        _: &mut Self,
        _: &wl_surface::WlSurface,
        _: wl_surface::Event,
        _: &u32,
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
