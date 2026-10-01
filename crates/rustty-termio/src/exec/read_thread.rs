//! PORTS: ghostty src/termio/Exec.zig — the `ReadThread` struct (`threadMainPosix`, `gatherMainPosix`,
//! `threadMainWindows` and their helpers). The read pipeline's shared ring
//! (`Pipeline`) stays in `exec.rs`.

use super::*;

// pub const ReadThread = struct {
pub struct ReadThread;

/// The spawn arguments for [`ReadThread::threadMainPosix`], wrapped so the
/// `!Send` `*mut Termio<M>` can cross the `std::thread::spawn` boundary.
//
// PORT NOTE (blueprint §3): the reference spawns `std.Thread.spawn(...,
// threadMainPosix, .{ pty_fds.read, io, pipe[0] })`. The `io: *mut Termio<M>`
// raw pointer is `!Send`, so — exactly the `IoThreadArgs` (surface.rs) precedent
// for the writer thread — a tiny newtype carries it with an `unsafe impl Send`.
// The contract: the read thread is the SOLE toucher of `*io` (for
// `processOutput`) between the spawn and `Exec::threadExit`'s `join()`; the io
// thread owns it before the spawn and after the join.
//
// (S211-F3: the two descriptors are `posix.fd_t`, a HANDLE on Windows, where
// `threadMainWindows` takes them.)
pub(crate) struct ReadThreadArgs<M: rustty_apprt_api::surface::AppMailbox>(
    pub ReadThreadFd,
    pub *mut crate::Termio<M>,
    pub ReadThreadFd,
);

/// `posix.fd_t` for the read thread's pty and quit descriptors.
#[cfg(not(windows))]
pub(crate) type ReadThreadFd = std::os::fd::RawFd;
/// `posix.fd_t` is a HANDLE on Windows.
#[cfg(windows)]
pub(crate) type ReadThreadFd = rustty_os::windows::HANDLE;

// SAFETY: see the newtype's contract — the wrapped `*mut Termio<M>` has a single
// toucher (the read thread) for the window it is shared, matching the landed
// `IoThreadArgs` unsafe-Send discipline (surface.rs).
unsafe impl<M: rustty_apprt_api::surface::AppMailbox> Send for ReadThreadArgs<M> {}

impl ReadThread {
    /// The number of buffers rotated between the gather and parse
    /// stages. The gather stage can run at most this many batches
    /// ahead of the parse stage before it blocks, which (via the
    /// kernel pty queue) is also what preserves flow control to the
    /// child. Empirically chosen through measurements on an M4 Max.
    /// Less than 4 there are minor slowdowns, above 4 there are no
    /// improvements.
    // const buffer_count = 4;
    //
    // CORRECTION (S211-F1): this was `4` on Darwin and `2` elsewhere, citing
    // `if (builtin.os.tag.isDarwin()) 4 else 2`. That is upstream bed47168c
    // ("termio: bound POSIX read-ahead on non-Darwin"), which 60121a039
    // reverted before the pin; at 20abdb50a both constants are unconditional
    // (`Exec.zig:1305`, `:1318`). The doc paragraph that went with it ("Other
    // POSIX platforms keep fewer batches in flight ...") is gone with it.
    #[cfg_attr(windows, allow(dead_code))] // (read only by `threadMainPosix`)
    pub(crate) const BUFFER_COUNT: usize = 4;

    /// The capacity of each gather buffer. One batch is also the unit
    /// of work the parse stage does per terminal lock acquisition, so
    /// this bounds both gather latency and lock hold time.
    // const buffer_capacity = 64 * 1024;
    //
    // CORRECTION (S211-F1): was `64 * 1024` on Darwin and `8 * 1024` elsewhere
    // (the same reverted commit).
    #[cfg_attr(windows, allow(dead_code))] // (read only by `threadMainPosix`)
    pub(crate) const BUFFER_CAPACITY: usize = 64 * 1024;

    /// How many gathered bytes mark a stream as saturated. The macOS
    /// kernel tty output queue hands the master at most about 1 KiB
    /// per read, so gathering a full 1 KiB means the writer filled
    /// the queue (a bulk stream worth briefly waiting on), while
    /// anything smaller is an interactive trickle that must be
    /// delivered with no added latency.
    // const bridge_threshold = 1024;
    #[cfg_attr(windows, allow(dead_code))] // (read only by `threadMainPosix`)
    const BRIDGE_THRESHOLD: usize = 1024;

    /// How many times an EAGAIN on a saturated stream is retried
    /// with an immediate read before we're willing to sleep in poll.
    /// Basically, a spin retry.
    ///
    /// The writer refills the drained kernel queue within a few
    /// microseconds, while a sleep and wakeup through poll costs
    /// several more. If the gather stage sleeps on every refill gap,
    /// the whole pipeline degenerates to lockstep with the writer at
    /// about 1 KiB per wakeup. A short burst of nonblocking reads
    /// bridges nearly all refill gaps without sleeping. Measured, 8
    /// to 16 retries catches over 90% of the gaps and nearly doubles
    /// the saturated drain rate, and larger values helped little.
    /// The cost is bounded to at most this many extra ~0.5us read
    /// syscalls per gap, and we only spin on streams that already
    /// gathered a full kernel queue, so an idle or interactive
    /// terminal never spins.
    // const bridge_spin_max = 16;
    #[cfg_attr(windows, allow(dead_code))] // (read only by `threadMainPosix`)
    const BRIDGE_SPIN_MAX: usize = 16;

    /// How long one bridge poll waits for the writer's next refill
    /// once the spin retries above have failed. If the stream is
    /// quiet for this long the burst is over and we deliver what we
    /// have.
    // const bridge_poll_timeout_ms = 1;
    #[cfg_attr(windows, allow(dead_code))] // (read only by `threadMainPosix`)
    const BRIDGE_POLL_TIMEOUT_MS: libc::c_int = 1;

    /// The longest one batch may spend bridging refill gaps before it
    /// is delivered regardless. This bounds output latency for
    /// streams that produce just enough to keep bridging. Three
    /// milliseconds is well under one display frame, so batching is
    /// invisible on screen.
    // const gather_budget_ns = 3 * std.time.ns_per_ms;
    #[cfg_attr(windows, allow(dead_code))] // (read only by `threadMainPosix`)
    pub(crate) const GATHER_BUDGET_NS: u64 = 3 * 1_000_000;

    // [task-18 STALE HEADER STRUCK (S211-F7): this said "DEFERRED (D6 thread
    // lifecycle, 0 tests — R1-safe): spawned only by the deferred threadEnter;
    // drives the deferred `Termio.processOutput` …". `threadMainPosix` is FULLY
    // IMPLEMENTED at :4482 below, `threadEnter` (:441) really does spawn it, and
    // `Termio::processOutput` is live. The crash-sentry `thread_state` clause
    // that stood after that was closed by T-G (S211-F2); see its site.]
    // Reference, verbatim:
    //
    // fn threadMainPosix(fd: posix.fd_t, io: *termio.Termio, quit: posix.fd_t) void {
    //     // Always close our end of the pipe when we exit.
    //     defer posix.close(quit);
    //
    //     // Right now, on Darwin, `std.Thread.setName` can only name the current
    //     // thread, and we have no way to get the current thread from within it,
    //     // so instead we use this code to name the thread instead.
    //     if (builtin.os.tag.isDarwin()) {
    //         internal_os.macos.pthread_setname_np(&"io-reader".*);
    //     }
    //
    //     // Setup our crash metadata
    //     crash.sentry.thread_state = .{
    //         .type = .io,
    //         .surface = io.surface_mailbox.surface,
    //     };
    //     defer crash.sentry.thread_state = null;
    //
    //     // First thing, we want to set the fd to non-blocking. We do this
    //     // so that we can try to read from the fd in a tight loop and only
    //     // check the quit fd occasionally.
    //     if (posix.fcntl(fd, posix.F.GETFL, 0)) |flags| {
    //         _ = posix.fcntl(
    //             fd,
    //             posix.F.SETFL,
    //             flags | @as(u32, @bitCast(posix.O{ .NONBLOCK = true })),
    //         ) catch |err| {
    //             log.warn("read thread failed to set flags err={}", .{err});
    //             log.warn("this isn't a fatal error, but may cause performance issues", .{});
    //         };
    //     } else |err| {
    //         log.warn("read thread failed to get flags err={}", .{err});
    //         log.warn("this isn't a fatal error, but may cause performance issues", .{});
    //     }
    //
    //     // Build up the list of fds we're going to poll. We are looking
    //     // for data on the pty and our quit notification.
    //     var pollfds: [2]posix.pollfd = .{
    //         .{ .fd = fd, .events = posix.POLL.IN, .revents = undefined },
    //         .{ .fd = quit, .events = posix.POLL.IN, .revents = undefined },
    //     };
    //
    //     var buf: [1024]u8 = undefined;
    //     while (true) {
    //         // We try to read from the file descriptor as long as possible
    //         // to maximize performance. We only check the quit fd if the
    //         // main fd blocks. This optimizes for the realistic scenario that
    //         // the data will eventually stop while we're trying to quit. This
    //         // is always true because we kill the process.
    //         while (true) {
    //             const n = posix.read(fd, &buf) catch |err| {
    //                 switch (err) {
    //                     // This means our pty is closed. We're probably
    //                     // gracefully shutting down.
    //                     error.NotOpenForReading,
    //                     error.InputOutput,
    //                     => {
    //                         log.info("io reader exiting", .{});
    //                         return;
    //                     },
    //
    //                     // No more data, fall back to poll and check for
    //                     // exit conditions.
    //                     error.WouldBlock => break,
    //
    //                     else => {
    //                         log.err("io reader error err={}", .{err});
    //                         unreachable;
    //                     },
    //                 }
    //             };
    //
    //             // This happens on macOS instead of WouldBlock when the
    //             // child process dies. To be safe, we just break the loop
    //             // and let our poll happen.
    //             if (n == 0) break;
    //
    //             // log.info("DATA: {d}", .{n});
    //             @call(.always_inline, termio.Termio.processOutput, .{ io, buf[0..n] });
    //         }
    //
    //         // Wait for data.
    //         _ = posix.poll(&pollfds, -1) catch |err| {
    //             log.warn("poll failed on read thread, exiting early err={}", .{err});
    //             return;
    //         };
    //
    //         // If our quit fd is set, we're done.
    //         if (pollfds[1].revents & posix.POLL.IN != 0) {
    //             log.info("read thread got quit signal", .{});
    //             return;
    //         }
    //
    //         // If our pty fd is closed, then we're also done with our
    //         // read thread.
    //         if (pollfds[0].revents & posix.POLL.HUP != 0) {
    //             log.info("pty fd closed, read thread exiting", .{});
    //             return;
    //         }
    //     }
    // }
    // PORTED (xev B3, then rewritten as the two-stage pipeline by 2f0e6659d +
    // bb0ac4c72 in WS10 T20): the real raw `poll(2)`/`read`/`processOutput`
    // pipeline, below the preserved verbatim comment above. This function is
    // the PARSE stage; `gatherMainPosix` below is the gather stage.
    #[cfg(not(windows))]
    pub(super) fn threadMainPosix<M: rustty_apprt_api::surface::AppMailbox>(
        fd: std::os::fd::RawFd,
        io: *mut crate::Termio<M>,
        quit: std::os::fd::RawFd,
    ) {
        // Always close our end of the pipe when we exit.
        // defer posix.close(quit);
        //
        // PORT NOTE: `defer` -> a Drop guard (the `CloseHandleGuard` precedent) so the
        // quit-pipe read end is closed on EVERY return path below — including the
        // two NEW early returns this commit adds (the `setNonblock` failure and
        // the gather-thread spawn failure), and including a panic unwind.
        // Declared BEFORE the scoped gather thread so it drops AFTER the join:
        // the gather stage polls `quit` for its whole life.
        struct QuitFdCloser(std::os::fd::RawFd);
        impl Drop for QuitFdCloser {
            fn drop(&mut self) {
                // posix.close(quit);
                unsafe {
                    libc::close(self.0);
                }
            }
        }
        let _quit_closer = QuitFdCloser(quit);

        // Right now, on Darwin, `std.Thread.setName` can only name the current
        // thread, and we have no way to get the current thread from within it,
        // so instead we use this code to name the thread instead.
        // if (builtin.os.tag.isDarwin()) {
        //     internal_os.macos.pthread_setname_np(&"io-reader".*);
        //     setQosClass();
        // }
        #[cfg(any(target_os = "macos", target_os = "ios"))]
        unsafe {
            rustty_os::macos::pthread_setname_np(c"io-reader".as_ptr());
        }
        #[cfg(any(target_os = "macos", target_os = "ios"))]
        Self::setQosClass();

        // Setup our crash metadata
        // crash.sentry.thread_state = .{
        //     .type = .io,
        //     .surface = io.surface_mailbox.surface,
        // };
        // defer crash.sentry.thread_state = null;
        //
        // CORRECTION (S211-F2): this was "DEFERRED (cross-crate crash-sentry
        // seam, not part of B3) ... the `AppMailbox` trait does not yet expose a
        // `.surface` accessor ... wired when core lands." It now does
        // (`AppMailbox::crash_surface`); the guard's `Drop` is the `defer`,
        // dropped before `_quit_closer`, the reference's LIFO order.
        let _crash_thread_state = unsafe { crate::IoCrashThreadState::set(io) };

        // Set the fd to non-blocking so the gather stage can drain it
        // in a tight loop and fall back to poll for readiness. The
        // pipeline can't run with a blocking fd (a blocking read
        // would hang the gather stage on a quiet pty), but this also
        // can't realistically fail on a valid pty master.
        // if (!setNonblock(fd)) {
        //     log.err("read thread exiting, pty fd must be non-blocking", .{});
        //     return;
        // }
        //
        // PORT NOTE (R7, checked under R13 by task R13-C, inventory M88): the
        // early return below would leave the pty with no reader (the `?Es`
        // wedge `drain_pty_master_once` exists for), but it is not reachable,
        // so it stays faithful. `fd` is the pty master `threadEnter` opened
        // and keeps open until this thread is joined, and `F_GETFL`/`F_SETFL`
        // on an open descriptor fails only for an invalid one. Measured on
        // macOS 26.5.1 (XNU also pushes `FIONBIO` to the tty, which accepts
        // it): a fresh master, a master whose slave is closed, and a master
        // whose session leader has exited all take `O_NONBLOCK`; only a closed
        // descriptor fails (`EBADF`). The same four cases on Linux (Fedora,
        // kernel 6.19.10 aarch64) give the same results; there `setfl`
        // refuses only `O_APPEND`/`O_DIRECT`/`O_NOATIME` changes, which this
        // call does not make.
        if !Self::setNonblock(fd) {
            // log.err("read thread exiting, pty fd must be non-blocking", .{});
            log::error!(target: LOG_SCOPE, "read thread exiting, pty fd must be non-blocking");
            return;
        }

        // Shared pipeline
        // var pipeline: Pipeline = .{};
        let mut pipeline = Pipeline::new();

        // The idle self-pipe (see the Pipeline field docs). If we
        // can't create it we still run correctly, bridge polls are
        // just bounded by their timeout instead of being interrupted
        // when the parse stage goes idle.
        // if (posix.pipe2(.{ .CLOEXEC = true, .NONBLOCK = true })) |fds| { ... }
        //
        // PORT NOTE: `pipe2(2)` is Linux-only; macOS has no such call, so this
        // is `pipe(2)` + the two `fcntl` flag sets, which is what Zig's
        // `posix.pipe2` itself falls back to on Darwin.
        let mut idle_fds = [-1 as std::os::fd::RawFd; 2];
        if unsafe { libc::pipe(idle_fds.as_mut_ptr()) } == 0 {
            for fd in idle_fds {
                unsafe {
                    let fl = libc::fcntl(fd, libc::F_GETFL, 0);
                    if fl != -1 {
                        libc::fcntl(fd, libc::F_SETFL, fl | libc::O_NONBLOCK);
                    }
                    let fd_fl = libc::fcntl(fd, libc::F_GETFD, 0);
                    if fd_fl != -1 {
                        libc::fcntl(fd, libc::F_SETFD, fd_fl | libc::FD_CLOEXEC);
                    }
                }
            }
            pipeline.idle_read_fd = idle_fds[0];
            pipeline.idle_write_fd = idle_fds[1];
        } else {
            // log.warn("read thread failed to create idle pipe err={}", .{err});
            log::warn!(
                target: LOG_SCOPE,
                "read thread failed to create idle pipe err={}",
                std::io::Error::last_os_error()
            );
        }
        // defer if (pipeline.idle_read_fd >= 0) { close both };
        struct IdlePipeCloser(std::os::fd::RawFd, std::os::fd::RawFd);
        impl Drop for IdlePipeCloser {
            fn drop(&mut self) {
                if self.0 >= 0 {
                    unsafe {
                        libc::close(self.0);
                        libc::close(self.1);
                    }
                }
            }
        }
        let _idle_closer = IdlePipeCloser(pipeline.idle_read_fd, pipeline.idle_write_fd);

        // const gather_thread = std.Thread.spawn(.{}, gatherMainPosix, .{ fd, quit, &pipeline });
        //
        // PORT NOTE: `spawn` + `defer gather_thread.join()` becomes
        // `std::thread::scope`, which IS the join — including on a panic unwind,
        // which the reference's `defer` also covers. The scope is what lets the
        // gather stage borrow `&pipeline` without an `Arc`, matching the
        // reference's stack-local `Pipeline`.
        //
        // const gather_thread = std.Thread.spawn(
        //     .{},
        //     gatherMainPosix,
        //     .{ fd, quit, &pipeline },
        // ) catch |err| {
        //     // If we can't spawn a thread the process is already
        //     // doomed (every surface spawns several), so don't try
        //     // to limp along.
        //     log.err("read thread exiting, failed to spawn gather thread err={}", .{err});
        //     return;
        // };
        // defer gather_thread.join();
        //
        // PORT NOTE (task-14e-1 fix round 1, closes the log half of S211-F6):
        // `Builder::spawn_scoped` DOES return a `Result`. This site used to
        // discard it with `.expect("spawn io-gather")` and carried a note
        // claiming `std::thread::scope` has no fallible spawn — that claim was
        // wrong, and `S211-F6` ("gather-thread spawn failure panics instead of
        // logging and returning") is the ledger row that already said so. The
        // reference deliberately does NOT crash here: it logs and RETURNS,
        // letting the read thread exit while the rest of the process lives.
        // Both halves are restored below. The `return` leaves this closure,
        // which is the last statement of `threadMainPosix`, so it returns from
        // the read thread exactly as the reference's `return` does; nothing was
        // spawned, so there is nothing for the scope to join.
        let pipeline = &pipeline;
        std::thread::scope(|scope| {
            let gather_fd = fd;
            let gather_quit = quit;
            let mut builder = rustty_zigstd::thread::spawn_config();
            if cfg!(not(any(target_os = "macos", target_os = "ios"))) {
                // if (comptime !builtin.os.tag.isDarwin()) gather_thread.setName("io-gather");
                builder = builder.name("io-gather".to_string());
            }
            // (Test builds only: `tests::FAIL_GATHER_SPAWN` makes this spawn
            // fail for real, with a stack no thread can get.)
            #[cfg(test)]
            if tests::FAIL_GATHER_SPAWN.with(std::cell::Cell::get) {
                builder = builder.stack_size(usize::MAX / 2);
            }
            if let Err(err) = builder.spawn_scoped(scope, move || {
                Self::gatherMainPosix(gather_fd, gather_quit, pipeline);
            }) {
                log::error!(
                    target: LOG_SCOPE,
                    "read thread exiting, failed to spawn gather thread err={err:?}"
                );
                return;
            }

            // This thread is the parse stage. We consume batches in ring
            // order until the gather stage reports the stream is over and
            // the ring is drained.
            loop {
                // const batch: []const u8 = batch: { ... };
                //
                // INV-P1/P3: only the ring METADATA is read under the guard. The
                // slot INDEX and LENGTH are copied out and the guard is dropped
                // before the buffer is touched, and the buffer is reached only
                // through `slot_ptr` — never `(*bufs)[slot]`.
                let (slot, len) = {
                    let mut m = pipeline.mutex.lock().unwrap_or_else(|e| e.into_inner());
                    // while (pipeline.count == 0) { if (pipeline.done) return;
                    //     pipeline.batch_ready.wait(&pipeline.mutex); }
                    while m.count == 0 {
                        if m.done {
                            return;
                        }
                        m = pipeline
                            .batch_ready
                            .wait(m)
                            .unwrap_or_else(|e| e.into_inner());
                    }
                    // const slot = pipeline.tail;
                    // break :batch pipeline.bufs[slot][0..pipeline.lens[slot]];
                    (m.tail, m.lens[m.tail])
                };

                // The batch buffer is owned by this stage until we advance
                // the tail below, so it is safe to read outside the lock.
                // io.processOutput(batch);
                //
                // SAFETY (INV-P2): this stage owns `slot` from the take above to
                // the tail advance below; `len <= BUFFER_CAPACITY` was written by
                // the gather stage under the same mutex. `io` is the read
                // thread's sole `processOutput` toucher (the `ReadThreadArgs`
                // contract); its `renderer_state` is live for the pointer's
                // lifetime (joined in `threadExit` before teardown).
                unsafe {
                    let batch = core::slice::from_raw_parts(pipeline.slot_ptr(slot), len);
                    crate::Termio::processOutput(io, batch);
                }

                {
                    let wake = {
                        let mut m = pipeline.mutex.lock().unwrap_or_else(|e| e.into_inner());
                        // pipeline.tail = (pipeline.tail + 1) % buffer_count;
                        m.tail = (m.tail + 1) % Self::BUFFER_COUNT;
                        // pipeline.count -= 1;
                        m.count -= 1;
                        // const wake = pipeline.count == 0 and pipeline.bridging and
                        //     pipeline.idle_write_fd >= 0;
                        m.count == 0 && m.bridging && pipeline.idle_write_fd >= 0
                    };
                    // pipeline.slot_free.signal();
                    pipeline.slot_free.notify_one();

                    // We ran out of batches while the gather stage is
                    // bridging a refill gap: interrupt its poll so it
                    // delivers what it has instead of sleeping out the
                    // timeout while we sit idle.
                    // if (wake) { _ = posix.write(pipeline.idle_write_fd, "i") catch {}; }
                    if wake {
                        unsafe {
                            libc::write(
                                pipeline.idle_write_fd,
                                b"i".as_ptr().cast::<std::ffi::c_void>(),
                                1,
                            );
                        }
                    }
                }

                // Batch boundary: hand the renderer state mutex off if the
                // renderer is waiting. See renderer::State::lock_demand.
                //
                // PORT NOTE (D4, oracle d34b54e9b): the oracle inserts this at the
                // end of the parse-stage loop of exactly this pipeline
                // (`io.renderer_state.yieldToDemand()` after the batch's slot is
                // returned) — a LATER upstream commit than the two this task
                // ports. rustty landed it early against the then-current
                // single-loop shape; now that the pipeline exists it moves to the
                // position the oracle gives it, immediately after the slot is
                // returned. `processOutput` locks AND unlocks
                // `renderer_state.mutex` internally, so the mutex is NOT held
                // here, as `yield_to_demand` requires.
                //
                // SAFETY: `io` and its `renderer_state` are live for the read
                // thread's whole lifetime (the `ReadThreadArgs` contract, as for
                // the `processOutput` call above).
                unsafe {
                    (*(*io).renderer_state).yield_to_demand();
                }
            }
        });
    }

    /// The gather stage. This drains the pty into rotating buffers,
    /// bridging the kernel queue's refill gaps for saturated streams,
    /// and publishes each batch to the parse stage. This thread owns
    /// all fd monitoring, including the quit fd.
    //
    // fn gatherMainPosix(fd: posix.fd_t, quit: posix.fd_t, pipeline: *Pipeline) void {
    #[cfg(not(windows))]
    pub(super) fn gatherMainPosix(
        fd: std::os::fd::RawFd,
        quit: std::os::fd::RawFd,
        pipeline: &Pipeline,
    ) {
        // if (builtin.os.tag.isDarwin()) {
        //     internal_os.macos.pthread_setname_np(&"io-gather".*);
        //     setQosClass();
        // }
        #[cfg(any(target_os = "macos", target_os = "ios"))]
        unsafe {
            rustty_os::macos::pthread_setname_np(c"io-gather".as_ptr());
        }
        #[cfg(any(target_os = "macos", target_os = "ios"))]
        Self::setQosClass();

        // However we exit, tell the parse stage the stream is over so
        // it drains the ring and joins us.
        //
        // defer { pipeline.mutex.lock(); pipeline.done = true;
        //         pipeline.mutex.unlock(); pipeline.batch_ready.signal(); }
        //
        // PORT NOTE: a Drop guard, so this fires on EVERY exit path INCLUDING a
        // panic unwind. Without it the parse stage would wait on `batch_ready`
        // forever and `thread::scope` would never join (E1's "return early in
        // two new places" hazard, generalized).
        struct DoneSignal<'a>(&'a Pipeline);
        impl Drop for DoneSignal<'_> {
            fn drop(&mut self) {
                {
                    let mut m = self.0.mutex.lock().unwrap_or_else(|e| e.into_inner());
                    m.done = true;
                }
                self.0.batch_ready.notify_one();
            }
        }
        let _done = DoneSignal(pipeline);

        // The fds we poll: data on the pty, our quit notification,
        // and the parse stage's idle wake. The idle fd only
        // participates in bridge polls (the parse stage only writes
        // while we're bridging), so the outer poll slices it off.
        // var pollfds: [3]posix.pollfd = .{ ... };
        let mut pollfds: [libc::pollfd; 3] = [
            libc::pollfd {
                fd,
                events: libc::POLLIN,
                revents: 0,
            },
            libc::pollfd {
                fd: quit,
                events: libc::POLLIN,
                revents: 0,
            },
            libc::pollfd {
                fd: pipeline.idle_read_fd,
                events: libc::POLLIN,
                revents: 0,
            },
        ];

        // Set once an outer poll has reported the hangup together with
        // unread data and the loop went back to read it (UD19; see
        // `outer_poll_hup_exits`).
        let mut drained_after_hup = false;

        loop {
            // Claim the next free buffer. This blocks only when the
            // parse stage is a full ring behind, which is exactly when
            // we should stop reading and let the kernel queue exert
            // backpressure on the child.
            //
            // INV-P1/P3: the guard yields only the slot INDEX; the buffer is
            // reached below through `slot_ptr` alone.
            // const buf: *[buffer_capacity]u8 = buf: { ... &pipeline.bufs[pipeline.head] };
            let slot = {
                let mut m = pipeline.mutex.lock().unwrap_or_else(|e| e.into_inner());
                while m.count == Self::BUFFER_COUNT {
                    m = pipeline
                        .slot_free
                        .wait(m)
                        .unwrap_or_else(|e| e.into_inner());
                }
                m.head
            };
            let buf: *mut u8 = pipeline.slot_ptr(slot);

            // var total: usize = 0;
            let mut total: usize = 0;
            // var bridge_start: ?std.time.Instant = null;
            let mut bridge_start: Option<std::time::Instant> = None;
            // var spins: usize = 0;
            let mut spins: usize = 0;
            // var fatal = false;
            let mut fatal = false;

            // Fill the buffer from the pty. For a saturated stream the
            // kernel queue momentarily runs dry while the writer
            // refills it in parallel, so we bridge those gaps with
            // spin retries and a short poll instead of delivering a
            // tiny batch.
            // gather: while (total < buffer_capacity) {
            'gather: while total < Self::BUFFER_CAPACITY {
                // const n = posix.read(fd, buf[total..]) catch |err| switch (err) { ... };
                //
                // SAFETY (INV-P2): this stage owns `slot` until the publish
                // below, and `total < BUFFER_CAPACITY`, so the destination range
                // is inside this slot.
                let rc = unsafe {
                    libc::read(
                        fd,
                        buf.add(total).cast::<std::ffi::c_void>(),
                        Self::BUFFER_CAPACITY - total,
                    )
                };
                if rc < 0 {
                    let e = errno();
                    // (posix.read retries on EINTR.)
                    if e == libc::EINTR {
                        continue 'gather;
                    }

                    // error.WouldBlock
                    if e == libc::EAGAIN {
                        // Anything below the threshold is interactive.
                        // if (total < bridge_threshold) break :gather;
                        if total < Self::BRIDGE_THRESHOLD {
                            break 'gather;
                        }

                        // The stream is saturated, so we bridge the
                        // gap. First retry the read directly a bounded
                        // number of times, since the refill usually
                        // lands within microseconds.
                        // if (spins < bridge_spin_max) { spins += 1; continue :gather; }
                        if spins < Self::BRIDGE_SPIN_MAX {
                            spins += 1;
                            continue 'gather;
                        }

                        // Still dry, so we want to sleep in poll for
                        // the next refill, within our latency budget.
                        // const now = std.time.Instant.now() catch break :gather;
                        // if (bridge_start) |start| {
                        //     if (now.since(start) >= gather_budget_ns) break :gather;
                        // } else bridge_start = now;
                        let now = std::time::Instant::now();
                        match bridge_start {
                            Some(start) => {
                                if now.duration_since(start).as_nanos()
                                    >= u128::from(Self::GATHER_BUDGET_NS)
                                {
                                    break 'gather;
                                }
                            }
                            None => bridge_start = Some(now),
                        }

                        // Bridging a refill gap is only free while the
                        // parse stage is busy, since the wait hides
                        // behind parse time. Once the parser is idle,
                        // every microsecond we hold this batch is
                        // added straight to output latency, and for a
                        // request/response producer (a burst ending
                        // in a query, e.g. frame + cursor position
                        // report) the writer is blocked on a reply to
                        // data sitting in this buffer, so a poll here
                        // would always sleep its full timeout. Deliver
                        // now if the parser is idle, and otherwise arm
                        // the idle wake so it can interrupt our poll
                        // the moment that changes.
                        // (bb0ac4c72)
                        {
                            let mut m = pipeline.mutex.lock().unwrap_or_else(|e| e.into_inner());
                            if m.count == 0 {
                                break 'gather;
                            }
                            m.bridging = true;
                        }

                        // const r = posix.poll(&pollfds, bridge_poll_timeout_ms) catch { ... };
                        let r = unsafe {
                            libc::poll(
                                pollfds.as_mut_ptr(),
                                pollfds.len() as libc::nfds_t,
                                Self::BRIDGE_POLL_TIMEOUT_MS,
                            )
                        };
                        if r < 0 {
                            let pe = errno();
                            Self::clearBridging(pipeline);
                            if pe == libc::EINTR {
                                continue 'gather;
                            }
                            // log.warn("bridge poll failed err={}", .{poll_err});
                            log::warn!(
                                target: LOG_SCOPE,
                                "bridge poll failed err={}",
                                std::io::Error::from_raw_os_error(pe)
                            );
                            break 'gather;
                        }
                        // clearBridging(pipeline);
                        Self::clearBridging(pipeline);

                        // Quiet for a full timeout means the burst ended.
                        // if (r == 0) break :gather;
                        if r == 0 {
                            break 'gather;
                        }

                        // On a quit signal we deliver what we have and stop.
                        if pollfds[1].revents & libc::POLLIN != 0 {
                            // log.info("read thread got quit signal", .{});
                            log::info!(target: LOG_SCOPE, "read thread got quit signal");
                            fatal = true;
                            break 'gather;
                        }

                        // The parse stage went idle: drain the wake
                        // and deliver what we have. The pty may have
                        // data as well, but the next batch can pick
                        // that up; an idle parser means delivery must
                        // not wait.
                        if pollfds[2].revents & libc::POLLIN != 0 {
                            // var trash: [16]u8 = undefined;
                            let mut trash = [0u8; 16];
                            loop {
                                let drained = unsafe {
                                    libc::read(
                                        pipeline.idle_read_fd,
                                        trash.as_mut_ptr().cast::<std::ffi::c_void>(),
                                        trash.len(),
                                    )
                                };
                                if drained < 0 || (drained as usize) < trash.len() {
                                    break;
                                }
                            }
                            break 'gather;
                        }

                        // HUP without IN means no more data is
                        // coming. Deliver and let the outer poll
                        // decide what to do.
                        if pollfds[0].revents & libc::POLLIN == 0 {
                            break 'gather;
                        }

                        continue 'gather;
                    }

                    // The pty is closed. We're probably gracefully
                    // shutting down.
                    // error.NotOpenForReading, error.InputOutput =>
                    if e == libc::EBADF || e == libc::EIO {
                        // log.info("io gather exiting", .{});
                        log::info!(target: LOG_SCOPE, "io gather exiting");
                        fatal = true;
                        break 'gather;
                    }

                    // else => { log.err("io gather error err={}", .{err}); unreachable; }
                    log::error!(
                        target: LOG_SCOPE,
                        "io gather error err={}",
                        std::io::Error::from_raw_os_error(e)
                    );
                    unreachable!("io gather error");
                }
                let n = rc as usize;

                // This happens on macOS instead of WouldBlock when the
                // child process dies. Deliver what we have and let the
                // outer poll detect HUP.
                // if (n == 0) break :gather;
                if n == 0 {
                    break 'gather;
                }

                // total += n;
                total += n;

                // Each refill gap gets a fresh spin budget.
                // spins = 0;
                spins = 0;
            }

            // Publish the batch (if any) to the parse stage and rotate
            // to the next buffer.
            // if (total > 0) { ... }
            if total > 0 {
                {
                    let mut m = pipeline.mutex.lock().unwrap_or_else(|e| e.into_inner());
                    m.lens[slot] = total;
                    m.head = (m.head + 1) % Self::BUFFER_COUNT;
                    m.count += 1;
                }
                pipeline.batch_ready.notify_one();
            }

            // if (fatal) return;
            if fatal {
                return;
            }

            // A full buffer means the stream is still hot, so go
            // claim the next buffer without an intervening poll.
            // if (total == buffer_capacity) continue;
            if total == Self::BUFFER_CAPACITY {
                continue;
            }

            // Wait for data. The idle fd is sliced off: the parse
            // stage only writes to it while we're bridging.
            // _ = posix.poll(pollfds[0..2], -1) catch |err| { ... return; };
            //
            // (Test builds only: `tests::before_outer_poll` lets a test change
            // the fd's state at this exact point, UD19.)
            #[cfg(test)]
            tests::before_outer_poll(fd);
            let rc = unsafe { libc::poll(pollfds.as_mut_ptr(), 2, -1) };
            if rc < 0 {
                if errno() == libc::EINTR {
                    continue;
                }
                // log.warn("poll failed on read thread, exiting early err={}", .{err});
                log::warn!(
                    target: LOG_SCOPE,
                    "poll failed on read thread, exiting early err={}",
                    std::io::Error::last_os_error()
                );
                return;
            }

            // If our quit fd is set, we're done.
            // if (pollfds[1].revents & posix.POLL.IN != 0) { ... return; }
            if pollfds[1].revents & libc::POLLIN != 0 {
                // log.info("read thread got quit signal", .{});
                log::info!(target: LOG_SCOPE, "read thread got quit signal");
                return;
            }

            // If our pty fd is closed, then we're also done with our
            // read thread.
            // if (pollfds[0].revents & posix.POLL.HUP != 0) { ... return; }
            //
            // PORT NOTE: reference defect (R13): see `outer_poll_hup_exits`.
            if Self::outer_poll_hup_exits(pollfds[0].revents, &mut drained_after_hup) {
                // log.info("pty fd closed, read thread exiting", .{});
                log::info!(target: LOG_SCOPE, "pty fd closed, read thread exiting");
                return;
            }
        }
    }

    /// Whether the gather stage's outer poll ends the read thread on a
    /// hangup.
    ///
    /// PORT NOTE: reference defect (R13). The reference returns as soon as
    /// `POLLHUP` is set (`Exec.zig:1727-1730`), without looking at `POLLIN`.
    /// On Linux a pty master reports both in one word (`0x11`, measured on
    /// the port in phase 2) when the child writes and exits while this stage
    /// is parked in the poll, and the bytes still queued on the pty are never
    /// read: the last output of a command that exits at once can be missing
    /// from the terminal. The same function's bridge poll states the intent,
    /// "HUP without IN means no more data is coming" (`Exec.zig:1657-1662`).
    /// So a hangup with `POLLIN` sends the loop back to read once more; the
    /// next outer poll that reports the hangup ends it, whatever `POLLIN`
    /// says. That second rule matters: once the data is read the fd still
    /// reports `POLLIN | POLLHUP` where a read returns 0 (measured on macOS
    /// for a pty master and for a pipe), and on Linux the read after the
    /// data fails with EIO, which the gather loop already treats as the end.
    /// A hangup without `POLLIN` ends the thread at once, as in the
    /// reference. `.port/parity-closure/UPSTREAM-DEFECTS.md` UD19.
    #[cfg(not(windows))]
    pub(super) fn outer_poll_hup_exits(
        revents: libc::c_short,
        drained_after_hup: &mut bool,
    ) -> bool {
        if revents & libc::POLLHUP == 0 {
            return false;
        }
        if revents & libc::POLLIN != 0 && !*drained_after_hup {
            *drained_after_hup = true;
            return false;
        }
        true
    }

    /// Clears the bridging flag armed before a bridge poll, closing
    /// the window in which the parse stage writes idle wakes.
    //
    // fn clearBridging(pipeline: *Pipeline) void { ... }
    #[cfg(not(windows))]
    fn clearBridging(pipeline: &Pipeline) {
        let mut m = pipeline.mutex.lock().unwrap_or_else(|e| e.into_inner());
        m.bridging = false;
    }

    /// Set `fd` to non-blocking. Returns false if it could not be done.
    //
    // PORT NOTE: the reference factors this out of `threadMainPosix` as part of
    // 2f0e6659d because the failure is now FATAL (the pipeline cannot run on a
    // blocking fd), where the pre-pipeline loop only logged a perf warning.
    #[cfg(not(windows))]
    fn setNonblock(fd: std::os::fd::RawFd) -> bool {
        let flags = unsafe { libc::fcntl(fd, libc::F_GETFL, 0) };
        if flags == -1 {
            // log.warn("read thread failed to get flags err=E{s}", .{@tagName(posix.errno(-1))});
            //
            // PORT NOTE: the port's earlier Zig comment here reproduced an OLDER
            // upstream shape (`err={}` plus a "this isn't a fatal error" second
            // line); at pin 20abdb50a the reference renders the ERRNO NAME as
            // `err=E{s}` and has no second line. The live call follows the pin.
            log::warn!(
                target: LOG_SCOPE,
                "read thread failed to get flags err=E{}",
                errno_tag_name(errno())
            );
            return false;
        }
        // switch (posix.errno(posix.system.fcntl(fd, F.SETFL, flags | NONBLOCK))) {
        //     .SUCCESS => {},
        //     else => |e| {
        //         log.warn("read thread failed to set flags err=E{s}", .{@tagName(e)});
        //         return false;
        //     },
        // }
        // return true;
        if unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } == -1 {
            log::warn!(
                target: LOG_SCOPE,
                "read thread failed to set flags err=E{}",
                errno_tag_name(errno())
            );
            return false;
        }
        true
    }

    /// Sets the QoS class of the calling thread for the read pipeline
    /// (macOS only). Both pipeline threads feed content the user is
    /// actively watching, and at default QoS the scheduler may place
    /// them on efficiency cores with wakeup latencies that are large
    /// compared to the ~10us cadence of the pty producer/consumer
    /// dance. Measured on an M4 Max, this results in a 15% throughput
    /// difference (on the change, not 15% total).
    //
    // fn setQosClass() void {
    //     internal_os.macos.setQosClass(.user_initiated) catch |err| {
    //         log.warn("error setting QoS class err={}", .{err});
    //     };
    // }
    //
    // PORT NOTE (2f0e6659d, WS10 T8): this helper is the ONE hunk of the pty
    // pipeline commit that is separable from the pipeline itself — it applies
    // verbatim to the single read thread rustty has today, which is why
    // `E-os-termio-platform.md` row E4 calls it out on its own. The reference
    // calls it from BOTH `threadMainPosix` and the new `gatherMainPosix`; only
    // the former exists here, so only the former call site lands. When the
    // pipeline lands, `gatherMainPosix` calls this same fn.
    //
    // The reference's `catch |err| log.warn(...)` is LOAD-BEARING, not
    // decoration: `setQosClass` returns `SetQosClassError::ThreadIncompatible`
    // on EPERM, and swallowing it is what keeps a QoS failure from killing the
    // read thread. The port swallows it identically (the `log.warn` itself is
    // the tree-wide log-as-comment convention).
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    #[allow(non_snake_case)]
    fn setQosClass() {
        if let Err(err) = rustty_os::macos::setQosClass(rustty_os::macos::QosClass::user_initiated)
        {
            // log.warn("error setting QoS class err={}", .{err});
            log::warn!(target: LOG_SCOPE, "error setting QoS class err={err:?}");
        }
    }

    /// `threadExit`'s Windows interrupt: cancel the read thread's blocking
    /// `ReadFile` so it sees the quit byte (`Exec.zig:219-227`).
    ///
    /// PORT NOTE: reference defect (R13), UPSTREAM-DEFECTS.md UD326 (N341). The
    /// reference cancels once and ignores `ERROR_NOT_FOUND`. `CancelIoEx`
    /// cancels only I/O that is pending when it is called; if the read thread is
    /// between two reads (in `processOutput`), nothing is cancelled, the thread
    /// re-enters `ReadFile`, and `threadExit`'s `join` blocks until the pseudo
    /// console writes again, possibly forever. MEASURED on Windows (task W-1b,
    /// `windows_readthread_quit_survives_a_cancel_between_reads`). The port
    /// repeats the cancel while the thread has not finished (`ERROR_NOT_FOUND`
    /// means "not reading yet", so it tries again), and stops on any other
    /// error after the reference's warning, as the reference does.
    #[cfg(windows)]
    pub(crate) fn interruptWindows(
        read_thread_fd: rustty_os::windows::HANDLE,
        finished: impl Fn() -> bool,
    ) {
        const ERROR_NOT_FOUND: u32 = 1168;
        while !finished() {
            // if (windows.kernel32.CancelIoEx(exec.read_thread_fd, null) == 0) {
            if unsafe {
                rustty_os::windows::exp::kernel32::CancelIoEx(read_thread_fd, std::ptr::null_mut())
            } == 0
            {
                // switch (GetLastError()) {
                //     .NOT_FOUND => {},
                //     else => |err| log.warn("error interrupting read thread err={}", .{err}),
                // }
                let err = unsafe { rustty_os::windows::kernel32::GetLastError() };
                if err != ERROR_NOT_FOUND {
                    log::warn!(target: LOG_SCOPE, "error interrupting read thread err={err}");
                    return;
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    }

    // The reference body, preserved verbatim (it is PORTED below — this block is
    // the R3 verbatim record, not a deferral marker):
    //
    // fn threadMainWindows(fd: posix.fd_t, io: *termio.Termio, quit: posix.fd_t) void {
    //     // Always close our end of the pipe when we exit.
    //     defer posix.close(quit);
    //
    //     // Setup our crash metadata
    //     crash.sentry.thread_state = .{
    //         .type = .io,
    //         .surface = io.surface_mailbox.surface,
    //     };
    //     defer crash.sentry.thread_state = null;
    //
    //     var buf: [1024]u8 = undefined;
    //     while (true) {
    //         while (true) {
    //             var n: windows.DWORD = 0;
    //             if (windows.kernel32.ReadFile(fd, &buf, buf.len, &n, null) == 0) {
    //                 const err = windows.kernel32.GetLastError();
    //                 switch (err) {
    //                     // Check for a quit signal
    //                     .OPERATION_ABORTED => break,
    //
    //                     else => {
    //                         log.err("io reader error err={}", .{err});
    //                         unreachable;
    //                     },
    //                 }
    //             }
    //
    //             @call(.always_inline, termio.Termio.processOutput, .{ io, buf[0..n] });
    //
    //             // See threadMainPosix: hand the renderer state mutex
    //             // off if the renderer is waiting, since this loop
    //             // would otherwise starve it under heavy output.
    //             io.renderer_state.yieldToDemand();
    //         }
    //
    //         var quit_bytes: windows.DWORD = 0;
    //         if (windows.exp.kernel32.PeekNamedPipe(quit, null, 0, null, &quit_bytes, null) == 0) {
    //             const err = windows.kernel32.GetLastError();
    //             log.err("quit pipe reader error err={}", .{err});
    //             unreachable;
    //         }
    //
    //         if (quit_bytes > 0) {
    //             log.info("read thread got quit signal", .{});
    //             return;
    //         }
    //     }
    // }
    // PORTED (WS20). It cannot be cross-checked from macOS (the `highway-sys`
    // C++ build needs an MSVC sysroot); task W-1 built and ran it on a Windows
    // host (`windows_readthread_sets_io_crash_thread_state`).
    #[cfg(windows)]
    pub(super) fn threadMainWindows<M: rustty_apprt_api::surface::AppMailbox>(
        fd: rustty_os::windows::HANDLE,
        io: *mut crate::Termio<M>,
        quit: rustty_os::windows::HANDLE,
    ) {
        use rustty_os::windows as win;

        // Always close our end of the pipe when we exit.
        // defer _ = posix.system.close(quit);
        //
        // PORT NOTE: reference defect (R13), UPSTREAM-DEFECTS.md UD325. `quit` is a
        // Win32 HANDLE (the reference's `posix.fd_t` is a HANDLE on Windows), but
        // the reference closes it with `posix.system.close`, the C runtime's
        // `close` (Ghostty links libc), which takes a CRT file descriptor and
        // fast-fails the process on a HANDLE (0xC0000409, MEASURED for the CRT
        // fd functions on Windows, task W-1b). The port closes it with
        // `CloseHandle`, the Win32 call for the HANDLE the reference holds.
        //
        // CORRECTION (task W-1b, review W-1 F1): this note used to say the
        // close is "`CloseHandle` — the same call the posix arm's `close(2)` maps
        // to for this handle class"; the reference's call is the CRT's `close`,
        // which does not map to `CloseHandle`.
        struct CloseOnExit(win::HANDLE);
        impl Drop for CloseOnExit {
            fn drop(&mut self) {
                let _ = win::CloseHandle(self.0);
            }
        }
        let _quit_guard = CloseOnExit(quit);

        // Setup our crash metadata
        // crash.sentry.thread_state = .{ .type = .io, .surface = io.surface_mailbox.surface };
        // defer crash.sentry.thread_state = null;
        //
        // (S211-F2: the same guard as the posix arm.)
        let _crash_thread_state = unsafe { crate::IoCrashThreadState::set(io) };

        // var buf: [1024]u8 = undefined;
        let mut buf = [0u8; 1024];
        loop {
            loop {
                // var n: windows.DWORD = 0;
                let mut n: win::DWORD = 0;
                // if (windows.exp.kernel32.ReadFile(fd, &buf, buf.len, &n, null) == windows.FALSE) {
                if unsafe {
                    win::exp::kernel32::ReadFile(
                        fd,
                        buf.as_mut_ptr().cast::<std::ffi::c_void>(),
                        buf.len() as win::DWORD,
                        &mut n,
                        std::ptr::null_mut(),
                    )
                } == win::FALSE
                {
                    // const err = windows.GetLastError();
                    let err = unsafe { win::kernel32::GetLastError() };
                    // switch (err) { .OPERATION_ABORTED => break, else => { ... unreachable; } }
                    const ERROR_OPERATION_ABORTED: win::DWORD = 995;
                    if err == ERROR_OPERATION_ABORTED {
                        // Check for a quit signal
                        break;
                    }
                    // log.err("io reader error err={}", .{err});
                    log::error!(target: LOG_SCOPE, "io reader error err={err}");
                    unreachable!("io reader error");
                }

                // @call(.always_inline, termio.Termio.processOutput, .{ io, buf[0..n] });
                //
                // SAFETY: `io` is the read thread's sole `processOutput` toucher
                // (the `ReadThreadArgs` contract); its `renderer_state` is live for
                // the pointer's lifetime (joined in `threadExit` before teardown).
                unsafe {
                    crate::Termio::processOutput(io, &buf[..n as usize]);
                }

                // See threadMainPosix: hand the renderer state mutex
                // off if the renderer is waiting, since this loop
                // would otherwise starve it under heavy output.
                // io.renderer_state.yieldToDemand();
                unsafe {
                    (*(*io).renderer_state).yield_to_demand();
                }
            }

            // var quit_bytes: windows.DWORD = 0;
            let mut quit_bytes: win::DWORD = 0;
            // if (windows.exp.kernel32.PeekNamedPipe(quit, null, 0, null, &quit_bytes, null) == windows.FALSE) {
            if unsafe {
                win::exp::kernel32::PeekNamedPipe(
                    quit,
                    std::ptr::null_mut(),
                    0,
                    std::ptr::null_mut(),
                    &mut quit_bytes,
                    std::ptr::null_mut(),
                )
            } == win::FALSE
            {
                // const err = windows.GetLastError();
                // log.err("quit pipe reader error err={}", .{err});
                let err = unsafe { win::kernel32::GetLastError() };
                log::error!(target: LOG_SCOPE, "quit pipe reader error err={err}");
                unreachable!("quit pipe reader error");
            }

            // if (quit_bytes > 0) { log.info("read thread got quit signal", .{}); return; }
            if quit_bytes > 0 {
                log::info!(target: LOG_SCOPE, "read thread got quit signal");
                return;
            }
        }
    }
}
