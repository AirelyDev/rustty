//! The `os/wasm.zig` host-owned allocation tracker.
//!
//! PORTS: ghostty `src/os/wasm.zig` lines 27-115 **as of `9dc2e5978^`** — the
//! `allocs` map, `malloc`, `free`, `toHostOwned`, `isHostOwned`, `toModuleOwned`,
//! and both `test {}` blocks (`"basics"`, `"toHostOwned"`).
//!
//! PORT STATUS: ported — COMPLETE, RESURRECTED. See the PORT NOTE in the parent
//! module (`os/wasm.rs`): commit `9dc2e5978` ("lib-vt: enable freestanding wasm
//! builds (#9301)", 2025-10-21) deleted these 90 lines upstream, which left
//! `font/Atlas.zig`'s `Wasm` block and `font/face/web_canvas.zig` calling symbols
//! that no longer exist (upstream does not notice because `main_wasm.zig` is
//! referenced nowhere in `build.zig`, so neither file is analyzed). rustty
//! resurrects them so that `Atlas.Wasm` and its 4-assert reference test can be
//! ported LIVE (WS14 T3), on the same precedent that resurrected
//! `font/face/web_canvas.zig`.
//!
//! Read the reference text with:
//! `git -C ../ghostty show 9dc2e5978^:src/os/wasm.zig`.

use std::alloc::Layout;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

/// For host-owned allocations:
/// We need to keep track of our own pointer lengths because Zig
/// allocators usually don't do this and we need to be able to send
/// a direct pointer back to the host system. A more appropriate thing
/// to do would be to probably make a custom allocator that keeps track
/// of size.
//
// PORT: `var allocs: std.AutoHashMapUnmanaged([*]u8, usize) = .{};`
//
// PORT NOTE — three deliberate representation divergences, none observable:
//
//  1. Zig's file-scope `var` with an explicitly-threaded allocator has no direct
//     Rust analog. A `OnceLock<Mutex<..>>` is used so the map is `Sync` (the
//     reference relies on wasm being single-threaded; a `Mutex` costs nothing
//     there and keeps the item usable from host tests). The observable contract is
//     preserved 1:1 — `putNoClobber` semantics (PANIC on a duplicate key),
//     `remove`, `get`, `contains`, and `allocs.size`.
//  2. The key is `usize` (the pointer's address), not `*mut u8`: raw pointers are
//     not `Send`/`Sync`, and the map only ever compares addresses — exactly what
//     Zig's `AutoHashMapUnmanaged([*]u8, ...)` does.
//  3. The value is a `Layout`, not a bare `usize` length. Zig's `alloc.free(slice)`
//     recovers the alignment from the slice's element type (`u8`, i.e. 1); Rust's
//     `dealloc` requires the SAME `Layout` the allocation was made with, so
//     `to_host_owned::<T>` must record `align_of::<T>()` alongside
//     `size_of::<T>()` or freeing a `T` allocation would be UB. `allocs_len()`
//     still counts one entry per tracked pointer, so the reference's
//     `allocs.size` assertions port unchanged.
#[allow(clippy::type_complexity)]
fn allocs() -> &'static Mutex<HashMap<usize, Layout>> {
    static ALLOCS: OnceLock<Mutex<HashMap<usize, Layout>>> = OnceLock::new();
    ALLOCS.get_or_init(|| Mutex::new(HashMap::new()))
}

// PORT NOTE: `allocs.putNoClobber(alloc, ptr, len)` — Zig's `putNoClobber`
// asserts the key is absent. The faithful Rust map of "assert" is a panic; under
// the C-ABI panic policy (DG5) that aborts, which is the same program-terminating
// outcome as Zig's `unreachable` in a safe build.
fn put_no_clobber(addr: usize, layout: Layout) {
    let mut map = allocs().lock().unwrap();
    assert!(
        map.insert(addr, layout).is_none(),
        "putNoClobber: pointer {addr:#x} is already tracked as host-owned"
    );
}

/// Test-only view of `allocs.size`.
//
// PORT NOTE: the reference's two tests read `allocs.size` directly (same-file
// access to the private `var`). This helper is that same access, expressed as a
// function because Rust has no way to name a `static` behind a `OnceLock` from a
// child `mod tests` without one. It is NOT new API: `pub(crate)`, test-only.
#[cfg(test)]
pub(crate) fn allocs_len() -> usize {
    allocs().lock().unwrap().len()
}

/// The error returned when a pointer cannot be made host-owned.
//
// PORT NOTE: `toHostOwned(ptr: anytype) ![*]u8` — the Zig error set is the
// allocator's (`error{OutOfMemory}`, from `putNoClobber`). Rust's `HashMap::insert`
// aborts on allocation failure rather than returning, so the only reachable
// failure is a null/unaligned input, which the reference cannot express. The
// `Result` shape is preserved so call sites port verbatim (`catch return null`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostOwnedError {
    /// The pointer was null, so there is nothing to hand to the host.
    NullPointer,
}

impl std::fmt::Display for HostOwnedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HostOwnedError::NullPointer => f.write_str("null pointer cannot be host-owned"),
        }
    }
}

impl std::error::Error for HostOwnedError {}

/// Allocate len bytes and return a pointer to the memory in the host.
/// The data is not zeroed.
//
// PORT: `pub export fn malloc(len: usize) ?[*]u8 { return alloc_(len) catch return null; }`
//
// PORT NOTE (symbol name): the reference exports the BARE name `malloc`, which
// collides with libc on a native link. `#[no_mangle]` is therefore applied only
// for wasm targets via `cfg_attr`; the Rust-visible path `wasm::malloc` exists on
// every target (that is what the reference tests, and ours, call).
//
// PORT NOTE (`?[*]u8` -> `*mut u8`): `null` is the `catch return null` result.
// Plain `extern "C"` — DG5 abort policy, no per-function catch-unwind wrapper.
#[cfg_attr(target_arch = "wasm32", no_mangle)]
pub extern "C" fn malloc(len: usize) -> *mut u8 {
    match alloc_(len) {
        Ok(ptr) => ptr,
        // return null
        Err(_) => std::ptr::null_mut(),
    }
}

// PORT: `fn alloc_(len: usize) ![*]u8 { ... }`
fn alloc_(len: usize) -> Result<*mut u8, HostOwnedError> {
    // PORT NOTE: Zig's `alloc.alloc(u8, 0)` yields a valid zero-length slice;
    // Rust's `std::alloc::alloc` with a zero-size layout is UB. A 0-length request
    // therefore allocates one byte, which is indistinguishable to the caller (the
    // pointer is valid for the 0 bytes they asked for) and keeps `free` sound.
    let layout = Layout::from_size_align(len.max(1), 1).map_err(|_| HostOwnedError::NullPointer)?;

    // Create the allocation
    // const slice = try alloc.alloc(u8, len);
    let ptr = unsafe { std::alloc::alloc(layout) };
    if ptr.is_null() {
        // errdefer alloc.free(slice) — nothing to undo: the allocation failed.
        return Err(HostOwnedError::NullPointer);
    }

    // Store the size so we can deallocate later
    // try allocs.putNoClobber(alloc, slice.ptr, slice.len);
    // errdefer _ = allocs.remove(slice.ptr);
    put_no_clobber(ptr as usize, layout);

    // return slice.ptr;
    Ok(ptr)
}

/// Free an allocation from malloc.
//
// PORT: `pub export fn free(ptr: ?[*]u8) void { ... }`. Null-tolerant, and a
// pointer that is NOT tracked is a no-op (faithful to `if (allocs.get(v)) |len|`).
//
/// # Safety
/// `ptr` must be null, or a pointer previously returned by [`malloc`] or
/// [`to_host_owned`] that has not already been freed.
#[cfg_attr(target_arch = "wasm32", no_mangle)]
pub unsafe extern "C" fn free(ptr: *mut u8) {
    // if (ptr) |v| {
    if ptr.is_null() {
        return;
    }
    // if (allocs.get(v)) |len| {
    let layout = allocs().lock().unwrap().get(&(ptr as usize)).copied();
    if let Some(layout) = layout {
        // const slice = v[0..len]; alloc.free(slice);
        unsafe { std::alloc::dealloc(ptr, layout) };
        // _ = allocs.remove(v);
        allocs().lock().unwrap().remove(&(ptr as usize));
    }
}

/// Convert an allocated pointer of any type to a host-owned pointer.
/// This pushes the responsibility to free it to the host. The returned
/// pointer will match the pointer but is typed correctly for returning
/// to the host.
//
// PORT: `pub fn toHostOwned(ptr: anytype) ![*]u8 { ... }`. Zig's `anytype` +
// `@typeInfo(...).pointer.child` is a generic `<T>` here; `@sizeOf(T)` is
// `size_of::<T>()` (plus `align_of::<T>()`, see the `allocs()` PORT NOTE).
//
/// # Safety
/// `ptr` must be a live, uniquely-owned pointer to a `T` that was allocated with
/// the global allocator (e.g. `Box::into_raw`), and must not already be tracked.
pub unsafe fn to_host_owned<T>(ptr: *mut T) -> Result<*mut u8, HostOwnedError> {
    // PORT NOTE: a zero-sized `T` has no allocation for `free` to release
    // (`dealloc` with a zero-size layout is UB in Rust), so it is rejected. The
    // reference cannot express this because Zig's `alloc.create` of a 0-byte type
    // yields a dangling-but-valid pointer that its `free` silently ignores; the
    // rejection is strictly safer and unreachable from every real call site.
    if ptr.is_null() || std::mem::size_of::<T>() == 0 {
        return Err(HostOwnedError::NullPointer);
    }

    // Convert our pointer to a byte array
    // const info = @typeInfo(@TypeOf(ptr)).pointer; const T = info.child;
    // const size = @sizeOf(T);
    // const casted = @as([*]u8, @ptrFromInt(@intFromPtr(ptr)));
    let casted = ptr as *mut u8;
    let layout = Layout::new::<T>();

    // Store the information about it
    // try allocs.putNoClobber(alloc, casted, size);
    // errdefer _ = allocs.remove(casted);
    put_no_clobber(casted as usize, layout);

    // return casted;
    Ok(casted)
}

/// Returns true if the value is host owned.
//
// PORT: `pub fn isHostOwned(ptr: anytype) bool { ... return allocs.contains(casted); }`
pub fn is_host_owned<T>(ptr: *const T) -> bool {
    // const casted = @as([*]u8, @ptrFromInt(@intFromPtr(ptr)));
    let casted = ptr as usize;
    // return allocs.contains(casted);
    allocs().lock().unwrap().contains_key(&casted)
}

/// Convert a pointer back to a module-owned value. The caller is expected
/// to cast or have the valid pointer for alloc calls.
//
// PORT: `pub fn toModuleOwned(ptr: anytype) void { _ = allocs.remove(casted); }`
//
/// # Safety
/// `ptr` must be a pointer whose ownership the module is taking back; after this
/// call [`free`] will no longer release it, so the module must free it itself.
pub unsafe fn to_module_owned<T>(ptr: *const T) {
    // const casted = @as([*]u8, @ptrFromInt(@intFromPtr(ptr)));
    // _ = allocs.remove(casted);
    allocs().lock().unwrap().remove(&(ptr as usize));
}

#[cfg(test)]
mod host_owned_tests {
    use super::*;

    // PORT NOTE: the reference's two tests read the SAME process-global `allocs`
    // map, which under `cargo test`'s thread-parallel runner would make their
    // `allocs.size == 1` assertions race each other. A module-private mutex
    // serializes them, restoring the reference's single-threaded semantics without
    // weakening either assertion (Zig runs tests serially).
    static TEST_LOCK: Mutex<()> = Mutex::new(());

    // test "basics" {
    #[test]
    fn basics() {
        let _guard = TEST_LOCK.lock().unwrap();
        // const buf = malloc(32).?;
        let buf = malloc(32);
        assert!(!buf.is_null());
        // try testing.expect(allocs.size == 1);
        assert_eq!(allocs_len(), 1);
        // free(buf);
        unsafe { free(buf) };
        // try testing.expect(allocs.size == 0);
        assert_eq!(allocs_len(), 0);
    }

    // test "toHostOwned" {
    #[test]
    fn to_host_owned_roundtrip() {
        let _guard = TEST_LOCK.lock().unwrap();
        // const Point = struct { x: u32 = 0, y: u32 = 0 };
        #[derive(Default)]
        #[allow(dead_code)]
        struct Point {
            x: u32,
            y: u32,
        }
        // const p = try alloc.create(Point);
        // errdefer alloc.destroy(p);
        let p = Box::into_raw(Box::new(Point::default()));
        // const ptr = try toHostOwned(p);
        let ptr = unsafe { to_host_owned(p) }.expect("to_host_owned");
        // try testing.expect(allocs.size == 1);
        assert_eq!(allocs_len(), 1);
        // try testing.expect(isHostOwned(p));
        assert!(is_host_owned(p));
        // try testing.expect(isHostOwned(ptr));
        assert!(is_host_owned(ptr));
        // free(ptr);
        unsafe { free(ptr) };
        // try testing.expect(allocs.size == 0);
        assert_eq!(allocs_len(), 0);
    }
}
