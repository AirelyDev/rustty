#!/usr/bin/env python3
"""diff_unicode.py — rustty-unicode differential diff (ORACLE #2, G4-class).

Diffs rustty's embedded unicode tables (dumped by the
`cargo run --example dump_tables` Rust dumper) against the INDEPENDENT live
uucode oracle (dumped by the Zig `uucode-oracle`, built against uucode 0.2.0 @ 9d555245 +
ghostty's uucode_config.zig). This is the faithful full-range reproduction of
ghostty's `props_uucode.zig` / `symbols_uucode.zig` `test "... tables match
uucode"` blocks, whose live external oracle R3 forbids in-crate.

Asserts 0 diffs over EVERY code point 0..=0x10FFFF for:
  * width                     (width.bin)
  * is_symbol                 (symbol.bin)
  * Properties flags          (props_flags.bin: width, wcwidth_zero_in_grapheme,
                               is_emoji_vs_base packed per cp)
  * grapheme_break_no_control (props_gb.bin)
  * the 8192-entry grapheme precompute (grapheme.bin)

Exit 0 iff every field is byte-identical with the right length; non-zero +
per-field diff report otherwise. The gate is NEVER weakened (R5).

Usage: diff_unicode.py <oracle-dir> <rust-dir>
"""
import sys
import os

MAX_CP = 0x10FFFF
FULL_LEN = MAX_CP + 1  # 1114112
GRAPHEME_LEN = 8192

# (filename, expected length, human label, per-cp decoder for diff context)
FIELDS = [
    ("width.bin", FULL_LEN, "width"),
    ("symbol.bin", FULL_LEN, "is_symbol"),
    ("props_flags.bin", FULL_LEN, "props_flags(width|wzg|evb)"),
    ("props_gb.bin", FULL_LEN, "grapheme_break_no_control"),
    ("grapheme.bin", GRAPHEME_LEN, "grapheme_precompute(8192)"),
]


def read_bytes(path):
    with open(path, "rb") as fh:
        return fh.read()


def main():
    if len(sys.argv) != 3:
        print("usage: diff_unicode.py <oracle-dir> <rust-dir>", file=sys.stderr)
        return 2
    oracle_dir, rust_dir = sys.argv[1], sys.argv[2]

    total_diffs = 0
    report = []
    for name, exp_len, label in FIELDS:
        op = os.path.join(oracle_dir, name)
        rp = os.path.join(rust_dir, name)
        if not os.path.exists(op):
            report.append(f"MISSING oracle file: {op}")
            total_diffs += 1
            continue
        if not os.path.exists(rp):
            report.append(f"MISSING rust file: {rp}")
            total_diffs += 1
            continue
        ob = read_bytes(op)
        rb = read_bytes(rp)
        if len(ob) != exp_len:
            report.append(f"BAD LEN oracle {name}: {len(ob)} != {exp_len}")
            total_diffs += 1
        if len(rb) != exp_len:
            report.append(f"BAD LEN rust {name}: {len(rb)} != {exp_len}")
            total_diffs += 1
        n = min(len(ob), len(rb))
        field_diffs = 0
        first = []
        for i in range(n):
            if ob[i] != rb[i]:
                field_diffs += 1
                if len(first) < 10:
                    if name == "grapheme.bin":
                        first.append(f"key={i} oracle={ob[i]} rust={rb[i]}")
                    else:
                        first.append(f"U+{i:04X} oracle={ob[i]} rust={rb[i]}")
        total_diffs += field_diffs
        if field_diffs == 0 and len(ob) == exp_len and len(rb) == exp_len:
            report.append(f"OK    {label:32s} 0 diffs ({n} entries)")
        else:
            report.append(f"DIFF  {label:32s} {field_diffs} diffs")
            for d in first:
                report.append(f"        {d}")

    print("=== rustty-unicode differential (ORACLE #2): rustty tables vs live uucode ===")
    for line in report:
        print(line)

    if total_diffs == 0:
        print("RESULT: 0 diffs across all fields over 0..=0x10FFFF + 8192 grapheme entries")
        return 0
    print(f"RESULT: {total_diffs} diff(s) — BLOCKER (R5: gate not weakened)", file=sys.stderr)
    return 1


if __name__ == "__main__":
    sys.exit(main())
