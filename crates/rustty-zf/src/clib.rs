//! clib.zig — the C ABI over `filter`.
//!
//! Zig `export fn {rank,rankNeedle,highlight,highlightNeedle}` (zf 0.11.0; 0.10.3 had
//! `rankToken`/`highlightToken`) → Rust
//! `#[no_mangle] pub unsafe extern "C" fn rustty_zf_*` (R2 export names). The
//! `ghostty_zf_*` / `zf_*` alias exports are DEFERRED to a later chunk — this
//! crate ships only the `rustty_zf_*` names.
//!
//! `?f64` collapses to the C sentinel `-1.0` on no-match; the `?usize` slice
//! length collapses to the number of matches. The null-terminated inputs
//! (`[*:0]const u8`) are rebuilt with `CStr::from_ptr(..).to_bytes()` (the
//! faithful `std.mem.span`). The needle pointer array is walked lazily with
//! pointer arithmetic — mirroring Zig's `needles[index]` — so a `num_needles`
//! larger than the backing array (the `zig`/2 no-match test) never forms an
//! out-of-bounds slice and stays Miri-clean (Zig bails before the OOB index too).

// const filter = @import("filter.zig");
// const std = @import("std");
// const testing = std.testing;
use crate::filter;
use std::ffi::{c_char, CStr};

/// rank a given haystack against a slice of needles
#[no_mangle]
pub unsafe extern "C" fn rustty_zf_rank(
    haystack: *const c_char,
    needles: *const *const c_char,
    num_needles: usize,
    case_sensitive: bool,
    plain: bool,
) -> f64 {
    let string = unsafe { CStr::from_ptr(haystack) }.to_bytes();
    let filename = if plain {
        None
    } else {
        Some(filter::basename(string))
    };

    let mut total_rank: f64 = 0.0;
    let mut index: usize = 0;
    while index < num_needles {
        let needle = unsafe { CStr::from_ptr(*needles.add(index)) }.to_bytes();
        let strict_path = filter::has_separator(needle);
        if let Some(r) = filter::rank_needle(string, filename, needle, case_sensitive, strict_path)
        {
            total_rank += r;
        } else {
            return -1.0;
        }
        index += 1;
    }
    total_rank
}

/// rank a given haystack against a single needle
#[no_mangle]
pub unsafe extern "C" fn rustty_zf_rank_needle(
    haystack: *const c_char,
    filename: *const c_char,
    needle: *const c_char,
    case_sensitive: bool,
    strict_path: bool,
) -> f64 {
    let string = unsafe { CStr::from_ptr(haystack) }.to_bytes();
    let name = if filename.is_null() {
        None
    } else {
        Some(unsafe { CStr::from_ptr(filename) }.to_bytes())
    };
    let n = unsafe { CStr::from_ptr(needle) }.to_bytes();
    // Zig `if (rankNeedle(...)) |r| return r else return -1.0;` — the C -1.0 sentinel.
    filter::rank_needle(string, name, n, case_sensitive, strict_path).unwrap_or(-1.0)
}

#[no_mangle]
pub unsafe extern "C" fn rustty_zf_highlight(
    haystack: *const c_char,
    needles: *const *const c_char,
    needles_len: usize,
    case_sensitive: bool,
    plain: bool,
    matches: *mut usize,
    matches_len: usize,
) -> usize {
    let string = unsafe { CStr::from_ptr(haystack) }.to_bytes();
    let filename = if plain {
        None
    } else {
        Some(filter::basename(string))
    };
    let matches_slice = unsafe { std::slice::from_raw_parts_mut(matches, matches_len) };

    let mut index: usize = 0;
    let mut needle_index: usize = 0;
    while needle_index < needles_len {
        let needle = unsafe { CStr::from_ptr(*needles.add(needle_index)) }.to_bytes();
        let strict_path = filter::has_separator(needle);
        let matched = filter::highlight_needle(
            string,
            filename,
            needle,
            case_sensitive,
            strict_path,
            &mut matches_slice[index..],
        );
        index += matched.len();
        needle_index += 1;
    }

    index
}

#[no_mangle]
pub unsafe extern "C" fn rustty_zf_highlight_needle(
    haystack: *const c_char,
    filename: *const c_char,
    needle: *const c_char,
    case_sensitive: bool,
    strict_path: bool,
    matches: *mut usize,
    matches_len: usize,
) -> usize {
    let string = unsafe { CStr::from_ptr(haystack) }.to_bytes();
    let name = if filename.is_null() {
        None
    } else {
        Some(unsafe { CStr::from_ptr(filename) }.to_bytes())
    };
    let n = unsafe { CStr::from_ptr(needle) }.to_bytes();
    let matches_slice = unsafe { std::slice::from_raw_parts_mut(matches, matches_len) };
    let matched =
        filter::highlight_needle(string, name, n, case_sensitive, strict_path, matches_slice);
    matched.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rank_exported_c_library_interface() {
        unsafe {
            {
                let needles: [*const c_char; 2] = [c"a".as_ptr(), c"z".as_ptr()];
                assert!(
                    rustty_zf_rank(c"abcdefg".as_ptr(), needles.as_ptr(), 2, false, false) == -1.0
                );
            }
            {
                let needles: [*const c_char; 2] = [c"a".as_ptr(), c"b".as_ptr()];
                assert!(
                    rustty_zf_rank(c"abcdefg".as_ptr(), needles.as_ptr(), 2, false, false) != -1.0
                );
            }
            {
                let needles: [*const c_char; 2] = [c"a".as_ptr(), c"B".as_ptr()];
                assert!(
                    rustty_zf_rank(c"abcdefg".as_ptr(), needles.as_ptr(), 2, true, false) == -1.0
                );
            }
            {
                let needles: [*const c_char; 2] = [c"a".as_ptr(), c"B".as_ptr()];
                assert!(
                    rustty_zf_rank(c"aBcdefg".as_ptr(), needles.as_ptr(), 2, true, false) != -1.0
                );
            }
            {
                // `num_needles = 2` over a 1-element array: `zig` fails to match, so
                // `rank` returns -1 before ever indexing `needles[1]` (Zig quirk).
                let needles: [*const c_char; 1] = [c"zig".as_ptr()];
                assert!(
                    rustty_zf_rank(
                        c"a/path/to/file".as_ptr(),
                        needles.as_ptr(),
                        2,
                        false,
                        false
                    ) == -1.0
                );
            }
            {
                let needles: [*const c_char; 2] = [c"path".as_ptr(), c"file".as_ptr()];
                assert!(
                    rustty_zf_rank(
                        c"a/path/to/file".as_ptr(),
                        needles.as_ptr(),
                        2,
                        false,
                        false
                    ) != -1.0
                );
            }

            assert!(
                rustty_zf_rank_needle(
                    c"abcdefg".as_ptr(),
                    std::ptr::null(),
                    c"a".as_ptr(),
                    false,
                    false
                ) != -1.0
            );
            assert!(
                rustty_zf_rank_needle(
                    c"abcdefg".as_ptr(),
                    std::ptr::null(),
                    c"z".as_ptr(),
                    false,
                    false
                ) == -1.0
            );
            assert!(
                rustty_zf_rank_needle(
                    c"abcdefG".as_ptr(),
                    std::ptr::null(),
                    c"G".as_ptr(),
                    true,
                    false
                ) != -1.0
            );
            assert!(
                rustty_zf_rank_needle(
                    c"abcdefg".as_ptr(),
                    std::ptr::null(),
                    c"A".as_ptr(),
                    true,
                    false
                ) == -1.0
            );
            assert!(
                rustty_zf_rank_needle(
                    c"a/path/to/file".as_ptr(),
                    c"file".as_ptr(),
                    c"file".as_ptr(),
                    false,
                    false
                ) != -1.0
            );
            assert!(
                rustty_zf_rank_needle(
                    c"a/path/to/file".as_ptr(),
                    c"file".as_ptr(),
                    c"zig".as_ptr(),
                    false,
                    false
                ) == -1.0
            );

            // zero length haystacks and needles
            {
                let needles: [*const c_char; 1] = [c"a".as_ptr()];
                assert!(rustty_zf_rank(c"".as_ptr(), needles.as_ptr(), 1, false, false) == -1.0);
            }
            assert!(
                rustty_zf_rank_needle(c"".as_ptr(), std::ptr::null(), c"a".as_ptr(), false, false)
                    == -1.0
            );
            {
                let needles: [*const c_char; 1] = [c"".as_ptr()];
                assert!(rustty_zf_rank(c"a".as_ptr(), needles.as_ptr(), 1, false, false) == -1.0);
            }
            assert!(
                rustty_zf_rank_needle(c"a".as_ptr(), std::ptr::null(), c"".as_ptr(), false, false)
                    == -1.0
            );
        }
    }

    // Mirrors clib.zig's `testHighlight` helper.
    // (Its callers are the POSIX-separator tests; task W-2.)
    #[cfg(not(windows))]
    unsafe fn test_highlight(
        expected: &[usize],
        haystack: *const c_char,
        needles: &[*const c_char],
        case_sensitive: bool,
        plain: bool,
        matches_buf: &mut [usize],
    ) {
        let len = unsafe {
            rustty_zf_highlight(
                haystack,
                needles.as_ptr(),
                needles.len(),
                case_sensitive,
                plain,
                matches_buf.as_mut_ptr(),
                matches_buf.len(),
            )
        };
        assert_eq!(&matches_buf[0..len], expected);
    }

    // Not on Windows (task W-2, R14 form 1): upstream zf's own test hardcodes `/`
    // path separators, while `sep` is `std.fs.path.sep` (`'\\'` on Windows), so it
    // cannot pass there upstream either (zf's tests are not part of the reference's
    // Windows run). The Windows separator is covered by
    // `filter::windows_separator_is_the_path_separator`.
    #[cfg(not(windows))]
    #[test]
    fn highlight_exported_c_library_interface() {
        unsafe {
            let mut matches_buf: [usize; 128] = [0; 128];

            test_highlight(
                &[0, 5],
                c"abcdef".as_ptr(),
                &[c"a".as_ptr(), c"f".as_ptr()],
                false,
                false,
                &mut matches_buf,
            );
            test_highlight(
                &[0, 5],
                c"abcdeF".as_ptr(),
                &[c"a".as_ptr(), c"F".as_ptr()],
                true,
                false,
                &mut matches_buf,
            );
            test_highlight(
                &[2, 3, 4, 5, 10, 11, 12, 13],
                c"a/path/to/file".as_ptr(),
                &[c"path".as_ptr(), c"file".as_ptr()],
                false,
                false,
                &mut matches_buf,
            );

            let mut len = rustty_zf_highlight_needle(
                c"abcdef".as_ptr(),
                std::ptr::null(),
                c"a".as_ptr(),
                false,
                false,
                matches_buf.as_mut_ptr(),
                matches_buf.len(),
            );
            assert_eq!(&matches_buf[0..len], &[0][..]);
            len = rustty_zf_highlight_needle(
                c"abcdeF".as_ptr(),
                std::ptr::null(),
                c"F".as_ptr(),
                true,
                false,
                matches_buf.as_mut_ptr(),
                matches_buf.len(),
            );
            assert_eq!(&matches_buf[0..len], &[5][..]);
            len = rustty_zf_highlight_needle(
                c"a/path/to/file".as_ptr(),
                c"file".as_ptr(),
                c"file".as_ptr(),
                false,
                false,
                matches_buf.as_mut_ptr(),
                matches_buf.len(),
            );
            assert_eq!(&matches_buf[0..len], &[10, 11, 12, 13][..]);

            // highlights with basename trailing slashes
            len = rustty_zf_highlight_needle(
                c"s/".as_ptr(),
                c"s".as_ptr(),
                c"s".as_ptr(),
                false,
                false,
                matches_buf.as_mut_ptr(),
                matches_buf.len(),
            );
            assert_eq!(&matches_buf[0..len], &[0][..]);
            len = rustty_zf_highlight_needle(
                c"/this/is/path/not/a/file/".as_ptr(),
                c"file".as_ptr(),
                c"file".as_ptr(),
                false,
                false,
                matches_buf.as_mut_ptr(),
                matches_buf.len(),
            );
            assert_eq!(&matches_buf[0..len], &[20, 21, 22, 23][..]);

            // disconnected highlights
            test_highlight(
                &[0, 2, 3],
                c"ababab".as_ptr(),
                &[c"aab".as_ptr()],
                false,
                false,
                &mut matches_buf,
            );
            test_highlight(
                &[6, 8, 9],
                c"abbbbbabab".as_ptr(),
                &[c"aab".as_ptr()],
                false,
                false,
                &mut matches_buf,
            );
            test_highlight(
                &[0, 2, 6],
                c"abcdefg".as_ptr(),
                &[c"acg".as_ptr()],
                false,
                false,
                &mut matches_buf,
            );
            test_highlight(
                &[2, 3, 4, 5, 9, 10],
                c"__init__.py".as_ptr(),
                &[c"initpy".as_ptr()],
                false,
                false,
                &mut matches_buf,
            );

            // small buffer to ensure highlighting doesn't go out of range when the needles overflow
            let mut small_buf: [usize; 4] = [0; 4];
            test_highlight(
                &[0, 1, 2, 3],
                c"abcd".as_ptr(),
                &[c"ab".as_ptr(), c"cd".as_ptr(), c"abcd".as_ptr()],
                false,
                false,
                &mut small_buf,
            );
            test_highlight(
                &[0, 1, 2, 1],
                c"wxyz".as_ptr(),
                &[c"wxy".as_ptr(), c"xyz".as_ptr()],
                false,
                false,
                &mut small_buf,
            );

            // zero length haystacks and needles
            test_highlight(
                &[],
                c"".as_ptr(),
                &[c"a".as_ptr()],
                false,
                false,
                &mut matches_buf,
            );
            len = rustty_zf_highlight_needle(
                c"".as_ptr(),
                std::ptr::null(),
                c"a".as_ptr(),
                false,
                false,
                matches_buf.as_mut_ptr(),
                matches_buf.len(),
            );
            assert_eq!(&matches_buf[0..len], &[] as &[usize]);
            test_highlight(
                &[],
                c"a".as_ptr(),
                &[c"".as_ptr()],
                false,
                false,
                &mut matches_buf,
            );
            len = rustty_zf_highlight_needle(
                c"a".as_ptr(),
                std::ptr::null(),
                c"".as_ptr(),
                false,
                false,
                matches_buf.as_mut_ptr(),
                matches_buf.len(),
            );
            assert_eq!(&matches_buf[0..len], &[] as &[usize]);
        }
    }
}
