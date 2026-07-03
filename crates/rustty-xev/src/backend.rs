//! backend.rs — the full port of libxev `backend.zig` (D6, XEV_BLUEPRINT §1/§7).
//!
//! `Backend` is the enum of every libxev backend. On macOS `Backend::default()`
//! is `kqueue` and `candidates()` is the single-element `&[kqueue]` — the one fact
//! that shapes everything (XEV_BLUEPRINT §0): a single candidate makes
//! `dynamic.zig`'s `Xev(bes)` collapse to `bes[0].Api()`, i.e. the static kqueue
//! `Api()`, so the whole Dynamic layer is comptime-elided (deferred, not weakened).
//!
//! Returns the Api (return value of Xev) for the given backend type.
//! `Api(self)` in Zig returns a comptime `type` (the backend's `Xev(be, T)`
//! struct). Rust has no runtime type-returning function, and on macOS the sole live
//! arm — `kqueue` — collapses to this crate's own public surface (`lib.rs`, the
//! `main.zig`/`api.zig` re-exports). So `Api()` has NO runtime analogue and is
//! intentionally not ported as a method; the crate module IS the kqueue `Api()`.

/// PORT: `pub const Backend = enum { io_uring, epoll, kqueue, wasi_poll, iocp }`.
///
/// The backend types. Every variant is preserved (R3/R5) even though only
/// `kqueue` is live on macOS — the others are whole follow-up platform ports
/// (DEFERRALS.md), kept here so `default()`/`candidates()` read faithfully.
//
// PORT NOTE: `#[repr(C)]` — the reference is a plain Zig `enum` (tag-only); the
// C-repr keeps a stable discriminant should a later C-ABI consumer name it. The
// derives mirror a trivially-copyable comparable Zig enum value.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    IoUring,
    Epoll,
    Kqueue,
    WasiPoll,
    Iocp,
}

/// Returns a recommend default backend from inspecting the system.
//
// PORT: `pub fn default() Backend` mapped to Rust's `Default` trait so the call
// site reads identically (`Backend::default()`), and clippy stays clean.
//
// PORT NOTE: the reference switches on `builtin.os.tag` (a comptime value); the
// faithful Rust form is `#[cfg(target_os = ...)]` arms. macOS/iOS/FreeBSD →
// kqueue; the other targets (linux → io_uring, wasi → wasi_poll, windows → iocp)
// are DEFERRED whole-backend ports (XEV_BLUEPRINT §1). The reference's
// `else => @compileError("no default backend")` maps to a `compile_error!` on an
// unsupported target.
impl Default for Backend {
    fn default() -> Backend {
        #[cfg(any(target_os = "macos", target_os = "ios", target_os = "freebsd"))]
        {
            Backend::Kqueue
        }
        #[cfg(target_os = "linux")]
        {
            Backend::IoUring
        }
        #[cfg(target_os = "windows")]
        {
            Backend::Iocp
        }
        #[cfg(not(any(
            target_os = "macos",
            target_os = "ios",
            target_os = "freebsd",
            target_os = "linux",
            target_os = "windows"
        )))]
        {
            compile_error!("no default backend for this target")
        }
    }
}

impl Backend {
    /// Candidate backends for this platform in priority order.
    //
    // PORT NOTE: returns a `&'static [Backend]` (the reference's
    // `[]const Backend`). On macOS/iOS this is the single-element `&[kqueue]` — the
    // len==1 that comptime-elides the Dynamic layer (§0). linux → `&[io_uring,
    // epoll]`; the deferred targets return their single arm.
    pub fn candidates() -> &'static [Backend] {
        #[cfg(any(target_os = "macos", target_os = "ios", target_os = "freebsd"))]
        {
            &[Backend::Kqueue]
        }
        #[cfg(target_os = "linux")]
        {
            &[Backend::IoUring, Backend::Epoll]
        }
        #[cfg(target_os = "windows")]
        {
            &[Backend::Iocp]
        }
        #[cfg(not(any(
            target_os = "macos",
            target_os = "ios",
            target_os = "freebsd",
            target_os = "linux",
            target_os = "windows"
        )))]
        {
            compile_error!("no candidate backends for this target")
        }
    }
}

// ---------------------------------------------------------------------------
// rustty-only sanity test (backend.zig carries 0 `test{}`; this is a +margin
// helper, faithful to the `default()==kqueue`/single-candidate contract §0).
// ---------------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backend_default_and_candidates() {
        // On macOS the sole live arm: default is kqueue and the candidate list is
        // the single element that comptime-elides the Dynamic layer (§0).
        #[cfg(target_os = "macos")]
        {
            assert_eq!(Backend::default(), Backend::Kqueue);
            assert_eq!(Backend::candidates(), &[Backend::Kqueue]);
            assert_eq!(Backend::candidates().len(), 1);
        }
        // Off-macOS the enum still round-trips (whole backends deferred).
        #[cfg(not(target_os = "macos"))]
        {
            let _ = Backend::default();
            assert!(!Backend::candidates().is_empty());
        }
    }
}
