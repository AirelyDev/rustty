//! mirihar-ws20-flatpak — isolated Stacked-/Tree-Borrows proof for the ONE new
//! cross-thread raw-pointer handoff WS20 introduced:
//! `rustty_os::flatpak::FlatpakHostCommand::spawn` handing `self` to a DETACHED
//! worker thread (`threadMain`) that runs a GLib loop and mutates the command's
//! state, while the spawning thread concurrently blocks on the condition variable
//! and then keeps using the same object (`wait`/`waitXev`/`signal`).
//!
//! The real module cannot be run under Miri: it links GIO/GLib and issues DBus
//! calls, both of which are foreign functions Miri refuses to execute. So the
//! ALIASING SHAPE is reproduced here with the FFI removed and nothing else
//! changed — same field layout class, same handoff, same lock discipline, same
//! `onExit`-style callback that recovers the object from an opaque `gpointer`.
//!
//! Two variants are built. `sound()` is the shipped shape. `unsound_mut()` is the
//! shape the first WS20 draft had (`&mut *ptr` on the worker, `&mut self`
//! receivers) — it is compiled but NOT run by default, and exists so the harness
//! is demonstrably non-vacuous: run it with `-- --unsound` and Miri must report a
//! violation. A harness whose failure arm never fires proves nothing.
//!
//! Run:
//!   cd .port/phase-2/mirihar-ws20-flatpak
//!   MIRIFLAGS="-Zmiri-strict-provenance" cargo +nightly miri run
//!   MIRIFLAGS="-Zmiri-tree-borrows"      cargo +nightly miri run
//!   cargo +nightly miri run -- --unsound          # must FAIL (negative control)
//!
//! (The switch is an ARGV flag, not an env var: Miri does not forward the host
//! environment by default, so an env-var switch would silently take the sound
//! path and the negative control would be vacuous.)

use std::sync::{Condvar, Mutex};

// ---------------------------------------------------------------------------
// The shipped shape: every method takes `&self`, ALL mutation goes through the
// `Mutex`, and the worker receives a `*const` it reborrows as `&`. There is no
// `&mut` to the object anywhere, so there is no exclusive (protected) tag for a
// foreign thread's access to invalidate.
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum State {
    Init,
    Started { pid: u32 },
    Exited { status: u8 },
}

struct HostCommand {
    // Immutable-after-construction inputs (the real `argv`/`cwd`/`env`/fds).
    argv: Vec<String>,
    stdin: i32,
    // `state` lives INSIDE the mutex that the reference only documented as
    // guarding it (Zig's mutex-by-convention over a sibling field).
    state_mutex: Mutex<State>,
    state_cv: Condvar,
}

impl HostCommand {
    fn new() -> Self {
        Self {
            argv: vec!["/bin/sh".into(), "-c".into(), ":".into()],
            stdin: 7,
            state_mutex: Mutex::new(State::Init),
            state_cv: Condvar::new(),
        }
    }

    fn update_state(&self, s: State) {
        let mut g = self.state_mutex.lock().unwrap();
        *g = s;
        self.state_cv.notify_all();
    }

    /// `spawn(&self)` — the handoff under test.
    fn spawn(&self) -> u32 {
        struct SendPtr(*const HostCommand);
        // SAFETY: mirrors the shipped SAFETY comment.
        unsafe impl Send for SendPtr {}
        let arg = SendPtr(self as *const HostCommand);

        let t = std::thread::spawn(move || {
            let arg = arg;
            // SHARED reborrow — the load-bearing difference from the draft.
            let s: &HostCommand = unsafe { &*arg.0 };
            thread_main(s);
        });
        drop(t); // detach, as the reference does

        let g = self.state_mutex.lock().unwrap();
        let g = self
            .state_cv
            .wait_while(g, |s| matches!(s, State::Init))
            .unwrap();
        match &*g {
            State::Started { pid } => *pid,
            State::Exited { .. } => 0,
            State::Init => unreachable!(),
        }
    }

    fn wait(&self) -> u8 {
        let mut g = self.state_mutex.lock().unwrap();
        loop {
            match &*g {
                State::Exited { status } => {
                    let s = *status;
                    *g = State::Init;
                    self.state_cv.notify_all();
                    return s;
                }
                _ => g = self.state_cv.wait(g).unwrap(),
            }
        }
    }

    fn signal(&self) -> Option<u32> {
        let g = self.state_mutex.lock().unwrap();
        match &*g {
            State::Started { pid } => Some(*pid),
            _ => None,
        }
    }
}

/// The worker. Reads the immutable inputs (as the real one reads argv/fds to
/// build the DBus payload), publishes `Started`, then the `onExit` analogue
/// publishes `Exited` through an opaque-pointer callback.
fn thread_main(s: &HostCommand) {
    assert_eq!(s.argv.len(), 3);
    assert_eq!(s.stdin, 7);
    s.update_state(State::Started { pid: 4321 });

    // `onExit(..., ud: gpointer)` — recovered as `&`, never `&mut`.
    let ud: *mut std::ffi::c_void = s as *const HostCommand as *mut std::ffi::c_void;
    on_exit(ud);
}

fn on_exit(ud: *mut std::ffi::c_void) {
    // SAFETY: the GIO callback userdata is the (stable) command pointer.
    let s: &HostCommand = unsafe { &*(ud as *const HostCommand) };
    let started = { matches!(*s.state_mutex.lock().unwrap(), State::Started { .. }) };
    if started {
        s.update_state(State::Exited { status: 3 });
    }
}

fn sound() {
    // The command is frame-pinned by the caller (the documented "self pointer
    // MUST be stable" precondition); a local is exactly that here.
    let cmd = HostCommand::new();
    let pid = cmd.spawn();
    assert_eq!(pid, 4321);
    // Concurrent-with-the-worker uses from the spawning thread:
    let _ = cmd.signal();
    let status = cmd.wait();
    assert_eq!(status, 3);
    println!("sound: ok (pid={pid} status={status})");
}

// ---------------------------------------------------------------------------
// NEGATIVE CONTROL — the draft shape. `&mut self` on the spawner plus `&mut *ptr`
// on the worker: two live exclusive tags over the same allocation on two threads.
// Miri must reject this (data race / invalidated protected tag). If it does NOT,
// this harness is vacuous and the `sound()` green above means nothing.
// ---------------------------------------------------------------------------

struct DraftCommand {
    counter: u64,
}

fn unsound_mut() {
    struct SendPtrMut(*mut DraftCommand);
    unsafe impl Send for SendPtrMut {}

    let mut cmd = DraftCommand { counter: 0 };
    let me: &mut DraftCommand = &mut cmd; // the `&mut self` receiver
    let arg = SendPtrMut(me as *mut DraftCommand);

    let t = std::thread::spawn(move || {
        let arg = arg;
        // The draft's `threadMain(unsafe { &mut *arg.0 })`.
        let s: &mut DraftCommand = unsafe { &mut *arg.0 };
        s.counter += 1;
    });

    // The spawner keeps using its own `&mut` — unsynchronized, exactly as the
    // draft's `spawn` did before taking the lock.
    me.counter += 1;
    t.join().unwrap();
    println!("unsound: counter={} (Miri should NOT get here)", me.counter);
}

fn main() {
    if std::env::args().any(|a| a == "--unsound") {
        unsound_mut();
        return;
    }
    sound();
}
