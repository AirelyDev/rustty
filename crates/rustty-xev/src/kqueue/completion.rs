//! The kqueue backend's `Completion` and the per-operation surface.
//!
//! PORTS: libxev src/backend/kqueue.zig — `Completion` (kqueue.zig:971), the loop
//! `Wakeup` (Darwin mach-port arm and FreeBSD stub, kept together), the
//! `Operation`/`Result` unions, the error sets and errno mappers, the `posix.*`
//! syscall wrappers, `ReadBuffer`/`WriteBuffer`, `Timer`, and the `Completion`
//! size tripwire.

use super::*;

// ---------------------------------------------------------------------------
// Completion (kqueue.zig:971).
// ---------------------------------------------------------------------------

/// A completion is a request to perform some work with the loop.
#[repr(C)]
pub struct Completion {
    /// Operation to execute.
    pub op: Operation,

    /// Userdata and callback for when the completion is finished.
    pub userdata: *mut core::ffi::c_void,
    pub callback: Callback,

    //---------------------------------------------------------------
    // Internal fields
    /// Intrusive queue field
    pub(super) next: *mut Completion,

    /// Result code of the syscall. Only used internally in certain
    /// scenarios, should not be relied upon by program authors.
    pub(super) result: Option<Result>,

    pub(super) flags: CompletionFlags,

    /// If scheduled on a thread pool, this will be set. This is NOT a
    /// reliable way to get access to the loop and shouldn't be used
    /// except internally.
    pub(super) task: thread_pool::Task,
    pub(super) task_loop: *mut Loop,

    /// The two loop-side handles `thread_perform` needs, COPIED INTO THIS
    /// COMPLETION at schedule time (WS19 T10).
    ///
    /// `task_completions` RESTORES the reference's own per-completion
    /// `task_completions: *TaskCompletionQueue` field, which the Task-2 port had
    /// elided in favour of reaching the queue via `task_loop` (see the elision
    /// note that used to sit here and its epoll.rs twin) — so this half is a
    /// FIDELITY RESTORATION, not a divergence. `task_wakeup` is PORT-ADDED
    /// [R1-DIVERGENCE]: the reference wakes the loop with `c.task_loop.wakeup()`.
    ///
    /// Why the port cannot: the loop thread is inside `tick(&mut self)` /
    /// `run(&mut self)` for the whole time a pool worker runs, so it holds a live
    /// (function-entry, PROTECTED) `&mut Loop`. ANY worker access to the `Loop`
    /// allocation is a foreign access that invalidates it — including a by-value
    /// read of a `Copy` handle that forms no reference at all. Proven on
    /// `.port/phase-2/mirihar-xev-mpsc`: `wakeup_naive` (the autoref'd
    /// `(*task_loop).wakeup()` this port shipped) is UB under SB **and** TB;
    /// `wakeup_copied` (the by-value-handle shape epoll.rs shipped as its fix) is
    /// still UB under SB; only `wakeup_completion_cached` — the worker touching
    /// ONLY the caller-owned `Completion` allocation — is CLEAN under both. The
    /// same argument condemns reading `(*task_loop).thread_pool_completions_ptr`,
    /// which is exactly what the restored reference field avoids.
    ///
    /// A `Completion` is allocated by the CALLER, never inside `Loop`, so it is
    /// immune to `Loop`'s per-tick reborrows. Both handles are written on the
    /// loop thread in `Loop::start`'s threadpool arm, before `schedule`.
    pub(super) task_completions: *mut TaskCompletionQueue,
    /// PORT NOTE (W2D): macOS-only, because the call it exists to serve is
    /// comptime-Darwin-gated in the reference: `thread_perform` ends with
    /// `if (comptime builtin.target.os.tag.isDarwin()) { c.task_loop.wakeup() ... }`
    /// (`kqueue.zig:965-968`). On FreeBSD the pool worker PUSHES to
    /// `thread_pool_completions` and returns — the completion is collected on the
    /// next tick — so there is no wakeup handle to cache, and inventing one
    /// (in particular a panicking one, since the FreeBSD `Wakeup.wakeup` IS a
    /// `@panic` stub, `kqueue.zig:1506-1509`) would crash every pool task, a
    /// behavior the reference does not have.
    #[cfg(any(target_os = "macos", target_os = "ios", target_os = "visionos"))]
    pub(super) task_wakeup: Async,
}

/// PORT NOTE: the `flags: packed struct { state: State, threadpool: bool }` sub-struct.
#[derive(Clone, Copy)]
pub(super) struct CompletionFlags {
    /// Watch state of this completion. We use this to determine whether
    /// we're active, adding, deleting, etc. This lets us add and delete
    /// multiple times before a loop tick and handle the state properly.
    pub(super) state: State,
    /// Set this to true to schedule this operation on the thread pool.
    /// This can be set by anyone. If the operation is scheduled on
    /// the thread pool then it will NOT be registered with kqueue even
    /// if it is supported.
    pub(super) threadpool: bool,
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
    /// completion is submitted with kqueue successfully
    Active = 3,
}

// PORT NOTE [ADV-FIX F3]: hand-written `Default` (NOT `#[derive]`) — `Completion`
// has an fn-ptr `callback` + raw-ptr fields with no `Default`. The Zig field
// defaults are `op = .noop`, `userdata = null`, `callback = noopCallback`,
// `next = null`, `result = null`, `flags = { state: dead, threadpool: false }`;
// `task`/`task_loop` are Zig `undefined` (only set when scheduled on the pool —
// never, while the pool is deferred), zeroed here (Rust has no undefined). This
// is load-bearing: termio's `xev_seam` (`pub use rustty_xev::*`) default-
// constructs `xev::Completion` in ~7 places (thread.rs ThreadData).
impl Default for Completion {
    fn default() -> Self {
        Completion {
            op: Operation::default(),
            userdata: ptr::null_mut(),
            callback: noop_callback,
            next: ptr::null_mut(),
            result: None,
            flags: CompletionFlags {
                state: State::Dead,
                threadpool: false,
            },
            task: thread_pool::Task::default(),
            task_loop: ptr::null_mut(),
            // PORT-ADDED (WS19 T10): same discipline as `task`/`task_loop` —
            // Zig `undefined`, only set when scheduled on the pool.
            task_completions: ptr::null_mut(),
            #[cfg(any(target_os = "macos", target_os = "ios", target_os = "visionos"))]
            task_wakeup: Async {
                port: darwin::MACH_PORT_NULL,
            },
        }
    }
}

impl Completion {
    /// PORT NOTE: the `c.* = .{ .op, .userdata, .callback }` initialization the watcher
    /// `wait`/`run` methods perform (e.g. `async.zig:326`, `timer.zig`) — a FULL
    /// reset with the internal fields (`next`/`result`/`flags`/`task`/`task_loop`)
    /// taking their `Default` values. Exposed `pub(crate)` so sibling watcher
    /// modules (`async_watcher`, …) can build a completion without seeing the
    /// module-private internal fields (Zig accesses those freely within the
    /// package; C4 `Async::wait` is the first cross-module consumer).
    pub(crate) fn new_op(
        op: Operation,
        userdata: *mut core::ffi::c_void,
        callback: Callback,
    ) -> Completion {
        Completion {
            op,
            userdata,
            callback,
            ..Completion::default()
        }
    }

    /// PORT NOTE: the `c.flags.threadpool = true` assignment the stream `Closeable`
    /// (and the `Readable`/`Writeable` kqueue arms when `options.threadpool`)
    /// perform inside the package (stream.zig:434/609/998). `flags` is module-
    /// private, so this `pub(crate)` seam lets the sibling `stream` module
    /// (C6) set the threadpool bit without seeing the internal flags layout —
    /// exactly as Zig sets `c.flags.threadpool` directly within the package.
    ///
    /// # Safety
    /// `c` must be a valid, stable `Completion` pointer.
    pub(crate) unsafe fn set_threadpool(c: *mut Completion, v: bool) {
        unsafe { (*c).flags.threadpool = v };
    }
}

// PORT NOTE [ADV-FIX F2]: `Completion` holds raw-ptr links (`next`/`task_loop`) +
// an fn-ptr → auto-`!Send`; it rides the `Loop` moved onto the termio thread.
// SAFETY / single-thread affinity: after the move a completion is touched by
// exactly the owning event-loop thread, so transferring ownership is sound.
unsafe impl Send for Completion {}

// PORT NOTE: intrusive `next` link seam for `queue::Intrusive<Completion>`
// (submissions/cancellations/completions).
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
    /// Returns the state of this completion. There are some things to
    /// be caution about when calling this function.
    ///
    /// First, this is only safe to call from the main thread. This cannot
    /// be called from any other thread.
    ///
    /// Second, if you are using default "undefined" completions, this will
    /// NOT return a valid value if you access it. You must zero your
    /// completion using ".{}". You only need to zero the completion once.
    /// Once the completion is in use, it will always be valid.
    ///
    /// Third, if you stop the loop (loop.stop()), the completions registered
    /// with the loop will NOT be reset to a dead state.
    //
    // PORT NOTE: reference `state(self: Completion)` (by value); ported as an
    // associated fn taking `*mut Completion` (raw self) to stay on the raw-ptr
    // discipline. Test call sites (`c1.state()`) route through the `state_of`
    // convenience below.
    pub(super) fn state(c: *mut Completion) -> CompletionState {
        match unsafe { (*c).flags.state } {
            State::Dead => CompletionState::Dead,
            State::Adding | State::Deleting | State::Active => CompletionState::Active,
        }
    }

    /// Convenience wrapper for `state` callable on a `&Completion` (tests do
    /// `c1.state()`).
    pub fn state_of(&self) -> CompletionState {
        Completion::state(self as *const Completion as *mut Completion)
    }

    /// Returns a kevent for this completion, if any. Note that the
    /// kevent isn't immediately useful for all event types. For example,
    /// "connect" requires you to initiate the connection first.
    ///
    /// # Safety
    /// `self_` must be a valid, stable `Completion` pointer.
    pub(super) unsafe fn kevent(self_: *mut Completion) -> Option<Kevent> {
        match unsafe { &(*self_).op } {
            Operation::Noop => unreachable!(),

            Operation::Cancel { .. }
            | Operation::Close { .. }
            | Operation::Timer(_)
            | Operation::Shutdown { .. } => None,

            Operation::Accept { socket, .. } => Some(kevent_init(KeventInit {
                ident: *socket as u64,
                filter: libc::EVFILT_READ,
                flags: libc::EV_ADD | libc::EV_ENABLE,
                fflags: 0,
                data: 0,
                udata: self_ as u64,
            })),

            Operation::Connect { socket, .. } => Some(kevent_init(KeventInit {
                ident: *socket as u64,
                filter: libc::EVFILT_WRITE,
                flags: libc::EV_ADD | libc::EV_ENABLE,
                fflags: 0,
                data: 0,
                udata: self_ as u64,
            })),

            // PORT NOTE (W2D): `.machport => if (comptime !builtin.os.tag.isDarwin())
            // return null else kevent: {...}` (`kqueue.zig:1081`) — the whole arm
            // is Darwin-only and the off-Darwin answer is `null`.
            #[cfg(target_os = "freebsd")]
            Operation::Machport => None,

            #[cfg(any(target_os = "macos", target_os = "ios", target_os = "visionos"))]
            Operation::Machport { port, buffer } => {
                // We can't use |*v| above because it crahses the Zig
                // compiler (as of 0.11.0-dev.1413). We can retry another time.
                // (Rust: matched by shared ref `buffer` in the arm above.)
                // The kevent below waits for a machport to have a message
                // available AND automatically reads the message into the
                // buffer since MACH_RCV_MSG is set.
                let (slice_ptr, slice_len): (*mut u8, usize) = match buffer {
                    ReadBuffer::Slice(slice) => (slice.cast::<u8>(), slice.len()),
                    // Take the address of the in-completion array buffer.
                    ReadBuffer::Array(arr) => (arr.as_ptr() as *mut u8, arr.len()),
                };
                Some(Kevent {
                    ident: *port as libc::c_uint as u64,
                    filter: libc::EVFILT_MACHPORT,
                    flags: libc::EV_ADD | libc::EV_ENABLE,
                    fflags: darwin::MACH_RCV_MSG as u32,
                    data: 0,
                    udata: self_ as u64,
                    ext: [slice_ptr as u64, slice_len as u64],
                })
            }

            Operation::Proc { pid, flags } => Some(kevent_init(KeventInit {
                ident: *pid as u64,
                filter: libc::EVFILT_PROC,
                flags: libc::EV_ADD | libc::EV_ENABLE,
                fflags: *flags,
                data: 0,
                udata: self_ as u64,
            })),

            // write, pwrite, send, sendto
            Operation::Write { fd, .. }
            | Operation::Pwrite { fd, .. }
            | Operation::Send { fd, .. }
            | Operation::Sendto { fd, .. } => Some(kevent_init(KeventInit {
                ident: *fd as u64,
                filter: libc::EVFILT_WRITE,
                flags: libc::EV_ADD | libc::EV_ENABLE,
                fflags: 0,
                data: 0,
                udata: self_ as u64,
            })),

            // read, pread, recv, recvfrom
            Operation::Read { fd, .. }
            | Operation::Pread { fd, .. }
            | Operation::Recv { fd, .. }
            | Operation::Recvfrom { fd, .. } => Some(kevent_init(KeventInit {
                ident: *fd as u64,
                filter: libc::EVFILT_READ,
                flags: libc::EV_ADD | libc::EV_ENABLE,
                fflags: 0,
                data: 0,
                udata: self_ as u64,
            })),
        }
    }

    /// Perform the operation associated with this completion. This will
    /// perform the full blocking operation for the completion.
    ///
    /// # Safety
    /// `self_` valid+stable; `ev_` (if Some) a valid kevent for this completion.
    pub(super) unsafe fn perform(self_: *mut Completion, ev_: Option<*const Kevent>) -> Result {
        match unsafe { &mut (*self_).op } {
            Operation::Cancel { .. }
            | Operation::Noop
            | Operation::Timer(_)
            | Operation::Shutdown { .. } => {
                // log.warn("perform op={s}", .{@tagName(self.op)});
                unreachable!()
            }

            Operation::Accept {
                socket,
                addr,
                addr_size,
                flags,
            } => Result::Accept({
                // SAFETY: accept into the in-completion addr/addr_size storage.
                let r = unsafe {
                    libc::accept(
                        *socket,
                        ptr::addr_of_mut!(*addr) as *mut libc::sockaddr,
                        addr_size,
                    )
                };
                if r < 0 {
                    Err(accept_err(errno()))
                } else {
                    // posix.accept applies SOCK.CLOEXEC via fcntl on macOS.
                    if *flags & SOCK_CLOEXEC != 0 {
                        unsafe { libc::fcntl(r, libc::F_SETFD, libc::FD_CLOEXEC) };
                    }
                    Ok(r as socket_t)
                }
            }),

            Operation::Connect { socket, .. } => {
                Result::Connect(unsafe { getsockopt_error(*socket) })
            }

            Operation::Write { fd, buffer } => Result::Write(match buffer {
                WriteBuffer::Slice(v) => unsafe { posix_write(*fd, v.cast::<u8>(), v.len()) },
                WriteBuffer::Array { array, len } => unsafe {
                    posix_write(*fd, array.as_ptr(), *len)
                },
            }),

            Operation::Pwrite { fd, buffer, offset } => Result::Pwrite(match buffer {
                WriteBuffer::Slice(v) => unsafe {
                    posix_pwrite(*fd, v.cast::<u8>(), v.len(), *offset)
                },
                WriteBuffer::Array { array, len } => unsafe {
                    posix_pwrite(*fd, array.as_ptr(), *len, *offset)
                },
            }),

            Operation::Send { fd, buffer } => Result::Send(match buffer {
                WriteBuffer::Slice(v) => unsafe { posix_send(*fd, v.cast::<u8>(), v.len()) },
                WriteBuffer::Array { array, len } => unsafe {
                    posix_send(*fd, array.as_ptr(), *len)
                },
            }),

            Operation::Sendto { fd, buffer, addr } => Result::Sendto(match buffer {
                WriteBuffer::Slice(v) => unsafe {
                    posix_sendto(*fd, v.cast::<u8>(), v.len(), addr)
                },
                WriteBuffer::Array { array, len } => unsafe {
                    posix_sendto(*fd, array.as_ptr(), *len, addr)
                },
            }),

            Operation::Read { fd, buffer } => {
                let n_: core::result::Result<usize, ReadError> = match buffer {
                    ReadBuffer::Slice(v) => {
                        if v.len() == 0 {
                            match ev_ {
                                None => return Result::Read(Err(ReadError::MissingKevent)),
                                Some(ev) => Ok(unsafe { (*ev).data } as usize),
                            }
                        } else {
                            unsafe { posix_read(*fd, v.cast::<u8>(), v.len()) }
                        }
                    }
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

            Operation::Pread { fd, buffer, offset } => {
                let n_: core::result::Result<usize, ReadError> = match buffer {
                    ReadBuffer::Slice(v) => {
                        if v.len() == 0 {
                            match ev_ {
                                None => return Result::Read(Err(ReadError::MissingKevent)),
                                Some(ev) => Ok(unsafe { (*ev).data } as usize),
                            }
                        } else {
                            unsafe { posix_pread(*fd, v.cast::<u8>(), v.len(), *offset) }
                        }
                    }
                    ReadBuffer::Array(v) => unsafe {
                        posix_pread(*fd, v.as_mut_ptr(), v.len(), *offset)
                    },
                };

                Result::Pread(match n_ {
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

            Operation::Recv { fd, buffer } => {
                let n_: core::result::Result<usize, ReadError> = match buffer {
                    ReadBuffer::Slice(v) => {
                        if v.len() == 0 {
                            match ev_ {
                                None => return Result::Read(Err(ReadError::MissingKevent)),
                                Some(ev) => Ok(unsafe { (*ev).data } as usize),
                            }
                        } else {
                            unsafe { posix_recv(*fd, v.cast::<u8>(), v.len()) }
                        }
                    }
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

            Operation::Recvfrom {
                fd,
                buffer,
                addr,
                addr_size,
            } => {
                let n_: core::result::Result<usize, ReadError> = match buffer {
                    ReadBuffer::Slice(v) => {
                        if v.len() == 0 {
                            match ev_ {
                                None => return Result::Read(Err(ReadError::MissingKevent)),
                                Some(ev) => Ok(unsafe { (*ev).data } as usize),
                            }
                        } else {
                            unsafe { posix_recvfrom(*fd, v.cast::<u8>(), v.len(), addr, addr_size) }
                        }
                    }
                    ReadBuffer::Array(v) => unsafe {
                        posix_recvfrom(*fd, v.as_mut_ptr(), v.len(), addr, addr_size)
                    },
                };

                Result::Recvfrom(match n_ {
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

            // Our machport operation ALWAYS has MACH_RCV set so there
            // is no operation to perform. kqueue automatically reads in
            // the mach message into the read buffer.
            #[cfg(any(target_os = "macos", target_os = "ios", target_os = "visionos"))]
            Operation::Machport { .. } => Result::Machport(Ok(())),

            // PORT NOTE (W2D): the reference's arm is ungated
            // (`kqueue.zig:1265`); only the pattern spelling differs, because
            // the `.machport` payload is `void` off Darwin (`:1674`) and Rust
            // spells a unit variant without braces.
            #[cfg(target_os = "freebsd")]
            Operation::Machport => Result::Machport(Ok(())),

            // For proc watching, it is identical to the syscall result.
            //
            // PORT NOTE (W2D): this decode is SHARED across macOS and FreeBSD,
            // exactly as the reference shares it (`kqueue.zig:1269-1283` has no
            // `builtin.os.tag` switch here). What differs is only the FLAG the
            // watcher armed: macOS `NOTE_EXIT | NOTE_EXITSTATUS`, FreeBSD plain
            // `NOTE_EXIT` (`kqueue.zig:45-49`) — on both, the delivered event's
            // `data` carries the raw `wait(2)` status, and `libc_wifexited`/
            // `libc_wexitstatus` are plain bit tests with no per-platform macro
            // dependency. `(*ev).data` is `i64` on BOTH kevent ABIs, so it must
            // NOT be routed through the `kev_udata` accessors.
            Operation::Proc { .. } => {
                let ev = match ev_ {
                    None => return Result::Proc(Err(ProcError::MissingKevent)),
                    Some(ev) => ev,
                };

                // If we have the exit status, we read it.
                if unsafe { (*ev).fflags } & NOTE_EXIT_FLAGS > 0 {
                    let data: u32 = unsafe { (*ev).data } as u32;
                    if libc_wifexited(data) {
                        return Result::Proc(Ok(libc_wexitstatus(data)));
                    }
                }

                Result::Proc(Ok(0))
            }

            Operation::Close { fd } => {
                // SAFETY: close the stored fd.
                unsafe { libc::close(*fd) };
                Result::Close(Ok(()))
            }
        }
    }

    /// Returns the error result for the given result code. This is called
    /// in the situation that kqueue fails to enqueue the completion or
    /// a raw syscall fails.
    ///
    /// # Safety
    /// `c` must be a valid completion pointer.
    pub(super) unsafe fn syscall_result(c: *mut Completion, r: i32) -> Result {
        // const errno: posix.E = if (r >= 0) .SUCCESS else @enumFromInt(-r);
        let errno: i32 = if r >= 0 { 0 } else { -r };
        match unsafe { &(*c).op } {
            Operation::Noop => unreachable!(),

            Operation::Accept { .. } => Result::Accept(match errno {
                0 => Ok(r as socket_t),
                libc::ECANCELED => Err(AcceptError::Canceled),
                e => Err(accept_err(e)),
            }),

            Operation::Connect { .. } => Result::Connect(match errno {
                0 => Ok(()),
                libc::ECANCELED => Err(ConnectError::Canceled),
                e => Err(connect_err(e)),
            }),

            Operation::Write { .. } => Result::Write(match errno {
                0 => Ok(r as usize),
                libc::ECANCELED => Err(WriteError::Canceled),
                libc::EPERM => Err(WriteError::PermissionDenied),
                e => Err(write_err(e)),
            }),

            Operation::Pwrite { .. } => Result::Pwrite(match errno {
                0 => Ok(r as usize),
                libc::ECANCELED => Err(WriteError::Canceled),
                e => Err(write_err(e)),
            }),

            Operation::Read { .. } => Result::Read(match errno {
                0 => {
                    if r == 0 {
                        Err(ReadError::EOF)
                    } else {
                        Ok(r as usize)
                    }
                }
                libc::ECANCELED => Err(ReadError::Canceled),
                libc::EPERM => Err(ReadError::PermissionDenied),
                e => Err(read_err(e)),
            }),

            Operation::Pread { .. } => Result::Pread(match errno {
                0 => {
                    if r == 0 {
                        Err(ReadError::EOF)
                    } else {
                        Ok(r as usize)
                    }
                }
                libc::ECANCELED => Err(ReadError::Canceled),
                e => Err(read_err(e)),
            }),

            Operation::Send { .. } => Result::Send(match errno {
                0 => Ok(r as usize),
                libc::ECANCELED => Err(WriteError::Canceled),
                e => Err(write_err(e)),
            }),

            Operation::Recv { .. } => Result::Recv(match errno {
                0 => {
                    if r == 0 {
                        Err(ReadError::EOF)
                    } else {
                        Ok(r as usize)
                    }
                }
                libc::ECANCELED => Err(ReadError::Canceled),
                e => Err(read_err(e)),
            }),

            Operation::Sendto { .. } => Result::Sendto(match errno {
                0 => Ok(r as usize),
                libc::ECANCELED => Err(WriteError::Canceled),
                e => Err(write_err(e)),
            }),

            Operation::Recvfrom { .. } => Result::Recvfrom(match errno {
                0 => Ok(r as usize),
                libc::ECANCELED => Err(ReadError::Canceled),
                e => Err(read_err(e)),
            }),

            #[cfg(any(target_os = "macos", target_os = "ios", target_os = "visionos"))]
            Operation::Machport { .. } => Result::Machport(match errno {
                0 => Ok(()),
                libc::ECANCELED => Err(MachPortError::Canceled),
                e => Err(machport_err(e)),
            }),

            // PORT NOTE (W2D): ungated in the reference (`kqueue.zig:1381`);
            // split here only because the off-Darwin variant is a unit variant.
            #[cfg(target_os = "freebsd")]
            Operation::Machport => Result::Machport(match errno {
                0 => Ok(()),
                libc::ECANCELED => Err(MachPortError::Canceled),
                e => Err(machport_err(e)),
            }),

            Operation::Proc { .. } => Result::Proc(match errno {
                0 => Ok(r as u32),
                libc::ECANCELED => Err(ProcError::Canceled),
                libc::ESRCH => Err(ProcError::NoSuchProcess),
                e => Err(proc_err(e)),
            }),

            Operation::Shutdown { .. } => Result::Shutdown(match errno {
                0 => Ok(()),
                libc::ECANCELED => Err(ShutdownError::Canceled),
                e => Err(shutdown_err(e)),
            }),

            Operation::Close { .. } => Result::Close(match errno {
                0 => Ok(()),
                libc::ECANCELED => Err(CloseError::Canceled),
                libc::EPERM => Err(CloseError::ThreadPoolRequired),
                e => Err(close_err(e)),
            }),

            Operation::Timer(_) => Result::Timer(match errno {
                // Success is impossible because timers don't execute syscalls.
                0 => unreachable!(),
                libc::ECANCELED => Err(TimerError::Canceled),
                _ => Err(TimerError::Unexpected),
            }),

            Operation::Cancel { .. } => Result::Cancel(match errno {
                0 => Ok(()),
                libc::ECANCELED => Err(CancelError::Canceled),
                // Syscall errors should not be possible since cancel
                // doesn't run any syscalls.
                _ => {
                    // posix.unexpectedErrno(err) catch {}; unreachable;
                    unreachable!()
                }
            }),
        }
    }
}

// ---------------------------------------------------------------------------
// Wakeup (kqueue.zig:1441) — Darwin mach-port wakeup + the non-Darwin stub.
// ---------------------------------------------------------------------------

/// The struct used for loop wakeup. This is only internal state.
//
// PORT NOTE: the `if (builtin.os.tag.isDarwin())` Darwin arm. The non-Darwin
// `else` arm is ported verbatim below (W2D).
#[cfg(any(target_os = "macos", target_os = "ios", target_os = "visionos"))]
pub(super) struct Wakeup {
    /// The mach port that this kqueue always has a filter for. Writing
    /// an empty message to this port can be used to wake up the loop
    /// at any time. Waking up the loop via this port won't trigger any
    /// particular completion, it just forces tick to cycle.
    pub(super) mach_port: Async,
    mach_port_buffer: [u8; 32],
}

#[cfg(any(target_os = "macos", target_os = "ios", target_os = "visionos"))]
impl Wakeup {
    pub(super) fn init() -> core::result::Result<Wakeup, Error> {
        let mach_port = Async::init()?;
        // errdefer mach_port.deinit();  (no fallible step follows)
        Ok(Wakeup {
            mach_port,
            mach_port_buffer: [0; 32],
        })
    }

    pub(super) fn deinit(&mut self) {
        self.mach_port.deinit();
    }

    pub(super) fn setup(&mut self, kqueue_fd: fd_t) -> core::result::Result<(), Error> {
        let events = [Kevent {
            ident: self.mach_port.port as u64,
            filter: libc::EVFILT_MACHPORT,
            flags: libc::EV_ADD | libc::EV_ENABLE,
            fflags: darwin::MACH_RCV_MSG as u32,
            data: 0,
            udata: 0,
            ext: [
                ptr::addr_of_mut!(self.mach_port_buffer) as u64,
                self.mach_port_buffer.len() as u64,
            ],
        }];
        // eventlist is empty (events[0..0]) — this is a pure changelist submit.
        let n = unsafe {
            kevent_syscall(
                kqueue_fd,
                events.as_ptr(),
                events.len(),
                ptr::null_mut(),
                0,
                None,
            )
        }?;
        assert!(n == 0);
        Ok(())
    }

    pub(super) fn wakeup(&mut self) -> core::result::Result<(), Error> {
        // try self.mach_port.notify();
        self.mach_port.notify().map_err(|_| Error)
    }
}

/// The struct used for loop wakeup. This is only internal state.
//
// PORT NOTE (W2D): the non-Darwin `else struct {...}` arm of `const Wakeup =
// if (builtin.os.tag.isDarwin()) struct {...} else struct {...}`
// (`kqueue.zig:1488-1510`). Ported BUG-FOR-BUG (R1): `init`/`deinit`/`setup` are
// no-ops (so no EVFILT_MACHPORT filter is ever registered on FreeBSD) and
// `wakeup` is the reference's `@panic`. Its verbatim comment:
//   // TODO: We should use eventfd for FreeBSD. Until this is
//   // implemented, loop wakeup will crash on BSD.
//
// REACHABILITY (S-13, UD275 / N255, re-verified against libxev 9ce8e8e6 pinned
// by ghostty@20abdb50a): this panic is UNREACHABLE, so R13 does not apply and
// the stub stays faithful (R1). Its only caller is `Loop.wakeup`, which is
// `fn wakeup` — NOT `pub` — in `kqueue.zig:973`, so nothing outside kqueue.zig
// can call it; inside kqueue.zig its one call site is `thread_perform`'s,
// behind `if (comptime builtin.target.os.tag.isDarwin())` (`:965-968`). On
// FreeBSD the pool worker just pushes to `thread_pool_completions` and the loop
// picks it up on its next tick. `xev.Async` does not route through it either:
// on non-Darwin kqueue it is `AsyncEventFd` (`watcher/async.zig:22-25`), its
// own eventfd registered with the loop. Ghostty never names `Loop.wakeup` (it is
// not in the static kqueue `Api()` nor in `dynamic.zig`), and never creates an
// `xev.ThreadPool`. In this port `Loop::wakeup` is likewise private and has no
// caller on any target (the Darwin pool path notifies the cached
// `task_wakeup` handle instead). Test: `tests::freebsd_wakeup_stub_is_the_reference_panic`.
#[cfg(target_os = "freebsd")]
pub(super) struct Wakeup;

#[cfg(target_os = "freebsd")]
impl Wakeup {
    pub(super) fn init() -> core::result::Result<Wakeup, Error> {
        // return .{};
        Ok(Wakeup)
    }

    pub(super) fn deinit(&mut self) {
        // _ = self;
    }

    pub(super) fn setup(&mut self, kqueue_fd: fd_t) -> core::result::Result<(), Error> {
        // _ = self;
        // _ = kqueue_fd;
        let _ = kqueue_fd;
        Ok(())
    }

    pub(super) fn wakeup(&mut self) -> core::result::Result<(), Error> {
        // _ = self;
        // @panic("wakeup not implemented on this platform");
        panic!("wakeup not implemented on this platform")
    }
}

// ---------------------------------------------------------------------------
// Operation / Result unions (kqueue.zig:1503/1527/1616) + errors + buffers.
// ---------------------------------------------------------------------------

// PORT NOTE: `pub const OperationType = enum { ... }` — the 17-variant tag. In Rust
// the tag is carried by the `Operation` enum itself (no separate type needed);
// the variant order is preserved for fidelity.

/// All the supported operations of this event loop. These are always
/// backend-specific and therefore the structure and types change depending
/// on the underlying system in use. The high level operations are
/// done by initializing the request handles.
//
// PORT NOTE: `pub const Operation = union(OperationType) { ... }`.
// `op: Operation = .{ .noop = {} }` — the `#[default]` `Noop` variant.
#[derive(Default)]
pub enum Operation {
    #[default]
    Noop,

    Accept {
        socket: socket_t,
        addr: net::sockaddr::Any,
        addr_size: socklen_t,
        flags: u32,
    },

    Connect {
        socket: socket_t,
        addr: net::Address,
    },

    Read {
        fd: fd_t,
        buffer: ReadBuffer,
    },

    Write {
        fd: fd_t,
        buffer: WriteBuffer,
    },

    Pread {
        fd: fd_t,
        buffer: ReadBuffer,
        offset: u64,
    },

    Pwrite {
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

    // Note: this is making our Completion quite large. We can follow
    // the pattern of io_uring and require another user-provided pointer
    // here for state to move all this stuff out to a pointer.
    Sendto {
        fd: fd_t,
        buffer: WriteBuffer,
        addr: net::Address,
    },

    Recvfrom {
        fd: fd_t,
        buffer: ReadBuffer,
        addr: net::sockaddr::Any,
        addr_size: socklen_t,
    },

    Close {
        fd: fd_t,
    },

    Shutdown {
        socket: socket_t,
        how: ShutdownHow,
    },

    Timer(Timer),

    Cancel {
        c: *mut Completion,
    },

    #[cfg(any(target_os = "macos", target_os = "ios", target_os = "visionos"))]
    Machport {
        port: darwin::mach_port_name_t,
        buffer: ReadBuffer,
    },

    /// PORT NOTE (W2D): `machport: if (!builtin.os.tag.isDarwin()) void else
    /// struct { port, buffer }` (`kqueue.zig:1674`). Off Darwin the reference's
    /// payload is `void` — the variant EXISTS in the tagged union (so `Result`
    /// and `OperationType` stay platform-free) but carries nothing and can never
    /// be usefully constructed, since `Completion.kevent()`'s machport arm is a
    /// comptime `return null` there (`kqueue.zig:1081`).
    #[cfg(target_os = "freebsd")]
    Machport,

    Proc {
        pid: pid_t,
        flags: u32,
    },
}

/// PORT NOTE: `posix.ShutdownHow` — the `how` selector of the `shutdown` op
/// (`.both` default).
#[derive(Clone, Copy)]
pub enum ShutdownHow {
    Recv,
    Send,
    Both,
}

/// PORT NOTE: `pub const Result = union(OperationType) { ... }`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Result {
    Noop,
    Accept(core::result::Result<socket_t, AcceptError>),
    Connect(core::result::Result<(), ConnectError>),
    Read(core::result::Result<usize, ReadError>),
    Write(core::result::Result<usize, WriteError>),
    Pread(core::result::Result<usize, ReadError>),
    Pwrite(core::result::Result<usize, WriteError>),
    Send(core::result::Result<usize, WriteError>),
    Recv(core::result::Result<usize, ReadError>),
    Sendto(core::result::Result<usize, WriteError>),
    Recvfrom(core::result::Result<usize, ReadError>),
    Close(core::result::Result<(), CloseError>),
    Shutdown(core::result::Result<(), ShutdownError>),
    Timer(core::result::Result<TimerTrigger, TimerError>),
    Cancel(core::result::Result<(), CancelError>),
    Machport(core::result::Result<(), MachPortError>),
    Proc(core::result::Result<u32, ProcError>),
}

// PORT NOTE: the libxev error sets combine the Zig `posix.*` error sets (std, not
// libxev) with libxev's own named variants (`Canceled`/`EOF`/`MissingKevent`/…).
// Faithfully enumerating every `posix` variant across six error sets is a
// disproportionate std port; since (a) `posix.*` are std, (b) libxev's own API
// only ever distinguishes the NAMED variants + a `posix.unexpectedErrno`
// catch-all (which itself collapses unknown errnos to `error.Unexpected`), and
// (c) no in-scope test exercises a raw posix syscall-error path, every non-named
// errno collapses to the `Unexpected` variant here (the mapping helpers below) —
// the faithful observable behavior. (memory `right-tool-for-the-job`.)

/// PORT NOTE: `pub const CancelError = error{Canceled}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CancelError {
    Canceled,
}

/// PORT NOTE: `pub const AcceptError = posix.KEventError || posix.AcceptError ||
/// error{Canceled, Unexpected}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AcceptError {
    Canceled,
    Unexpected,
}

/// PORT NOTE: `pub const ConnectError = posix.KEventError || posix.ConnectError ||
/// error{Canceled, Unexpected}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectError {
    Canceled,
    Unexpected,
}

/// PORT NOTE: `pub const ReadError = posix.KEventError || posix.ReadError ||
/// posix.PReadError || posix.RecvFromError || error{EOF, Canceled, MissingKevent,
/// PermissionDenied, Unexpected}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadError {
    EOF,
    Canceled,
    MissingKevent,
    PermissionDenied,
    Unexpected,
}

/// PORT NOTE: `pub const WriteError = posix.KEventError || posix.WriteError ||
/// posix.PWriteError || posix.SendError || posix.SendMsgError || posix.SendToError
/// || error{Canceled, PermissionDenied, Unexpected}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteError {
    Canceled,
    PermissionDenied,
    Unexpected,
}

/// PORT NOTE: `pub const MachPortError = posix.KEventError || error{Canceled,
/// Unexpected}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MachPortError {
    Canceled,
    Unexpected,
}

/// PORT NOTE: `pub const ProcError = posix.KEventError || error{Canceled,
/// MissingKevent, Unexpected, NoSuchProcess}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcError {
    Canceled,
    MissingKevent,
    Unexpected,
    NoSuchProcess,
}

/// PORT NOTE: `pub const ShutdownError = posix.ShutdownError || error{Canceled,
/// Unexpected}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShutdownError {
    Canceled,
    Unexpected,
}

/// PORT NOTE: `const ThreadPoolError = error{ThreadPoolRequired,
/// ThreadPoolUnsupported}` folded into `CloseError = ThreadPoolError ||
/// error{Canceled, Unexpected}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseError {
    ThreadPoolRequired,
    ThreadPoolUnsupported,
    Canceled,
    Unexpected,
}

/// PORT NOTE: `pub const TimerError = error{Canceled, Unexpected}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimerError {
    Canceled,
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

// PORT NOTE: the errno → error mappers. `posix.unexpectedErrno(e)` → `Unexpected`;
// `ECANCELED` → `Canceled` (used by the perform syscall paths). Named-errno arms
// that a specific op maps (`EPERM`/`ESRCH`) are handled at the op's call site.
fn accept_err(e: i32) -> AcceptError {
    match e {
        libc::ECANCELED => AcceptError::Canceled,
        _ => AcceptError::Unexpected,
    }
}
fn connect_err(e: i32) -> ConnectError {
    match e {
        libc::ECANCELED => ConnectError::Canceled,
        _ => ConnectError::Unexpected,
    }
}
fn read_err(e: i32) -> ReadError {
    match e {
        libc::ECANCELED => ReadError::Canceled,
        libc::EPERM => ReadError::PermissionDenied,
        _ => ReadError::Unexpected,
    }
}
fn write_err(e: i32) -> WriteError {
    match e {
        libc::ECANCELED => WriteError::Canceled,
        libc::EPERM => WriteError::PermissionDenied,
        _ => WriteError::Unexpected,
    }
}
fn machport_err(e: i32) -> MachPortError {
    match e {
        libc::ECANCELED => MachPortError::Canceled,
        _ => MachPortError::Unexpected,
    }
}
fn proc_err(e: i32) -> ProcError {
    match e {
        libc::ECANCELED => ProcError::Canceled,
        libc::ESRCH => ProcError::NoSuchProcess,
        _ => ProcError::Unexpected,
    }
}
fn shutdown_err(e: i32) -> ShutdownError {
    match e {
        libc::ECANCELED => ShutdownError::Canceled,
        _ => ShutdownError::Unexpected,
    }
}
fn close_err(e: i32) -> CloseError {
    match e {
        libc::ECANCELED => CloseError::Canceled,
        libc::EPERM => CloseError::ThreadPoolRequired,
        _ => CloseError::Unexpected,
    }
}

// PORT NOTE: the `posix.*` syscall wrappers `perform` calls (read/write families +
// sendto/recvfrom). Each returns the value or a mapped error (see the collapse
// note above). Raw `libc` bindings (§3).
unsafe fn posix_write(
    fd: fd_t,
    ptr: *const u8,
    len: usize,
) -> core::result::Result<usize, WriteError> {
    let n = unsafe { libc::write(fd, ptr.cast(), len) };
    if n < 0 {
        Err(write_err(errno()))
    } else {
        Ok(n as usize)
    }
}
unsafe fn posix_pwrite(
    fd: fd_t,
    ptr: *const u8,
    len: usize,
    offset: u64,
) -> core::result::Result<usize, WriteError> {
    let n = unsafe { libc::pwrite(fd, ptr.cast(), len, offset as libc::off_t) };
    if n < 0 {
        Err(write_err(errno()))
    } else {
        Ok(n as usize)
    }
}
unsafe fn posix_send(
    fd: fd_t,
    ptr: *const u8,
    len: usize,
) -> core::result::Result<usize, WriteError> {
    let n = unsafe { libc::send(fd, ptr.cast(), len, 0) };
    if n < 0 {
        Err(write_err(errno()))
    } else {
        Ok(n as usize)
    }
}
unsafe fn posix_sendto(
    fd: fd_t,
    ptr: *const u8,
    len: usize,
    addr: &net::Address,
) -> core::result::Result<usize, WriteError> {
    let n = unsafe {
        libc::sendto(
            fd,
            ptr.cast(),
            len,
            0,
            ptr::addr_of!(addr.any) as *const libc::sockaddr,
            addr.get_os_sock_len(),
        )
    };
    if n < 0 {
        Err(write_err(errno()))
    } else {
        Ok(n as usize)
    }
}
unsafe fn posix_read(fd: fd_t, ptr: *mut u8, len: usize) -> core::result::Result<usize, ReadError> {
    let n = unsafe { libc::read(fd, ptr.cast(), len) };
    if n < 0 {
        Err(read_err(errno()))
    } else {
        Ok(n as usize)
    }
}
unsafe fn posix_pread(
    fd: fd_t,
    ptr: *mut u8,
    len: usize,
    offset: u64,
) -> core::result::Result<usize, ReadError> {
    let n = unsafe { libc::pread(fd, ptr.cast(), len, offset as libc::off_t) };
    if n < 0 {
        Err(read_err(errno()))
    } else {
        Ok(n as usize)
    }
}
unsafe fn posix_recv(fd: fd_t, ptr: *mut u8, len: usize) -> core::result::Result<usize, ReadError> {
    let n = unsafe { libc::recv(fd, ptr.cast(), len, 0) };
    if n < 0 {
        Err(read_err(errno()))
    } else {
        Ok(n as usize)
    }
}
unsafe fn posix_recvfrom(
    fd: fd_t,
    ptr: *mut u8,
    len: usize,
    addr: &mut net::sockaddr::Any,
    addr_size: &mut socklen_t,
) -> core::result::Result<usize, ReadError> {
    let n = unsafe {
        libc::recvfrom(
            fd,
            ptr.cast(),
            len,
            0,
            ptr::addr_of_mut!(*addr) as *mut libc::sockaddr,
            addr_size,
        )
    };
    if n < 0 {
        Err(read_err(errno()))
    } else {
        Ok(n as usize)
    }
}

/// PORT NOTE: `posix.getsockoptError(socket)` — reads `SO_ERROR` and maps to a
/// `ConnectError` (0 → success).
///
/// # Safety
/// `socket` must be a valid socket fd.
unsafe fn getsockopt_error(socket: socket_t) -> core::result::Result<(), ConnectError> {
    let mut err_code: libc::c_int = 0;
    let mut size = size_of::<libc::c_int>() as socklen_t;
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
        Err(connect_err(err_code))
    }
}

// PORT NOTE: `posix.W.IFEXITED` / `posix.W.EXITSTATUS` (the proc-watch exit decode).
fn libc_wifexited(status: u32) -> bool {
    (status & 0x7f) == 0
}
fn libc_wexitstatus(status: u32) -> u32 {
    (status >> 8) & 0xff
}

// ---------------------------------------------------------------------------
// ReadBuffer / WriteBuffer (kqueue.zig:1718/1745).
// ---------------------------------------------------------------------------

/// ReadBuffer are the various options for reading.
//
// PORT NOTE (R3): the Zig `slice: []u8` byte-slice ports as a raw fat pointer
// `*mut [u8]` (never a lifetime — the buffer lives in caller memory and the loop
// re-enters it across ticks), NEVER a `String`.
#[derive(Clone, Copy)]
pub enum ReadBuffer {
    /// Read into this slice.
    ///
    /// For zero-length slices, the event will notify of read readiness
    /// (similar to poll) rather than performing an explicit zero-length
    /// read which will always succeed regardless of if the fd is ready
    /// or not.
    Slice(*mut [u8]),

    /// Read into this array, just set this to undefined and it will
    /// be populated up to the size of the array. This is an option because
    /// the other union members force a specific size anyways so this lets us
    /// use the other size in the union to support small reads without worrying
    /// about buffer allocation.
    ///
    /// To know the size read you have to use the return value of the
    /// read operations (i.e. recv).
    ///
    /// Note that the union at the time of this writing could accomodate a
    /// much larger fixed size array here but we want to retain flexiblity
    /// for future fields.
    Array([u8; 32]),
    // TODO: future will have vectors
}

/// WriteBuffer are the various options for writing.
#[derive(Clone, Copy)]
pub enum WriteBuffer {
    /// Write from this buffer.
    Slice(*const [u8]),

    /// Write from this array. See ReadBuffer.array for why we support this.
    Array { array: [u8; 32], len: usize },
    // TODO: future will have vectors
}

// ---------------------------------------------------------------------------
// Timer (heap node) (kqueue.zig:1758).
// ---------------------------------------------------------------------------

/// Timer that is inserted into the heap.
pub struct Timer {
    /// The absolute time to fire this timer next.
    pub(super) next: Timespec,

    /// Only used internally. If this is non-null and timer is
    /// CANCELLED, then the timer is rearmed automatically with this
    /// as the next time. The callback will not be called on the
    /// cancellation.
    pub(super) reset: Option<Timespec>,

    /// Internal heap fields.
    pub(super) heap: heap::IntrusiveField<Timer>,

    /// We point back to completion for now. When issue[1] is fixed,
    /// we can juse use that from our heap fields.
    /// [1]: https://github.com/ziglang/zig/issues/6611
    pub(super) c: *mut Completion,
}

// PORT NOTE: the intrusive `heap` field seam for `heap::Intrusive<Timer, ()>` (the
// loop's `TimerHeap`).
impl heap::Node for Timer {
    unsafe fn heap_field(this: *mut Self) -> *mut heap::IntrusiveField<Self> {
        unsafe { ptr::addr_of_mut!((*this).heap) }
    }
}

/// The address of `c`'s `.timer` payload, carrying `c`'s OWN provenance.
///
/// PORT NOTE (N434): Zig reaches the timer as the place `c.op.timer` (a plain
/// `*Timer` into the completion), and the heap links it. The Rust port must not
/// derive that pointer from a `&mut (*c).op` reborrow: the heap keeps it while the
/// timer is linked, and the next `&mut (*c).op` (`timer_reset`, `stop_completion`)
/// is a fresh Unique retag from `c` that pops it under Stacked Borrows (and, once
/// the heap has written the node's links through it, is a Tree Borrows conflict).
/// This helper forms only a shared borrow of the op to find the payload and
/// returns that address with `c`'s tag, so the heap's links and every later
/// access on the heap path share one tag. Every heap-path Timer access goes
/// through it (proof: `.port/phase-2/mirihar-n434`).
///
/// # Safety
/// `c` must be a valid `Completion` pointer whose op is `.timer`.
#[inline]
pub(super) unsafe fn timer_ptr(c: *mut Completion) -> *mut Timer {
    unsafe {
        let addr = match &(*c).op {
            Operation::Timer(t) => ptr::from_ref(t).addr(),
            _ => unreachable!(),
        };
        c.cast::<u8>().with_addr(addr).cast::<Timer>()
    }
}

impl Timer {
    /// PORT NOTE: `fn less(_: void, a: *const Timer, b: *const Timer) bool`.
    pub(super) fn less(_: (), a: *mut Timer, b: *mut Timer) -> bool {
        unsafe { Timer::ns(a) < Timer::ns(b) }
    }

    /// Returns the nanoseconds of this timer. Note that maxInt(u64) ns is
    /// 584 years so if we get any overflows we just use maxInt(u64). If
    /// any software is running in 584 years waiting on this timer...
    /// shame on me I guess... but I'll be dead.
    ///
    /// # Safety
    /// `this` must be a valid `Timer` pointer.
    unsafe fn ns(this: *mut Timer) -> u64 {
        let next = unsafe { (*this).next };
        debug_assert!(next.tv_sec >= 0);
        debug_assert!(next.tv_nsec >= 0);

        let max = u64::MAX;
        let s_ns = match (next.tv_sec as u64).checked_mul(ns_per_s) {
            Some(v) => v,
            None => return max,
        };
        s_ns.checked_add(next.tv_nsec as u64).unwrap_or(max)
    }
}

// PORT NOTE: `comptime { if (@sizeOf(Completion) != 256) { @compileLog(...); unreachable; } }`
// — the layout tripwire (kqueue.zig:1866).
//
// PORT NOTE: the reference asserts the Zig `@sizeOf(Completion) == 256`. Rust's
// tagged-enum + struct layout differs FUNDAMENTALLY from Zig's tagged-union
// layout (tag placement, niche optimization, non-bit-packed `flags`), so the
// concrete byte size is not 256 and cannot be made 256 without adding padding
// fields absent from the reference (which would both be unfaithful AND defeat the
// guard). The tripwire's PURPOSE — trip on any accidental `Completion` bloat — is
// preserved by pinning the measured Rust size; drift in the op union / buffers /
// error sets will fail this `const` assert exactly as the Zig `@compileLog` does.
// (See DEFERRALS.md; surfaced in the chunk report as a deliberate divergence.)
//
// Measured breakdown (Rust 248 vs Zig 256): `Operation` 160B (Rust tagged-enum +
// niche) vs Zig `union(OperationType)` 176B; `flags` a 2-byte struct vs Zig's
// bit-packed 1 byte; the ThreadPool `Task.node` is now the real intrusive Zap
// node (+16B over the former ZST placeholder, once the pool was ported).
// `Result` (24B) and `Address` (108B) match Zig.
//
// RE-MEASURED (WS19 T10): 248 → 264. The pool-worker aliasing fix puts the two
// loop-side handles `thread_perform` needs in the completion itself, so the worker
// never accesses the `Loop` allocation: `task_completions: *mut TaskCompletionQueue`
// (8 — the reference's OWN field, restored after the Task-2 elision, so this half
// moves the Rust size TOWARD Zig's) + `task_wakeup: Async` (4, port-added, +4 tail
// padding). See the field docs and the `[R1-DIVERGENCE]` entry in
// `.port/phase-2/DEFERRALS.md`.
//
// UPSTREAM DRIFT (unabsorbed): the libxev bundled by the ghostty pin 6ad1fe7d
// (libxev 9ce8e8e6) replaced the `== 256` equality with a `<= 256` CEILING
// (`if (@sizeOf(Completion) > 256) @compileLog`, kqueue.zig:1949-1950).
// Rustty's 264 exceeds that ceiling for the documented reasons above (Rust
// tagged-enum layout + the WS19 T10 `task_wakeup` SB-soundness fields). The
// bump is a documented non-port (`.port/phase-3/NON_PORTS.md` G3; rustty-xev
// is a D6 Zig-port-defer and does not track libxev revisions) — a future bump
// absorption re-measures against the <=256 ceiling.
//
// W2D (per-target): the FreeBSD `Completion` is 256 — exactly 8 bytes smaller,
// because `task_wakeup: Async` (4 + 4 tail padding) is macOS-only there
// (`thread_perform`'s wakeup call is comptime-Darwin-gated, kqueue.zig:965-968,
// so there is no handle to cache). That lands FreeBSD ON the upstream `<= 256`
// ceiling; only the macOS arm carries the documented overshoot above.
#[cfg(any(target_os = "macos", target_os = "ios", target_os = "visionos"))]
const COMPLETION_SIZE: usize = 264;
#[cfg(target_os = "freebsd")]
const COMPLETION_SIZE: usize = 256;
const _: () = assert!(size_of::<Completion>() == COMPLETION_SIZE);
