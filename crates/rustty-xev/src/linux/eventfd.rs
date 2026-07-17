//! linux/eventfd.rs — the eventfd wakeup primitive (XEV_BLUEPRINT §1 Linux
//! port, task L1 LT2).
//!
//! This is the standalone fd-level primitive libxev's `watcher/async.zig`
//! `AsyncEventFd` variant wraps for the io_uring/epoll backends. The loop-
//! integrated `Async` watcher (its `wait`/callback wiring) is Task 3; this
//! module only ports the raw eventfd(2) init/notify/drain/close surface.
//!
//! eventfd's kernel-side u64 counter IS the coalescing mechanism `drain`
//! exploits: repeated `notify()` calls each add 1 to that counter, and a
//! single `read` drains the accumulated total back out in one shot (mirrors
//! libxev `AsyncEventFd.notify`'s doc comment: "async messages may be
//! coalesced").

use std::io;
use std::os::unix::io::RawFd;

/// Eventfd is a wrapper around the eventfd system call — the wakeup
/// primitive the epoll/io_uring `Async` watchers (Tasks 3+) wrap.
///
/// `Copy`: this wraps a raw fd (an `i32`), exactly like the Zig
/// `AsyncEventFd` value type.
#[derive(Debug, Clone, Copy)]
pub struct Eventfd {
    /// The eventfd file descriptor for use with poll, epoll, io_uring, etc.
    pub fd: RawFd,
}

impl Eventfd {
    /// `eventfd(0, EFD_CLOEXEC | EFD_NONBLOCK)`.
    pub fn init() -> io::Result<Self> {
        let res = unsafe { libc::eventfd(0, libc::EFD_CLOEXEC | libc::EFD_NONBLOCK) };
        if res < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(Self { fd: res })
    }

    /// `write(fd, &1u64)` — increments the kernel-side counter by 1.
    ///
    /// A `WouldBlock` write (the counter is already at its max value, i.e.
    /// the eventfd is "full") is treated as success: the fd is guaranteed to
    /// stay readable either way, so the loop will still wake up. Mirrors
    /// libxev `AsyncEventFd.notify`'s `error.WouldBlock => return` arm.
    pub fn notify(&self) -> io::Result<()> {
        let buf = 1u64.to_ne_bytes();
        let res = unsafe { libc::write(self.fd, buf.as_ptr().cast(), buf.len()) };
        if res < 0 {
            let err = io::Error::last_os_error();
            if err.kind() == io::ErrorKind::WouldBlock {
                return Ok(());
            }
            return Err(err);
        }
        Ok(())
    }

    /// `read(fd, &mut u64)` — drains (and resets to 0) the kernel-side
    /// counter, returning the coalesced notify count. Returns a `WouldBlock`
    /// error if no notification is currently pending (the counter is 0).
    pub fn drain(&self) -> io::Result<u64> {
        let mut buf = [0u8; 8];
        let res = unsafe { libc::read(self.fd, buf.as_mut_ptr().cast(), buf.len()) };
        if res < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(u64::from_ne_bytes(buf))
    }

    /// `close(fd)`.
    pub fn deinit(&self) {
        unsafe {
            libc::close(self.fd);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Standalone (no loop) fd-level test per the Task-2 brief: notify twice,
    /// confirm `drain` returns the coalesced count of 2, then confirm a
    /// second `drain` (nothing pending) returns `WouldBlock`.
    #[test]
    fn eventfd_notify_drain() {
        let e = Eventfd::init().expect("eventfd init");

        e.notify().expect("notify #1");
        e.notify().expect("notify #2");

        let count = e.drain().expect("drain should coalesce both notifies");
        assert_eq!(count, 2, "drain should return the coalesced notify count");

        let err = e
            .drain()
            .expect_err("second drain should block (no pending notify)");
        assert_eq!(err.kind(), io::ErrorKind::WouldBlock);

        e.deinit();
    }
}
