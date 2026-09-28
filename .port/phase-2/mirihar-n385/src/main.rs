//! mirihar-n385 — isolated, non-FFI reproduction of the IOCP `Async::notify` /
//! `Loop::async_notify` borrow structure (NEW-FINDINGS N385), used to PROVE the
//! fix applied to `crates/rustty-xev/src/{iocp.rs,iocp_watcher.rs}` under Stacked
//! Borrows and Tree Borrows. The real code cannot run under Miri (IOCP is Win32
//! FFI), so this harness keeps only the borrow shapes:
//!
//! - the loop thread is inside `Loop::run(&mut self)` -> `tick(&mut self)` ->
//!   `tick_inner(&mut self)` (function-entry, PROTECTED `&mut Loop`) for the whole
//!   run, reached through ONE stable raw root (termio's
//!   `(*self_).loop_.run(..)`, `crates/rustty-termio/src/thread.rs`);
//! - a second thread calls `Async::notify`, which (reference `async.zig:597`)
//!   reaches the loop through the waiter's `*Loop` and calls
//!   `loop.async_notify(c)` (reference `iocp.zig:850`): store `true` into the
//!   completion's `wakeup` atomic, then `PostQueuedCompletionStatus(iocp_handle)`.
//!
//! The kernel completion port is modelled as an atomic counter in its OWN heap
//! allocation (`Port`): `PostQueuedCompletionStatus` / the loop's
//! `GetQueuedCompletionStatusEx` are FFI calls on a kernel object, so neither
//! touches the `Loop` allocation; only the `HANDLE` value lives in `Loop`.
//!
//! Two notifications are forced into the two windows the real code allows:
//!   1. while the loop thread is in `start_completion` with its borrow of the
//!      completion's `op` live (the `c` was just submitted by `Async::wait`; the
//!      waiter is already published, so a notifier may run here);
//!   2. while the loop thread is blocked in `GetQueuedCompletionStatusEx`, still
//!      holding its protected `&mut Loop` (the ordinary wakeup case).
//!
//! Candidate = (notify shape) x (start_completion shape), selected by argv[1]:
//!   notify:  `loopref`  — shipped: `(*w.loop_).async_notify(w.c)`, `&self` receiver
//!            `rawread`  — N385 fix-sketch option A: read the HANDLE by value
//!                         through the raw `*Loop` (`addr_of!(..).read()`)
//!            `cached`   — APPLIED: the HANDLE is copied into the waiter at `wait`
//!                         time (loop thread); `notify` never touches `Loop`
//!   start:   `mutop`    — shipped: `match &mut (*completion).op { .. }`
//!            `sharedop` — APPLIED: decide the `async_wait` arm through a shared
//!                         borrow; `&mut` is formed only for the other ops

use std::ptr;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

// ---------------------------------------------------------------------------
// The kernel completion port (FFI object, its own allocation).
// ---------------------------------------------------------------------------

struct Port {
    posted: AtomicUsize,
}

/// `HANDLE`: a `Copy` value naming the kernel object.
#[derive(Clone, Copy)]
struct Handle(*const Port);

/// `PostQueuedCompletionStatus(handle, 0, 0, null)`.
unsafe fn post_queued_completion_status(h: Handle) {
    unsafe { (*h.0).posted.fetch_add(1, Ordering::SeqCst) };
}

// ---------------------------------------------------------------------------
// Completion / Operation (the shapes that matter from iocp.rs).
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Debug)]
enum State {
    Dead,
    Adding,
    Active,
}

#[allow(dead_code)]
enum Operation {
    Noop,
    Timer { next: u64 },
    AsyncWait { wakeup: AtomicBool },
}

struct Completion {
    op: Operation,
    next: *mut Completion,
    state: State,
    fired: u32,
}

// ---------------------------------------------------------------------------
// Loop
// ---------------------------------------------------------------------------

struct Loop {
    iocp_handle: Handle,
    active: usize,
    submissions: *mut Completion,
    asyncs: *mut Completion,
    other: u64,
}

#[derive(Clone, Copy, PartialEq)]
enum Notify {
    LoopRef,
    RawRead,
    Cached,
}

#[derive(Clone, Copy, PartialEq)]
enum Start {
    MutOp,
    SharedOp,
}

// Phase handshakes that force each notification into its window.
static P1: AtomicBool = AtomicBool::new(false);
static N1: AtomicBool = AtomicBool::new(false);
static P2: AtomicBool = AtomicBool::new(false);
static N2: AtomicBool = AtomicBool::new(false);

fn wait_for(flag: &AtomicBool) {
    while !flag.load(Ordering::Acquire) {
        std::thread::yield_now();
    }
}

impl Loop {
    unsafe fn add(&mut self, c: *mut Completion) {
        unsafe {
            (*c).state = State::Adding;
            (*c).next = self.submissions;
        }
        self.submissions = c;
    }

    /// `pub fn run(&mut self, mode)` — the protected `&mut Loop` held for the
    /// whole run.
    fn run(&mut self, start: Start) {
        self.tick(start);
    }

    fn tick(&mut self, start: Start) {
        self.other = self.other.wrapping_add(1);
        self.tick_inner(start);
        self.other = self.other.wrapping_add(1);
    }

    fn tick_inner(&mut self, start: Start) {
        let self_ptr: *mut Loop = self;

        // Submissions -> start_completion.
        let c = self.submissions;
        self.submissions = ptr::null_mut();
        unsafe { self.start_completion(c, start) };

        // GetQueuedCompletionStatusEx(self.iocp_handle, ...): the loop thread is
        // blocked in the kernel, still holding `&mut self`. Window 2.
        P2.store(true, Ordering::Release);
        wait_for(&N2);
        let posted = unsafe { (*self.iocp_handle.0).posted.load(Ordering::SeqCst) };
        assert_eq!(posted, 2, "both notifications posted to the port");

        // Process asyncs (iocp.rs tick_inner "Process asyncs").
        let c = self.asyncs;
        self.asyncs = ptr::null_mut();
        let c_wakeup = match unsafe { &(*c).op } {
            Operation::AsyncWait { wakeup } => wakeup.swap(false, Ordering::SeqCst),
            _ => unreachable!(),
        };
        assert!(c_wakeup, "the async completion was woken");
        unsafe { (*c).state = State::Dead };
        self.active -= 1;
        // cb(ud, self_ptr, c, ..)
        callback(self_ptr, c);
        // The loop thread keeps using its `&mut self` after the notifier ran.
        self.other = self.other.wrapping_add(1);
    }

    unsafe fn start_completion(&mut self, completion: *mut Completion, start: Start) {
        let is_async = match start {
            Start::MutOp => {
                // SHIPPED: `match unsafe { &mut (*completion).op } { .. }`.
                let op = unsafe { &mut (*completion).op };
                // Window 1: the notifier runs between the retag and the use.
                P1.store(true, Ordering::Release);
                wait_for(&N1);
                match op {
                    Operation::Noop => false,
                    Operation::Timer { next } => {
                        *next += 1;
                        false
                    }
                    Operation::AsyncWait { .. } => true,
                }
            }
            Start::SharedOp => {
                // APPLIED: the `async_wait` arm is decided through a shared
                // borrow; `&mut` only for the ops no other thread touches.
                let op = unsafe { &(*completion).op };
                P1.store(true, Ordering::Release);
                wait_for(&N1);
                if matches!(op, Operation::AsyncWait { .. }) {
                    true
                } else {
                    match unsafe { &mut (*completion).op } {
                        Operation::Timer { next } => *next += 1,
                        Operation::Noop => {}
                        Operation::AsyncWait { .. } => unreachable!(),
                    }
                    false
                }
            }
        };
        assert!(is_async);
        unsafe { (*completion).next = self.asyncs };
        self.asyncs = completion;
        self.active += 1;
        unsafe { (*completion).state = State::Active };
    }

    /// SHIPPED `pub unsafe fn async_notify(&self, completion)`; with the fix it
    /// forwards to `async_notify_handle(self.iocp_handle, ..)` for loop-thread
    /// callers.
    unsafe fn async_notify(&self, completion: *mut Completion) {
        unsafe { Loop::async_notify_handle(self.iocp_handle, completion) }
    }

    /// The body of `async_notify`, with the port HANDLE passed by value so a
    /// caller on another thread need not touch the `Loop`.
    unsafe fn async_notify_handle(iocp_handle: Handle, completion: *mut Completion) {
        debug_assert!(matches!(
            unsafe { &(*completion).op },
            Operation::AsyncWait { .. }
        ));
        match unsafe { &(*completion).op } {
            Operation::AsyncWait { wakeup } => wakeup.store(true, Ordering::SeqCst),
            _ => unreachable!(),
        }
        unsafe { post_queued_completion_status(iocp_handle) };
    }
}

fn callback(_l: *mut Loop, c: *mut Completion) {
    unsafe { (*c).fired += 1 };
}

// ---------------------------------------------------------------------------
// AsyncIOCP watcher (iocp_watcher.rs).
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
struct AsyncWaiter {
    loop_: *mut Loop,
    c: *mut Completion,
    /// APPLIED: the port HANDLE, copied on the loop thread at `wait` time.
    iocp_handle: Handle,
}

struct AsyncState {
    wakeup: bool,
    waiter: Option<AsyncWaiter>,
}

struct Async {
    guard: Mutex<AsyncState>,
}
unsafe impl Send for Async {}
unsafe impl Sync for Async {}

impl Async {
    unsafe fn wait(&self, loop_: *mut Loop, c: *mut Completion) {
        unsafe {
            *c = Completion {
                op: Operation::AsyncWait {
                    wakeup: AtomicBool::new(false),
                },
                next: ptr::null_mut(),
                state: State::Dead,
                fired: 0,
            };
            (*loop_).add(c);
        }
        // Loop thread: reading the handle through `loop_` here is the loop
        // thread's own access (no other borrow of the loop is live).
        let iocp_handle = unsafe { (*loop_).iocp_handle };
        let mut state = self.guard.lock().unwrap();
        state.waiter = Some(AsyncWaiter {
            loop_,
            c,
            iocp_handle,
        });
        if state.wakeup {
            unsafe { (*loop_).async_notify(c) };
        }
    }

    fn notify(&self, mode: Notify) {
        let mut state = self.guard.lock().unwrap();
        match state.waiter {
            Some(w) => unsafe {
                match mode {
                    Notify::LoopRef => (*w.loop_).async_notify(w.c),
                    Notify::RawRead => {
                        let h = ptr::addr_of!((*w.loop_).iocp_handle).read();
                        Loop::async_notify_handle(h, w.c)
                    }
                    Notify::Cached => Loop::async_notify_handle(w.iocp_handle, w.c),
                }
            },
            None => state.wakeup = true,
        }
    }
}

// ---------------------------------------------------------------------------
// Driver.
// ---------------------------------------------------------------------------

fn run(notify: Notify, start: Start) -> u64 {
    let port: *mut Port = Box::into_raw(Box::new(Port {
        posted: AtomicUsize::new(0),
    }));
    let mut lp: Box<Loop> = Box::new(Loop {
        iocp_handle: Handle(port),
        active: 0,
        submissions: ptr::null_mut(),
        asyncs: ptr::null_mut(),
        other: 0,
    });
    // ONE stable raw root (termio's `&raw mut (*self_).loop_`).
    let root: *mut Loop = &raw mut *lp;
    let c: *mut Completion = Box::into_raw(Box::new(Completion {
        op: Operation::Noop,
        next: ptr::null_mut(),
        state: State::Dead,
        fired: 0,
    }));
    let a = Arc::new(Async {
        guard: Mutex::new(AsyncState {
            wakeup: false,
            waiter: None,
        }),
    });

    // `mailbox.wakeup.wait(&self.loop, &self.wakeup_c, ..)` on the loop thread.
    unsafe { a.wait(root, c) };

    let a2 = Arc::clone(&a);
    let notifier = std::thread::spawn(move || {
        wait_for(&P1);
        a2.notify(notify);
        N1.store(true, Ordering::Release);
        wait_for(&P2);
        a2.notify(notify);
        N2.store(true, Ordering::Release);
    });

    // `try self.loop.run(.until_done)` through the stable root.
    unsafe { (*root).run(start) };
    notifier.join().unwrap();

    let fired = unsafe { (*c).fired };
    assert_eq!(fired, 1);
    assert_eq!(unsafe { (*root).active }, 0);
    let other = unsafe { (*root).other };
    drop(unsafe { Box::from_raw(c) });
    drop(lp);
    drop(unsafe { Box::from_raw(port) });
    other
}

fn parse(name: &str) -> Option<(Notify, Start)> {
    let (n, s) = name.split_once('_')?;
    let n = match n {
        "loopref" => Notify::LoopRef,
        "rawread" => Notify::RawRead,
        "cached" => Notify::Cached,
        _ => return None,
    };
    let s = match s {
        "mutop" => Start::MutOp,
        "sharedop" => Start::SharedOp,
        _ => return None,
    };
    Some((n, s))
}

fn main() {
    let name = std::env::args().nth(1).unwrap_or_else(|| "cached_sharedop".into());
    let Some((n, s)) = parse(&name) else {
        eprintln!("unknown candidate: {name:?}");
        eprintln!(
            "candidates: loopref_mutop loopref_sharedop rawread_mutop rawread_sharedop \
             cached_mutop cached_sharedop"
        );
        std::process::exit(2);
    };
    let other = run(n, s);
    println!("{name}: other={other}");
}

#[cfg(test)]
mod tests {
    /// The APPLIED shape. `cargo +nightly miri test` (SB) and
    /// `MIRIFLAGS=-Zmiri-tree-borrows cargo +nightly miri test` (TB) must be clean.
    /// The UB candidates cannot live here (Miri aborts the whole process on UB);
    /// `run-all.sh` drives every candidate through `cargo miri run`.
    #[test]
    fn cached_sharedop_is_clean() {
        assert_eq!(super::run(super::Notify::Cached, super::Start::SharedOp), 3);
    }
}
