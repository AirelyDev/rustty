//! fontconfig-sys — FFI bindings to the vendored Fontconfig 2.14.2 font
//! discovery/matching engine (`links = "fontconfig"`).
//!
//! PORT STATUS: vendored (full library)
//!
//! R3: copies the SAME C library the reference uses (fontconfig 2.14.2, per
//! pkg/fontconfig/build.zig.zon:8) and builds it standalone (`cc` + `bindgen`;
//! see `build.rs`). The host's system fontconfig (2.17) is deliberately NOT
//! linked — parity is pinned to the version the reference itself builds. The
//! FFI is bindgen-generated from `wrapper.h` — the same single public umbrella
//! header the reference's `pkg/fontconfig/c.zig` @cImports
//! (`<fontconfig/fontconfig.h>`).

// Force-link the vendored FreeType and libxml2. fontconfig is built with the
// FreeType backend (`src/fcfreetype.c` references the FreeType API) and
// `-DENABLE_LIBXML2` (`src/fcxml.c` references libxml2), so the archive has
// undefined references into both. Without an explicit reference to the
// `freetype-sys` / `libxml2-sys` rlibs, rustc prunes them — and with them the
// `cargo:rustc-link-lib=static={freetype,xml2}` directives — leaving those
// symbols undefined at link time. `extern crate` keeps the archives on the link
// line (mirrors the reference's `lib.linkLibrary(freetype)` /
// `lib.linkLibrary(libxml2)`).
extern crate freetype_sys;
extern crate libxml2_sys;

// The bindgen-generated module carries C naming conventions and machine-shaped
// code; suppress the whole surface (this is generated, not hand-authored — the
// clippy/rustc-lint gate applies to our code, not the generated bindings).
#[allow(
    non_camel_case_types,
    non_upper_case_globals,
    non_snake_case,
    unnecessary_transmutes,
    clippy::all,
    clippy::pedantic,
    clippy::nursery,
    dead_code,
    rustdoc::all
)]
mod bindings {
    include!(concat!(env!("OUT_DIR"), "/bindings.rs"));
}

pub use bindings::*;

// The configuration-directory defines `build.rs` compiles in, tested here
// (S31-F1); `build.rs` includes the same file through `#[path]`.
#[cfg(test)]
mod dir_defines;

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CStr;
    use std::os::raw::{c_char, c_int};
    use std::ptr;

    /// R3 known-answer test: prove the vendored fontconfig 2.14.2 links and runs.
    ///
    /// Mirrors the reference's smoke paths (pkg/fontconfig `init.zig` version /
    /// initLoadConfig tests + `pattern.zig`'s "create" test):
    ///
    ///   * `FcGetVersion()` returns exactly `21402` — the fixed known answer for
    ///     the pinned 2.14.2, and proof we linked the vendored archive rather
    ///     than the host's system fontconfig (2.17, which reports `21700`).
    ///   * `FcInitLoadConfig()` returns a non-null `*FcConfig` — a LIVE fontconfig
    ///     call that parses the XML config (exercising the `fcxml.c` → libxml2
    ///     wiring end to end) but, unlike `FcInit()`/`FcInitLoadConfigAndFonts()`,
    ///     does NOT scan system fonts and does NOT write the on-disk cache. So
    ///     the KAT is hermetic w.r.t. the compiled-in `/var/cache/fontconfig`
    ///     (which may be unwritable in the sandbox) and needs no installed fonts.
    ///   * `FcPatternCreate()` → non-null; `FcPatternAddString(FC_FAMILY,
    ///     "monospace")` → `FcTrue`; `FcPatternGetString(FC_FAMILY, 0)` →
    ///     `FcResultMatch` with the value read back as `"monospace"`;
    ///     `FcPatternDestroy` / `FcConfigDestroy` tear down cleanly.
    #[test]
    fn kat_fontconfig_2_14_2_init_and_pattern() {
        // Known-answer: the vendored release is exactly fontconfig 2.14.2, whose
        // encoded version is major*10000 + minor*100 + micro = 2*10000 + 14*100
        // + 2 = 21402. If this ever reads 21700 (or anything else), we linked a
        // different fontconfig than the pinned one.
        // SAFETY: `FcGetVersion` takes no arguments and only reads a compile-time
        // constant; it is safe to call at any time.
        let version = unsafe { FcGetVersion() };
        assert_eq!(version, 21402, "vendored fontconfig FcGetVersion (2.14.2)");

        unsafe {
            // Load the configuration WITHOUT scanning fonts or touching the
            // cache (the reference's `initLoadConfig` path). Non-null on success.
            let config = FcInitLoadConfig();
            assert!(!config.is_null(), "FcInitLoadConfig returned NULL");

            // Build a pattern and round-trip the `family` property.
            let pat = FcPatternCreate();
            assert!(!pat.is_null(), "FcPatternCreate returned NULL");

            // FC_FAMILY is the string macro `"family"`; bindgen emits it as a
            // NUL-terminated byte array, usable directly as a C string pointer.
            let family_obj = FC_FAMILY.as_ptr() as *const c_char;
            let monospace = b"monospace\0";

            let added = FcPatternAddString(pat, family_obj, monospace.as_ptr());
            assert_ne!(added, 0, "FcPatternAddString(FC_FAMILY, monospace)");

            let mut out: *mut FcChar8 = ptr::null_mut();
            let result = FcPatternGetString(pat, family_obj, 0 as c_int, &mut out);
            // FcResultMatch == 0 (the first FcResult enumerator).
            assert_eq!(result, 0, "FcPatternGetString result must be FcResultMatch");
            assert!(!out.is_null(), "FcPatternGetString yielded a NULL string");
            let got = CStr::from_ptr(out as *const c_char);
            assert_eq!(
                got.to_str().unwrap(),
                "monospace",
                "round-tripped FC_FAMILY value"
            );

            // Tear down (the string `out` is owned by the pattern; destroying the
            // pattern frees it).
            FcPatternDestroy(pat);
            FcConfigDestroy(config);
        }
    }
}
