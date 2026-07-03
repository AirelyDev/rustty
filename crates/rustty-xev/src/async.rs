//! async.rs — the C3 sequencing subset of libxev `watcher/async.zig` (D6,
//! XEV_BLUEPRINT §4/§8).
//!
//! `AsyncMachPort` is the Darwin cross-thread wakeup handle. The loop `Wakeup`
//! (kqueue.rs, C3) EMBEDS an `Async` and drives it at `Loop.init`/`tick`/`wakeup`,
//! and termio/core name `xev::Async` (`renderer_wakeup`, mailbox wakeups) and
//! default/Copy/`notify()` it — so `init`/`deinit`/`notify` MUST be real in C3
//! (a live loop cannot allocate its mach-port wakeup otherwise). The remaining
//! async.zig surface — `wait`/`drain` (the `machport`-op trampoline), the
//! non-Darwin `AsyncEventFd`/`AsyncLoopState`/`AsyncIOCP` variants, and the
//! `AsyncTests` (async×3 + KAT 1) — is DEFERRED to chunk C4 (§8); this file
//! carries ONLY the AsyncMachPort lifecycle C3 depends on.
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
use crate::{Error, NotifyError};

/// PORT: `AsyncMachPort` — "Async implementation that is deferred to the Darwin
/// mach port." `#[derive(Clone, Copy)]`: the reference `wait`/`notify` take
/// `self: Self` by value (a trivially-copyable `port` handle) and ghostty copies
/// the handle by value (`renderer_wakeup = render_thread.wakeup`); the `port:
/// u32` field makes it auto-`Send` (the cross-thread `notify()` handle, §2 F2).
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

    // PORT (DEFERRED to C4, §8): `wait` (the `machport`-op trampoline that
    // `drain`s the port then calls the typed user callback) + `drain` (coalesces
    // N notifies → 1 wakeup). The loop `machport` op arm is ported in kqueue.rs
    // (C3) so the wiring is present; the watcher `wait`/`drain` + the `AsyncTests`
    // (async×3 + KAT 1) land in C4.

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
