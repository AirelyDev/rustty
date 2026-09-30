//! BL-2 empirical verification probe (WS8 T13) for `pagelist::decommit_pool_item`.
//!
//! `madvise` has no observable effect on program semantics — only on resident
//! set size — so it cannot be asserted by a normal unit test. It also cannot be
//! exercised from one: the `madvise` arms of `decommit_pool_item` are
//! `#[cfg(not(test))]`, so ANY `cargo test` build (including `--release`) runs
//! the plain-memset arm instead, and the memset actually *commits* the pages,
//! driving RSS up rather than down. This is the same reason `examples/alloc_bench.rs`
//! exists: a standalone example is the only way to observe the library's
//! PRODUCTION page geometry and production `cfg` arms.
//!
//! MEASURED (verified by running it, see the WS8 T13 commit body):
//!   * RSS after growing ~10 MiB of scrollback
//!   * RSS after clearing that scrollback (the pooled buffers return to the
//!     free list and are decommitted)
//!
//! Expected: "after clear" drops back near "before grow". WITHOUT the decommit
//! the "after clear" figure stays at the "after grow" high-water mark.
//!
//! RUN:
//!   cargo run -p rustty-vt --example rss_probe --release
//!
//! Exit code is non-zero if the scrollback clear did not return at least 2 MiB.
//!
//! PORT NOTE (60b43068c, P7-W1 T6): this is also the Windows reclaim smoke run
//! (DECISIONS D3) — `decommit`'s `DiscardVirtualMemory` arm is `#[cfg(windows)]`
//! but still unreachable from any `cargo test` build (`cfg!(test)` always takes
//! the simulate-by-memset arm first), so this release example is the only place
//! that ever calls the real syscall. `rss_bytes` reads `WorkingSetSize` via
//! `K32GetProcessMemoryInfo` on Windows, the resident-memory analog of Darwin's
//! `phys_footprint` and Linux's `statm` resident field.

use rustty_vt::pagelist::Options;
use rustty_vt::PageList;

// macOS: `MADV_FREE_REUSABLE` removes pages from the process PHYSICAL FOOTPRINT
// immediately; it does not necessarily move `ps -o rss`. `phys_footprint` (the
// number Activity Monitor shows as "Memory") is therefore the metric that
// reflects the decommit. It comes from `proc_pid_rusage(RUSAGE_INFO_V0)`.
//
// PORT NOTE (platform primitive): hand-rolled `extern "C"` binding matching
// the existing convention of `page.rs`'s `Backing::alloc`/`Backing::free`
// `mmap`/`munmap` bindings, even though `libc` is already a dependency of
// this crate (used elsewhere, e.g. `rustty-os`'s page allocator).
#[cfg(target_os = "macos")]
#[repr(C)]
#[derive(Default)]
struct RUsageInfoV0 {
    ri_uuid: [u8; 16],
    ri_user_time: u64,
    ri_system_time: u64,
    ri_pkg_idle_wkups: u64,
    ri_interrupt_wkups: u64,
    ri_pageins: u64,
    ri_wired_size: u64,
    ri_resident_size: u64,
    ri_phys_footprint: u64,
    ri_proc_start_abstime: u64,
    ri_proc_exit_abstime: u64,
}

#[cfg(target_os = "macos")]
extern "C" {
    fn proc_pid_rusage(pid: i32, flavor: i32, buffer: *mut core::ffi::c_void) -> i32;
    fn getpid() -> i32;
}

/// Resident set size of this process, in bytes (macOS: physical footprint).
fn rss_bytes() -> u64 {
    #[cfg(target_os = "macos")]
    {
        const RUSAGE_INFO_V0: i32 = 0;
        let mut info = RUsageInfoV0::default();
        // SAFETY: `info` is a live, correctly-sized `rusage_info_v0`.
        let rc = unsafe {
            proc_pid_rusage(
                getpid(),
                RUSAGE_INFO_V0,
                &mut info as *mut RUsageInfoV0 as *mut core::ffi::c_void,
            )
        };
        assert_eq!(0, rc, "proc_pid_rusage failed");
        info.ri_phys_footprint
    }
    #[cfg(target_os = "linux")]
    {
        let statm = std::fs::read_to_string("/proc/self/statm").expect("statm");
        let pages: u64 = statm
            .split_whitespace()
            .nth(1)
            .expect("resident field")
            .parse()
            .expect("resident pages");
        pages * 4096
    }
    #[cfg(windows)]
    {
        // PORT NOTE (60b43068c, P7-W1 T6): `WorkingSetSize` is the resident
        // (physically backed) portion of the process's address space — the
        // Windows analog of Darwin's `phys_footprint` and Linux's `statm`
        // resident field. `DiscardVirtualMemory` releases physical pages
        // while keeping the range committed, so it shows up here even though
        // the commit charge (and thus `PagefileUsage`) does not move.
        use std::os::raw::c_void;

        #[repr(C)]
        #[derive(Default)]
        struct ProcessMemoryCountersEx {
            cb: u32,
            page_fault_count: u32,
            peak_working_set_size: usize,
            working_set_size: usize,
            quota_peak_paged_pool_usage: usize,
            quota_paged_pool_usage: usize,
            quota_peak_non_paged_pool_usage: usize,
            quota_non_paged_pool_usage: usize,
            pagefile_usage: usize,
            peak_pagefile_usage: usize,
            private_usage: usize,
        }

        extern "system" {
            fn GetCurrentProcess() -> *mut c_void;
        }
        // PORT NOTE: `K32GetProcessMemoryInfo` is exported by psapi.dll, not
        // kernel32.dll, despite the `K32` prefix Microsoft gives the whole
        // family — psapi.lib must be linked explicitly.
        #[link(name = "psapi")]
        extern "system" {
            fn K32GetProcessMemoryInfo(
                process: *mut c_void,
                counters: *mut ProcessMemoryCountersEx,
                size: u32,
            ) -> i32;
        }

        let mut counters = ProcessMemoryCountersEx {
            cb: std::mem::size_of::<ProcessMemoryCountersEx>() as u32,
            ..Default::default()
        };
        // SAFETY: `counters` is live and correctly sized for the `cb` field
        // it reports, matching the `PROCESS_MEMORY_COUNTERS_EX` contract.
        let ok =
            unsafe { K32GetProcessMemoryInfo(GetCurrentProcess(), &mut counters, counters.cb) };
        assert_ne!(0, ok, "GetProcessMemoryInfo failed");
        counters.working_set_size as u64
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
    {
        0
    }
}

fn mib(bytes: u64) -> f64 {
    bytes as f64 / (1024.0 * 1024.0)
}

fn main() {
    let before = rss_bytes();

    // Grow ~10 MiB of scrollback (the default scrollback limit), then clear it.
    let mut list = PageList::init(
        rustty_vt::alloc::Global,
        Options {
            max_size: Some(1 << 30),
            ..Options::new(80, 24)
        },
    )
    .unwrap();
    while list.page_size < 10_000_000 {
        let _ = list.grow().unwrap();
    }

    // Touch every grown page so the lazily-mapped memory is actually resident;
    // otherwise "after grow" would not reflect the scrollback at all.
    {
        use rustty_datastruct::intrusive_linked_list::Node as _;
        let mut node = list.pages.first;
        while !node.is_null() {
            // Metadata-only: the mapping is all this probe needs, and using
            // `Node::page` here would restore any compressed page (WS7 B5).
            let page = unsafe { &*rustty_vt::pagelist::Node::metadata(node) };
            let ptr = page.memory_ptr() as *mut u8;
            let len = page.memory_len();
            let mut off = 0usize;
            while off < len {
                unsafe { core::ptr::write_volatile(ptr.add(off), 1u8) };
                off += 4096;
            }
            node = unsafe { (*node).next() };
        }
    }

    let after_grow = rss_bytes();
    list.erase_history(None);
    let after_clear = rss_bytes();

    println!(
        "RSS before={:.1} MiB  after_grow={:.1} MiB  after_clear={:.1} MiB  \
         (page_size={:.1} MiB)",
        mib(before),
        mib(after_grow),
        mib(after_clear),
        mib(list.page_size as u64),
    );

    let returned = after_grow.saturating_sub(after_clear);
    println!("returned to the OS: {:.1} MiB", mib(returned));

    list.deinit();

    if returned < 2 * 1024 * 1024 {
        eprintln!(
            "FAIL: scrollback clear did not return RSS: grow={after_grow} clear={after_clear}"
        );
        std::process::exit(1);
    }
    println!("PASS");
}
