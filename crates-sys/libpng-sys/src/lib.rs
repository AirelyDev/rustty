//! libpng-sys — FFI bindings to the vendored libpng 1.6.43 PNG codec
//! (`links = "png"`).
//!
//! PORT STATUS: vendored (full library)
//!
//! R3: copies the SAME C library the reference links (libpng 1.6.43, the exact
//! `glennrp/libpng` release pinned in pkg/libpng/build.zig.zon) and builds it
//! standalone (`cc` + `bindgen`; see `build.rs`). No pure-Rust substitution
//! during parity phases. The generated bindings expose libpng's public `png_*`
//! surface; downstream safe wrappers live in the parity crates, and this crate
//! is the raw FFI leaf that will back dcimgui-sys.

// bindgen output over libpng's public headers: silence the lints that fire on
// machine-generated C-shaped Rust.
#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(dead_code)]
#![allow(clippy::all)]

// Force-link the vendored zlib. libpng's `pngstruct.h` embeds a `z_stream` and
// the codec references the zlib API (e.g. `png.c`'s `inflateReset`,
// `pngread.c`'s `inflateEnd`). Without an explicit reference to the `zlib-sys`
// rlib, rustc prunes it as unused — dropping its `cargo:rustc-link-lib=static=z`
// so those symbols go undefined at link time. `extern crate` keeps the archive
// on the link line (mirrors the reference's `lib.linkLibrary(zlib_dep)`).
extern crate zlib_sys;

include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

#[cfg(test)]
mod kat {
    use super::*;

    /// KAT — the vendored libpng reports its exact pinned version.
    ///
    /// `png_access_version_number()` returns `PNG_LIBPNG_VER` (10643 == 1.6.43).
    /// Linking and matching this proves the vendored C compiled and links: the
    /// value is baked into `png.c` from the on-disk `png.h`, so a mismatched or
    /// stale vendor tree fails loudly.
    #[test]
    fn access_version_number_is_1_6_43() {
        // SAFETY: FFI call to a pure accessor (no arguments, no receiver).
        let ver = unsafe { png_access_version_number() };
        assert_eq!(ver, 10643, "libpng version mismatch (expected 1.6.43)");
    }

    /// KAT — create then destroy a libpng read struct.
    ///
    /// `png_create_read_struct` allocates and version-checks a `png_struct`
    /// against the compiled-in version string; passing the matching
    /// `PNG_LIBPNG_VER_STRING` yields a non-null pointer. This exercises the
    /// real allocator/setup path (png.c + pngmem.c + pngerror.c) end-to-end and
    /// tears it back down with `png_destroy_read_struct`.
    #[test]
    fn create_and_destroy_read_struct() {
        let ver = PNG_LIBPNG_VER_STRING.as_ptr() as *const std::os::raw::c_char;

        // SAFETY: `ver` points at the NUL-terminated compiled-in version
        // string; NULL error_ptr with default (None) error/warn handlers is the
        // documented "use libpng's built-in setjmp handlers" configuration.
        let png = unsafe { png_create_read_struct(ver, std::ptr::null_mut(), None, None) };
        assert!(!png.is_null(), "png_create_read_struct returned NULL");

        // SAFETY: `png` was just returned non-null by png_create_read_struct;
        // the info/end-info out-params are NULL (none were allocated).
        let mut png_ptr = png;
        unsafe {
            png_destroy_read_struct(&mut png_ptr, std::ptr::null_mut(), std::ptr::null_mut());
        }
    }
}
