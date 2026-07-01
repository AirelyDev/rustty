//! oniguruma-sys — FFI bindings to the vendored Oniguruma regex engine (`links = "onig"`).
//!
//! PORT STATUS: vendored (full library)
//!
//! R3: this crate copies the SAME C library the reference uses — Oniguruma
//! 6.9.9 — into `vendor/` and builds it standalone (`cc` + generated `config.h`
//! + `bindgen`); see build.rs. No pure-Rust substitution during parity phases.
//!
//! The FFI surface is bindgen-generated from `oniguruma.h` (the same header the
//! reference's `pkg/oniguruma/c.zig` @cImports) and re-exported verbatim below.
//! Downstream, the R1 safe-wrapper crate builds the frozen `oni::*` API on top
//! of these raw symbols.

#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

#[cfg(test)]
mod kat {
    use super::*;

    /// Known-answer test: initialize Oniguruma for UTF-8, compile the pattern
    /// `foo`, search `"hello foo bar"`, and assert the engine reports the match
    /// at byte offsets [6, 9). This exercises the full vendored pipeline
    /// (`onig_initialize` → `onig_new` → `onig_search` → `OnigRegion`), proving
    /// the standalone C build + FFI link are correct.
    #[test]
    fn onig_search_finds_known_match() {
        unsafe {
            // Initialize the library with the UTF-8 encoding (matches the
            // reference testing.ensureInit, which inits only `Encoding.utf8`).
            let mut enc: OnigEncoding = &raw mut OnigEncodingUTF8;
            let rc = onig_initialize(&mut enc, 1);
            assert_eq!(rc, 0, "onig_initialize failed: {rc}");

            let pattern: &[u8] = b"foo";
            let mut regex: OnigRegex = std::ptr::null_mut();
            let mut einfo: OnigErrorInfo = std::mem::zeroed();
            let rc = onig_new(
                &mut regex,
                pattern.as_ptr(),
                pattern.as_ptr().add(pattern.len()),
                ONIG_OPTION_NONE,
                &raw mut OnigEncodingUTF8,
                OnigDefaultSyntax,
                &mut einfo,
            );
            assert_eq!(rc, 0, "onig_new failed: {rc}");
            assert!(!regex.is_null(), "onig_new produced a null regex");

            let haystack: &[u8] = b"hello foo bar";
            let region = onig_region_new();
            assert!(!region.is_null(), "onig_region_new returned null");

            let pos = onig_search(
                regex,
                haystack.as_ptr(),
                haystack.as_ptr().add(haystack.len()),
                haystack.as_ptr(),
                haystack.as_ptr().add(haystack.len()),
                region,
                ONIG_OPTION_NONE,
            );

            // onig_search returns the byte offset of the match (>= 0) or
            // ONIG_MISMATCH (-1) on no match.
            assert_eq!(pos, 6, "expected match at offset 6, got {pos}");

            let r = &*region;
            assert_eq!(r.num_regs, 1, "expected one capture region");
            let begs = std::slice::from_raw_parts(r.beg, r.num_regs as usize);
            let ends = std::slice::from_raw_parts(r.end, r.num_regs as usize);
            assert_eq!(begs[0], 6, "match start");
            assert_eq!(ends[0], 9, "match end");

            // Verify a genuine mismatch is reported as ONIG_MISMATCH.
            let none: &[u8] = b"hello";
            let region2 = onig_region_new();
            let miss = onig_search(
                regex,
                none.as_ptr(),
                none.as_ptr().add(none.len()),
                none.as_ptr(),
                none.as_ptr().add(none.len()),
                region2,
                ONIG_OPTION_NONE,
            );
            assert_eq!(miss, ONIG_MISMATCH, "expected ONIG_MISMATCH");

            onig_region_free(region2, 1);
            onig_region_free(region, 1);
            onig_free(regex);
            onig_end();
        }
    }
}
