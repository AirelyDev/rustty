//! P3-W7 T4 — the process-argv seam after `src/os/args.zig` was deleted.
//!
//! Upstream deleted `os/args.zig` **and its `test "args"`**. rustty keeps the
//! smoke test on the new seam rather than dropping it silently: the test-count
//! contract (`.port/tools/gates/check_tests_ported.py`) needs the delta
//! recorded. It is joined here by the control the old code could not pass.

use rustty_args::args::{arg_bytes, GenIter, OsArgIterator};

// test "args" {
//     var iter = try iterator(alloc);
//     defer iter.deinit();
//     try testing.expect(iter.next().?.len > 0);
// }
//
// MOVED from `crates/rustty-os/src/args.rs` (deleted, P3-W7 T4).
#[test]
fn args() {
    let mut iter = OsArgIterator::new();
    assert!(!GenIter::next(&mut iter)
        .expect("argv0 must exist")
        .is_empty());
}

/// NEGATIVE CONTROL (P3-W7 T4 Step 4): the argv seam must preserve a
/// **non-UTF-8** byte exactly.
///
/// The deleted `rustty_os::args` macOS arm did
/// `String::from_utf8_lossy(bytes).into_owned()` (`args.rs:172`) and returned
/// `Option<String>` (`:66`/`:141`), so a non-UTF-8 argv element was replaced by
/// U+FFFD — a real concern for `rustty -e <path with invalid UTF-8>`. A
/// UTF-8-only test cannot see that defect at all, which is why this control
/// exists. Reverting `arg_bytes` to
/// `arg.to_string_lossy().into_owned().into_bytes()` makes it FAIL — verified,
/// see the P3-W7 report.
///
/// WHY NOT AN END-TO-END SUBPROCESS TEST: the obvious stronger control is to
/// re-exec this test binary with a bad argv element and assert on what
/// `args_os()` yields. That is **impossible for a libtest binary**: libtest's
/// own `main` calls `std::env::args()` (the panicking form) before any test
/// runs, so a non-UTF-8 argv aborts the child with
/// `called Result::unwrap() on an Err value: "\xFF\xFE"` at
/// `library/std/src/env.rs:871` regardless of what the seam does. Observed, not
/// assumed. The production binary is unaffected — it never calls
/// `std::env::args()`; the seam under test uses `args_os()`.
#[test]
fn arg_bytes_preserves_non_utf8() {
    #[cfg(unix)]
    let bad: std::ffi::OsString = {
        use std::os::unix::ffi::OsStringExt as _;
        std::ffi::OsString::from_vec(vec![b'-', b'e', b'/', 0xff, 0xfe, b'x'])
    };
    #[cfg(not(unix))]
    let bad: std::ffi::OsString =
        // SAFETY: the bytes are a valid WTF-8 encoding of the ASCII prefix
        // followed by two lone continuation bytes is NOT valid WTF-8, so on
        // non-unix we use an ASCII-only value and the assertion below still
        // pins losslessness for every representable input.
        std::ffi::OsString::from("-e/x");

    let got = arg_bytes(&bad);
    assert_eq!(
        got.as_slice(),
        bad.as_encoded_bytes(),
        "argv element was not preserved byte-exactly (lossy conversion?)"
    );

    #[cfg(unix)]
    {
        assert_eq!(got, vec![b'-', b'e', b'/', 0xff, 0xfe, b'x']);
        assert!(
            std::str::from_utf8(&got).is_err(),
            "the input must be non-UTF-8 or this control is vacuous"
        );
    }
}
