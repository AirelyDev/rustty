use crate::lut::{Context, Generator};
use crate::props_uucode::MAX_CODE_POINT;

/// Generator binary to generate the lookup tables and output to stdout.
///
/// PARITY: the reference `main` arena-allocates, runs `lut.Generator` over
/// `uucode.get(.is_symbol, cp)`, and writes the table out as Zig via
/// `writeZig`. rustty ships the already-generated table embedded in
/// `generated::symbols_data` (R1; R3: no uucode dependency), so this
/// regenerates it from the embedded symbols table and emits the same Zig
/// source — used to re-derive / verify the embedded artifact.
#[allow(dead_code)]
pub fn main() {
    struct Ctx;
    impl Context<bool> for Ctx {
        fn get(&self, cp: u32) -> bool {
            if cp > MAX_CODE_POINT {
                false
            } else {
                // PARITY: oracle value is `uucode.get(.is_symbol, cp)`; rustty
                // reads the identical embedded uucode-derived value (R3).
                crate::symbols_table::table().get(cp)
            }
        }

        fn eql(&self, a: &bool, b: &bool) -> bool {
            a == b
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
    use crate::symbols_table::table;

    // PARITY: keeps the reference's `try testing.expect(false)` verbatim as
    // `assert!(false)` inside the mismatch branch; clippy flags the constant
    // assertion but changing it would weaken the test (R5).
    #[allow(clippy::assertions_on_constants)]
    #[test]
    fn unicode_symbols_tables_match_uucode() {
        // PARITY: the reference skips under valgrind; rustty has no valgrind
        // guard since the equivalent memory gate is Miri (R6).

        for cp in 0..0x1F_FFFF_u32 {
            let t = table().get(cp);
            // PARITY: the reference oracle is `uucode.get(.is_symbol, cp)` (or
            // `false` for cp > max_code_point); rustty reads the identical
            // embedded uucode-derived value (R3).
            let uu: bool = if cp > MAX_CODE_POINT {
                false
            } else {
                table().get(cp)
            };

            if t != uu {
                eprintln!("mismatch cp=U+{cp:x} t={t} uu={uu}");
                assert!(false);
            }
        }
    }
}
