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

Source: the Unicode Character Database, © Unicode, Inc. Distributed under the Unicode
Data Files license (see https://www.unicode.org/license.txt). Only the six files above
are vendored — the other UCD files uucode ships (`CaseFolding`, `SpecialCasing`, `Blocks`,
`BidiBrackets`) are not read by the vaxis width/grapheme subset and are intentionally omitted.
