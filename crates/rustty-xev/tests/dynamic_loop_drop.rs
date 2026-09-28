//! N406 (parity tail L-5): the Linux `Loop` (`dynamic.zig`'s `Loop`, an
//! untagged union of the io_uring and epoll loops) must free its backend loop's
//! heap when it is dropped. The union arms are `ManuallyDrop`, and before
//! `impl Drop for Loop` the epoll arm's boxed `thread_pool_completions` queue
//! leaked with every loop, i.e. with every termio thread (found by
//! G-VALGRIND's first run over rustty-termio).
//!
//! A test binary of its own because it installs a counting global allocator.
//! It drives the epoll arm, the one that owns heap; the io_uring arm goes
//! through the same `Drop` match.
#![cfg(target_os = "linux")]

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicIsize, Ordering};

use rustty_xev::{Backend, Loop, Options};

/// Net bytes currently allocated through the global allocator.
static LIVE: AtomicIsize = AtomicIsize::new(0);

struct Counting;

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let p = unsafe { System.alloc(layout) };
        if !p.is_null() {
            LIVE.fetch_add(layout.size() as isize, Ordering::SeqCst);
        }
        p
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let p = unsafe { System.alloc_zeroed(layout) };
        if !p.is_null() {
            LIVE.fetch_add(layout.size() as isize, Ordering::SeqCst);
        }
        p
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        LIVE.fetch_sub(layout.size() as isize, Ordering::SeqCst);
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let p = unsafe { System.realloc(ptr, layout, new_size) };
        if !p.is_null() {
            LIVE.fetch_add(new_size as isize - layout.size() as isize, Ordering::SeqCst);
        }
        p
    }
}

// D6-ALLOW(counting shim, not an allocator substitution): an integration-test
// binary of its own, so it never enters a shipped build; every arm forwards to
// `std::alloc::System` (plain malloc/free, which the ASan/LSan interceptors and
// valgrind still see) and only keeps a byte count.
#[global_allocator]
static COUNTING: Counting = Counting;

/// Init, deinit and drop one loop on the active backend; return the net bytes
/// it left allocated and the bytes it held while alive.
fn one_loop() -> (isize, isize) {
    let before = LIVE.load(Ordering::SeqCst);
    let mut l = Loop::init(Options::default()).expect("Loop::init");
    let held = LIVE.load(Ordering::SeqCst) - before;
    l.deinit();
    drop(l);
    (LIVE.load(Ordering::SeqCst) - before, held)
}

#[test]
fn a_dropped_loop_frees_its_backend_loop() {
    // One test in this binary, so nothing else allocates while it counts. The
    // first loop may create lazy process-wide state; measure the second.
    let _ = one_loop();

    // The epoll arm owns heap (the boxed completion queue): it must be held
    // while the loop lives, and returned when it drops.
    assert!(
        rustty_xev::prefer(Backend::Epoll),
        "epoll is always available"
    );
    let (left, held) = one_loop();
    assert!(
        held > 0,
        "the epoll loop holds heap while alive ({held} bytes)"
    );
    assert_eq!(
        left, 0,
        "the epoll loop frees all {held} bytes when dropped"
    );
}
