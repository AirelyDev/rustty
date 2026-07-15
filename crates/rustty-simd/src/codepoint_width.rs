//! codepoint_width — PORTS ghostty src/simd/codepoint_width.zig.

/// `options.simd` — whether SIMD-accelerated code paths are built. Mirrors the
/// reference's `build_options.simd`, which defaults to `true`.
// WASM DIVERGENCE: on `wasm32` the SIMD branch is gated OFF (no C++ kernel on
// freestanding wasm), so this const is unreferenced there — the scalar
// `rustty_unicode` lane below is always taken.
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
const SIMD: bool = true;

// vt.cpp — gated OFF on `wasm32` (no C++ kernel on freestanding wasm).
#[cfg(not(target_arch = "wasm32"))]
extern "C" {
    fn rustty_simd_codepoint_width(cp: u32) -> i8;
}

#[allow(non_snake_case)]
pub fn codepointWidth(cp: u32) -> i8 {
    #[cfg(not(target_arch = "wasm32"))]
    if SIMD {
        return unsafe { rustty_simd_codepoint_width(cp) };
    }
    // const uucode = @import("uucode");
    // R3: rustty has no `uucode` Cargo dependency; the uucode-derived data is
    // embedded in `rustty_unicode` (props_uucode), which returns the identical
    // values uucode would for every code point in range.
    if cp > rustty_unicode::props_uucode::MAX_CODE_POINT {
        return 1;
    }
    rustty_unicode::props_uucode::get(cp).width as i8
}

#[cfg(test)]
mod tests {
    // `codepointWidth_basic` carries the reference test name
    // `"codepointWidth basic"` (camelCase `codepointWidth` preserved); spaces
    // are not valid in a Rust fn name.
    #![allow(non_snake_case)]
    use super::*;

    #[test]
    fn codepointWidth_basic() {
        assert_eq!(1_i8, codepointWidth(b'a' as u32));
        assert_eq!(1_i8, codepointWidth(0x100)); // Ā
        assert_eq!(2_i8, codepointWidth(0x3400)); // 㐀
        assert_eq!(2_i8, codepointWidth(0x2E3A)); // ⸺
        assert_eq!(2_i8, codepointWidth(0x1F1E6)); // 🇦
        assert_eq!(2_i8, codepointWidth(0x4E00)); // 一
        assert_eq!(2_i8, codepointWidth(0xF900)); // 豈
        assert_eq!(2_i8, codepointWidth(0x20000)); // 𠀀
        assert_eq!(2_i8, codepointWidth(0x30000)); // 𠀀
                                                   // try testing.expectEqual(@as(i8, 1), @import("uucode").get(.width, 0x100));
    }

    // This is not very fast in debug modes, so its commented by default.
    // IMPORTANT: UNCOMMENT THIS WHENEVER MAKING CODEPOINTWIDTH CHANGES.
    // test "codepointWidth matches uucode" {
    //     const testing = std.testing;
    //     const uucode = @import("uucode");
    //
    //     const min = 0xFF + 1; // start outside ascii
    //     const max = std.math.maxInt(u21) + 1;
    //     for (min..max) |cp| {
    //         const simd = codepointWidth(@intCast(cp));
    //         const uu = if (cp > uucode.config.max_code_point)
    //             1
    //         else
    //             uucode.get(.width, @intCast(cp));
    //         if (simd != uu) mismatch: {
    //             if (cp == 0x2E3B) {
    //                 try testing.expectEqual(@as(i8, 2), simd);
    //                 break :mismatch;
    //             }
    //
    //             std.log.warn("mismatch cp=U+{x} simd={} uucode={}", .{ cp, simd, uu });
    //             try testing.expect(false);
    //         }
    //     }
    // }
}
