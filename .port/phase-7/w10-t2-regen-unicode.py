#!/usr/bin/env python3
"""w10-t2-regen-unicode.py -- regenerate rustty-unicode's four generated tables
for uucode 9d555245 (Unicode 18, ghostty 12542b392, P7-W10 T2).

Inputs (none of them built inside the rustty tree, R3):

  props.zig / symbols.zig  stdout of the reference's own `props-unigen` /
                           `symbols-unigen` (ghostty@12752b2ac
                           src/unicode/{props,symbols}_uucode.zig + lut.zig +
                           props.zig), built as src/build/UnicodeTables.zig
                           builds them: uucode 9d555245 with
                           `build_config_path` = that pin's uucode_config.zig.
                           Build them in a scratch directory outside both
                           trees (stub `props_table.zig` / `symbols_table.zig`
                           so the test blocks' imports resolve).
  grapheme.bin             the G4-uni oracle's 8192-entry precompute
                           (`.port/phase-0/harness/unicode/zig-out/bin/uucode-oracle <dir>`),
                           i.e. uucode's `computeGraphemeBreakNoControl` over
                           every (state, gb1, gb2) key, as ghostty's
                           src/unicode/grapheme.zig embeds it at comptime.
  CaseFolding.txt          `ucd/CaseFolding.txt` of the uucode 9d555245 package.

Usage:
  w10-t2-regen-unicode.py <props.zig> <symbols.zig> <grapheme.bin> \
      <CaseFolding.txt> <out-dir>

Writes {props,symbols,grapheme,case_folding}_data.rs into <out-dir>; run
`rustfmt --edition 2021` on them afterwards. G4-uni
(`run_unicode_diff.sh`) then re-proves props/symbols/grapheme against the live
oracle, and `generated/checksum.rs` is re-pinned in the same commit.
"""
import hashlib
import re
import sys

UUCODE = "9d55524551411b493cca41ca06363625d90aff1e"
PIN = "12752b2ac"

GB = {
    "other": "Other",
    "prepend": "Prepend",
    "regional_indicator": "RegionalIndicator",
    "spacing_mark": "SpacingMark",
    "l": "L",
    "v": "V",
    "t": "T",
    "lv": "Lv",
    "lvt": "Lvt",
    "zwj": "Zwj",
    "zwnj": "Zwnj",
    "extended_pictographic": "ExtendedPictographic",
    "emoji_modifier_base": "EmojiModifierBase",
    "emoji_modifier": "EmojiModifier",
    "indic_conjunct_break_extend": "IndicConjunctBreakExtend",
    "indic_conjunct_break_linker_extend": "IndicConjunctBreakLinkerExtend",
    "indic_conjunct_break_linker_other": "IndicConjunctBreakLinkerOther",
    "indic_conjunct_break_consonant": "IndicConjunctBreakConsonant",
}

REGEN_NOTE = f"""//! Re-generated on 2026-09-28 (P7-W10 T2) for ghostty 12542b392 "deps: Update
//! uucode for Unicode 18" at the reference pin {PIN}, i.e. uucode 0.2.0 @
//! 9d555245 (UCD 18.0.0) with that pin's `uucode_config.zig`
//! (`.port/phase-7/w10-t2-regen-unicode.py`)."""


def stage(text, name):
    m = re.search(r"pub const %s: \[(\d+)\](\w+) = \.\{(.*?)\};" % name, text, re.S)
    if not m:
        sys.exit(f"no {name}")
    return int(m.group(1)), m.group(2), m.group(3)


def ints(body, n):
    vals = [int(v) for v in body.replace("\n", "").split(",") if v.strip()]
    assert len(vals) == n, (len(vals), n)
    return vals


def arr(name, ty, vals):
    return f"pub const {name}: [{ty}; {len(vals)}] = [\n" + "".join(
        f"    {v},\n" for v in vals
    ) + "];\n"


def props(text):
    n1, _, b1 = stage(text, "stage1")
    n2, _, b2 = stage(text, "stage2")
    n3, _, b3 = stage(text, "stage3")
    s1, s2 = ints(b1, n1), ints(b2, n2)
    elems = re.findall(
        r"\.\{\s*\.width= (\d+),\s*\.width_zero_in_grapheme= (\w+),\s*"
        r"\.grapheme_break= \.(\w+),\s*\.emoji_vs_base= (\w+),\s*\}",
        b3,
    )
    assert len(elems) == n3, (len(elems), n3)
    out = f"""//! This file is auto-generated. Do not edit.
//!
//! Generated bit-for-bit from the reference `props-unigen` (ghostty
//! src/unicode/props_uucode.zig) built against uucode 0.2.0 with
//! ghostty's src/build/uucode_config.zig. The Zig generator emits
//! `stage1`/`stage2`/`stage3` constants; this is the Rust transliteration
//! of those exact values (R1: comptime-generated artifacts reproduced
//! bit-for-bit).
//!
{REGEN_NOTE}

// PARITY: the reference emits these as `pub const stageN` arrays (Zig comptime
// constants). Clippy's `large_const_arrays` would have us switch to `static`,
// which would change the artifact shape away from the reference's const tables;
// we keep `const` to preserve parity (R5: never change a value/shape to silence
// a lint).
#![allow(clippy::large_const_arrays)]

use crate::props::GraphemeBreakNoControl;
use crate::props::Properties;

"""
    out += arr("STAGE1", "u16", s1)
    out += arr("STAGE2", "u16", s2)
    out += f"pub const STAGE3: [Properties; {n3}] = [\n"
    for w, wzg, gb, evb in elems:
        out += (
            "    Properties {\n"
            f"        width: {w},\n"
            f"        width_zero_in_grapheme: {wzg},\n"
            f"        grapheme_break: GraphemeBreakNoControl::{GB[gb]},\n"
            f"        emoji_vs_base: {evb},\n"
            "    },\n"
        )
    out += "];\n"
    return out


def symbols(text):
    n1, _, b1 = stage(text, "stage1")
    n2, _, b2 = stage(text, "stage2")
    n3, _, b3 = stage(text, "stage3")
    s3 = [v.strip() for v in b3.split(",") if v.strip()]
    assert len(s3) == n3
    out = f"""//! This file is auto-generated. Do not edit.
//!
//! Generated bit-for-bit from the reference `symbols-unigen` (ghostty
//! src/unicode/symbols_uucode.zig) built against uucode 0.2.0 with
//! ghostty's src/build/uucode_config.zig.
//!
{REGEN_NOTE}

"""
    out += arr("STAGE1", "u16", ints(b1, n1))
    out += arr("STAGE2", "u16", ints(b2, n2))
    out += f"pub const STAGE3: [bool; {n3}] = [{', '.join(s3)}];\n"
    return out


def grapheme(data):
    assert len(data) == 8192
    out = f"""//! This file is auto-generated. Do not edit.
//!
//! Precomputed grapheme-break lookup table generated bit-for-bit from the
//! reference algorithm (uucode 0.2.0 @ 9d555245
//! `uucode.grapheme.computeGraphemeBreakNoControl`), the same logic ghostty's
//! src/unicode/grapheme.zig embeds at comptime. Each entry packs a
//! `Value{{ result: bool, state: BreakState }}` into the low 4 bits: bit 0 is
//! `result`, bits 1..=3 are the `BreakState` table index (`after_linker` in
//! bit 1, `base` in bits 2..=3). Keys whose state is not a table row
//! (`BreakState.table_len` = 5) or whose break property is past the enum's
//! last field are zero.
//!
{REGEN_NOTE}

"""
    out += arr("DATA", "u8", list(data))
    return out


def case_folding(path):
    raw = open(path, "rb").read()
    sha = hashlib.sha256(raw).hexdigest()
    lines = raw.decode("utf-8").splitlines()
    header = lines[0].lstrip("# ").strip()
    c, f = {}, {}
    for line in lines:
        line = line.split("#", 1)[0].strip()
        if not line:
            continue
        cp, status, mapping = [p.strip() for p in line.split(";")[:3]]
        cp = int(cp, 16)
        mapping = [int(m, 16) for m in mapping.split()]
        if status == "C":
            c[cp] = mapping
        elif status == "F":
            f[cp] = mapping
    full = {}
    for cp, m in c.items():
        full[cp] = m
    for cp, m in f.items():
        full[cp] = m
    n_c = sum(1 for cp in full if cp not in f)
    out = f"""//! `case_folding_full`: Unicode full case folding, the uucode field that
//! ghostty's `runtime` table declares (`src/build/uucode_config.zig:31-38`)
//! and `input/Binding.zig` `foldedCodepoint` reads.
//!
//! GENERATED from `ucd/CaseFolding.txt` of the pinned uucode package
//! (`build.zig.zon`: uucode {UUCODE},
//! `{header}`, sha256
//! {sha}),
//! following uucode's `CaseFoldingFull.build`: a code point's status-F
//! mapping if it has one, else its status-C mapping. Code points with
//! neither are absent (they fold to themselves). Statuses S and T are not
//! part of full folding.
//! {n_c} C entries and {len(f)} F entries, {len(full)} code points.

/// `(code point, mapping)` sorted by code point; unused mapping slots are 0.
#[rustfmt::skip]
pub const CASE_FOLDING_FULL: &[(u32, [u32; 3])] = &[
"""
    for cp in sorted(full):
        m = full[cp] + [0] * (3 - len(full[cp]))
        out += f"    (0x{cp:04X}, [{', '.join(f'0x{v:04X}' for v in m)}]),\n"
    out += "];\n"
    return out


def main():
    if len(sys.argv) != 6:
        sys.exit(__doc__)
    props_zig, symbols_zig, grapheme_bin, casefolding, out = sys.argv[1:]
    files = {
        "props_data.rs": props(open(props_zig).read()),
        "symbols_data.rs": symbols(open(symbols_zig).read()),
        "grapheme_data.rs": grapheme(open(grapheme_bin, "rb").read()),
        "case_folding_data.rs": case_folding(casefolding),
    }
    for name, text in files.items():
        with open(f"{out}/{name}", "w") as fh:
            fh.write(text)
        print(f"wrote {out}/{name}")


if __name__ == "__main__":
    main()
