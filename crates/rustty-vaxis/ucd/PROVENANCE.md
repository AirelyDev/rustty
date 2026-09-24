# Vendored UCD 17.0.0 corpus (R3 provenance)

These files are **byte-for-byte copies** of the Unicode Character Database corpus
shipped inside `uucode` **2826a37a** (git `2826a37a4562284fdacd8fa029d49509cc9bffcd`,
Zig package hash `uucode-0.2.0-ZZjBPlK5VADj7fdoq7G8LIHzD5o6FSkcBXXrRWr4jnrA`), under its
`ucd/` directory. That is the uucode revision the reference pin (ghostty `20abdb50a`,
`build.zig.zon`) pins, and the one it wires into libvaxis 0.6.0 (`external_uucode = true`,
`src/build/SharedDeps.zig:24-78, 512-519`). They are vendored here so that `build.rs` can
reproduce uucode's per-code-point values for the four fields vaxis reads
(`east_asian_width`, `general_category`, `grapheme_break`, `is_emoji_presentation`)
**without a Zig toolchain** (VAXIS_C1B_DESIGN.md §3).

`uucode` 2826a37a targets **Unicode 17.0.0** (file headers: `DerivedEastAsianWidth-17.0.0`,
`GraphemeBreakProperty-17.0.0`, `GraphemeBreakTest-17.0.0`, `DerivedCoreProperties-17.0.0`,
`emoji-data.txt` `Version: 17.0`, dated 2025-07-25). `UnicodeData.txt` carries no version
header but is the 17.0.0 file shipped alongside. It is the same tarball the G4-uni harness
(`.port/phase-0/harness/unicode/`) fetches.

HISTORY: until parity tail S-10b (2026-09-23, `NON_PORTS.md` §G2, `NEW-FINDINGS.md` N240)
this directory held the **16.0.0** corpus of `uucode` 0.1.0 (git `5f05f8f…`, package
`ZZjBPj96QADXyt5sqwBJUnhaDYs_qBeeKijZvlRa0eqM`), which libvaxis 0.5.1 pinned. At the
reference pin that revision is not used by anything.

| file                                  | parsed by `build.rs` for                                                                                          | origin (in uucode 2826a37a `ucd/`)        |
| ------------------------------------- | ----------------------------------------------------------------------------------------------------------------- | ----------------------------------------- |
| `UnicodeData.txt`                     | `general_category` (+ First/Last range fill)                                                                      | `ucd/UnicodeData.txt`                     |
| `extracted/DerivedEastAsianWidth.txt` | `east_asian_width` (+ `@missing Wide`)                                                                            | `ucd/extracted/DerivedEastAsianWidth.txt` |
| `emoji/emoji-data.txt`                | `is_emoji_presentation`, `is_extended_pictographic`, `is_emoji_modifier`, `is_emoji_modifier_base`, `is_emoji_component` | `ucd/emoji/emoji-data.txt`                |
| `auxiliary/GraphemeBreakProperty.txt` | `original_grapheme_break`                                                                                         | `ucd/auxiliary/GraphemeBreakProperty.txt` |
| `DerivedCoreProperties.txt`           | `indic_conjunct_break` (InCB only)                                                                                | `ucd/DerivedCoreProperties.txt`           |
| `auxiliary/GraphemeBreakTest.txt`     | UAX #29 conformance test fixture                                                                                  | `ucd/auxiliary/GraphemeBreakTest.txt`     |

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

Unicode version for all six: **17.0.0**. Retrieval: copied from `uucode` 2826a37a's own
`ucd/` directory (Zig package `uucode-0.2.0-ZZjBPlK5VADj7fdoq7G8LIHzD5o6FSkcBXXrRWr4jnrA`,
the tarball ghostty mirrors at
`https://deps.files.ghostty.org/uucode-2826a37a4562284fdacd8fa029d49509cc9bffcd.tar.gz`), which
in turn took them from `https://www.unicode.org/Public/17.0.0/ucd/<path>` (the `emoji/` and
`auxiliary/` and `extracted/` sub-paths are the same under that root).

| sha256                                                             | file                                  |
| ------------------------------------------------------------------ | ------------------------------------- |
| `2e1efc1dcb59c575eedf5ccae60f95229f706ee6d031835247d843c11d96470c` | `UnicodeData.txt`                     |
| `0b5523a2217cb318d20b329a05d31eec5af5686ba09d263b85bb75a28989a3a8` | `extracted/DerivedEastAsianWidth.txt` |
| `2cb2bb9455cda83e8481541ecf5b6dfda66a3bb89efa3fa7c5297eccf607b72b` | `emoji/emoji-data.txt`                |
| `d6b51d1d2ae5c33b451b7ed994b48f1f4dc62b2272a5831e7fd418514a6bae89` | `auxiliary/GraphemeBreakProperty.txt` |
| `e2d134d2c52919bace503ebb6a551c1855fe1a1faec18478c78fff254a1793ec` | `auxiliary/GraphemeBreakTest.txt`     |
| `24c7fed1195c482faaefd5c1e7eb821c5ee1fb6de07ecdbaa64b56a99da22c08` | `DerivedCoreProperties.txt`           |

Source: the Unicode Character Database, © Unicode, Inc. Distributed under the Unicode
Data Files license (see https://www.unicode.org/license.txt). Only the six files above
are vendored — the other UCD files uucode ships (`CaseFolding`, `SpecialCasing`, `Blocks`,
`BidiBrackets`, `BidiMirroring`, `Scripts`, the Indic category files, …) are not read by the
vaxis width/grapheme subset and are intentionally omitted.
