//! Isolated, non-FFI Miri harness for WIRE2 (cli C5) — the `list_themes` picker's
//! Loop-holds-raw-self aliasing.
//!
//! It faithfully models the SHAPE `run_preview` actually uses (the C3b/loop_-
//! lifecycle proven shape, NOT the boxed-struct-field variant the design first
//! sketched): `tty`/`vx` are SEPARATE run-scope locals whose addresses are stable
//! for the read thread's life; a `Loop`-like object stores raw `*mut Tty`/`*mut Vx`
//! cast from `&mut tty`/`&mut vx`; a spawned reader thread derefs those raw ptrs
//! under a sync boundary WHILE the main loop keeps reborrowing `&mut tty`/`&mut vx`/
//! `&mut st` through the disjoint-field free fns `update`/`draw`. The proof
//! obligation: no `&mut` reborrow on the main thread pops the reader's live raw ptrs
//! off the borrow stack (Stacked Borrows) / invalidates them (Tree Borrows).
//!
//! Run under BOTH aliasing models:
//!   MIRIFLAGS="-Zmiri-tree-borrows" cargo +nightly miri run
//!   cargo +nightly miri run            # Stacked Borrows (default)
//! (`cargo +nightly miri test` runs the `#[test]` below in both when the env var is
//! set per invocation.)
#![allow(dead_code)]

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;

// ---- stand-ins for vaxis.Tty / vaxis.Vaxis (SEPARATE run-scope locals) ----
// The read thread only ever PROJECTS narrow fields (a shared read of `fd`, a narrow
// write to `caps`), exactly the vaxis Loop cross-thread contract.
struct Tty {
    fd: i32,
    write_ticks: u64,
}
impl Tty {
    fn new() -> Self {
        Tty {
            fd: 7,
            write_ticks: 0,
        }
    }
    // `tty.writer()` — a &mut projection the main loop drives each frame.
    fn writer_tick(&mut self) {
        self.write_ticks = self.write_ticks.wrapping_add(1);
    }
    // the read thread's `(*tty).read(&self)` — a SHARED read of the fd only.
    fn read_fd(&self) -> i32 {
        self.fd
    }
}

struct Vx {
    caps_color_scheme_updates: bool,
    render_ticks: u64,
}
impl Vx {
    fn new() -> Self {
        Vx {
            caps_color_scheme_updates: false,
            render_ticks: 0,
        }
    }
    // `vx.render(writer)` — a &mut projection the main loop drives each frame.
    fn render_tick(&mut self) {
        self.render_ticks = self.render_ticks.wrapping_add(1);
    }
}

// ---- DrawState (the reference Preview's non-tty/non-vx fields) ----
struct DrawState {
    should_quit: bool,
    current: usize,
    ticks: u64,
}

// ---- the Loop stand-in: holds raw ptrs into the pinned locals (like vaxis.Loop) ----
struct Loop {
    tty: *mut Tty,
    vx: *mut Vx,
    // the reader's synchronization (models the internally-synced queue): the reader
    // spins until `go` is set each frame, touches the raw ptrs, then bumps `done`.
    go: Arc<AtomicUsize>,
    done: Arc<AtomicUsize>,
    observed_fd: Arc<AtomicU64>,
    handle: Option<std::thread::JoinHandle<()>>,
}

// A Send newtype carrying the raw ptrs into the reader (models `SendLoop`).
struct SendPtrs {
    tty: *mut Tty,
    vx: *mut Vx,
    go: Arc<AtomicUsize>,
    done: Arc<AtomicUsize>,
    observed_fd: Arc<AtomicU64>,
}
// SAFETY: the pointees are pinned (stack locals held for the reader's life) and only
// projected narrowly on both sides; this is the vaxis Loop raw-self contract.
unsafe impl Send for SendPtrs {}

impl Loop {
    fn new(tty: *mut Tty, vx: *mut Vx) -> Self {
        Loop {
            tty,
            vx,
            go: Arc::new(AtomicUsize::new(0)),
            done: Arc::new(AtomicUsize::new(0)),
            observed_fd: Arc::new(AtomicU64::new(0)),
            handle: None,
        }
    }

    fn start(&mut self) {
        let send = SendPtrs {
            tty: self.tty,
            vx: self.vx,
            go: Arc::clone(&self.go),
            done: Arc::clone(&self.done),
            observed_fd: Arc::clone(&self.observed_fd),
        };
        let handle = std::thread::spawn(move || {
            let s = send;
            let mut served = 0usize;
            loop {
                // wait for the main loop to request a frame (or to stop).
                while s.go.load(Ordering::Acquire) == served {
                    std::hint::spin_loop();
                }
                served = s.go.load(Ordering::Acquire);
                if served == usize::MAX {
                    break;
                }
                // Narrow projections through the raw ptrs WHILE the main thread is
                // between its own `&mut` reborrows (the interleave the proof targets):
                //  - SHARED read of the tty fd (the reference `(*tty).read(&self)`).
                let fd = unsafe { (*s.tty).read_fd() };
                s.observed_fd.store(fd as u64, Ordering::Relaxed);
                //  - narrow write into vx.caps (the reference cross-thread caps write).
                unsafe {
                    (*s.vx).caps_color_scheme_updates = true;
                }
                s.done.store(served, Ordering::Release);
            }
        });
        self.handle = Some(handle);
    }

    // request one reader frame and wait for it to complete (models poll/drain sync).
    fn serve_one(&self, n: usize) {
        self.go.store(n, Ordering::Release);
        while self.done.load(Ordering::Acquire) != n {
            std::hint::spin_loop();
        }
    }

    fn stop(&mut self) {
        self.go.store(usize::MAX, Ordering::Release);
        if let Some(h) = self.handle.take() {
            h.join().ok();
        }
    }
}

// ---- disjoint-field free fns (models cli `update` / `draw`) ----
// They reborrow ONLY `&mut tty` / `&mut vx` / `&mut st` — the SAME field paths the
// raw ptrs were cast from — never a whole-object `&mut`.
fn update(vx: &mut Vx, tty: &mut Tty, st: &mut DrawState) {
    tty.writer_tick();
    st.ticks = st.ticks.wrapping_add(1);
    st.current = st.current.wrapping_add(1);
    let _ = vx.render_ticks; // touch vx via the disjoint borrow
}

fn draw(vx: &mut Vx, st: &mut DrawState) {
    vx.render_tick();
    st.ticks = st.ticks.wrapping_add(1);
}

// The `run_preview` interleave: separate locals, Loop over raw ptrs, then a loop that
// reborrows the disjoint fields each iteration while the reader touches the raw ptrs.
fn run_interleave() -> (u64, u64, u64) {
    // === separate run-scope locals (the proven shape) ===
    let mut tty = Tty::new();
    let mut vx = Vx::new();
    let mut st = DrawState {
        should_quit: false,
        current: 0,
        ticks: 0,
    };

    // SB-CLEAN DISCIPLINE (the KEY finding): derive the raw ptrs ONCE via `&raw mut`
    // (a `SharedReadWrite` place-projection) and thereafter access `tty`/`vx` ONLY
    // through those raw ptrs (TRANSIENT `&mut *ptr` reborrows, dropped immediately) —
    // NEVER reborrow the `tty`/`vx` LOCALS directly, which would push a `Unique` that
    // pops the read thread's live raw tag off the borrow stack (the UB an earlier
    // `Loop::new(&mut tty, &mut vx)` + body-`tty.writer()` shape trips). Both threads
    // then derive children of the SAME `SharedReadWrite` parent; the serve_one sync
    // keeps their transient borrows non-overlapping.
    let tty_ptr: *mut Tty = &raw mut tty;
    let vx_ptr: *mut Vx = &raw mut vx;
    let mut loop_ = Loop::new(tty_ptr, vx_ptr);
    loop_.start();

    // main loop: each iteration, the reader touches the raw ptrs (serve_one) and the
    // main thread reborrows the disjoint fields (update/draw) THROUGH the raw ptrs.
    for frame in 1..=8usize {
        loop_.serve_one(frame); // reader derefs *tty / *vx now
                                // transient child reborrows of the SharedReadWrite parent (never the local):
        update(unsafe { &mut *vx_ptr }, unsafe { &mut *tty_ptr }, &mut st);
        draw(unsafe { &mut *vx_ptr }, &mut st);
        unsafe { (*tty_ptr).writer_tick() }; // the render+flush tail
        if st.current >= 8 {
            st.should_quit = true;
        }
    }

    loop_.stop(); // join the reader before the locals die

    // now that the reader is dead, whole-object access is sound (the deinit phase).
    (tty.write_ticks, vx.render_ticks, st.ticks)
}

fn main() {
    let (w, r, t) = run_interleave();
    println!("write_ticks={w} render_ticks={r} state_ticks={t}");
    assert!(w > 0 && r > 0 && t > 0);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interleave_no_reborrow_invalidation() {
        // Under Stacked AND Tree Borrows: the reader's raw-ptr derefs interleaved with
        // the main thread's disjoint `&mut tty`/`&mut vx`/`&mut st` reborrows must not
        // trip any borrow-stack violation. Miri interprets the whole interleave.
        let (w, r, t) = run_interleave();
        assert!(w > 0 && r > 0 && t > 0);
    }
}
