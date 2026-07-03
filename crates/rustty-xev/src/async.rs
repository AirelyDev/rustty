//! async.rs — faithful port of libxev `watcher/async.zig` `AsyncMachPort` (D6,
//! XEV_BLUEPRINT §4/§7/§8, chunk C4; the C3 lifecycle subset landed earlier).
//!
//! Reference (async.zig:1): "Wake up" an event loop from any thread using an async completion.
//!
//! `AsyncMachPort` is the Darwin cross-thread wakeup handle. The loop `Wakeup`
//! (kqueue.rs) EMBEDS an `Async` and drives it at `Loop.init`/`tick`/`wakeup`,
//! and termio/core name `xev::Async` (`renderer_wakeup`, mailbox wakeups) and
//! default/Copy/`notify()` it. C4 completes the watcher surface: `wait` (the
//! `machport`-op trampoline) + `drain` (coalesces N notifies → 1 wakeup) + the
//! `AsyncTests` (async×3) + rustty KAT 1 (cross-thread wakeup round-trip). The
//! non-Darwin `AsyncEventFd`/`AsyncLoopState`/`AsyncIOCP`/`AsyncDynamic` variants
//! are comptime-elided on macOS (§0) and ported later as whole backends (§1);
//! their reference doc comments are preserved below (R3).
//
// PORT NOTE: the reference is `fn AsyncMachPort(comptime xev: type) type` — a
// comptime-generic struct. Monomorphized for the single macOS backend (xev =
// kqueue, §0), it is this concrete `Async`. It is `#[cfg(target_os = "macos")]`
// (Darwin mach ports) exactly as the whole kqueue port is.

// (module is `#[cfg(target_os = "macos")]`-gated in lib.rs)
// PORT NOTE: `libc::mach_task_self()` is `#[deprecated]` in libc (it nudges toward
// the `mach2` crate), but this port binds `libc` directly (R3, no new dep) exactly
// as the reference uses `posix.system.mach_task_self()`. The call is correct; the
// deprecation is a libc-crate policy, so it is allowed here.
#![allow(deprecated)]

use crate::darwin;
use crate::kqueue::{Completion, Loop, MachPortError, Operation, ReadBuffer, Result};
use crate::looppkg::CallbackAction;
use crate::{Error, NotifyError};
use core::ffi::c_void;
use core::ptr::NonNull;

// ---------------------------------------------------------------------------
// PORT (§0/§7): the sibling `Async` variants are COMPTIME-ELIDED on macOS and
// DEFERRED (not weakened). The reference `pub fn Async(comptime xev: type) type`
// dispatches on `xev.backend`: `.io_uring`/`.epoll` -> `AsyncEventFd`,
// `.wasi_poll` -> `AsyncLoopState`, `.iocp` -> `AsyncIOCP`, and (the live arm)
//     // Supported, uses mach port on Darwin and eventfd on BSD.
//     .kqueue => if Darwin then AsyncMachPort else AsyncEventFd
// On macOS the single backend candidate is kqueue + Darwin, so ONLY
// `AsyncMachPort` (below) instantiates. The others are ported as whole backends
// later (blueprint §1); their reference doc comments are preserved here (R3) so
// nothing is dropped:
//
//   The dispatcher `switch (xev.backend)` arms:
//     // Supported, uses eventfd
//     // Supported, uses the backend API
//     // Supported, uses mach port on Darwin and eventfd on BSD.
//
//   AsyncEventFd (Unix/Linux) — verbatim reference comment lines (R3):
//     /// Async implementation using eventfd (Unix/Linux).
//     // This is only used for FreeBSD currently.
//     // std.posix is unavailable on FreeBSD. We call the
//     // syscall directly.
//     // Use std.posix if we can.
//     // We use a poll operation instead of a read operation
//     // because in Kernel 6.15.4, read was regressed for
//     // io_uring on eventfd/timerfd and would block forever.
//     // However, poll works fine.
//     // We need to read so that we can consume the
//     // eventfd value. We only read 8 bytes because
//     // we only write up to 8 bytes and we own the fd.
//     // We ignore errors here because we expect the
//     // read to succeed given we just polled it.
//     // We'll call the callback with the error later.
//     // We want to just write "1" in the correct byte order as our host.
//     /// The "c" value is the completion associated with the "wait".
//     /// Internal details subject to change but if you're relying on these
//     /// details then you may want to consider using a lower level interface
//     /// using the loop directly:
//     ///   - linux+io_uring: eventfd is used. If the eventfd write would block
//     ///     (EAGAIN) then we assume success because the eventfd is full.
//
//   AsyncLoopState (WASI):
//     /// Async implementation that is deferred to the backend implementation
//     /// loop state. This is kind of a hacky implementation and not recommended
//     /// but its the only way currently to get asyncs to work on WASI.
//     // TODO: we don't support threaded loop state async. We _can_ it just
//     // isn't done yet. To support it we need to have some sort of mutex
//     // to guard waiter below.
//
//   AsyncIOCP (Windows) + AsyncDynamic (multi-backend): code-only, no
//   distinctive doc comments; deferred with the other backends.
// ---------------------------------------------------------------------------

/// PORT: `pub const WaitError = xev.Sys.MachPortError;`.
/// The error that can come in the wait callback.
pub type WaitError = MachPortError;

/// Async implementation using mach ports (Darwin).
///
/// This allocates a mach port per async request and sends to that mach
/// port to wake up the loop and trigger the completion.
//
// PORT NOTE: `AsyncMachPort` monomorphized. `#[derive(Clone, Copy)]`: the
// reference `wait`/`notify` take `self: Self` by value (a trivially-copyable
// `port` handle) and ghostty copies the handle by value (`renderer_wakeup =
// render_thread.wakeup`); the `port: u32` field makes it auto-`Send` (the
// cross-thread `notify()` handle, §2 F2).
//
// PORT NOTE: the reference struct also declares these bindings inline:
//   // Missing Mach APIs from Zig stdlib. Data from xnu: osfmk/mach/port.h
// (`mach_port_flavor_t`, `mach_port_limits`, `MACH_PORT_LIMITS_INFO`,
// `mach_port_set_attributes`, `mach_port_destroy`). This port relocates those to
// `darwin.rs` (chunk C2) — the shared darwin/syscall home — so `init`/`deinit`
// name `darwin::mach_port_set_attributes`/`darwin::mach_port_destroy` below.
#[derive(Clone, Copy)]
pub struct Async {
    /// The mach port
    pub(crate) port: darwin::mach_port_name_t,
}

impl Async {
    /// Create a new async. An async can be assigned to exactly one loop
    /// to be woken up. The completion must be allocated in advance.
    //
    // PORT NOTE: the reference error is `error{MachPortAllocFailed}`; per the D6
    // seam (blueprint §2.1 / termio `xev::Async::init()?`), every alloc failure
    // collapses to the unified `crate::Error`.
    pub fn init() -> core::result::Result<Async, Error> {
        // SAFETY: the mach syscalls below are the faithful `posix.system.*` /
        // async.zig-local mach bindings (darwin.rs); each is a plain C call with
        // POD in/out params and no aliasing.
        unsafe {
            let mach_self = libc::mach_task_self();

            // Allocate the port
            let mut mach_port: darwin::mach_port_name_t = 0;
            match darwin::getKernError(darwin::mach_port_allocate(
                mach_self,
                darwin::MACH_PORT_RIGHT::RECEIVE.0,
                &mut mach_port,
            )) {
                darwin::KernE::SUCCESS => {} // Success
                _ => return Err(Error),      // error.MachPortAllocFailed
            }
            // errdefer _ = mach_port_destroy(mach_self, mach_port);
            let insert = darwin::getKernError(darwin::mach_port_insert_right(
                mach_self,
                mach_port,
                mach_port,
                darwin::MACH_MSG_TYPE::MAKE_SEND.0,
            ));
            // Insert a send right into the port since we also use this to send
            match insert {
                darwin::KernE::SUCCESS => {} // Success
                _ => {
                    let _ = darwin::mach_port_destroy(mach_self, mach_port);
                    return Err(Error); // error.MachPortAllocFailed
                }
            }

            // Modify the port queue size to be 1 because we are only
            // using it for notifications and not for any other purpose.
            let mut limits = darwin::mach_port_limits { mpl_qlimit: 1 };
            let attrs = darwin::getKernError(darwin::mach_port_set_attributes(
                mach_self,
                mach_port,
                darwin::MACH_PORT_LIMITS_INFO,
                (&mut limits as *mut darwin::mach_port_limits).cast::<libc::c_void>(),
                (core::mem::size_of::<darwin::mach_port_limits>()
                    / core::mem::size_of::<darwin::natural_t>())
                    as darwin::mach_msg_type_number_t,
            ));
            match attrs {
                darwin::KernE::SUCCESS => {} // Success
                _ => {
                    let _ = darwin::mach_port_destroy(mach_self, mach_port);
                    return Err(Error); // error.MachPortAllocFailed
                }
            }

            Ok(Async { port: mach_port })
        }
    }

    /// Clean up the async. This will forcibly deinitialize any resources
    /// and may result in erroneous wait callbacks to be fired.
    //
    // PORT NOTE: the reference receiver is `self: *Self`; ported as `&mut self`
    // (termio binds `mut wakeup` and calls `wakeup.deinit()`).
    pub fn deinit(&mut self) {
        // SAFETY: destroying our own mach port; `self.port` is the RECEIVE right
        // allocated in `init`.
        unsafe {
            let _ = darwin::mach_port_destroy(libc::mach_task_self(), self.port);
        }
    }

    /// Wait for a message on this async. Note that async messages may be
    /// coalesced (or they may not be) so you should not expect a 1:1 mapping
    /// between send and wait.
    ///
    /// Just like the rest of libxev, the wait must be re-queued if you want
    /// to continue to be notified of async events.
    ///
    /// You should NOT register an async with multiple loops (the same loop
    /// is fine -- but unnecessary). The behavior when waiting on multiple
    /// loops is undefined.
    //
    // PORT NOTE (§2.2): the reference `comptime Userdata: type` + `userdata:
    // ?*Userdata` + `comptime cb: *const fn(...)` monomorphize to a generic
    // `wait::<UD, CB>` where `CB` is the callback's ZST fn-item/non-capturing-
    // closure TYPE (Zig's comptime-known `cb` value ⇒ a Rust zero-sized type, NOT
    // a runtime fn pointer). Each call-site emits exactly one untyped loop
    // trampoline `trampoline::<UD, CB>` with zero allocation and one indirection —
    // matching Zig's comptime `struct{fn callback}` (no `dyn`, no boxing). `self`
    // is by value (Copy); `loop`/`c` stay RAW pointers (`rust-mut-args-strongly-
    // protected`: the loop re-enters `c` across ticks, a `&mut` re-read would be UB).
    ///
    /// # Safety
    /// `c` must be a valid, stable `Completion` pointer that outlives the wait,
    /// and `loop_` a valid `Loop` pointer; both are held raw by the loop.
    pub unsafe fn wait<UD, CB>(
        self,
        loop_: *mut Loop,
        c: *mut Completion,
        userdata: *mut UD,
        cb: CB,
    ) where
        UD: 'static,
        CB: Fn(
                *mut UD,
                *mut Loop,
                *mut Completion,
                core::result::Result<(), WaitError>,
            ) -> CallbackAction
            + 'static,
    {
        // The typed `cb` must be a zero-sized comptime-known value (a fn-item or
        // non-capturing closure) so the trampoline can reconstruct it with no
        // state — exactly Zig's comptime capture. `cb` itself is dropped here.
        const {
            assert!(
                core::mem::size_of::<CB>() == 0,
                "Async::wait cb must be zero-sized (a fn item / non-capturing closure), matching Zig comptime cb"
            )
        };
        let _ = cb;

        // SAFETY: `c` is a caller-owned, stable Completion; we configure it as a
        // `.machport` op (a full reset, per `Completion::new_op`) and hand it to
        // the loop by raw pointer (never a borrow).
        unsafe {
            *c = Completion::new_op(
                Operation::Machport {
                    port: self.port,
                    buffer: ReadBuffer::Array(core::mem::zeroed()),
                },
                userdata.cast::<c_void>(),
                Self::trampoline::<UD, CB>,
            );

            (*loop_).add(c);
        }
    }

    /// The monomorphized untyped loop trampoline for a given (`UD`, `CB`). On
    /// fire it `drain()`s the mach port (coalescing) then calls the typed `cb`.
    fn trampoline<UD, CB>(
        ud: *mut c_void,
        l_inner: *mut Loop,
        c_inner: *mut Completion,
        r: Result,
    ) -> CallbackAction
    where
        UD: 'static,
        CB: Fn(
                *mut UD,
                *mut Loop,
                *mut Completion,
                core::result::Result<(), WaitError>,
            ) -> CallbackAction
            + 'static,
    {
        // Drain the mach port so that we only fire one
        // notification even if many are queued.
        //
        // SAFETY: this trampoline is only ever installed by `wait` on a `.machport`
        // op, so `c_inner`'s op carries the port to drain.
        let port = match unsafe { &(*c_inner).op } {
            Operation::Machport { port, .. } => *port,
            _ => unreachable!(),
        };
        Self::drain(port);

        // Reconstruct the zero-sized `CB` value — the Rust equivalent of Zig
        // baking `comptime cb` into the monomorphized trampoline.
        //
        // SAFETY: `CB` is a ZST (asserted in `wait`), so reading it through an
        // aligned dangling pointer accesses zero bytes and is sound (ZST reads
        // never dereference).
        let cb: CB = unsafe { NonNull::<CB>::dangling().as_ptr().read() };

        let ud_typed = crate::common::userdata_value::<UD>(ud);
        // if (r.machport) |_| {} else |err| err
        let mapped: core::result::Result<(), WaitError> = match r {
            Result::Machport(Ok(())) => Ok(()),
            Result::Machport(Err(err)) => Err(err),
            _ => unreachable!(),
        };
        cb(ud_typed, l_inner, c_inner, mapped)
    }

    /// Drain the given mach port. All message bodies are discarded.
    fn drain(port: darwin::mach_port_name_t) {
        // PORT NOTE: the reference wraps the header in a one-field struct so
        // `@sizeOf(@TypeOf(message))` is the receive-buffer size handed to
        // `mach_msg`. Reproduced faithfully (`size_of::<Message>()`).
        #[repr(C)]
        struct Message {
            header: darwin::mach_msg_header_t,
        }
        // `= undefined` in the reference; a mach header must be fully initialized
        // in Rust, so it is zeroed (the kernel overwrites it on receive).
        let mut message = Message {
            header: unsafe { core::mem::zeroed() },
        };

        loop {
            // SAFETY: `mach_msg` receives into `message.header` (a valid stack
            // buffer sized `size_of::<Message>()`) from our own `port`; POD FFI.
            let rc = unsafe {
                darwin::mach_msg(
                    &mut message.header,
                    darwin::MACH_RCV_MSG | darwin::MACH_RCV_TIMEOUT,
                    0,
                    core::mem::size_of::<Message>() as darwin::mach_msg_size_t,
                    port,
                    darwin::MACH_MSG_TIMEOUT_NONE,
                    darwin::MACH_PORT_NULL,
                )
            };
            match darwin::getMachMsgError(rc) {
                // This means a read would've blocked, so we drained.
                darwin::MachMsgE::RCV_TIMED_OUT => return,

                // We dequeued, so we want to loop again.
                darwin::MachMsgE::SUCCESS => {}

                // We dequeued but the message had a body. We ignore
                // message bodies for async so we are happy to discard
                // it and continue.
                darwin::MachMsgE::RCV_TOO_LARGE => {}

                _err => {
                    // std.log.warn("mach msg drain err, may duplicate async wakeups err={}", .{err});
                    return;
                }
            }
        }
    }

    /// Notify a loop to wake up synchronously. This should never block forever
    /// (it will always EVENTUALLY succeed regardless of if the loop is currently
    /// ticking or not).
    //
    // PORT NOTE: the reference receiver is `self: Self` (by value, Copy) and the
    // error is `error{MachMsgFailed}`; ported as `&self` (non-mutating; permissive
    // for every termio/core call site) returning the unified `crate::NotifyError`.
    pub fn notify(&self) -> core::result::Result<(), NotifyError> {
        // This constructs an empty mach message. It has no data.
        let mut msg = darwin::mach_msg_header_t {
            // We use COPY_SEND which will not increment any send ref
            // counts because it'll reuse the existing send right.
            msgh_bits: darwin::MACH_MSG_TYPE::COPY_SEND.0,
            msgh_size: core::mem::size_of::<darwin::mach_msg_header_t>() as darwin::mach_msg_size_t,
            msgh_remote_port: self.port,
            msgh_local_port: darwin::MACH_PORT_NULL,
            // msgh_voucher_port / msgh_id are `undefined` in the reference; a mach
            // header must still be fully initialized in Rust, so they are zeroed
            // (the kernel ignores them for this send).
            msgh_voucher_port: 0,
            msgh_id: 0,
        };

        // SAFETY: `mach_msg` reads/writes only `msg` (a valid stack header) and
        // sends to our own port; POD FFI, no aliasing.
        let rc = unsafe {
            darwin::mach_msg(
                &mut msg,
                darwin::MACH_SEND_MSG | darwin::MACH_SEND_TIMEOUT,
                msg.msgh_size,
                0,
                darwin::MACH_PORT_NULL,
                0, // Fail instantly if the port is full
                darwin::MACH_PORT_NULL,
            )
        };
        match darwin::getMachMsgError(rc) {
            darwin::MachMsgE::SUCCESS => Ok(()),
            // This is okay because it means that there was no more buffer
            // space meaning that the port will wake up.
            darwin::MachMsgE::SEND_NO_BUFFER => Ok(()),
            // This means that the send would've blocked because the
            // queue is full. We assume success because the port is full.
            darwin::MachMsgE::SEND_TIMED_OUT => Ok(()),
            _e => {
                // std.log.warn("mach msg err={}", .{e});
                Err(NotifyError) // error.MachMsgFailed
            }
        }
    }
}

// ---------------------------------------------------------------------------
// PORT: `AsyncTests(xev, AsyncMachPort)` (async.zig:714). On macOS the single
// `Async` instantiation is `AsyncMachPort` (§0/§7), so `Impl == Async`. The three
// reference `test{}` land as live Rust tests, plus rustty KAT 1 (§7) which proves
// the cross-thread wakeup on a real kqueue (Miri can't — real mach syscalls).
// `#[cfg(target_os = "macos")]` via the module gate.
// ---------------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;
    use crate::kqueue::{Completion, Loop};
    use crate::looppkg::RunMode;

    // test "async"
    #[test]
    fn async_() {
        let mut loop_ = Loop::init().unwrap();

        let mut notifier = Async::init().unwrap();

        // Wait
        let mut wake = false;
        let mut c_wait = Completion::default();
        let cb = |ud: *mut bool,
                  _l: *mut Loop,
                  _c: *mut Completion,
                  r: core::result::Result<(), WaitError>| {
            r.unwrap(); // _ = r catch unreachable
            unsafe { *ud = true };
            CallbackAction::Rearm
        };
        unsafe {
            notifier.wait(
                &mut loop_ as *mut Loop,
                core::ptr::addr_of_mut!(c_wait),
                core::ptr::addr_of_mut!(wake),
                cb,
            );
        }

        // Send a notification
        notifier.notify().unwrap();

        // Wait for wake
        loop_.run(RunMode::Once).unwrap();
        assert!(wake);

        // Make sure it only triggers once
        wake = false;
        loop_.run(RunMode::NoWait).unwrap();
        assert!(!wake);

        notifier.deinit();
        loop_.deinit();
    }

    // test "async: notify first"
    #[test]
    fn async_notify_first() {
        let mut loop_ = Loop::init().unwrap();

        let mut notifier = Async::init().unwrap();

        // Send a notification
        notifier.notify().unwrap();

        // Wait
        let mut wake = false;
        let mut c_wait = Completion::default();
        let cb = |ud: *mut bool,
                  _l: *mut Loop,
                  _c: *mut Completion,
                  r: core::result::Result<(), WaitError>| {
            r.unwrap(); // _ = r catch unreachable
            unsafe { *ud = true };
            CallbackAction::Disarm
        };
        unsafe {
            notifier.wait(
                &mut loop_ as *mut Loop,
                core::ptr::addr_of_mut!(c_wait),
                core::ptr::addr_of_mut!(wake),
                cb,
            );
        }

        // Wait for wake
        loop_.run(RunMode::UntilDone).unwrap();
        assert!(wake);

        notifier.deinit();
        loop_.deinit();
    }

    // test "async batches multiple notifications"
    #[test]
    fn async_batches_multiple_notifications() {
        let mut loop_ = Loop::init().unwrap();

        let mut notifier = Async::init().unwrap();

        // Send a notification many times
        notifier.notify().unwrap();
        notifier.notify().unwrap();
        notifier.notify().unwrap();
        notifier.notify().unwrap();
        notifier.notify().unwrap();

        // Wait
        let mut count: u32 = 0;
        let mut c_wait = Completion::default();
        let cb = |ud: *mut u32,
                  _l: *mut Loop,
                  _c: *mut Completion,
                  r: core::result::Result<(), WaitError>| {
            r.unwrap(); // _ = r catch unreachable
            unsafe { *ud += 1 };
            CallbackAction::Rearm
        };
        unsafe {
            notifier.wait(
                &mut loop_ as *mut Loop,
                core::ptr::addr_of_mut!(c_wait),
                core::ptr::addr_of_mut!(count),
                cb,
            );
        }

        // Send a notification
        notifier.notify().unwrap();

        // Wait for wake
        loop_.run(RunMode::Once).unwrap();
        for _ in 0..10 {
            loop_.run(RunMode::NoWait).unwrap();
        }
        assert_eq!(count, 1);

        notifier.deinit();
        loop_.deinit();
    }

    // KAT 1 (XEV_BLUEPRINT §7) — Async wakeup round-trip: `init`; `wait` (rearm);
    // `notify` from ANOTHER thread; `run(.once)`; assert woke EXACTLY once. Proves
    // the [ADV-FIX F2] Send discipline (the `Async` port:u32 handle crosses the
    // thread boundary) end-to-end on a live kqueue + the drain coalescing contract.
    #[test]
    fn kat1_async_wakeup_roundtrip() {
        let mut loop_ = Loop::init().unwrap();

        let mut notifier = Async::init().unwrap();

        // Register the waiter (rearm so we can re-run and prove "only once").
        let mut count: u32 = 0;
        let mut c_wait = Completion::default();
        let cb = |ud: *mut u32,
                  _l: *mut Loop,
                  _c: *mut Completion,
                  r: core::result::Result<(), WaitError>| {
            r.unwrap();
            unsafe { *ud += 1 };
            CallbackAction::Rearm
        };
        unsafe {
            notifier.wait(
                &mut loop_ as *mut Loop,
                core::ptr::addr_of_mut!(c_wait),
                core::ptr::addr_of_mut!(count),
                cb,
            );
        }

        // Notify from ANOTHER thread. `Async` is `Copy` + auto-`Send` (its handle
        // is a bare `port: u32`), so it moves across the boundary; the mach send
        // queues the wakeup on the port regardless of whether the loop is ticking.
        let n = notifier;
        std::thread::spawn(move || {
            n.notify().unwrap();
        })
        .join()
        .unwrap();

        // Run once — the port has a message, so we wake EXACTLY once.
        loop_.run(RunMode::Once).unwrap();
        assert_eq!(count, 1);

        // No further wake without another notify.
        loop_.run(RunMode::NoWait).unwrap();
        assert_eq!(count, 1);

        notifier.deinit();
        loop_.deinit();
    }
}
