//! rustty-vt-c paste
//!
//! PORTS: ghostty src/terminal/c/paste.zig
//!
//! PORT STATUS: done

// PORT NOTE: the reference imports are
//   const std = @import("std");
//   const lib = @import("../lib.zig");
//   const paste = @import("../../input/paste.zig");
//   const Result = @import("result.zig").Result;
// `paste` is rustty-vt's `input::paste` module; `Result` is the sibling
// `result` module's `Result`.

use crate::result::Result;
use rustty_vt::input::paste;

/// PORT NOTE: `pub fn is_safe(data, len) callconv(...) bool`. The C calling
/// convention is the `extern "C"` wrapper in `lib.rs` (`rustty_paste_is_safe`).
/// `data` is the optional caller buffer (NULL ==> a zero-length slice).
///
/// # Safety
/// `data`, if non-null, must point to `len` readable bytes.
pub unsafe fn is_safe(data: *const u8, len: usize) -> bool {
    // const slice: []const u8 = if (data) |v| v[0..len] else &.{};
    // SAFETY: `data`/`len` describe a valid readable region when non-null
    // (caller contract).
    let slice: &[u8] = if data.is_null() {
        &[]
    } else {
        unsafe { std::slice::from_raw_parts(data, len) }
    };
    // return paste.isSafe(slice);
    paste::is_safe(slice)
}

/// PORT NOTE: `pub fn encode(data, data_len, bracketed, out_, out_len,
/// out_written) callconv(...) Result`. The C calling convention is the
/// `extern "C"` wrapper in `lib.rs` (`rustty_paste_encode`). `data` is mutated
/// in place by the strip pass (`paste.encode` takes `[]u8`), so the rustty
/// source-of-truth analog is `paste::encode_mut`.
///
/// # Safety
/// `data`, if non-null, must point to `data_len` writable bytes; `out_`, if
/// non-null, must point to `out_len` writable bytes; `out_written` must be a
/// valid, writable pointer to `usize`.
pub unsafe fn encode(
    data: *mut u8,
    data_len: usize,
    bracketed: bool,
    out_: *mut u8,
    out_len: usize,
    out_written: *mut usize,
) -> Result {
    // const slice: []u8 = if (data) |v| v[0..data_len] else &.{};
    // SAFETY: `data`/`data_len` describe a valid writable region when non-null
    // (caller contract).
    let mut empty: [u8; 0] = [];
    let slice: &mut [u8] = if data.is_null() {
        &mut empty
    } else {
        unsafe { std::slice::from_raw_parts_mut(data, data_len) }
    };
    // const result = paste.encode(slice, .{ .bracketed = bracketed });
    let result = paste::encode_mut(slice, paste::Options { bracketed });

    // const total = result[0].len + result[1].len + result[2].len;
    let total = result[0].len() + result[1].len() + result[2].len();
    // out_written.* = total;
    // SAFETY: `out_written` is a valid writable pointer (caller contract).
    unsafe { *out_written = total };

    // const out: []u8 = if (out_) |o| o[0..out_len] else &.{};
    // SAFETY: `out_`/`out_len` describe a valid writable region when non-null
    // (caller contract).
    let mut out_empty: [u8; 0] = [];
    let out: &mut [u8] = if out_.is_null() {
        &mut out_empty
    } else {
        unsafe { std::slice::from_raw_parts_mut(out_, out_len) }
    };
    // if (out.len < total) return .out_of_space;
    if out.len() < total {
        return Result::OutOfSpace;
    }

    // var offset: usize = 0;
    let mut offset: usize = 0;
    // for (result) |segment| {
    for segment in result {
        // @memcpy(out[offset..][0..segment.len], segment);
        out[offset..offset + segment.len()].copy_from_slice(segment);
        // offset += segment.len;
        offset += segment.len();
    }

    // return .success;
    Result::Success
}

#[cfg(test)]
mod tests {
    use super::*;

    // PORT NOTE: the reference allocates the input via `testing.allocator.dupe`
    // and frees it (proving the strip pass mutates a heap buffer, not a literal).
    // In Rust an owned `Vec<u8>`/array provides the same writable buffer; the
    // `dupe`/`free` pair has no separate observable behavior to assert.

    #[test]
    fn encode_bracketed() {
        // const input = try testing.allocator.dupe(u8, "hello");
        let mut input = b"hello".to_vec();
        // var buf: [64]u8 = undefined;
        let mut buf = [0u8; 64];
        // var written: usize = 0;
        let mut written: usize = 0;
        // const result = encode(input.ptr, input.len, true, &buf, buf.len, &written);
        let result = unsafe {
            encode(
                input.as_mut_ptr(),
                input.len(),
                true,
                buf.as_mut_ptr(),
                buf.len(),
                &mut written,
            )
        };
        // try testing.expectEqual(.success, result);
        assert_eq!(Result::Success, result);
        // try testing.expectEqualStrings("\x1b[200~hello\x1b[201~", buf[0..written]);
        assert_eq!(b"\x1b[200~hello\x1b[201~", &buf[0..written]);
    }

    #[test]
    fn encode_unbracketed_no_newlines() {
        let mut input = b"hello".to_vec();
        let mut buf = [0u8; 64];
        let mut written: usize = 0;
        // const result = encode(input.ptr, input.len, false, &buf, buf.len, &written);
        let result = unsafe {
            encode(
                input.as_mut_ptr(),
                input.len(),
                false,
                buf.as_mut_ptr(),
                buf.len(),
                &mut written,
            )
        };
        assert_eq!(Result::Success, result);
        // try testing.expectEqualStrings("hello", buf[0..written]);
        assert_eq!(b"hello", &buf[0..written]);
    }

    #[test]
    fn encode_unbracketed_newlines() {
        // const input = try testing.allocator.dupe(u8, "hello\nworld");
        let mut input = b"hello\nworld".to_vec();
        let mut buf = [0u8; 64];
        let mut written: usize = 0;
        let result = unsafe {
            encode(
                input.as_mut_ptr(),
                input.len(),
                false,
                buf.as_mut_ptr(),
                buf.len(),
                &mut written,
            )
        };
        assert_eq!(Result::Success, result);
        // try testing.expectEqualStrings("hello\rworld", buf[0..written]);
        assert_eq!(b"hello\rworld", &buf[0..written]);
    }

    #[test]
    fn encode_strip_unsafe_bytes() {
        // const input = try testing.allocator.dupe(u8, "hel\x1blo\x00world");
        let mut input = b"hel\x1blo\x00world".to_vec();
        let mut buf = [0u8; 64];
        let mut written: usize = 0;
        let result = unsafe {
            encode(
                input.as_mut_ptr(),
                input.len(),
                true,
                buf.as_mut_ptr(),
                buf.len(),
                &mut written,
            )
        };
        assert_eq!(Result::Success, result);
        // try testing.expectEqualStrings("\x1b[200~hel lo world\x1b[201~", buf[0..written]);
        assert_eq!(b"\x1b[200~hel lo world\x1b[201~", &buf[0..written]);
    }

    #[test]
    fn encode_with_insufficient_buffer() {
        let mut input = b"hello".to_vec();
        // var buf: [1]u8 = undefined;
        let mut buf = [0u8; 1];
        let mut written: usize = 0;
        let result = unsafe {
            encode(
                input.as_mut_ptr(),
                input.len(),
                true,
                buf.as_mut_ptr(),
                buf.len(),
                &mut written,
            )
        };
        // try testing.expectEqual(.out_of_space, result);
        assert_eq!(Result::OutOfSpace, result);
        // try testing.expectEqual(17, written);
        assert_eq!(17, written);
    }

    #[test]
    fn encode_with_null_buffer() {
        let mut input = b"hello".to_vec();
        let mut written: usize = 0;
        // const result = encode(input.ptr, input.len, true, null, 0, &written);
        let result = unsafe {
            encode(
                input.as_mut_ptr(),
                input.len(),
                true,
                std::ptr::null_mut(),
                0,
                &mut written,
            )
        };
        assert_eq!(Result::OutOfSpace, result);
        assert_eq!(17, written);
    }

    #[test]
    fn is_safe_with_safe_data() {
        // const safe = "hello world";
        let safe = b"hello world";
        // try testing.expect(is_safe(safe.ptr, safe.len));
        assert!(unsafe { is_safe(safe.as_ptr(), safe.len()) });
    }

    #[test]
    fn is_safe_with_newline() {
        // const unsafe = "hello\nworld";
        let unsafe_ = b"hello\nworld";
        // try testing.expect(!is_safe(unsafe.ptr, unsafe.len));
        assert!(!unsafe { is_safe(unsafe_.as_ptr(), unsafe_.len()) });
    }

    #[test]
    fn is_safe_with_bracketed_paste_end() {
        // const unsafe = "hello\x1b[201~world";
        let unsafe_ = b"hello\x1b[201~world";
        assert!(!unsafe { is_safe(unsafe_.as_ptr(), unsafe_.len()) });
    }

    #[test]
    fn is_safe_with_empty_data() {
        // const empty = "";
        let empty = b"";
        // try testing.expect(is_safe(empty.ptr, 0));
        assert!(unsafe { is_safe(empty.as_ptr(), 0) });
    }

    #[test]
    fn is_safe_with_null_empty_data() {
        // try testing.expect(is_safe(null, 0));
        assert!(unsafe { is_safe(std::ptr::null(), 0) });
    }
}
