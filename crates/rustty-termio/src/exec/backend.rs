//! PORTS: ghostty src/termio/Exec.zig — the `Exec` backend's methods (`init`/`deinit`, `initTerminal`,
//! `threadEnter`/`threadExit`, `focusGained`, `resize`, `processExit*`,
//! `flatpakExit`, `termiosTimer`, `queueWrite`, `ttyWrite`, `getForegroundPid`,
//! `getTtyName`).

use super::*;

impl Exec {
    /// Initialize the exec state. This will NOT start it, this only sets
    /// up the internal state necessary to start it later.
    //
    // pub fn init(
    //     alloc: Allocator,
    //     cfg: Config,
    // ) !Exec {
    pub fn init(alloc: Allocator, cfg: Config) -> Result<Exec, InitError> {
        // var subprocess = try Subprocess.init(alloc, cfg);
        // errdefer subprocess.deinit();
        // (PORT NOTE: on the error path the partially-built Subprocess drops.)
        let subprocess = Subprocess::init(alloc, cfg)?;

        // return .{ .subprocess = subprocess };
        Ok(Exec {
            subprocess,
            pty_ready: core::sync::atomic::AtomicBool::new(false),
        })
    }

    // pub fn deinit(self: *Exec) void {
    pub fn deinit(&mut self) {
        // (UD367: the pty is closed below; no thread queries it any more.)
        self.pty_ready
            .store(false, core::sync::atomic::Ordering::Relaxed);
        self.subprocess.deinit();
    }

    /// Call to initialize the terminal state as necessary for this backend.
    /// This is called before any termio begins. This should not be called
    /// after termio begins because it may put the internal terminal state
    /// into a bad state.
    //
    // pub fn initTerminal(self: *Exec, term: *terminal.Terminal) void {
    pub fn initTerminal(&mut self, term: &mut Terminal) {
        // If we have an initial pwd requested by the subprocess, then we
        // set that on the terminal now. This allows rapidly initializing
        // new surfaces to use the proper pwd.
        // if (self.subprocess.cwd) |cwd| term.setPwd(cwd) catch |err| {
        //     log.warn("error setting initial pwd err={}", .{err});
        // };
        if let Some(cwd) = &self.subprocess.cwd {
            if let Err(err) = term.set_pwd(cwd) {
                // log.warn("error setting initial pwd err={}", .{err});
                log::warn!(target: LOG_SCOPE, "error setting initial pwd err={err:?}");
            }
        }

        // Setup our initial grid/screen size from the terminal. This
        // can't fail because the pty should not exist at this point.
        // self.resize(.{
        //     .columns = term.cols,
        //     .rows = term.rows,
        // }, .{
        //     .width = term.width_px,
        //     .height = term.height_px,
        // }) catch unreachable;
        //
        // SAFETY: `self` is a live `&mut Exec` (N449: `resize` is raw-self).
        unsafe {
            Self::resize(
                self,
                GridSize {
                    columns: term.cols,
                    rows: term.rows,
                },
                ScreenSize {
                    width: term.width_px,
                    height: term.height_px,
                },
            )
        }
        .unwrap();
    }

    // [task-18 STALE HEADER STRUCK (S211-F7): this said "DEFERRED (D6 xev
    // event-loop wiring, 0 tests — R1-safe)". `threadEnter` is FULLY IMPLEMENTED
    // at :441 below — it starts the subprocess and wires the process watcher,
    // write stream, termios timer and read thread over the `std::thread` +
    // `xev_seam` substitute for libxev.] The reference body, preserved verbatim:
    //
    // pub fn threadEnter(
    //     self: *Exec,
    //     alloc: Allocator,
    //     io: *termio.Termio,
    //     td: *termio.Termio.ThreadData,
    // ) !void {
    //     // Start our subprocess
    //     const pty_fds = self.subprocess.start(alloc) catch |err| {
    //         // If we specifically got this error then we are in the forked
    //         // process and our child failed to execute. If we DIDN'T
    //         // get this specific error then we're in the parent and
    //         // we need to bubble it up.
    //         if (err != error.ExecFailedInChild) return err;
    //
    //         // We're in the child. Nothing more we can do but abnormal exit.
    //         // The Command will output some additional information.
    //         posix.exit(1);
    //     };
    //     errdefer self.subprocess.stop();
    //
    //     // Watcher to detect subprocess exit
    //     var process: ?xev.Process = if (self.subprocess.process) |v| switch (v) {
    //         .fork_exec => |cmd| try xev.Process.init(
    //             cmd.pid orelse return error.ProcessNoPid,
    //         ),
    //
    //         // If we're executing via Flatpak then we can't do
    //         // traditional process watching (its implemented
    //         // as a special case in os/flatpak.zig) since the
    //         // command is on the host.
    //         .flatpak => null,
    //     } else return error.ProcessNotStarted;
    //     errdefer if (process) |*p| p.deinit();
    //
    //     // Track our process start time for abnormal exits
    //     const process_start = try std.time.Instant.now();
    //
    //     // Create our pipe that we'll use to kill our read thread.
    //     // pipe[0] is the read end, pipe[1] is the write end.
    //     const pipe = try internal_os.pipe();
    //     errdefer posix.close(pipe[0]);
    //     errdefer posix.close(pipe[1]);
    //
    //     // Setup our stream so that we can write.
    //     var stream = xev.Stream.initFd(pty_fds.write);
    //     errdefer stream.deinit();
    //
    //     // Start our timer to read termios state changes. This is used
    //     // to detect things such as when password input is being done
    //     // so we can render the terminal in a different way.
    //     var termios_timer = try xev.Timer.init();
    //     errdefer termios_timer.deinit();
    //
    //     // Start our read thread
    //     const read_thread = try std.Thread.spawn(
    //         .{},
    //         if (builtin.os.tag == .windows) ReadThread.threadMainWindows else ReadThread.threadMainPosix,
    //         .{ pty_fds.read, io, pipe[0] },
    //     );
    //     read_thread.setName("io-reader") catch {};
    //
    //     // Setup our threadata backend state to be our own
    //     td.backend = .{ .exec = .{
    //         .start = process_start,
    //         .write_stream = stream,
    //         .process = process,
    //         .read_thread = read_thread,
    //         .read_thread_pipe = pipe[1],
    //         .read_thread_fd = pty_fds.read,
    //         .termios_timer = termios_timer,
    //     } };
    //
    //     // Start our process watcher. If we have an xev.Process use it.
    //     if (process) |*p| p.wait(
    //         td.loop,
    //         &td.backend.exec.process_wait_c,
    //         termio.Termio.ThreadData,
    //         td,
    //         processExit,
    //     ) else if (comptime build_config.flatpak) flatpak: {
    //         switch (self.subprocess.process orelse break :flatpak) {
    //             // If we're in flatpak and we have a flatpak command
    //             // then we can run the special flatpak logic for watching.
    //             .flatpak => |*c| c.waitXev(
    //                 td.loop,
    //                 &td.backend.exec.flatpak_wait_c,
    //                 termio.Termio.ThreadData,
    //                 td,
    //                 flatpakExit,
    //             ),
    //
    //             .fork_exec => {},
    //         }
    //     }
    //
    //     // Start our termios timer. We don't support this on Windows.
    //     // Fundamentally, we could support this on Windows so we're just
    //     // waiting for someone to implement it.
    //     if (comptime builtin.os.tag != .windows) {
    //         termios_timer.run(
    //             td.loop,
    //             &td.backend.exec.termios_timer_c,
    //             TERMIOS_POLL_MS,
    //             termio.Termio.ThreadData,
    //             td,
    //             termiosTimer,
    //         );
    //     }
    // }
    //
    // PORT NOTE (N449, P7-W7 T5d): `self_` is the raw `self: *Exec` (see
    // `Backend`'s note). `subprocess.start` still takes `&mut Subprocess`: it
    // runs before the read thread exists, and it is what publishes the pty.
    //
    // PORT NOTE ([ADV-FIX F2], xev Milestone B): `io` is a RAW `*mut Termio<M>`
    // (the reference `io: *termio.Termio`) — it is handed to the spawned read
    // thread, which aliases `*io` concurrently under the renderer mutex; a `&mut`
    // would be a whole-call protected `Unique` retag. `td` stays `&mut` for
    // Milestone B; a raw `td_ptr` is derived once for the loop-stored completion
    // pointers + userdata (the loop re-enters them across ticks — never a
    // long-lived `&mut`). The full frame-pinned-local raw threading of `td` lands
    // with `Thread::threadMain` in B4.
    ///
    /// # Safety
    /// `self_` must point at a live `Exec`; `io` must be a valid `Termio<M>`
    /// pointer that outlives the read thread; `td` must be frame-pinned for
    /// the loop's life (see the blueprint §3).
    pub unsafe fn threadEnter<M: rustty_apprt_api::surface::AppMailbox + 'static>(
        self_: *mut Exec,
        alloc: Allocator,
        io: *mut crate::Termio<M>,
        td: &mut crate::termio_impl::ThreadData<M>,
    ) -> Result<(), crate::Error> {
        // A raw pointer to the frame-pinned `td` for the loop-stored completion
        // pointers / userdata (never a long-lived `&mut` — the loop re-enters
        // these across ticks; `rust-mut-args-strongly-protected`).
        let td_ptr: *mut crate::termio_impl::ThreadData<M> = td;

        // Start our subprocess
        // const pty_fds = self.subprocess.start(alloc) catch |err| {
        //     // If we specifically got this error then we are in the forked
        //     // process and our child failed to execute. If we DIDN'T
        //     // get this specific error then we're in the parent and
        //     // we need to bubble it up.
        //     if (err != error.ExecFailedInChild) return err;
        //     // We're in the child. Nothing more we can do but abnormal exit.
        //     // The Command will output some additional information.
        //     posix.exit(1);
        // };
        //
        // (PORT NOTE: the `error.ExecFailedInChild` -> `posix.exit(1)` child
        // branch is handled inside `Subprocess::start` — the one place `in_child`
        // is known, since the opaque `crate::Error` cannot carry the discriminant
        // here; see start's PORT NOTE. Every error that reaches here is a parent
        // error and bubbles up.)
        let pty_fds = match unsafe { (*self_).subprocess.start(alloc) } {
            Ok(fds) => fds,
            Err(e) => {
                note_thread_enter_failure!("subprocess.start", e);
                return Err(e);
            }
        };
        // PORT NOTE: reference defect (R13), UD367. `start` has returned, so
        // its `&mut Subprocess` has ended and the pty is fully written (the
        // slave already closed); publish it to the main thread's
        // process-info accessors (see `Exec::pty_ready`).
        unsafe {
            (*self_)
                .pty_ready
                .store(true, core::sync::atomic::Ordering::Release)
        };
        // errdefer self.subprocess.stop();
        // (PORT NOTE: no `errdefer`; each fallible step below runs the accumulated
        // cleanup explicitly, in reverse registration order, before returning.)

        // Watcher to detect subprocess exit
        // var process: ?xev.Process = if (self.subprocess.process) |v| switch (v) {
        //     .fork_exec => |cmd| try xev.Process.init(cmd.pid orelse return error.ProcessNoPid),
        //     .flatpak => null,
        // } else return error.ProcessNotStarted;
        //
        // (PORT NOTE: copy the pid out from behind the `&self.subprocess.process`
        // borrow first, so the error arms can call `&mut self.subprocess.stop()`
        // — the `errdefer self.subprocess.stop()` — without a borrow conflict.)
        // (`posix.pid_t`: a HANDLE on Windows, the `Command.pid` type.)
        #[cfg(not(windows))]
        type ChildPid = libc::pid_t;
        #[cfg(windows)]
        type ChildPid = rustty_os::windows::HANDLE;
        enum ProcessWhich {
            NotStarted,
            Flatpak,
            ForkExec(Option<ChildPid>),
        }
        let which = match unsafe { &(*self_).subprocess.process } {
            None => ProcessWhich::NotStarted,
            Some(Process::Flatpak(_)) => ProcessWhich::Flatpak,
            Some(Process::ForkExec(cmd)) => ProcessWhich::ForkExec(cmd.pid),
        };
        let mut process: Option<xev::Process> = match which {
            // } else return error.ProcessNotStarted;
            ProcessWhich::NotStarted => {
                note_thread_enter_failure!("threadEnter", "ProcessNotStarted");
                // WS19 T17a [R1-DIVERGENCE]: drain the pty master BEFORE stop() — the
                // io-reader thread does not exist on this arm, so without this the
                // SIGHUP'd child can wedge unreapably (`?Es`). See
                // `drain_pty_master_once`.
                #[cfg(not(windows))]
                let _ = drain_pty_master_once(pty_fds.read);
                unsafe { Subprocess::stop(&raw mut (*self_).subprocess) };
                return Err(crate::Error);
            }
            // .flatpak => null (traditional process watching not possible)
            ProcessWhich::Flatpak => None,
            // .fork_exec => |cmd| try xev.Process.init(cmd.pid orelse return error.ProcessNoPid)
            ProcessWhich::ForkExec(None) => {
                // return error.ProcessNoPid (errdefer stop fires)
                note_thread_enter_failure!("threadEnter", "ProcessNoPid");
                // WS19 T17a [R1-DIVERGENCE]: drain the pty master BEFORE stop() — the
                // io-reader thread does not exist on this arm, so without this the
                // SIGHUP'd child can wedge unreapably (`?Es`). See
                // `drain_pty_master_once`.
                #[cfg(not(windows))]
                let _ = drain_pty_master_once(pty_fds.read);
                unsafe { Subprocess::stop(&raw mut (*self_).subprocess) };
                return Err(crate::Error);
            }
            ProcessWhich::ForkExec(Some(pid)) => match xev::Process::init(pid) {
                Ok(p) => Some(p),
                Err(_e) => {
                    note_thread_enter_failure!("threadEnter: xev.Process.init", _e);
                    // WS19 T17a [R1-DIVERGENCE]: drain the pty master BEFORE stop() — the
                    // io-reader thread does not exist on this arm, so without this the
                    // SIGHUP'd child can wedge unreapably (`?Es`). See
                    // `drain_pty_master_once`.
                    #[cfg(not(windows))]
                    let _ = drain_pty_master_once(pty_fds.read);
                    unsafe { Subprocess::stop(&raw mut (*self_).subprocess) };
                    return Err(crate::Error);
                }
            },
        };
        // errdefer if (process) |*p| p.deinit();

        // Track our process start time for abnormal exits
        // const process_start = try std.time.Instant.now();
        let process_start = std::time::Instant::now();

        // Create our pipe that we'll use to kill our read thread.
        // pipe[0] is the read end, pipe[1] is the write end.
        // const pipe = try internal_os.pipe();
        let pipe = match rustty_os::pipe::pipe() {
            Ok(p) => p,
            Err(_e) => {
                note_thread_enter_failure!("threadEnter: internal_os.pipe", _e);
                // errdefer if (process) |*p| p.deinit(); errdefer self.subprocess.stop();
                if let Some(p) = &mut process {
                    p.deinit();
                }
                // WS19 T17a [R1-DIVERGENCE]: drain the pty master BEFORE stop() — the
                // io-reader thread does not exist on this arm, so without this the
                // SIGHUP'd child can wedge unreapably (`?Es`). See
                // `drain_pty_master_once`.
                #[cfg(not(windows))]
                let _ = drain_pty_master_once(pty_fds.read);
                unsafe { Subprocess::stop(&raw mut (*self_).subprocess) };
                return Err(crate::Error);
            }
        };
        // errdefer posix.close(pipe[0]);
        // errdefer posix.close(pipe[1]);

        // Setup our stream so that we can write.
        // var stream = xev.Stream.initFd(pty_fds.write);
        let stream = xev::Stream::init_fd(pty_fds.write);
        // errdefer stream.deinit();

        // Start our timer to read termios state changes. This is used
        // to detect things such as when password input is being done
        // so we can render the terminal in a different way.
        // var termios_timer = try xev.Timer.init();
        let termios_timer = match xev::Timer::init() {
            Ok(t) => t,
            Err(_e) => {
                note_thread_enter_failure!("threadEnter: xev.Timer.init", _e);
                // errdefer (reverse): stream.deinit(); close pipe[1]; close pipe[0];
                //          process.deinit(); self.subprocess.stop();
                stream.deinit();
                unsafe {
                    close_fd(pipe[1]);
                    close_fd(pipe[0]);
                }
                if let Some(p) = &mut process {
                    p.deinit();
                }
                // WS19 T17a [R1-DIVERGENCE]: drain the pty master BEFORE stop() — the
                // io-reader thread does not exist on this arm, so without this the
                // SIGHUP'd child can wedge unreapably (`?Es`). See
                // `drain_pty_master_once`.
                #[cfg(not(windows))]
                let _ = drain_pty_master_once(pty_fds.read);
                unsafe { Subprocess::stop(&raw mut (*self_).subprocess) };
                return Err(crate::Error);
            }
        };
        // errdefer termios_timer.deinit();

        // Start our read thread
        // const read_thread = try std.Thread.spawn(
        //     .{},
        //     if (builtin.os.tag == .windows) ReadThread.threadMainWindows else ReadThread.threadMainPosix,
        //     .{ pty_fds.read, io, pipe[0] },
        // );
        // read_thread.setName("io-reader") catch {};
        //
        // (PORT NOTE: `io: *mut Termio<M>` is `!Send`, so the spawn captures it behind
        // the `ReadThreadArgs` Send wrapper — the `IoThreadArgs` precedent
        // (surface.rs), whose sole toucher of `*io` for `processOutput` is this
        // read thread, between spawn and `threadExit`'s `join()`; blueprint §3.)
        let read_args = ReadThreadArgs::<M>(pty_fds.read, io, pipe[0]);
        let read_thread_builder =
            rustty_zigstd::thread::spawn_config().name("io-reader".to_string());
        // (Test builds only: `tests::FAIL_READ_THREAD_SPAWN` makes this spawn
        // fail for real, with a stack no thread can get, to drive the error arm.)
        #[cfg(test)]
        let read_thread_builder = if tests::FAIL_READ_THREAD_SPAWN.with(std::cell::Cell::get) {
            read_thread_builder.stack_size(usize::MAX / 2)
        } else {
            read_thread_builder
        };
        let read_thread = match read_thread_builder.spawn(move || {
            let args = read_args;
            // SAFETY: `args.1` (`*mut Termio<M>`) is valid for the read
            // thread's life; this thread is its sole `processOutput` toucher.
            //
            // if (builtin.os.tag == .windows) ReadThread.threadMainWindows else ReadThread.threadMainPosix,
            //
            // PORT NOTE (S211-F3): this always called `threadMainPosix`, so the
            // ported `threadMainWindows` had no caller. Selected per target now,
            // as the reference does at comptime. Built and run on Windows by
            // task W-1 (`windows_threadenter_reads_the_pty_through_the_windows_read_thread`).
            #[cfg(windows)]
            ReadThread::threadMainWindows::<M>(args.0, args.1, args.2);
            #[cfg(not(windows))]
            ReadThread::threadMainPosix::<M>(args.0, args.1, args.2);
        }) {
            Ok(t) => t,
            Err(_e) => {
                note_thread_enter_failure!("threadEnter: std.Thread.spawn", _e);
                // errdefer (reverse): termios_timer.deinit(); stream.deinit();
                //          close pipe[1]; close pipe[0]; process.deinit();
                //          self.subprocess.stop();
                termios_timer.deinit();
                stream.deinit();
                unsafe {
                    close_fd(pipe[1]);
                    close_fd(pipe[0]);
                }
                if let Some(p) = &mut process {
                    p.deinit();
                }
                // WS19 T17a [R1-DIVERGENCE]: drain the pty master BEFORE stop() — the
                // io-reader thread does not exist on this arm, so without this the
                // SIGHUP'd child can wedge unreapably (`?Es`). See
                // `drain_pty_master_once`.
                #[cfg(not(windows))]
                let _ = drain_pty_master_once(pty_fds.read);
                unsafe { Subprocess::stop(&raw mut (*self_).subprocess) };
                return Err(crate::Error);
            }
        };

        // Setup our threadata backend state to be our own
        // td.backend = .{ .exec = .{
        //     .start = process_start,
        //     .write_stream = stream,
        //     .process = process,
        //     .read_thread = read_thread,
        //     .read_thread_pipe = pipe[1],
        //     .read_thread_fd = pty_fds.read,
        //     .termios_timer = termios_timer,
        // } };
        //
        // PORT NOTE: the reference's `td.backend` was `undefined` (the
        // out-parameter protocol — `Termio.threadEnter` builds `ThreadData{ …,
        // .backend = undefined }`, and THIS is the Backend that fills it). Rust
        // has no `undefined`; the caller supplies a placeholder `backend`, and
        // this move-assign drops it (the placeholder is constructed to be
        // trivially droppable). The remaining exec fields take their reference
        // struct defaults (`write_pool` = `.empty` = an empty pool,
        // `write_queue`/`*_c` = `.{}`, `termios_timer_running`
        // = true, `termios_mode` = `.{}`).
        td.backend = crate::backend::ThreadData::Exec(ThreadData {
            start: process_start,
            exited: false,
            write_stream: stream,
            process,
            write_pool: MemoryPool::init(alloc),
            write_queue: xev::WriteQueue::default(),
            process_wait_c: xev::Completion::default(),
            flatpak_wait_c: FlatpakHostCommandCompletion::default(),
            read_thread,
            read_thread_pipe: pipe[1],
            read_thread_fd: pty_fds.read,
            termios_timer,
            termios_timer_c: xev::Completion::default(),
            termios_timer_running: true,
            termios_mode: ptypkg::Mode::default(),
        });

        // A raw pointer into the now-installed `td.backend.exec` for the
        // loop-stored completion pointers (`&td.backend.exec.*_c`) — stable for
        // the loop's life (frame-pinned `td`).
        // (PORT NOTE: `td.backend.exec` — a single-variant `union(Kind){exec}` field
        // access; the irrefutable `let` binds the sole `Exec` variant.)
        let crate::backend::ThreadData::Exec(exec_ptr) = &mut td.backend;
        let exec_ptr: *mut ThreadData = exec_ptr;

        // Start our process watcher. If we have an xev.Process use it.
        // if (process) |*p| p.wait(
        //     td.loop,
        //     &td.backend.exec.process_wait_c,
        //     termio.Termio.ThreadData,
        //     td,
        //     processExit,
        // ) else if (comptime build_config.flatpak) flatpak: {
        //     switch (self.subprocess.process orelse break :flatpak) {
        //         .flatpak => |*c| c.waitXev(
        //             td.loop, &td.backend.exec.flatpak_wait_c,
        //             termio.Termio.ThreadData, td, flatpakExit,
        //         ),
        //         .fork_exec => {},
        //     }
        // }
        //
        // (PORT NOTE (WS20): filled in. `comptime build_config.flatpak` is the
        // `flatpak` cargo feature; with it off the `else if` arm is compiled out
        // exactly as Zig's lazy compilation leaves it.)
        if let Some(p) = process {
            // SAFETY: `td.loop_`/`process_wait_c`/`td_ptr` are all valid, stable
            // pointers held raw by the loop across ticks.
            unsafe {
                p.wait(
                    td.loop_,
                    std::ptr::addr_of_mut!((*exec_ptr).process_wait_c),
                    td_ptr,
                    Self::processExit::<M>,
                );
            }
        } else {
            #[cfg(feature = "flatpak")]
            // switch (self.subprocess.process orelse break :flatpak) { ... }
            if let Some(proc) = unsafe { (*self_).subprocess.process.as_mut() } {
                match proc {
                    // If we're in flatpak and we have a flatpak command
                    // then we can run the special flatpak logic for watching.
                    Process::Flatpak(c) => unsafe {
                        c.waitXev(
                            td.loop_,
                            std::ptr::addr_of_mut!((*exec_ptr).flatpak_wait_c),
                            td_ptr.cast::<core::ffi::c_void>(),
                            Self::flatpakExit::<M>,
                        );
                    },
                    Process::ForkExec(_) => {}
                }
            }
        }

        // Start our termios timer. We don't support this on Windows.
        // Fundamentally, we could support this on Windows so we're just
        // waiting for someone to implement it.
        // if (comptime builtin.os.tag != .windows) {
        //     termios_timer.run(
        //         td.loop, &td.backend.exec.termios_timer_c, TERMIOS_POLL_MS,
        //         termio.Termio.ThreadData, td, termiosTimer,
        //     );
        // }
        #[cfg(not(windows))]
        unsafe {
            termios_timer.run(
                td.loop_,
                std::ptr::addr_of_mut!((*exec_ptr).termios_timer_c),
                TERMIOS_POLL_MS,
                td_ptr,
                Self::termiosTimer::<M>,
            );
        }

        Ok(())
    }

    // [M3 / review follow-up] This function is FULLY IMPLEMENTED (an earlier
    // "DEFERRED (D6 xev thread lifecycle)" header here was stale). Everything
    // below is ported: `externalExit`, `stop()`, the read-thread quit-pipe
    // write, the windows `CancelIoEx` arm and the read-thread join. It matters
    // that this is real code, because the `killPid` divergence note above leans
    // on it: the ORDERING here (`stop()` runs while the io-reader thread is
    // still draining the pty master) is what keeps the normal close path off
    // the wedging condition. Do not reorder it. Reference, verbatim:
    //
    // pub fn threadExit(self: *Exec, td: *termio.Termio.ThreadData) void {
    //     assert(td.backend == .exec);
    //     const exec = &td.backend.exec;
    //
    //     if (exec.exited) self.subprocess.externalExit();
    //     self.subprocess.stop();
    //
    //     // Quit our read thread after exiting the subprocess so that
    //     // we don't get stuck waiting for data to stop flowing if it is
    //     // a particularly noisy process.
    //     _ = posix.write(exec.read_thread_pipe, "x") catch |err| switch (err) {
    //         // BrokenPipe means that our read thread is closed already,
    //         // which is completely fine since that is what we were trying
    //         // to achieve.
    //         error.BrokenPipe => {},
    //
    //         else => log.warn(
    //             "error writing to read thread quit pipe err={}",
    //             .{err},
    //         ),
    //     };
    //
    //     if (comptime builtin.os.tag == .windows) {
    //         // Interrupt the blocking read so the thread can see the quit message
    //         if (windows.kernel32.CancelIoEx(exec.read_thread_fd, null) == 0) {
    //             switch (windows.kernel32.GetLastError()) {
    //                 .NOT_FOUND => {},
    //                 else => |err| log.warn("error interrupting read thread err={}", .{err}),
    //             }
    //         }
    //     }
    //
    //     exec.read_thread.join();
    // }
    //
    // PORT NOTE (N449, P7-W7 T5d): raw-self, the reference's `self: *Exec`.
    // The io writer thread runs this while the read thread still writes the
    // terminal (it is joined at the end) and the main thread may read the pty
    // and `args`; a `&mut Exec` would be a Unique over the whole subprocess.
    // Only `subprocess.process` is reached, by place (`externalExit`, `stop`).
    ///
    /// # Safety
    /// `self_` must point at a live `Exec`.
    pub unsafe fn threadExit<M: rustty_apprt_api::surface::AppMailbox>(
        self_: *mut Exec,
        td: &mut crate::termio_impl::ThreadData<M>,
    ) {
        // assert(td.backend == .exec);
        // const exec = &td.backend.exec;
        // (PORT NOTE: single-variant `union(Kind){exec}` field access.)
        let crate::backend::ThreadData::Exec(exec) = &mut td.backend;

        // if (exec.exited) self.subprocess.externalExit();
        if exec.exited {
            unsafe { Subprocess::externalExit(&raw mut (*self_).subprocess) };
        }
        // self.subprocess.stop();
        unsafe { Subprocess::stop(&raw mut (*self_).subprocess) };

        // Quit our read thread after exiting the subprocess so that
        // we don't get stuck waiting for data to stop flowing if it is
        // a particularly noisy process.
        // _ = posix.write(exec.read_thread_pipe, "x") catch |err| switch (err) {
        //     // BrokenPipe means that our read thread is closed already,
        //     // which is completely fine since that is what we were trying
        //     // to achieve.
        //     error.BrokenPipe => {},
        //     else => log.warn("error writing to read thread quit pipe err={}", .{err}),
        // };
        //
        // SAFETY: `read_thread_pipe` is the write end of the quit pipe, valid
        // until `ThreadData::deinit` closes it. A closed read end yields EPIPE
        // (Rust ignores SIGPIPE), which — like the reference's `error.BrokenPipe`
        // — is fine (the read thread is already gone).
        #[cfg(not(windows))]
        let rc = unsafe {
            libc::write(
                exec.read_thread_pipe,
                b"x".as_ptr().cast::<std::ffi::c_void>(),
                1,
            )
        };
        // PORT NOTE: reference defect (R13), UPSTREAM-DEFECTS.md UD325. At the pin
        // the write is `switch (posix.errno(posix.system.write(exec.read_thread_pipe,
        // "x", 1)))` (`Exec.zig:206`), and Ghostty links libc, so `posix.system`
        // is `std.c`: the C runtime's `write`, whose first argument is a CRT file
        // descriptor. On Windows `read_thread_pipe` is a Win32 HANDLE, which the
        // UCRT rejects through its invalid-parameter handler: the process
        // fast-fails (0xC0000409) at every surface close. MEASURED on Windows
        // (task W-1b, and the W-1 review's C repro). The port writes the byte
        // with `WriteFile`, the Win32 call for the HANDLE the reference holds. A
        // read end that is already closed fails with ERROR_BROKEN_PIPE (109) or
        // ERROR_NO_DATA (232), the Windows form of the EPIPE the POSIX arm
        // ignores; any other failure gets the reference's warning, with the
        // Win32 error code.
        //
        // CORRECTION (task W-1b, review W-1 F1): this note used to read
        // "`posix.fd_t` is a HANDLE there, and the write is `WriteFile` on the
        // quit pipe's write end", as if the reference called Zig's
        // `posix.write` (which would be `WriteFile`); it calls `posix.system.write`.
        #[cfg(windows)]
        {
            use rustty_os::windows as win;
            let mut written: win::DWORD = 0;
            let ok = unsafe {
                win::exp::kernel32::WriteFile(
                    exec.read_thread_pipe,
                    b"x".as_ptr().cast::<std::ffi::c_void>(),
                    1,
                    &mut written,
                    std::ptr::null_mut(),
                )
            };
            if ok == win::FALSE {
                let e = unsafe { win::kernel32::GetLastError() };
                const ERROR_BROKEN_PIPE: win::DWORD = 109;
                const ERROR_NO_DATA: win::DWORD = 232;
                if e != ERROR_BROKEN_PIPE && e != ERROR_NO_DATA {
                    log::warn!(
                        target: LOG_SCOPE,
                        "error writing to read thread quit pipe err={e}"
                    );
                }
            }
        }
        #[cfg(not(windows))]
        if rc < 0 {
            // .SUCCESS => {}, .PIPE => {},
            // else => |e| log.warn(
            //     "error writing to read thread quit pipe err=E{s}",
            //     .{@tagName(e)},
            // ),
            //
            // PORT NOTE: the port's earlier Zig comment here reproduced an OLDER
            // upstream shape (`error.BrokenPipe` + `err={}`); at pin 20abdb50a the
            // reference switches on the ERRNO and renders `err=E{s}` from
            // `@tagName`. The live call below follows the pin.
            let e = errno();
            if e != libc::EPIPE {
                log::warn!(
                    target: LOG_SCOPE,
                    "error writing to read thread quit pipe err=E{}",
                    errno_tag_name(e)
                );
            }
        }

        // if (comptime builtin.os.tag == .windows) {
        //     // Interrupt the blocking read so the thread can see the quit message
        //     if (windows.kernel32.CancelIoEx(exec.read_thread_fd, null) == 0) {
        //         switch (windows.kernel32.GetLastError()) {
        //             .NOT_FOUND => {},
        //             else => |err| log.warn("error interrupting read thread err={}", .{err}),
        //         }
        //     }
        // }
        #[cfg(windows)]
        {
            let read_thread = &exec.read_thread;
            ReadThread::interruptWindows(exec.read_thread_fd, || read_thread.is_finished());
        }

        // exec.read_thread.join();
        //
        // (PORT NOTE: the reference `join()` returns `void`; Rust's returns a `Result`
        // that is `Err` if the read thread panicked — the reference ignores it,
        // so we do too. `JoinHandle::join` consumes the handle by value, but
        // `exec` is a `&mut` place, so the handle is moved out via `mem::replace`
        // and the slot is left holding a detached already-finished no-op thread —
        // keeping `exec` fully-initialized for its later `deinit`. `threadExit`
        // is terminal for the ThreadData; nothing reads `read_thread` again.)
        let read_thread = std::mem::replace(&mut exec.read_thread, std::thread::spawn(|| {}));
        let _ = read_thread.join();
    }

    // [task-18 STALE HEADER STRUCK (S211-F7): this said "DEFERRED (D6 xev termios
    // timer, 0 tests — R1-safe)". `focusGained` is FULLY IMPLEMENTED at :939
    // below. It was also mislabelled: the block it heads is `focusGained`, not the
    // termios timer (that is `termiosTimer`, live at :1276).] Reference, verbatim:
    //
    // pub fn focusGained(
    //     self: *Exec,
    //     td: *termio.Termio.ThreadData,
    //     focused: bool,
    // ) !void {
    //     _ = self;
    //
    //     assert(td.backend == .exec);
    //     const execdata = &td.backend.exec;
    //
    //     // Windows has no termios, so there is nothing to poll.
    //     if (comptime builtin.os.tag == .windows) return;
    //
    //     if (!focused) {
    //         // Flag the timer to end on the next iteration. This is
    //         // a lot cheaper than doing full timer cancellation.
    //         execdata.termios_timer_running = false;
    //     } else {
    //         // Always set this to true. There is a race condition if we lose
    //         // focus and regain focus before the termios timer ticks where
    //         // if we don't set this unconditionally the timer will end on
    //         // the next iteration.
    //         execdata.termios_timer_running = true;
    //
    //         // If we're focused, we want to start our termios timer. We
    //         // only do this if it isn't already running. We use the termios
    //         // callback because that'll trigger an immediate state check AND
    //         // start the timer.
    //         if (execdata.termios_timer_c.state() != .active) {
    //             _ = termiosTimer(td, undefined, undefined, {});
    //         }
    //     }
    // }
    // (Windows returns before the termios half, as the reference's comptime
    // `return` does, so its locals go unused there.)
    //
    // PORT NOTE (N449, P7-W7 T5d): the receiver is the raw `self: *Exec` (see
    // `Backend`'s note); the body never touches it.
    #[cfg_attr(windows, allow(unused_variables))]
    pub fn focusGained<M: rustty_apprt_api::surface::AppMailbox + 'static>(
        self_: *mut Exec,
        td: &mut crate::termio_impl::ThreadData<M>,
        focused: bool,
    ) -> Result<(), crate::Error> {
        // _ = self;
        let _ = self_;

        // assert(td.backend == .exec);
        // const execdata = &td.backend.exec;
        let td_ptr: *mut crate::termio_impl::ThreadData<M> = td;
        // (PORT NOTE: single-variant `union(Kind){exec}` field access.)
        let crate::backend::ThreadData::Exec(execdata) = &mut td.backend;
        let execdata: *mut ThreadData = execdata;

        // Windows has no termios, so there is nothing to poll.
        // if (comptime builtin.os.tag == .windows) return;
        #[cfg(windows)]
        return Ok(());

        // if (!focused) {
        #[cfg(not(windows))]
        if !focused {
            // Flag the timer to end on the next iteration. This is
            // a lot cheaper than doing full timer cancellation.
            // execdata.termios_timer_running = false;
            unsafe {
                (*execdata).termios_timer_running = false;
            }
        } else {
            // Always set this to true. There is a race condition if we lose
            // focus and regain focus before the termios timer ticks where
            // if we don't set this unconditionally the timer will end on
            // the next iteration.
            // execdata.termios_timer_running = true;
            unsafe {
                (*execdata).termios_timer_running = true;
            }

            // If we're focused, we want to start our termios timer. We
            // only do this if it isn't already running. We use the termios
            // callback because that'll trigger an immediate state check AND
            // start the timer.
            // if (execdata.termios_timer_c.state() != .active) {
            //     _ = termiosTimer(td, undefined, undefined, {});
            // }
            //
            // SAFETY: `termios_timer_c` is a stable Completion in the frame-pinned
            // `td.backend.exec`; `Completion::state_of` only reads its state flag.
            if unsafe { (*execdata).termios_timer_c.state_of() } != xev::CompletionState::Active {
                // The reference passes `undefined` loop/completion — `termiosTimer`
                // ignores both (`_`); `{}` is the void success value `Ok(())`.
                let _ = Self::termiosTimer::<M>(
                    td_ptr,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    Ok(()),
                );
            }
        }

        #[cfg(not(windows))]
        return Ok(());
    }

    // pub fn resize(
    //     self: *Exec,
    //     grid_size: renderer.GridSize,
    //     screen_size: renderer.ScreenSize,
    // ) !void {
    //
    // PORT NOTE (N449, P7-W7 T5d): raw-self (see `Subprocess::resize`).
    ///
    /// # Safety
    /// `self_` must point at a live `Exec`.
    pub unsafe fn resize(
        self_: *mut Exec,
        grid_size: GridSize,
        screen_size: ScreenSize,
    ) -> Result<(), crate::Error> {
        // return try self.subprocess.resize(grid_size, screen_size);
        unsafe { Subprocess::resize(&raw mut (*self_).subprocess, grid_size, screen_size) }
    }

    // [WS20] STALE HEADER STRUCK: this said "DEFERRED (D6 — called only from the
    // xev processExit/flatpakExit callbacks; ...)", but `processExitCommon`,
    // `processExit` and (WS20) `flatpakExit` are all LIVE below and are wired
    // from `threadEnter`. What follows is only the verbatim reference source the
    // live bodies were ported from:
    //
    // fn processExitCommon(td: *termio.Termio.ThreadData, exit_code: u32) void {
    //     assert(td.backend == .exec);
    //     const execdata = &td.backend.exec;
    //     execdata.exited = true;
    //
    //     // Determine how long the process was running for.
    //     const runtime_ms: ?u64 = runtime: {
    //         const process_end = std.time.Instant.now() catch break :runtime null;
    //         const runtime_ns = process_end.since(execdata.start);
    //         const runtime_ms = runtime_ns / std.time.ns_per_ms;
    //         break :runtime runtime_ms;
    //     };
    //     log.debug(
    //         "child process exited status={} runtime={}ms",
    //         .{ exit_code, runtime_ms orelse 0 },
    //     );
    //
    //     // We always notify the surface immediately that the child has
    //     // exited and some metadata about the exit.
    //     _ = td.surface_mailbox.push(.{
    //         .child_exited = .{
    //             .exit_code = exit_code,
    //             .runtime_ms = runtime_ms orelse 0,
    //         },
    //     }, .{ .forever = {} });
    // }
    //
    // fn processExit(
    //     td_: ?*termio.Termio.ThreadData,
    //     _: *xev.Loop,
    //     _: *xev.Completion,
    //     r: xev.Process.WaitError!u32,
    // ) xev.CallbackAction {
    //     const exit_code = r catch unreachable;
    //     processExitCommon(td_.?, exit_code);
    //     return .disarm;
    // }
    //
    // fn flatpakExit(
    //     td_: ?*termio.Termio.ThreadData,
    //     _: *xev.Loop,
    //     _: *FlatpakHostCommand.Completion,
    //     r: FlatpakHostCommand.WaitError!u8,
    // ) void {
    //     const exit_code = r catch unreachable;
    //     processExitCommon(td_.?, exit_code);
    // }
    //
    // fn termiosTimer(
    //     td_: ?*termio.Termio.ThreadData,
    //     _: *xev.Loop,
    //     _: *xev.Completion,
    //     r: xev.Timer.RunError!void,
    // ) xev.CallbackAction {
    //     // log.debug("termios timer fired", .{});
    //
    //     // This should never happen because we guard starting our
    //     // timer on windows but we want this assertion to fire if
    //     // we ever do start the timer on windows.
    //     // TODO: support on windows
    //     if (comptime builtin.os.tag == .windows) {
    //         @panic("termios timer not implemented on Windows");
    //     }
    //
    //     _ = r catch |err| switch (err) {
    //         // This is sent when our timer is canceled. That's fine.
    //         error.Canceled => return .disarm,
    //
    //         else => {
    //             log.warn("error in termios timer callback err={}", .{err});
    //             @panic("crash in termios timer callback");
    //         },
    //     };
    //
    //     const td = td_.?;
    //     assert(td.backend == .exec);
    //     const exec = &td.backend.exec;
    //
    //     // This is kind of hacky but we rebuild a Pty struct to get the
    //     // termios data.
    //     const mode: ptypkg.Mode = (Pty{
    //         .master = exec.read_thread_fd,
    //         .slave = undefined,
    //     }).getMode() catch |err| err: {
    //         log.warn("error getting termios mode err={}", .{err});
    //
    //         // If we have an error we return the default mode values
    //         // which are the likely values.
    //         break :err .{};
    //     };
    //
    //     // If the mode changed, then we process it.
    //     if (!std.meta.eql(mode, exec.termios_mode)) mode_change: {
    //         log.debug("termios change mode={}", .{mode});
    //         exec.termios_mode = mode;
    //
    //         // We assume we're in some sort of password input if we're
    //         // in canonical mode and not echoing. This is a heuristic.
    //         const password_input = mode.canonical and !mode.echo;
    //
    //         // If our password input state changed on the terminal then
    //         // we notify the surface.
    //         {
    //             td.renderer_state.mutex.lock();
    //             defer td.renderer_state.mutex.unlock();
    //             const t = td.renderer_state.terminal;
    //             if (t.flags.password_input == password_input) {
    //                 break :mode_change;
    //             }
    //         }
    //
    //         // We have to notify the surface that we're in password input.
    //         // We must block on this because the balanced true/false state
    //         // of this is critical to apprt behavior.
    //         _ = td.surface_mailbox.push(.{
    //             .password_input = password_input,
    //         }, .{ .forever = {} });
    //     }
    //
    //     // Repeat the timer
    //     if (exec.termios_timer_running) {
    //         exec.termios_timer.run(
    //             td.loop,
    //             &exec.termios_timer_c,
    //             TERMIOS_POLL_MS,
    //             termio.Termio.ThreadData,
    //             td,
    //             termiosTimer,
    //         );
    //     }
    //
    //     return .disarm;
    // }
    //
    // The four exit / termios callbacks below fill the verbatim reference bodies
    // preserved in the comment block above. [WS20] `flatpakExit` is now LIVE too
    // (gated on the `flatpak` cargo feature = `comptime build_config.flatpak`,
    // matching Zig lazy compilation). Userdata is termio's
    // `ThreadData<M>` (the reference `termio.Termio.ThreadData`), so these are
    // generic over `M` and reference it via `Self::processExit::<M>` etc.

    // fn processExitCommon(td: *termio.Termio.ThreadData, exit_code: u32) void
    fn processExitCommon<M: rustty_apprt_api::surface::AppMailbox + 'static>(
        td: *mut crate::termio_impl::ThreadData<M>,
        exit_code: u32,
    ) {
        // assert(td.backend == .exec);
        // const execdata = &td.backend.exec;
        // (PORT NOTE: single-variant `union(Kind){exec}` field access.)
        let crate::backend::ThreadData::Exec(execdata) = unsafe { &mut (*td).backend };
        let execdata: *mut ThreadData = execdata;
        // execdata.exited = true;
        unsafe {
            (*execdata).exited = true;
        }

        // Determine how long the process was running for.
        // const runtime_ms: ?u64 = runtime: {
        //     const process_end = std.time.Instant.now() catch break :runtime null;
        //     const runtime_ns = process_end.since(execdata.start);
        //     const runtime_ms = runtime_ns / std.time.ns_per_ms;
        //     break :runtime runtime_ms;
        // };
        //
        // (PORT NOTE: Rust's `Instant::now()` is infallible, so the `catch ... null`
        // arm never fires; `since` -> `duration_since`, `/ ns_per_ms` ->
        // `as_nanos() / 1_000_000`.)
        let runtime_ms: Option<u64> = {
            let process_end = std::time::Instant::now();
            let start = unsafe { (*execdata).start };
            let runtime_ns = process_end.duration_since(start);
            Some((runtime_ns.as_nanos() / 1_000_000) as u64)
        };
        // log.debug("child process exited status={} runtime={}ms", .{ exit_code, runtime_ms orelse 0 });
        log::debug!(
            target: LOG_SCOPE,
            "child process exited status={} runtime={}ms",
            exit_code,
            runtime_ms.unwrap_or(0)
        );

        // We always notify the surface immediately that the child has
        // exited and some metadata about the exit.
        // _ = td.surface_mailbox.push(.{ .child_exited = .{
        //     .exit_code = exit_code, .runtime_ms = runtime_ms orelse 0,
        // } }, .{ .forever = {} });
        let _ = unsafe {
            (*td).surface_mailbox.push(
                SurfaceMessage::ChildExited(ChildExited {
                    exit_code,
                    runtime_ms: runtime_ms.unwrap_or(0),
                }),
                Timeout::Forever,
            )
        };
    }

    // fn processExit(
    //     td_: ?*termio.Termio.ThreadData,
    //     _: *xev.Loop,
    //     _: *xev.Completion,
    //     r: xev.Process.WaitError!u32,
    // ) xev.CallbackAction
    fn processExit<M: rustty_apprt_api::surface::AppMailbox + 'static>(
        td_: *mut crate::termio_impl::ThreadData<M>,
        _l: *mut xev::Loop,
        _c: *mut xev::Completion,
        r: core::result::Result<u32, xev::ProcessWaitError>,
    ) -> xev::CallbackAction {
        // const exit_code = r catch unreachable;
        //
        // PORT NOTE: reference defect (R13), UD67. The reference unwraps the wait
        // result with `catch unreachable`, but on the kqueue backend (macOS,
        // FreeBSD) a child that exits BEFORE the `EVFILT_PROC` registration is
        // submitted makes XNU answer the registration with `ESRCH`, which libxev
        // maps to `error.NoSuchProcess`. The registration is only submitted on
        // the loop's next tick after `threadEnter`, so a command that exits at
        // once (a typo'd `command`, `/usr/bin/true`) can reach it; measured on
        // macOS 26.5.1 (a zombie child's `EV_ADD` returns `EV_ERROR` with
        // `data = ESRCH`). ReleaseSafe panics; ReleaseFast is undefined
        // behaviour. The child HAS exited, so the port takes the reference's own
        // exit path, `processExitCommon`, with exit code 0 — the value libxev's
        // kqueue backend itself reports when it has no decodable exit status
        // (`kqueue.zig`'s proc arm returns 0 unless `WIFEXITED`). The other
        // wait errors are left as the reference's `unreachable`: the completion
        // is never canceled, and the proc op always delivers a kevent.
        let exit_code = match r {
            Ok(code) => code,
            #[cfg(any(
                target_os = "macos",
                target_os = "ios",
                target_os = "visionos",
                target_os = "freebsd"
            ))]
            Err(xev::ProcessWaitError::NoSuchProcess) => 0,
            Err(e) => unreachable!("process wait failed: {e:?}"),
        };
        // processExitCommon(td_.?, exit_code);
        Self::processExitCommon::<M>(td_, exit_code);
        // return .disarm;
        xev::CallbackAction::Disarm
    }

    // fn flatpakExit(
    //     td_: ?*termio.Termio.ThreadData,
    //     _: *xev.Loop,
    //     _: *FlatpakHostCommand.Completion,
    //     r: FlatpakHostCommand.WaitError!u8,
    // ) void
    //
    // PORT NOTE (WS20): filled in. The userdata arrives as the `?*anyopaque` the
    // `os/flatpak.zig` `Completion.callback` signature carries, so the typed
    // `ThreadData<M>` pointer is recovered by cast exactly as the reference's
    // `waitXev` trampoline does. `#[cfg(feature = "flatpak")]` because its only
    // caller is the `comptime build_config.flatpak` arm of `threadEnter`.
    #[cfg(feature = "flatpak")]
    fn flatpakExit<M: rustty_apprt_api::surface::AppMailbox + 'static>(
        td_: *mut core::ffi::c_void,
        _l: *mut xev::Loop,
        _c: *mut FlatpakHostCommandCompletion,
        r: core::result::Result<u8, FlatpakHostCommandWaitError>,
    ) {
        // const exit_code = r catch unreachable;
        //
        // PORT NOTE (N171): an R7 invariant, not an R13 defect; nothing
        // outside the process can produce either half of `WaitError`
        // (`os/flatpak.zig:115`) here. The `Error` half is stored only while
        // the state is `.init` or `.err`, but `threadEnter` calls `waitXev`
        // only after `spawn` succeeded, which means the state is `.started`
        // or `.exited`. The timer half is `Canceled`, which needs a cancel of
        // this completion (nothing issues one), or `Unexpected`, which only
        // an io_uring timeout result other than ETIME/ECANCELED produces (the
        // kernel does not; kqueue and epoll never do).
        let exit_code = r.expect(
            "flatpak wait result is an exit code: waitXev runs only after a \
             successful spawn and its timer is never canceled",
        );
        // processExitCommon(td_.?, exit_code);
        Self::processExitCommon::<M>(
            td_.cast::<crate::termio_impl::ThreadData<M>>(),
            exit_code as u32,
        );
    }

    // fn termiosTimer(
    //     td_: ?*termio.Termio.ThreadData,
    //     _: *xev.Loop,
    //     _: *xev.Completion,
    //     r: xev.Timer.RunError!void,
    // ) xev.CallbackAction
    //
    // (Windows: every caller is gated off Windows, as the reference's comptime
    // guards gate them, so Zig never analyzes this fn there; it is compiled
    // out the same way. The `@panic` arm below is kept for R1.)
    #[cfg(not(windows))]
    fn termiosTimer<M: rustty_apprt_api::surface::AppMailbox + 'static>(
        td_: *mut crate::termio_impl::ThreadData<M>,
        _l: *mut xev::Loop,
        _c: *mut xev::Completion,
        r: core::result::Result<(), xev::TimerRunError>,
    ) -> xev::CallbackAction {
        // log.debug("termios timer fired", .{});

        // This should never happen because we guard starting our
        // timer on windows but we want this assertion to fire if
        // we ever do start the timer on windows.
        // TODO: support on windows
        // if (comptime builtin.os.tag == .windows) { @panic("termios timer not implemented on Windows"); }
        #[cfg(windows)]
        panic!("termios timer not implemented on Windows");

        // _ = r catch |err| switch (err) {
        //     error.Canceled => return .disarm,
        //     else => { log.warn(...); @panic("crash in termios timer callback"); },
        // };
        match r {
            Ok(()) => {}
            // This is sent when our timer is canceled. That's fine.
            Err(xev::TimerRunError::Canceled) => return xev::CallbackAction::Disarm,
            Err(err) => {
                // log.warn("error in termios timer callback err={}", .{err});
                log::warn!(target: LOG_SCOPE, "error in termios timer callback err={err:?}");
                panic!("crash in termios timer callback");
            }
        }

        // const td = td_.?;
        let td = td_;
        // assert(td.backend == .exec);
        // const exec = &td.backend.exec;
        // (PORT NOTE: single-variant `union(Kind){exec}` field access.)
        let crate::backend::ThreadData::Exec(exec) = unsafe { &mut (*td).backend };
        let exec: *mut ThreadData = exec;

        // This is kind of hacky but we rebuild a Pty struct to get the
        // termios data.
        // const mode: ptypkg.Mode = (Pty{ .master = exec.read_thread_fd, .slave = undefined }).getMode() catch |err| err: {
        //     log.warn("error getting termios mode err={}", .{err});
        //     break :err .{};
        // };
        //
        // (PORT NOTE: Rust struct privacy forbids the inline `Pty{ .master, .slave =
        // undefined }` literal; `Pty::init_master_only` is that throwaway
        // master-only Pty — see its rustty-os PORT NOTE.)
        let master = unsafe { (*exec).read_thread_fd };
        // (PORT NOTE: kept as a `match` — not `.unwrap_or_default()` — to preserve the
        // reference's `catch |err| { log.warn(...); break :err .{} }` error arm,
        // which logs before falling back to the default `Mode`.)
        #[allow(clippy::manual_unwrap_or_default)]
        let mode: ptypkg::Mode = match Pty::init_master_only(master).getMode() {
            Ok(m) => m,
            Err(err) => {
                // log.warn("error getting termios mode err={}", .{err});
                log::warn!(target: LOG_SCOPE, "error getting termios mode err={err:?}");
                // break :err .{};
                ptypkg::Mode::default()
            }
        };

        // If the mode changed, then we process it.
        // if (!std.meta.eql(mode, exec.termios_mode)) mode_change: {
        if mode != unsafe { (*exec).termios_mode } {
            'mode_change: {
                // log.debug("termios change mode={}", .{mode});
                log::debug!(target: LOG_SCOPE, "termios change mode={mode:?}");
                // exec.termios_mode = mode;
                unsafe {
                    (*exec).termios_mode = mode;
                }

                // We assume we're in some sort of password input if we're
                // in canonical mode and not echoing. This is a heuristic.
                // const password_input = mode.canonical and !mode.echo;
                let password_input = mode.canonical && !mode.echo;

                // If our password input state changed on the terminal then
                // we notify the surface.
                // {
                //     td.renderer_state.mutex.lock();
                //     defer td.renderer_state.mutex.unlock();
                //     const t = td.renderer_state.terminal;
                //     if (t.flags.password_input == password_input) break :mode_change;
                // }
                {
                    // SAFETY: `renderer_state` is the live shared render State for
                    // the io thread's life; `.mutex` is the guard-less shared lock
                    // (the guard-less C0 model makes the `defer unlock` explicit).
                    let rs = unsafe { (*td).renderer_state };
                    unsafe {
                        (*(*rs).mutex).lock();
                    }
                    let t = unsafe { (*rs).terminal };
                    let same = unsafe { (*t).flags.password_input } == password_input;
                    unsafe {
                        (*(*rs).mutex).unlock();
                    }
                    if same {
                        break 'mode_change;
                    }
                }

                // We have to notify the surface that we're in password input.
                // We must block on this because the balanced true/false state
                // of this is critical to apprt behavior.
                // _ = td.surface_mailbox.push(.{ .password_input = password_input }, .{ .forever = {} });
                let _ = unsafe {
                    (*td).surface_mailbox.push(
                        SurfaceMessage::PasswordInput(password_input),
                        Timeout::Forever,
                    )
                };
            }
        }

        // Repeat the timer
        // if (exec.termios_timer_running) {
        if unsafe { (*exec).termios_timer_running } {
            // exec.termios_timer.run(
            //     td.loop, &exec.termios_timer_c, TERMIOS_POLL_MS,
            //     termio.Termio.ThreadData, td, termiosTimer,
            // );
            //
            // SAFETY: `td.loop_`/`&exec.termios_timer_c` are stable pointers held
            // raw by the loop; `td` is the frame-pinned userdata.
            unsafe {
                let timer = (*exec).termios_timer;
                let loop_ = (*td).loop_;
                timer.run(
                    loop_,
                    std::ptr::addr_of_mut!((*exec).termios_timer_c),
                    TERMIOS_POLL_MS,
                    td,
                    Self::termiosTimer::<M>,
                );
            }
        }

        // return .disarm;
        xev::CallbackAction::Disarm
    }

    // [W6 STALE HEADER STRUCK: this said "DEFERRED (D6 xev write stream/pools,
    // 0 tests — R1-safe)" — the body below is LIVE.] Preserved verbatim:
    //
    // pub fn queueWrite(
    //     self: *Exec,
    //     alloc: Allocator,
    //     td: *termio.Termio.ThreadData,
    //     data: []const u8,
    //     linefeed: bool,
    // ) !void {
    //     _ = self;
    //     const exec = &td.backend.exec;
    //
    //     // If our process is exited then we don't send any more writes.
    //     if (exec.exited) return;
    //
    //     // We go through and chunk the data if necessary to fit into
    //     // our cached buffers that we can queue to the stream.
    //     var i: usize = 0;
    //     while (i < data.len) {
    //         const w = try exec.write_pool.create(alloc);
    //         w.td = exec;
    //         const buf = &w.buf;
    //         const slice = slice: {
    //             // The maximum end index is either the end of our data or
    //             // the end of our buffer, whichever is smaller.
    //             const max = @min(data.len, i + buf.len);
    //
    //             // Fast
    //             if (!linefeed) {
    //                 fastmem.copy(u8, buf, data[i..max]);
    //                 const len = max - i;
    //                 i = max;
    //                 break :slice buf[0..len];
    //             }
    //
    //             // Slow, have to replace \r with \r\n
    //             var buf_i: usize = 0;
    //             while (i < data.len and buf_i < buf.len - 1) {
    //                 const ch = data[i];
    //                 i += 1;
    //
    //                 if (ch != '\r') {
    //                     buf[buf_i] = ch;
    //                     buf_i += 1;
    //                     continue;
    //                 }
    //
    //                 // CRLF
    //                 buf[buf_i] = '\r';
    //                 buf[buf_i + 1] = '\n';
    //                 buf_i += 2;
    //             }
    //
    //             break :slice buf[0..buf_i];
    //         };
    //
    //         //for (slice) |b| log.warn("write: {x}", .{b});
    //
    //         exec.write_stream.queueWrite(
    //             td.loop,
    //             &exec.write_queue,
    //             &w.req,
    //             .{ .slice = slice },
    //             ThreadData.Write,
    //             w,
    //             ttyWrite,
    //         );
    //     }
    // }
    //
    // PORT NOTE (N449, P7-W7 T5d): the receiver is the raw `self: *Exec` (see
    // `Backend`'s note); the body never touches it.
    pub fn queueWrite<M: rustty_apprt_api::surface::AppMailbox>(
        self_: *mut Exec,
        alloc: Allocator,
        td: &mut crate::termio_impl::ThreadData<M>,
        data: &[u8],
        linefeed: bool,
    ) -> Result<(), crate::Error> {
        // _ = self;
        let _ = self_;
        let _ = alloc;
        // const exec = &td.backend.exec;
        //
        // Raw pointers: `exec` is `td.backend.exec` (an `exec::ThreadData`); the
        // `Stream::queue_write` engine stores `&req.completion` + the WriteQueue
        // raw into the loop and re-enters them across ticks, and `td.loop_` is the
        // shared loop — never a long-lived `&mut` (`rust-mut-args-strongly-
        // protected`).
        let loop_: *mut xev::Loop = td.loop_;
        // (PORT NOTE: single-variant `union(Kind){exec}` field access.)
        let crate::backend::ThreadData::Exec(exec) = &mut td.backend;
        let exec: *mut ThreadData = exec;

        // If our process is exited then we don't send any more writes.
        // if (exec.exited) return;
        if unsafe { (*exec).exited } {
            return Ok(());
        }

        // We go through and chunk the data if necessary to fit into
        // our cached buffers that we can queue to the stream.
        // var i: usize = 0;
        let mut i: usize = 0;
        // while (i < data.len) {
        while i < data.len() {
            // const w = try exec.write_pool.create(alloc);
            //
            // SB/UB (rust-mut-args-strongly-protected): the checked-out node
            // lives in the pool's arena heap, NOT inline in `ThreadData`, so
            // the momentary `&mut write_pool` autoref here never covers the
            // node's bytes; the node's ADDRESS is held by the event loop
            // across completion (stable until `ttyWrite` destroys it). The
            // slot is logically uninitialized (`undefined`), so its fields
            // are written via `addr_of_mut!` raw places — never a `&mut
            // Write` re-borrow (Miri-proven at
            // .port/phase-4/mirihar-termio-pool).
            // (PORT NOTE: the reference passes `alloc` per call
            // (`create(alloc)`); the rustty-zigstd `MemoryPool` owns its
            // arena's child allocator from `init` — the per-call alloc
            // param collapses, matching the `deinit` note below.)
            let w: *mut Write = unsafe {
                (*exec)
                    .write_pool
                    .create()
                    .map_err(|_| crate::Error)?
                    .as_ptr()
            };
            // w.td = exec;
            unsafe { std::ptr::addr_of_mut!((*w).td).write(exec) };
            // const buf = &w.buf;
            // (PORT NOTE: `buf` is the raw address of the node's 64-byte
            // buffer — `buf.len` == 64.)
            let buf: *mut [u8; 64] = unsafe { std::ptr::addr_of_mut!((*w).buf) };
            let buf_len = 64usize;

            // const slice = slice: {
            let slice: *const [u8] = {
                // The maximum end index is either the end of our data or
                // the end of our buffer, whichever is smaller.
                // const max = @min(data.len, i + buf.len);
                let max = std::cmp::min(data.len(), i + buf_len);

                // Fast
                // if (!linefeed) {
                if !linefeed {
                    // fastmem.copy(u8, buf, data[i..max]);
                    // const len = max - i;
                    let len = max - i;
                    // SAFETY: `buf` is a stable 64-byte pool buffer; `len <=
                    // buf_len`; `data[i..max]` is in-bounds.
                    unsafe {
                        std::ptr::copy_nonoverlapping(data[i..max].as_ptr(), buf.cast::<u8>(), len);
                    }
                    // i = max;
                    i = max;
                    // break :slice buf[0..len];
                    std::ptr::slice_from_raw_parts(buf.cast::<u8>(), len)
                } else {
                    // Slow, have to replace \r with \r\n
                    // var buf_i: usize = 0;
                    let mut buf_i: usize = 0;
                    // while (i < data.len and buf_i < buf.len - 1) {
                    while i < data.len() && buf_i < buf_len - 1 {
                        // const ch = data[i];
                        let ch = data[i];
                        // i += 1;
                        i += 1;

                        // if (ch != '\r') {
                        if ch != b'\r' {
                            // buf[buf_i] = ch;
                            unsafe {
                                (*buf)[buf_i] = ch;
                            }
                            // buf_i += 1;
                            buf_i += 1;
                            // continue;
                            continue;
                        }

                        // CRLF
                        // buf[buf_i] = '\r';
                        // buf[buf_i + 1] = '\n';
                        unsafe {
                            (*buf)[buf_i] = b'\r';
                            (*buf)[buf_i + 1] = b'\n';
                        }
                        // buf_i += 2;
                        buf_i += 2;
                    }

                    // break :slice buf[0..buf_i];
                    std::ptr::slice_from_raw_parts(buf.cast::<u8>(), buf_i)
                }
            };

            //for (slice) |b| log.warn("write: {x}", .{b});

            // exec.write_stream.queueWrite(
            //     td.loop, &exec.write_queue, &w.req, .{ .slice = slice },
            //     ThreadData.Write, w, ttyWrite,
            // );
            //
            // SAFETY: `loop_`/`&exec.write_queue`/`&w.req` are stable pointers
            // held raw by the ordered-write engine (the pool-arena node `w`
            // keeps its address until `ttyWrite` destroys it); `slice` points
            // into the node's own 64-byte buffer; the userdata is the `Write`
            // node pointer `ttyWrite` recovers.
            unsafe {
                let write_stream = (*exec).write_stream;
                write_stream.queue_write(
                    loop_,
                    std::ptr::addr_of_mut!((*exec).write_queue),
                    std::ptr::addr_of_mut!((*w).req),
                    xev::WriteBuffer::Slice(slice),
                    w,
                    Self::ttyWrite,
                );
            }
        }

        Ok(())
    }

    // [W6 STALE HEADER STRUCK: this said "DEFERRED (D6 — the xev stream-write
    // completion callback)" — the body below is LIVE.] Preserved verbatim:
    //
    // fn ttyWrite(
    //     w_: ?*ThreadData.Write,
    //     _: *xev.Loop,
    //     _: *xev.Completion,
    //     _: xev.Stream,
    //     _: xev.WriteBuffer,
    //     r: xev.WriteError!usize,
    // ) xev.CallbackAction {
    //     const w = w_.?;
    //     w.td.write_pool.destroy(w);
    //
    //     const d = r catch |err| {
    //         log.err("write error: {}", .{err});
    //         return .disarm;
    //     };
    //     _ = d;
    //     //log.info("WROTE: {d}", .{d});
    //
    //     return .disarm;
    // }
    //
    // PORT NOTE: `w_` is the pooled `ThreadData.Write` node — the reference
    // passes `w` as the `queueWrite` userdata. Non-generic (no `M`).
    fn ttyWrite(
        w_: *mut Write,
        _l: *mut xev::Loop,
        _c: *mut xev::Completion,
        _s: xev::Stream,
        _b: xev::WriteBuffer,
        r: core::result::Result<usize, xev::WriteError>,
    ) -> xev::CallbackAction {
        // const w = w_.?;
        let w = w_;
        // w.td.write_pool.destroy(w);
        //
        // SAFETY: `w` is the pool-arena node this write checked out (the
        // engine pops it from the write queue and never touches it after this
        // callback returns Disarm); `w.td` was written at checkout and the
        // `ThreadData` outlives the loop. The `&mut write_pool` autoref
        // covers only the pool header (free-list + arena fields) inside
        // `ThreadData` — never the arena-heap node bytes — so the addresses
        // of other in-flight nodes stay valid (Miri-proven at
        // .port/phase-4/mirihar-termio-pool).
        unsafe {
            let td = (*w).td;
            (*td)
                .write_pool
                .destroy(std::ptr::NonNull::new_unchecked(w));
        }

        // const d = r catch |err| { log.err("write error: {}", .{err}); return .disarm; };
        let _d = match r {
            Ok(d) => d,
            Err(err) => {
                // log.err("write error: {}", .{err});
                log::error!(target: LOG_SCOPE, "write error: {err:?}");
                return xev::CallbackAction::Disarm;
            }
        };
        // _ = d;
        //log.info("WROTE: {d}", .{d});

        // return .disarm;
        xev::CallbackAction::Disarm
    }

    /// Get information about the process(es) running within the backend. Returns
    /// `null` if there was an error getting the information or the information is
    /// not available on a particular platform.
    //
    // PORT NOTE: reference `pub fn getProcessInfo(self: *Exec, comptime info:
    // ProcessInfo) ?ProcessInfo.Type(info) { return self.subprocess.getProcessInfo(info); }`
    // — split into the two concrete accessors (rustty-os pty.rs / backend.rs
    // precedent).
    //
    // PORT NOTE (N449, P7-W7 T5d): raw-self, like every `Exec` method another
    // thread's call overlaps (see `Backend`'s note).
    ///
    /// # Safety
    /// `self_` must point at a live `Exec`.
    pub unsafe fn getForegroundPid(self_: *mut Exec) -> Option<u64> {
        // PORT NOTE: reference defect (R13), UD367: `None` until the io
        // thread has published the pty (see `Exec::pty_ready`).
        if !unsafe { Self::pty_published(self_) } {
            return None;
        }
        unsafe { Subprocess::getForegroundPid(&raw mut (*self_).subprocess) }
    }

    /// Whether `threadEnter` has published the pty to the process-info
    /// accessors (UD367; see [`Exec::pty_ready`]). Rust-only helper.
    ///
    /// # Safety
    /// `self_` must point at a live `Exec`.
    #[inline]
    unsafe fn pty_published(self_: *const Exec) -> bool {
        unsafe {
            (*self_)
                .pty_ready
                .load(core::sync::atomic::Ordering::Acquire)
        }
    }

    /// See [`Exec::getForegroundPid`] (the `.tty_name` half of the reference
    /// `getProcessInfo`).
    ///
    /// # Safety
    /// As [`Exec::getForegroundPid`]; the caller picks `'a` no longer than
    /// the `Exec` lives.
    pub unsafe fn getTtyName<'a>(self_: *mut Exec) -> Option<&'a CStr> {
        // PORT NOTE: reference defect (R13), UD367 (see `getForegroundPid`).
        if !unsafe { Self::pty_published(self_) } {
            return None;
        }
        unsafe { Subprocess::getTtyName(&raw mut (*self_).subprocess) }
    }
}
