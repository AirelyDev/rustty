//! [DOUBLE-FREE FIX] Regression proof: constructing + STARTING (posix fork/exec) +
//! tearing down a `Subprocess` frees its argv/env C-string storage exactly
//! once — no glibc `free(): invalid size` double-free abort.
//!
//! Background: `Subprocess.args` is a `Vec<CString>`. The port's teardown idiom
//! (mirroring the reference `deinit` + `alloc.destroy`) is CONSUMING deinit +
//! free-the-memory-WITHOUT-dropping. An apprt Surface holding only `&mut self`
//! feeds its by-value `Termio::deinit` a `std::ptr::read` BITWISE DUPLICATE of
//! `self.io`; the deinit'd duplicate frees the storage, and the abandoned
//! original must then be freed WITHOUT dropping. The GTK surface finalize
//! violated that by `drop`ping the surface box (Drop glue), which freed the
//! ALREADY-freed `args` (and mailbox queue, terminal) AGAIN. That caller-side
//! fix is `crates/rustty-apprt/src/gtk/class/surface.rs` (free the box slot via
//! `dealloc` instead of `drop`).
//!
//! The termio-side change proven here: `Subprocess::deinit` now frees + EMPTIES
//! its owned storage IN PLACE (`args`, `cwd`, `pty`, `env`) — faithful to the
//! reference `arena.deinit()` + `self.* = undefined` (Exec.zig:877-884). This
//! makes `deinit` free the C strings exactly once and leaves the value safe to
//! drop afterward (empty fields → no-op) and safe to `deinit` twice
//! (idempotent). These tests exercise the posix start path + every teardown
//! shape and must never abort.
#![cfg(unix)]
#![allow(non_snake_case)]

use allocator_api2::alloc::Global;
use rustty_termio::exec::{Config, Exec};

fn mk_cfg() -> Config {
    Config {
        command: Some(rustty_config::command::Command::Direct(vec![
            b"/bin/sh".to_vec(),
            b"-c".to_vec(),
            b"true".to_vec(),
        ])),
        env: rustty_termio::command::EnvMap::new(),
        env_override: rustty_config::RepeatableStringMap::default(),
        shell_integration: rustty_config::value_types::ShellIntegration::Detect,
        shell_integration_features: rustty_config::value_types::ShellIntegrationFeatures::default(),
        cursor_blink: None,
        working_directory: None,
        resources_dir: None,
        term: b"xterm-256color".to_vec(),
        rt_pre_exec_info: rustty_termio::command::RtPreExecInfo::default(),
        rt_post_fork_info: rustty_termio::command::RtPostForkInfo::default(),
    }
}

// ── [T8-HANG] pty-drain + fork serialization scaffolding ────────────────────
//
// These tests fork a REAL pty child (macOS wraps the command in `login(1)`).
// Two host-level hazards apply, both already handled inside the crate but NOT
// reachable from an integration test (they live behind `pub(crate)`):
//
//  1. NO PTY READER. Every teardown shape below reaches `Subprocess::deinit`
//     -> `stop()` -> `killCommand`/`killPid`, which SIGHUPs the child's process
//     group and then loops on `waitpid(WNOHANG)` until the child is reaped. A
//     `login(1)` SIGHUP'd mid-startup still has pending output queued on its pty
//     SLAVE; if nobody is draining the MASTER, it wedges UNINTERRUPTIBLY in the
//     kernel exit path (state `?Es`, SIGKILL-immune) and is never reapable — so
//     the waitpid loop spins forever. Measured on macOS 26.5.1: 12/12 runs of
//     this binary hung, with three `?Es` `(login)` children and all three test
//     threads parked in `killPid`.
//
//     PRODUCTION DOES NOT DO THIS. `Exec::threadExit` (exec.rs) calls `stop()`
//     while the io-reader thread is STILL RUNNING and draining the master, and
//     only signals the read thread to quit AFTERWARDS — upstream comments the
//     ordering explicitly ("Quit our read thread after exiting the subprocess so
//     that we don't get stuck waiting for data to stop flowing"). The in-crate
//     KAT `exec::tests::subprocess_start_spawns_and_reaps` already models that
//     with a drainer thread; this file was the one spawn site that did not.
//     `drain_pty` below restores the production-faithful shape.
//
//  2. UNSERIALIZED FORKS. `fork()` snapshots every open fd, so two concurrent
//     forks leak each other's pty master/slave into unrelated children — and a
//     child holding a leaked slave copy wedges the same `?Es` way on exit. The
//     crate serializes its in-crate fork KATs on `FORK_KAT_SERIAL`; that lock is
//     `pub(crate)`, so this binary keeps its own equivalent. Pure serialization:
//     all three tests still run, unchanged.

/// This binary's analogue of the crate-private `FORK_KAT_SERIAL`: no two tests
/// in this binary may `fork()` concurrently. See hazard (2) above.
static FORK_SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn fork_serial() -> std::sync::MutexGuard<'static, ()> {
    FORK_SERIAL.lock().unwrap_or_else(|e| e.into_inner())
}

/// Drain the pty master concurrently for the duration of a teardown — the role
/// the real io-reader thread plays across `threadExit`'s `stop()`. See hazard
/// (1) above: without this the SIGHUP'd `login(1)` wedges in `?Es` and
/// `killPid`'s (oracle-faithful) waitpid loop never observes a reapable child.
///
/// Takes its OWN `dup` of the master so it is immune to the teardown closing
/// the original mid-read (`Subprocess::deinit` closes the pty right after
/// `stop()` returns, and a closed fd number can be recycled under a blocked
/// reader). Returns once the child is gone and every slave ref is closed
/// (read yields 0/EIO).
fn drain_pty(master: std::os::fd::RawFd) -> std::thread::JoinHandle<()> {
    // SAFETY: `master` is the live pty master returned by `start()`; `dup`
    // only reads the fd table. The dup is owned solely by the spawned thread.
    let dup = unsafe { libc::dup(master) };
    std::thread::spawn(move || {
        if dup < 0 {
            return;
        }
        let mut buf = [0u8; 4096];
        loop {
            // SAFETY: `dup` is an fd this thread exclusively owns; `read`
            // writes at most `buf.len()` bytes into `buf`.
            let n = unsafe { libc::read(dup, buf.as_mut_ptr().cast(), buf.len()) };
            if n < 0 && std::io::Error::last_os_error().raw_os_error() == Some(libc::EINTR) {
                continue;
            }
            // n == 0 (EOF) or n < 0 (EIO once the last slave ref closes).
            if n <= 0 {
                break;
            }
        }
        // SAFETY: sole owner; closed exactly once as the thread ends.
        unsafe { libc::close(dup) };
    })
}

// The apprt finalize sequence: deinit the surface IN PLACE, then drop the heap
// box (Drop glue runs on the SAME, now-emptied object). Before the fix this was
// a double-free of the argv `Vec<CString>`; after the fix the drop is a no-op.
// Run repeatedly to shake out heap-corruption aborts.
#[test]
fn subprocess_start_deinit_in_place_then_drop_no_abort() {
    let _serial = fork_serial();
    for i in 0..10 {
        eprintln!("iteration {i}");
        let mut exec = Exec::init(Global, mk_cfg()).expect("Exec::init");
        let fds = exec.subprocess.start(Global).expect("start");
        // [T8-HANG] Drain the master across the teardown, as the real io-reader
        // thread does across `threadExit`'s `stop()`.
        let drainer = drain_pty(fds.read);
        // deinit IN PLACE (frees + empties the owned C-string storage) ...
        exec.deinit();
        // ... then drop the SAME object — a no-op now (empty fields), NOT a
        // second free of `args`/`cwd`.
        drop(exec);
        drainer.join().expect("drainer join");
    }
    eprintln!("done");
}

// deinit is also idempotent: calling it twice must not double-free (the second
// call sees already-emptied fields). Mirrors `stop()` + `deinit()` both running
// in the real teardown (threadExit then Exec::deinit).
#[test]
fn subprocess_deinit_is_idempotent() {
    let _serial = fork_serial();
    for i in 0..10 {
        eprintln!("iteration {i}");
        let mut exec = Exec::init(Global, mk_cfg()).expect("Exec::init");
        let fds = exec.subprocess.start(Global).expect("start");
        // [T8-HANG] see `drain_pty`.
        let drainer = drain_pty(fds.read);
        exec.deinit();
        exec.deinit();
        drop(exec);
        drainer.join().expect("drainer join");
    }
    eprintln!("done");
}

// Reproduces the apprt finalize/free-without-drop shape at the Exec level: the
// port's consuming (`ptr::read`) deinit idiom frees the storage via the read-out
// COPY, and the abandoned original is freed WITHOUT dropping (the reference
// `alloc.destroy`). This is the CORRECT idiom the init-error path uses; the GTK
// finalize's `drop(boxed)` was the lone violator. Here `mem::forget` stands in
// for the caller's dealloc-without-drop. Must not abort or leak (the copy freed
// everything; the forgotten original shares the same freed allocations).
#[test]
fn exec_consume_via_copy_then_forget_original_no_abort() {
    let _serial = fork_serial();
    for i in 0..10 {
        eprintln!("iteration {i}");
        let mut exec = Exec::init(Global, mk_cfg()).expect("Exec::init");
        let fds = exec.subprocess.start(Global).expect("start");
        // [T8-HANG] see `drain_pty`.
        let drainer = drain_pty(fds.read);

        // The reference/port idiom: read a copy, consume+drop it (frees the
        // storage exactly once), then free the original WITHOUT dropping.
        let copy = unsafe { std::ptr::read(&exec) };
        {
            let mut c = copy;
            c.deinit();
            // `c` drops here: frees the (now-shared) storage exactly once.
        }
        // Free-without-drop of the original bitwise duplicate — NO second free.
        std::mem::forget(exec);
        drainer.join().expect("drainer join");
    }
    eprintln!("done");
}
