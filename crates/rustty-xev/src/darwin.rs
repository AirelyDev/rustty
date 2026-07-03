//! darwin.rs — faithful port of libxev `darwin.zig` (D6, XEV_BLUEPRINT §3 chunk C2).
//!
//! The darwin/syscall layer: the mach-port / `mach_msg` bindings + `KernE` /
//! `MachMsgE` that `Async`(mach) and the loop `Wakeup` ride on macOS/kqueue.
//! `#[cfg(target_os = "macos")]`-gated (the whole xev port is kqueue-only for now).
//!
//! PORT SCOPE (blueprint §3, [ADV-FIX F4/F5]):
//! - The kqueue syscall **consts** (`EVFILT_*`, `EV_*`, `NOTE_*`) and the
//!   `kevent64` / `kevent64_s` syscall come from the **`libc` crate directly**
//!   (libc-0.2.186 apple/mod.rs) — they are NOT re-declared here; the kqueue
//!   backend (C3) names `libc::{EVFILT_READ, EV_ADD, NOTE_EXIT, kevent64,
//!   kevent64_s, …}` exactly as libxev names `std.c.EVFILT.READ` etc. This
//!   module binds only the mach APIs libc lacks.
//! - `mach_task_self` **is** in libc (`libc::mach_task_self()`), so it is used
//!   from libc, not re-declared (F5: rustty-os `mach.rs` is the VM-tag
//!   allocator, NOT `mach_msg`, so these mach-msg/mach-port bindings are
//!   non-duplicative).
//! - The mach APIs genuinely absent from libc (`mach_msg`, `mach_msg_header_t`,
//!   `mach_port_allocate`/`_insert_right`/`_deallocate`/`_set_attributes`/
//!   `_destroy`) are declared here as `extern "C"` — this is the crate's single
//!   mach syscall home (C4 `async.rs` + the kqueue "mach port" test call into it
//!   rather than re-declaring). Provenance is noted at each: darwin.zig, the Zig
//!   `posix.system.*` (std.c) surface, or the async.zig-local "Missing Mach APIs
//!   from Zig stdlib" block.

// PORT: the reference file header comment.
//   //! These are copied from Zig's stdlib as of Zig 0.14 because
//   //! they are no longer exported. This is probably going to be
//   //! fixed in the future: https://github.com/ziglang/zig/pull/21218

// Faithful Zig identifiers: `getKernError`/`getMachMsgError` (camelCase fns) and
// the lowercase C type aliases (`natural_t`, `mach_msg_header_t`, …).
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

// PORT: `const std = @import("std");` — Rust binds the `std.c` equivalents
// (`kern_return_t`, the mach primitives) through the platform `libc` crate.

pub const MACH_SEND_MSG: mach_msg_option_t = 0x00000001;
pub const MACH_RCV_MSG: mach_msg_option_t = 0x00000002;

pub const MACH_SEND_TIMEOUT: mach_msg_option_t = 0x00000010;
pub const MACH_SEND_OVERRIDE: mach_msg_option_t = 0x00000020;
pub const MACH_SEND_INTERRUPT: mach_msg_option_t = 0x00000040;
pub const MACH_SEND_NOTIFY: mach_msg_option_t = 0x00000080;
pub const MACH_SEND_ALWAYS: mach_msg_option_t = 0x00010000;
pub const MACH_SEND_FILTER_NONFATAL: mach_msg_option_t = 0x00010000;
pub const MACH_SEND_TRAILER: mach_msg_option_t = 0x00020000;
pub const MACH_SEND_NOIMPORTANCE: mach_msg_option_t = 0x00040000;
pub const MACH_SEND_NODENAP: mach_msg_option_t = MACH_SEND_NOIMPORTANCE;
pub const MACH_SEND_IMPORTANCE: mach_msg_option_t = 0x00080000;
pub const MACH_SEND_SYNC_OVERRIDE: mach_msg_option_t = 0x00100000;
pub const MACH_SEND_PROPAGATE_QOS: mach_msg_option_t = 0x00200000;
pub const MACH_SEND_SYNC_USE_THRPRI: mach_msg_option_t = MACH_SEND_PROPAGATE_QOS;
pub const MACH_SEND_KERNEL: mach_msg_option_t = 0x00400000;
pub const MACH_SEND_SYNC_BOOTSTRAP_CHECKIN: mach_msg_option_t = 0x00800000;

pub const MACH_RCV_TIMEOUT: mach_msg_option_t = 0x00000100;
pub const MACH_RCV_NOTIFY: mach_msg_option_t = 0x00000000;
pub const MACH_RCV_INTERRUPT: mach_msg_option_t = 0x00000400;
pub const MACH_RCV_VOUCHER: mach_msg_option_t = 0x00000800;
pub const MACH_RCV_OVERWRITE: mach_msg_option_t = 0x00000000;
pub const MACH_RCV_GUARDED_DESC: mach_msg_option_t = 0x00001000;
pub const MACH_RCV_SYNC_WAIT: mach_msg_option_t = 0x00004000;
pub const MACH_RCV_SYNC_PEEK: mach_msg_option_t = 0x00008000;

// PORT NOTE: the reference leaves the `MACH_SEND_*` / `MACH_RCV_*` flags as
// untyped comptime ints (they coerce at each use). Rust `const` needs a concrete
// type; they are typed `mach_msg_option_t` (= `integer_t` = `c_int`) — the type
// of the `mach_msg` `option` parameter every flag is OR'd into (async.zig
// `drain`/`notify`). The one non-option use — `MACH_RCV_MSG` as a `kevent64_s`
// `fflags` (u32) in the kqueue `Wakeup` (C3) — casts `as u32` at that site.

pub const MACH_PORT_NULL: mach_port_t = 0;
pub const MACH_MSG_TIMEOUT_NONE: mach_msg_timeout_t = 0;

pub type natural_t = libc::c_uint;
pub type integer_t = libc::c_int;
pub type mach_port_t = libc::c_uint;
pub type mach_port_name_t = natural_t;
pub type mach_msg_bits_t = libc::c_uint;
pub type mach_msg_id_t = integer_t;
pub type mach_msg_type_number_t = natural_t;
pub type mach_msg_type_name_t = libc::c_uint;
pub type mach_msg_option_t = integer_t;
pub type mach_msg_size_t = natural_t;
pub type mach_msg_timeout_t = natural_t;
// PORT: `pub const mach_msg_return_t = std.c.kern_return_t;`
pub type mach_msg_return_t = libc::kern_return_t;

// PORT NOTE: `#[derive(Clone, Copy)]` — the reference is a plain `extern struct`
// (POD); Zig copies it by value (`var msg: mach_msg_header_t = .{...}` in
// async.zig `notify`), so `Copy` is faithful. `#[repr(C)]` matches `extern
// struct` layout for the `mach_msg` FFI.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct mach_msg_header_t {
    pub msgh_bits: mach_msg_bits_t,
    pub msgh_size: mach_msg_size_t,
    pub msgh_remote_port: mach_port_t,
    pub msgh_local_port: mach_port_t,
    pub msgh_voucher_port: mach_port_name_t,
    pub msgh_id: mach_msg_id_t,
}

extern "C" {
    // PORT: `pub extern "c" fn mach_msg(...) std.c.kern_return_t`. The reference
    // `?*mach_msg_header_t` (optional pointer) is a nullable raw pointer in Rust.
    pub fn mach_msg(
        msg: *mut mach_msg_header_t,
        option: mach_msg_option_t,
        send_size: mach_msg_size_t,
        rcv_size: mach_msg_size_t,
        rcv_name: mach_port_name_t,
        timeout: mach_msg_timeout_t,
        notify: mach_port_name_t,
    ) -> libc::kern_return_t;
}

// PORT: `pub fn getKernError(err: std.c.kern_return_t) KernE`.
pub fn getKernError(err: libc::kern_return_t) -> KernE {
    // return @as(KernE, @enumFromInt(@as(u32, @truncate(@as(usize, @intCast(err))))));
    //
    // PORT NOTE: the kern/mach return domain is always non-negative as `i32`
    // (KERN_* are 0..=56; mach-msg codes like 0x10000004 are positive `i32`), so
    // the `@intCast(usize, err)` (checked-non-negative) → `@truncate(u32, …)`
    // chain equals `err as usize as u32` faithfully.
    KernE(err as usize as u32)
}

/// Kernel return values
//
// PORT NOTE [precedent: rustty-os `mach::VMTag`, apprt-api `GotoTab`]: the
// reference `KernE` is a NON-EXHAUSTIVE `enum(u32)` (trailing `_`), open over any
// `u32` because `getKernError` builds it from a truncated syscall return. Rust
// enums cannot be open over arbitrary integers, so this is a
// `#[repr(transparent)]` newtype over `u32` with the named codes as associated
// consts; any other value is representable via `KernE(n)` (the `_` member) and
// the inner `u32` is `intFromEnum`.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KernE(pub u32);

impl KernE {
    pub const SUCCESS: KernE = KernE(0);

    /// Specified address is not currently valid
    pub const INVALID_ADDRESS: KernE = KernE(1);

    /// Specified memory is valid, but does not permit the
    /// required forms of access.
    pub const PROTECTION_FAILURE: KernE = KernE(2);

    /// The address range specified is already in use, or
    /// no address range of the size specified could be
    /// found.
    pub const NO_SPACE: KernE = KernE(3);

    /// The function requested was not applicable to this
    /// type of argument, or an argument is invalid
    pub const INVALID_ARGUMENT: KernE = KernE(4);

    /// The function could not be performed.  A catch-all.
    pub const FAILURE: KernE = KernE(5);

    /// A system resource could not be allocated to fulfill
    /// this request.  This failure may not be permanent.
    pub const RESOURCE_SHORTAGE: KernE = KernE(6);

    /// The task in question does not hold receive rights
    /// for the port argument.
    pub const NOT_RECEIVER: KernE = KernE(7);

    /// Bogus access restriction.
    pub const NO_ACCESS: KernE = KernE(8);

    /// During a page fault, the target address refers to a
    /// memory object that has been destroyed.  This
    /// failure is permanent.
    pub const MEMORY_FAILURE: KernE = KernE(9);

    /// During a page fault, the memory object indicated
    /// that the data could not be returned.  This failure
    /// may be temporary; future attempts to access this
    /// same data may succeed, as defined by the memory
    /// object.
    pub const MEMORY_ERROR: KernE = KernE(10);

    /// The receive right is already a member of the portset.
    pub const ALREADY_IN_SET: KernE = KernE(11);

    /// The receive right is not a member of a port set.
    pub const NOT_IN_SET: KernE = KernE(12);

    /// The name already denotes a right in the task.
    pub const NAME_EXISTS: KernE = KernE(13);

    /// The operation was aborted.  Ipc code will
    /// catch this and reflect it as a message error.
    pub const ABORTED: KernE = KernE(14);

    /// The name doesn't denote a right in the task.
    pub const INVALID_NAME: KernE = KernE(15);

    /// Target task isn't an active task.
    pub const INVALID_TASK: KernE = KernE(16);

    /// The name denotes a right, but not an appropriate right.
    pub const INVALID_RIGHT: KernE = KernE(17);

    /// A blatant range error.
    pub const INVALID_VALUE: KernE = KernE(18);

    /// Operation would overflow limit on user-references.
    pub const UREFS_OVERFLOW: KernE = KernE(19);

    /// The supplied (port) capability is improper.
    pub const INVALID_CAPABILITY: KernE = KernE(20);

    /// The task already has send or receive rights
    /// for the port under another name.
    pub const RIGHT_EXISTS: KernE = KernE(21);

    /// Target host isn't actually a host.
    pub const INVALID_HOST: KernE = KernE(22);

    /// An attempt was made to supply "precious" data
    /// for memory that is already present in a
    /// memory object.
    pub const MEMORY_PRESENT: KernE = KernE(23);

    /// A page was requested of a memory manager via
    /// memory_object_data_request for an object using
    /// a MEMORY_OBJECT_COPY_CALL strategy, with the
    /// VM_PROT_WANTS_COPY flag being used to specify
    /// that the page desired is for a copy of the
    /// object, and the memory manager has detected
    /// the page was pushed into a copy of the object
    /// while the kernel was walking the shadow chain
    /// from the copy to the object. This error code
    /// is delivered via memory_object_data_error
    /// and is handled by the kernel (it forces the
    /// kernel to restart the fault). It will not be
    /// seen by users.
    pub const MEMORY_DATA_MOVED: KernE = KernE(24);

    /// A strategic copy was attempted of an object
    /// upon which a quicker copy is now possible.
    /// The caller should retry the copy using
    /// vm_object_copy_quickly. This error code
    /// is seen only by the kernel.
    pub const MEMORY_RESTART_COPY: KernE = KernE(25);

    /// An argument applied to assert processor set privilege
    /// was not a processor set control port.
    pub const INVALID_PROCESSOR_SET: KernE = KernE(26);

    /// The specified scheduling attributes exceed the thread's
    /// limits.
    pub const POLICY_LIMIT: KernE = KernE(27);

    /// The specified scheduling policy is not currently
    /// enabled for the processor set.
    pub const INVALID_POLICY: KernE = KernE(28);

    /// The external memory manager failed to initialize the
    /// memory object.
    pub const INVALID_OBJECT: KernE = KernE(29);

    /// A thread is attempting to wait for an event for which
    /// there is already a waiting thread.
    pub const ALREADY_WAITING: KernE = KernE(30);

    /// An attempt was made to destroy the default processor
    /// set.
    pub const DEFAULT_SET: KernE = KernE(31);

    /// An attempt was made to fetch an exception port that is
    /// protected, or to abort a thread while processing a
    /// protected exception.
    pub const EXCEPTION_PROTECTED: KernE = KernE(32);

    /// A ledger was required but not supplied.
    pub const INVALID_LEDGER: KernE = KernE(33);

    /// The port was not a memory cache control port.
    pub const INVALID_MEMORY_CONTROL: KernE = KernE(34);

    /// An argument supplied to assert security privilege
    /// was not a host security port.
    pub const INVALID_SECURITY: KernE = KernE(35);

    /// thread_depress_abort was called on a thread which
    /// was not currently depressed.
    pub const NOT_DEPRESSED: KernE = KernE(36);

    /// Object has been terminated and is no longer available
    pub const TERMINATED: KernE = KernE(37);

    /// Lock set has been destroyed and is no longer available.
    pub const LOCK_SET_DESTROYED: KernE = KernE(38);

    /// The thread holding the lock terminated before releasing
    /// the lock
    pub const LOCK_UNSTABLE: KernE = KernE(39);

    /// The lock is already owned by another thread
    pub const LOCK_OWNED: KernE = KernE(40);

    /// The lock is already owned by the calling thread
    pub const LOCK_OWNED_SELF: KernE = KernE(41);

    /// Semaphore has been destroyed and is no longer available.
    pub const SEMAPHORE_DESTROYED: KernE = KernE(42);

    /// Return from RPC indicating the target server was
    /// terminated before it successfully replied
    pub const RPC_SERVER_TERMINATED: KernE = KernE(43);

    /// Terminate an orphaned activation.
    pub const RPC_TERMINATE_ORPHAN: KernE = KernE(44);

    /// Allow an orphaned activation to continue executing.
    pub const RPC_CONTINUE_ORPHAN: KernE = KernE(45);

    /// Empty thread activation (No thread linked to it)
    pub const NOT_SUPPORTED: KernE = KernE(46);

    /// Remote node down or inaccessible.
    pub const NODE_DOWN: KernE = KernE(47);

    /// A signalled thread was not actually waiting.
    pub const NOT_WAITING: KernE = KernE(48);

    /// Some thread-oriented operation (semaphore_wait) timed out
    pub const OPERATION_TIMED_OUT: KernE = KernE(49);

    /// During a page fault, indicates that the page was rejected
    /// as a result of a signature check.
    pub const CODESIGN_ERROR: KernE = KernE(50);

    /// The requested property cannot be changed at this time.
    pub const POLICY_STATIC: KernE = KernE(51);

    /// The provided buffer is of insufficient size for the requested data.
    pub const INSUFFICIENT_BUFFER_SIZE: KernE = KernE(52);

    /// Denied by security policy
    pub const DENIED: KernE = KernE(53);

    /// The KC on which the function is operating is missing
    pub const MISSING_KC: KernE = KernE(54);

    /// The KC on which the function is operating is invalid
    pub const INVALID_KC: KernE = KernE(55);

    /// A search or query operation did not return a result
    pub const NOT_FOUND: KernE = KernE(56);

    // `_,` — non-exhaustive: any other `u32` is representable via `KernE(n)`.
}

// PORT: `pub fn getMachMsgError(err: mach_msg_return_t) MachMsgE`.
pub fn getMachMsgError(err: mach_msg_return_t) -> MachMsgE {
    // return @as(MachMsgE, @enumFromInt(@as(u32, @truncate(@as(usize, @intCast(err))))));
    MachMsgE(err as usize as u32)
}

/// Mach msg return values
//
// PORT NOTE: unlike `KernE`, the reference `MachMsgE` is EXHAUSTIVE (no trailing
// `_`). It is nonetheless modeled as a `#[repr(transparent)]` newtype over `u32`
// (matching `KernE` / the `rustty-os::mach::VMTag` precedent) rather than a
// `#[repr(u32)] enum`, because `getMachMsgError` constructs it from an arbitrary
// truncated `mach_msg_return_t`: a real Rust enum would make an unexpected return
// code instant UB (invalid-discriminant transmute), strictly worse than Zig's
// checked `@enumFromInt`. Consumers only ever compare against the named codes
// (`== .SUCCESS`; `switch` over `.SEND_TIMED_OUT`/`.SEND_NO_BUFFER` in async
// `notify`), for which named-const equality is identical behavior.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MachMsgE(pub u32);

impl MachMsgE {
    pub const SUCCESS: MachMsgE = MachMsgE(0x00000000);

    /// Thread is waiting to send.  (Internal use only.)
    pub const SEND_IN_PROGRESS: MachMsgE = MachMsgE(0x10000001);
    /// Bogus in-line data.
    pub const SEND_INVALID_DATA: MachMsgE = MachMsgE(0x10000002);
    /// Bogus destination port.
    pub const SEND_INVALID_DEST: MachMsgE = MachMsgE(0x10000003);
    ///  Message not sent before timeout expired.
    pub const SEND_TIMED_OUT: MachMsgE = MachMsgE(0x10000004);
    ///  Bogus voucher port.
    pub const SEND_INVALID_VOUCHER: MachMsgE = MachMsgE(0x10000005);
    ///  Software interrupt.
    pub const SEND_INTERRUPTED: MachMsgE = MachMsgE(0x10000007);
    ///  Data doesn't contain a complete message.
    pub const SEND_MSG_TOO_SMALL: MachMsgE = MachMsgE(0x10000008);
    ///  Bogus reply port.
    pub const SEND_INVALID_REPLY: MachMsgE = MachMsgE(0x10000009);
    ///  Bogus port rights in the message body.
    pub const SEND_INVALID_RIGHT: MachMsgE = MachMsgE(0x1000000a);
    ///  Bogus notify port argument.
    pub const SEND_INVALID_NOTIFY: MachMsgE = MachMsgE(0x1000000b);
    ///  Invalid out-of-line memory pointer.
    pub const SEND_INVALID_MEMORY: MachMsgE = MachMsgE(0x1000000c);
    ///  No message buffer is available.
    pub const SEND_NO_BUFFER: MachMsgE = MachMsgE(0x1000000d);
    ///  Send is too large for port
    pub const SEND_TOO_LARGE: MachMsgE = MachMsgE(0x1000000e);
    ///  Invalid msg-type specification.
    pub const SEND_INVALID_TYPE: MachMsgE = MachMsgE(0x1000000f);
    ///  A field in the header had a bad value.
    pub const SEND_INVALID_HEADER: MachMsgE = MachMsgE(0x10000010);
    ///  The trailer to be sent does not match kernel format.
    pub const SEND_INVALID_TRAILER: MachMsgE = MachMsgE(0x10000011);
    ///  The sending thread context did not match the context on the dest port
    pub const SEND_INVALID_CONTEXT: MachMsgE = MachMsgE(0x10000012);
    ///  compatibility: no longer a returned error
    pub const SEND_INVALID_RT_OOL_SIZE: MachMsgE = MachMsgE(0x10000015);
    ///  The destination port doesn't accept ports in body
    pub const SEND_NO_GRANT_DEST: MachMsgE = MachMsgE(0x10000016);
    ///  Message send was rejected by message filter
    pub const SEND_MSG_FILTERED: MachMsgE = MachMsgE(0x10000017);

    ///  Thread is waiting for receive.  (Internal use only.)
    pub const RCV_IN_PROGRESS: MachMsgE = MachMsgE(0x10004001);
    ///  Bogus name for receive port/port-set.
    pub const RCV_INVALID_NAME: MachMsgE = MachMsgE(0x10004002);
    ///  Didn't get a message within the timeout value.
    pub const RCV_TIMED_OUT: MachMsgE = MachMsgE(0x10004003);
    ///  Message buffer is not large enough for inline data.
    pub const RCV_TOO_LARGE: MachMsgE = MachMsgE(0x10004004);
    ///  Software interrupt.
    pub const RCV_INTERRUPTED: MachMsgE = MachMsgE(0x10004005);
    ///  compatibility: no longer a returned error
    pub const RCV_PORT_CHANGED: MachMsgE = MachMsgE(0x10004006);
    ///  Bogus notify port argument.
    pub const RCV_INVALID_NOTIFY: MachMsgE = MachMsgE(0x10004007);
    ///  Bogus message buffer for inline data.
    pub const RCV_INVALID_DATA: MachMsgE = MachMsgE(0x10004008);
    ///  Port/set was sent away/died during receive.
    pub const RCV_PORT_DIED: MachMsgE = MachMsgE(0x10004009);
    ///  compatibility: no longer a returned error
    pub const RCV_IN_SET: MachMsgE = MachMsgE(0x1000400a);
    ///  Error receiving message header.  See special bits.
    pub const RCV_HEADER_ERROR: MachMsgE = MachMsgE(0x1000400b);
    ///  Error receiving message body.  See special bits.
    pub const RCV_BODY_ERROR: MachMsgE = MachMsgE(0x1000400c);
    ///  Invalid msg-type specification in scatter list.
    pub const RCV_INVALID_TYPE: MachMsgE = MachMsgE(0x1000400d);
    ///  Out-of-line overwrite region is not large enough
    pub const RCV_SCATTER_SMALL: MachMsgE = MachMsgE(0x1000400e);
    ///  trailer type or number of trailer elements not supported
    pub const RCV_INVALID_TRAILER: MachMsgE = MachMsgE(0x1000400f);
    ///  Waiting for receive with timeout. (Internal use only.)
    pub const RCV_IN_PROGRESS_TIMED: MachMsgE = MachMsgE(0x10004011);
    ///  invalid reply port used in a STRICT_REPLY message
    pub const RCV_INVALID_REPLY: MachMsgE = MachMsgE(0x10004012);
}

// ---------------------------------------------------------------------------
// Consolidated mach syscall bindings (blueprint §3 C2) — the mach-port APIs that
// are genuinely absent from the `libc` crate. libxev reaches these through Zig's
// `std.c` (`posix.system.*`) or declares them locally in `watcher/async.zig`;
// this crate binds `libc` directly (no ghostty os layer), so they are declared
// here as the ONE mach syscall home. C4 `async.rs` + the kqueue "mach port" test
// call into these instead of re-declaring; each carries its reference provenance.
// (`mach_task_self` is NOT here — it IS in libc: `libc::mach_task_self()`.)
// ---------------------------------------------------------------------------

/// PORT: `posix.system.ipc_space_t` (Zig std.c) — the mach task/ipc-space port.
pub type ipc_space_t = mach_port_t;
/// PORT: `posix.system.mach_port_right_t` (Zig std.c) — the `right` argument of
/// `mach_port_allocate` (`@intFromEnum(MACH_PORT_RIGHT.RECEIVE)`).
pub type mach_port_right_t = natural_t;
/// PORT: async.zig-local `const mach_port_flavor_t = c_int;` ("Missing Mach APIs
/// from Zig stdlib. Data from xnu: osfmk/mach/port.h").
pub type mach_port_flavor_t = libc::c_int;

extern "C" {
    /// PORT: `posix.system.mach_port_allocate` (Zig std.c) — used by async.zig
    /// `init` to allocate the RECEIVE port.
    pub fn mach_port_allocate(
        task: ipc_space_t,
        right: mach_port_right_t,
        name: *mut mach_port_name_t,
    ) -> libc::kern_return_t;

    /// PORT: `posix.system.mach_port_insert_right` (Zig std.c) — used by
    /// async.zig `init` to insert the MAKE_SEND right.
    pub fn mach_port_insert_right(
        task: ipc_space_t,
        name: mach_port_name_t,
        poly: mach_port_t,
        poly_poly: mach_msg_type_name_t,
    ) -> libc::kern_return_t;

    /// PORT: `posix.system.mach_port_deallocate` (Zig std.c) — used by the
    /// kqueue "mach port" test (kqueue.zig:2598) to release the send right.
    pub fn mach_port_deallocate(task: ipc_space_t, name: mach_port_name_t) -> libc::kern_return_t;

    /// PORT: async.zig-local `extern "c" fn mach_port_set_attributes(...)`
    /// ("Missing Mach APIs from Zig stdlib") — used by async.zig `init` to set
    /// the port queue limit (qlimit=1). The reference `info: *anyopaque` is a
    /// `*mut c_void` in Rust.
    pub fn mach_port_set_attributes(
        task: ipc_space_t,
        name: mach_port_name_t,
        flavor: mach_port_flavor_t,
        info: *mut libc::c_void,
        count: mach_msg_type_number_t,
    ) -> libc::kern_return_t;

    /// PORT: async.zig-local `extern "c" fn mach_port_destroy(...)` ("Missing
    /// Mach APIs from Zig stdlib") — used by async.zig `init` errdefer / `deinit`.
    pub fn mach_port_destroy(task: ipc_space_t, name: mach_port_name_t) -> libc::kern_return_t;
}
