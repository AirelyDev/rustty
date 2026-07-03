# XEV Milestone B — Wire termio to the real rustty-xev loop (first live shell)

Status: ARCHITECT blueprint. Milestone A (real rustty-xev kqueue loop) landed at HEAD.
This milestone makes the **termio writer thread** run on that real loop and spawns a
**real /bin/sh** end-to-end. It grounds every claim in the code cited inline.

**Adversarially reviewed: ACCEPT-WITH-FIXES (2026-07-03) — 3 mandatory UB fixes folded in**
(marked `[ADV-FIX n]`). The structure (pid-only-handle spawn, frame-pinned ThreadData,
bounded live-shell KAT, API-match, test accounting) survived; but the draft waved past three
genuine self-reference/aliasing UB spots in the highest-stakes code:
- **[F1, HIGHEST] `Termio::init` self-reference dangles.** `StreamHandler` holds raw ptrs back
  into the owning Termio (`terminal`/`size`/`termio_mailbox`, stream_handler.rs:5/7/11). The Rust
  seam RETURNS `Termio` by value (termio_impl.rs:446), then the surface `ptr::write(&raw mut
  (*self_).io, io)` (surface.rs:2117/2143) memcpy's it — the handler's ptr *values* are copied but
  still point at the OLD return slot → dangling; nothing re-seats them. §1's "pure struct assembly,
  already pinned" is FALSE. FIX: make `init` an in-place out-param `init(self: *mut Termio<M>, …)
  -> Result<()>` (faithful to the reference `self: *Termio`), OR re-seat
  `terminal_stream.handler.{terminal,size,termio_mailbox}` to `&(*self_).io.{…}` after the
  `ptr::write` (in the surface, or in `threadEnter` which runs on the io thread post-placement).
- **[F2] `threadMain(&mut self, &mut Termio)` + `processOutput(&mut self)` are protected-`&mut`
  aliasing UB.** A `&mut` arg gets a protected Unique retag for the WHOLE call; `threadMain` doesn't
  return until `loop.run(.until_done)` finishes (shell exit), during which (a) the ReadThread
  aliases the same Termio and (b) the parent reads `self.io_thread.stop.notify()` before `join`.
  The reference threads RAW pointers on both paths (`threadMain(self: *Thread, io: *Termio)`); the
  port already made the reader raw but left the WRITER as `&mut`. FIX: `Thread::threadMain` +
  `Termio::processOutput` take raw `*mut Thread`/`*mut Termio<M>`, forming only transient,
  non-overlapping `&mut`/`&` under the renderer mutex.
- **[F3] pty.slave `File` double-close.** `Command.stdin/out/err: Option<&std::fs::File>` — an
  OWNING handle that closes on Drop. The reference passes a non-owning `.{handle=pty.slave}` and
  `posix.close(pty.slave)` exactly once. A `File::from_raw_fd(slave)` that drops + the verbatim
  `posix.close` = double-close (fd-reuse race). FIX: wrap the slave `File` in `ManuallyDrop`
  (or `mem::forget` it) and port the single explicit `posix.close`, matching the reference exactly.
- **[F4, rationale] §2's Stacked-Borrows justification names the wrong field:** `args` is
  materialized into a local `argv` (borrows the local, not `self`); the borrow that coexists with
  the `self_ptr: *mut Subprocess` (from `&mut self`) is `env: self.env.as_ref()` (a `&` of self).
  Not a blocker (B1's Miri check catches it), but derive `self_ptr` via `addr_of_mut!` and order the
  `cmd` field evaluations so no `&self.*` is live across the raw-pointer creation.

Scope decisions (reaffirming XEV_BLUEPRINT §6, unchanged):
- IN: everything in termio + the core `Async::notify` leaves the io thread pokes.
- OUT: `renderer.Thread` (GPU+xev, `renderer_seam.rs:381` = Phase-3) and `App.zig`
  (has no xev loop). The io thread's `renderer_wakeup.notify()` still *fires* (a real
  mach send) — it just lands on an Async whose loop nobody pumps this milestone. That is
  correct and non-blocking (`async.rs` `notify()` fails-safe on `SEND_NO_BUFFER`/timeout).

Two facts discovered while grounding this plan shape everything below:
1. **The self-referential `fork_exec` Command's borrow fields (`args`/`env`/`cwd`/
   `path`/`data`) are read ONLY inside `cmd.start()`. After `start()` returns, the sole
   field ever read again is `pid`** (`killCommand` reads `command.pid` and nothing else —
   `exec.rs:1703-1731`; the parent-side `defer` even frees `self.env` on success,
   Exec.zig:929-930). This collapses the "hard spot" to a scoped-borrow problem.
2. **Core already spawns the io thread and calls `threadMain`.** `surface.rs:2264-2272`
   spawns `std::thread::Builder…spawn(move || unsafe { (*args.thread).threadMain(&mut
   *args.io) })` behind the `IoThreadArgs` Send-wrapper (`surface.rs:1440-1446`). The
   wiring exists; only the bodies are `unimplemented!()`. Milestone B fills the bodies.

---

## §1 — The exact un-defer list (seam → landed rustty-xev API)

Every deferred body below has its VERBATIM reference in-comment already (R5). Milestone B
replaces each `unimplemented!()` with the ported body. Signatures of the real xev API were
checked against each call site — **all match; no termio body needs an xev API absent from
Milestone A.**

### termio `thread.rs` — the run loop (`Thread::threadMain` + private helpers)
Deferred bodies at `thread.rs:310` (threadMain) and the comment block `thread.rs:314-611`
(`threadMain_`, `CallbackData`, `drainMailbox`, `startSynchronizedOutput`, `handleResize`,
`syncResetCallback`, `coalesceCallback`, `wakeupCallback`, `stopCallback`,
`startScrollTimer`, `stopScrollTimer`, `selectionScrollCallback`).

| termio call (verbatim body) | landed rustty-xev API | signature check |
|---|---|---|
| `mailbox.wakeup.wait(&self.loop, &self.wakeup_c, CallbackData, &cb, wakeupCallback)` | `async.rs::Async::wait::<UD,CB>(self, *mut Loop, *mut Completion, *mut UD, CB)` | cb `Fn(*mut UD,*mut Loop,*mut Completion, Result<(),WaitError>)->CallbackAction` ✓ |
| `self.stop.wait(&self.loop, &self.stop_c, …, stopCallback)` | same `Async::wait` | ✓ |
| `self.loop.run(.until_done)` / `.stopped()` / `stop()` | `kqueue.rs::Loop::run(RunMode)` (590), `stopped()` (383), `stop()` (377) | ✓ |
| `self.sync_reset.reset(&self.loop,&sync_reset_c,&sync_reset_cancel_c, SYNC_RESET_MS, …, syncResetCallback)` | `timer.rs::Timer::reset::<UD,CB>(self,*mut Loop,*mut Completion c,*mut Completion c_cancel,u64,*mut UD,CB)` | cb `Fn(…, Result<(),Timer::RunError>)` ✓ |
| `self.coalesce.reset(…, Coalesce::MIN_MS, …, coalesceCallback)` | same `Timer::reset` | ✓ |
| `self.coalesce_c.state() == .active`, `switch(self.scroll_c.state())` | `kqueue.rs::Completion::state_of()->CompletionState` (1595); `CompletionState::{Active,Dead}` | reference `.state()`→ `.state_of()` ✓ |
| `self.scroll.run(&self.loop,&scroll_c, SELECTION_SCROLL_MS, …, selectionScrollCallback)` | `timer.rs::Timer::run::<UD,CB>(self,*mut Loop,*mut Completion,u64,*mut UD,CB)` | ✓ |
| `io.surface_mailbox.push(.{.selection_scroll_tick=…}, .{.instant={}})` | `apprt-api surface.rs::AppMailbox::push(msg, Timeout)` | ✓ (already live) |
| `io.renderer_wakeup.notify()` (drainMailbox redraw) | `async.rs::Async::notify(&self)->Result<(),NotifyError>` | ✓ |

`CallbackData { self: *mut Thread, io: *mut Termio<M>, data: ThreadData<M> }` is the
on-stack shuttle. **`data` must be a stable pointer for the thread's lifetime** — the
reference comment (`thread.rs:344-347`) says so ("created on the stack here so that it has
a stable pointer throughout the lifetime of the thread"). In Rust it is a local in
`threadMain_` that we take `addr_of_mut!` of and hand to `threadEnter`/the callbacks as
`*mut`; it never moves because `threadMain_` owns the frame until `loop.run` returns. All
callbacks are **ZST fn-items / non-capturing closures** (the xev `wait`/`run`/`reset` cb is
`const { size_of::<CB>()==0 }`, e.g. `async.rs:235-240`) — so each callback is a free `fn`
or `|…| {…}` capturing nothing, recovering all state through the `*mut CallbackData`
userdata. This is the only faithful shape (Zig passes a comptime `struct{fn}`).

### termio `termio_impl.rs`
- `Termio::init` (`:446`, deferred body `:355-445`): construct the `StreamHandler`
  (`terminal_stream = .initAlloc(alloc, handler)`), fill `self.*`. StreamHandler's writer
  methods are **already live** (see §4). **[ADV-FIX F1] NOT "pure struct assembly, already
  pinned" — the handler's raw ptrs (`terminal`/`size`/`termio_mailbox`) point into the owning
  Termio, but the current seam returns `Termio` BY VALUE and the surface `ptr::write`s it into
  `(*self_).io`, so those ptr values dangle at the old return slot.** MANDATORY: change `init` to
  the in-place out-param form `pub unsafe fn init(self_: *mut Termio<M>, …) -> Result<()>`
  (faithful to the reference `init(self: *Termio, …)`), writing fields via `ptr::write` and setting
  `handler.{terminal,size,termio_mailbox}` to `&(*self_).{terminal,size,mailbox}` at the FINAL
  address — OR keep the by-value return and re-seat those three handler ptrs after the surface's
  `ptr::write` (surface.rs:2143). Prefer the in-place form (matches the reference + surface's own
  `renderer_state.terminal = &raw mut (*self_).io.terminal` at surface.rs:1920). `renderer_wakeup:
  xev::Async` is `Copy`.
- `Termio::threadEnter` (`:531`, body `:477-530`): builds `ThreadData { loop_: &mut
  thread.loop_ as *mut xev::Loop, … , backend: undefined }`, calls
  `self.backend.threadEnter(alloc, self, data)`, then `queueWrite`s prepared inputs. The
  `loop_` field is `*mut xev::Loop` (`termio_impl.rs:1196`) — a raw pointer into the
  Thread's owned `loop_` (`thread.rs:87`, moved onto the io thread). `backend: undefined`
  is the out-parameter protocol: `Exec::threadEnter` writes `td.backend = .{.exec=…}`.
- `Termio::processOutput` (`:1049`) + `processOutputLocked` (`:1108`): **must un-defer** —
  the ReadThread calls `Termio::processOutput(io, buf[0..n])`. Body: lock renderer mutex
  (`rustty_zigstd::thread::Mutex` — guard-less, core C0), `terminal_stream.nextSlice(buf)`
  (rustty-vt, live), `handler.queueRender()` (live, §4), `mailbox.notify()` if
  `termio_messaged`. Uses `renderer_mailbox.push(.{.reset_cursor_blink}, .instant)` (live).
  No xev call except `queueRender`→`Async::notify` (live) and `mailbox.notify()`→
  `Async::notify` (live). The inspector-slow-path branch (`renderer_state.inspector`) stays
  as-is (inspector is a live seam already).

### termio `exec.rs`
- `Exec::threadEnter` (`:303`, body `:194-302`): the spine. Calls `subprocess.start(alloc)`
  (§2), then:
  - `xev::Process::init(cmd.pid orelse ProcessNoPid)` — `process.rs::Process::init(pid_t)`
    ✓. Flatpak arm is const-false (stays comment).
  - `internal_os.pipe()` (the read-thread quit pipe) — rustty-os pipe, live (not xev).
  - `xev::Stream::init_fd(pty_fds.write)` — `stream.rs::Stream::init_fd(fd_t)` ✓.
  - `xev::Timer::init()` (termios_timer) — `timer.rs::Timer::init()` ✓.
  - `std::thread::spawn(ReadThread::threadMainPosix, (pty_fds.read, io, pipe[0]))` (§3).
  - `td.backend = .{.exec = ThreadData{…}}` — fills `exec::ThreadData` (`exec.rs:678-741`).
  - `process.wait(td.loop, &td.backend.exec.process_wait_c, ThreadData, td, processExit)` —
    `process.rs::Process::wait::<UD,CB>` ✓; cb matches `processExit` (`exec.rs:445-454`).
  - `termios_timer.run(td.loop, &…termios_timer_c, TERMIOS_POLL_MS, ThreadData, td,
    termiosTimer)` — `Timer::run` ✓.
- `Exec::threadExit` (`:350`, body `:316-349`): `subprocess.stop()` + write "x" to
  `read_thread_pipe` + `exec.read_thread.join()`. No xev; pure posix + `JoinHandle::join`.
- `Exec::focusGained` (`:392`, body `:359-391`): termios-timer flag flip; calls
  `termiosTimer(td, undefined, undefined, {})` directly (state check). `Completion::state_of`.
- `processExit`/`processExitCommon`/`flatpakExit`/`termiosTimer` (`:414-550`): the exit +
  termios callbacks. `processExitCommon` pushes `td.surface_mailbox.push(.child_exited,
  .forever)` (live). `termiosTimer` rebuilds a `Pty{master,slave}` to `getMode()` (rustty-os,
  live), pushes `.password_input` on change, re-arms via `Timer::run`.
- `Exec::queueWrite` (`:621`, body `:555-620`) + `ttyWrite` (`:634-654`): the pty write.
  Chunks `data` into `write_buf_pool.getGrow()` buffers (CRLF-rewrite when `linefeed`),
  then `exec.write_stream.queueWrite(td.loop, &exec.write_queue, req, .{.slice=slice},
  ThreadData, exec, ttyWrite)` — `stream.rs::Stream::queue_write::<UD,CB>(self,*mut Loop,
  *mut WriteQueue,*mut WriteRequest, WriteBuffer, *mut UD, CB)` ✓; buf is
  `WriteBuffer::Slice(...)` (kqueue.rs:1737) over the pool buffer (stable during the write).
  `ttyWrite` returns pooled req/buf via `write_req_pool.put()/write_buf_pool.put()`. cb
  matches (`exec.rs:634-641`) ✓.
- `ReadThread::threadMainPosix` (`:1995`, body `:1892-1992`): **NOT xev** — a raw `poll(2)`
  loop over `[pty_fd, quit_fd]` doing blocking `read` + `Termio::processOutput`. See §3.
- `Subprocess::start` (`:1542`) — §2.

### termio `mailbox.rs` / `stream_handler.rs`
Already un-deferred (core C-T). `mailbox::send` (`mailbox.rs:122-183`) is live (guard-less
mutex model). `StreamHandler::{queueRender, surfaceMessageWriter, rendererMessageWriter}`
(`stream_handler.rs:212/256/307`) are live. See §4 — nothing to land, but they go LIVE.

### core `surface.rs` — the Async::notify leaves
Already live calls: `renderer_thread.wakeup.notify()` (`:3756/3817/3860/5573`),
`io_thread.stop.notify()` (`:2512`), `renderer_thread.stop.notify()` (`:2489`),
`state.stop.notify()` (`:509`), `state.wakeup.notify()` (`:10483`). These invoke the real
`async.rs::Async::notify` today; Milestone B changes only *who is listening* (the io
thread's own `stop`/mailbox-`wakeup` Asyncs are now pumped). **No core edit required.**

**Gap analysis: none.** Every deferred termio body maps cleanly to a landed
`Loop`/`Async`/`Timer`/`Process`/`Stream`/`WriteQueue`/`Completion` decl with a
matching callback ABI.

---

## §2 — The subprocess-spawn self-reference (the hard spot), resolved

### The reference shape (Exec.zig:1000-1061)
```
var cmd: Command = .{ .path=self.args[0], .args=self.args, .env=&self.env.?,
                      .cwd=cwd, .stdin/out/err=pty.slave, .data=self, … };
cmd.start(alloc);                 // fork+exec; child runs os_pre_exec→getData(Subprocess)
self.process = .{ .fork_exec = cmd };   // store cmd back INTO self
// defer: close pty.slave; if (self.env) |*e| { e.deinit(); self.env=null; }  ← frees env
```
So the stored `cmd` (a) borrows `self.args`/`self.env`, (b) holds `data = self`
(`*Subprocess`), and (c) is embedded back into `self.process`. In Zig these are raw arena
pointers and the arena (owned by `Subprocess`) outlives them. In Rust `Command<'a>`'s
fields are `&'a` references (`command.rs:185/192/198`) — storing a `Command<'static>` that
actually points into the non-`'static` `Subprocess` is instant Stacked-Borrows UB (the
`rust-mut-args-strongly-protected` rule), even before any deref, and even more so because
the `defer` frees `self.env` while the stored `cmd.env` still "references" it.

### Decision: scoped-borrow spawn + a pid-bearing stored `Command` (the "pid-only handle"
### the deferral authorized — dressed in `Command`'s clothes so the match arms stay faithful)

The `fork_exec` payload stays `Command<'static>` (byte-faithful shape: `stop`/`killCommand`/
`deinit`/`externalExit` match `Process::ForkExec(cmd)` unchanged), but its reference fields
are **genuinely-`'static` empty placeholders**, and only `pid` carries live data. The real
borrows exist only as **locals in `start()`**, alive exactly for the `cmd.start()` call —
which is the only time they are read.

```rust
pub fn start(&mut self, alloc: Allocator) -> Result<PtyFds, crate::Error> {
    assert!(self.pty.is_none() && self.process.is_none());
    let mut in_child = false;
    // … Pty::open, self.pty = Some(pty), errdefer/defer as in the verbatim body …

    // Build the transient &[&CStr] VIEW over self.args (Vec<CString>) — a stack local,
    // valid for this frame only. This is the Rust materialization of the Zig
    // `[]const [:0]const u8` slice; nothing post-start reads it.
    let argv: Vec<&CStr> = self.args.iter().map(|c| c.as_c_str()).collect();
    let cwd_bytes: Option<&[u8]> = cwd_checked.as_deref();       // access()-validated cwd

    let self_ptr: *mut Subprocess = self;   // RAW ptr for the os_pre_exec callback
    let mut cmd: Command = Command {         // Command<'a>, 'a = this frame
        path: argv[0],
        args: &argv,
        env: self.env.as_ref(),
        cwd: cwd_bytes,
        // [ADV-FIX F3] pty_slave_file is a ManuallyDrop<File> (or the fd wrapped non-owningly):
        // Command reads only .as_raw_fd(), never closes. The slave fd is closed ONCE by the
        // verbatim `posix.close(pty.slave)` in the success-defer / failure-errdefer below — a
        // dropping File here would double-close (fd-reuse race).
        stdin: Some(&*pty_slave_file), stdout: Some(&*pty_slave_file), stderr: Some(&*pty_slave_file),
        os_pre_exec: Some(subprocess_pre_exec),   // fn(&mut Command)->Option<u8>
        data: Some(self_ptr.cast::<c_void>()),
        rt_pre_exec: …, rt_pre_exec_info: self.rt_pre_exec_info, … , pid: None,
    };

    match cmd.start(alloc) {   // reads path/args/env/cwd/data; forks; child getData→self
        Err(e) if is_exec_failed_in_child(&e) => { in_child = true; return Err(e); }
        Err(e) => return Err(e),
        Ok(()) => {}
    }
    let pid = cmd.pid;         // the ONLY field alive past here

    // Store the pid-bearing handle. All reference fields are TRUE 'static empties —
    // no dangling borrow can exist. killCommand(&mut Command) reads only `pid`.
    self.process = Some(Process::ForkExec(Command {
        path: c"",             // &'static CStr
        args: &[],             // &'static [&'static CStr]
        env: None, cwd: None, stdin: None, stdout: None, stderr: None,
        os_pre_exec: None, rt_pre_exec: None,
        rt_pre_exec_info: self.rt_pre_exec_info,
        rt_post_fork: None, rt_post_fork_info: self.rt_post_fork_info,
        data: None,
        pid,                   // ← the live value
    }));
    // defer: close pty.slave; self.env = None (drops EnvMap) — sound: no stored borrow of it.
    Ok(PtyFds { read: pty.master, write: pty.master })
}
```

### Ownership / lifetime / why it is sound
- **No long-lived `&mut` or `&'static`-to-non-static.** The real `&self.args`/`&self.env`
  borrows live only inside `start()`'s frame (Rust NLL scopes them to `cmd`'s use). The
  stored `Command`'s references are actual `'static` empties (`c""`, `&[]`, `None`), so the
  self-referential aliasing that made this the hard spot **no longer exists at rest.**
- **The `data = self` self-pointer is a raw `*mut c_void`**, dereferenced only in the forked
  child (`subprocess_pre_exec` → `getData::<Subprocess>()` → `childPreExec`,
  `exec.rs:1557-1564`), post-fork/pre-exec, where the parent's borrows are irrelevant (Rust/Miri
  never sees the child's mutation because `fork` copies the address space). This matches the
  `vtc-effects-trampoline-seam` raw-ptr container_of discipline. **[ADV-FIX F4]** the borrow that
  actually coexists with `self_ptr` is `env: self.env.as_ref()` (a `&` of self), NOT `args` (which
  is materialized into the local `argv`). Derive `self_ptr` via `addr_of_mut!((*self).…)` /
  `self as *mut _` BEFORE creating any `&self.*` field borrow, and order the `cmd` struct-literal
  field evaluations so no `&self.env`/`&self.rt_*_info` borrow is live across the `self_ptr`
  creation — keeping the write-capable raw pointer and the shared `&self.env` from being
  simultaneously live (B1's Miri Stacked-Borrows run validates this).
- **Pinning:** `Subprocess` is reached through `Termio.backend.Exec`, and `Termio` is
  already heap-pinned by the surface (`IoThreadArgs`, the embedded-Surface precedent). The
  `argv` VIEW does not need `Subprocess` pinned (it is dropped before `start` returns); the
  stored handle holds no pointer into `Subprocess`, so no pin obligation is introduced
  beyond what already exists. This is strictly *less* pinning than a heap-box variant.

### Alternative considered — heap-box + raw-ptr `Command` variant (rejected as heavier)
Give `Subprocess` a boxed `SpawnStorage { args, env }` with a stable address and add a
`Process::ForkExecRaw` whose `Command` holds `*const`/`*mut` into it. This preserves the
reference's *stored pointer values* verbatim, but: (a) forces a `Command` type fork
(`&'a` → raw) that ripples into `command.rs`'s live tests, and (b) adds a genuine
self-referential pin obligation on `Subprocess` for zero behavioral gain (the pointers are
never read post-start). Choose it ONLY if a future reference version starts reading
`cmd.args`/`env` after `start()`. It does not today. The pid-bearing handle is the minimal
sound model and is exactly the "pid-only handle" the deferral note (`exec.rs:1350`) named.

---

## §3 — Thread lifecycle + Send

Three threads, two of them new-this-milestone. The reference topology (Thread.zig doc):
- **Parent (surface) thread** runs `Thread::init` (`thread.rs:150`) — creates `loop_`,
  `stop`, three `Timer`s. The `Loop` is created here but **MOVED onto the io thread**.
- **IO/writer thread** runs `Thread::threadMain(&mut self, &mut io)` — owns the moved
  `Loop`, runs `loop.run(.until_done)`. Spawned by core `surface.rs:2264` (already wired).
- **Reader thread** (`io-reader`) runs `ReadThread::threadMainPosix(pty_read_fd, io_ptr,
  quit_fd)` — a raw `poll`+`read` loop, **NOT xev**, spawned by `Exec::threadEnter`
  (`exec.rs:248`).

### Send mapping (the landed unsafe-Send discipline)
- `Loop`: `unsafe impl Send for Loop` (`kqueue.rs:307`) — the move-onto-thread is sound
  because the loop is single-thread-affine after the move (XEV_BLUEPRINT §2 F2).
- `Async` (`stop`, mailbox `wakeup`): `#[derive(Clone,Copy)]` + auto-`Send` (bare
  `port: u32`, `async.rs:109-113`). The parent keeps a `Copy` of `stop`/`wakeup` to
  `notify()` cross-thread (KAT 1 proved exactly this round-trip).
- `Completion`/`WriteQueue`/`WriteRequest`: `unsafe impl Send` (`kqueue.rs:1549`,
  `stream.rs:168`) — single-loop-thread affinity after the `ThreadData` move.
- The **spawn wrapper is already the precedent**: `IoThreadArgs{ thread: *mut Thread, io:
  *mut Termio<M> }` + `unsafe impl Send` (`surface.rs:1440-1446`), whole-struct capture
  into the `move ||` closure. The ReadThread spawn needs its own tiny Send wrapper for
  `io: *mut Termio<M>` (a `!Send` raw ptr): a `ReadThreadArgs<M>(RawFd, *mut Termio<M>,
  RawFd)` newtype with `unsafe impl<M> Send`, documented with the same "sole user until the
  quit-pipe join" contract. The reader thread is the sole toucher of `*io` for
  `processOutput` between spawn and `threadExit`'s `join()`.

### Thread-data stability (the crux for the callbacks)
`CallbackData` (thread.rs) and the per-thread `ThreadData<M>` (`exec.rs` writes
`td.backend.exec`) must have **stable addresses for the loop's life** because the loop
re-enters `&td.backend.exec.process_wait_c`, `&…write_queue`, etc. as raw completion
pointers across ticks. They live as **locals in `threadMain_`'s frame**, which does not
return until `loop.run` finishes — exactly the reference's "stack here so it has a stable
pointer" (thread.rs:344). No `Box` needed; the frame IS the pin. All raw `*mut` into them
follow `rust-mut-args-strongly-protected` (never reborrowed `&mut` across a tick).

### [ADV-FIX F2] Writer + processOutput must be raw-pointer, not `&mut` (protected-aliasing UB)
The reference threads raw pointers on BOTH shared paths: `threadMain(self: *Thread, io:
*termio.Termio)` and `threadMainPosix(fd, io: *termio.Termio, quit)`. The port already made the
reader raw (`threadMainPosix(_io: *mut Termio<M>)`) but LEFT the writer as
`Thread::threadMain(&mut self, _io: &mut Termio<M>)` (thread.rs:310) and
`Termio::processOutput(&mut self)` (termio_impl.rs:1049). A `&mut` argument gets a protected
`Unique` retag for the whole call; `threadMain` blocks in `loop.run(.until_done)` until shell exit,
so during that window the ReadThread's `processOutput(&mut *io)` aliases the same Termio and the
parent's `self.io_thread.stop.notify()` (surface.rs:2511) reads `self.io_thread` before the join —
both protected-aliasing UB. MANDATORY: retype `Thread::threadMain(self_: *mut Thread, io: *mut
Termio<M>)` and `Termio::processOutput(io: *mut Termio<M>, …)` to RAW pointers (matching the
reference), forming only transient, non-overlapping `&mut`/`&` under the renderer mutex inside the
body. Update the two spawn/call sites (surface.rs:2270 `(*args.thread).threadMain(args.io)`) to pass
raw. This is a seam-signature change (do it in B4).

### The read path (raw pty read, not xev)
`ReadThread::threadMainPosix` sets the pty fd `O_NONBLOCK`, builds `pollfd[2] = {pty, quit}`,
then loops: tight `read(pty, buf)` until `WouldBlock`/`n==0` (each chunk →
`Termio::processOutput(io, &buf[..n])`), then `poll(-1)`; exits on the quit fd readable or
pty `POLLHUP`. `processOutput` takes the renderer mutex and drives `terminal_stream` — so
the reader thread mutates terminal state under the lock while the writer thread's callbacks
also take that lock. This is the reference's two-thread + shared-mutex model, verbatim.

### The write path (xev-driven)
Writer-thread only: mailbox `write_*` messages → `drainMailbox` → `Termio::queueWrite` →
`Exec::queueWrite` → `Stream::queue_write` (the ordered-write engine, `stream.rs:404`),
partial-write re-arm handled inside `queue_write_trampoline`. `ttyWrite` recycles the pool
slots. Ordering is guaranteed by the single `WriteQueue`.

---

## §4 — mailbox / mutex + stream_handler (already landed; goes LIVE here)

**These do NOT re-land in Milestone B — they were un-deferred at core C-T and are already
real code.** Milestone B is the first time they are *exercised on a live loop*:
- `mailbox::send(&mut self, msg, Option<*mut zigstd::thread::Mutex>)` (`mailbox.rs:122`):
  the caller-held-mutex fast/slow path is fully ported — instant `push_or_return`, else
  `wakeup.notify()` + `(*m).unlock()` + blocking `push(Forever)` + `(*m).lock()`. The
  guard-less `rustty_zigstd::thread::Mutex` (core C0) is what makes the callee unlock/relock
  expressible. Live now because the io thread actually drains the queue.
- `StreamHandler::rendererMessageWriter`/`surfaceMessageWriter`/`queueRender`
  (`stream_handler.rs:212/256/307`): fully ported, including the
  `renderer_mailbox.push_or_return(.instant)` → unlock → `push(.forever)` slow path and the
  `renderer_wakeup.notify()` leaf. The **renderer mailbox is real** (rustty-renderer's
  `Thread::Mailbox`), even though the renderer *loop* is Phase-3 — pushes queue fine and the
  notify fails-safe. The **core `Timeout` is resolved** (`blocking_queue::Timeout`, a real
  datastruct type, `apprt-api surface.rs:226`). So: **nothing here stays deferred.** The
  only genuinely-new leaf behavior is that `processOutput`/`drainMailbox` now *call* them.

Justification for "no partial deferral remains": every mailbox/mutex/message-writer body is
already `unimplemented!()`-free (verified: the only `unimplemented!` in mailbox.rs /
stream_handler.rs message-writer paths is zero — the remaining `stream_handler`
`unimplemented!` at `:445` is the tmux DCS `TmuxCommand=()` stub, out of scope and unrelated
to IO wiring).

---

## §5 — The LIVE-SHELL KAT (the milestone proof)

**Crate:** `crates/rustty-termio/tests/live_shell.rs` (a termio *integration test* — it needs
the whole Exec→pty→xev→terminal stack; a unit `#[cfg(test)]` mod would work too, but an
integration test best matches "first time the port runs a real terminal" and isolates the
pty-spawning process from the in-process unit suite — see the parallel-cargo-test lesson in
§7). `#[cfg(target_os = "macos")]`.

**What it proves:** real `/bin/sh` in a real pty, driven through the real termio Exec path
and the real rustty-xev loop, echoes a command whose output lands in the terminal `Screen`.

**Shape (deterministic, no hangs):**
```
1. Build a minimal Termio<TestMailbox> via Termio::init (TestMailbox = a trivial AppMailbox
   test double that records pushes; renderer_state = a standalone State with a real Terminal;
   renderer_mailbox = a throwaway renderer Thread::Mailbox; renderer_wakeup = Async::init()).
   Backend = Exec with Config{ command: Some("/bin/sh"), term:"xterm-ghostty", … }.
2. let mut thread = Thread::init()?;  let mut td: MaybeUninit<ThreadData> stand-in via the
   threadMain_ frame — BUT for the KAT we drive the pieces directly to stay bounded:
     io.threadEnter(&mut thread, &mut cb.data)   // starts sh, spawns ReadThread, arms watchers
     mailbox.wakeup.wait(...) ; thread.stop.wait(...)   // as threadMain_ does
3. Write a command via the mailbox: io.queueMessage(.write_stable(b"echo RUSTTY_OK\n"), .unlocked)
   (this notifies wakeup; the writer loop drains → queueWrite → Stream::queue_write).
4. PUMP BOUNDED: for _ in 0..DEADLINE_TICKS {           // DEADLINE_TICKS ~ 2000
       thread.loop_.run(RunMode::NoWait)?;              // non-blocking single pass
       { lock renderer_state.mutex; if terminal.screen contains b"RUSTTY_OK" { break } }
       if elapsed > 3s { panic!("live-shell KAT timed out") }
       std::thread::sleep(1ms);                          // yield to the ReadThread
   }
5. assert the active Screen's row text contains "RUSTTY_OK" (round-trip: pty write → sh echo
   → pty read → ReadThread → processOutput → terminal_stream → Screen).
6. Teardown: thread.stop.notify(); io.threadExit(&mut cb.data) (writes quit pipe, joins
   ReadThread); subprocess stop kills sh; thread.deinit().
```

**Determinism / hang-avoidance:**
- **Never `run(.until_done)` in the KAT** — that blocks until sh exits. Use bounded
  `run(NoWait)` passes + a wall-clock deadline + a tick cap. Both a satisfied-assertion
  break and a deadline panic terminate.
- The ReadThread runs concurrently; the 1ms sleep between pumps gives it time to `read` and
  `processOutput`. The assertion polls under the renderer mutex, so it observes committed
  Screen state.
- Send `echo …\n` via `write_stable` (a `'static` byte slice — the mailbox's stability
  contract, `mailbox.rs:46-48`), avoiding alloc-lifetime concerns.
- `/bin/sh` is guaranteed on macOS (unlike `/bin/true`, which the xev KAT 3 already
  canonicalized to `/usr/bin/true`). Force a dumb, quiet shell: `sh -c 'echo RUSTTY_OK'` as
  the *command* config so it prints and exits, OR keep interactive `sh` and write the line —
  prefer the former for tightest determinism (bounded output, self-terminating), then a
  second KAT variant with interactive write if a two-way round-trip is wanted.

A second, smaller KAT ("write round-trip only") can assert `Stream::queue_write` of `"exit\n"`
reaches sh (observed via `child_exited` on the test mailbox) — but the echo KAT already
covers the full read+write loop, so one KAT suffices for the milestone.

---

## §6 — Test accounting + acyclicity

- **No termio deferred *tests* un-defer** — every deferred body is a `0 reference test{}`
  deferral (thread.zig has 0 `test{}`; the Exec.zig xev bodies are inside methods with no
  `test{}`). So filling the bodies changes **zig_count by 0**. termio stays **≥ 44**.
- **The gate is `rust_count >= zig_count`** (`check_test_counts.py:5-10`) — rustty-only
  additions are always allowed as margin. The live-shell KAT is a **rustty-only +1**
  (integration test, not a mapped Zig `test{}`), exactly like the xev KATs 1-4 are margin
  over libxev's `test{}` count. So termio lands at **44 unit tests (unchanged) + the
  live-shell KAT(s)** as margin. **termio does NOT shrink and is not required to grow past
  44** — but it MAY (and does, by the KAT). No mapping table edit needed for the KAT
  (integration tests under `tests/` are not part of the per-file `RSTIO` src mapping).
- If Milestone B adds any small rustty-only unit helpers (e.g. a `ReadThreadArgs` Send
  smoke), they are margin too. Do not add fake `test{}`-parity entries.
- **Acyclicity:** termio already depends on rustty-xev (`Cargo.toml:45`, via
  `xev_seam.rs::pub use rustty_xev::*`). No new crate edge is introduced (the ReadThread
  uses `libc`/`std`, already deps). `check_acyclic.sh` / `cargo_cycle_check.py` stay green —
  the `[ADV-FIX F6]` note (`lib.rs:206`) already established there is **no** `global→xev`
  back-edge, so wiring termio bodies cannot create one.

---

## §7 — Chunked execution plan + risks

**B1 — Subprocess.start (the hard spot first, in isolation).** Land §2: the scoped-borrow
spawn + pid-bearing stored `Command`. Add a rustty-only unit KAT: `Subprocess::init` →
`start` a `sh -c 'exit 7'` on a real pty → assert `PtyFds` returned, `process` is
`ForkExec(pid)`, then `stop()` reaps. Run under Miri-excluded (real fork/exec) but
Stacked-Borrows-clean for the parent-side borrow scoping. This de-risks the UB spot before
any thread wiring.

**B2 — Exec::threadEnter/threadExit/queueWrite/ttyWrite + ThreadData assembly.** Wire the
xev `Process`/`Stream`/`Timer` handles + the write engine. No thread yet — drive
`threadEnter` then a manual `loop.run(NoWait)` + a direct `queueWrite` in a unit KAT,
asserting bytes reach the pty (read the slave side directly).

**B3 — ReadThread + processOutput.** Land the raw `poll`/`read` loop and
`processOutput`/`processOutputLocked`. Unit KAT: feed the pty master a byte string from the
test, assert it lands in the Screen (no shell yet — just the read path).

**B4 — Thread::threadMain run loop + callbacks.** Land `threadMain_`, `drainMailbox`, and
the timer/async callbacks. Now the mailbox→writer path is whole.

**B5 — The live-shell KAT (§5).** The culmination: real sh, full round-trip, bounded pump.

**B6 — Green-up:** `check_test_counts.py` (≥44 + KAT margin), `check_acyclic.sh`,
scoped-G3b comment coverage on every touched body (each un-deferred body must keep its
verbatim reference comments — the bodies already carry them, so this is verifying the
`unimplemented!()` line is replaced *below* the preserved comment, not deleting the comment).

### Risks
1. **Self-ref spawn UB (highest).** Mitigated by B1-first + the scoped-borrow model (§2): no
   `&'static`-to-non-static ever exists at rest; the only raw self-pointer (`data`) derefs in
   the forked child. Validate the parent side under Miri's Stacked Borrows for the
   `init`/store path (the fork itself is Miri-excluded — document in MIRI_EXCEPTIONS.md, the
   xev-KAT precedent).
2. **Thread-lifecycle races.** The `CallbackData`/`ThreadData` stable-pointer contract is
   frame-pinned (§3); the loop is single-thread-affine post-move. The one genuine cross-
   thread edge is the ReadThread mutating terminal state under `renderer_state.mutex` while
   writer callbacks also lock it — this is the reference's design; correctness rests on the
   guard-less `zigstd::thread::Mutex` being a real `pthread_mutex`. Teardown ordering
   (`stop.notify` → writer loop ends → `threadExit` writes quit pipe → ReadThread `join`)
   must match Exec.zig:316-349 exactly, or the reader hangs on a noisy child — hence
   `threadExit` writes the quit pipe *before* `join`.
3. **pty-spawning KAT isolation (parallel-cargo-test lesson).** A KAT that forks a real shell
   in a shared test binary can race other tests' global state (the onig_initialize lesson:
   parallel `cargo test` exposes non-thread-safe C init). Put the live-shell KAT in its OWN
   integration test file (`tests/live_shell.rs` = its own process), and/or gate the
   in-process spawn KATs behind a serialized guard. Do not let two pty-spawning tests share a
   process without serialization.
4. **Determinism / hangs.** Never `run(.until_done)` in a KAT (blocks on shell life); always
   bounded `run(NoWait)` + wall-clock deadline + tick cap + explicit teardown (§5). Every KAT
   must terminate on both success and timeout. Prefer self-terminating `sh -c 'echo …'` over
   interactive sh.
5. **notify to an unpumped renderer loop.** `renderer_wakeup.notify()` from the io thread
   targets an Async whose loop is Phase-3. This is fine (non-blocking mach send, fails-safe),
   but the KAT must not *assert* on renderer-side effects — assert only on the terminal
   `Screen`, which the read path fills directly.

This is the culmination: after B5, the port runs a real terminal.
