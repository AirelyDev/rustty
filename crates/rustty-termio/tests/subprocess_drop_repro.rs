//! [DOUBLE-FREE FIX] Regression proof: constructing + STARTING (posix fork/exec)
//! + tearing down a `Subprocess` frees its argv/env C-string storage exactly
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
        rt_pre_exec_info: rustty_termio::command::RtPreExecInfo,
        rt_post_fork_info: rustty_termio::command::RtPostForkInfo,
    }
}

// The apprt finalize sequence: deinit the surface IN PLACE, then drop the heap
// box (Drop glue runs on the SAME, now-emptied object). Before the fix this was
// a double-free of the argv `Vec<CString>`; after the fix the drop is a no-op.
// Run repeatedly to shake out heap-corruption aborts.
#[test]
fn subprocess_start_deinit_in_place_then_drop_no_abort() {
    for i in 0..10 {
        eprintln!("iteration {i}");
        let mut exec = Exec::init(Global, mk_cfg()).expect("Exec::init");
        let _fds = exec.subprocess.start(Global).expect("start");
        // deinit IN PLACE (frees + empties the owned C-string storage) ...
        exec.deinit();
        // ... then drop the SAME object — a no-op now (empty fields), NOT a
        // second free of `args`/`cwd`.
        drop(exec);
    }
    eprintln!("done");
}

// deinit is also idempotent: calling it twice must not double-free (the second
// call sees already-emptied fields). Mirrors `stop()` + `deinit()` both running
// in the real teardown (threadExit then Exec::deinit).
#[test]
fn subprocess_deinit_is_idempotent() {
    for i in 0..10 {
        eprintln!("iteration {i}");
        let mut exec = Exec::init(Global, mk_cfg()).expect("Exec::init");
        let _fds = exec.subprocess.start(Global).expect("start");
        exec.deinit();
        exec.deinit();
        drop(exec);
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
    for i in 0..10 {
        eprintln!("iteration {i}");
        let mut exec = Exec::init(Global, mk_cfg()).expect("Exec::init");
        let _fds = exec.subprocess.start(Global).expect("start");

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
    }
    eprintln!("done");
}
