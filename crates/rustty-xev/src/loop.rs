//! loop.rs — faithful port of libxev `loop.zig` (D6, XEV_BLUEPRINT §2/§8 chunk C3).
//!
//! Common loop structures. The actual loop implementation is in backend-specific
//! files such as linux/io_uring.zig.
//
// PORT NOTE: `loop.zig`'s `Callback`/`NoopCallback` are `comptime fn(T: type)`
// forms that reference `T.Loop`/`T.Completion`/`T.Result`. On macOS the single
// backend candidate is kqueue (XEV_BLUEPRINT §0: `xev.Dynamic` == the static
// kqueue `Api()`), so those two comptime-generic decls MONOMORPHIZE to exactly
// one instantiation, `T = kqueue`. Rust has no comptime-generic-over-module, so
// they are ported here as CONCRETE decls referencing `crate::kqueue::{Loop,
// Completion, Result}` (the sole macOS instantiation). The pure, backend-agnostic
// decls (`Options`, `RunMode`, `CallbackAction`, `CompletionState`) are ported
// verbatim.

use crate::kqueue::{Completion, Loop};

// PORT NOTE: `const std = @import("std");` / `const assert = std.debug.assert;` /
// `const xev = @import("main.zig");` — the Rust equivalents are the concrete
// `crate::kqueue` types imported above (the macOS single-backend `xev`).

/// Common options across backends. Not all options apply to all backends.
/// Read the doc comment for individual fields to learn what backends they
/// apply to.
//
// PORT NOTE [ADV-FIX F3]: hand-written `Default` (NOT `#[derive]`) — the Zig
// field defaults are `entries = 256`, `thread_pool = null`, which `#[derive]`
// would render as `0`/`None`; the faithful defaults are set explicitly below.
// `thread_pool: ?*xev.ThreadPool` ports as a raw `*mut ThreadPool` option (the
// pool BODY is deferred, only the type surface is ported — §1/§6).
pub struct Options {
    /// The number of queued completions that can be in flight before
    /// requiring interaction with the kernel.
    ///
    /// Backends: io_uring
    pub entries: u32,

    /// A thread pool to use for blocking operations. If the backend doesn't
    /// need to perform any blocking operations then no threads will ever
    /// be spawned. If the backend does need to perform blocking operations
    /// on a thread and no thread pool is provided, the operations will simply
    /// fail. Unless you're trying to really optimize for space, it is
    /// recommended you provide a thread pool.
    ///
    /// Backends: epoll, kqueue
    pub thread_pool: Option<*mut crate::thread_pool::ThreadPool>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            entries: 256,
            thread_pool: None,
        }
    }
}

/// The loop run mode -- all backends are required to support this in some way.
/// Backends may provide backend-specific APIs that behave slightly differently
/// or in a more configurable way.
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

/// The callback of the main Loop operations. Higher level interfaces may
/// use a different callback mechanism.
//
// PORT NOTE: the reference `pub fn Callback(comptime T: type) type` returns
// `*const fn (userdata: ?*anyopaque, loop: *T.Loop, completion: *T.Completion,
// result: T.Result) CallbackAction`. Monomorphized for the single macOS backend
// (T = kqueue), it is this concrete fn-pointer type. Per the
// `rust-mut-args-strongly-protected` memory, `loop`/`completion` are RAW pointers
// (`*mut`), NEVER `&mut`/lifetimes — the loop holds these pointers into caller
// memory and re-enters them across ticks (a `&mut` re-read would be Stacked-
// Borrows UB). A plain Rust `fn` pointer (not `extern "C"`) preserves the Zig
// zero-cost fn-pointer dispatch.
pub type Callback = fn(
    userdata: *mut core::ffi::c_void,
    loop_: *mut Loop,
    completion: *mut Completion,
    result: crate::kqueue::Result,
) -> CallbackAction;

/// A callback that does nothing and immediately disarms. This
/// implements xev.Callback and is the default value for completions.
//
// PORT NOTE: the reference `pub fn NoopCallback(comptime T: type) Callback(T)`
// returns the inner `noopCallback` fn. Monomorphized (T = kqueue), it IS this
// `noop_callback` fn item — the default value for `Completion.callback`.
pub fn noop_callback(
    _: *mut core::ffi::c_void,
    _: *mut Loop,
    _: *mut Completion,
    _: crate::kqueue::Result,
) -> CallbackAction {
    CallbackAction::Disarm
}

/// The result type for callbacks. This should be used by all loop
/// implementations and higher level abstractions in order to control
/// what to do after the loop completes.
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallbackAction {
    /// The request is complete and is not repeated. For example, a read
    /// callback only fires once and is no longer watched for reads. You
    /// can always free memory associated with the completion prior to
    /// returning this.
    Disarm = 0,

    /// Requeue the same operation request with the same parameters
    /// with the event loop. This makes it easy to repeat a read, timer,
    /// etc. This rearms the request EXACTLY as-is. For example, the
    /// low-level timer interface for io_uring uses an absolute timeout.
    /// If you rearm the timer, it will fire immediately because the absolute
    /// timeout will be in the past.
    ///
    /// The completion is reused so it is not safe to use the same completion
    /// for anything else.
    Rearm = 1,
}

/// The state that a completion can be in.
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompletionState {
    /// The completion is not being used and is ready to be configured
    /// for new work.
    Dead = 0,

    /// The completion is part of an event loop. This may be already waited
    /// on or in the process of being registered.
    Active = 1,
}
