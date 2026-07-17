//! linux/timerfd.rs — port of libxev's `linux/timerfd.zig` (XEV_BLUEPRINT §1
//! Linux port, task L1 LT2).
//!
//! Timerfd is a wrapper around the timerfd system calls. See the
//! timerfd_create(2) man page for information on timerfd and associated
//! system calls.
//!
//! This is a small wrapper around timerfd to make it slightly more pleasant
//! to use, but may not expose all available functionality. For maximum
//! control you should use the syscalls directly.
//
// PORT NOTE: the Zig `set`/`get` surface takes the full `TFD.TIMER` flags +
// `itimerspec` (`Spec`) + optional `old_value` out-param. Per the Task-2 brief
// interface (`set(next_ms)`, `get()`), this is simplified to the one-shot
// relative-timer subset the epoll/io_uring `Timer` watchers (Tasks 3+) need:
// `set` arms (or disarms, at `next_ms == 0`) a non-repeating relative timer;
// `get` returns the remaining time until the next expiration. The underlying
// syscalls (`timerfd_create`/`timerfd_settime`/`timerfd_gettime`,
// `CLOCK_MONOTONIC`, `TFD_NONBLOCK | TFD_CLOEXEC`) are ported verbatim.

use std::io;
use std::os::unix::io::RawFd;
use std::time::Duration;

/// Timerfd is a wrapper around the timerfd system calls.
///
/// `Copy`: this wraps a raw fd (an `i32`), exactly like the Zig `Timerfd`
/// value type.
#[derive(Debug, Clone, Copy)]
pub struct Timerfd {
    /// The timerfd file descriptor for use with poll, epoll, io_uring, etc.
    pub fd: RawFd,
}

impl Timerfd {
    /// `timerfd_create(CLOCK_MONOTONIC, TFD_NONBLOCK | TFD_CLOEXEC)`.
    pub fn init() -> io::Result<Self> {
        let res = unsafe {
            libc::timerfd_create(
                libc::CLOCK_MONOTONIC,
                libc::TFD_NONBLOCK | libc::TFD_CLOEXEC,
            )
        };
        if res < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(Self { fd: res })
    }

    /// `timerfd_settime` — arms a one-shot (non-repeating) relative timer
    /// that expires `next_ms` milliseconds from now. Passing `0` disarms the
    /// timer (per `timerfd_settime(2)`, a zero `it_value` disarms).
    pub fn set(&self, next_ms: u64) -> io::Result<()> {
        let new_value = libc::itimerspec {
            it_interval: libc::timespec {
                tv_sec: 0,
                tv_nsec: 0,
            },
            it_value: duration_to_timespec(Duration::from_millis(next_ms)),
        };
        let res = unsafe { libc::timerfd_settime(self.fd, 0, &new_value, std::ptr::null_mut()) };
        if res < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }

    /// `timerfd_gettime` — the remaining time until the next expiration.
    pub fn get(&self) -> io::Result<Duration> {
        let mut out: libc::itimerspec = unsafe { std::mem::zeroed() };
        let res = unsafe { libc::timerfd_gettime(self.fd, &mut out) };
        if res < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(timespec_to_duration(out.it_value))
    }

    /// `close(fd)`.
    pub fn deinit(&self) {
        unsafe {
            libc::close(self.fd);
        }
    }
}

fn duration_to_timespec(d: Duration) -> libc::timespec {
    libc::timespec {
        tv_sec: d.as_secs() as libc::time_t,
        tv_nsec: libc::c_long::from(d.subsec_nanos() as i32),
    }
}

fn timespec_to_duration(ts: libc::timespec) -> Duration {
    Duration::new(ts.tv_sec.max(0) as u64, ts.tv_nsec.max(0) as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Standalone (no loop) fd-level test per the Task-2 brief: init, arm a
    /// short one-shot timer, poll the raw fd until readable, and confirm
    /// `read(2)` yields the u64 expiration count timerfd(2) documents.
    #[test]
    fn timerfd_fires() {
        let t = Timerfd::init().expect("timerfd init");

        t.set(5).expect("timerfd set(5ms)");

        let mut pfd = libc::pollfd {
            fd: t.fd,
            events: libc::POLLIN,
            revents: 0,
        };
        // Generous bound (2s) around a 5ms timer: a hang here indicates a
        // real bug, not test flakiness.
        let n = unsafe { libc::poll(&mut pfd, 1, 2000) };
        assert_eq!(
            n, 1,
            "poll should report exactly one ready fd (timerfd never became readable)"
        );
        assert_ne!(
            pfd.revents & libc::POLLIN,
            0,
            "timerfd should be POLLIN-readable"
        );

        // read(2) on a timerfd yields the u64 expiration count.
        let mut buf = [0u8; 8];
        let r = unsafe { libc::read(t.fd, buf.as_mut_ptr().cast(), buf.len()) };
        assert_eq!(r, 8, "timerfd read should yield 8 bytes");
        let count = u64::from_ne_bytes(buf);
        assert!(count >= 1, "expiration count should be >= 1, got {count}");

        t.deinit();
    }
}
