# VAXIS Chunk 1b — gwidth / unicode + the vendored uucode-0.1.0 subset — DESIGN

**Crate:** existing `crates/rustty-vaxis` (Chunk 1a value types already landed; this
chunk adds the width/grapheme surface). Ports the VENDORED dep `libvaxis` 0.5.1
(rockorager, hash `BWNV_LosCQAGmCCNOLljCIw6j6-yt53tji6n6rwJ2BhS`).

**Sources (read-only):**
- vaxis: `…/vaxis-0.5.1-…/src/gwidth.zig` (216 LOC, 11 test) + `src/unicode.zig` (64 LOC).
- uucode 0.1.0 (git `5f05f8f…`, hash `…ZZjBPj96QADXyt5sqwBJUnhaDYs_qBeeKijZvlRa0eqM`):
  `src/{root,get,types,grapheme,utf8,config}.zig` + `src/build/{Ucd,tables}.zig`
  + the `ucd/` UCD 16.0.0 text corpus.

**uucode DECISION (given):** Option **B** — vendor the uucode 0.1.0 runtime subset as a
**module inside rustty-vaxis** at `crates/rustty-vaxis/src/uucode/`. NOT rustty-unicode
(its 0.2.0 Ghostty-stripped config lacks `east_asian_width` / `general_category` /
`is_emoji_presentation` raw fields and would introduce version skew).

---

## DISPOSITION: **READY**

The table generator is fully tractable **without a Zig toolchain**. vaxis pins a
_trivially small_ uucode table (4 basic fields, resolving to a **2-stage, unpacked**
table — §3), the packing algorithm in `build/tables.zig` is a straightforward
256-block dedup we reproduce in a `build.rs` from the vendored UCD 16.0.0 files, and
the result is verifiable against Unicode's own conformance corpus (`GraphemeBreakTest.txt`,
1093 cases) plus vaxis's 11 gwidth tests — **all pinned, no Zig required**. The 11
gwidth codepoints were hand-traced against the vendored UCD data (§8) and resolve
correctly. Proceed with the PREFERRED build.rs-from-UCD path.

**Fold-phase adversary fixes applied (no blocker):** (1) the `@missing`-Wide path is now pinned by
added property-anchor width tests — assigned ideographs AND @missing-only unassigned holes (§8.2,
§10.2a); investigation found the extracted file *also* has explicit `W` rows for the assigned
ideographs, so the isolating pins are the holes `FA6E`/`2A6E0`/`3134B` (§3.5.1). (2) the
DerivedEastAsianWidth parser now detects `# @missing:` on the WHITESPACE-trimmed (not
comment-stripped) line, skips `Neutral`, panics on any non-`Wide` @missing (§3.5.1). (3) the
`OriginalGraphemeBreak → GraphemeBreak` derivation is made explicit + exhaustive with the four
build-time asserts as `panic!`s, and `GraphemeBreak(18)`/`BreakState(5)` discriminants are pinned to
the exact types.zig/grapheme.zig order (§3.4, §5, §7).

---

## 1. What vaxis actually consumes from uucode (the whole surface)

From `vaxis/build.zig`, vaxis instantiates uucode with **exactly one table, four fields**:

```zig
.fields = &.{ "east_asian_width", "grapheme_break", "general_category", "is_emoji_presentation" }
```

Runtime API touched by `gwidth.zig` + `unicode.zig`:

| uucode symbol | used by | notes |
|---|---|---|
| `uucode.get(.east_asian_width, cp)` → `types.EastAsianWidth` | gwidth `eawToWidth` | u3 enum |
| `uucode.get(.general_category, cp)` → `types.GeneralCategory` | gwidth `eawToWidth` | u5 enum |
| `uucode.get(.is_emoji_presentation, cp)` → `bool` | gwidth `.unicode` | bool |
| `uucode.get(.grapheme_break, cp)` → `types.GraphemeBreak` | grapheme `Iterator` | u5 enum (derived) |
| `uucode.types.EastAsianWidth` | gwidth signature | — |
| `uucode.grapheme.Iterator(uucode.utf8.Iterator)` | gwidth `.unicode`, unicode.zig | UAX #29 |
| `uucode.utf8.Iterator` | gwidth (all methods), unicode.zig | Höhrmann DFA |

**NOT used** (out of scope, do NOT port): `uucode.x.*`, `ascii.zig`, all the other
config fields (name/case-folding/bidi/decomposition/blocks/numeric/…), `getAll`,
`FieldEnum`, `TypeOf`, the generic comptime `get` dispatch, the `Slice`/`Shift`/
`Union`/`PackedOptional` field machinery. Our `get` is four concrete typed accessors.

---

## 2. Module mapping (Zig → rustty), under `crates/rustty-vaxis/src/`

| Zig source | rustty module | Tests |
|---|---|---|
| `uucode/src/utf8.zig` | `uucode/utf8.rs` | 3 (`ascii`, `emoji`, `overlong utf8`) |
| `uucode/src/types.zig` (subset) | `uucode/types.rs` | 0 |
| `uucode/src/grapheme.zig` | `uucode/grapheme.rs` | 4 (`next/peek`, `nextBreak`, GraphemeBreakTest ×2) |
| `uucode/src/get.zig` (subset) | `uucode/get.rs` | 0 |
| generated stage1/stage2 + `Data` + lookup | `uucode/tables.rs` (via `build.rs` → `OUT_DIR`) | 1 (exhaustive dedup self-check, added) |
| `uucode/build/{Ucd,tables}.zig` | `build.rs` (+ vendored `ucd/`) | `parseCp`/`parseRange` unit tests (ported) |
| `vaxis/src/gwidth.zig` | `gwidth.rs` | **11** |
| `vaxis/src/unicode.zig` | `unicode.rs` | 0 (Old-API wrapper; exercised via gwidth) |
| — | `uucode/mod.rs` (re-exports `types`,`get`,`grapheme`,`utf8`,`tables`) | 0 |

**Ported `test{}` total for Chunk 1b = 11 (gwidth) + 3 (utf8) + 4 (grapheme) = 18**,
plus the added exhaustive-dedup self-check, the `@missing`-Wide anchor tests (§8.2), and the
`parseCp/parseRange` build tests
(verification scaffolding, not part of the ghostty parity contract — rustty-vaxis is
a vendored dep-of-a-dep, ABSENT from the parity crate_map per Cargo.toml). Pins
`check_test_counts.py` for this crate at the ported subset.

---

## 3. table_approach (THE CRUX) — build.rs from vendored UCD 16.0.0

### 3.1 Unicode version + vendored files

uucode 0.1.0 ships **UCD 16.0.0** (headers: `DerivedCoreProperties-16.0.0`,
`DerivedEastAsianWidth-16.0.0`, `GraphemeBreakProperty-16.0.0`, `emoji-data` 2024-05-01,
`GraphemeBreakTest-16.0.0`). `UnicodeData.txt` carries no version header but is the
16.0.0 file shipped alongside. **Vendor the exact 6 files** uucode's generator reads,
in-tree at `crates/rustty-vaxis/ucd/` (byte-copies of uucode's `ucd/`, R3 provenance
noted in a `ucd/PROVENANCE.md`):

| file | parsed for | stored at runtime? |
|---|---|---|
| `UnicodeData.txt` (2.1M) | `general_category` (+ First/Last range fill) | **yes** (gc) |
| `extracted/DerivedEastAsianWidth.txt` (185K) | `east_asian_width` (+ `@missing Wide`) | **yes** (eaw) |
| `emoji/emoji-data.txt` (113K) | `is_emoji_presentation`, `is_extended_pictographic` | **yes** (emoji_pres) + intermediate |
| `auxiliary/GraphemeBreakProperty.txt` (99K) | `original_grapheme_break` | intermediate |
| `DerivedCoreProperties.txt` (1.1M) | `indic_conjunct_break` (InCB only) | intermediate |
| `auxiliary/GraphemeBreakTest.txt` (172K) | conformance test fixture | test-only |

Only `general_category`, `east_asian_width`, `is_emoji_presentation` land in the
runtime `Data`. `grapheme_break` is **derived** (§3.4) from three intermediates —
`original_grapheme_break`, `indic_conjunct_break`, `is_extended_pictographic` — which
is why the emoji/DCP files must be parsed even though those raw fields are not stored.

### 3.2 Why 2-stage, unpacked (resolve() traced)

The runtime `Data` = `{ east_asian_width: EastAsianWidth(u3), grapheme_break:
GraphemeBreak(u5), general_category: GeneralCategory(u5), is_emoji_presentation: bool }`.
`config.Table.resolve()` (config.zig) computes:
- `can_be_packed = true` (all four are packable ints/enums/bool).
- `unpacked_size = @sizeOf(Data.unpacked) = 4` (four 1-byte fields, `.auto` layout).
- `packed_size = @sizeOf(Data.packed) = 2` (3+5+5+1 = 14 bits → u16).
- `two_stage_size_threshold = 4`; `min_size = 2 ≤ 4` → **stages = .two**.
- packing: `unpacked_size (4) ≤ two_stage_size_threshold (4)` → **packing = .unpacked**.

So `get.zig::data()` takes the 2-stage branch (no stage3):

```zig
const stage1_idx = cp >> 8;      // cp ∈ 0..=0x10FFFF ⇒ idx ∈ 0..=0x10FF (len 4352)
const stage2_idx = cp & 0xFF;
return table.stage2[table.stage1[stage1_idx] + stage2_idx];   // stage2 holds Data directly
```

`stage1: [4352]u16`, `stage2: []Data` (deduplicated 256-blocks; offsets are Data-element
indices, i.e. multiples of 256). For each of the four fields (all "basic"), `get` just
returns `@field(data, name)` — no unshift/unpack/value path is reached.

### 3.3 The packing algorithm we reproduce (from build/tables.zig)

Deterministic, no comptime magic, no allocator subtleties for our 4-basic-field case:

```
STAGE1: Vec<u16> = []
STAGE2: Vec<Data> = []             // flat, element-indexed
block_map: HashMap<[Data;256], u16> = {}
for block_start in (0 ..= 0x10FFFF).step_by(256):
    block = [ data_for_cp(cp) for cp in block_start .. block_start+256 ]   // 256 Data
    off = block_map.get(block).unwrap_or_else(|| {
              let o = STAGE2.len() as u16;   // multiple of 256
              block_map.insert(block, o);
              STAGE2.extend_from_slice(&block);
              o
          })
    STAGE1.push(off)
```

Output is a pure function of scan order (0..4351) — the HashMap is used only for
`found_existing`, offsets are assigned in scan order — so it is **byte-deterministic and
reproducible**, independent of HashMap internals. `data_for_cp` builds the 4-field `Data`
exactly as `writeTableData` does (each `if (@hasField…)` branch, restricted to our 4
fields). No `backing`/`tracking`/`Slice`/`Shift` code is needed (those serve slice/shift
fields we don't have).

`build.rs` writes `tables.rs` into `OUT_DIR` (`STAGE1`, `STAGE2`, `Data`, and the typed
lookup), `include!`d by `uucode/tables.rs`. `cargo:rerun-if-changed` on each vendored UCD
file. No build-dependencies (std-only parsing) — keeps rustty-vaxis a dep-free R4 leaf and
Miri-clean at runtime (statics only).

**Data storage representation** is a free choice (the table is *unpacked* in uucode, so
there is no canonical bit layout to match — only the per-cp *values* must be identical).
Recommend emitting `STAGE2` as `&[Data]` where `Data` is a `#[derive(Clone,Copy,PartialEq,Eq,Hash)]`
struct of the three stored fields + the derived `grapheme_break`, keeping the code obvious;
a `u16`-packed encoding is an optional size optimization and is not required.

### 3.4 grapheme_break derivation (port verbatim — the subtle part)

From `build/tables.zig` lines 927–971 (the `if (@hasField(AllData,"grapheme_break"))` block).
The inputs per cp are the three intermediates `is_extended_pictographic: bool`,
`indic_conjunct_break: IndicConjunctBreak` (InCB), `original_grapheme_break:
OriginalGraphemeBreak`; the output is `grapheme_break: GraphemeBreak`. The Zig collapses the
common branch into `inline else |o| std.meta.stringToEnum(GraphemeBreak, @tagName(o)) orelse
unreachable` — in Rust there is **no comptime tag-name reflection**, so build.rs makes this
mapping **explicit and exhaustive** over every `OriginalGraphemeBreak` variant. Per cp, in order:

1. `is_extended_pictographic == true` ⇒ `grapheme_break = extended_pictographic`.
   **ASSERT** `original_grapheme_break == other` (build.rs `panic!` — tables.zig:930).
2. else `match indic_conjunct_break {`
   - `none` ⇒ map `original_grapheme_break` by identically-named `GraphemeBreak` variant, per
     this **exhaustive** table (the `inline else` fan-out, tables.zig:947–950), with `extend`
     handled specially:

     | `OriginalGraphemeBreak` | → `GraphemeBreak` |
     |---|---|
     | `other` | `other` |
     | `prepend` | `prepend` |
     | `cr` | `cr` |
     | `lf` | `lf` |
     | `control` | `control` |
     | `regional_indicator` | `regional_indicator` |
     | `spacing_mark` | `spacing_mark` |
     | `l` | `l` |
     | `v` | `v` |
     | `t` | `t` |
     | `lv` | `lv` |
     | `lvt` | `lvt` |
     | `zwj` | `zwj` |
     | `extend` | **special:** `cp == 0x200C` (`zero_width_non_joiner`) ⇒ `zwnj`; else `panic!` (tables.zig:944 `unreachable`) |

     (`GraphemeBreak` has **no `extend` variant** — it is decomposed into `zwnj` +
     `indic_conjunct_break_extend` + `indic_conjunct_break_linker`, which is exactly why
     `extend` cannot map by same-name and must branch. The Rust `match` on
     `OriginalGraphemeBreak` must enumerate all 14 arms — no `_ =>` catch-all — so a future
     enum change fails to compile rather than silently mis-mapping.)
   - `extend` ⇒ `cp == 0x200D` (`zero_width_joiner`) ⇒ `zwj`; else `grapheme_break =
     indic_conjunct_break_extend`, **ASSERT** `original_grapheme_break == extend` (tables.zig:957).
   - `linker` ⇒ `grapheme_break = indic_conjunct_break_linker`, **ASSERT**
     `original_grapheme_break == extend` (tables.zig:962).
   - `consonant` ⇒ `grapheme_break = indic_conjunct_break_consonant`, **ASSERT**
     `original_grapheme_break == other` (tables.zig:966).

   `}`

**All four asserts** (`is_extended_pictographic ⇒ original==other`; InCB `.extend`/`.linker` ⇒
`original==extend`; InCB `.consonant` ⇒ `original==other`) are replicated as build.rs `panic!`s
with the same `{cp:x}` context — they are genuine port-time invariants over the vendored UCD, and
firing one means the parse diverged from Zig's. `zero_width_non_joiner = 0x200C` /
`zero_width_joiner = 0x200D` come from uucode `config.zig:5–6`.

This derivation is exactly why the skin-tone modifiers `1F3FB..1F3FF` (verified §8:
GraphemeBreakProperty `Extend`, DCP `InCB;Extend`, NOT Extended_Pictographic) resolve to
`indic_conjunct_break_extend` (InCB `.extend`, `cp != 0x200D`) rather than hitting the
`none/extend` panic — porting the InCB branch faithfully is load-bearing for the `👋🏿` width test.

### 3.5 Parser fidelity notes (Ucd.zig → build.rs)

- `UnicodeData.txt`: handle `<…, First>` / `<…, Last>` ranges + gap-fill unassigned with
  default `general_category = other_not_assigned` (Cn). (We only keep `general_category`;
  skip the other 14 fields per-line.)
- `DerivedEastAsianWidth.txt`: default `.neutral`; honor the `# @missing:` directives
  BEFORE regular entries; map `F/H/W/Na/A/N`. **@missing detection is on the
  WHITESPACE-trimmed line, NOT the comment-stripped line** — this is load-bearing and a
  naive port breaks it (see §3.5.1).
- `GraphemeBreakProperty.txt`: default `.other`; map `Prepend/CR/LF/Control/Extend/
  Regional_Indicator/SpacingMark/L/V/T/LV/LVT/ZWJ`; unknown ⇒ `.other`.
- `DerivedCoreProperties.txt`: only the `; InCB; {Linker|Consonant|Extend}` lines matter
  (ignore all other properties). Default `.none`.
- `emoji-data.txt`: set `is_emoji_presentation` on `Emoji_Presentation` ranges and
  `is_extended_pictographic` on `Extended_Pictographic` ranges.
- Shared helpers: `trim` (strip `#`-comment then whitespace), `parseCp` (hex u21),
  `parseRange` (`AAAA..BBBB` or single). Port their `test{}` (`parseCp`, `parseRange`).

### 3.5.1 DerivedEastAsianWidth @missing — the whitespace-trim vs comment-strip trap

`Ucd.zig::parseEastAsianWidth` (lines 672–722) processes each line in a specific,
**non-interchangeable** order that a naive port silently gets wrong:

```zig
const trimmed = std.mem.trim(u8, line, " \t");   // WHITESPACE-only trim
if (trimmed.len == 0) continue;
if (std.mem.startsWith(u8, trimmed, "# @missing:")) {   // @missing on the WHITESPACE-trimmed line
    const missing_line = trimmed["# @missing:".len..];
    // split on ';', trim both halves, parseRange(cp_str)
    if (std.mem.eql(u8, width_str, "Neutral")) continue;   // default — skip
    if (!std.mem.eql(u8, width_str, "Wide")) unreachable;   // any other @missing value = panic
    // fill range with .wide, then continue
}
const data_line = trim(trimmed);   // trim() = COMMENT-strip (cut at first '#') THEN whitespace-trim
if (data_line.len == 0) continue;
// regular `AAAA..BBBB; W` entry
```

The crux: `trim()` (Ucd.zig 137–142) cuts the line at the **first `#`** and whitespace-trims
the prefix. Applied to `# @missing: 4E00..9FFF; Wide` it yields the **empty string** (the `#`
is at index 0), so the line is dropped as blank. **If build.rs comment-strips first, all 5
`Wide` @missing directives vanish** and every CJK ideograph block (3400.., 4E00.., F900..,
20000.., 30000..) silently defaults to `.neutral` (width 1) — the exact silently-wrong-layout
failure R1 forbids. build.rs therefore MUST, per line, in order:

1. `let trimmed = line.trim_matches([' ', '\t']);` — whitespace-only. `if trimmed.is_empty() { continue }`.
2. `if let Some(rest) = trimmed.strip_prefix("# @missing:")` — detect on `trimmed`, **not** on a
   comment-stripped form. Split `rest` on `;`, whitespace-trim both halves, `parse_range(cp_str)`.
   `width == "Neutral"` ⇒ `continue` (default; the sole non-Wide @missing is `0000..10FFFF; Neutral`).
   `width != "Wide"` ⇒ `panic!` (mirror the Zig `unreachable`). Else fill `[start..=end] = .wide`,
   `continue`.
3. Only now `let data_line = trim_comment(trimmed)` (cut at first `#`, whitespace-trim). Skip if
   empty. Parse `AAAA..BBBB; <F|H|W|Na|A|N>` via `east_asian_width_map`; unknown ⇒ `panic!`.

The vendored file's exact @missing directives (verified): `0000..10FFFF; Neutral` (skipped) +
the five `Wide` blocks `3400..4DBF`, `4E00..9FFF`, `F900..FAFF`, `20000..2FFFD`, `30000..3FFFD`.

**Precisely what the five Wide @missing directives are load-bearing FOR** (verified against the
vendored file — this refines the earlier design's over-broad claim): the extracted file *also*
carries explicit `W` rows that already cover every **assigned** CJK ideograph in these ranges
(`3400..4DBF; W`, `4E00..A014; W`, `20000..2A6DF; W`, `30000..3134A; W`, plus the F900 compat
block). So an assigned ideograph (U+4E00, U+3400, U+20000) is Wide **via its explicit row**, and a
@missing parser miss would NOT flip it. The @missing Wide directives are load-bearing only for the
**unassigned holes** inside those ranges that no explicit row covers — e.g. `U+FA6E`/`U+FA6F` (hole
in `F900..FAFF`), `U+2A6E0` (hole in `20000..2FFFD`), `U+3134B` (hole in `30000..3FFFD`). Those code
points are `Cn` (gap-fill) and are Wide **iff** @missing is honored; a naive comment-strip-first
port silently flips them to `Neutral` (width 1). §8.2 therefore pins BOTH kinds of anchor: assigned
ideographs (guards the explicit-`W`-row + `general_category` path) AND @missing-only holes (the true
isolation of the @missing mechanism).

### 3.6 Why NOT vendored pre-generated arrays (the fallback)

The fallback (commit generated Rust static arrays) is viable but we **do not** need it:
build.rs-from-UCD is auditable (the UCD text is the reviewable source of truth), needs no
Zig, and is fast (parsing ~3.5M of text once, cached by cargo). We would only fall back if
build.rs runtime became a problem (it will not for a one-shot parse). If ever taken, the
fallback arrays MUST be produced by this same build.rs (run once, output committed) — never
hand-authored.

---

## 4. `uucode/utf8.rs` (utf8.zig) — Höhrmann DFA, verbatim

Port the two `const [_]u8` tables (`utf8d` byte→class 256 entries; `state_utf8d`
transition table), `decodeByte`, `isDoneDecoding`, and `Iterator { bytes, i }` with
`init`/`next`/`peek`. `next` returns `Some(0xFFFD)` on `UTF8_REJECT` (not `None`), advances
`i` byte-by-byte, stops on accept/reject or end. Byte-slice input is `&[u8]` (never `String`
— zig-byteslice-to-rust-rawbytes). Preserve the Höhrmann license reference comment. Port
tests `ascii`, `emoji`, `overlong utf8`.

## 5. `uucode/types.rs` — enum subset

Port ONLY the enums our surface needs, with **exact discriminants + variant order**
(the numeric values index into the grapheme-break precompute table and must match):
- `EastAsianWidth` `enum(u3)` — `neutral=0, fullwidth=1, halfwidth=2, wide=3, narrow=4,
  ambiguous=5` (types.zig:101–108).
- `GeneralCategory` `enum(u5)` — 30 variants Lu..Cn (order per types.zig:6–).
- `GraphemeBreak` `enum(u5)` — **exactly 18 variants, discriminants pinned to types.zig:127–150**:
  `other=0, control=1, prepend=2, cr=3, lf=4, regional_indicator=5, spacing_mark=6, l=7, v=8,
  t=9, lv=10, lvt=11, zwj=12, zwnj=13, extended_pictographic=14, indic_conjunct_break_extend=15,
  indic_conjunct_break_linker=16, indic_conjunct_break_consonant=17`. **Load-bearing:** the
  1620-entry grapheme-break precompute indexes by `state*324 + gb1*18 + gb2` (§7), so any drift
  in these 18 values silently corrupts the whole table. (Note this order differs from
  `OriginalGraphemeBreak`'s — e.g. `control`/`prepend`/`cr`/`lf` and the InCB tail — so the §3.4
  mapping is genuinely a re-numbering, not an identity cast.)
- `OriginalGraphemeBreak` `enum(u4)` — 14 variants, discriminants per types.zig:110–125:
  `other=0, prepend=1, cr=2, lf=3, control=4, extend=5, regional_indicator=6, spacing_mark=7,
  l=8, v=9, t=10, lv=11, lvt=12, zwj=13` (build-time derivation input; no `extend` counterpart
  in `GraphemeBreak`, cf. §3.4).
- `IndicConjunctBreak` `enum(u2)` — `none=0, linker=1, consonant=2, extend=3` (types.zig:94–99;
  build-time input).

`OriginalGraphemeBreak`/`IndicConjunctBreak` are needed by `build.rs` only, but live in
`types.rs` (shared by build.rs via a small `#[path]` include, or duplicated in build.rs —
recommend build.rs `include!`ing a `types_ucd.rs` shared enum file to avoid drift). The
runtime `types.rs` re-exports the three runtime enums + `GraphemeBreak`. Derive
`Clone,Copy,PartialEq,Eq` (+ `Hash` for `Data` dedup). Discriminants pinned via
`#[repr(u8)]` + explicit `= n`.

## 6. `uucode/get.rs` — four typed accessors

Replace the generic comptime `get(field, cp)` with concrete fns over `tables::data(cp)`:
`east_asian_width(cp) -> EastAsianWidth`, `general_category(cp) -> GeneralCategory`,
`is_emoji_presentation(cp) -> bool`, `grapheme_break(cp) -> GraphemeBreak`. `cp: u32`
(Rust has no u21; callers pass decoded scalars ≤ 0x10FFFF). Each is the `data(cp).<field>`
read — the exact 2-stage lookup of §3.2. (Faithful behavior; the comptime dispatch is a
Zig-generics artifact with no observable-value role for basic fields.)

## 7. `uucode/grapheme.rs` (grapheme.zig) — UAX #29, verbatim

Port in full:
- `IteratorResult { cp: u32, is_break: bool }`.
- `BreakState` `enum(u3)` — **exactly 5 variants, discriminants pinned** to grapheme.zig:157–:
  `default=0, regional_indicator=1, extended_pictographic=2, indic_conjunct_break_consonant=3,
  indic_conjunct_break_linker=4`. These index the precompute's `state` dimension
  (`state*324 + …`), so pin them (`#[repr(u8)]` + explicit `= n`) exactly as the 18 `GraphemeBreak`
  values (§5).
- `computeGraphemeBreak(gb1, gb2, &mut state) -> bool` — the full GB3..GB999 rule
  ladder + the state-reset preamble + `isIndicConjunctBreakExtend`/`isExtend` helpers,
  line-for-line (every rule comment preserved for G3b).
- `GraphemeBreakTable`/`buildGraphemeBreakTable`/`precomputedGraphemeBreak` — the
  1620-entry (5 states × 18 × 18) precompute. In Rust this is a `const fn` producing
  `[Result; 1620]` (const loops over `0..N` with `enum_from_int`), `Result = { result:
  bool, state: BreakState }`; keep the `@sizeOf == 1620` / index-formula assertions as
  `const _: () = assert!(…)`. The precompute is a pure optimization: `precomputedGraphemeBreak`
  ≡ `computeGraphemeBreak` for all inputs — porting both preserves R1.
- `Iterator<CodePointIterator>` = the `CustomIterator` monomorphized to
  `(utf8::Iterator, GraphemeBreak, BreakState, grapheme_break, precomputedGraphemeBreak)`.
  Generic over the code-point iterator (Rust generic param or a trait `CodePointIterator`
  with `next(&mut) -> Option<u32>` + `i: usize`). Fields `state,i,next_cp_it,next_cp,next_gb`;
  `init`/`next`/`peek`/`nextBreak` verbatim.
- `isBreak(cp1, cp2, &mut state)` helper.
- Tests: `Iterator next/peek`, `Iterator nextBreak`, and the two `GraphemeBreakTest.txt`
  conformance tests (`computeGraphemeBreak` + `isBreak`) — read the vendored fixture via
  `concat!(env!("CARGO_MANIFEST_DIR"), "/ucd/auxiliary/GraphemeBreakTest.txt")`, port
  `testGraphemeBreak`'s `÷`/`×` line parser (1093 cases). These two tests are the **primary
  faithfulness proof** for the `grapheme_break` table + break algorithm without Zig.

## 8. `gwidth.rs` (gwidth.zig) — the 11-test pin

Port `Method { unicode, wcwidth, no_zwj }`, `eawToWidth(cp, eaw) -> i16`, and
`gwidth(str: &[u8], method) -> u16` line-for-line, including:
- the control-char guards (`cp==0`→0; `cp<32 || (0x7f..0xa0)`→−1),
- the `general_category` `mark_nonspacing/mark_enclosing`→0 check,
- the explicit zero-width list (00AD, 200B–200D, 2060, 034F, FEFF, 180B–180D,
  FE00–FE0F, E0100–E01EF),
- EAW `fullwidth/wide`→2 else 1,
- `.unicode`: the grapheme-cluster loop with VS16/VS15/emoji-presentation/`ri_count==2`
  width arbitration (`has_text_vs`→max(1,w); `has_emoji_vs||has_emoji_presentation||
  ri_count==2`→max(2,w)),
- `.wcwidth`: the `1f3fb..1f3ff`→2 skin-tone override,
- `.no_zwj`: split on U+200D, sum `.unicode` of each piece.
`i16` arithmetic + `@max(0, …)` saturation preserved exactly (silently-wrong widths = R1
failure). Port all **11** tests verbatim (UTF-8 string literals / the raw `\xE2\x9D…` byte
strings become Rust `b"…"` / explicit byte arrays — never lossy).

### 8.1 The 11 gwidth codepoints — confirmed against vendored UCD 16.0.0

| test | key cps | resolved facts (verified) | result |
|---|---|---|---|
| `a` | 0061 | gc Ll, eaw Na → 1 | 1/1/1 ✓ |
| emoji ZWJ | 1F469,200D,1F680 | 200D gb=zwj; 1F469 epicto+eaw W → cluster w2; wcwidth sums | 2/4/4 ✓ |
| emoji VS16 | 2764,FE0F | 2764 gc So, eaw **N**, not Emoji_Presentation; FE0F Mn(→0); emoji_vs→max(2) | 2/1/2 ✓ |
| skin tone | 1F44B,1F3FF | 1F3FF gb=**indic_conjunct_break_extend** (InCB Extend) → stays in cluster; epicto→w2 | 2/4/2 ✓ |
| ZW space | 200B | explicit 0 | 0/0 ✓ |
| ZW non-joiner | 200C | explicit 0; gb=zwnj | 0/0 ✓ |
| combining marks | 05B0,093C | both gc Mn → 0 | 0 ✓ |
| flag (RI) | 1F1FA,1F1F8 / 1F1EC,1F1E7 | gb=regional_indicator; `ri_count==2`→max(2) | 2 ✓ |
| text VS | 2764,FE0E | FE0E Mn(→0); text_vs→max(1) | 1 ✓ |
| keycap | 0031,FE0F,20E3 | 20E3 gc Me→0; emoji_vs→max(2) | 2 ✓ |
| base+combining | 0061,0301 | NFD 'a'+acute; 0301 Mn→0 | 1 ✓ |

Every value hinges on `general_category` (Mn/Me), `east_asian_width` (W/N/Na), the
derived `grapheme_break` (zwj/zwnj/regional_indicator/indic_conjunct_break_extend/
extended_pictographic), and `is_emoji_presentation` — precisely the four table fields.

### 8.2 Added @missing-Wide anchor tests (ADDED — closes the last verification crack)

The 11 ported gwidth tests and the 1093-case `GraphemeBreakTest` corpus **never exercise a bare
wide CJK ideograph nor the `@missing` mechanism**, so the load-bearing DerivedEastAsianWidth
`# @missing: …; Wide` directives (§3.5.1) are otherwise UNPINNED — a silent parser miss would still
pass every stated test. Add these **property-anchor width tests** (not from vaxis; ADDED
verification, part of the §2 scaffolding count, not the ghostty parity contract) to `gwidth.rs`:

```rust
// (a) Assigned CJK ideographs — Wide via explicit `W` rows + general_category Lo.
//     Pins the "bare wide ideograph ⇒ width 2" behavior the 11 tests never touch.
assert_eq!(gwidth("\u{4E00}".as_bytes(), Method::Unicode), 2);   // CJK UNIFIED IDEOGRAPH-4E00
assert_eq!(gwidth("\u{3400}".as_bytes(), Method::Unicode), 2);   // CJK Ext A, First
assert_eq!(gwidth("\u{20000}".as_bytes(), Method::Unicode), 2);  // CJK Ext B, First (astral)

// (b) @missing-ONLY holes — unassigned (gc Cn), Wide IFF the `@missing: …; Wide` directive is
//     honored. THIS is the isolating pin: a comment-strip-first parser (§3.5.1 trap) drops
//     @missing ⇒ these become Neutral ⇒ these asserts fail (would-be 1, not 2).
assert_eq!(gwidth("\u{FA6E}".as_bytes(),  Method::Unicode), 2);  // hole in F900..FAFF
assert_eq!(gwidth("\u{2A6E0}".as_bytes(), Method::Unicode), 2);  // hole in 20000..2FFFD
assert_eq!(gwidth("\u{3134B}".as_bytes(), Method::Unicode), 2);  // hole in 30000..3FFFD
```

Trace (both kinds): single-cp cluster; `eawToWidth` sees `cp ≥ 0xA0`, `general_category ∈ {Lo, Cn}`
(neither `mark_nonspacing`/`mark_enclosing`), not in the explicit zero-width list,
`is_emoji_presentation == false`, `east_asian_width == wide` ⇒ **2**; the `.unicode` arbitration
(no VS16/VS15, no emoji presentation, `ri_count == 0`) leaves `w = 2`. Verified against the vendored
UCD 16.0.0: `FA6E`/`2A6E0`/`3134B` are absent from `UnicodeData.txt` (⇒ `Cn` gap-fill) and covered
by NO explicit `DerivedEastAsianWidth` row (⇒ Wide solely via `@missing`). Also add the dual
negative pin — that `@missing` does not leak beyond its ranges — implicitly held by existing
`a`/`0061` (Neutral→1) plus, if desired, an unassigned **non**-CJK cp such as `gwidth("\u{0378}")
== 1` (Cn, eaw Neutral). Keep test (a) even though it does not isolate @missing: it guards the
plain wide-ideograph width the corpus otherwise misses.

## 9. `unicode.rs` (unicode.zig) — Old-API grapheme wrapper

Port `Grapheme { start: usize, len: usize }` + `bytes(&self, str) -> &[u8]`,
`GraphemeIterator { str, inner: grapheme::Iterator<utf8::Iterator>, start, prev_break }`
with `init`/`next` (incl. the trailing "flush last cluster" branch), and
`graphemeIterator(str)`. `&[u8]` throughout. No own tests (uucode's grapheme tests +
gwidth exercise the same path).

---

## 10. Verification ladder (no Zig)

1. **`GraphemeBreakTest.txt` (1093 cases)** — ports uucode's own conformance tests →
   proves `grapheme_break` values + `computeGraphemeBreak`/`precomputedGraphemeBreak`.
2. **11 gwidth tests** — pin EAW + GC + emoji_presentation + clustering (§8).
2a. **@missing-Wide anchor tests (added, §8.2)** — pin the DerivedEastAsianWidth `@missing: …;
   Wide` mechanism the 11 tests + corpus miss: assigned ideographs `4E00`/`3400`/`20000` ⇒ 2, and
   the @missing-ONLY unassigned holes `FA6E`/`2A6E0`/`3134B` ⇒ 2 (fail iff the §3.5.1 comment-strip
   trap drops @missing). Closes the last crack in the "provably bit-exact" claim.
3. **utf8 3 tests** — DFA decoder incl. overlong-reject → 0xFFFD.
4. **Exhaustive dedup self-check (added)** — a `#[test]` builds a flat `[Data]` (or
   re-derives `data_for_cp`) for all 0x110000 cps and asserts `tables::data(cp)` equals it,
   proving the 2-stage block-dedup lookup is lossless.
5. **Miri** (R6 primary — pure crate, no FFI) over the above.

---

## 11. Rules / gates

- **R1** — every line/comment/`test{}` of gwidth.zig + unicode.zig + the uucode runtime
  subset ported (rule comments in `computeGraphemeBreak`, the Höhrmann license note, the
  gwidth zero-width list, the grapheme_break derivation asserts). The 11 gwidth widths are
  exact — any divergence = silently-wrong TUI layout.
- **Byte-slices → `&[u8]`/`Vec<u8>`**, never lossy `String` (zig-byteslice-to-rust-rawbytes):
  gwidth `str`, utf8 `bytes`, unicode `Grapheme.bytes`, all UCD parsing in build.rs.
- **R3** — vendor the 6 UCD 16.0.0 files in-tree under `crates/rustty-vaxis/ucd/` with a
  `PROVENANCE.md` (copied from uucode 0.1.0's `ucd/`, Unicode 16.0.0).
- **R4 acyclic** — the uucode subset is an INTERNAL module of rustty-vaxis (not a new
  crate); rustty-vaxis stays a dep-free leaf. build.rs is std-only (no build-deps).
  `check_acyclic.sh` unaffected.
- **R6** — PURE ⇒ Miri primary. Generated tables are statics; Miri-clean.
- **Enum discriminant fidelity** (§5) — pin `#[repr(u8)]` + explicit values so the
  grapheme-break precompute indexing (`state*324 + gb1*18 + gb2`) matches Zig.
- **Gates:** build / test / `clippy -D` / fmt / `check_acyclic.sh` /
  `check_test_counts.py` (18 ported tests + scaffolding) / `check_comment_coverage.py` + Miri.

---

## 12. Bottom line

**READY, PREFERRED path.** Vendor UCD 16.0.0 (6 files) → `build.rs` parses them, derives
`grapheme_break` (porting the InCB/extended_pictographic branch — load-bearing for skin
tones), and reproduces the **2-stage / unpacked** 256-block-dedup `stage1`/`stage2` (values
identical per-cp; representation free). Port `utf8` (DFA), `types` (5 enums, pinned
discriminants), `get` (4 typed accessors), `grapheme` (full UAX #29 + 1620-entry precompute
+ GraphemeBreakTest conformance), `gwidth` (11-test width logic), `unicode` (Old-API
wrapper). Faithfulness is proven WITHOUT Zig by Unicode's own `GraphemeBreakTest.txt` (1093
cases) + the 11 gwidth pins + an exhaustive dedup self-check, all under Miri. Do not fake a
single width.
