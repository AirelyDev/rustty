//! Harness dumper for the rustty-unicode differential gate (ORACLE #2).
//!
//! Dumps rustty's EMBEDDED unicode tables over the full code-point range
//! 0..=0x10FFFF into raw byte files, in the identical layout the Zig uucode
//! oracle emits (`.port/phase-0/harness/unicode/uucode_oracle.zig`). The diff
//! script (`.port/phase-0/harness/unicode/diff_unicode.py`) then asserts 0
//! diffs for width, is_symbol, all Properties fields, and all 8192 grapheme
//! precompute entries.
//!
//! This is the faithful full-range reproduction of the reference's
//! `props_uucode.zig` / `symbols_uucode.zig` `test "... tables match uucode"`
//! blocks, which compare ghostty's generated tables against the LIVE uucode
//! oracle — independence that R3 forbids in-crate (no uucode Cargo dep). The
//! curated in-crate spot-checks (props_uucode.rs / symbols_uucode.rs tests)
//! cover representative points; this gate covers EVERY point.
//!
//! Usage: cargo run --example dump_tables -p rustty-unicode -- <out-dir>
//!
//! Out-of-range rule (reference): for cp > 0x10FFFF the reference returns
//! width=1 / symbol=false. The oracle and this dumper both stop at 0x10FFFF
//! (== uucode.config.max_code_point), so the >MAX rule is asserted separately
//! by the in-crate spot-checks.

use std::io::Write;

const MAX_CP: u32 = 0x10FFFF; // uucode.config.max_code_point

fn main() {
    let out_dir = std::env::args().nth(1).unwrap_or_else(|| {
        eprintln!("usage: dump_tables <out-dir>");
        std::process::exit(2);
    });
    std::fs::create_dir_all(&out_dir).expect("create out dir");

    let n = (MAX_CP + 1) as usize;
    let mut width = Vec::with_capacity(n);
    let mut symbol = Vec::with_capacity(n);
    let mut props_flags = Vec::with_capacity(n);
    let mut props_gb = Vec::with_capacity(n);

    let props_table = rustty_unicode::props_table::table();
    let symbols_table = rustty_unicode::symbols_table::table();

    for cp in 0..=MAX_CP {
        let p = props_table.get(cp);
        let sym = symbols_table.get(cp);

        width.push(p.width);
        symbol.push(u8::from(sym));
        // bit0..1 width(u2), bit2 width_zero_in_grapheme, bit3 emoji_vs_base
        let flags: u8 = (p.width & 0x3)
            | (u8::from(p.width_zero_in_grapheme) << 2)
            | (u8::from(p.emoji_vs_base) << 3);
        props_flags.push(flags);
        props_gb.push(p.grapheme_break as u8);
    }

    let grapheme = rustty_unicode::grapheme::harness_precompute_bytes();

    write(&out_dir, "width.bin", &width);
    write(&out_dir, "symbol.bin", &symbol);
    write(&out_dir, "props_flags.bin", &props_flags);
    write(&out_dir, "props_gb.bin", &props_gb);
    write(&out_dir, "grapheme.bin", &grapheme);
}

fn write(dir: &str, name: &str, bytes: &[u8]) {
    let path = std::path::Path::new(dir).join(name);
    let mut f = std::fs::File::create(&path).expect("create file");
    f.write_all(bytes).expect("write bytes");
    f.flush().expect("flush");
}
