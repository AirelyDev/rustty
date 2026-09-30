# Vendored UCD 18.0.0 corpus (R3 provenance)

These files are **byte-for-byte copies** of the Unicode Character Database corpus
shipped inside `uucode` **9d555245** (git `9d55524551411b493cca41ca06363625d90aff1e`,
Zig package hash `uucode-0.2.0-ZZjBPuuFVgC8YZ8eld4fOKsZANLIhTFMzULQxhkLi1C7`), under its
`ucd/` directory. That is the uucode revision the reference pin (ghostty `12752b2ac`,
`build.zig.zon`, bumped by `12542b392` "deps: Update uucode for Unicode 18") pins, and the one it wires into libvaxis 0.6.0 (`external_uucode = true`,
`src/build/SharedDeps.zig:24-78, 512-519`). They are vendored here so that `build.rs` can
reproduce uucode's per-code-point values for the four fields vaxis reads
(`east_asian_width`, `general_category`, `grapheme_break`, `is_emoji_presentation`)
**without a Zig toolchain** (VAXIS_C1B_DESIGN.md §3).

`uucode` 9d555245 targets **Unicode 18.0.0** (file headers: `DerivedEastAsianWidth-18.0.0`,
`GraphemeBreakProperty-18.0.0`, `GraphemeBreakTest-18.0.0`, `DerivedCoreProperties-18.0.0`,
`emoji-data.txt` `Version: 18.0.0`, dated 2026-01-30). `UnicodeData.txt` carries no version
header but is the 18.0.0 file shipped alongside. It is the same tarball the G4-uni harness
(`.port/phase-0/harness/unicode/`) fetches.

HISTORY: P7-W10 T2 (2026-09-28) re-vendored all six files from `uucode` 9d555245 (18.0.0);
before that they were the **17.0.0** corpus of `uucode` 2826a37a (package
`ZZjBPlK5VADj7fdoq7G8LIHzD5o6FSkcBXXrRWr4jnrA`), which the reference pin `20abdb50a` used.
Until parity tail S-10b (2026-09-23, `NON_PORTS.md` §G2, `NEW-FINDINGS.md` N240)
this directory held the **16.0.0** corpus of `uucode` 0.1.0 (git `5f05f8f…`, package
`ZZjBPj96QADXyt5sqwBJUnhaDYs_qBeeKijZvlRa0eqM`), which libvaxis 0.5.1 pinned. At the
reference pin that revision is not used by anything.

| file                                  | parsed by `build.rs` for                                                                                                 | origin (in uucode 9d555245 `ucd/`)        |
| ------------------------------------- | ------------------------------------------------------------------------------------------------------------------------ | ----------------------------------------- |
| `UnicodeData.txt`                     | `general_category` (+ First/Last range fill)                                                                             | `ucd/UnicodeData.txt`                     |
| `extracted/DerivedEastAsianWidth.txt` | `east_asian_width` (+ `@missing Wide`)                                                                                   | `ucd/extracted/DerivedEastAsianWidth.txt` |
| `emoji/emoji-data.txt`                | `is_emoji_presentation`, `is_extended_pictographic`, `is_emoji_modifier`, `is_emoji_modifier_base`, `is_emoji_component` | `ucd/emoji/emoji-data.txt`                |
| `auxiliary/GraphemeBreakProperty.txt` | `original_grapheme_break`                                                                                                | `ucd/auxiliary/GraphemeBreakProperty.txt` |
| `DerivedCoreProperties.txt`           | `indic_conjunct_break` (InCB only)                                                                                       | `ucd/DerivedCoreProperties.txt`           |
| `auxiliary/GraphemeBreakTest.txt`     | UAX #29 conformance test fixture                                                                                         | `ucd/auxiliary/GraphemeBreakTest.txt`     |

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

Unicode version for all six: **18.0.0**. Retrieval: copied from `uucode` 9d555245's own
`ucd/` directory (Zig package `uucode-0.2.0-ZZjBPuuFVgC8YZ8eld4fOKsZANLIhTFMzULQxhkLi1C7`,
the tarball ghostty's `build.zig.zon` fetches from
`https://github.com/jacobsandlund/uucode/archive/9d55524551411b493cca41ca06363625d90aff1e.tar.gz`), which
in turn took them from `https://www.unicode.org/Public/18.0.0/ucd/<path>` (the `emoji/` and
`auxiliary/` and `extracted/` sub-paths are the same under that root).

| sha256                                                             | file                                  |
| ------------------------------------------------------------------ | ------------------------------------- |
| `0736451de439ae7baf1425136617da495e09ee5afbe6e394374db7009ea08950` | `UnicodeData.txt`                     |
| `f4c7bc4537a71e12452f6347ecbbc71c31cba21b5190c92be1ae296b0ef32b09` | `extracted/DerivedEastAsianWidth.txt` |
| `80d00f8e616a0ef27fd6b8de3b758c06383b5d917e2977709578e68baf733bf1` | `emoji/emoji-data.txt`                |
| `0839dcb79e4ac639ecd538b1abf7c9d22e3f9dd265b7e182d33627aa4d75b45a` | `auxiliary/GraphemeBreakProperty.txt` |
| `b0cf047ee94485bbdc846de2b902f5f8a815f6b674f9d04223cddadd91c9df31` | `auxiliary/GraphemeBreakTest.txt`     |
| `09c928886a178fcafd93c29e4bd59073a058e5a100b716d425cb563ab50f68c9` | `DerivedCoreProperties.txt`           |

Source: the Unicode Character Database, © Unicode, Inc. Distributed under the Unicode
Data Files license (see https://www.unicode.org/license.txt). Only the six files above
are vendored — the other UCD files uucode ships (`CaseFolding`, `SpecialCasing`, `Blocks`,
`BidiBrackets`, `BidiMirroring`, `Scripts`, the Indic category files, …) are not read by the
vaxis width/grapheme subset and are intentionally omitted.
