//! TEST-ONLY allocation / latency / RSS bench harness for the PageList page-alloc
//! paths (§4 — quantify the audit-fix wins).
//!
//! Run with:
//!   `cargo test -p rustty-vt --release pagelist::bench -- --nocapture --ignored`
//!
//! It is `#[ignore]`d so it never runs in the normal test sweep, and lives inside
//! the crate so it can reach the `pub(crate)` workload primitives (`grow_rows`)
//! and the `page_tracking` TrackingAllocator seam that counts page allocs/bytes.
//!
//! ALL reported numbers are MEASURED:
//!   * latency_ms       — median wall time of >=5 full-workload runs.
//!   * peak_rss_bytes   — getrusage(RUSAGE_SELF).ru_maxrss (macOS reports BYTES).
//!   * page_allocs      — `TrackerState::total_allocs()` delta over ONE workload
//!     run (cumulative page-pool allocations served through the
//!     `TrackingAllocator` child of the PagePool — arena
//!     backing blocks + non-standard pages).
//!   * page_alloc_bytes — peak `outstanding_bytes()` (high-water mark of live
//!     page-pool bytes) sampled across ONE workload run.
//!   * binary_size      — measured separately by the runner (rlib size).

use std::time::Instant;

use super::access::Clone as CloneOpts;
use super::resize::Resize;
use super::scroll::Scroll;
use super::{page_tracking, PageList};
use crate::point;

/// The identical workload exercised by EVERY measurement:
///   init(80,24,large_max) -> grow_rows(~2000 scrollback) -> scroll across the
///   viewport -> clone active+scrollback -> resize cols (more then fewer; forces
///   reflow + page churn) -> reset(retain) -> deinit.
fn workload() {
    // init(80, 24, large_max). A large max_size lets the ~2000 scrollback rows
    // accumulate without trimming, forcing many page allocations.
    let mut s = PageList::init(80, 24, Some(64 * 1024 * 1024)).unwrap();

    // grow_rows to ~2000 scrollback rows (24 active + 2000 scrollback). Each
    // overflow past a page's row capacity forces a new page-pool allocation.
    s.grow_rows(2000).unwrap();

    // Scroll across the viewport: top, then walk back down to the active area in
    // row deltas, then settle on active.
    s.scroll(Scroll::Top);
    for _ in 0..40 {
        s.scroll(Scroll::DeltaRow(50));
    }
    s.scroll(Scroll::Active);

    // Clone the active + full scrollback (top..=end). Cloning rebuilds the whole
    // page list in a fresh pool — a second burst of page allocations.
    let mut clone_opts = CloneOpts {
        top: point::Point::Screen(point::Coordinate { x: 0, y: 0 }),
        bot: None,
        tracked_pins: None,
    };
    let mut s2 = s.clone(&mut clone_opts).unwrap();

    // Resize cols: grow (80 -> 120) then shrink (120 -> 40), both with reflow.
    // Reflow re-lays every row across new page geometry, churning pages.
    s.resize(Resize {
        cols: Some(120),
        rows: None,
        reflow: true,
        cursor: None,
    })
    .unwrap();
    s.resize(Resize {
        cols: Some(40),
        rows: None,
        reflow: true,
        cursor: None,
    })
    .unwrap();

    // reset (retain the PageList, free scrollback back to the pools).
    s.reset();

    // deinit both.
    s2.deinit();
    s.deinit();
}

/// Median of a slice (sorts a copy; n>=1).
fn median(v: &mut [f64]) -> f64 {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let n = v.len();
    if n % 2 == 1 {
        v[n / 2]
    } else {
        (v[n / 2 - 1] + v[n / 2]) / 2.0
    }
}

/// getrusage(RUSAGE_SELF).ru_maxrss. On macOS/Darwin this field is in BYTES;
/// on Linux it is in KiB (we normalize to bytes for the caller's platform note).
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

#[test]
#[ignore = "release bench harness — run explicitly with --release --ignored --nocapture"]
fn pagelist_alloc_bench() {
    // ---- alloc-count + peak page bytes: ONE instrumented workload run. ----
    // Snapshot the thread-local page tracker, run the workload while sampling the
    // live-bytes high-water mark, then read the cumulative-alloc delta.
    let (base_total_allocs, _base_bytes) =
        page_tracking::with_tracker(|t| (t.total_allocs(), t.outstanding_bytes()));

    // Run the workload once for the alloc-count snapshot. We sample the peak live
    // bytes by checking after each heavy phase via a wrapper.
    let peak_bytes = run_workload_sampling_peak();

    let total_allocs_after = page_tracking::with_tracker(|t| t.total_allocs());
    let page_allocs = total_allocs_after - base_total_allocs;
    let page_alloc_bytes = peak_bytes;

    // ---- latency: median of >=5 full-workload runs (release). ----
    // Warm up once to fault in lazy pages / amortize first-touch.
    workload();
    let runs = 9usize;
    let mut samples = Vec::with_capacity(runs);
    for _ in 0..runs {
        let t0 = Instant::now();
        workload();
        samples.push(t0.elapsed().as_secs_f64() * 1000.0);
    }
    let latency_ms = median(&mut samples);

    // ---- peak RSS: after all runs (high-water mark is monotonic). ----
    let rss = peak_rss_bytes();

    // Emit a machine-greppable block the runner captures to /tmp/bench_new.txt.
    println!("BENCH_LABEL=new");
    println!("BENCH_LATENCY_MS={latency_ms:.4}");
    println!("BENCH_PEAK_RSS_BYTES={rss}");
    println!("BENCH_PAGE_ALLOCS={page_allocs}");
    println!("BENCH_PAGE_ALLOC_BYTES={page_alloc_bytes}");
    println!("BENCH_RUNS={runs}");
    println!(
        "BENCH_LATENCY_SAMPLES_MS={}",
        samples
            .iter()
            .map(|x| format!("{x:.4}"))
            .collect::<Vec<_>>()
            .join(",")
    );
}

/// One workload run that samples the page tracker's live `outstanding_bytes`
/// high-water mark after each heavy page-churn phase, returning the peak. This is
/// a copy of [`workload`] with sampling points; behavior is identical.
fn run_workload_sampling_peak() -> usize {
    let mut peak = 0usize;
    let sample = |peak: &mut usize| {
        let b = page_tracking::with_tracker(|t| t.outstanding_bytes());
        if b > *peak {
            *peak = b;
        }
    };

    let mut s = PageList::init(80, 24, Some(64 * 1024 * 1024)).unwrap();
    sample(&mut peak);
    s.grow_rows(2000).unwrap();
    sample(&mut peak);

    s.scroll(Scroll::Top);
    for _ in 0..40 {
        s.scroll(Scroll::DeltaRow(50));
    }
    s.scroll(Scroll::Active);
    sample(&mut peak);

    let mut clone_opts = CloneOpts {
        top: point::Point::Screen(point::Coordinate { x: 0, y: 0 }),
        bot: None,
        tracked_pins: None,
    };
    let mut s2 = s.clone(&mut clone_opts).unwrap();
    sample(&mut peak);

    s.resize(Resize {
        cols: Some(120),
        rows: None,
        reflow: true,
        cursor: None,
    })
    .unwrap();
    sample(&mut peak);
    s.resize(Resize {
        cols: Some(40),
        rows: None,
        reflow: true,
        cursor: None,
    })
    .unwrap();
    sample(&mut peak);

    s.reset();
    s2.deinit();
    s.deinit();
    peak
}
