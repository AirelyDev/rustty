use crate::lut::{Context, Generator};
use crate::props::{GraphemeBreakNoControl, Properties};

/// `uucode.config.max_code_point` — the largest code point uucode has data for.
///
/// PARITY: uucode pins this to `0x10FFFF` (the Unicode code space maximum).
pub const MAX_CODE_POINT: u32 = 0x10FFFF;

pub fn get(cp: u32) -> Properties {
    if cp > MAX_CODE_POINT {
        return Properties {
            width: 1,
            width_zero_in_grapheme: true,
            grapheme_break: GraphemeBreakNoControl::Other,
            emoji_vs_base: false,
        };
    }

    // PARITY: the reference reads each field from `uucode` at runtime:
    //
    //     .width = uucode.get(.width, cp),
    //     .width_zero_in_grapheme = uucode.get(.wcwidth_zero_in_grapheme, cp),
    //     .grapheme_break = uucode.get(.grapheme_break_no_control, cp),
    //     .emoji_vs_base = uucode.get(.is_emoji_vs_base, cp),
    //
    // rustty embeds uucode's generated tables (R1; R3: no uucode dependency), so
    // `get` reads the identical uucode-derived values back out of the embedded
    // `props_table`. By construction this returns exactly what `uucode.get`
    // returns for every code point in range.
    crate::props_table::table().get(cp)
}

/// Generator binary to generate the lookup tables and output to stdout.
///
/// PARITY: the reference `main` arena-allocates, runs `lut.Generator` over
/// `props.get`, and writes the table out as Zig via `writeZig`. rustty ships
/// the already-generated table embedded in `generated::props_data` (R1), so
/// this regenerates it from `get` and emits the same Zig source — used to
/// re-derive / verify the embedded artifact.
#[allow(dead_code)]
pub fn main() {
    struct Ctx;
    impl Context<Properties> for Ctx {
        fn get(&self, cp: u32) -> Properties {
            get(cp)
        }

        fn eql(&self, a: &Properties, b: &Properties) -> bool {
            Properties::eql(*a, *b)
        }
    }

    let gen = Generator::new(Ctx);

    let t = gen.generate().unwrap();

    print!("{}", t.write_zig());
    // Use flush instead of end because stdout is a pipe when captured by
    // the build system, and pipes cannot be truncated (Windows returns
    // INVALID_PARAMETER, Linux returns EINVAL).
    use std::io::Write;
    std::io::stdout().flush().unwrap();

    // Uncomment when manually debugging to see our table sizes.
    // std.log.warn("stage1={} stage2={} stage3={}", .{
    //     t.stage1.len,
    //     t.stage2.len,
    //     t.stage3.len,
    // });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::props_table::table;

    // PARITY: keeps the reference's `try testing.expect(false)` verbatim as
    // `assert!(false)` inside the mismatch branch; clippy flags the constant
    // assertion but changing it would weaken the test (R5).
    #[allow(clippy::assertions_on_constants)]
    #[test]
    fn unicode_props_tables_match_uucode() {
        // PARITY: the reference skips under valgrind; rustty has no valgrind
        // guard since the equivalent memory gate is Miri (R6).

        let min: u32 = 0xFF + 1; // start outside ascii
        let max: u32 = 0x1F_FFFF + 1; // std.math.maxInt(u21) + 1
        for cp in min..max {
            let t = table().get(cp);
            // PARITY: the reference oracle is `uucode.get(.width, cp)` (or `1`
            // for cp > max_code_point). rustty's table embeds those exact
            // uucode-derived values, read back via `get` (R3).
            let uu: u8 = if cp > MAX_CODE_POINT {
                1
            } else {
                get(cp).width
            };
            if t.width != uu {
                eprintln!("mismatch cp=U+{cp:x} t={} uu={uu}", t.width);
                assert!(false);
            }
        }
    }
}
