//! codepoint_width — PORTS ghostty src/simd/codepoint_width.zig.

/// `options.simd` — whether SIMD-accelerated code paths are built. Mirrors the
/// reference's `build_options.simd`, which defaults to `true`.
// WASM DIVERGENCE: on `wasm32` the SIMD branch is gated OFF (no C++ kernel on
// freestanding wasm), so the value passed to `codepoint_width_lane` is not
// consulted there — the scalar `rustty_unicode` lane below is always taken
// (the reference's `simd = false` lane; `Config.zig` forces it on wasm32).
const SIMD: bool = true;

// vt.cpp — gated OFF on `wasm32` (no C++ kernel on freestanding wasm).
#[cfg(not(target_arch = "wasm32"))]
extern "C" {
    fn rustty_simd_codepoint_width(cp: u32) -> i8;
}

#[allow(non_snake_case)]
// PERF(port): `#[inline]` — a non-generic cross-crate fn is un-inlinable
// without LTO (banned); the reference's `codepointWidth` is a plain Zig fn in
// the same compilation unit, so its callers never pay a frame here.
#[inline]
pub fn codepointWidth(cp: u32) -> i8 {
    codepoint_width_lane::<SIMD>(cp)
}

/// `codepointWidth` with `options.simd` as a const parameter (R8: the build
/// switch stays a switch), so the host's tests can run the `simd = false` lane
/// that wasm32 builds take.
#[inline(always)]
fn codepoint_width_lane<const USE_SIMD: bool>(cp: u32) -> i8 {
    // if (comptime options.simd) return ghostty_simd_codepoint_width(cp);
    #[cfg(not(target_arch = "wasm32"))]
    if USE_SIMD {
        // PERF(port) (R10 reshape): the C++ kernel's OWN first statement is
        //   `if (input <= 0xFF) return 1;`  (cbits/simd/codepoint_width.cpp,
        //   `CodepointWidth`). Hoisting it here is byte-identical for every
        // input and skips the FFI frame + highway's dynamic-dispatch resolution
        // for the ASCII/Latin-1 share of real input. Machine-checked by
        // `codepointWidth_matches_kernel` (exhaustive 0..=0x10FFFF) below.
        //
        // CORRECTION (parity S120-F1): the hoist used to sit above this `if`,
        // so the `simd = false` lane also answered 1 for U+0000..U+00FF,
        // where the reference asks uucode (0 for C0/C1 controls). It belongs to
        // the kernel, so it lives inside the SIMD arm.
        if cp <= 0xFF {
            return 1;
        }
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

    /// Parity S120-F1: the `simd = false` lane (what a wasm32 build runs) is
    /// the uucode width for every code point, including U+0000..U+00FF, where
    /// uucode gives 0 to C0/C1 controls. The SIMD kernel answers 1 there.
    #[test]
    fn codepoint_width_scalar_lane_uses_uucode_below_0x100() {
        let lane = super::codepoint_width_lane::<false>;
        assert_eq!(0_i8, lane(0x00));
        assert_eq!(0_i8, lane(0x09)); // TAB
        assert_eq!(0_i8, lane(0x7F));
        assert_eq!(0_i8, lane(0x9F));
        assert_eq!(1_i8, lane(b'a' as u32));
        for cp in 0..=0xFFu32 {
            assert_eq!(
                rustty_unicode::props_uucode::get(cp).width as i8,
                lane(cp),
                "cp={cp:#x}"
            );
        }
        // Above the table the lane answers 1, as the reference does.
        assert_eq!(1_i8, lane(rustty_unicode::props_uucode::MAX_CODE_POINT + 1));
    }

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

    /// PERF(port) guard for the ASCII early-out hoist in `codepointWidth`:
    /// the wrapper must agree with the raw kernel on every scalar (the hoisted
    /// predicate is the kernel's own first statement).
    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn codepointWidth_matches_kernel() {
        if !super::SIMD {
            return;
        }
        for cp in 0..=0x10_FFFFu32 {
            assert_eq!(
                super::codepointWidth(cp),
                unsafe { super::rustty_simd_codepoint_width(cp) },
                "cp={cp:#x}"
            );
        }
    }
}
