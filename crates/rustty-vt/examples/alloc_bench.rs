//! Standalone equal-scope PageList allocation bench — the Rust twin of the Zig
//! reference bench at `.port/phase-1/bench-zig-pagelist` (READ-ONLY oracle
//! harness over ghostty `src/terminal/PageList.zig`).
//!
//! WORKLOAD (identical to the Zig bench `main.zig` `workload()`; one iteration):
//!   `PageList::init(80, 24, max = 1 GiB)`
//!    -> `grow()` until `total_rows >= 2000` (forces page allocs)
//!    -> scroll across the viewport (`scroll(.{ .row = r })` for every r, then
//!       `.active`)
//!    -> `clone` (active + scrollback, full screen), clone deinit'd immediately
//!    -> `resize` cols 200 then 40 then 80 (reflow + page churn)
//!    -> `reset()` (retain_with_limit — pool capacity retained)
//!    -> deinit
//!
//! REPORTED (all MEASURED):
//!   * page_allocs        — cumulative page-pool allocations served through the
//!     `TrackingAllocator` child of the PagePool over ONE workload run (arena
//!     backing blocks + non-standard pages). Deterministic.
//!   * peak_outstanding   — high-water mark of live page-pool bytes, sampled
//!     after each workload stage. Deterministic.
//!   * peak_rss_bytes     — getrusage(RUSAGE_SELF).ru_maxrss (BYTES on macOS).
//!   * latency            — median of 9 timed runs after 1 warmup (report as
//!     NON-OFFICIAL when the machine is not idle).
//!   * per-stage accounting — `page_size`/`total_rows`/approx std pages after
//!     each stage, mirroring the Zig bench's `--accounting` output.
//!
//! RUN (must be standalone — no cargo-test/libtest overhead, production page
//! geometry since `cfg!(test)` is false in an example's library build):
//!   cargo run -p rustty-vt --example alloc_bench \
//!       --features alloc-bench --profile release-fast

use std::time::Instant;

use rustty_vt::page;
use rustty_vt::pagelist::access::Clone as CloneOpts;
use rustty_vt::pagelist::page_tracking;
use rustty_vt::pagelist::resize::Resize;
use rustty_vt::pagelist::scroll::Scroll;
use rustty_vt::pagelist::PageList;
use rustty_vt::point;

/// `max_size` — matches the Zig bench (`large_max: usize = 1024*1024*1024`).
const LARGE_MAX: usize = 1024 * 1024 * 1024;

/// Scrollback target — matches the Zig bench (`target_rows: usize = 2000`).
const TARGET_ROWS: usize = 2000;

/// One full workload iteration — the EXACT Zig bench `workload()`.
fn workload() {
    // var pl = try PageList.init(alloc, 80, 24, large_max);
    let mut pl = PageList::init(80, 24, Some(LARGE_MAX)).expect("init");

    // grow to ~2000 total rows (start at 24 active):
    // while (pl.total_rows < target_rows) _ = try pl.grow();
    while pl.total_rows < TARGET_ROWS {
        let _ = pl.grow().expect("grow");
    }

    // scroll across the viewport: walk the viewport from top to active.
    // while (r < pl.total_rows) pl.scroll(.{ .row = r }); then .active.
    for r in 0..pl.total_rows {
        pl.scroll(Scroll::Row(r));
    }
    pl.scroll(Scroll::Active);

    // clone the active + scrollback (full screen), deinit'd immediately:
    // var cloned = try pl.clone(alloc, .{ .top = .{ .screen = .{...} } });
    // cloned.deinit();
    {
        let mut opts = CloneOpts {
            top: point::Point::Screen(point::Coordinate { x: 0, y: 0 }),
            bot: None,
            tracked_pins: None,
        };
        let mut cloned = pl.clone(&mut opts).expect("clone");
        cloned.deinit();
    }

    // resize cols: more then fewer (forces reflow + page churn):
    // try pl.resize(.{ .cols = 200/40/80, .reflow = true });
    for cols in [200u16, 40, 80] {
        pl.resize(Resize {
            cols: Some(cols),
            rows: None,
            reflow: true,
            cursor: None,
        })
        .expect("resize");
    }

    // reset(retain)
    pl.reset();

    // defer pl.deinit();
    pl.deinit();
}

/// One instrumented workload run: identical stages, but samples the tracker's
/// live `outstanding_bytes` high-water mark and prints the Zig bench's
/// `--accounting` stage lines (`page_size` / `total_rows` / approx std pages).
fn workload_accounting(item_size: usize) -> usize {
    let mut peak = 0usize;
    let sample = |peak: &mut usize| {
        let b = page_tracking::with_tracker(|t| t.outstanding_bytes());
        if b > *peak {
            *peak = b;
        }
    };
    let base_allocs = page_tracking::with_tracker(|t| t.total_allocs());
    let stage = move |name: &str, pl: &PageList| {
        println!(
            "STAGE {name}: page_size={} total_rows={} approx_std_pages={} cum_page_allocs={}",
            pl.page_size,
            pl.total_rows,
            pl.page_size / item_size,
            page_tracking::with_tracker(|t| t.total_allocs()) - base_allocs,
        );
    };

    let mut pl = PageList::init(80, 24, Some(LARGE_MAX)).expect("init");
    sample(&mut peak);
    stage("init", &pl);

    let mut grow_calls = 0usize;
    let mut new_node_grows = 0usize;
    while pl.total_rows < TARGET_ROWS {
        let r = pl.grow().expect("grow");
        grow_calls += 1;
        if r.is_some() {
            new_node_grows += 1;
        }
    }
    sample(&mut peak);
    println!("STAGE grow: grow_calls={grow_calls} grow_returned_node={new_node_grows}");
    stage("grow(->2000)", &pl);

    for r in 0..pl.total_rows {
        pl.scroll(Scroll::Row(r));
    }
    pl.scroll(Scroll::Active);
    sample(&mut peak);
    stage("scroll", &pl);

    {
        let mut opts = CloneOpts {
            top: point::Point::Screen(point::Coordinate { x: 0, y: 0 }),
            bot: None,
            tracked_pins: None,
        };
        let mut cloned = pl.clone(&mut opts).expect("clone");
        // Sample while the clone's separate pool is live (its page set counts).
        sample(&mut peak);
        println!(
            "STAGE clone: clone_page_size={} clone_total_rows={} clone_approx_std_pages={}",
            cloned.page_size,
            cloned.total_rows,
            cloned.page_size / item_size,
        );
        cloned.deinit();
    }

    for cols in [200u16, 40, 80] {
        pl.resize(Resize {
            cols: Some(cols),
            rows: None,
            reflow: true,
            cursor: None,
        })
        .expect("resize");
        sample(&mut peak);
        stage(&format!("resize->{cols}"), &pl);
    }

    pl.reset();
    sample(&mut peak);
    stage("reset", &pl);

    pl.deinit();
    peak
}

/// getrusage(RUSAGE_SELF).ru_maxrss. On macOS/Darwin this field is in BYTES;
/// on Linux it is in KiB (normalized to bytes).
fn peak_rss_bytes() -> u64 {
    // SAFETY: getrusage with a zeroed `rusage` out-param is a well-defined libc
    // call; we read a scalar field from the populated struct.
    unsafe {
        let mut ru: libc::rusage = std::mem::zeroed();
        let rc = libc::getrusage(libc::RUSAGE_SELF, &mut ru);
        assert_eq!(rc, 0, "getrusage failed");
        let maxrss = ru.ru_maxrss as u64;
        #[cfg(target_os = "macos")]
        {
            maxrss // Darwin: bytes
        }
        #[cfg(not(target_os = "macos"))]
        {
            maxrss * 1024 // Linux: KiB -> bytes
        }
    }
}

/// Median of a sorted-in-place sample slice (n >= 1).
fn median(v: &mut [f64]) -> f64 {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let n = v.len();
    if n % 2 == 1 {
        v[n / 2]
    } else {
        (v[n / 2 - 1] + v[n / 2]) / 2.0
    }
}

fn main() {
    // GEOMETRY — the equal-scope precondition. In this standalone (non-test)
    // build `std_capacity()` is the production capacity, so std_size must be
    // the Zig bench's 589824 (36 x 16 KiB) on aarch64-macos.
    let cap = page::std_capacity();
    let layout = page::Page::layout(cap);
    let item_size = layout.total_size;
    println!("GEOMETRY:");
    println!(
        "std_capacity.cols={} std_capacity.rows={} std_capacity.styles={} std_capacity.grapheme_bytes={}",
        cap.cols, cap.rows, cap.styles, cap.grapheme_bytes,
    );
    println!("std_size_total_size={item_size}");

    // ---- alloc accounting: ONE instrumented run (deterministic). ----
    let base_allocs = page_tracking::with_tracker(|t| t.total_allocs());
    let peak_outstanding = workload_accounting(item_size);
    let page_allocs = page_tracking::with_tracker(|t| t.total_allocs()) - base_allocs;
    // Leak gate: everything the workload allocated must be back.
    page_tracking::with_tracker(|t| {
        assert_eq!(
            t.outstanding_allocs(),
            0,
            "page allocation leak after deinit"
        );
        assert_eq!(t.outstanding_bytes(), 0, "page byte leak after deinit");
    });

    // ---- latency: 1 warmup + 9 timed runs, median. ----
    workload(); // warmup
    let runs = 9usize;
    let mut samples = Vec::with_capacity(runs);
    for _ in 0..runs {
        let t0 = Instant::now();
        workload();
        samples.push(t0.elapsed().as_secs_f64() * 1e6); // us
    }
    let median_us = median(&mut samples);

    // ---- peak RSS after all runs (monotonic high-water mark). ----
    let rss = peak_rss_bytes();

    println!("RESULTS:");
    println!("BENCH_PAGE_ALLOCS={page_allocs}");
    println!("BENCH_PEAK_OUTSTANDING_BYTES={peak_outstanding}");
    println!("BENCH_PEAK_RSS_BYTES={rss}");
    println!("BENCH_LATENCY_MEDIAN_US={median_us:.1}");
    println!("BENCH_RUNS={runs} BENCH_WARMUP=1");
    println!(
        "BENCH_LATENCY_SAMPLES_US={}",
        samples
            .iter()
            .map(|x| format!("{x:.1}"))
            .collect::<Vec<_>>()
            .join(",")
    );
}
