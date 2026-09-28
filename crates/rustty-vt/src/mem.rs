//! Virtual-memory operations shared by terminal page owners.
//!
//! Terminal pages use page-aligned, page-multiple mappings. This module can
//! discard the physical pages behind one of those mappings without releasing
//! its virtual address range, then prepare the same range for reuse. It does
//! not allocate memory or decide which terminal pages should be discarded.
//!
//! PORTS: ghostty src/terminal/mem.zig (oracle origin/main @ f6fd4cb08).
//!
//! PORT NOTE (WS7): the oracle's `[]align(page_size_min) u8` argument becomes a
//! raw pointer + length here. A `&mut [u8]` over page memory would be a live
//! `&mut` alias of the same bytes a `*mut Page` also names, which this port
//! treats as Stacked-Borrows UB (see the WS7 plan's raw-pointer discipline).
//!
//! PORT NOTE (WS7): the oracle reaches `madvise` through `std.posix`. rustty
//! deliberately does NOT depend on the `libc` crate here, so this declares the
//! one syscall it needs with `extern "C"` exactly like the hand-rolled
//! `mmap`/`munmap` in `page.rs`'s `Backing::alloc`/`Backing::free`.
//!
//! PORT NOTE (WS7 / BL-2 reconciliation): `pagelist/mod.rs` grew a hand-rolled
//! `decommit_pool_item`/`recommit_pool_item` pair in BL-2, before this module
//! existed upstream. Upstream subsequently replaced those helpers with direct
//! `terminal_mem.decommit(.zero, ...)` / `terminal_mem.recommit(...)` calls
//! (see `oracle@main:src/terminal/PageList.zig:690, :3897, :4379`), so the
//! rustty helpers are now thin wrappers over this module.

use crate::page::PAGE_SIZE_MIN;

// const log = std.log.scoped(.terminal_mem);
// The scope, carried through the `log` facade as `target: LOG_SCOPE`
// (DECISION-logging.md §6r4).
// CORRECTION (parity S156-F3): the three `log.warn` calls below were
// comment-only.
#[cfg_attr(
    not(any(
        target_os = "macos",
        target_os = "ios",
        target_os = "linux",
        target_os = "android"
    )),
    allow(dead_code)
)]
const LOG_SCOPE: &str = "terminal_mem";

/// What guarantee decommit must provide when the OS cannot discard a mapping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecommitMode {
    /// The dirty prefix must read as zero after this call, even when physical
    /// reclamation is unavailable. Bytes after the prefix must already be zero.
    Zero,

    /// Physical-memory reclamation is required. Do not touch the mapping when
    /// reclamation is unsupported or fails; report the failure to the caller.
    Strict,
}

// -- madvise bindings ------------------------------------------------------
//
// PORT NOTE: values verified against <sys/mman.h> / <asm-generic/mman-common.h>
// on both targets.
//   Darwin: MADV_DONTNEED 4, MADV_FREE 5, MADV_FREE_REUSABLE 7, MADV_FREE_REUSE 8
//   Linux and Android: MADV_DONTNEED 4, the asm-generic value, which x86_64,
//           aarch64 and arm (armv7-linux-androideabi) all use
#[cfg(any(target_os = "linux", target_os = "android"))]
const MADV_DONTNEED: i32 = 4;
#[cfg(any(target_os = "macos", target_os = "ios"))]
const MADV_FREE: i32 = 5;
#[cfg(any(target_os = "macos", target_os = "ios"))]
const MADV_FREE_REUSABLE: i32 = 7;
#[cfg(any(target_os = "macos", target_os = "ios"))]
const MADV_FREE_REUSE: i32 = 8;

#[cfg(any(
    target_os = "macos",
    target_os = "ios",
    target_os = "linux",
    target_os = "android"
))]
extern "C" {
    #[link_name = "madvise"]
    fn libc_madvise(addr: *mut core::ffi::c_void, len: usize, advice: i32) -> i32;
}

/// `std.posix.MadviseError`, the error `std.posix.madvise` maps each errno to
/// (zig 0.16 `std/posix.zig:1408-1421`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)] // each errno arm is reachable only on some targets
enum MadviseError {
    PermissionDenied,
    AccessDenied,
    SystemResources,
    InvalidSyscall,
    WouldExceedMaximumResidentSetSize,
    OutOfMemory,
    MadviseUnavailable,
    Unexpected,
}

impl MadviseError {
    // switch (errno(system.madvise(ptr, length, advice))) {
    //     .SUCCESS => return,
    //     .PERM => return error.PermissionDenied,
    //     .ACCES => return error.AccessDenied,
    //     .AGAIN => return error.SystemResources,
    //     .BADF => unreachable, // The map exists, but the area maps something that isn't a file.
    //     .INVAL => return error.InvalidSyscall,
    //     .IO => return error.WouldExceedMaximumResidentSetSize,
    //     .NOMEM => return error.OutOfMemory,
    //     .NOSYS => return error.MadviseUnavailable,
    //     else => |err| return unexpectedErrno(err),
    // }
    // PORT NOTE: the std `.BADF => unreachable` arm is `Unexpected` here; the
    // callers pass anonymous page mappings, so the kernel never reports it,
    // and a std-library `unreachable` is not an invariant this crate asserts.
    #[cfg(any(
        target_os = "macos",
        target_os = "ios",
        target_os = "linux",
        target_os = "android"
    ))]
    fn from_errno(errno: i32) -> MadviseError {
        match errno {
            libc_errno::EPERM => MadviseError::PermissionDenied,
            libc_errno::EACCES => MadviseError::AccessDenied,
            libc_errno::EAGAIN => MadviseError::SystemResources,
            libc_errno::EINVAL => MadviseError::InvalidSyscall,
            libc_errno::EIO => MadviseError::WouldExceedMaximumResidentSetSize,
            libc_errno::ENOMEM => MadviseError::OutOfMemory,
            libc_errno::ENOSYS => MadviseError::MadviseUnavailable,
            _ => MadviseError::Unexpected,
        }
    }
}

/// The errno values `MadviseError::from_errno` distinguishes. They are the
/// same on Darwin and on Linux/Android (the asm-generic values, used by x86_64,
/// aarch64 and arm) except `EAGAIN` and `ENOSYS`.
#[cfg(any(
    target_os = "macos",
    target_os = "ios",
    target_os = "linux",
    target_os = "android"
))]
mod libc_errno {
    pub const EPERM: i32 = 1;
    pub const EIO: i32 = 5;
    pub const ENOMEM: i32 = 12;
    pub const EACCES: i32 = 13;
    pub const EINVAL: i32 = 22;
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    pub const EAGAIN: i32 = 35;
    #[cfg(any(target_os = "linux", target_os = "android"))]
    pub const EAGAIN: i32 = 11;
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    pub const ENOSYS: i32 = 78;
    #[cfg(any(target_os = "linux", target_os = "android"))]
    pub const ENOSYS: i32 = 38;
}

/// Issue one `madvise` call (`std.posix.madvise`).
///
/// # Safety
/// `ptr` must be a live, page-aligned mapping of at least `len` bytes.
#[cfg(any(
    target_os = "macos",
    target_os = "ios",
    target_os = "linux",
    target_os = "android"
))]
#[inline]
unsafe fn madvise(ptr: *mut u8, len: usize, advice: i32) -> Result<(), MadviseError> {
    // SAFETY: caller contract — `ptr` is a live page-aligned mapping of `len`
    // bytes; `madvise` never dereferences it from Rust's perspective.
    if unsafe { libc_madvise(ptr as *mut core::ffi::c_void, len, advice) } == 0 {
        return Ok(());
    }
    let errno = std::io::Error::last_os_error().raw_os_error().unwrap_or(0);
    Err(MadviseError::from_errno(errno))
}

/// # Safety
/// See the POSIX variant above; this stub never touches memory.
#[cfg(not(any(
    target_os = "macos",
    target_os = "ios",
    target_os = "linux",
    target_os = "android"
)))]
#[inline]
#[allow(dead_code)] // no caller on targets without retained-mapping reclaim (e.g. wasm32)
unsafe fn madvise(_ptr: *mut u8, _len: usize, _advice: i32) -> Result<(), MadviseError> {
    Err(MadviseError::MadviseUnavailable)
}

/// Return whether this target can reclaim physical memory for `mode` while
/// retaining the mapping's virtual address range.
///
/// Test builds support both modes because `decommit` simulates reclamation by
/// clearing the supplied range. Runtime reclamation is intentionally limited
/// to 64-bit Linux and Darwin. Other targets must leave strict callers' memory
/// resident; zero mode still provides its documented memset fallback through
/// `decommit` even when this function returns false.
///
/// PORTS: oracle@main mem.zig `canReclaim`.
#[inline]
pub fn can_reclaim(mode: DecommitMode) -> bool {
    // Both modes use the same retained-mapping primitives. Keeping the match
    // exhaustive makes additions to DecommitMode choose target support
    // explicitly rather than inheriting it accidentally.
    match mode {
        DecommitMode::Zero | DecommitMode::Strict => {
            // Tests never call into the OS because their allocator ranges can
            // share mappings with unrelated allocations. `decommit` simulates
            // successful reclamation by zeroing the requested range instead,
            // so both modes are always available to tests on every target.
            // `cfg!(miri)` joins them (see the note in `decommit`).
            if cfg!(test) || cfg!(miri) || cfg!(feature = "test-decommit-sim") {
                return true;
            }

            // Compression currently retains complete page mappings for its
            // lifetime. Limit the initial runtime support to 64-bit address
            // spaces where that virtual-memory cost is negligible and where
            // the retained-mapping behavior has been validated.
            if cfg!(not(target_pointer_width = "64")) {
                return false;
            }

            // Linux provides MADV_DONTNEED, which immediately discards pages
            // from a private anonymous mapping and faults them back as zeroes.
            // Darwin provides the paired MADV_FREE_REUSABLE/FREE_REUSE
            // operations used below.
            //
            // Other targets have no retained-mapping reclamation contract in
            // this module. Zero mode can still clear through its memset
            // fallback, but strict callers must leave their mapping resident.
            cfg!(any(
                target_os = "linux",
                target_os = "android",
                target_os = "macos",
                target_os = "ios"
            ))
        }
    }
}

/// Discard physical pages while retaining a mapping's virtual address range.
///
/// The complete mapping must be page-aligned and a multiple of the minimum
/// system page size. `dirty_len` identifies the prefix whose contents may be
/// nonzero. Strict mode requires the complete mapping to be dirty because a
/// successful discard invalidates all of its contents.
///
/// The return value reports whether the OS accepted the reclamation request.
/// Test builds return true after simulating reclamation by zeroing dirty bytes.
/// In zero mode, the requested bytes are guaranteed to be zero regardless of
/// the return value.
///
/// PORTS: oracle@main mem.zig `decommit`.
///
/// # Safety
/// `ptr` must be a live, page-aligned mapping of at least `len` bytes with no
/// live Rust reference aliasing it, and `dirty_len <= len`.
pub unsafe fn decommit(mode: DecommitMode, ptr: *mut u8, len: usize, dirty_len: usize) -> bool {
    debug_assert!(len > 0);
    debug_assert!((ptr as usize).is_multiple_of(PAGE_SIZE_MIN));
    debug_assert!(len.is_multiple_of(PAGE_SIZE_MIN));
    debug_assert!(dirty_len <= len);
    if matches!(mode, DecommitMode::Strict) {
        debug_assert!(dirty_len == len);
    }

    // Testing allocator ranges may share an OS mapping with unrelated memory,
    // so madvise is not safe. Zeroing models the only content guarantee callers
    // have after a successful discard.
    //
    // PORT NOTE (WS7 I2): `cfg!(miri)` takes the same arm. Miri has no FFI, so
    // the real `madvise` is uncallable there — and it would be wrong anyway:
    // `rustty-os`'s page allocator already serves page memory from the std heap
    // under `cfg(miri)`, which is not an OS mapping. This is what lets
    // `.port/phase-2/mirihar-pagelist-compress` interpret the real compression
    // paths.
    if cfg!(test) || cfg!(miri) || cfg!(feature = "test-decommit-sim") {
        // SAFETY: caller contract — `ptr` is live for `len >= dirty_len` bytes.
        unsafe { core::ptr::write_bytes(ptr, 0, dirty_len) };
        return true;
    }

    // DONTNEED immediately reclaims private anonymous pages on Linux and
    // faults them back as zero-filled pages. We deliberately avoid MADV_FREE:
    // it does not reduce RSS until memory pressure and does not guarantee that
    // the next read is zero.
    #[cfg(any(target_os = "linux", target_os = "android"))]
    {
        // SAFETY: caller contract — `ptr` is a live page-aligned mapping.
        let Err(err) = (unsafe { madvise(ptr, len, MADV_DONTNEED) }) else {
            return true;
        };
        // log.warn("madvise(DONTNEED) failed err={}", .{err});
        log::warn!(target: LOG_SCOPE, "madvise(DONTNEED) failed err={err:?}");
        if matches!(mode, DecommitMode::Strict) {
            return false;
        }
        // Zero mode falls through to the memset below.
    }

    // FREE_REUSABLE removes the range from the Darwin process footprint while
    // retaining its mapping. Zero mode clears its dirty prefix first because
    // the kernel may preserve the contents. Strict mode avoids that write
    // because its caller will replace the entire mapping after recommit.
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    {
        if matches!(mode, DecommitMode::Zero) {
            // SAFETY: caller contract — `ptr` is live for `dirty_len` bytes.
            unsafe { core::ptr::write_bytes(ptr, 0, dirty_len) };
        }

        // SAFETY: caller contract — `ptr` is a live page-aligned mapping.
        let Err(err) = (unsafe { madvise(ptr, len, MADV_FREE_REUSABLE) }) else {
            return true;
        };

        match mode {
            DecommitMode::Strict => {
                // log.warn("madvise(FREE_REUSABLE) failed err={}", .{err});
                log::warn!(target: LOG_SCOPE, "madvise(FREE_REUSABLE) failed err={err:?}");
                return false;
            }

            DecommitMode::Zero => {
                // Plain FREE can still reclaim the already-zero mapping
                // under pressure and does not require a reuse pairing.
                // SAFETY: as above.
                let _ = unsafe { madvise(ptr, len, MADV_FREE) };
                return false;
            }
        }
    }

    #[allow(unreachable_code)]
    {
        if matches!(mode, DecommitMode::Zero) {
            // SAFETY: caller contract — `ptr` is live for `dirty_len` bytes.
            unsafe { core::ptr::write_bytes(ptr, 0, dirty_len) };
        }
        false
    }
}

/// Prepare a mapping previously passed to [`decommit`] for reuse.
///
/// Linux and test builds need no explicit operation. Darwin pairs
/// FREE_REUSABLE with FREE_REUSE so pages touched by the caller are accounted
/// to the process again. Failure does not invalidate the retained mapping, so
/// reuse can continue after logging the accounting failure.
///
/// PORTS: oracle@main mem.zig `recommit`.
///
/// # Safety
/// Same contract as [`decommit`].
pub unsafe fn recommit(ptr: *mut u8, len: usize) {
    debug_assert!(len > 0);
    debug_assert!((ptr as usize).is_multiple_of(PAGE_SIZE_MIN));
    debug_assert!(len.is_multiple_of(PAGE_SIZE_MIN));

    if cfg!(test) || cfg!(miri) || cfg!(feature = "test-decommit-sim") {
        return;
    }

    #[cfg(any(target_os = "macos", target_os = "ios"))]
    {
        // SAFETY: caller contract — `ptr` is a live page-aligned mapping.
        if let Err(err) = unsafe { madvise(ptr, len, MADV_FREE_REUSE) } {
            // log.warn("madvise(FREE_REUSE) failed err={}", .{err});
            log::warn!(target: LOG_SCOPE, "madvise(FREE_REUSE) failed err={err:?}");
        }
    }

    #[cfg(not(any(target_os = "macos", target_os = "ios")))]
    {
        let _ = (ptr, len);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Parity S156-F3 (no reference test): the logged `err` is the
    // `std.posix.MadviseError` the call failed with. An unknown advice value
    // is EINVAL, which std maps to `error.InvalidSyscall`.
    #[test]
    #[cfg(any(
        target_os = "macos",
        target_os = "ios",
        target_os = "linux",
        target_os = "android"
    ))]
    #[cfg_attr(miri, ignore)] // FFI
    fn madvise_reports_the_std_posix_error() {
        let (ptr, layout) = aligned_alloc(crate::page::PAGE_SIZE_MIN);
        // SAFETY: the kernel rejects the advice without touching the range.
        let result = unsafe { madvise(ptr, crate::page::PAGE_SIZE_MIN, 0x7fff) };
        assert_eq!(Err(MadviseError::InvalidSyscall), result);
        assert_eq!("InvalidSyscall", format!("{:?}", result.unwrap_err()));
        // SAFETY: allocated above with `layout`.
        unsafe { std::alloc::dealloc(ptr, layout) };
    }

    fn aligned_alloc(len: usize) -> (*mut u8, std::alloc::Layout) {
        let layout = std::alloc::Layout::from_size_align(len, crate::page::PAGE_SIZE_MIN).unwrap();
        // SAFETY: len > 0 and the layout is valid.
        let ptr = unsafe { std::alloc::alloc_zeroed(layout) };
        assert!(!ptr.is_null());
        (ptr, layout)
    }

    /// PORTS: oracle@main mem.zig
    /// `test "decommit with zero fallback clears the dirty prefix"`
    #[test]
    fn mem_decommit_with_zero_fallback_clears_the_dirty_prefix() {
        let memory_len = 2 * crate::page::PAGE_SIZE_MIN;
        let (ptr, layout) = aligned_alloc(memory_len);
        unsafe {
            core::ptr::write_bytes(ptr, 0xAA, memory_len);
            let _ = decommit(DecommitMode::Zero, ptr, memory_len, memory_len);
            assert!(core::slice::from_raw_parts(ptr, memory_len)
                .iter()
                .all(|&b| b == 0));

            // The tail is already zero by contract, so a partially dirty
            // mapping only needs its dirty prefix cleared.
            core::ptr::write_bytes(ptr, 0xAA, 1024);
            let _ = decommit(DecommitMode::Zero, ptr, memory_len, 1024);
            assert!(core::slice::from_raw_parts(ptr, memory_len)
                .iter()
                .all(|&b| b == 0));

            std::alloc::dealloc(ptr, layout);
        }
    }

    /// PORTS: oracle@main mem.zig
    /// `test "strict decommit retains the mapping for recommit"`
    #[test]
    fn mem_strict_decommit_retains_the_mapping_for_recommit() {
        let memory_len = 2 * crate::page::PAGE_SIZE_MIN;
        let (ptr, layout) = aligned_alloc(memory_len);
        unsafe {
            core::ptr::write_bytes(ptr, 0xAA, memory_len);
            assert!(decommit(DecommitMode::Strict, ptr, memory_len, memory_len));
            assert!(core::slice::from_raw_parts(ptr, memory_len)
                .iter()
                .all(|&b| b == 0));

            recommit(ptr, memory_len);
            core::ptr::write_bytes(ptr, 0xBB, memory_len);
            assert!(core::slice::from_raw_parts(ptr, memory_len)
                .iter()
                .all(|&b| b == 0xBB));

            std::alloc::dealloc(ptr, layout);
        }
    }

    /// PORTS: oracle@main mem.zig `test "test builds can reclaim retained mappings"`
    #[test]
    fn mem_test_builds_can_reclaim_retained_mappings() {
        assert!(can_reclaim(DecommitMode::Zero));
        assert!(can_reclaim(DecommitMode::Strict));
    }
}
