# PERF_FIX_PLAN — ranked, dependency-aware fix plan for the rustty perf gaps

Synthesized from 6 parallel static diagnoses (terminal-parser, osc-parser,
screen-clone/render, pagelist allocator, unicode-property family, R10 debt
audit). All evidence is static analysis + rlib disassembly on the benched
`--release` build (overflow-checks=true, no LTO, cgu=16); no new timings were
taken (machine busy). Reference oracle: /Users/gael/projects/ghostty (Zig
0.15.2, ReleaseFast).

Ranking rule: (impact x confidence) / effort, parity risk as tie-breaker
(lower first). Dependencies are called out per fix; the sequence below is
safe to execute in order.

Bench ids used throughout:

| id | measured gap (Zig -> Rust, idle, N=7 medians) |
|---|---|
| `terminal-parser/utf8` | 286ms -> 834ms (+191%, 2.9x) |
| `osc-parser` | 63ms -> 204ms (+226%, 3.3x) |
| `screen-clone.clone` | 58ms -> 118ms (+104%) |
| `screen-clone.render` | 20ms -> 50ms (+150%) |
| `codepoint-width.table/.simd/.wcwidth` | +23% / +18% / +27% |
| `grapheme-break.noop/.table` | +31% / +30% |
| `is-symbol.table/.uucode` | +24% / +23% |
| `alloc.rss` / `alloc.count` | RSS +47%; 7 vs 3 arena backing allocs (see T0-B) |

---

## TIER 0 — build-profile parity (already in flight separately)

Variants being built elsewhere: **A** = the benched release profile
(overflow-checks=true, no LTO, cgu=16, panic=unwind); **B** = release-fast
(overflow-checks=false, panic=abort); **C** = B + fat LTO + cgu=1. Zig is
ReleaseFast whole-program, so C is the honest apples-to-apples baseline.

Expected coverage per the findings — what profile parity DOES and DOES NOT
close. This table is the null hypothesis every code fix must beat:

| Gap component | Closed by B (no overflow checks)? | Closed by C (LTO/cgu=1)? | Survives C (needs a code fix) |
|---|---|---|---|
| `terminal-parser/utf8` | marginal — only ONE checked-add site exists in `next` (parser.rs:611 twin sites), on non-print arms; print path has zero arithmetic | partial — bench->`next` cross-crate call inlines under LTO | **yes, mostly**: disasm shows `do_action`/`stage_to_action` live in the SAME CGU as `next` and LLVM still refuses to inline them (sret-heavy signatures beat the heuristic); the per-byte slice bounds checks (parser.rs:521-523) are side-effecting panics LLVM cannot delete; dispatch-time Vec allocs are flag-independent |
| `osc-parser` | negligible — per-byte OSC path has no arithmetic | partial — the per-byte cross-crate call (`osc_parser.rs:196-204` PORT NOTE) is the main LTO-sensitive site | **yes, mostly**: per-record malloc/realloc/free chains (Vec capture, `.to_vec()` payloads, `cmd.clone()`) are untouched by any flag |
| `screen-clone.clone` | partial — the 9600-cell/clone copy loop (page_impl.rs:1105-1119) has an overflow-checked Range increment per cell; this is the single site in the whole diagnosis where flags plausibly dominate | partial — cross-crate clone chain inlines | residual: per-cell 3-4x u64 RMW stores vs Zig's coalesced packed store, Flags to_u16 repack in Style::eql, per-chunk pin-key collect, SipHash remap |
| `screen-clone.render` | minor | minor | **yes, almost entirely**: the AoS-vs-SoA reshape (render.rs:552-563 vs render.zig:491-505) is structural; Zig's own comment quantifies it at ~3x on this exact bench |
| decode family (7 lanes) | partial — the u8 `state + char_class` overflow check (utf8_decoder.rs:99) goes away | partial — `grapheme_break`/`props_uucode::get` outlined calls inline | **yes**: the TRANSITIONS[108] bounds check survives ALL flags (u8 index range 0..=255 > 107, not provable); live `assert!(consumed)` per byte survives; 3-stage lut bounds checks survive |
| `alloc.rss` / `alloc.count` | n/a | n/a | mostly measurement artifact (see T0-B); one real divergence = arena resize seam (Tier 2) |

### T0-B — measurement corrections (no code, do before trusting alloc numbers)

1. The "7 vs 3" compares rustty's FULL workload (main pool + clone's fresh
   pool + reset realloc) against an isolated single-arena Zig replay
   (ALLOC_BENCH.md:52, :96-106). Decomposed with the shared 1.5x growth
   formula, the real gap is ~+1 alloc (the reset realloc — Tier 2 fix
   `arena-resize-seam`). Re-run with equal scope.
2. The +47% RSS was measured inside `cargo test --release` (tracking seam is
   `#[cfg(test)]`-only), which (a) carries a multi-MB libtest baseline vs
   Zig's 341KB standalone exe and (b) flips `cfg!(test)` so
   `grapheme_bytes` shrinks 8192 -> 512 (page.rs:362 — mirroring
   page.zig:1787), i.e. the measured geometry was NOT the production
   589,824B std_size the Zig bench used. Move the seam to an off-by-default
   `alloc-bench` cargo feature and re-measure standalone. Expect +47% to
   collapse toward the +11.6% page-bytes delta or into the stated 10-18MB
   noise band.
3. Add a target-gated test asserting `std_size() == 589824` on
   aarch64-macos (pagelist/mod.rs:183-185 has no golden gate today) so page
   geometry drift is caught structurally. Note: the earlier "4KiB min-page"
   concern is STALE — page.rs:172-199 already derives PAGE_SIZE_MIN=16KiB on
   aarch64-macos.

### T0-C — process gate

There are ZERO `PERF(port)` markers in the workspace (only a preserved
upstream `PERF(mitchellh)` comment at rustty-vt/src/terminal.rs:2322)
despite R10 mandating them. Every fix below MUST land with its marker, and a
grep-based G-gate check should be added so future sub-batches cannot land
unmarked downgrades.

---

## TIER 1 — low-risk, high-impact code fixes

### 1. `fix-inline-idioms` — restore every dropped Zig inline hint  (effort S)

**Targets:** `terminal-parser/utf8`, `osc-parser`, all 7 decode lanes.

**Files/changes:**
- `crates/rustty-vt/src/parser.rs` — `#[inline]` on `next` (:429);
  `#[inline(always)]` on `do_action` (:582, reference is `inline fn`,
  Parser.zig:324), `collect` (:570, ref `pub inline fn`, Parser.zig:313),
  `clear` (:690, ref Parser.zig:409), and `stage_to_action` (:535 —
  port-only glue that must never be a call).
- `crates/rustty-vt/src/parser.rs:641-643` — the OscPut arm's
  `self.osc_parser.next(c)` mirrors `@call(.always_inline, ...)` at
  Parser.zig:364; satisfied by the next bullet.
- `crates/rustty-vt/src/osc/mod.rs` — `#[inline(always)]` on
  `Parser::next` (:769, ref bench forces always_inline at
  OscParser.zig:103); `#[inline]` on `ensure_allocator` (:720, ref
  osc.zig:441 `inline fn`), `capture_trailing` (:740, ref osc.zig:503),
  `Capture::write_byte` (:616).
- `crates/rustty-vt/src/utf8_decoder.rs:86` — upgrade `#[inline]` ->
  `#[inline(always)]` (ref UTF8Decoder.zig:56 `pub inline fn` is a
  MANDATORY inline in Zig).
- `crates/rustty-unicode/src/grapheme.rs:41` (`grapheme_break`),
  `crates/rustty-unicode/src/props_uucode.rs:9` (`get`), the
  symbols_table/symbols_uucode getters, and `props_table::table` — add
  `#[inline]` (non-generic cross-crate fns are un-inlinable without LTO).

Each site marked `// PERF(port): Zig 'inline fn' / @call(.always_inline) —
restore mandatory-inline idiom`.

**Why it beats profile parity:** disasm of librustty_vt shows `next` (0x1c0
stack frame) with BRANCH26 relocations to `do_action` (x2) and
`stage_to_action` (x3) — all in the SAME codegen unit, i.e. LLVM's refusal
is heuristic (sret-heavy signatures), not a flags artifact; only
`inline(always)` overrides it. Inlining `next` into the bench loop unlocks
DSE of the per-byte fully-materialized-then-discarded 192-byte
`[Option<Action>;3]` sret, exactly what Zig ReleaseFast does with its
`[3]?Action` (~240B — return SIZE is not the divergence; DSE-ability is).

**Parity argument:** attributes only; zero semantic change. **Gates:** full
`cargo test --workspace`; no test content changes. **Expected:** high —
removes 5 out-of-line calls + 448B frame churn per input byte on
`terminal-parser/utf8`; per-byte call + sret on `osc-parser`; outlined
per-codepoint call on `grapheme-break.table`.

### 2. `fix-utf8-dfa` — elide the per-byte TRANSITIONS bounds/overflow checks  (effort S)

**Targets:** `grapheme-break.noop/.table`, `codepoint-width.*`,
`is-symbol.*` (all share the UTF8Decoder loop over 64MiB = 67M iterations),
minor tailwind for `terminal-parser/utf8`.

**Files/changes:** `crates/rustty-vt/src/utf8_decoder.rs:38,99`.
(a) Pad `TRANSITIONS` from `[u8; 108]` to `[u8; 256]`, entries 108..=255
filled with 12 (REJECT). The u8 index is then provably < 256; LLVM elides
the bounds check with zero unsafe.
(b) Compute the index as `self.state as usize + char_class as usize` —
both zext-from-u8 <= 255, so the checked add constant-folds even with
overflow-checks=true.

**Parity argument:** DFA invariant machine-verified by the diagnosis:
reachable states are {0,24,36,48,60,72,84,96} (REJECT=12 resets to 0 before
return) and char_class <= 11, so max reachable index is exactly 107 — pad
entries are unreachable, and even if reached the filler is the reference's
own fail-safe REJECT value. Codepoint stream is bit-identical.
Zig side pays neither check (UTF8Decoder.zig:68, ReleaseFast).
Mark `// PERF(port): pad table so the u8 index is provably in-bounds; Zig
ReleaseFast has no check here`.

**Gates:** existing exhaustive decoder byte-sequence tests; add a
const/unit assert that entries 0..108 are unchanged from the reference
table. **Expected:** high for the decode family — this is the only cost
shared by all seven +18..31% lanes including decode-only `noop` (+31%);
(b) is the ONE site where overflow-checks plausibly dominate, and (a)
survives every profile flag.

### 3. `fix-osc-fixed-capture` — restore the zero-alloc fixed writer over `self.buffer`  (effort M)

**Targets:** `osc-parser` (primary), `terminal-parser/utf8` (minor — C1
0x9D in UTF-8 continuation bytes enters osc_string).

**Files/changes:** `crates/rustty-vt/src/osc/mod.rs:496,616-631,744-793`
and the ~15 `osc/parsers/*` files that read `trailing()`/`trailing_mut()`.
Replace `CaptureBacking::Fixed(Vec::new())` (:744-747) with writes into the
Parser's already-ported-but-unused inline `buffer: [u8; MAX_BUF]` (:496):
Capture stores only a running `len` (it cannot hold `&mut self.buffer` —
self-referential), the write happens in `Parser::next`
(`if len < MAX_BUF { self.buffer[len] = c; len += 1 } else { state =
Invalid }`), and `trailing()`/`trailing_mut()` become `&self.buffer[..len]`
accessors threaded through Parser. Mark `// PERF(port): restore Zig
fixed-writer-over-self.buffer capture (osc.zig:462-512); Vec-per-record
downgrade removed`.

**Reference:** osc.zig:509-512 (`Capture.fixed(&self.capture,
&self.buffer)`), :462-467 (`std.Io.Writer.fixed` — zero allocation ever),
:543-549 (writeByte = bounds check + store). Rust today: 1 malloc + ~5
doubling reallocs + memcpys + 1 free PER OSC RECORD vs Zig's literal zero,
plus a redundant double bound check per byte (mod.rs:784 + Vec's grow
check).

**Parity argument:** overflow-to-Invalid at MAX_BUF is byte-identical;
`trailing_mut` in-place NUL stamping (rxvt_extension) gets the same &mut
slice. The extensive ported boundary tests (change_window_title
longer-than / exactly-at / one-short-of buffer) pin the behavior. This
INCREASES fidelity. **Gates:** full osc test suite + vt suite.
**Expected:** high — plausibly 40-60% of the osc-parser gap; flag-
independent (no profile removes malloc traffic).

### 4. `fix-bench-assert-tier` — port `std.debug.assert` at its correct tier  (effort S)

**Targets:** `codepoint-width.table/.wcwidth/.simd`, `is-symbol.*`,
`grapheme-break.table` (all non-noop decode lanes).

**Files/changes:** `crates/rustty-bench/src/codepoint_width.rs:250,291,336`,
`is_symbol.rs` (step_uucode/step_table), `grapheme_break.rs` (step_table):
`assert!(consumed)` -> `debug_assert!(consumed)`. Optionally follow with
`unsafe { core::hint::assert_unchecked(consumed) }` under a PERF(port) note
to restore ReleaseFast's assume-hint (the DFA only returns consumed=false
from the REJECT arm mid-sequence, and these loops always re-feed).

**Parity argument:** `debug_assert!` IS `std.debug.assert`'s exact
semantics tier — live in Debug, absent in release, and ghostty's own tests
run in Debug where the check stays live (CodepointWidth.zig:131,158,190).
The current live `assert!` is a per-byte compare+branch Zig compiles out
AND converts to an optimizer assume. **Gates:** bench --validate modes in
debug builds. **Expected:** medium — 1 branch per byte x 67M in 5 lanes +
recovered optimizer assumption.

### 5. `fix-pin-keys-collect` — drop the per-chunk tracked-pin key Vec  (effort S)

**Targets:** `screen-clone.clone` (small), any multi-page clone.

**Files/changes:** `crates/rustty-vt/src/pagelist/access.rs:876` — replace
`let pin_keys: Vec<*mut Pin> = self.tracked_pins.keys().copied().collect();`
(inside the chunk loop at :827) with direct iteration of
`self.tracked_pins.keys()`. Self is `&self` and the loop mutates only the
local pool/remap, so no borrow conflict; every OTHER ported site
(grow.rs:97,343,529,617, erase.rs:56,117) already iterates directly.
Reference: PageList.zig:866 (`const pin_keys = self.tracked_pins.keys();` —
free borrow). Same treatment for `pin.rs:108-110` callers that only
iterate. Mark PERF(port).

**Parity argument:** identical iteration order (insertion order both
sides), identical visited set. **Gates:** pagelist tests. **Expected:** low
for this bench (~one small alloc/free per chunk per clone), but free,
riskless, and it restores the exact reference idiom.

---

## TIER 2 — medium effort or medium parity-risk

### 6. `fix-parser-destage` — restore the reference arm structure in `Parser::next`  (effort M; depends on fix 1 landing first so before/after is attributable)

**Targets:** `terminal-parser/utf8` (primary), `osc-parser` (minor).

**Files/changes:** `crates/rustty-vt/src/parser.rs:377-390,460-462,
521-529,535-567`. Kill the port-only `Staged` -> `stage_to_action` layer:
build the final `Option<Action>` directly in each exit/transition/entry
arm, materializing `&self.intermediates[..idx]` / `&self.params[..idx]`
slices (and the owned-Vec copies) ONLY inside the CsiDispatch/EscDispatch/
DcsHook arms — exactly where Parser.zig:377-384/:398-403/:299-306 slices.
Today the slices are materialized unconditionally per byte with two REAL
bounds-check branches (disasm: `cmp x21,#0x5; b.hs` / `cmp x22,#0x18;
b.hi` -> slice_index_fail), a SepList copy, and 3x 72B `Staged` values +
3x 64B sret round-trips — on the Ground->Ground print path where none of
it is needed. These panics are side-effecting under Rust semantics, so
even perfect inlining (fix 1) cannot delete them.

The Staged detour existed to slice after `self.state` was written; since
payloads are owned Vecs (not borrows into self), that borrow-ordering
constraint (PORT NOTE parser.rs:370-376) is obsolete — `do_action` can
return `Option<Action>` directly and the deferred state write stays put.
Keep the arm-for-arm reference comment mapping. Mark `// PORT NOTE
(reshape): restore reference arm structure (Parser.zig:267-310)`.
Fallback minimal variant: move :521-523 into `stage_to_action` behind
`match staged { CsiDispatch|EscDispatch|DcsHook => ... }`.

**Parity argument:** observable behavior identical for every reachable
state — `collect()` and the param arm maintain `intermediates_idx<=4`,
`params_idx<=24` (tests csi_too_many_params / dcs_too_many_params pin the
boundaries); only panic timing in impossible corrupted-state cases
differs, which the reference treats as safety-checked UB. **Gates:** all
20 ported parser tests + full vt suite. **Expected:** medium-high — with
fix 1 it lets the print path collapse to table-load + compare + store, as
in Zig.

### 7. `fix-render-soa` — restore MultiArrayList SoA + raw-column memcpy in RenderState  (effort L; independent, schedule early despite effort — it is the only fix for `.render`)

**Targets:** `screen-clone.render` (primary; the reference's own comment
quantifies this exact idiom at "~300% faster based on the screen-clone
benchmark" — matching the measured +150% almost exactly).

**Files/changes:** `crates/rustty-vt/src/render.rs:91,184-226,522-563,
570-624` (+ renderer-crate consumers of `row_data`). Split Row cells into
parallel Vecs mirroring the SoA columns — `raw: Vec<page::Cell>` (plain
u64), `style: Vec<Style>`, grapheme as a per-row reusable buffer
(`Vec<u32>` + (offset,len) per cell; `.clear()` retains capacity — the
direct analogue of `arena.reset(.retain_capacity)`, render.zig:469-471).
Rebuild = resize len + `raw.copy_from_slice(page_cells)` (960B/row at 120
cols), writing style/grapheme ONLY inside the managed-row loop for flagged
cells — the reference's undefined-unless-managed discipline
(render.zig:491-505). Also hoist `let row = &mut self.row_data[y]` /
`let cells = &mut row.cells` once per row and iterate
`cells.iter_mut().zip(page_cells)` so the double bounds-checked
`self.row_data[y].cells[x]` indexing per cell (:574,598,604,616) vanishes
(Zig hoists the column slices at render.zig:383-393,509-511). Expose a
small Cell-view accessor so consumers keep an equivalent API. Mark
`// PORT NOTE (reshape): restore MultiArrayList SoA (render.zig:187) — the
carried '~300% faster' comment at render.rs:545-551 describes THIS layout`.

Current cost being removed: per rebuilt cell, drop-glue on the old
`Cell{raw:8B, grapheme:Vec<u32>:24B, style:~24B}`, a fresh push with
capacity check + `Style::default()` + `Vec::new()` stores (~7x memory
traffic vs the 8B raw copy), x 9600 cells x 1000 updates.

**Parity argument:** RenderState CONTENTS are identical; only internal
layout changes; the read contract (raw always valid; style/grapheme valid
only when flagged) is exactly the Zig contract. Medium risk because
`row_data`/cells are pub and read by the renderer crate — needs a consumer
sweep. **Gates:** all 15 ported render tests + renderer crate build +
workspace tests. **Expected:** high — the single largest well-quantified
structural gap in the whole diagnosis.

### 8. `fix-lut-bounds` — remove the 3 chained bounds checks in the 3-stage unicode lookup  (effort S-M)

**Targets:** `codepoint-width.table`, `grapheme-break.table`,
`is-symbol.table` (+ the terminal print path via the width seam).

**Files/changes:** `crates/rustty-unicode/src/lut.rs:226-231`. Preferred
zero-unsafe variant first: change `TablesRef` to hold `&'static [u16; N]`
fixed-size array refs from the generated tables (props_data.rs:20,567,1643:
STAGE1 [u16;8192], STAGE2 [u16;31744], STAGE3 [Properties;30]) so LLVM
elides stage2/stage3 checks; mask the codepoint to the reference's u21
domain (`cp & 0x1F_FFFF`, exactly what Zig's `cp: u21` parameter type
guarantees at lut.zig:141-145) so stage1 is provable too. If a residual gap
remains, `get_unchecked` for stage2/stage3 under the generator invariant
(every stage1 entry + 255 < stage2.len(); every stage2 entry <
stage3.len()) with a unit test that machine-checks the invariant over the
embedded constants, + debug_asserts. Mark PERF(port).

**Parity argument:** same tables, same indices; the mask matches the
reference's u21 parameter domain exactly (Rust widened it to u32, making
the range unprovable — that widening is the port bug). **Gates:** unicode
crate tests + the invariant-checking test. **Expected:** medium — 3
branches per lookup, 1-2 lookups per decoded codepoint, stacking on fix 2;
consistent with table lanes trending above `.simd` (+18%), whose inner
call is verbatim-identical Highway C++ FFI on both sides (verified — not a
gap).

### 9. `fix-cell-copy-coalesce` — coalesce the per-cell flag resets in PageList clone  (effort M; measure AFTER variant C exists, since this loop is also where profile flags bite hardest)

**Targets:** `screen-clone.clone`.

**Files/changes:** `crates/rustty-vt/src/page_impl.rs:1105-1119` — replace
`*dst = *src` + 3 separate RMW bitfield setters (`set_hyperlink(false)`,
`set_style_id(DEFAULT_ID)`, conditional `set_content_tag`) with one masked
u64 op (`let mut v = src.to_bits(); v &= !(HYPERLINK|STYLE_ID masks); if
tag == CodepointGrapheme { retag }; dst.write(from_bits(v))`), iterating
with a pointer pair or zip instead of the overflow-checked `for i in
0..span`. Optionally: store `Flags` as its packed u16 internally with
getters so `Style::eql` (page_impl.rs:1229-1259) becomes a single 16-bit
compare like Zig's `packed struct(u16)` instead of a 9-shift `to_u16`
repack per side (style.rs:98-121).

**Parity argument:** bit-identical cell values by construction — Zig's
packed-struct field writes (page.zig:895-908) fold to exactly this masked
store under ReleaseFast; add a unit test asserting the masked-op result
equals the sequential-setter result over all flag combinations. Flags
repacking touches PackedStyle hashing — keep the packed_style_layout test
as the gate. **Gates:** that new equivalence test + full vt suite +
existing style/packing tests. **Expected:** medium as a code change (LLVM
may partially coalesce already); decide from the A-vs-C delta on this
bench before spending the effort.

### 10. `fix-color-list-prealloc` — restore SegmentedList(Request, 2) inline prealloc  (effort S)

**Targets:** `osc-parser` (corpus-dependent: OSC 4/10-19/104/110-119).

**Files/changes:** `crates/rustty-vt/src/osc/parsers/color.rs:417-422` —
replace `pub type List = Vec<Request>` with a 2-slot-inline small-vector
(local enum `{Inline([Request;2], u8), Heap(Vec)}` or the workspace-
sanctioned SmallVec equivalent) keeping the same push/iter API. The
reference chose prealloc=2 explicitly "because a large majority of the
time there will be only one operation per OSC" (rationale carried verbatim
at osc/mod.rs:91-95; color.zig:332). Mark `// PERF(port): restore
SegmentedList(Request,2) prealloc (color.zig:332)`.

**Parity argument:** module-internal storage, identical element order and
contents; the SegmentedList->Vec swap is already a declared PORT NOTE
divergence — this restores its prealloc property. **Gates:** color parser
tests. **Expected:** low-medium, corpus-dependent; also removes
ColorOperation's Vec from Command drop glue.

### 11. `fix-remap-hasher` — pointer-identity hasher for TrackedPinsRemap  (effort S)

**Targets:** `screen-clone.clone` (small).

**Files/changes:** `crates/rustty-vt/src/pagelist/access.rs:721` — swap the
default SipHash `RandomState` for `BuildHasherDefault<PtrHasher>` (FxHash-
style u64 mix, or the already-ported rustty-zigstd AutoHashMap equivalent),
keeping the type alias so call sites (screen.rs:844-848,862,919,927,986)
are unchanged. Reference: PageList.zig:778 `std.AutoHashMap(*Pin, *Pin)` —
cheap integer autoHash, no per-init seed setup. Mark PERF(port).

**Parity argument:** map is internal to clone; all access is by key,
iteration order never observed — any hasher preserves behavior.
**Gates:** clone tests. **Expected:** low here (few ops/clone); medium for
many-pin screens; strict restore of the reference's cheaper idiom.

### 12. `fix-arena-resize-seam` — wire in-place resize from ArenaAllocator to PageAllocator  (effort M)

**Targets:** `alloc.count` (+1 alloc per PageList::reset), reset-heavy
sessions; NOT the four latency benches.

**Files/changes:** add a `ResizeInPlace` trait (default false — matching
Zig children that refuse resize, e.g. Global); implement for
`PageAllocator` by delegating to the ALREADY-PORTED-BUT-DEAD
`TaggedPageAllocator::resize/realloc` (crates/rustty-os/src/mach.rs:99-109,
192-236 — in-place tail munmap on shrink, unreachable today because
page_allocator.rs:102-156 wires only allocate/deallocate); call it from
`ArenaAllocator::reset` (crates/rustty-zigstd/src/arena_allocator.rs:
386-411) before the manual-realloc fallback, mirroring Zig's
arena_allocator.zig:154 rawResize attempt. This also kills the pure-waste
~590KB memset of a freshly-mmap'd already-zero node at
pagelist/mod.rs:1286-1290 (the retained node is then genuinely retained,
as in ghostty). CRITICAL: the `cfg(test)` TrackingAllocator must forward
the resize AND adjust recorded byte sizes, or the R6 leak gates see a
phantom byte leak (alloc(909,360)…dealloc(589,840)).

**Parity argument:** allocation geometry is not observable behavior; no
pagelist test asserts backing-node counts (tests assert only
delta_allocs()>0 and return-to-baseline); in-place shrink is sound — reset
only resizes a node with end_index=0 (no live carve-outs), exactly the
invariant Zig relies on. rustty-zigstd arena node-count tests use Global
(default-false impl) and stay green. Mark PERF(port). **Gates:** R6 leak
gates + arena tests + pagelist tests. **Expected:** medium for alloc
metrics, ~zero for the latency benches — schedule last in Tier 2.

---

## TIER 3 — speculative / algorithmic; ONLY if Tier 1+2 + variant C leave a gap

### 13. `t3-action-arrayvec` — inline fixed-capacity CSI/ESC/DCS payloads  (effort L)

Replace `Vec<u8>`/`Vec<u16>` in Action's CSI/ESC/DCS variants
(parser.rs:160-165) with ArrayVec-style `([u8; 4], len)` / `([u16; 24],
len)` — capacities are hard invariants (MAX_INTERMEDIATE=4 at
parser.rs:332, MAX_PARAMS=24 at :345; Zig's fixed arrays,
Parser.zig:80-84) — exposing `as_slice()`/`len()` so tests keep asserting
contents. Removes 2-3 mallocs per dispatched escape sequence AND makes
these variants drop-free (today the `[Option<Action>;3]` return carries
Vec drop glue executed on EVERY byte including plain prints).
**Targets:** `terminal-parser/utf8` (drop glue), `osc-parser`/escape-heavy
replays (allocs). **Parity risk:** medium — public payload type change
ripples into stream.rs handlers, vt-c mirrors, and every pattern-matching
test; behavior identical but the edit surface is wide. Do only after fixes
1+6 are measured, gated on the full workspace suite.

### 14. `t3-osc-command-move` — move, don't deep-clone, the OSC Command at dispatch  (effort S-M)

parser.rs:460-462 `Staged::OscDispatch(cmd.clone())` deep-clones a 64B
Command incl. Vec payloads (~15 Vec::clone sites in the disasm); the
reference copies the union shallowly (`cmd.*`, Parser.zig:270-271) under
the documented "valid until the next call" contract (osc.zig:744-745).
Replace with `std::mem::replace(cmd, Command::Invalid)`-style move-out —
observationally identical under that same contract. **Targets:**
`osc-parser`, `terminal-parser/utf8` (C1 wander). **Parity risk:** low-
medium — audit that the osc parser's `reset()` doesn't inspect the moved-
from command. Bundled here (not Tier 1) because its win overlaps fix 3 and
it should be measured against the post-fix-3 baseline.

### 15. `t3-osc-payload-copies` — reduce per-record `.to_vec()` payload copies  (effort M, contingent)

After fix 3, each OSC record still pays ONE malloc+memcpy+free
(change_window_title.rs:26, clipboard_operation.rs:31,46,
hyperlink.rs:36,68, iterm2.rs:142,180 — :180 is a double copy —
context_signal.rs:414-415, kitty_text_sizing.rs:199-202) vs Zig's zero
(borrowed `[:0]const u8` into parser.buffer). Accept it unless post-C
numbers still gap; the iterm2.rs:180 double copy can be fixed standalone.
The full zero-copy `Command<'buf>` reshape is on the DO-NOT list.

### 16. `t3-mmap-addr-hint` — port `next_mmap_addr_hint`  (effort S, low value)

rustty-os/src/mach.rs:143-147 passes NULL where mach.zig seeds and
cmpxchg-updates a process-global hint so consecutive arena mmaps pack
adjacently. ~15 lines, AtomicPtr, advisory-only (identical results either
way; Zig tolerates a failed cmpxchg). Affects VM-region locality/count,
not RSS. Do opportunistically.

---

## Verification & follow-ups (no bench needed, do anytime)

- **std_size golden test** (T0-B item 3): assert 589,824 on aarch64-macos;
  if it differs, diff `Page::layout` section-by-section vs
  page.zig:1700-1760 (suspects: RefCountedSet Item padding, hyperlink
  PageEntry, STRING_CHUNK). A mismatch would be a parity FIX (restores
  reference geometry) gated on the full vt suite.
- **Alloc re-measure with equal scope** (T0-B items 1-2), idle machine,
  after the seam moves off `cfg(test)`; optionally dtrace/ktrace one clone
  per side to close the 7-vs-3 question for good.
- **Fidelity note (perf-neutral):** allocating-mode OSC capture uses the
  global allocator (mod.rs:762) instead of the provided ZigAllocatorRef
  (osc.zig:474 routes through the allocator) — add a PORT NOTE or route it
  through; behavior parity holds either way (same underlying malloc).
- **PERF(port) G-gate** (T0-C).

## Explicitly ruled out as causes (do not re-investigate)

- OSC state-machine dispatch shape: identical jump tables both sides
  (osc/mod.rs:797-994 vs osc.zig:552-736); no string compares.
- ZigAllocatorRef dyn vtable: never touched per byte (`is_some()` checks
  only, mod.rs:721,751); Zig's std.mem.Allocator is runtime-dispatched too.
- Per-byte UTF-8 validation: none on either side.
- `.simd` lane inner call: same verbatim Highway C++ at -O3 behind extern
  on both sides (rustty-simd/src/codepoint_width.rs:21 vs
  src/simd/codepoint_width.zig:5).
- Pagelist geometry/policy: PAGE_PREHEAT=4, pin preheat 8, std_capacity
  215x215/128/8192, growth formula, retain limits, lazy-zero MAP_ANON,
  destroy-time zeroing — ALL verified faithful. Parameter tuning is
  forbidden (would diverge from the reference).
- Overflow checks as the dominant cost in the PARSER benches: the print
  hot path has zero arithmetic; only 1 checked-add site in `next`
  (non-print arms). They matter most in the page_impl clone loop (fix 9)
  and the UTF8Decoder add (fix 2b).

---

## Measurement protocol

Preconditions: idle machine (no other load), performance cores stable,
same 64MiB corpora and OSC replay files as the original runs.

1. **Baselines:** run all bench ids on variants A (release, as originally
   benched), B (release-fast), C (rf + fat LTO + cgu=1) — already built.
   N=7, medians, sequential (never parallel). C-vs-Zig is the honest gap;
   A-vs-C is the profile-flag share and must match the Tier 0 coverage
   table's predictions (if it doesn't, revisit attribution before coding).
2. **One bench run per accepted fix, sequential:** apply fixes in the
   ranked order below; after each, rebuild variant A AND variant C, run
   ONLY the fix's target bench ids (N=7 medians) plus `terminal-stream/
   ascii` as a canary (it is at parity, +1.3% — it must NOT regress).
3. **Accept/revert rule:** accept iff (a) at least one target bench
   improves beyond run-to-run noise (>3% median shift) on variant A or C,
   and (b) no non-target bench regresses >2%, and (c) all gates
   (workspace tests, R6 leak gates, Miri lanes where applicable) are
   green. Otherwise revert and log.
4. **Re-baseline after each accepted fix** — later fixes are measured
   against the updated baseline so impact attribution stays honest.
5. Log every run in `.port/phase-2/PERF_LOG.md` (fix id, commit, variant,
   per-bench medians, accept/revert).
6. Full-sweep checkpoint after Tier 1 completes and again after Tier 2;
   Tier 3 items proceed only for bench ids still >10% over Zig on
   variant C.

## DO-NOT list (rejected for parity risk)

1. **Zero-copy `Command<'buf>` lifetime reshape** of OSC payloads — the
   lifetime infects every Command consumer, Clone impls, and the vt-c C
   API contract; rustty consumers may rely on owned data today. Requires a
   dedicated parity audit; not a perf-plan item.
2. **Any pagelist geometry/policy tuning** (preheat counts, block sizes,
   std_capacity, growth factor) — verified faithful; changing them
   DIVERGES from the reference. Forbidden.
3. **`get_unchecked` without a machine-checked proof** — every unchecked
   index in this plan ships with either a compiler-provable bound (padded
   table, fixed-size array ref, mask) or a unit test that verifies the
   invariant over the embedded constants, plus a debug_assert.
4. **Deleting the live `assert!(consumed)` outright** (vs debug_assert) —
   debug_assert is the faithful tier; full removal would drop the
   reference's Debug-mode check that ghostty's own tests rely on.
5. **Replacing the global allocator or arena strategy** — the ALLOC study
   shows rustty latency already BEATS the Zig standalone bench there; the
   only sanctioned change is the resize seam (fix 12).
6. **wrapping_add on parser index increments** (parser.rs:578,611,657) —
   legal with the guard-invariant argument, but the win is one predictable
   branch on non-print arms; not worth unsafe-adjacent divergence. Variant
   B/C already removes it. Revisit only if a shipped-default-profile win
   is demanded post-measurement.
7. **Touching the SIMD FFI lane** — verified identical on both sides.
