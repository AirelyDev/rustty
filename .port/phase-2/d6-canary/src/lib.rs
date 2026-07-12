//! rustty-d6-canary — sanitizer-visibility canaries (detector D6, kills Bun
//! class 5 / oven-sh/bun#30875).
//!
//! These tests are compiled **only in sanitizer builds** (`cfg(sanitize = …)`,
//! which rustc stamps when `-Zsanitizer=…` is active). In a normal `cargo test`
//! they evaluate to nothing — the crate is empty — so they can never appear in,
//! nor perturb, the ordinary cargo lanes.
//!
//! Each canary commits a real, textbook memory sin against a **heap** allocation
//! obtained through the port's ordinary global allocator:
//!   * `canary_heap_overflow_must_be_caught_by_asan` reads past a heap `Vec`;
//!   * `canary_memory_leak_must_be_caught_by_lsan` leaks a `Box`.
//!
//! If the sanitizer runtime can SEE the allocator, ASan reports
//! `heap-buffer-overflow` and LSan reports `detected memory leaks`, turning the
//! lane RED. That RED is the *healthy* outcome — the gate
//! (`check_sanitizer_canary.sh`) INVERTS it: canary fired ⇒ the sanitizer can
//! see the port's heap ⇒ PASS; canary silent ⇒ the lane is BLIND (the exact
//! #30875 failure: a constant-folded custom global allocator bypassing the
//! interceptors) ⇒ FAIL. The gate then filters the canary out of the real
//! roll-up so a deliberately-red proof never counts against a subsystem.

// `cfg(sanitize = …)` is unstable; the gate always builds us with +nightly.
#![feature(cfg_sanitize)]

/// Heap-buffer-overflow canary — fires under AddressSanitizer (macOS + Linux).
///
/// Allocates a 16-byte heap `Vec` through the port's global allocator, then
/// READS 8 bytes past its end — squarely inside ASan's redzone. If ASan's
/// allocator interceptor observed the `Vec`'s allocation it aborts with
/// `ERROR: AddressSanitizer: heap-buffer-overflow`. A custom global allocator
/// that bypassed the interceptor (the #30875 mechanism) would let this READ
/// succeed silently — which the gate reports as a BLIND lane.
#[cfg(sanitize = "address")]
#[test]
fn canary_heap_overflow_must_be_caught_by_asan() {
    let v: Vec<u8> = (0u8..16).collect();
    let p = v.as_ptr();
    // 8 bytes past the allocation: inside the ASan redzone, never a valid read.
    let oob = unsafe { *p.add(v.len() + 8) };
    std::hint::black_box(oob);
    std::hint::black_box(&v);
}

/// Memory-leak canary — fires under LeakSanitizer (Linux `leak`/`address`
/// lanes; ASan on Linux has LSan built in). On macOS ASan runs with
/// `detect_leaks=0` (no LSan on Darwin), so this is inert there and fires in
/// the Linux-CI leak lane — documented, never skipped (R5/R6). Leaks a 64-byte
/// `Box` obtained via the port's global allocator: if LSan can see the heap it
/// aborts with `ERROR: LeakSanitizer: detected memory leaks`.
#[cfg(any(sanitize = "address", sanitize = "leak"))]
#[test]
fn canary_memory_leak_must_be_caught_by_lsan() {
    let leaked: Box<[u8; 64]> = Box::new([0xABu8; 64]);
    // Intentionally never freed. `Box::leak` makes the intent explicit and
    // prevents the optimizer from proving the allocation dead and eliding it.
    std::hint::black_box(Box::leak(leaked));
}
