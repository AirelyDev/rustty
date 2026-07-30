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

/// PORT NOTE: `pub const Backend = enum { io_uring, epoll, kqueue, wasi_poll, iocp }`.
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
// PORT NOTE: `pub fn default() Backend` mapped to Rust's `Default` trait so the call
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
        // PORT NOTE: `.wasi => .wasi_poll` — the reference's wasi arm. The
        // WasiPoll backend body itself is a whole follow-up port (the wasm build
        // runs the synchronous C-ABI with no event loop, so the placeholder
        // Loop/Async no-op types in `lib.rs`'s non-macOS seam stand in for it);
        // `default()`/`candidates()` still name it faithfully so the crate
        // compiles for wasm32 (backend selection is comptime here).
        // WS14 T6 (cfg widening ONLY — no logic change): also select this arm for
        // freestanding wasm. The reference's target enum distinguishes `.wasi` from
        // bare `wasm32-freestanding`; the port treats BOTH as the single-candidate
        // wasi arm, because rustty's wasm builds (wasip1 AND the freestanding
        // browser cdylib) run the SYNCHRONOUS C-ABI with no event loop — the same
        // no-op Loop/Async seam either way. Without this a
        // `wasm32-unknown-unknown` build hits the `compile_error!` below even
        // though nothing in the wasm module ever constructs a Loop.
        #[cfg(all(target_family = "wasm", not(target_os = "emscripten")))]
        {
            Backend::WasiPoll
        }
        #[cfg(not(any(
            target_os = "macos",
            target_os = "ios",
            target_os = "freebsd",
            target_os = "linux",
            target_os = "windows",
            all(target_family = "wasm", not(target_os = "emscripten"))
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
    // epoll]`; the out-of-scope-by-platform targets (iocp/wasi_poll) return their
    // single arm — the enum names every backend even where no body is built.
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
        // PORT NOTE: `.wasi => &.{.wasi_poll}` — the single-candidate wasi arm
        // (mirrors the macOS single-candidate shape; the WasiPoll BODY is out of scope
        // by platform — see `lib.rs`'s `unsupported_target_seam` and DEFERRALS.md's
        // `# Justified R1 Divergences`).
        // WS14 T6: same cfg widening as `default()` above.
        #[cfg(all(target_family = "wasm", not(target_os = "emscripten")))]
        {
            &[Backend::WasiPoll]
        }
        #[cfg(not(any(
            target_os = "macos",
            target_os = "ios",
            target_os = "freebsd",
            target_os = "linux",
            target_os = "windows",
            all(target_family = "wasm", not(target_os = "emscripten"))
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
        // Off-macOS the enum still round-trips (iocp/wasi_poll have no body: out of
        // scope by platform).
        #[cfg(not(target_os = "macos"))]
        {
            let _ = Backend::default();
            assert!(!Backend::candidates().is_empty());
        }
    }

    // XEV_BLUEPRINT Linux port, task L1 LT1: strengthens the weaker
    // `backend_default_and_candidates` off-macOS assertion above with the exact
    // linux values (backend.zig:19-41's `.linux => .io_uring` / `.linux =>
    // &.{ .io_uring, .epoll }` arms).
    #[cfg(target_os = "linux")]
    #[test]
    fn linux_candidates_and_default() {
        assert_eq!(Backend::default(), Backend::IoUring);
        assert_eq!(Backend::candidates(), &[Backend::IoUring, Backend::Epoll]);
    }
}
