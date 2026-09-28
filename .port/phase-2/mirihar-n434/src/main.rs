//! N434: the rustty-xev timer heap stored a `*mut Timer` derived from a
//! `&mut (*c).op` reborrow; a later `&mut (*c).op` (timer_reset's Active arm,
//! stop_completion) is a fresh Unique retag from the completion's own tag, which
//! pops the heap's pointer under Stacked Borrows; `heap::remove` / `delete_min` /
//! `peek` + `less` then read through the popped tag. Isolated, non-FFI model of
//! the four heap backends' borrow shapes (see README.md). One candidate per
//! argv[1]: `<backend>_<shape>_<path>_<target>`.
//!
//! The heap is the REAL `crates/rustty-xev/src/heap.rs` (included by `#[path]`),
//! so its own raw-pointer discipline is exercised too.
//!
//! Real-code map (crates/rustty-xev/src):
//!   kqueue.rs    start (whole-op `&mut` match, Timer arm inserts), timer_reset Active,
//!                stop_completion (`if let Operation::Timer(v) = &mut (*c).op`)
//!   iocp.rs      start_completion (whole-op `&mut` match), timer_reset Active (`if let`),
//!                stop_completion (`match &mut (*completion).op`, sets Dead before add)
//!   wasi_poll.rs start (whole-op `&mut` match), timer_reset Active (`if let`),
//!                stop_completion (`match &mut (*c).op` + nested `&mut (*c).op` re-matches)
//!   epoll.rs     start (`match &(*completion).op`, then an inner `&mut` match for the
//!                Timer), timer_reset Active (`match &mut`), stop_completion (`match &mut`,
//!                `let c = v.c`)
#![allow(clippy::missing_safety_doc, dead_code)]

#[path = "../../../../crates/rustty-xev/src/heap.rs"]
mod heap;

use std::ffi::c_void;
use std::ptr;

// ---------------------------------------------------------------------------
// Candidate configuration
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Backend {
    Kqueue,
    Iocp,
    Wasi,
    Epoll,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Shape {
    /// The shipped code: Timer accesses through `&mut (*c).op` bindings.
    Shipped,
    /// The fix: every heap-path Timer access through `timer_ptr(c)`.
    Fixed,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Path {
    /// `timer_reset` on an Active timer: Active arm writes `reset`, queues the
    /// cancel, `stop_completion` removes + rearms, then every timer fires.
    Reset,
    /// `cancel` on an Active timer: `stop_completion` removes + fires Cancel.
    Cancel,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Target {
    /// The acted-on timer is the heap root (`remove` -> `delete_min` reads root).
    Root,
    /// The acted-on timer is a non-root node.
    Inner,
}

// ---------------------------------------------------------------------------
// The model
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum State {
    Dead,
    Adding,
    Active,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Res {
    Fired,
    Canceled,
    CancelDone,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Action {
    Disarm,
    Rearm,
}

pub type Callback = fn(*mut c_void, *mut Loop, *mut Completion, Res) -> Action;

pub struct Timer {
    next: u64,
    reset: Option<u64>,
    heap: heap::IntrusiveField<Timer>,
    c: *mut Completion,
}

impl heap::Node for Timer {
    unsafe fn heap_field(this: *mut Self) -> *mut heap::IntrusiveField<Self> {
        unsafe { ptr::addr_of_mut!((*this).heap) }
    }
}

impl Timer {
    fn less(_: (), a: *mut Timer, b: *mut Timer) -> bool {
        unsafe { (*a).next < (*b).next }
    }
}

/// A multi-variant op so the Timer payload sits at a non-zero offset inside the
/// enum, as in the real backends.
pub enum Operation {
    Noop,
    Read { fd: i32, buf: [u8; 16] },
    Cancel { c: *mut Completion },
    Timer(Timer),
}

pub struct Completion {
    op: Operation,
    userdata: *mut c_void,
    callback: Callback,
    state: State,
    result: Option<Res>,
}

fn noop_cb(_: *mut c_void, _: *mut Loop, _: *mut Completion, _: Res) -> Action {
    Action::Disarm
}

impl Completion {
    fn dead() -> Self {
        Completion {
            op: Operation::Noop,
            userdata: ptr::null_mut(),
            callback: noop_cb,
            state: State::Dead,
            result: None,
        }
    }
}

pub struct Loop {
    backend: Backend,
    shape: Shape,
    timers: heap::Intrusive<Timer, ()>,
    active: usize,
    submissions: Vec<*mut Completion>,
    completions: Vec<*mut Completion>,
    cancellations: Vec<*mut Completion>,
    log: Vec<(usize, Res)>,
}

/// THE FIX (mirrors the real helper): the Timer payload's address, with the
/// completion's OWN provenance. Only a shared read of the op; never `&mut`.
unsafe fn timer_ptr(c: *mut Completion) -> *mut Timer {
    unsafe {
        let addr = match &(*c).op {
            Operation::Timer(t) => ptr::from_ref(t).addr(),
            _ => unreachable!(),
        };
        c.cast::<u8>().with_addr(addr).cast::<Timer>()
    }
}

impl Loop {
    unsafe fn add(this: *mut Loop, c: *mut Completion) {
        unsafe {
            (*c).state = State::Adding;
            (*this).submissions.push(c);
        }
    }

    unsafe fn timer(this: *mut Loop, c: *mut Completion, next: u64, ud: *mut c_void, cb: Callback) {
        unsafe {
            *c = Completion {
                op: Operation::Timer(Timer {
                    next,
                    reset: None,
                    heap: heap::IntrusiveField::default(),
                    c: ptr::null_mut(),
                }),
                userdata: ud,
                callback: cb,
                state: State::Dead,
                result: None,
            };
            Loop::add(this, c);
        }
    }

    // ---- start -----------------------------------------------------------

    unsafe fn start(this: *mut Loop, c: *mut Completion) {
        unsafe {
            match ((*this).backend, (*this).shape) {
                (Backend::Epoll, Shape::Shipped) => match &(*c).op {
                    Operation::Timer(_) => {
                        let v_ptr: *mut Timer = match &mut (*c).op {
                            Operation::Timer(t) => {
                                t.c = c;
                                t
                            }
                            _ => unreachable!(),
                        };
                        (*this).timers.insert(v_ptr);
                    }
                    Operation::Cancel { c: target } => {
                        let target = *target;
                        Loop::stop_completion(this, target);
                        (*c).state = State::Dead;
                        let _ = ((*c).callback)((*c).userdata, this, c, Res::CancelDone);
                        return;
                    }
                    _ => unreachable!(),
                },
                (Backend::Epoll, Shape::Fixed) => match &(*c).op {
                    Operation::Timer(_) => {
                        let v_ptr = timer_ptr(c);
                        (*v_ptr).c = c;
                        (*this).timers.insert(v_ptr);
                    }
                    Operation::Cancel { c: target } => {
                        let target = *target;
                        Loop::stop_completion(this, target);
                        (*c).state = State::Dead;
                        let _ = ((*c).callback)((*c).userdata, this, c, Res::CancelDone);
                        return;
                    }
                    _ => unreachable!(),
                },
                // kqueue `start`, iocp `start_completion`, wasi_poll `start`: one
                // whole-op `&mut` match.
                (_, Shape::Shipped) => match &mut (*c).op {
                    Operation::Timer(v) => {
                        v.c = c;
                        let v_ptr: *mut Timer = v;
                        (*this).timers.insert(v_ptr);
                    }
                    Operation::Cancel { .. } => {
                        (*this).cancellations.push(c);
                        (*c).state = State::Active;
                        return;
                    }
                    Operation::Read { buf, .. } => buf[0] = 1,
                    Operation::Noop => unreachable!(),
                },
                (_, Shape::Fixed) => match &mut (*c).op {
                    Operation::Timer(_) => {
                        let v_ptr = timer_ptr(c);
                        (*v_ptr).c = c;
                        (*this).timers.insert(v_ptr);
                    }
                    Operation::Cancel { .. } => {
                        (*this).cancellations.push(c);
                        (*c).state = State::Active;
                        return;
                    }
                    Operation::Read { buf, .. } => buf[0] = 1,
                    Operation::Noop => unreachable!(),
                },
            }
            (*c).state = State::Active;
            (*this).active += 1;
        }
    }

    // ---- timer_reset (Active arm) ----------------------------------------

    unsafe fn timer_reset(this: *mut Loop, c: *mut Completion, c_cancel: *mut Completion, next: u64) {
        unsafe {
            assert!((*c).state == State::Active);
            match ((*this).backend, (*this).shape) {
                (Backend::Kqueue | Backend::Epoll, Shape::Shipped) => match &mut (*c).op {
                    Operation::Timer(t) => t.reset = Some(next),
                    _ => unreachable!(),
                },
                (Backend::Iocp | Backend::Wasi, Shape::Shipped) => {
                    if let Operation::Timer(v) = &mut (*c).op {
                        v.reset = Some(next);
                    }
                }
                (Backend::Kqueue | Backend::Epoll, Shape::Fixed) => {
                    (*timer_ptr(c)).reset = Some(next);
                }
                (Backend::Iocp | Backend::Wasi, Shape::Fixed) => {
                    if matches!(&(*c).op, Operation::Timer(_)) {
                        (*timer_ptr(c)).reset = Some(next);
                    }
                }
            }
            *c_cancel = Completion {
                op: Operation::Cancel { c },
                ..Completion::dead()
            };
            Loop::add(this, c_cancel);
        }
    }

    unsafe fn cancel(this: *mut Loop, c: *mut Completion, c_cancel: *mut Completion) {
        unsafe {
            *c_cancel = Completion {
                op: Operation::Cancel { c },
                ..Completion::dead()
            };
            Loop::add(this, c_cancel);
        }
    }

    // ---- stop_completion --------------------------------------------------

    unsafe fn stop_completion(this: *mut Loop, c: *mut Completion) {
        unsafe {
            match ((*this).backend, (*this).shape) {
                (Backend::Kqueue, Shape::Shipped) => {
                    if let Operation::Timer(v) = &mut (*c).op {
                        if (*c).state == State::Active {
                            let v_ptr: *mut Timer = v;
                            (*this).timers.remove(v_ptr);
                            if let Some(r) = v.reset {
                                v.next = r;
                                v.reset = None;
                                (*this).active -= 1;
                                Loop::add(this, c);
                                return;
                            }
                        }
                        (*c).result = Some(Res::Canceled);
                        (*this).completions.push(c);
                    }
                }
                (Backend::Kqueue, Shape::Fixed) => {
                    if matches!(&(*c).op, Operation::Timer(_)) {
                        let v = timer_ptr(c);
                        if (*c).state == State::Active {
                            (*this).timers.remove(v);
                            if let Some(r) = (*v).reset {
                                (*v).next = r;
                                (*v).reset = None;
                                (*this).active -= 1;
                                Loop::add(this, c);
                                return;
                            }
                        }
                        (*c).result = Some(Res::Canceled);
                        (*this).completions.push(c);
                    }
                }
                (Backend::Iocp, Shape::Shipped) => match &mut (*c).op {
                    Operation::Timer(v) => {
                        if (*c).state == State::Active {
                            let v_ptr: *mut Timer = v;
                            (*this).timers.remove(v_ptr);
                            if let Some(r) = v.reset {
                                v.next = r;
                                v.reset = None;
                                (*c).state = State::Dead;
                                (*this).active -= 1;
                                Loop::add(this, c);
                                return;
                            }
                        }
                        (*c).result = Some(Res::Canceled);
                        (*this).completions.push(c);
                    }
                    _ => {}
                },
                (Backend::Iocp, Shape::Fixed) => match &(*c).op {
                    Operation::Timer(_) => {
                        let v = timer_ptr(c);
                        if (*c).state == State::Active {
                            (*this).timers.remove(v);
                            if let Some(r) = (*v).reset {
                                (*v).next = r;
                                (*v).reset = None;
                                (*c).state = State::Dead;
                                (*this).active -= 1;
                                Loop::add(this, c);
                                return;
                            }
                        }
                        (*c).result = Some(Res::Canceled);
                        (*this).completions.push(c);
                    }
                    _ => {}
                },
                (Backend::Wasi, shape) => {
                    let active = (*c).state == State::Active;
                    let rearm = if shape == Shape::Shipped {
                        match &mut (*c).op {
                            Operation::Timer(v) => {
                                debug_assert!(v.c == c);
                                if (*c).state == State::Active {
                                    let v_ptr: *mut Timer = v;
                                    (*this).timers.remove(v_ptr);
                                }
                                if (*c).state != State::Dead {
                                    let reset = match &mut (*c).op {
                                        Operation::Timer(v) => v.reset,
                                        _ => unreachable!(),
                                    };
                                    if let Some(r) = reset {
                                        if let Operation::Timer(v) = &mut (*c).op {
                                            v.next = r;
                                            v.reset = None;
                                        }
                                        true
                                    } else {
                                        (*c).state = State::Dead;
                                        let cb = (*c).callback;
                                        cb((*c).userdata, this, c, Res::Canceled) == Action::Rearm
                                    }
                                } else {
                                    false
                                }
                            }
                            _ => unreachable!(),
                        }
                    } else {
                        match &(*c).op {
                            Operation::Timer(_) => {
                                let v = timer_ptr(c);
                                debug_assert!((*v).c == c);
                                if (*c).state == State::Active {
                                    (*this).timers.remove(v);
                                }
                                if (*c).state != State::Dead {
                                    let reset = (*v).reset;
                                    if let Some(r) = reset {
                                        (*v).next = r;
                                        (*v).reset = None;
                                        true
                                    } else {
                                        (*c).state = State::Dead;
                                        let cb = (*c).callback;
                                        cb((*c).userdata, this, c, Res::Canceled) == Action::Rearm
                                    }
                                } else {
                                    false
                                }
                            }
                            _ => unreachable!(),
                        }
                    };
                    if active {
                        (*this).active -= 1;
                    }
                    if rearm {
                        Loop::start(this, c);
                    }
                }
                (Backend::Epoll, Shape::Shipped) => {
                    match &mut (*c).op {
                        Operation::Timer(v) => {
                            let cc = v.c;
                            if (*cc).state == State::Active {
                                let v_ptr: *mut Timer = v;
                                (*this).timers.remove(v_ptr);
                            }
                            if (*cc).state != State::Dead {
                                if let Some(r) = v.reset {
                                    v.next = r;
                                    v.reset = None;
                                    (*this).active -= 1;
                                    Loop::start(this, cc);
                                    return;
                                }
                                let cb = (*cc).callback;
                                let _ = cb((*cc).userdata, this, cc, Res::Canceled);
                            }
                        }
                        _ => unreachable!(),
                    }
                    if (*c).state == State::Active {
                        (*this).active -= 1;
                    }
                    (*c).state = State::Dead;
                }
                (Backend::Epoll, Shape::Fixed) => {
                    match &(*c).op {
                        Operation::Timer(_) => {
                            let v = timer_ptr(c);
                            let cc = (*v).c;
                            if (*cc).state == State::Active {
                                (*this).timers.remove(v);
                            }
                            if (*cc).state != State::Dead {
                                if let Some(r) = (*v).reset {
                                    (*v).next = r;
                                    (*v).reset = None;
                                    (*this).active -= 1;
                                    Loop::start(this, cc);
                                    return;
                                }
                                let cb = (*cc).callback;
                                let _ = cb((*cc).userdata, this, cc, Res::Canceled);
                            }
                        }
                        _ => unreachable!(),
                    }
                    if (*c).state == State::Active {
                        (*this).active -= 1;
                    }
                    (*c).state = State::Dead;
                }
            }
        }
    }

    // ---- tick ------------------------------------------------------------

    unsafe fn tick(this: *mut Loop, now: u64) {
        unsafe {
            // Submissions -> start (the cancel arm stops its target).
            loop {
                let subs = core::mem::take(&mut (*this).submissions);
                if subs.is_empty() {
                    break;
                }
                for c in subs {
                    Loop::start(this, c);
                }
                // kqueue / iocp: process_cancellations.
                let cancels = core::mem::take(&mut (*this).cancellations);
                for cc in cancels {
                    let target = match &(*cc).op {
                        Operation::Cancel { c } => *c,
                        _ => unreachable!(),
                    };
                    Loop::stop_completion(this, target);
                    (*cc).state = State::Dead;
                    let _ = ((*cc).callback)((*cc).userdata, this, cc, Res::CancelDone);
                }
            }

            // Expired timers (peek + less + delete_min, as every tick_inner).
            let mut now_timer = Timer {
                next: now,
                reset: None,
                heap: heap::IntrusiveField::default(),
                c: ptr::null_mut(),
            };
            loop {
                let t = (*this).timers.peek();
                if t.is_null() {
                    break;
                }
                if !Timer::less((), t, &mut now_timer) {
                    break;
                }
                assert!((*this).timers.delete_min() == t);
                let c = (*t).c;
                (*c).state = State::Dead;
                (*this).active -= 1;
                let cb = (*c).callback;
                let _ = cb((*c).userdata, this, c, Res::Fired);
            }

            // kqueue / iocp completion queue (the Cancel result of a timer).
            let comps = core::mem::take(&mut (*this).completions);
            for c in comps {
                let r = (*c).result.take().unwrap();
                if (*c).state == State::Active {
                    (*this).active -= 1;
                }
                (*c).state = State::Dead;
                let cb = (*c).callback;
                let _ = cb((*c).userdata, this, c, r);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// The scenario (renderer Thread shape: loop + completions inline in a Box,
// every access through `&raw mut (*th).field`, renderer_seam.rs:1319/1444/1489).
// ---------------------------------------------------------------------------

struct Thread {
    loop_: Loop,
    c0: Completion,
    c1: Completion,
    c2: Completion,
    c_cancel: Completion,
}

fn log_cb(ud: *mut c_void, l: *mut Loop, _c: *mut Completion, r: Res) -> Action {
    unsafe { (*l).log.push((ud as usize, r)) };
    Action::Disarm
}

pub fn run(backend: Backend, shape: Shape, path: Path, target: Target) {
    let th: *mut Thread = Box::into_raw(Box::new(Thread {
        loop_: Loop {
            backend,
            shape,
            timers: heap::Intrusive::new((), Timer::less),
            active: 0,
            submissions: Vec::new(),
            completions: Vec::new(),
            cancellations: Vec::new(),
            log: Vec::new(),
        },
        c0: Completion::dead(),
        c1: Completion::dead(),
        c2: Completion::dead(),
        c_cancel: Completion::dead(),
    }));
    unsafe {
        let l = &raw mut (*th).loop_;
        let c0 = &raw mut (*th).c0;
        let c1 = &raw mut (*th).c1;
        let c2 = &raw mut (*th).c2;
        let cc = &raw mut (*th).c_cancel;

        Loop::timer(l, c0, 10, 0 as *mut c_void, log_cb);
        Loop::timer(l, c1, 20, 1 as *mut c_void, log_cb);
        Loop::timer(l, c2, 30, 2 as *mut c_void, log_cb);
        Loop::tick(l, 0); // start all three; none expired
        assert_eq!((*l).active, 3);
        assert!((*l).log.is_empty());

        let (tc, tid) = match target {
            Target::Root => (c0, 0usize),
            Target::Inner => (c1, 1usize),
        };

        match path {
            Path::Reset => Loop::timer_reset(l, tc, cc, 25),
            Path::Cancel => Loop::cancel(l, tc, cc),
        }
        Loop::tick(l, 0); // process the cancel; nothing expires
        Loop::tick(l, 100); // fire everything left

        let expect: Vec<(usize, Res)> = match (path, target) {
            (Path::Reset, Target::Root) => vec![(1, Res::Fired), (0, Res::Fired), (2, Res::Fired)],
            (Path::Reset, Target::Inner) => vec![(0, Res::Fired), (1, Res::Fired), (2, Res::Fired)],
            (Path::Cancel, _) => {
                let mut v = vec![];
                // epoll / wasi fire the Cancel synchronously in stop_completion
                // (first tick); kqueue / iocp queue it to the completion queue
                // (also first tick). Either way it precedes the expiries.
                v.push((tid, Res::Canceled));
                for i in 0..3 {
                    if i != tid {
                        v.push((i, Res::Fired));
                    }
                }
                v
            }
        };
        assert_eq!((*l).log, expect, "{backend:?} {shape:?} {path:?} {target:?}");
        assert_eq!((*l).active, 0);
        assert!((*l).timers.peek().is_null());
        for c in [c0, c1, c2, cc] {
            assert_eq!((*c).state, State::Dead);
        }
        drop(Box::from_raw(th));
    }
}

pub fn parse(name: &str) -> (Backend, Shape, Path, Target) {
    let p: Vec<&str> = name.split('_').collect();
    assert_eq!(p.len(), 4, "candidate: <backend>_<shape>_<path>_<target>");
    let b = match p[0] {
        "kqueue" => Backend::Kqueue,
        "iocp" => Backend::Iocp,
        "wasi" => Backend::Wasi,
        "epoll" => Backend::Epoll,
        x => panic!("backend {x}"),
    };
    let s = match p[1] {
        "shipped" => Shape::Shipped,
        "fixed" => Shape::Fixed,
        x => panic!("shape {x}"),
    };
    let pa = match p[2] {
        "reset" => Path::Reset,
        "cancel" => Path::Cancel,
        x => panic!("path {x}"),
    };
    let t = match p[3] {
        "root" => Target::Root,
        "inner" => Target::Inner,
        x => panic!("target {x}"),
    };
    (b, s, pa, t)
}

fn main() {
    let name = std::env::args().nth(1).expect("candidate name");
    let (b, s, p, t) = parse(&name);
    run(b, s, p, t);
    println!("{name}: ok");
}

#[cfg(test)]
mod applied {
    use super::*;

    /// Every backend's applied (fixed) shape, every path and target.
    #[test]
    fn applied_shape_is_clean() {
        for b in [Backend::Kqueue, Backend::Iocp, Backend::Wasi, Backend::Epoll] {
            for p in [Path::Reset, Path::Cancel] {
                for t in [Target::Root, Target::Inner] {
                    run(b, Shape::Fixed, p, t);
                }
            }
        }
    }
}
