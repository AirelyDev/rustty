//! linux.rs — Linux-only shared primitives (XEV_BLUEPRINT §1 Linux port, task
//! L1 LT2).
//!
//! Groups the Linux-only helper bindings (`timerfd`, `eventfd`) the way
//! `darwin`/`kqueue` group the macOS-only ones, PLUS byte-faithful TWINS of
//! `loop.rs`'s pure, backend-agnostic ABI enums (`RunMode`/`CallbackAction`/
//! `CompletionState`/`Options`) — see the PORT NOTE below for why these are
//! twins rather than a shared import.
//!
//! **Scope (per STRUCTURE CORRECTION, Task 2):** ONLY the standalone `linux/`
//! primitives both Linux backends (epoll, io_uring — Tasks 3+) build on. NO
//! event loop yet: `timerfd`/`eventfd` are plain fd wrappers with standalone
//! fd-level tests (`poll`/`read`/`write` directly), not loop-driven watchers.
//
// PORT NOTE (R5 / no cross-import from macOS-gated files): `loop.rs` defines
// these same four decls, but it is `#[cfg(target_os = "macos")]`-gated (see
// lib.rs) and its `Callback`/`noop_callback` are coupled to the concrete
// kqueue `Loop`/`Completion`/`Result` types — importing from it here would
// pull macOS-only code into the Linux build. `RunMode`/`CallbackAction`/
// `CompletionState`/`Options`, however, are the PURE, backend-agnostic slice
// of that file (no kqueue coupling), so they are ported a second time here,
// byte-faithfully (same variants/discriminant values/derives), as the shared
// ABI both Linux backends and the eventual `linux::Loop` will use.

#![allow(dead_code)]

pub mod eventfd;
pub mod timerfd;

/// TWIN of `loop.rs`'s `Options` (byte-faithful: same fields/derives/hand-
/// written `Default`).
//
// PORT NOTE: the Zig field defaults are `entries = 256`, `thread_pool = null`,
// which `#[derive(Default)]` would render as `0`/`None` — the faithful
// defaults are set explicitly below, exactly as `loop.rs` does.
//
// PORT NOTE: `loop.rs`'s twin of this field names the concrete macOS-gated
// `*mut crate::thread_pool::ThreadPool` (that module doesn't exist on Linux —
// R5 forbids reaching into it anyway). The Linux `ThreadPool` body is a
// whole follow-up port (same "type surface only, body deferred" status as
// macOS's), so this field is an opaque raw pointer option instead: same
// shape (`Option<*mut _>`), no macOS-only type name.
pub struct Options {
    /// The number of queued completions that can be in flight before
    /// requiring interaction with the kernel.
    ///
    /// Backends: io_uring
    pub entries: u32,

    /// A thread pool to use for blocking operations. If the backend doesn't
    /// need to perform any blocking operations then no threads will ever be
    /// spawned. If the backend does need to perform blocking operations on a
    /// thread and no thread pool is provided, the operations will simply
    /// fail. Unless you're trying to really optimize for space, it is
    /// recommended you provide a thread pool.
    ///
    /// Backends: epoll, io_uring
    pub thread_pool: Option<*mut core::ffi::c_void>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            entries: 256,
            thread_pool: None,
        }
    }
}

/// TWIN of `loop.rs`'s `RunMode` (byte-faithful: same variants/discriminant
/// values/derives).
///
/// The loop run mode -- all backends are required to support this in some
/// way. Backends may provide backend-specific APIs that behave slightly
/// differently or in a more configurable way.
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunMode {
    /// Run the event loop once. If there are no blocking operations ready,
    /// return immediately.
    NoWait = 0,

    /// Run the event loop once, waiting for at least one blocking operation
    /// to complete.
    Once = 1,

    /// Run the event loop until it is "done". "Doneness" is defined as
    /// there being no more completions that are active.
    UntilDone = 2,
}

/// TWIN of `loop.rs`'s `CallbackAction` (byte-faithful: same variants/
/// discriminant values/derives).
///
/// The result type for callbacks. This should be used by all loop
/// implementations and higher level abstractions in order to control what to
/// do after the loop completes.
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallbackAction {
    /// The request is complete and is not repeated. For example, a read
    /// callback only fires once and is no longer watched for reads. You can
    /// always free memory associated with the completion prior to returning
    /// this.
    Disarm = 0,

    /// Requeue the same operation request with the same parameters with the
    /// event loop. This makes it easy to repeat a read, timer, etc. This
    /// rearms the request EXACTLY as-is. For example, the low-level timer
    /// interface for io_uring uses an absolute timeout. If you rearm the
    /// timer, it will fire immediately because the absolute timeout will be
    /// in the past.
    ///
    /// The completion is reused so it is not safe to use the same completion
    /// for anything else.
    Rearm = 1,
}

/// TWIN of `loop.rs`'s `CompletionState` (byte-faithful: same variants/
/// discriminant values/derives).
///
/// The state that a completion can be in.
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompletionState {
    /// The completion is not being used and is ready to be configured for
    /// new work.
    Dead = 0,

    /// The completion is part of an event loop. This may be already waited
    /// on or in the process of being registered.
    Active = 1,
}
