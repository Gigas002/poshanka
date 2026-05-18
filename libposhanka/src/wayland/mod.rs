//! Minimal `zwlr_layer_shell_v1` overlay: one solid-color buffer (Phase 0).

use std::io::Write;
use std::os::fd::AsFd;

use rustix::event::{PollFd, PollFlags, poll};
use tracing::{debug, warn};
use wayland_client::protocol::{
    wl_buffer, wl_compositor, wl_registry, wl_shm, wl_shm_pool, wl_surface,
};
use wayland_client::{Connection, Dispatch, QueueHandle};
use wayland_protocols_wlr::layer_shell::v1::client::{
    zwlr_layer_shell_v1::{Layer, ZwlrLayerShellV1},
    zwlr_layer_surface_v1::{self, Anchor, KeyboardInteractivity, ZwlrLayerSurfaceV1},
};

use crate::error::PoshankaError;
use crate::model::OverlaySpec;

/// Blocks until the layer surface is closed or an unrecoverable error occurs.
pub fn run_overlay(spec: OverlaySpec) -> Result<(), PoshankaError> {
    let conn = Connection::connect_to_env()?;
    let display = conn.display();
    let mut event_queue = conn.new_event_queue();
    let qh = event_queue.handle();

    display.get_registry(&qh, ());

    let mut state = AppState {
        running: true,
        spec,
        compositor: None,
        shm: None,
        layer_shell: None,
        surface: None,
        layer_surface: None,
        pending_configure: None,
        buffer: None,
        pool: None,
        pool_file: None,
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

        let Some(read_guard) = event_queue.prepare_read() else {
            continue;
        };

        let wayland_fd = read_guard.connection_fd();
        let mut pollfds = [PollFd::from_borrowed_fd(wayland_fd, PollFlags::IN)];

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

        if !state.running {
            break;
        }
    }

    Ok(())
}

struct AppState {
    running: bool,
    spec: OverlaySpec,
    compositor: Option<wl_compositor::WlCompositor>,
    shm: Option<wl_shm::WlShm>,
    layer_shell: Option<ZwlrLayerShellV1>,
    surface: Option<wl_surface::WlSurface>,
    layer_surface: Option<ZwlrLayerSurfaceV1>,
    pending_configure: Option<(u32, u32, u32)>,
    buffer: Option<wl_buffer::WlBuffer>,
    pool: Option<wl_shm_pool::WlShmPool>,
    pool_file: Option<std::fs::File>,
}

impl AppState {
    fn try_init_layer_shell(&mut self, qh: &QueueHandle<Self>) {
        if self.layer_surface.is_some() {
            return;
        }
        let Some(compositor) = self.compositor.as_ref() else {
            return;
        };
        let Some(layer_shell) = self.layer_shell.as_ref() else {
            return;
        };

        let surface = compositor.create_surface(qh, ());
        let layer_surface = layer_shell.get_layer_surface(
            &surface,
            None,
            Layer::Overlay,
            "poshanka".into(),
            qh,
            (),
        );

        layer_surface.set_anchor(Anchor::Top | Anchor::Right);
        layer_surface.set_margin(16, 0, 0, 16);
        layer_surface.set_keyboard_interactivity(KeyboardInteractivity::None);
        layer_surface.set_size(self.spec.width, self.spec.height);

        surface.commit();

        self.surface = Some(surface);
        self.layer_surface = Some(layer_surface);
        debug!("layer surface created (initial commit without buffer)");
    }

    fn on_configure(
        &mut self,
        layer_surface: &ZwlrLayerSurfaceV1,
        serial: u32,
        width: u32,
        height: u32,
        qh: &QueueHandle<Self>,
    ) -> Result<(), PoshankaError> {
        let width = width.max(1);
        let height = height.max(1);

        let Some(shm) = self.shm.clone() else {
            self.pending_configure = Some((width, height, serial));
            return Ok(());
        };

        layer_surface.ack_configure(serial);
        self.paint_solid(&shm, qh, width, height)
    }

    fn try_flush_pending_configure(&mut self, qh: &QueueHandle<Self>) -> Result<(), PoshankaError> {
        if self.pending_configure.is_none() {
            return Ok(());
        }
        let Some(shm) = self.shm.clone() else {
            return Ok(());
        };
        let Some(ls) = self.layer_surface.as_ref() else {
            return Ok(());
        };
        let Some((w, h, serial)) = self.pending_configure.take() else {
            return Ok(());
        };
        ls.ack_configure(serial);
        self.paint_solid(&shm, qh, w, h)
    }

    fn paint_solid(
        &mut self,
        shm: &wl_shm::WlShm,
        qh: &QueueHandle<Self>,
        width: u32,
        height: u32,
    ) -> Result<(), PoshankaError> {
        let width = width.max(1);
        let height = height.max(1);
        let stride = width.saturating_mul(4);
        let size = (stride as u64)
            .checked_mul(height as u64)
            .ok_or_else(|| PoshankaError::WaylandProtocol("buffer size overflow".into()))?;

        self.buffer.take();
        self.pool.take();
        self.pool_file.take();

        let pixel = self.spec.background_bgra;
        let mut data = vec![0u8; size as usize];
        for chunk in data.chunks_exact_mut(4) {
            chunk.copy_from_slice(&pixel);
        }

        let mut file = tempfile::tempfile_in("/dev/shm").map_err(|source| PoshankaError::Io {
            path: std::path::PathBuf::from("/dev/shm"),
            source,
        })?;
        file.write_all(&data).map_err(|source| PoshankaError::Io {
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
            stride as i32,
            wl_shm::Format::Argb8888,
            qh,
            (),
        );

        let surface = self.surface.as_ref().ok_or_else(|| {
            PoshankaError::WaylandProtocol("missing wl_surface during paint".into())
        })?;
        surface.attach(Some(&buffer), 0, 0);
        surface.damage_buffer(0, 0, width as i32, height as i32);
        surface.commit();

        self.pool_file = Some(file);
        self.pool = Some(pool);
        self.buffer = Some(buffer);
        Ok(())
    }
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
                state.try_init_layer_shell(qh);
            }
            "wl_shm" => {
                let shm = registry.bind::<wl_shm::WlShm, _, _>(name, 1.min(version), qh, ());
                state.shm = Some(shm);
                if let Err(e) = state.try_flush_pending_configure(qh) {
                    warn!(error = %e, "failed to apply pending layer configure");
                    state.running = false;
                }
            }
            "zwlr_layer_shell_v1" => {
                let shell = registry.bind::<ZwlrLayerShellV1, _, _>(name, 4.min(version), qh, ());
                state.layer_shell = Some(shell);
                state.try_init_layer_shell(qh);
            }
            _ => {}
        }
    }
}

impl Dispatch<ZwlrLayerSurfaceV1, ()> for AppState {
    fn event(
        state: &mut Self,
        layer_surface: &ZwlrLayerSurfaceV1,
        event: zwlr_layer_surface_v1::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        match event {
            zwlr_layer_surface_v1::Event::Configure {
                serial,
                width,
                height,
            } => {
                if let Err(e) = state.on_configure(layer_surface, serial, width, height, qh) {
                    warn!(error = %e, "configure handling failed");
                    state.running = false;
                }
            }
            zwlr_layer_surface_v1::Event::Closed => {
                debug!("layer surface closed");
                state.running = false;
            }
            _ => {}
        }
    }
}

wayland_client::delegate_noop!(AppState: ignore wl_compositor::WlCompositor);
wayland_client::delegate_noop!(AppState: ignore wl_surface::WlSurface);
wayland_client::delegate_noop!(AppState: ignore wl_shm::WlShm);
wayland_client::delegate_noop!(AppState: ignore wl_shm_pool::WlShmPool);
wayland_client::delegate_noop!(AppState: ignore wl_buffer::WlBuffer);
wayland_client::delegate_noop!(AppState: ignore ZwlrLayerShellV1);
