//! `SIGTERM`/`SIGINT` as a readable file descriptor, so the Wayland loop can
//! poll for them and stop cleanly.
//!
//! A handler writes a byte to a pipe (the self-pipe trick). Unlike blocking
//! the signals for a `signalfd`, this leaves the signal mask alone, which
//! child processes would inherit; handlers are reset to the default by
//! `exec`, so children such as the feed script still die on `SIGTERM`.

use std::os::fd::{FromRawFd, OwnedFd};
use std::sync::atomic::{AtomicI32, Ordering};

/// Write end of the pipe, for the handler.
static PIPE_WRITE: AtomicI32 = AtomicI32::new(-1);

extern "C" fn on_signal(_: libc::c_int) {
    let fd = PIPE_WRITE.load(Ordering::Relaxed);
    if fd >= 0 {
        // SAFETY: write(2) is async-signal-safe; the fd is non-blocking, and
        // a full pipe (a stop already pending) loses nothing.
        unsafe {
            libc::write(fd, [1u8].as_ptr().cast(), 1);
        }
    }
}

/// Install handlers for `SIGTERM` and `SIGINT` and return a descriptor that
/// becomes readable when one arrives; `None` (default handling kept) if that
/// fails.
pub fn stop_signals() -> Option<OwnedFd> {
    let mut fds = [0; 2];
    // SAFETY: pipe2 fills `fds` with two new descriptors on success.
    if unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC | libc::O_NONBLOCK) } != 0 {
        return None;
    }
    // SAFETY: both descriptors are new and owned by nothing else; the write
    // end is kept open for the handler for the rest of the process.
    let read = unsafe { OwnedFd::from_raw_fd(fds[0]) };
    PIPE_WRITE.store(fds[1], Ordering::Relaxed);

    for signal in [libc::SIGTERM, libc::SIGINT] {
        // SAFETY: a zeroed sigaction with a handler that only calls write(2).
        let installed = unsafe {
            let mut action: libc::sigaction = std::mem::zeroed();
            action.sa_sigaction = on_signal as extern "C" fn(libc::c_int) as libc::sighandler_t;
            action.sa_flags = libc::SA_RESTART;
            libc::sigemptyset(&mut action.sa_mask);
            libc::sigaction(signal, &action, std::ptr::null_mut()) == 0
        };
        if !installed {
            return None;
        }
    }
    Some(read)
}

#[cfg(test)]
mod tests;
