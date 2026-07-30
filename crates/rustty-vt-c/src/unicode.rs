//! rustty-vt-c unicode
//!
//! PORTS: ghostty src/terminal/c/unicode.zig (BL-5 fc5a72772 + 65e61282a)

// pub fn codepoint_width(cp: u32) callconv(lib.calling_conv) u8
/// Total codepoint-width projection: values beyond U+10FFFF are width 1.
pub fn codepoint_width(cp: u32) -> u8 {
    // if (cp > 0x10FFFF) return 1;
    if cp > 0x10FFFF {
        return 1;
    }
    rustty_unicode::codepoint_width(cp)
}

// pub fn grapheme_width(cps: ?[*]const u32, len: usize, width: ?*u8) ... usize
/// # Safety
/// `cps`, if non-null, must be readable for `len` u32s; `width` writable or null.
pub unsafe fn grapheme_width(cps: *const u32, len: usize, width: *mut u8) -> usize {
    // if (len == 0) { if (width) |ptr| ptr.* = 0; return 0; }
    if len == 0 {
        if !width.is_null() {
            // SAFETY: caller contract.
            unsafe { *width = 0 };
        }
        return 0;
    }
    // const ptr = cps orelse { if (width) |out| out.* = 0; return 0; };
    if cps.is_null() {
        if !width.is_null() {
            // SAFETY: caller contract.
            unsafe { *width = 0 };
        }
        return 0;
    }
    // SAFETY: caller contract.
    let slice = unsafe { std::slice::from_raw_parts(cps, len) };
    let result = rustty_unicode::grapheme::grapheme_width(slice);
    if !width.is_null() {
        // SAFETY: caller contract.
        unsafe { *width = result.width };
    }
    result.len
}

#[cfg(test)]
mod tests {
    use super::*;

    // test "codepoint_width narrow/wide/zero/out of range" (oracle fc5a72772)
    #[test]
    fn codepoint_width_cases() {
        assert_eq!(1, codepoint_width('a' as u32));
        assert_eq!(2, codepoint_width(0x4E00));
        assert_eq!(0, codepoint_width(0x0301));
        assert_eq!(1, codepoint_width(0x110000));
        assert_eq!(1, codepoint_width(u32::MAX));
    }

    // test "grapheme_width empty" (oracle 65e61282a)
    #[test]
    fn grapheme_width_empty() {
        let mut width: u8 = 42;
        assert_eq!(0, unsafe {
            grapheme_width(std::ptr::null(), 0, &mut width)
        });
        assert_eq!(0, width);
    }

    // test "grapheme_width null width" (oracle 65e61282a)
    #[test]
    fn grapheme_width_null_width() {
        let cps: [u32; 2] = [0x2764, 0xFE0F];
        assert_eq!(2, unsafe {
            grapheme_width(cps.as_ptr(), cps.len(), std::ptr::null_mut())
        });
    }

    // test "grapheme_width out of range" (oracle 65e61282a)
    #[test]
    fn grapheme_width_out_of_range() {
        let mut width: u8 = 0;
        let invalid_first: [u32; 2] = [0x110000, 0x0301];
        assert_eq!(1, unsafe {
            grapheme_width(invalid_first.as_ptr(), invalid_first.len(), &mut width)
        });
        assert_eq!(1, width);
        let invalid_second: [u32; 2] = ['a' as u32, 0x110000];
        assert_eq!(1, unsafe {
            grapheme_width(invalid_second.as_ptr(), invalid_second.len(), &mut width)
        });
        assert_eq!(1, width);
    }

    // test "grapheme_width emoji sequence" (oracle 65e61282a)
    #[test]
    fn grapheme_width_emoji_sequence() {
        let mut width: u8 = 0;
        let cps: [u32; 5] = [0x1F468, 0x200D, 0x1F469, 0x200D, 0x1F467];
        assert_eq!(cps.len(), unsafe {
            grapheme_width(cps.as_ptr(), cps.len(), &mut width)
        });
        assert_eq!(2, width);
    }
}
