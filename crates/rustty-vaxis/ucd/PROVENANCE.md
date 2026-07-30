# Vendored UCD 16.0.0 corpus (R3 provenance)

These files are **byte-for-byte copies** of the Unicode Character Database corpus
shipped inside `uucode` 0.1.0 (git `5f05f8f…`, Zig package hash
`ZZjBPj96QADXyt5sqwBJUnhaDYs_qBeeKijZvlRa0eqM`), under its `ucd/` directory. They are
vendored here so that `build.rs` can reproduce uucode's stage1/stage2 width/grapheme
tables **bit-faithfully without a Zig toolchain** (VAXIS_C1B_DESIGN.md §3).

`uucode` 0.1.0 targets **Unicode 16.0.0** (file headers: `DerivedEastAsianWidth-16.0.0`,
`GraphemeBreakProperty-16.0.0`, `GraphemeBreakTest-16.0.0`, `emoji-data` dated 2024-05-01).
`UnicodeData.txt` carries no version header but is the 16.0.0 file shipped alongside.

| file | parsed by `build.rs` for | origin (in uucode 0.1.0 `ucd/`) |
|---|---|---|
| `UnicodeData.txt` | `general_category` (+ First/Last range fill) | `ucd/UnicodeData.txt` |
| `extracted/DerivedEastAsianWidth.txt` | `east_asian_width` (+ `@missing Wide`) | `ucd/extracted/DerivedEastAsianWidth.txt` |
| `emoji/emoji-data.txt` | `is_emoji_presentation`, `is_extended_pictographic` | `ucd/emoji/emoji-data.txt` |
| `auxiliary/GraphemeBreakProperty.txt` | `original_grapheme_break` | `ucd/auxiliary/GraphemeBreakProperty.txt` |
| `DerivedCoreProperties.txt` | `indic_conjunct_break` (InCB only) | `ucd/DerivedCoreProperties.txt` |
| `auxiliary/GraphemeBreakTest.txt` | UAX #29 conformance test fixture | `ucd/auxiliary/GraphemeBreakTest.txt` |

## SHA-256 digests (WS12 T4)

Recorded so the vendored corpus is TAMPER-EVIDENT: `build.rs` derives the entire
width/grapheme table from these six files, so a silent edit to any of them silently
changes `rustty-vaxis`'s Unicode behaviour. `tests/ucd_provenance.rs` parses this exact
table and asserts every digest against the on-disk bytes; it is a real gate, negative-
controlled by a one-byte perturbation (see `.port/phase-2/VAXIS_C1B_DESIGN.md`,
"WS12 T4 non-vacuity proof"). Regenerate with:

```
shasum -a 256 crates/rustty-vaxis/ucd/{UnicodeData.txt,extracted/DerivedEastAsianWidth.txt,emoji/emoji-data.txt,auxiliary/GraphemeBreakProperty.txt,auxiliary/GraphemeBreakTest.txt,DerivedCoreProperties.txt}
```

Unicode version for all six: **16.0.0**. Retrieval: vendored from `uucode` 0.1.0's own
`ucd/` directory (Zig package `ZZjBPj96QADXyt5sqwBJUnhaDYs_qBeeKijZvlRa0eqM`), which in
turn took them from `https://www.unicode.org/Public/16.0.0/ucd/<path>` (the `emoji/` and
`auxiliary/` and `extracted/` sub-paths are the same under that root).

| sha256 | file |
|---|---|
| `ff58e5823bd095166564a006e47d111130813dcf8bf234ef79fa51a870edb48f` | `UnicodeData.txt` |
| `372e34de237e5a644ce8379a0d02b91a1f584f04d4fb240ea6ccc87c5767f926` | `extracted/DerivedEastAsianWidth.txt` |
| `f1365a5173eee18e1f98b240cdc492e84a25f1ce7e0c9d1094eb29c41a22696a` | `emoji/emoji-data.txt` |
| `c29360bd6f7132811d701d29069541e827eb44bfc4c8fbde8c370d6982689dc1` | `auxiliary/GraphemeBreakProperty.txt` |
| `ee2b9354d270ac061b29f09662cafea06341d77e704b8cc6bd72aaeeda363cb5` | `auxiliary/GraphemeBreakTest.txt` |
| `39d35161f2954497f69e08bdb9e701493f476a3d30222de20028feda36c1dabd` | `DerivedCoreProperties.txt` |

Source: the Unicode Character Database, © Unicode, Inc. Distributed under the Unicode
Data Files license (see https://www.unicode.org/license.txt). Only the six files above
are vendored — the other UCD files uucode ships (`CaseFolding`, `SpecialCasing`, `Blocks`,
`BidiBrackets`) are not read by the vaxis width/grapheme subset and are intentionally omitted.
