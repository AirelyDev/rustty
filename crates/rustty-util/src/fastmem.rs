#![allow(non_snake_case)]

/// Same as @memmove but prefers libc memmove if it is
/// available because it is generally much faster?.
//
// In the Zig reference this preferred libc `memmove` when `link_libc` was set,
// falling back to `@memmove` otherwise. Both branches are the move-semantics
// copy of `source` into `dest`; the slices may overlap when they share backing
// storage (the rotate helpers below rely on this), which `ptr::copy` (memmove
// semantics) handles with no UB for Miri.
#[inline]
pub fn r#move<T: Copy>(dest: &mut [T], source: &[T]) {
    let n = source.len();
    debug_assert!(dest.len() >= n);
    // SAFETY: `dest` is valid for `n` writes (asserted) and `source` is valid
    // for `n` reads; `T: Copy` so no drops are skipped. `ptr::copy` is memmove,
    // so overlapping `dest`/`source` regions are well-defined.
    unsafe {
        core::ptr::copy(source.as_ptr(), dest.as_mut_ptr(), n);
    }
}

/// Same as @memcpy but prefers libc memcpy if it is available
/// because it is generally much faster.
#[inline]
pub fn copy<T: Copy>(dest: &mut [T], source: &[T]) {
    // Equivalent to `@memcpy(dest[0..source.len], source)`: a non-overlapping
    // copy of `source` into the front of `dest`.
    let n = source.len();
    dest[0..n].copy_from_slice(source);
}

/// Moves the first item to the end.
/// For the reverse of this, use `fastmem.rotateOnceR`.
///
/// Same as std.mem.rotate(T, items, 1) but more efficient by using memmove
/// and a tmp var for the single rotated item instead of 3 calls to reverse.
///
/// e.g. `0 1 2 3` -> `1 2 3 0`.
#[inline]
pub fn rotateOnce<T: Copy>(items: &mut [T]) {
    let tmp = items[0];
    let len = items.len();
    items.copy_within(1..len, 0); // move(items[0..len-1], items[1..len])
    items[len - 1] = tmp;
}

/// Moves the last item to the start.
/// Reverse operation of `fastmem.rotateOnce`.
///
/// Same as std.mem.rotate(T, items, items.len - 1) but more efficient by
/// using memmove and a tmp var for the single rotated item instead of 3
/// calls to reverse.
///
/// e.g. `0 1 2 3` -> `3 0 1 2`.
#[inline]
pub fn rotateOnceR<T: Copy>(items: &mut [T]) {
    let len = items.len();
    let tmp = items[len - 1];
    items.copy_within(0..len - 1, 1); // move(items[1..len], items[0..len-1])
    items[0] = tmp;
}

/// Rotates a new item in to the end of a slice.
/// The first item from the slice is removed and returned.
///
/// e.g. rotating `4` in to `0 1 2 3` makes it `1 2 3 4` and returns `0`.
///
/// For the reverse of this, use `fastmem.rotateInR`.
#[inline]
pub fn rotateIn<T: Copy>(items: &mut [T], item: T) -> T {
    let len = items.len();
    let removed = items[0];
    items.copy_within(1..len, 0); // move(items[0..len-1], items[1..len])
    items[len - 1] = item;
    removed
}

/// Rotates a new item in to the start of a slice.
/// The last item from the slice is removed and returned.
///
/// e.g. rotating `4` in to `0 1 2 3` makes it `4 0 1 2` and returns `3`.
///
/// Reverse operation of `fastmem.rotateIn`.
#[inline]
pub fn rotateInR<T: Copy>(items: &mut [T], item: T) -> T {
    let len = items.len();
    let removed = items[len - 1];
    items.copy_within(0..len - 1, 1); // move(items[1..len], items[0..len-1])
    items[0] = item;
    removed
}
