use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicBool, Ordering};

use rustty_vt::pagelist::compress::CompressionMode;
use rustty_vt::pagelist::{Node, Options, PageList, Storage};

struct FailOnce;
static ARMED: AtomicBool = AtomicBool::new(false);

unsafe impl GlobalAlloc for FailOnce {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if ARMED.swap(false, Ordering::SeqCst) {
            return core::ptr::null_mut();
        }
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        if ARMED.swap(false, Ordering::SeqCst) {
            return core::ptr::null_mut();
        }
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        if ARMED.swap(false, Ordering::SeqCst) {
            return core::ptr::null_mut();
        }
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static ALLOC: FailOnce = FailOnce;

fn main() {
    if std::env::args().nth(1).as_deref() == Some("encoded-child") {
        let mut pages = cold_pages();
        ARMED.store(true, Ordering::SeqCst);
        let _ = pages.compress(CompressionMode::Full);
        eprintln!("encoded allocation unexpectedly survived injected failure");
        pages.deinit();
        std::process::exit(2);
    }

    let mut pages = cold_pages();
    let _ = pages.compress(CompressionMode::Full);
    let node = pages.pages.first;
    assert_eq!(unsafe { Node::storage(node) }, Storage::Compressed);

    ARMED.store(true, Ordering::SeqCst);
    let result = std::panic::catch_unwind(|| unsafe { Node::page_preserving_state(node) });
    assert!(result.is_err(), "preserved-page OOM must hit the current panic path");
    assert_eq!(unsafe { Node::storage(node) }, Storage::Compressed);

    println!("observed preserved-page allocation failure: panic; node remains compressed");
    pages.deinit();

    let status = std::process::Command::new(std::env::current_exe().expect("current exe"))
        .arg("encoded-child")
        .status()
        .expect("run encoded-allocation child");
    assert!(
        !status.success(),
        "encoded allocation failure must not report compression success"
    );
    println!("observed encoded-buffer allocation failure: child status {status}");
}

fn cold_pages() -> PageList {
    let mut pages = PageList::init(Options::new(80, 24)).expect("PageList init");
    for _ in 0..2_000 {
        pages.grow().expect("grow");
    }
    pages
}
