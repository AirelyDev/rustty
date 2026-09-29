//! Harness dumper for the rustty-vaxis uucode differential
//! (`.port/phase-7/w10-t2-vaxis-diff/run.sh`, the S-10b recipe re-run for
//! uucode 9d555245 by P7-W10 T2).
//!
//! Writes, in the layout the Zig probe (`probe.zig` there) emits:
//!   rust_fields.bin    4 bytes per code point 0..=0x10FFFF: east_asian_width,
//!                      general_category, grapheme_break (enum ints),
//!                      is_emoji_presentation (0/1)
//!   rust_gb_table.bin  `compute_grapheme_break` over every
//!                      (state row, gb1, gb2), key = row*COUNT^2 + gb1*COUNT + gb2,
//!                      value = result | state.table_index()<<1
//!
//! Harness-only glue (R1 adds nothing-lost): it reads only the crate's own tables
//! and algorithm.
//!
//! Usage: cargo run --example dump_uucode -p rustty-vaxis -- <out-dir>

use rustty_vaxis::uucode::{get, grapheme, types::GraphemeBreak};

fn main() {
    let out = std::env::args().nth(1).unwrap_or_else(|| {
        eprintln!("usage: dump_uucode <out-dir>");
        std::process::exit(2);
    });
    std::fs::create_dir_all(&out).expect("create out dir");

    let mut buf = Vec::with_capacity(0x110000 * 4);
    for cp in 0..0x110000u32 {
        buf.push(get::east_asian_width(cp) as u8);
        buf.push(get::general_category(cp) as u8);
        buf.push(get::grapheme_break(cp) as u8);
        buf.push(u8::from(get::is_emoji_presentation(cp)));
    }
    std::fs::write(format!("{out}/rust_fields.bin"), &buf).expect("write fields");

    let n = GraphemeBreak::COUNT;
    let mut tbl = vec![0u8; grapheme::BreakState::TABLE_LEN * n * n];
    for si in 0..grapheme::BreakState::TABLE_LEN {
        for g1 in 0..n {
            for g2 in 0..n {
                let mut st = grapheme::BreakState::from_table_index(si);
                let r = grapheme::compute_grapheme_break(
                    GraphemeBreak::from_u8(g1 as u8),
                    GraphemeBreak::from_u8(g2 as u8),
                    &mut st,
                );
                tbl[si * n * n + g1 * n + g2] = u8::from(r) | ((st.table_index() as u8) << 1);
            }
        }
    }
    std::fs::write(format!("{out}/rust_gb_table.bin"), &tbl).expect("write table");
}
