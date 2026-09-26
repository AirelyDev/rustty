//! xev Milestone B — B5: the LIVE-SHELL KAT (the culmination).
//!
//! This is the FIRST TIME THE PORT RUNS A REAL TERMINAL. A real `/bin/sh`
//! executes a command in a real pty, driven through the real termio Exec path
//! (fork/exec + xev `Process`/`Stream`/`Timer` wiring + a real `ReadThread`
//! poll/read loop) and the real rustty-xev event loop — the kqueue `Api()` on
//! macOS, the io_uring/epoll `Dynamic` union on Linux (L1 LT9); the shell's
//! output round-trips back through `Termio::processOutput` -> `terminal_stream`
//! and lands in the terminal `Screen`.
//!
//! ON LINUX this KAT is the keystone proof of the whole L1 event-loop port: the
//! `Dynamic` backend it selects (io_uring, falling back to epoll) is what
//! actually carries the live shell's `Process`/`Stream`/`Timer` completions.
//!
//! Blueprint: .port/phase-2/XEV_MILESTONE_B_BLUEPRINT.md §5.
//!
//! WHY AN INTEGRATION TEST (its own process): a KAT that `fork()`s a real shell
//! in a shared test binary can race other tests' global state (the
//! `parallel-cargo-test`/`onig_initialize` isolation lesson, blueprint §7 risk
//! 3). An integration test file compiles + runs as its OWN process, isolating
//! the pty-spawning shell from the in-process unit suite (whose own B1/B2/B3
//! fork KATs are serialized by a `FORK_KAT_SERIAL` mutex WITHIN that binary). A
//! `fork()` snapshots only the FORKING process's fds, so this separate-process
//! shell spawn cannot leak — or be leaked into by — the unit binary's forks; no
//! cross-binary serialization is needed. This is the sole test in this binary.
//!
//! DETERMINISM (blueprint §5 + §7 risk 4): NEVER `loop.run(.until_done)` here —
//! that blocks until the shell exits. We use a self-terminating command
//! (`sh -c 'echo RUSTTY_OK'`, blueprint §5's PREFERRED form: bounded output,
//! self-terminating) and a BOUNDED pump — non-blocking `run(NoWait)` passes + a
//! tick cap + a wall-clock deadline + explicit teardown — so the KAT terminates
//! on BOTH a satisfied assertion and a timeout, never hanging on the child's
//! life.
//!
//! WHAT THIS PROVES vs. WHAT IS ALREADY COVERED: the unique milestone proof here
//! is that a REAL SHELL interprets a command whose output round-trips into the
//! `Screen` (spawn -> sh -> pty -> ReadThread -> processOutput ->
//! terminal_stream -> Screen). The WRITE path (queueWrite -> `Stream::queue_write`
//! -> pty) is already exercised end-to-end by the committed B2 KAT
//! (`threadenter_queuewrite_writes_to_pty`, a `/bin/cat` write round-trip), so a
//! self-terminating command that needs no stdin write is both the most
//! deterministic proof AND avoids re-proving B2. The mailbox->drain->queueWrite
//! hop the reference uses for cross-thread writes is driven by the LIVE
//! `Thread::threadMain` io-thread (which `run(.until_done)`-blocks and arms the
//! private `wakeupCallback` mailbox watcher in `threadMain_`); from a bounded
//! integration-test pump that private watcher cannot be armed, so a queued
//! mailbox write would never drain here — another reason the self-terminating
//! shell (no write) is the correct shape for this bounded KAT.
//!
//! Real fork/exec + openpty + kqueue syscalls -> Miri-excluded
//! (.port/phase-2/MIRI_EXCEPTIONS.md, "rustty-termio B5"); the raw-pointer
//! `Termio`/read-thread aliasing follows the [ADV-FIX F1]/[ADV-FIX F2]
//! discipline (heap-pinned `Termio` with re-seated handler self-pointers; the
//! read thread is the sole concurrent `*mut Termio` toucher, via the raw-`*mut`
//! `processOutput`, under the renderer mutex).

// This whole file compiles on every unix (L1 LT9 widened the gate from
// `target_os = "macos"`): the spawn path is POSIX fork/exec + openpty, and the
// event loop underneath is now `rustty_xev`'s per-target surface — the kqueue
// `Api()` on macOS, the io_uring/epoll `Dynamic` union on Linux — reached through
// the SAME `xev_seam` names on both. On non-unix it is an empty test binary (0
// tests) — margin, no test-count regression.
#![cfg(unix)]
// PARITY: the reference termio identifiers are camelCase (`threadEnter`,
// `queueWrite`, ...); the KAT calls them verbatim per R1/R2.
#![allow(non_snake_case)]

use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use allocator_api2::alloc::Global;

use rustty_termio::exec::{Config, Exec};
use rustty_termio::termio_impl::{DerivedConfig, ThreadData};
use rustty_termio::xev_seam as xev;
use rustty_termio::{Backend, Mailbox, Options, Termio, Thread};

/// A trivial `AppMailbox` test double that ACCEPTS every push and RECORDS the
/// count (blueprint §5 "TestMailbox = a trivial AppMailbox test double that
/// records pushes"). It is a ZST so it stays `Copy` — required by
/// `Termio::init`/`threadEnter` (`where M: Copy + 'static`; the reference copies
/// `apprt.surface.Mailbox` by value into both `self.*` and the handler). The
/// exit / termios / selection-scroll callbacks only `push(...)` to it, so a
/// counter double is enough (e.g. `processExitCommon` pushes `.child_exited`
/// when the shell exits — recorded but NOT asserted on, since whether the
/// bounded pump catches the exit before the assertion is satisfied is timing
/// dependent).
static SURFACE_PUSHES: AtomicUsize = AtomicUsize::new(0);

#[derive(Clone, Copy)]
struct TestMailbox;

impl rustty_apprt_api::surface::AppMailbox for TestMailbox {
    type Surface = ();

    #[allow(clippy::result_large_err)]
    fn push_or_return(
        &self,
        _msg: rustty_apprt_api::surface::Message<'static>,
        _timeout: rustty_datastruct::blocking_queue::Timeout,
    ) -> Result<rustty_datastruct::blocking_queue::Size, rustty_apprt_api::surface::Message<'static>>
    {
        SURFACE_PUSHES.fetch_add(1, Ordering::Relaxed);
        // Non-zero => "accepted" (matches a live queue's push return).
        Ok(1)
    }

    fn crash_surface(&self) -> *const dyn rustty_crash::sentry::ThreadStateSurface {
        &NO_SURFACE
    }
}

/// The surface `TestMailbox::crash_surface` reports (this KAT has no core
/// `Surface`; nothing here reads it).
struct NoSurface;

impl rustty_crash::sentry::ThreadStateSurface for NoSurface {
    fn screen_size(&self) -> (u32, u32) {
        (0, 0)
    }
    fn grid_size(&self) -> (u32, u32) {
        (0, 0)
    }
    fn cell_size(&self) -> (u32, u32) {
        (0, 0)
    }
}

static NO_SURFACE: NoSurface = NoSurface;

/// The heap-pinned harness backing one live `Termio<TestMailbox>` plus the
/// shared render state it points into. The `Termio` is heap-pinned because
/// `Termio::init` re-seats the `StreamHandler` self-pointers
/// (`terminal`/`size`/`termio_mailbox`) to fields of `*io` at their FINAL
/// address (the [ADV-FIX F1] discipline); moving the `Termio` afterward would
/// dangle them, so it stays boxed until every thread touching it is joined.
struct Harness {
    io: *mut Termio<TestMailbox>,
    state: *mut rustty_renderer::state::State,
    mutex: *mut rustty_zigstd::thread::Mutex,
    renderer_mailbox: *mut rustty_renderer::thread::Mailbox,
    renderer_wakeup: xev::Async,
}

impl Harness {
    /// Build a minimal, real `Termio<TestMailbox>` via the public
    /// `Termio::init` (the faithful in-place out-param initializer, [ADV-FIX
    /// F1]) — NOT hand-assembled: `init` builds the `StreamHandler`, the owned
    /// `Terminal`, and re-seats the self-pointers exactly as the surface does.
    fn build(command: rustty_config::command::Command) -> Harness {
        // The shared render `State` (heap-pinned; guard-less zigstd `mutex`; its
        // `terminal` is re-seated to `&io.terminal` after the Termio is pinned,
        // matching the surface's `renderer_state.terminal = &raw mut io.terminal`).
        let mutex: *mut rustty_zigstd::thread::Mutex =
            Box::into_raw(Box::new(rustty_zigstd::thread::Mutex::new()));
        let state: *mut rustty_renderer::state::State =
            Box::into_raw(Box::new(rustty_renderer::state::State {
                mutex,
                terminal: std::ptr::null_mut(),
                inspector: None,
                inspector_ptr: None,
                preedit: None,
                mouse: rustty_renderer::state::Mouse::default(),
                // D4 handoff (oracle d34b54e9b) - reference field defaults are `.init(0)`.
                demand: Default::default(),
                handoff_gen: Default::default(),
                handoff_park: Default::default(),
            }));

        // A throwaway renderer mailbox (pushes queue; the io thread's
        // `renderer_wakeup.notify()` fails-safe to an unpumped renderer loop,
        // blueprint §6 risk 5) and a real renderer-wakeup `Async` (Copy; shared
        // by the handler + the Termio).
        let renderer_mailbox: *mut rustty_renderer::thread::Mailbox =
            Box::into_raw(rustty_renderer::thread::Mailbox::create());
        let renderer_wakeup = xev::Async::init().expect("Async::init");

        // The Exec backend: run `command` on a real pty. No resources dir
        // (shell integration disabled), xterm-256color.
        let exec_cfg = Config {
            command: Some(command),
            env: rustty_termio::command::EnvMap::new(),
            env_override: rustty_config::RepeatableStringMap::default(),
            shell_integration: rustty_config::value_types::ShellIntegration::Detect,
            shell_integration_features:
                rustty_config::value_types::ShellIntegrationFeatures::default(),
            cursor_blink: None,
            working_directory: None,
            resources_dir: None,
            term: b"xterm-256color".to_vec(),
            rt_pre_exec_info: rustty_termio::command::RtPreExecInfo::default(),
            rt_post_fork_info: rustty_termio::command::RtPostForkInfo::default(),
        };
        let backend = Backend::Exec(Exec::init(Global, exec_cfg).expect("Exec::init"));

        // The full app config (only its `grapheme-width-method`/`scrollback-limit`
        // are read by `Termio::init`; it must outlive the borrow in `Options`) and
        // the derived config the handler reads its cursor/OSC fields from.
        let full_config = rustty_config::Config::default();
        let derived = DerivedConfig::init(&full_config);

        // Size: 800x480 screen / 10x20 cells => an 80x24 grid.
        let size = rustty_renderer::size::Size {
            screen: rustty_renderer::size::ScreenSize {
                width: 800,
                height: 480,
            },
            cell: rustty_renderer::size::CellSize {
                width: 10,
                height: 20,
            },
            padding: rustty_renderer::size::Padding::default(),
        };

        // The termio mailbox (SPSC; its own wakeup Async).
        let mailbox = Mailbox::initSPSC().expect("Mailbox::initSPSC");

        let opts = Options {
            size,
            full_config: &full_config,
            config: derived,
            backend,
            mailbox,
            renderer_state: state,
            renderer_wakeup,
            renderer_mailbox,
            surface_mailbox: TestMailbox,
        };

        // Heap storage for the Termio, initialized IN PLACE by `Termio::init`
        // (the out-param form) so the handler self-pointers target the final
        // heap address.
        let io: *mut Termio<TestMailbox> = Box::into_raw(Box::new(std::mem::MaybeUninit::<
            Termio<TestMailbox>,
        >::uninit()))
        .cast::<Termio<TestMailbox>>();
        // SAFETY: `io` is freshly-allocated, overwritable `Termio`-sized storage;
        // `init` writes every field. `full_config` outlives this call.
        unsafe { Termio::init(io, Global, opts) }.expect("Termio::init");

        // Point the shared render state's terminal at the Termio's terminal (the
        // surface's job; harmless here but keeps fidelity with the real wiring).
        unsafe {
            (*state).terminal = std::ptr::addr_of_mut!((*io).terminal);
        }

        Harness {
            io,
            state,
            mutex,
            renderer_mailbox,
            renderer_wakeup,
        }
    }

    /// The active `Screen` text, read UNDER THE RENDERER MUTEX — the same lock
    /// discipline the read thread's `processOutput` holds while it commits, so
    /// this observes only committed Screen state (blueprint §5 determinism).
    fn screen_dump(&self) -> Vec<u8> {
        unsafe { (*self.mutex).lock() };
        let dump = unsafe {
            (*(*self.io).terminal.screens.active).dump_string_alloc(
                Global.into(),
                rustty_vt::point::Point::Active(rustty_vt::point::Coordinate::default()),
            )
        };
        unsafe { (*self.mutex).unlock() };
        dump.unwrap_or_default()
    }

    /// Deinit everything. MUST be called only after the read thread (the sole
    /// concurrent `io` toucher) is joined by `threadExit`, so nothing
    /// dereferences `io` anymore: moving the Termio out of its box invalidates
    /// the handler self-pointers, sound only post-join.
    fn teardown(self) {
        // SAFETY: post-join, `io` is a fully-initialized `Termio` with no live
        // aliases; reconstitute the box (MaybeUninit<T> shares T's layout) and
        // consume it via `deinit`.
        let termio = unsafe { *Box::from_raw(self.io) };
        termio.deinit();
        unsafe {
            drop(Box::from_raw(self.state));
            drop(Box::from_raw(self.mutex));
            drop(Box::from_raw(self.renderer_mailbox));
        }
        // The renderer-wakeup Async is an externally-owned Copy handle (Termio
        // does not deinit it); release its mach port once here.
        let mut w = self.renderer_wakeup;
        w.deinit();
    }
}

// KAT (rustty-only margin — blueprint §7 B5; NOT a mapped Zig `test{}`, so the
// test-count contract is unaffected — an integration test under `tests/` is not
// part of the per-file `RSTIO` src mapping). The culmination: a real `/bin/sh`
// runs `echo RUSTTY_OK` in a real pty; its output round-trips through the whole
// live stack into the terminal `Screen`.
#[test]
fn live_shell_echo_round_trips_into_the_screen() {
    // dynamic.zig:18-19 — the documented startup call: pick the backend before
    // the first `Loop::init`. Linux-only, and deliberately so: `detect`/`backend`/
    // `CANDIDATES` are the `Dynamic`-specific decls `main.zig` forwards ONLY when
    // the candidate list has more than one entry. On macOS `candidates() ==
    // &.{.kqueue}`, so `Xev(bes)` short-circuits to the flat kqueue `Api()`
    // (dynamic.zig:36-38) and `detect` is comptime-elided — it does NOT exist
    // there (rustty-xev/src/lib.rs:333-334 gates the re-export to linux), hence
    // the cfg rather than an unconditional call. On Linux this selects io_uring
    // and falls back to epoll.
    #[cfg(target_os = "linux")]
    {
        xev::detect().expect("detect() must find an available backend");
        assert!(
            xev::CANDIDATES.contains(&xev::backend()),
            "detect() must land on a candidate backend"
        );
        eprintln!("live_shell: xev Dynamic backend = {:?}", xev::backend());
    }

    // Command: `/bin/sh -c 'echo RUSTTY_OK; /bin/sleep 1'` (blueprint §5 preferred
    // form — self-terminating, bounded output). On macOS this is login-wrapped
    // (`/usr/bin/login -flp <user> /bin/sh -c "..."`, the same exec-path B1/B2
    // spawn); login may emit a banner, but we only assert the Screen CONTAINS the
    // marker, so banner noise is harmless.
    //
    // WHY THE TRAILING `/bin/sleep 1` (L1 LT9 — this KAT must not race a REFERENCE
    // QUIRK; it is NOT a port bug and NOT a weakening of the proof):
    // `ReadThread.threadMainPosix` checks HUP BEFORE it re-drains —
    // `if (pollfds[0].revents & posix.POLL.HUP != 0) return;` (ghostty
    // Exec.zig:1355-1358, ported verbatim in exec.rs's read loop). The read thread
    // reaches its blocking `poll(-1)` before a bare `echo` child can write, and on
    // LINUX the pty master then reports POLLIN|POLLHUP in the SAME revents word
    // (measured: 0x11) — the kernel raises HUP as soon as the last slave fd closes,
    // even with bytes still buffered — so the reference returns and the pending
    // echo is dropped. (Verified directly: at that HUP return the pty still held
    // the unread bytes "RUSTTY_OK\r\n".) macOS does not hit this: there the dying
    // child surfaces as `read() == 0` (the `n == 0` note in the read loop), not
    // HUP, so the drain happens first — which is why this KAT passed on macOS with
    // a bare `echo`.
    //
    // Keeping the shell alive for a beat AFTER the write decouples POLLIN from
    // POLLHUP, so the read thread drains on a HUP-free wakeup exactly as on macOS.
    // The command stays a real, self-terminating shell with bounded output; the
    // sleep removes only the exit RACE, not any part of the proof.
    //
    // UPDATE (task tail-s6): the port no longer drops that output. The read
    // loop's outer poll now reads once more on POLLIN|POLLHUP (reference defect,
    // R13, UPSTREAM-DEFECTS.md UD19; `ReadThread::outer_poll_hup_exits` and its
    // tests in exec.rs), so the paragraphs above describe the reference. The
    // sleep stays: this KAT proves the live stack, not the UD19 handling, whose
    // Linux end-to-end run is still pending.
    //
    // `/bin/sleep` is spelled ABSOLUTE on purpose: this harness's `Config.env` is a
    // deliberately empty `EnvMap`, so there is no PATH for the shell to search — a
    // bare `sleep` resolves to "command not found", the child exits instantly, and
    // the POLLHUP race is back. (`/bin/sleep` exists on both Linux and macOS.)
    let command = rustty_config::command::Command::Direct(vec![
        b"/bin/sh".to_vec(),
        b"-c".to_vec(),
        b"echo RUSTTY_OK; /bin/sleep 1".to_vec(),
    ]);
    let h = Harness::build(command);
    let io = h.io;

    // The writer-thread event loop (created here, pumped here — the KAT stands
    // in for `threadMain_`'s frame, but bounded via `run(NoWait)` instead of
    // `run(.until_done)`). `Thread` must not move after `threadEnter` stores a
    // raw pointer to `thread.loop_` into the ThreadData.
    let mut thread = Thread::init().expect("Thread::init");

    // The frame-pinned ThreadData out-param storage. `Termio::threadEnter`
    // writes it whole (the reference's `data.* = .{...}` + the backend's
    // `td.backend = .{ .exec = ... }` out-param protocol); its address must be
    // stable for the loop's life (the loop re-enters completions stored inside
    // it across ticks — blueprint §3). A stack `MaybeUninit` that we never move
    // is that pin; `MaybeUninit` does not auto-drop, so teardown drops it
    // explicitly after `deinit`.
    let mut td_storage = std::mem::MaybeUninit::<ThreadData<TestMailbox>>::uninit();
    let td_ptr = td_storage.as_mut_ptr();

    // Start the subprocess + wire the xev `Process`/`Stream`/`Timer` handles +
    // spawn the REAL read thread, and fill `*td_ptr`.
    // SAFETY: `io` is heap-pinned + outlives the read thread (joined in
    // threadExit); `&raw mut thread` is the live owning Thread; `td_ptr` is the
    // frame-pinned out-param storage.
    unsafe { Termio::threadEnter(io, std::ptr::addr_of_mut!(thread), td_ptr) }
        .expect("Termio::threadEnter");

    // Capture the pty master fd (both read + write ends of PtyFds; the read
    // thread is its sole reader) so we can close it at teardown — `ThreadData::
    // deinit` closes the quit pipe + the xev handles but NOT the master.
    let master: std::os::fd::RawFd = match unsafe { &(*td_ptr).backend } {
        rustty_termio::backend::ThreadData::Exec(e) => e.read_thread_fd,
    };

    // Bounded pump: non-blocking loop ticks (drive the Process/Stream/Timer
    // completions + fire `processExit` when sh exits) + poll the Screen (the
    // sink the concurrent read thread commits sh's echo into) + a wall-clock
    // deadline + a tick cap. Terminates on BOTH success and timeout.
    const NEEDLE: &[u8] = b"RUSTTY_OK";
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut found = false;
    for _ in 0..2000 {
        thread
            .loop_
            .run(xev::RunMode::NoWait)
            .expect("loop.run(NoWait)");

        let dump = h.screen_dump();
        if dump.windows(NEEDLE.len()).any(|w| w == NEEDLE) {
            found = true;
            break;
        }
        if Instant::now() > deadline {
            break;
        }
        // Yield to the read thread so it can `read` the pty and `processOutput`.
        std::thread::sleep(Duration::from_millis(1));
    }

    assert!(
        found,
        "live /bin/sh 'echo RUSTTY_OK' did not round-trip into the Screen within the deadline; \
         screen saw: {:?}",
        String::from_utf8_lossy(&h.screen_dump())
    );

    // Teardown (blueprint §5 step 6): reap the shell + write the quit pipe +
    // join the read thread, so nothing dereferences `io` after this. We call the
    // field-scoped `Backend::threadExit` (the inlined body of
    // `Termio::threadExit`, which is `self.backend.threadExit(td)`) rather than
    // the whole-`&mut io` `Termio::threadExit`: a whole-`&mut *io` retag would
    // alias the read thread that `threadExit` is in the middle of joining
    // ([ADV-FIX F2]); the field-scoped `.backend` borrow is disjoint from the
    // read thread's `processOutput` fields (terminal_stream/renderer_state/
    // mailbox).
    // SAFETY: `td_ptr` is fully initialized; `io.backend` is disjoint from the
    // winding-down read thread's touched fields.
    unsafe { (*io).backend.threadExit(&mut *td_ptr) };
    // Close the pty master (never closed by deinit); the read thread — its sole
    // reader — is joined above.
    unsafe { libc::close(master) };
    // Deinit the ThreadData (close quit pipe + deinit process/stream/timer),
    // then drop it (the pools + the detached read-thread placeholder). The loop
    // no longer holds completion pointers into it (nothing armed re-fires after
    // the handles are deinit'd).
    // SAFETY: read thread joined; nothing aliases `*td_ptr` anymore.
    unsafe {
        (*td_ptr).deinit();
        std::ptr::drop_in_place(td_ptr);
    }
    // Deinit the Thread (loop + timers + stop async) — after the ThreadData's
    // handles that referenced the loop are deinit'd.
    thread.deinit();
    // Deinit the Termio + free the harness heap boxes (safe now: read thread
    // joined, no live `io` alias).
    h.teardown();
}
