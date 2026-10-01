//! The epoll backend's `Completion` and the per-operation surface.
//!
//! PORTS: libxev src/backend/epoll.zig — `Completion` (epoll.zig:872), the
//! `Operation`/`Result` unions, the error sets, `ReadBuffer`/`WriteBuffer`, and
//! the `posix.*` syscall wrappers `perform` calls.

use super::*;

// ---------------------------------------------------------------------------
// Completion (epoll.zig:872).
// ---------------------------------------------------------------------------

/// A completion is a request to perform some work with the loop.
#[repr(C)]
pub struct Completion {
    /// Operation to execute. This is only safe to read BEFORE the completion
    /// is queued. After being queued (with "add"), the operation may change.
    pub op: Operation,

    /// Userdata and callback for when the completion is finished.
    pub userdata: *mut core::ffi::c_void,
    pub callback: Callback,

    //---------------------------------------------------------------
    // Internal fields
    /// Intrusive queue field
    pub(super) next: *mut Completion,

    pub(super) flags: CompletionFlags,

    /// If scheduled on a thread pool, these will be set. This is NOT a
    /// reliable way to get access to the loop and shouldn't be used
    /// except internally.
    //
    // PORT NOTE (Task 2, REVISED WS19 T10): the oracle's `task_completions:
    // *TaskCompletionQueue` per-completion field had been elided — `thread_perform`
    // reached the loop's cached `thread_pool_completions_ptr` via `task_loop`
    // instead. WS19 T10 RESTORED it (and added `task_wakeup`): the mirihar
    // `wakeup_copied` candidate proves that ANY worker access to the `Loop`
    // allocation — including a by-value read of a `Copy` handle — is a foreign
    // access against the protected `&mut Loop` the loop thread holds for the whole
    // tick, hence SB UB. `task_result` holds the result computed on the pool thread
    // until the loop drains it in `tick`. See kqueue.rs's field docs (the two
    // backends are symmetric) and `.port/phase-2/DEFERRALS.md`.
    pub(super) task: thread_pool::Task,
    pub(super) task_loop: *mut Loop,
    pub(super) task_result: Result,
    /// The reference's own field, restored (fidelity gain).
    pub(super) task_completions: *mut TaskCompletionQueue,
    /// PORT-ADDED [R1-DIVERGENCE, WS19 T10] — the reference wakes the loop with
    /// `c.task_loop.wakeup()`; `Eventfd` is a `Copy` fd wrapper, copied in here at
    /// schedule time so the worker touches only this caller-owned allocation.
    pub(super) task_wakeup: Eventfd,
}

/// PORT NOTE: the `flags: packed struct { state, threadpool, dup, dup_fd }`
/// sub-struct. The `threadpool` bit routes the op through the thread pool (Task 2);
/// `dup`/`dup_fd` back the epoll single-op-per-fd dup hack.
#[derive(Clone, Copy)]
pub(super) struct CompletionFlags {
    /// Watch state of this completion. We use this to determine whether
    /// we're active, adding, deleting, etc. This lets us add and delete
    /// multiple times before a loop tick and handle the state properly.
    pub(super) state: State,

    /// Schedule this onto the threadpool rather than epoll. Not all
    /// operations support this.
    //
    // PORT NOTE (Task 2): honored by the `.close` start arm (Stream.close sets it,
    // epoll arm). With a pool attached, `thread_schedule` enqueues the blocking op
    // onto it; with no pool it yields `ThreadPoolRequired`, exactly as the
    // reference's `thread_schedule` does when `Loop.thread_pool == null`.
    pub(super) threadpool: bool,

    /// Set to true to dup the file descriptor for the operation prior
    /// to setting it up with epoll. This is a hack to make it so that
    /// a completion can represent a single op per fd, since epoll requires
    /// a single fd for multiple ops.
    pub(super) dup: bool,
    pub(super) dup_fd: fd_t,
}

/// PORT NOTE: `const State = enum(u3) { dead, adding, deleting, active }`.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum State {
    /// completion is not part of any loop
    Dead = 0,
    /// completion is in the submission queue
    Adding = 1,
    /// completion is in the deletion queue
    Deleting = 2,
    /// completion is registered with epoll
    Active = 3,
}

// PORT NOTE: hand-written `Default` (NOT `#[derive]`) — `Completion` has an
// fn-ptr `callback` + raw-ptr fields with no `Default`. Mirrors the Zig field
// defaults `op = .noop`, `userdata = null`, `callback = noopCallback`,
// `next = null`, `flags = { state: dead, dup: false, dup_fd: 0 }`.
impl Default for Completion {
    fn default() -> Self {
        Completion {
            op: Operation::default(),
            userdata: ptr::null_mut(),
            callback: noop_callback,
            next: ptr::null_mut(),
            flags: CompletionFlags {
                state: State::Dead,
                threadpool: false,
                dup: false,
                dup_fd: 0,
            },
            // `task`/`task_loop`/`task_result`/`task_completions`/`task_wakeup` are
            // Zig `undefined` (only set when scheduled on the pool); zeroed/
            // placeholder here (Rust has no undefined).
            task: thread_pool::Task::default(),
            task_loop: ptr::null_mut(),
            task_result: Result::Noop,
            task_completions: ptr::null_mut(),
            task_wakeup: Eventfd { fd: -1 },
        }
    }
}

// PORT NOTE: `Completion` holds a raw-ptr `next` link + an fn-ptr → auto-`!Send`;
// it rides the `Loop` moved onto the termio thread. SAFETY / single-thread
// affinity: after the move a completion is touched by exactly the owning
// event-loop thread, so transferring ownership is sound.
unsafe impl Send for Completion {}

// PORT NOTE: intrusive `next` link seam for `queue::Intrusive<Completion>`
// (submissions/deletions).
impl queue::Node for Completion {
    unsafe fn next_field(this: *mut Self) -> *mut *mut Self {
        unsafe { ptr::addr_of_mut!((*this).next) }
    }
}

// PORT NOTE: intrusive `next` link seam for `queue_mpsc::Intrusive<Completion>`
// (thread_pool_completions).
impl queue_mpsc::Node for Completion {
    unsafe fn next_field(this: *mut Self) -> *mut *mut Self {
        unsafe { ptr::addr_of_mut!((*this).next) }
    }
}

impl Completion {
    /// Returns the state of this completion.
    //
    // PORT NOTE: reference `state(self: Completion)` (by value); ported as an
    // associated fn taking `*mut Completion` (raw self) to stay on the raw-ptr
    // discipline. Test call sites (`c1.state()`) route through `state_of`.
    pub(super) fn state(c: *mut Completion) -> CompletionState {
        match unsafe { (*c).flags.state } {
            State::Dead => CompletionState::Dead,
            State::Adding | State::Deleting | State::Active => CompletionState::Active,
        }
    }

    /// Convenience wrapper for `state` callable on a `&Completion` (tests do
    /// `c1.state_of()`).
    pub fn state_of(&self) -> CompletionState {
        Completion::state(self as *const Completion as *mut Completion)
    }

    /// PORT NOTE: the `c.* = .{ .op, .userdata, .callback }` initialization the
    /// watcher `wait`/`read`/`write`/`close`/`poll` methods perform — a FULL reset
    /// with the internal fields (`next`/`flags`) taking their `Default` values.
    //
    // PORT NOTE (`pub(crate)`): `watcher/timer.zig`'s `TimerLoop.cancel`
    // (timer.zig:137-210) builds its `c_cancel` completion as `.{ .op = .{ .cancel
    // = .{ .c = c_timer } }, ... }` from OUTSIDE this backend module, so the
    // watcher needs this full-reset seam — exactly the visibility `kqueue.rs`'s
    // twin (kqueue.rs:1589) already carries for the macOS `timer.rs` watcher.
    pub(crate) fn new_op(op: Operation, userdata: *mut c_void, callback: Callback) -> Completion {
        Completion {
            op,
            userdata,
            callback,
            ..Completion::default()
        }
    }

    /// PORT NOTE: the `c.flags.dup = true` assignment the epoll stream `Readable`/
    /// `Writeable` arms perform (`stream.zig:598/993`).
    ///
    /// # Safety
    /// `c` must be a valid, stable `Completion` pointer.
    pub(super) unsafe fn set_dup(c: *mut Completion, v: bool) {
        unsafe { (*c).flags.dup = v };
    }

    /// PORT NOTE: the `c.flags.threadpool = true` assignment the epoll stream
    /// `Closeable` arm performs (`stream.zig:430`).
    ///
    /// # Safety
    /// `c` must be a valid, stable `Completion` pointer.
    pub(super) unsafe fn set_threadpool(c: *mut Completion, v: bool) {
        unsafe { (*c).flags.threadpool = v };
    }

    /// Perform the operation associated with this completion. This performs
    /// the full blocking operation for the completion.
    ///
    /// # Safety
    /// `self_` valid+stable; `ev_` (if Some) a valid epoll_event for this op.
    pub(super) unsafe fn perform(
        self_: *mut Completion,
        _ev: Option<*const libc::epoll_event>,
    ) -> Result {
        match unsafe { &mut (*self_).op } {
            // These never reach perform: they complete synchronously in `start`
            // or the timer heap.
            Operation::Cancel { .. }
            | Operation::Noop
            | Operation::Shutdown { .. }
            | Operation::Timer(_) => {
                unreachable!()
            }

            Operation::Accept {
                socket,
                addr,
                addr_size,
                flags,
            } => {
                // SAFETY: `accept4` fills `addr`/`addr_size` for the accepted conn.
                let r = unsafe {
                    libc::accept4(
                        *socket,
                        ptr::addr_of_mut!(*addr) as *mut libc::sockaddr,
                        addr_size,
                        *flags as libc::c_int,
                    )
                };
                if r < 0 {
                    Result::Accept(Err(AcceptError::Unknown))
                } else {
                    Result::Accept(Ok(r as socket_t))
                }
            }

            Operation::Connect { socket, .. } => {
                // if (posix.getsockoptError(op.socket)) {} else |err| err
                Result::Connect(unsafe { getsockopt_error(*socket) })
            }

            Operation::Poll { .. } => Result::Poll(Ok(())),

            Operation::Read { fd, buffer } => {
                let n_: core::result::Result<usize, ReadError> = match buffer {
                    ReadBuffer::Slice(v) => unsafe {
                        posix_read(*fd, (*v).cast::<u8>(), (*v).len())
                    },
                    ReadBuffer::Array(v) => unsafe { posix_read(*fd, v.as_mut_ptr(), v.len()) },
                };

                Result::Read(match n_ {
                    Ok(n) => {
                        if n == 0 {
                            Err(ReadError::EOF)
                        } else {
                            Ok(n)
                        }
                    }
                    Err(err) => Err(err),
                })
            }

            // PORT NOTE: `.pread => |*op| { const n_ = switch (op.buffer) { .slice
            // => posix.pread(op.fd, v, op.offset), .array => posix.pread(...) };
            // ... if (n == 0) error.EOF else n }` (epoll.zig:992) — the `.read`
            // body with `posix.pread(fd, buf, offset)` in place of `posix.read`.
            Operation::PRead { fd, buffer, offset } => {
                let off = *offset;
                let n_: core::result::Result<usize, ReadError> = match buffer {
                    ReadBuffer::Slice(v) => unsafe {
                        posix_pread(*fd, (*v).cast::<u8>(), (*v).len(), off)
                    },
                    ReadBuffer::Array(v) => unsafe {
                        posix_pread(*fd, v.as_mut_ptr(), v.len(), off)
                    },
                };

                Result::PRead(match n_ {
                    Ok(n) => {
                        if n == 0 {
                            Err(ReadError::EOF)
                        } else {
                            Ok(n)
                        }
                    }
                    Err(err) => Err(err),
                })
            }

            Operation::Write { fd, buffer } => Result::Write(match buffer {
                WriteBuffer::Slice(v) => unsafe { posix_write(*fd, (*v).cast::<u8>(), (*v).len()) },
                WriteBuffer::Array { array, len } => unsafe {
                    posix_write(*fd, array.as_ptr(), *len)
                },
            }),

            // PORT NOTE: `.pwrite => |*op| .{ .pwrite = switch (op.buffer) { .slice
            // => posix.pwrite(op.fd, v, op.offset), .array => posix.pwrite(op.fd,
            // v.array[0..v.len], op.offset) } }` (epoll.zig:1013) — the `.write`
            // body with `posix.pwrite(fd, buf, offset)`.
            Operation::PWrite { fd, buffer, offset } => {
                let off = *offset;
                Result::PWrite(match buffer {
                    WriteBuffer::Slice(v) => unsafe {
                        posix_pwrite(*fd, (*v).cast::<u8>(), (*v).len(), off)
                    },
                    WriteBuffer::Array { array, len } => unsafe {
                        posix_pwrite(*fd, array.as_ptr(), *len, off)
                    },
                })
            }

            Operation::Send { fd, buffer } => Result::Send(match buffer {
                WriteBuffer::Slice(v) => unsafe { posix_send(*fd, (*v).cast::<u8>(), (*v).len()) },
                WriteBuffer::Array { array, len } => unsafe {
                    posix_send(*fd, array.as_ptr(), *len)
                },
            }),

            Operation::Recv { fd, buffer } => {
                let n_: core::result::Result<usize, ReadError> = match buffer {
                    ReadBuffer::Slice(v) => unsafe {
                        posix_recv(*fd, (*v).cast::<u8>(), (*v).len())
                    },
                    ReadBuffer::Array(v) => unsafe { posix_recv(*fd, v.as_mut_ptr(), v.len()) },
                };

                Result::Recv(match n_ {
                    Ok(n) => {
                        if n == 0 {
                            Err(ReadError::EOF)
                        } else {
                            Ok(n)
                        }
                    }
                    Err(err) => Err(err),
                })
            }

            // PORT NOTE: `.sendmsg => |*op| .{ .sendmsg = posix.sendmsg(op.fd,
            // op.msghdr, 0) }`. The `buffer` panic guard lives in `start`, not here.
            Operation::SendMsg { fd, msghdr, .. } => {
                Result::SendMsg(unsafe { posix_sendmsg(*fd, *msghdr) })
            }

            // PORT NOTE: `.recvmsg => |*op| { const res = linux.recvmsg(op.fd,
            // op.msghdr, 0); ... if (res == 0) error.EOF else if (res > 0) res ...
            // }`. A zero-byte datagram result is an unconditional `error.EOF`.
            Operation::RecvMsg { fd, msghdr } => {
                Result::RecvMsg(unsafe { posix_recvmsg(*fd, *msghdr) })
            }

            // PORT NOTE: an un-flagged `close` completes synchronously in `start`; a
            // threadpool-flagged `close` (Stream.close) reaches `perform` HERE, on a
            // pool worker thread via `thread_perform`, when a pool is attached (Task
            // 2). The `posix.close` body matches the reference's `perform` `.close`
            // arm.
            Operation::Close { fd } => {
                unsafe { libc::close(*fd) };
                Result::Close(Ok(()))
            }
        }
    }

    /// Return the fd for the completion. This will perform a dup(2) if
    /// requested.
    ///
    /// # Safety
    /// `self_` must be a valid, stable `Completion` pointer whose op has an fd.
    pub(super) unsafe fn fd_maybe_dup(self_: *mut Completion) -> core::result::Result<fd_t, ()> {
        let old_fd = unsafe { Completion::fd(self_) }.unwrap();
        if !unsafe { (*self_).flags.dup } {
            return Ok(old_fd);
        }
        if unsafe { (*self_).flags.dup_fd } > 0 {
            return Ok(unsafe { (*self_).flags.dup_fd });
        }

        let dup_fd = unsafe { libc::dup(old_fd) };
        if dup_fd < 0 {
            return Err(());
        }
        unsafe { (*self_).flags.dup_fd = dup_fd };
        Ok(dup_fd)
    }

    /// Returns the fd associated with the completion (if any).
    ///
    /// # Safety
    /// `self_` must be a valid, stable `Completion` pointer.
    pub(super) unsafe fn fd(self_: *mut Completion) -> Option<fd_t> {
        match unsafe { &(*self_).op } {
            Operation::Accept { socket, .. } => Some(*socket),
            Operation::Connect { socket, .. } => Some(*socket),
            Operation::Poll { fd, .. } => Some(*fd),
            Operation::Read { fd, .. } => Some(*fd),
            Operation::PRead { fd, .. } => Some(*fd),
            Operation::Recv { fd, .. } => Some(*fd),
            Operation::Write { fd, .. } => Some(*fd),
            Operation::PWrite { fd, .. } => Some(*fd),
            Operation::Send { fd, .. } => Some(*fd),
            Operation::SendMsg { fd, .. } => Some(*fd),
            Operation::RecvMsg { fd, .. } => Some(*fd),
            Operation::Close { fd, .. } => Some(*fd),
            Operation::Shutdown { socket, .. } => Some(*socket),

            Operation::Cancel { .. } | Operation::Timer(_) => None,

            Operation::Noop => unreachable!(),
        }
    }
}

// ---------------------------------------------------------------------------
// Operation / Result unions (epoll.zig:1105/1126/1149) + errors + buffers.
// ---------------------------------------------------------------------------

/// All the supported operations of this event loop.
//
// PORT NOTE: `pub const Operation = union(OperationType) { ... }`.
// `op: Operation = .{ .noop = {} }` — the `#[default]` `Noop` variant. Task 4 adds
// the socket/process/stream tags (accept/connect/poll/write/send/recv/close/
// shutdown); Task 5 adds `pread`/`pwrite` (the File watcher's offset-carrying
// read/write, which ride the thread pool on epoll).
#[derive(Default)]
pub enum Operation {
    #[default]
    Noop,

    Cancel {
        c: *mut Completion,
    },

    Accept {
        socket: socket_t,
        addr: net::sockaddr::Any,
        addr_size: net::socklen_t,
        flags: u32,
    },

    Connect {
        socket: socket_t,
        addr: net::Address,
    },

    /// Poll for events but do not perform any operations on them being
    /// ready. The "events" field are a OR-ed list of EPOLL events.
    Poll {
        fd: fd_t,
        events: u32,
    },

    Read {
        fd: fd_t,
        buffer: ReadBuffer,
    },

    /// PORT NOTE: `pread: struct { fd, buffer, offset: u64 }` — the offset-carrying
    /// read used by the File watcher.
    PRead {
        fd: fd_t,
        buffer: ReadBuffer,
        offset: u64,
    },

    Write {
        fd: fd_t,
        buffer: WriteBuffer,
    },

    /// PORT NOTE: `pwrite: struct { fd, buffer, offset: u64 }` — the offset-carrying
    /// write used by the File watcher.
    PWrite {
        fd: fd_t,
        buffer: WriteBuffer,
        offset: u64,
    },

    Send {
        fd: fd_t,
        buffer: WriteBuffer,
    },

    Recv {
        fd: fd_t,
        buffer: ReadBuffer,
    },

    /// Send a message on a socket using sendmsg (i.e. UDP).
    //
    // PORT NOTE: the reference field is `msghdr: *posix.msghdr_const`. `libc` has no
    // `msghdr_const` twin (Zig splits the type by constness; C/Rust do not), so the
    // constness rides the POINTER: `*const libc::msghdr`. `libc::msghdr` is the
    // glibc/kernel `struct msghdr` layout — exactly what `libc::sendmsg` reads.
    SendMsg {
        fd: fd_t,
        msghdr: *const libc::msghdr,

        /// Optionally, a write buffer can be specified and the given
        /// msghdr will be populated with information about this buffer.
        //
        // PORT NOTE (a REFERENCE QUIRK, preserved deliberately): this field is
        // DEAD in the reference — the `start` `.sendmsg` arm does
        // `if (v.buffer) |_| @panic("TODO: sendmsg with buffer");`, so the only
        // reachable value is `null`. The field (and `iov` below) are kept so the
        // op's shape matches the reference and so the panic arm is faithful (mirrors
        // io_uring.rs's `SendMsg`).
        buffer: Option<WriteBuffer>,

        /// Do not use this, it is only used internally.
        //
        // PORT NOTE: `iov: [1]posix.iovec_const = undefined` — never written nor
        // read by the reference (it is the scratch the unimplemented `buffer` path
        // would populate). Kept for shape fidelity; `libc::iovec` is the same layout
        // as Zig's `iovec_const` (constness rides the pointer in C/Rust).
        iov: [libc::iovec; 1],
    },

    /// Recieve a message on a socket using recvmsg (i.e. UDP).
    RecvMsg {
        fd: fd_t,
        msghdr: *mut libc::msghdr,
    },

    Close {
        fd: fd_t,
    },

    Shutdown {
        socket: socket_t,
        how: ShutdownHow,
    },

    Timer(Timer),
}

/// PORT NOTE: `posix.ShutdownHow` — the `how` selector of the `shutdown` op
/// (`.both` default).
#[derive(Clone, Copy)]
pub enum ShutdownHow {
    Recv,
    Send,
    Both,
}

/// PORT NOTE: `pub const Result = union(OperationType) { ... }`. Mirrors the
/// `Operation` tag subset ported so far (Task 3 core + Task 4 socket/stream).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Result {
    Noop,
    Cancel(core::result::Result<(), CancelError>),
    Accept(core::result::Result<socket_t, AcceptError>),
    Connect(core::result::Result<(), ConnectError>),
    Poll(core::result::Result<(), PollError>),
    Read(core::result::Result<usize, ReadError>),
    PRead(core::result::Result<usize, ReadError>),
    Write(core::result::Result<usize, WriteError>),
    PWrite(core::result::Result<usize, WriteError>),
    Send(core::result::Result<usize, WriteError>),
    Recv(core::result::Result<usize, ReadError>),
    SendMsg(core::result::Result<usize, WriteError>),
    RecvMsg(core::result::Result<usize, ReadError>),
    Close(core::result::Result<(), CloseError>),
    Shutdown(core::result::Result<(), ShutdownError>),
    Timer(core::result::Result<TimerTrigger, TimerError>),
}

/// PORT NOTE: `pub const CancelError = ThreadPoolError || error{NotFound}`. Only
/// the `Ok(())` result is produced by the ported paths; the variant is kept so
/// the result type is nameable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CancelError {
    NotFound,
}

/// PORT NOTE: `pub const AcceptError = posix.EpollCtlError || error{DupFailed,
/// Unknown}`. `Unknown` is the reference's `posix.accept catch → error.Unknown`;
/// `Unexpected` covers the `start`-arm epoll_ctl/dup failures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AcceptError {
    Unknown,
    Unexpected,
}

/// PORT NOTE: `pub const ConnectError = posix.EpollCtlError || posix.ConnectError
/// || error{DupFailed, Unknown}`. Collapsed to `Unexpected` (no in-scope test
/// exercises a specific connect-error path).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectError {
    Unexpected,
}

/// PORT NOTE: `pub const PollError = posix.EpollCtlError || error{DupFailed,
/// Unknown}` (the epoll `Sys.PollError`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PollError {
    Unexpected,
}

/// PORT NOTE: `pub const WriteError = ThreadPoolError || posix.EpollCtlError ||
/// posix.WriteError || posix.PWriteError || posix.SendError || error{DupFailed,
/// Unknown}`. The `ThreadPoolRequired` arm is reached when a threadpool-flagged
/// write/pwrite (File's, epoll arm) runs with no pool attached; the rest of the
/// posix syscall-error variants collapse to `Unexpected`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteError {
    ThreadPoolRequired,
    Unexpected,
}

/// PORT NOTE: `pub const CloseError = posix.EpollCtlError || ThreadPoolError ||
/// error{Unknown}`. The `ThreadPoolRequired` arm is reached when a threadpool-
/// flagged close (Stream.close) runs with no pool attached (deferred).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseError {
    ThreadPoolRequired,
    Unexpected,
}

/// PORT NOTE: `pub const ShutdownError = posix.EpollCtlError ||
/// posix.ShutdownError || error{Unknown}`. Collapsed to `Unexpected`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShutdownError {
    Unexpected,
}

/// PORT NOTE: `pub const ReadError = ThreadPoolError || posix.EpollCtlError ||
/// posix.ReadError || posix.PReadError || ... || error{DupFailed, EOF, Unknown}`.
/// The `ThreadPoolRequired` arm is reached when a threadpool-flagged read/pread
/// (File's, epoll arm) runs with no pool attached; the rest of the posix
/// syscall-error variants collapse to `Unexpected` (no in-scope test exercises a
/// raw read-error path).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadError {
    ThreadPoolRequired,
    EOF,
    Unexpected,
}

/// PORT NOTE: `pub const TimerError = error{Unexpected}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimerError {
    Unexpected,
}

/// PORT NOTE: `pub const TimerTrigger = enum { request, expiration, cancel }`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimerTrigger {
    /// Unused with epoll
    Request,
    /// Timer expired.
    Expiration,
    /// Timer was canceled.
    Cancel,
}

/// ReadBuffer are the various options for reading.
//
// PORT NOTE (R3): the Zig `slice: []u8` ports as a raw fat pointer `*mut [u8]`
// (never a lifetime — the buffer lives in caller memory and the loop re-enters it
// across ticks), NEVER a `String`.
#[derive(Clone, Copy)]
pub enum ReadBuffer {
    /// Read into this slice.
    Slice(*mut [u8]),

    /// Read into this array, just set this to undefined and it will
    /// be populated up to the size of the array.
    Array([u8; 32]),
    // TODO: future will have vectors
}

/// WriteBuffer are the various options for writing.
//
// PORT NOTE (R3): the Zig `slice: []const u8` ports as a raw fat pointer
// `*const [u8]` (never a lifetime — the buffer lives in caller memory and the
// loop re-enters it across ticks). The `array` arm mirrors the Zig
// `struct { array: [32]u8, len: usize }`.
#[derive(Clone, Copy)]
pub enum WriteBuffer {
    /// Write from this buffer.
    Slice(*const [u8]),

    /// Write from this array. See ReadBuffer.array for why we support this.
    Array { array: [u8; 32], len: usize },
    // TODO: future will have vectors
}

// PORT NOTE: `posix.read` wrapper the `perform` read path calls. Raw `libc`.
unsafe fn posix_read(fd: fd_t, ptr: *mut u8, len: usize) -> core::result::Result<usize, ReadError> {
    let n = unsafe { libc::read(fd, ptr.cast(), len) };
    if n < 0 {
        Err(ReadError::Unexpected)
    } else {
        Ok(n as usize)
    }
}

// PORT NOTE: `posix.pread(fd, buf, offset)` wrapper (the `pread` perform path).
unsafe fn posix_pread(
    fd: fd_t,
    ptr: *mut u8,
    len: usize,
    offset: u64,
) -> core::result::Result<usize, ReadError> {
    let n = unsafe { libc::pread(fd, ptr.cast(), len, offset as libc::off_t) };
    if n < 0 {
        Err(ReadError::Unexpected)
    } else {
        Ok(n as usize)
    }
}

// PORT NOTE: `posix.recv(fd, buf, 0)` wrapper (the `recv` perform path).
unsafe fn posix_recv(fd: fd_t, ptr: *mut u8, len: usize) -> core::result::Result<usize, ReadError> {
    let n = unsafe { libc::recv(fd, ptr.cast(), len, 0) };
    if n < 0 {
        Err(ReadError::Unexpected)
    } else {
        Ok(n as usize)
    }
}

// PORT NOTE: `posix.write` wrapper (the `write` perform path).
unsafe fn posix_write(
    fd: fd_t,
    ptr: *const u8,
    len: usize,
) -> core::result::Result<usize, WriteError> {
    let n = unsafe { libc::write(fd, ptr.cast(), len) };
    if n < 0 {
        Err(WriteError::Unexpected)
    } else {
        Ok(n as usize)
    }
}

// PORT NOTE: `posix.pwrite(fd, buf, offset)` wrapper (the `pwrite` perform path).
unsafe fn posix_pwrite(
    fd: fd_t,
    ptr: *const u8,
    len: usize,
    offset: u64,
) -> core::result::Result<usize, WriteError> {
    let n = unsafe { libc::pwrite(fd, ptr.cast(), len, offset as libc::off_t) };
    if n < 0 {
        Err(WriteError::Unexpected)
    } else {
        Ok(n as usize)
    }
}

// PORT NOTE: `posix.send(fd, buf, 0)` wrapper (the `send` perform path).
unsafe fn posix_send(
    fd: fd_t,
    ptr: *const u8,
    len: usize,
) -> core::result::Result<usize, WriteError> {
    let n = unsafe { libc::send(fd, ptr.cast(), len, 0) };
    if n < 0 {
        Err(WriteError::Unexpected)
    } else {
        Ok(n as usize)
    }
}

// PORT NOTE: `posix.sendmsg(fd, msghdr, 0)` wrapper (the `sendmsg` perform path).
//
// # Safety
// `fd` a live socket; `msghdr` (and everything it points at) valid for the call.
unsafe fn posix_sendmsg(
    fd: fd_t,
    msghdr: *const libc::msghdr,
) -> core::result::Result<usize, WriteError> {
    let n = unsafe { libc::sendmsg(fd, msghdr, 0) };
    if n < 0 {
        Err(WriteError::Unexpected)
    } else {
        Ok(n as usize)
    }
}

// PORT NOTE: `linux.recvmsg(fd, msghdr, 0)` wrapper (the `recvmsg` perform path):
// a zero-byte datagram is `error.EOF`; a negative return is `Unexpected`.
//
// # Safety
// `fd` a live socket; `msghdr` (and everything it points at) valid+writable for
// the call — the kernel writes `msg_name`/the iovec buffers.
unsafe fn posix_recvmsg(
    fd: fd_t,
    msghdr: *mut libc::msghdr,
) -> core::result::Result<usize, ReadError> {
    let n = unsafe { libc::recvmsg(fd, msghdr, 0) };
    if n < 0 {
        Err(ReadError::Unexpected)
    } else if n == 0 {
        Err(ReadError::EOF)
    } else {
        Ok(n as usize)
    }
}

/// PORT NOTE: `posix.getsockoptError(socket)` — reads `SO_ERROR` and maps to a
/// `ConnectError` (0 → success). The `connect` perform arm calls this once the
/// socket signals writable.
///
/// # Safety
/// `socket` must be a valid socket fd.
unsafe fn getsockopt_error(socket: socket_t) -> core::result::Result<(), ConnectError> {
    let mut err_code: libc::c_int = 0;
    let mut size = size_of::<libc::c_int>() as net::socklen_t;
    unsafe {
        libc::getsockopt(
            socket,
            libc::SOL_SOCKET,
            libc::SO_ERROR,
            ptr::addr_of_mut!(err_code).cast(),
            &mut size,
        )
    };
    if err_code == 0 {
        Ok(())
    } else {
        Err(ConnectError::Unexpected)
    }
}
