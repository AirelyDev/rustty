//! wuffs-sys — FFI bindings to the vendored Wuffs image codecs (`links = "wuffs"`).
//!
//! PORT STATUS: vendored (full single-file library, ghostty's module subset)
//!
//! R3: vendors the SAME C library the reference uses — Google's Wuffs
//! 0.4.0-alpha.9, a single-file C image-codec library — and builds it
//! standalone (`cc` + `bindgen`). No pure-Rust substitution during parity
//! phases. The build mirrors the reference's pkg/wuffs/build.zig: the single
//! `release/c/wuffs-v0.4.c` translation unit compiled with
//! `-DWUFFS_IMPLEMENTATION` and the exact `WUFFS_CONFIG__MODULE__*` gate list
//! from pkg/wuffs/src/c.zig (BASE + the ADLER32/CRC32/DEFLATE/JPEG/PNG/ZLIB
//! codecs ghostty decodes). See build.rs.
//!
//! The generated bindings expose the low-level Wuffs C API the reference's
//! png.zig / jpeg.zig / swizzle.zig drive (decoder structs, `sizeof__wuffs_*`
//! size helpers, the `wuffs_base__*` runtime types). Downstream safe wrappers
//! live in the parity crates; this crate is the raw FFI leaf.

// bindgen output over the enormous single-file Wuffs header: silence the lints
// that fire on machine-generated C-shaped Rust.
#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(dead_code)]
#![allow(clippy::all)]

include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

#[cfg(test)]
mod kat {
    use super::*;

    /// A Wuffs `wuffs_base__status` is OK iff its `repr` string pointer is NULL
    /// (see `wuffs_base__status__is_ok` in the vendored header). We read the
    /// field directly rather than calling that `static inline` helper (bindgen
    /// does not emit a linkable symbol for it).
    fn status_is_ok(status: &wuffs_base__status) -> bool {
        status.repr.is_null()
    }

    /// Build a `wuffs_base__slice_u8` over the given bytes. Mirrors the C
    /// `wuffs_base__make_slice_u8(ptr, len)` (a `static inline`, so constructed
    /// here directly as the two-field struct `{ ptr, len }`).
    fn slice_u8(data: &[u8]) -> wuffs_base__slice_u8 {
        wuffs_base__slice_u8 {
            ptr: data.as_ptr() as *mut u8,
            len: data.len(),
        }
    }

    /// A zeroed, 8-byte-aligned backing store for an opaque Wuffs hasher struct.
    ///
    /// Without `WUFFS_IMPLEMENTATION` the hasher/decoder structs are opaque
    /// (private_impl hidden), so — exactly like the reference's png.zig
    /// (`alloc.alloc(u8, c.sizeof__wuffs_png__decoder())` + `@ptrCast`) — we
    /// allocate the C-reported size and hand `initialize` a raw pointer. A
    /// `Vec<u64>` guarantees 8-byte alignment (sufficient: the hasher
    /// private_impl holds only u32 / pointer / bool fields, ≤ 8-aligned) and
    /// stays alive for the whole test. `initialize` verifies the size against
    /// its internal `sizeof(*self)`, so a wrong size fails loudly, not silently.
    fn zeroed_hasher(c_size: usize) -> Vec<u64> {
        assert!(c_size > 0, "sizeof__wuffs_*__hasher() returned 0");
        vec![0u64; c_size.div_ceil(8)]
    }

    /// KAT — Wuffs ADLER32 over a known input yields the known checksum.
    ///
    /// Adler-32("abc") == 0x024D0127 (s1=295=0x127, s2=589=0x24D). This drives
    /// the real vendored C: allocate + initialize the hasher struct, feed the
    /// bytes, read the u32 — proving the ADLER32 module compiled and links.
    #[test]
    fn adler32_abc_known_answer() {
        // SAFETY: FFI call to a pure size accessor (no receiver).
        let c_size = unsafe { sizeof__wuffs_adler32__hasher() };
        let mut backing = zeroed_hasher(c_size);
        let hasher_ptr = backing.as_mut_ptr() as *mut wuffs_adler32__hasher;

        // SAFETY: `hasher_ptr` points at `c_size` zeroed, 8-aligned bytes; the
        // version/options args mirror the reference (png.zig) exactly.
        let status = unsafe {
            wuffs_adler32__hasher__initialize(hasher_ptr, c_size, WUFFS_VERSION as u64, 0)
        };
        assert!(status_is_ok(&status), "adler32 initialize failed");

        // SAFETY: the hasher is initialized; the slice borrows live bytes for
        // the duration of the call.
        let checksum = unsafe { wuffs_adler32__hasher__update_u32(hasher_ptr, slice_u8(b"abc")) };
        assert_eq!(checksum, 0x024D_0127, "Adler-32(\"abc\") mismatch");
    }

    /// KAT — Wuffs CRC-32/IEEE over a known input yields the known checksum.
    ///
    /// CRC-32("abc") == 0x352441C2 (the standard zlib/IEEE CRC-32). A second,
    /// independent codec module (proves the CRC32 gate compiled + links too).
    #[test]
    fn crc32_abc_known_answer() {
        // SAFETY: FFI call to a pure size accessor (no receiver).
        let c_size = unsafe { sizeof__wuffs_crc32__ieee_hasher() };
        let mut backing = zeroed_hasher(c_size);
        let hasher_ptr = backing.as_mut_ptr() as *mut wuffs_crc32__ieee_hasher;

        // SAFETY: see adler32_abc_known_answer.
        let status = unsafe {
            wuffs_crc32__ieee_hasher__initialize(hasher_ptr, c_size, WUFFS_VERSION as u64, 0)
        };
        assert!(status_is_ok(&status), "crc32 initialize failed");

        // SAFETY: see adler32_abc_known_answer.
        let checksum =
            unsafe { wuffs_crc32__ieee_hasher__update_u32(hasher_ptr, slice_u8(b"abc")) };
        assert_eq!(checksum, 0x3524_41C2, "CRC-32(\"abc\") mismatch");
    }

    /// KAT — the 3 new `rustty_wuffs_pixel_format__*` shims (static-inline
    /// forwards over `bits_per_pixel`/`is_direct`/`is_interleaved`) link and
    /// return the correct values for `WUFFS_BASE__PIXEL_FORMAT__RGBA_PREMUL`: a
    /// direct, interleaved, 32-bit-per-pixel format (4 8-bit channels).
    #[test]
    fn pixel_format_rgba_premul_shims() {
        let fmt = wuffs_base__pixel_format {
            repr: WUFFS_BASE__PIXEL_FORMAT__RGBA_PREMUL,
        };

        // SAFETY: `fmt` is a valid, initialized `wuffs_base__pixel_format`; the
        // shims are pure accessors over `repr` (no allocation, no receiver
        // beyond the borrowed pointer).
        let bpp = unsafe { rustty_wuffs_pixel_format__bits_per_pixel(&fmt) };
        assert_eq!(bpp, 32, "RGBA_PREMUL bits_per_pixel mismatch");

        // SAFETY: see above.
        let is_direct = unsafe { rustty_wuffs_pixel_format__is_direct(&fmt) };
        assert!(is_direct, "RGBA_PREMUL should be direct (non-indexed)");

        // SAFETY: see above.
        let is_interleaved = unsafe { rustty_wuffs_pixel_format__is_interleaved(&fmt) };
        assert!(
            is_interleaved,
            "RGBA_PREMUL should be interleaved (non-planar)"
        );
    }
}
