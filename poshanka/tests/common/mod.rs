//! A headless sway to run poshanka in, a feed to send it notifications
//! through, a stand-in `notredctl` that records what poshanka asks of it, a
//! virtual pointer to click cards with, and `grim` to read back what it drew.
//!
//! Each test starts its own sway in a private runtime directory, so tests run
//! in parallel and never touch the session they are started from. Without
//! `sway` or `grim` the tests are skipped, unless `POSHANKA_LIVE_TESTS=1` (as
//! in CI), where that is a failure.

#![allow(dead_code)]

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

use wayland_client::protocol::{wl_pointer, wl_registry, wl_seat};
use wayland_client::{Connection, Dispatch, EventQueue, QueueHandle, delegate_noop};
use wayland_protocols_wlr::virtual_pointer::v1::client::{
    zwlr_virtual_pointer_manager_v1::ZwlrVirtualPointerManagerV1,
    zwlr_virtual_pointer_v1::ZwlrVirtualPointerV1,
};

pub const WIDTH: u32 = 1280;
pub const HEIGHT: u32 = 720;

/// Start a [`Sway`], or return from the test when the tools are missing.
#[macro_export]
macro_rules! sway_or_skip {
    () => {
        match common::Sway::start() {
            Some(sway) => sway,
            None => return,
        }
    };
}

/// Poll `f` until it returns `Some`, for up to 10 s.
pub fn wait_for<T>(what: &str, f: impl FnMut() -> Option<T>) -> T {
    try_wait_for(f).unwrap_or_else(|| panic!("timed out waiting for {what}"))
}

/// Poll `f` until it returns `Some`, or `None` after 10 s.
pub fn try_wait_for<T>(mut f: impl FnMut() -> Option<T>) -> Option<T> {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(v) = f() {
            return Some(v);
        }
        if Instant::now() > deadline {
            return None;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn have(tool: &str) -> bool {
    Command::new(tool)
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok()
}

pub struct Sway {
    child: Child,
    runtime: tempfile::TempDir,
    display: String,
    ipc: PathBuf,
}

impl Sway {
    pub fn start() -> Option<Self> {
        let required = std::env::var_os("POSHANKA_LIVE_TESTS").is_some_and(|v| v == "1");
        if !(have("sway") && have("grim")) {
            assert!(
                !required,
                "POSHANKA_LIVE_TESTS=1 but sway or grim is missing"
            );
            eprintln!("skipped: sway and grim are needed for live tests");
            return None;
        }
        // Short: the Wayland socket path must fit in sockaddr_un.
        let runtime = tempfile::Builder::new()
            .prefix("psk")
            .tempdir_in("/tmp")
            .unwrap();
        let conf = runtime.path().join("sway.conf");
        std::fs::write(
            &conf,
            format!(
                "output HEADLESS-1 resolution {WIDTH}x{HEIGHT} position 0 0 scale 1\n\
                 seat seat0 hide_cursor 0\n\
                 default_border none\n\
                 xwayland disable\n"
            ),
        )
        .unwrap();
        let log = std::fs::File::create(runtime.path().join("sway.log")).unwrap();
        let child = Command::new("sway")
            .arg("-c")
            .arg(&conf)
            .env_remove("WAYLAND_DISPLAY")
            .env_remove("DISPLAY")
            .env_remove("SWAYSOCK")
            .env_remove("HYPRLAND_INSTANCE_SIGNATURE")
            .env_remove("NIRI_SOCKET")
            .env("XDG_RUNTIME_DIR", runtime.path())
            .env("WLR_BACKENDS", "headless")
            .env("WLR_RENDERER", "pixman")
            .env("WLR_LIBINPUT_NO_DEVICES", "1")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(log)
            .spawn()
            .expect("spawn sway");
        let mut sway = Self {
            child,
            display: String::new(),
            ipc: PathBuf::new(),
            runtime,
        };
        let dir = sway.runtime.path().to_owned();
        let (display, ipc) = wait_for("sway sockets", || {
            if let Ok(Some(status)) = sway.child.try_wait() {
                panic!("sway exited ({status}):\n{}", sway.log());
            }
            let names: Vec<String> = std::fs::read_dir(&dir)
                .ok()?
                .flatten()
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect();
            let display = names
                .iter()
                .find(|n| n.starts_with("wayland-") && !n.ends_with(".lock"))?;
            let ipc = names.iter().find(|n| n.starts_with("sway-ipc."))?;
            Some((display.clone(), dir.join(ipc)))
        });
        sway.display = display;
        sway.ipc = ipc;
        wait_for("sway IPC", || sway.try_msg(&["-t", "get_outputs"]));
        Some(sway)
    }

    pub fn log(&self) -> String {
        std::fs::read_to_string(self.runtime.path().join("sway.log")).unwrap_or_default()
    }

    fn try_msg(&self, args: &[&str]) -> Option<String> {
        let out = Command::new("swaymsg")
            .arg("-s")
            .arg(&self.ipc)
            .args(args)
            .output()
            .ok()?;
        out.status
            .success()
            .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
    }

    /// A command line set up to talk to this compositor.
    pub fn command(&self, program: impl AsRef<std::ffi::OsStr>) -> Command {
        let mut cmd = Command::new(program);
        cmd.env_remove("DISPLAY")
            .env_remove("HYPRLAND_INSTANCE_SIGNATURE")
            .env_remove("NIRI_SOCKET")
            .env("XDG_RUNTIME_DIR", self.runtime.path())
            .env("WAYLAND_DISPLAY", &self.display)
            .env("SWAYSOCK", &self.ipc);
        cmd
    }

    /// Capture a region of the layout.
    pub fn screenshot(&self, x: u32, y: u32, w: u32, h: u32) -> Image {
        let out = self
            .command("grim")
            .args(["-g", &format!("{x},{y} {w}x{h}"), "-t", "ppm", "-"])
            .output()
            .expect("run grim");
        assert!(
            out.status.success(),
            "grim failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        Image::from_ppm(&out.stdout)
    }

    pub fn pointer(&self) -> Pointer {
        Pointer::connect(self)
    }
}

impl Drop for Sway {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// An RGB image read back with `grim`.
#[derive(Clone)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub rgb: Vec<u8>,
}

impl Image {
    fn from_ppm(data: &[u8]) -> Self {
        // P6\n<w> <h>\n<max>\n<data>, fields separated by any whitespace.
        let mut fields = Vec::new();
        let mut pos = 0;
        while fields.len() < 4 {
            while data[pos].is_ascii_whitespace() {
                pos += 1;
            }
            let start = pos;
            while !data[pos].is_ascii_whitespace() {
                pos += 1;
            }
            fields.push(std::str::from_utf8(&data[start..pos]).unwrap().to_owned());
        }
        assert_eq!(fields[0], "P6");
        Self {
            width: fields[1].parse().unwrap(),
            height: fields[2].parse().unwrap(),
            rgb: data[pos + 1..].to_vec(),
        }
    }

    pub fn pixel(&self, x: u32, y: u32) -> [u8; 3] {
        let i = ((y * self.width + x) * 3) as usize;
        [self.rgb[i], self.rgb[i + 1], self.rgb[i + 2]]
    }

    /// Number of pixels of exactly `color`.
    pub fn count(&self, color: [u8; 3]) -> usize {
        self.rgb
            .as_chunks::<3>()
            .0
            .iter()
            .filter(|p| **p == color)
            .count()
    }
}

#[derive(Default)]
struct PointerState {
    manager: Option<ZwlrVirtualPointerManagerV1>,
    seat: Option<wl_seat::WlSeat>,
}

impl Dispatch<wl_registry::WlRegistry, ()> for PointerState {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global {
            name,
            interface,
            version,
        } = event
        {
            match interface.as_str() {
                "zwlr_virtual_pointer_manager_v1" => {
                    state.manager = Some(registry.bind(name, version.min(2), qh, ()));
                }
                "wl_seat" => state.seat = Some(registry.bind(name, 1, qh, ())),
                _ => {}
            }
        }
    }
}
delegate_noop!(PointerState: ignore ZwlrVirtualPointerManagerV1);
delegate_noop!(PointerState: ignore ZwlrVirtualPointerV1);
delegate_noop!(PointerState: ignore wl_seat::WlSeat);

pub const BTN_LEFT: u32 = 0x110;
pub const BTN_RIGHT: u32 = 0x111;
pub const BTN_MIDDLE: u32 = 0x112;

/// A `zwlr_virtual_pointer_v1` on the compositor's seat.
pub struct Pointer {
    conn: Connection,
    queue: EventQueue<PointerState>,
    state: PointerState,
    pointer: ZwlrVirtualPointerV1,
}

impl Pointer {
    fn connect(sway: &Sway) -> Self {
        let socket = sway.runtime.path().join(&sway.display);
        let stream = std::os::unix::net::UnixStream::connect(socket).unwrap();
        let conn = Connection::from_socket(stream).unwrap();
        let mut queue = conn.new_event_queue();
        let qh = queue.handle();
        conn.display().get_registry(&qh, ());
        let mut state = PointerState::default();
        queue.roundtrip(&mut state).unwrap();
        let pointer = state
            .manager
            .as_ref()
            .expect("compositor has no zwlr_virtual_pointer_manager_v1")
            .create_virtual_pointer(state.seat.as_ref(), &qh, ());
        let mut this = Self {
            conn,
            queue,
            state,
            pointer,
        };
        this.sync();
        this
    }

    fn sync(&mut self) {
        self.conn.flush().unwrap();
        self.queue.roundtrip(&mut self.state).unwrap();
    }

    fn time() -> u32 {
        let ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis();
        (ms & 0xffff_ffff) as u32
    }

    /// Click `button` at `(x, y)` in the layout.
    pub fn click_at(&mut self, x: u32, y: u32, button: u32) {
        self.pointer
            .motion_absolute(Self::time(), x, y, WIDTH, HEIGHT);
        self.pointer.frame();
        self.sync();
        for state in [
            wl_pointer::ButtonState::Pressed,
            wl_pointer::ButtonState::Released,
        ] {
            self.pointer.button(Self::time(), button, state);
            self.pointer.frame();
            self.sync();
        }
    }
}

/// The test theme. Cards are [`CARD_W`] wide and exactly [`CARD_H`] tall
/// (their text needs more, so the height limit applies), with no icon, and
/// stack [`GAP`] apart from the top-right corner.
pub const CARD_W: u32 = 300;
pub const CARD_H: u32 = 40;
pub const GAP: u32 = 10;
pub const CARD_BG: [u8; 3] = [0x20, 0x40, 0x60];
pub const BORDER: [u8; 3] = [0xff, 0x00, 0x00];
pub const SCREEN_BG: [u8; 3] = [0, 0, 0];

pub const THEME: &str = r##"
[font]
name = "sans-serif"
size = 11.0

[colors]
background = "#204060ff"
foreground = "#ffffffff"
border = "#ff0000ff"
progress = "#00ff00ff"

[layout]
width = 300
height = 40
padding = 4
margin = 0

[border]
size = 2
radius = 0

[text]
alignment = "left"
summary = "<b>{summary}</b>"
body = "{body}"

[icons]
size = 0
position = "left"
theme = ""

[progress]
mode = "over"
"##;

/// The `[placement]`/`[stack]` part of the config; `{dir}` in a config is
/// replaced by the test directory.
pub const CONFIG: &str = r##"
[paths]
theme = "theme.toml"

[provider]
exec = "{dir}/feed.sh"
command = "{dir}/notredctl"

[stack]
gap = 10

[placement]
anchor = "top-right"
margin = 0

[layer]
layer = "overlay"
output = ""
"##;

/// One notification as the feed sends it; `extra` is spliced in as more
/// JSON fields (`"has_actions": true`).
pub fn notification(id: u32, summary: &str, extra: &str) -> String {
    let extra = if extra.is_empty() {
        String::new()
    } else {
        format!(", {extra}")
    };
    format!(
        r#"{{"id": {id}, "app_id": "app", "summary": "{summary}", "body": "body {id}", "urgency": "normal", "timeout_ms": 0{extra}}}"#
    )
}

/// The top-left corner of the `index`th card of a top-right stack.
pub fn card_origin(index: u32) -> (u32, u32) {
    (WIDTH - CARD_W, index * (CARD_H + GAP))
}

/// A poshanka process on a [`Sway`], fed from a FIFO, with a stand-in
/// `notredctl` that logs its arguments to `calls.log`.
pub struct Poshanka {
    child: Child,
    feed: std::fs::File,
    pub dir: tempfile::TempDir,
}

impl Poshanka {
    pub fn start(sway: &Sway) -> Self {
        Self::start_with(sway, CONFIG, THEME, &[])
    }

    pub fn start_with(sway: &Sway, config: &str, theme: &str, env: &[(&str, &str)]) -> Self {
        let dir = tempfile::Builder::new()
            .prefix("psk-cfg")
            .tempdir()
            .unwrap();
        let p = |name: &str| dir.path().join(name);
        std::fs::write(
            p("config.toml"),
            config.replace("{dir}", dir.path().to_str().unwrap()),
        )
        .unwrap();
        std::fs::write(p("theme.toml"), theme).unwrap();

        let status = Command::new("mkfifo").arg(p("feed")).status().unwrap();
        assert!(status.success(), "mkfifo failed");
        // Read-write keeps the FIFO open without waiting for a reader, so the
        // feed script never sees end of file between writes.
        let feed = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(p("feed"))
            .unwrap();
        write_script(
            &p("feed.sh"),
            &format!("exec cat '{}'\n", p("feed").display()),
        );
        write_script(
            &p("notredctl"),
            &format!(
                "echo \"$*\" >> '{calls}'\n\
                 case \"$1\" in list) cat '{list}' 2>/dev/null || echo '[]' ;; esac\n",
                calls = p("calls.log").display(),
                list = p("list.json").display(),
            ),
        );

        let log = std::fs::File::create(p("poshanka.log")).unwrap();
        let child = sway
            .command(env!("CARGO_BIN_EXE_poshanka"))
            .arg("--config")
            .arg(p("config.toml"))
            .env("RUST_LOG", "libposhanka=debug,poshanka=debug")
            .env("NO_COLOR", "1")
            .envs(std::env::var_os("PSK_WAYLAND_DEBUG").map(|_| ("WAYLAND_DEBUG", "1")))
            .envs(env.iter().copied())
            .stdin(Stdio::null())
            .stdout(log.try_clone().unwrap())
            .stderr(log)
            .spawn()
            .expect("spawn poshanka");
        let mut poshanka = Self { child, feed, dir };
        poshanka.wait_for_log("start-up", "poshanka starting");
        poshanka
    }

    pub fn path(&self, name: &str) -> PathBuf {
        self.dir.path().join(name)
    }

    pub fn log(&self) -> String {
        std::fs::read_to_string(self.path("poshanka.log")).unwrap_or_default()
    }

    /// Write one raw line to the feed.
    pub fn send(&mut self, line: &str) {
        writeln!(self.feed, "{line}").unwrap();
    }

    /// Send an `update` event with these notifications (JSON objects).
    pub fn update(&mut self, items: &[String]) {
        let n = self.log().matches("feed update").count();
        self.send(&format!(
            r#"{{"v": 1, "type": "event", "event": {{"kind": "update", "items": [{}]}}}}"#,
            items.join(", ")
        ));
        self.wait_for_log_count("the update", "feed update", n + 1);
    }

    /// [`Self::update`], then wait until the stack is painted.
    pub fn show(&mut self, items: &[String]) {
        let n = self.log().matches("stack painted").count();
        self.update(items);
        self.wait_for_log_count("the paint", "stack painted", n + 1);
    }

    /// Wait until the log contains `needle` `n` times or more.
    pub fn wait_for_log_count(&mut self, what: &str, needle: &str, n: usize) {
        let found = try_wait_for(|| {
            if let Ok(Some(status)) = self.child.try_wait() {
                panic!("poshanka exited ({status}):\n{}", self.log());
            }
            (self.log().matches(needle).count() >= n).then_some(())
        });
        if found.is_none() {
            panic!(
                "timed out waiting for {what}; poshanka's log:\n{}",
                self.log()
            );
        }
    }

    pub fn wait_for_log(&mut self, what: &str, needle: &str) {
        self.wait_for_log_count(what, needle, 1);
    }

    /// What poshanka ran `notredctl` with, one call per line.
    pub fn calls(&self) -> String {
        std::fs::read_to_string(self.path("calls.log")).unwrap_or_default()
    }

    pub fn wait_for_call(&self, call: &str) {
        try_wait_for(|| self.calls().lines().any(|l| l == call).then_some(())).unwrap_or_else(
            || {
                panic!(
                    "no `notredctl {call}`; calls:\n{}\nlog:\n{}",
                    self.calls(),
                    self.log()
                )
            },
        );
    }

    /// A virtual pointer, once poshanka has bound the seat's pointer (input
    /// sent before that is lost).
    pub fn pointer(&mut self, sway: &Sway) -> Pointer {
        let pointer = sway.pointer();
        self.wait_for_log("pointer bound", "pointer bound");
        pointer
    }

    pub fn try_wait(&mut self) -> Option<ExitStatus> {
        self.child.try_wait().unwrap()
    }

    pub fn pid(&self) -> u32 {
        self.child.id()
    }
}

impl Drop for Poshanka {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn write_script(path: &Path, body: &str) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::write(path, format!("#!/bin/sh\n{body}")).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

/// Wait until the card area at `index` shows (or no longer shows) the card
/// background.
pub fn wait_for_card(sway: &Sway, index: u32, shown: bool) -> Image {
    let (x, y) = card_origin(index);
    try_wait_for(|| {
        let img = sway.screenshot(x, y, CARD_W, CARD_H);
        ((img.count(CARD_BG) > 0) == shown).then_some(img)
    })
    .unwrap_or_else(|| {
        panic!(
            "card {index} is {} after 10 s",
            if shown { "not shown" } else { "still shown" }
        )
    })
}
