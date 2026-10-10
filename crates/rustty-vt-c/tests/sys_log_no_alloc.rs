//! sys_log_no_alloc.rs — the C-ABI log sink formats WITHOUT heap allocation
//! (codex-vet W36-B04).
//!
//! ORACLE: ghostty src/terminal/c/sys.zig:147-207. `logFn` builds a
//! `std.Io.Writer` over a `[2048]u8` STACK buffer (`LogEmitter.drain` emits
//! each full chunk through the installed C callback) and `writer.print(format,
//! args)` streams the formatted bytes straight into it. The doc comment on
//! `LogEmitter` states the guarantee outright: "this allows emitting larger
//! logs without heap allocation". A C embedder can therefore install a log
//! callback and know the sink never touches the allocator, no matter how large
//! the message is.
//!
//! THE DIVERGENCE THIS TEST PINS: rustty's `sys::log_fn` used to render the
//! message with `std::fmt::format(args)` — one heap `String` per log message,
//! sized to the WHOLE message — before feeding the 2048-byte chunker. The
//! C-ABI-visible bytes/chunk boundaries were identical, so no existing test
//! could see it; only the allocation behavior diverged. `log_fn` now writes
//! through a `core::fmt::Write` impl on `LogEmitter` (the direct analogue of
//! Zig's writer), so formatting streams into the stack buffer.
//!
//! WHY ITS OWN INTEGRATION BINARY: it installs a counting `#[global_allocator]`,
//! which must not be imposed on the crate's other test binaries.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::ffi::c_void;
use std::sync::atomic::{AtomicUsize, Ordering};

use rustty_vt_c::sys::{self, LogLevel, StdLevel};

/// Counts allocations made by the MEASURING thread while it is ARMED. Disarmed
/// by default so unrelated test harness/setup allocations are invisible.
struct CountingAlloc;

thread_local! {
    /// Per-thread arm flag (N547, P8-W10 FX11). It was a process-wide
    /// `AtomicBool`, so ANY thread's allocation inside `measure` counted:
    /// libtest's main thread, after spawning the test thread, is still
    /// entering `rx.recv_timeout` (the lazy mpmc `Context` TLS init, the
    /// waker-entry `Vec` growth, a lazy pthread mutex box), and under TSan's
    /// slowdown that overlapped the measured `log_fn` call in ~2% of runs
    /// (G-TSAN, Mac; probe: every stray allocation had `pthread_main_np() == 1`
    /// while `measure` ran on the test thread). `log_fn` formats synchronously
    /// on its caller's thread, so arming only that thread loses nothing it
    /// could allocate. `const` init and no destructor: reading it from inside
    /// the global allocator never allocates or touches a torn-down TLS slot.
    static ARMED: Cell<bool> = const { Cell::new(false) };
}
static ALLOCS: AtomicUsize = AtomicUsize::new(0);

/// True when the CURRENT thread is the armed, measuring one.
fn armed() -> bool {
    ARMED.with(Cell::get)
}

unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if armed() {
            ALLOCS.fetch_add(1, Ordering::Relaxed);
        }
        // SAFETY: forwarding the caller's contract to the system allocator.
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: forwarding the caller's contract to the system allocator.
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        if armed() {
            ALLOCS.fetch_add(1, Ordering::Relaxed);
        }
        // SAFETY: forwarding the caller's contract to the system allocator.
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

// D6-ALLOW(counting shim, not an allocator substitution): this lives in an
// integration test target (`tests/`), so it exists only in this one test binary
// and can never enter a shipped build. It does not blind ASan/LSan the way
// oven-sh/bun#30875's mimalloc did: every arm forwards to `std::alloc::System`,
// i.e. plain `malloc`/`realloc`/`free`, which the sanitizer interceptors still
// see; it only bumps a counter first, and only while ARMED. Same shape and
// rationale as the D6-ALLOW justifications in
// `rustty-vt/tests/viewport_fingerprint_alloc.rs` and
// `rustty-renderer/src/link.rs`.
#[global_allocator]
static ALLOCATOR: CountingAlloc = CountingAlloc;

static CHUNKS: AtomicUsize = AtomicUsize::new(0);
static TOTAL_LEN: AtomicUsize = AtomicUsize::new(0);

/// Allocation-free capturing callback (the `S.logCb` shape of the sys.zig
/// logFn tests: count the chunks, sum the lengths).
extern "C" fn counting_log_cb(
    _userdata: *mut c_void,
    _level: LogLevel,
    _scope_ptr: *const u8,
    _scope_len: usize,
    _msg_ptr: *const u8,
    msg_len: usize,
) {
    CHUNKS.fetch_add(1, Ordering::Relaxed);
    TOTAL_LEN.fetch_add(msg_len, Ordering::Relaxed);
    // Each chunk must not exceed the stack buffer size.
    assert!(msg_len <= 2048);
}

fn measure(f: impl FnOnce()) -> usize {
    ALLOCS.store(0, Ordering::Relaxed);
    ARMED.with(|a| a.set(true));
    f();
    ARMED.with(|a| a.set(false));
    ALLOCS.load(Ordering::Relaxed)
}

#[test]
fn log_fn_dispatch_is_allocation_free() {
    // Install the callback through the public C entry point (option 2 = log).
    let cb: sys::LogFn = counting_log_cb;
    let rc = unsafe { rustty_vt_c::rustty_sys_set(2, cb as *const c_void) };
    assert_eq!(rc, 0, "rustty_sys_set(RUSTTY_SYS_OPT_LOG) must succeed");

    // Warm-up: one unmeasured call, so any ONE-TIME lazy initialization
    // (e.g. a platform mutex's first-lock init) is not attributed to the
    // steady-state path this test measures.
    sys::log_fn(StdLevel::Info, "warmup", format_args!("warm {}", 1));

    // A short message with runtime arguments: the whole thing fits in the
    // 2048-byte stack buffer and must not touch the allocator.
    CHUNKS.store(0, Ordering::Relaxed);
    TOTAL_LEN.store(0, Ordering::Relaxed);
    let short_allocs = measure(|| {
        sys::log_fn(
            StdLevel::Warn,
            "build_info_c",
            format_args!("build_info invalid data value={}", 999u32),
        );
    });
    assert_eq!(CHUNKS.load(Ordering::Relaxed), 1);
    assert_eq!(TOTAL_LEN.load(Ordering::Relaxed), 33);
    assert_eq!(
        short_allocs, 0,
        "sys::log_fn must format into the stack buffer, not the heap \
         (ghostty sys.zig:147-207 emits \"without heap allocation\")"
    );

    // A message far larger than the stack buffer: the pre-fix implementation
    // allocated a 3000-byte String here before chunking. Streaming formatting
    // still allocates nothing, and the chunking is unchanged.
    let fill = [0x41u8; 3000];
    let fill_str = core::str::from_utf8(&fill).unwrap();
    CHUNKS.store(0, Ordering::Relaxed);
    TOTAL_LEN.store(0, Ordering::Relaxed);
    let long_allocs = measure(|| {
        sys::log_fn(StdLevel::Err, "", format_args!("{}", fill_str));
    });
    assert_eq!(CHUNKS.load(Ordering::Relaxed), 2);
    assert_eq!(TOTAL_LEN.load(Ordering::Relaxed), 3000);
    assert_eq!(
        long_allocs, 0,
        "an oversized message must chunk through the stack buffer with no heap \
         allocation (ghostty sys.zig:147-160)"
    );

    // Restore the pristine "no callback installed" global state.
    let rc = unsafe { rustty_vt_c::rustty_sys_set(2, std::ptr::null()) };
    assert_eq!(rc, 0);
}
