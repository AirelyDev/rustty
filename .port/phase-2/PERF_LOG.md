# PERF_LOG — perf-opt branch: per-fix attribution & accept/revert decisions

Session: 2026-07-16, Apple M4 Pro, macOS 26.5.1. All numbers are N=7 medians
(1 warmup discarded), sequential runs, identical `ghostty-gen` corpora, machine
gated on cordo-session quiet (sustained load ≈ 6–8 incl. 4 stray `yes` procs on
4 of 14 cores; single-threaded benches unaffected — fresh Zig ran *faster* than
the prior committed baseline, worst-case −9% on data benches).

Zig reference: `ghostty-bench` ReleaseFast (phase-0 binary), measured fresh in
the SAME session as every Rust variant (all comparisons same-session).

## Variants

| variant | code | profile |
|---|---|---|
| frozen A/B/C | pre-fix `10cc614` | A=release (overflow-checks ON), B=release-fast (checks off), C=B+fat LTO+cgu1+abort |
| stage_\<fix\>_C | cumulative chain commit | C profile |
| FINAL_A/C | post-merge `d211a84` | A / C |
| EXP-thinlto / EXP-Bfinal / EXP-demote / EXP-nocolor | `d211a84` (± noted delta) | isolation experiments |
| **SHIP-RF** | **`9e1837b` (shipped)** | **release-fast = overflow-checks off only** |

## Headline (SHIP-RF vs Zig, same session)

| bench | zig ms | rust ms | Δ | pre-fix A | code improvement |
|---|--:|--:|--:|--:|--:|
| terminal-parser | 262.9 | 322.7 | **+22.7%** | 667.1 | −51.6% |
| terminal-stream | 562.6 | 458.4 | **−18.5%** | 447.3 | +2.5% |
| codepoint-width.noop | 3.9 | 3.1 | −21.8% | 3.0 | ≈ |
| codepoint-width.table | 233.2 | 218.4 | **−6.3%** | 237.9 | −8.2% |
| codepoint-width.simd | 502.8 | 474.0 | **−5.7%** | 482.9 | −1.9% |
| codepoint-width.wcwidth | 258.6 | 242.2 | **−6.3%** | 260.4 | −7.0% |
| grapheme-break.noop | 231.3 | 219.1 | **−5.3%** | 260.1 | −15.8% |
| grapheme-break.table | 255.4 | 215.6 | **−15.6%** | 267.2 | −19.3% |
| is-symbol.table | 239.0 | 205.6 | **−14.0%** | 225.7 | −8.9% |
| is-symbol.uucode | 230.3 | 214.1 | **−7.0%** | 230.8 | −7.2% |
| osc-parser | 57.3 | 57.5 | +0.3% (tie) | 143.2 | **−59.9%** |
| screen-clone.noop | 5.6 | 5.2 | −6.7% | 5.3 | ≈ |
| screen-clone.clone | 49.7 | 79.8 | **+60.4%** | 79.0 | ≈ |
| screen-clone.render | 15.6 | 24.7 | **+57.6%** | 31.4 | −21.4% |

**10 wins / 1 tie / 3 losses. Suite total: Zig 2908 ms vs Rust 2740 ms (−5.8%).**
Before this branch: 0 wins / 1 tie / 13 losses.

## Per-fix attribution (stage deltas, C profile; values = the fix's own targets)

| fix | commit | verdict | evidence |
|---|---|---|---|
| fix-profile-lto | 9e028ec | superseded by 9e1837b | see LTO finding below |
| fix-inline-idioms | f2b7dcd | **ACCEPT** | osc 130→77 (−40%); t-parser 685→620 (−9.5%) |
| fix-utf8-dfa | 79dce2e | **ACCEPT** | whole decode family −5…−7% (cw.table 229→216, gb.table 250→236, is.table 223→209 …) |
| fix-bench-assert-tier | eb32359 | **ACCEPT** (≈noise here; correct tier regardless) | debug_assert = Zig std.debug.assert tier |
| fix-pin-keys-collect | 08299e0 | **ACCEPT** (noise-level; restores exact ref idiom) | — |
| fix-remap-hasher | cdb9a26 | **ACCEPT** (noise-level here; wins on many-pin screens) | — |
| fix-osc-fixed-capture | 11ca425 | **ACCEPT** | osc 79→49 (−38%) — zero-alloc capture restored |
| fix-color-list-prealloc | 67962f1 | **REVERTED** (408ddaa) | zero osc gain (48.9 vs 49.2); t-stream canary −3.3% when removed |
| fix-lut-bounds | 82defe5 | **ACCEPT** | gb.table 233→216 (−7%) |
| fix-parser-destage | 2bb5c42 | **ACCEPT** | t-parser 651→522 (−20%) on C; enables the A/RF-profile 322 result |
| fix-render-soa | 9789788 (merge d211a84) | **ACCEPT** | sc.render 30.9→23.9 (−23%) |

## The LTO finding (why release-fast is overflow-checks-only)

Fat LTO + cgu=1 + panic=abort (and thin-LTO, and fat-LTO-with-do_action-demoted)
all pessimize `terminal-parser` by ~60% (322→531/500/535 ms) while helping only
`osc-parser` (−16%) and `screen-clone.clone` (−16%). Isolation:

| profile @ FINAL code | t-parser | t-stream | osc | sc.clone |
|---|--:|--:|--:|--:|
| release (checks ON, no LTO) | 328.8 | 483.8 | 57.4 | 80.0 |
| checks off only (**shipped**) | 322.0 | 449.7 | 57.8 | 84.2 |
| checks off + fat LTO + cgu1 + abort | 531.1 | 484.5 | 48.4 | 70.4 |
| … + thin instead of fat | 500.6 | 469.6 | 48.4 | 70.4 |

Hypothesis (unproven): whole-program inlining of the now-`#[inline(always)]`
per-byte parser path into the bench loop explodes register pressure. Demoting
`do_action` alone did not recover it (535). Recorded as an open item; the
Cargo.toml comment documents the measurement so nobody "helpfully" re-adds LTO.

## Remaining losses & next candidates (not attempted, by protocol)

1. **screen-clone.clone +60%** — untouched by this branch (pin-keys/hasher were
   noise). Candidates: PERF_FIX_PLAN fix 9 (`fix-cell-copy-coalesce` — masked
   u64 store instead of 3 RMW bitfield setters per cell; the overflow-checked
   Range increment is already gone in release-fast), plus the LTO clone win
   (−16%) suggests cross-function inlining in the clone chain matters — a
   targeted `#[inline]` pass on the clone path may capture it without LTO.
2. **screen-clone.render +58%** — SoA landed (−23%) but Zig's carried comment
   promises ~3×. The remaining gap is likely the per-cell managed-row loop
   (style/grapheme handling). Re-profile post-SoA before more surgery.
3. **terminal-parser +22.7%** — residual vs Zig. Candidate: PERF_FIX_PLAN t3
   `t3-action-arrayvec` (drop-glue-free `[Option<Action>;3]`: Vec payloads →
   inline ArrayVec, removing per-byte drop-glue walk of the discarded return).

## Protocol compliance

- Sequential benches only; no parallel runs. Machine gated on cordo-session
  quiet (user instruction) — primary signal session-exit OR 15 min sustained
  quiet (fired at 03:06).
- Canary (`terminal-stream`): −18.5% vs Zig at ship (no regression; the
  color-prealloc canary hit was caught and reverted).
- Functional parity: full workspace test suite green at `d211a84` (3796 passed
  / 0 failed, debug lane); the 14 release-profile failures are the PRE-EXISTING
  tripwire `cfg(debug_assertions)` design (verified untouched by this branch);
  comment-coverage delta vs merge-base = 0; acyclic gate green.
- Raw data: `~/.claude/jobs/22556231/tmp/perfrun/*.jsonl` (17 sweeps + 4
  experiments + 2 ship confirmations), copied to `.port/phase-2/perf-v2/`.

# Round 2

## r2-action-arrayvec (fix-action-arrayvec-box) — terminal-parser

Target: terminal-parser +22.7% residual (PERF_FIX_PLAN t3-action-arrayvec).
Files: `crates/rustty-vt/src/parser.rs`, `crates/rustty-vt/src/stream.rs`,
`crates/rustty-inspector/src/widgets/termio.rs` (test literals only).

Diagnosis (SHIP-RF xctrace instruction histogram of the parse_all loop, 323
samples): ~53% of the loop was drop machinery + forced array materialization —
26.9% discriminant re-loads + out-of-line `drop_in_place::<Action>` dispatch
per non-None element (incl. `Action::Print` on every plain byte), 24.5%
`[Option<Action>;3]` stack materialization (3x64B elements). LLVM cannot
SROA/DCE the discarded per-byte array because the droppy
`OscDispatch(osc::Command)` variant forces a `drop_in_place` pointer escape of
the whole alloca. A further 20.5% residual (post-fix profile) was a redundant
per-byte reload of `self.state` across the `&mut self` `do_action` call.

Fix — three coupled pieces, one commit:

1. `InlineVec<T: Copy, const N: usize>` (Copy, `{buf:[T;N], len:u8}`) replaces
   the `Vec<u8>`/`Vec<u16>` CSI/ESC/DCS payloads; capacities are the
   reference's own hard invariants (MAX_INTERMEDIATE=4 / MAX_PARAMS=24 fixed
   arrays, Parser.zig:80-84), `from_buf` is a branch-free whole-array copy
   mirroring the reference's prefix slices (Parser.zig:377-384). Prefix-only
   PartialEq/Debug — observable contents identical to the old Vec payloads.
2. Boxed dispatch variants (THE decisive piece): `CsiDispatch(Box<CSI>)`,
   `EscDispatch(Box<ESC>)`, `OscDispatch(Box<osc::Command>)`,
   `DcsHook(Box<DCS>)` — `Action` is 16B, the per-byte array 48B, and LLVM
   fully SROAs the discarded return into registers (disassembly-verified: zero
   per-byte stack traffic; dealloc/drop outlined on cold dispatch paths only).
   Allocator traffic per escape sequence goes DOWN vs SHIP (1 box vs 2
   `to_vec` allocs for CSI/DCS, 1 vs 1 for ESC; OSC exit +1 box, rare).
3. state-hoist in `Parser::next` (`let state = self.state;` for the table
   index + exit/entry compares) — removes the 20.5% store->load round-trip;
   nothing writes `self.state` before the deferred assignment, so the reshape
   is observationally identical (Zig proves the no-write whole-program).

Negative results (measured, kept for the record):

- Naive by-value `InlineVec` payloads REGRESS +25-60%: any droppy variant
  (OscDispatch) still forces full materialization of the then-192-216B array
  per byte, and the by-value layout degenerates into unaligned overlapping
  store shuffles. Boxing is what unlocks the DCE, not the inline storage.
- by-value + `repr(align(8))`: worse still (align(8) is kept on the boxed
  structs where it is harmless and matches the reference union's alignment).
- Ceiling: severing the OscDispatch construction entirely (drop-free Action)
  gives 0.14s wall (2.3x faster than Zig) — not shippable (Command owns
  unbounded Vec payloads, locked decision), but bounds the design space.

Measured (NON-OFFICIAL, machine shared, interleaved pairs x7):
terminal-parser/utf8 SHIP 0.32s -> fix 0.20s stable (Zig-equiv ~0.26);
terminal-stream canary flat (0.44-0.46 vs 0.45-0.47); osc-parser 0.05 -> 0.04
(that bench does not route through Action; delta likely the concurrent osc
reshape lane present in both builds). Official N=7 median sweep per protocol
still owed once the machine-quiet gate fires — expect terminal-parser ~200ms
vs Zig 262.9 (WIN ~-23%).

Parity: observable contents of every Action identical (InlineVec observes only
the len-bounded prefix; Box is representation only); no test assertion changed
(type-level adaptation at 6 sites: 3 `as_ref` bindings, 1 `(**cmd).clone()`,
3 `Box::new` in inspector test constructors). No unsafe added. Full workspace
suite at commit time: 3796 passed / 0 failed.

Open item: t3-osc-command-move — the OSC exit arm still deep-clones the
Command into the box (`Action::OscDispatch(Box::new(cmd.clone()))`,
parser.rs); a move-out seam on `osc::Parser::end` would drop the clone. Only
matters for OSC-heavy streams, NOT the osc-parser bench (it drives
`osc::Parser` directly).

Verified binary: `.perf-bins/rustty-bench-r2-action-arrayvec` (untracked
stash, RF profile).

# Round 3

## r3-clone-outline — screen-clone.clone (+10.5% residual)

Target: screen-clone.clone zig 51.3 vs rust 56.7 official (post-r2).
File: `crates/rustty-vt/src/page_impl.rs` (only).

Diagnosis (xctrace Time Profiler, --all-processes, 12 s windows over external
250-run loops of BOTH binaries, per-symbol self-time diff):

- `RefCountedSet::add_with_id` (style set): rust 5007 ms vs zig 4987 ms of
  ~equal sample windows — a WASH. The r2 packed `Style::eql` closed this lane;
  no remaining rust-only cost in the set itself.
- `clone_partial_row_from`: rust 3982 ms (33.8%) vs zig 3342 ms (28.9%) —
  the ENTIRE remaining gap lives in the managed-copy per-cell loop body.
- Instruction histogram (leaf-PC within the function) of the rust loop vs zig:
  rust burned ~40% of the loop's self-time on artifacts zig does not have:
  a per-iteration loop-bound spill/reload round trip (`str/ldr [sp,#0x60]`,
  464+988 ms), per-iteration `mov+movk+movk` rematerialization of the
  `copy_with_managed_reset` mask constants and the kitty-placeholder
  constant `0x43bbb8` (zig hoists both into callee-saved x20/x28), and extra
  register-restore shuffling at the loop tail. Root cause: the grapheme and
  hyperlink arms — including `cps.to_vec()`'s `__rust_alloc`/dealloc paths
  and three fully-inlined hash-probe loops — were inlined into the loop,
  and their live ranges push the loop invariants off the register file.

Fix: outline the two rare arms into `#[cold] #[inline(never)]` per-cell
helpers `clone_cell_graphemes_from` / `clone_cell_hyperlink_from` (bodies
moved verbatim; the hyperlink helper returns `bool` = "same-page fast path
taken" so the caller's existing `continue` control flow is preserved
exactly). The hot style arm stays inline. Observable behavior byte-identical:
same calls, same order, same error mapping; no test touched; no new unsafe.

Measured (NON-OFFICIAL, interleaved 20-run loops x7, same session):
zig 51.8 / r2-SHIP 57.6 / r3 54.4 ms — recovers ~3.2 of the 5.4 ms gap.
Canaries flat: terminal-stream 458→460 ms (noise), screen-clone.render
21.2→20.9 ms (noise). rustty-vt suite green (2063 passed / 0 failed);
`cargo fmt --check` and clippy clean.

Negative result (measured, reverted): restructuring
`copy_with_managed_reset` to csel-between-results (hoping for two
logical-immediate `and`s) was neutral-to-slightly-worse (54.4→54.9); LLVM
re-fuses the masks and the constants still die across the hot
`bl add_with_id` (caller-saved), so they rematerialize either way.

Remaining ~2.6 ms (+5%): per-cell rematerialization of the two constant
pairs + `ldp/ldr` base reloads after the hot `add_with_id` return (constants
live in caller-saved regs; all 10 callee-saved regs are occupied), plus the
Rust-ABI 24-byte by-value `Style` marshal to the stack per call (Zig passes
big parameters by const reference). Both are ABI/regalloc-locked without
changing the reference's `addWithId(value: T)` by-value surface — recorded
as the parity-locked floor for this bench.

PARITY FLAG (pre-existing, NOT touched by this fix): the same-page hyperlink
fast path does `continue` (page_impl.rs), skipping the style and kitty
checks for that cell; the reference's `break :hyperlink` (page.zig:926) only
exits the labeled block and still runs both. A same-page partial clone of a
cell with BOTH hyperlink and style would drop the style copy/refcount.
Needs its own R1-parity fix + test outside the perf round.

Verified binary: `.perf-bins/rustty-bench-r3-clone-outline-RF` (untracked
stash, RF profile). Working-tree change left UNCOMMITTED for round-3 review.

## r3-render (fix-render-column-hoist) — screen-clone.render (+8.6% residual)

Target: screen-clone.render zig 15.7 vs rust 17.0 official (post-r2).
File: `crates/rustty-vt/src/render.rs` (only).

Diagnosis (xctrace Time Profiler, --all-processes, identical 400-invocation
traces of BOTH binaries, per-symbol + per-instruction self-time diff):

- `RenderState::update` self-time IS the entire official gap (rust 4619 vs
  zig 3837 samples incl. memmove ≈ +1.95 ms/1000 updates); the managed
  per-cell style loop is ~94% of update self-time in BOTH binaries
  (rust 4011/4272 vs zig 3350/3520).
- RUST-ONLY cost #1: per-styled-cell bounds check in `CellList::set_style`
  (`self.style[x]`) — the style-column len re-load `ldr [x22,#0x28]` alone
  carried 630 samples (~15% of update) + cmp + b.hs. Zig writes through the
  hoisted `cells_slice.items(.style)` pointer with no check
  (render.zig:508-510).
- RUST-ONLY cost #2: per-cell re-load of the style-column base pointer
  `ldr [x22,#0x20]` — the store through `&mut self` inside the helper defeats
  hoisting; styled path was 28 instrs in rust vs 20 in zig.
- NOT the gap (shared/equal): post-store cell re-load is HOTTER in zig
  (1602 vs 827), raw-column memmove equal (347 vs 317); dirty iteration,
  update() entry, row Vec bookkeeping, allocator — negligible in both.

Fix: replace the private `CellList::set_style`/`set_grapheme` helpers (only
callers were this loop; grep-verified) with `slice_mut()` returning the three
disjoint mutable field borrows (style column, grapheme-span column, grapheme
arena) — the Rust analogue of the reference's hoisted column pointers
(render.zig:508-510). `update()` hoists them once per row before the managed
loop and pre-slices `[..n]`, so per-write bounds checks fold into the loop
bound. Pure borrow-hoist reshape: identical writes in identical order; the
`[..n]` re-slices cannot panic because `rebuild_raw` (called 3 lines above on
the same `page_cells`) enforces `style.len() == grapheme_spans.len() ==
raw.len() == page_cells.len()`, and `assert!(page_cells.len() == self.cols)`
already guards above; no unsafe; no test touched. Disassembly (prototype)
confirmed the per-cell len load + column-base load are gone (residual
register-only cmp/b.hs, harmless).

Measured (NON-OFFICIAL, interleaved 20-run batches x7 reps, rep 1 discarded
as binary-cold; medians of per-run wall time incl. process startup):
r2-SHIP 16.90 / r3 15.49 / zig 15.50 ms — closes the full +1.3 ms/1000-updates
official gap to a tie with zig. rustty-vt suite green (2039 passed / 0
failed; one parallel-run flake in `search::thread::tests::search_thread`
[mutex PoisonError], passes standalone twice, unrelated subsystem); the
`--features alloc-bench` lane also green (2039 passed); `cargo clippy
--no-deps -D warnings` and `cargo fmt --check` clean.

Verified binary: `.perf-bins/rustty-bench-r3-render` (untracked stash, RF
profile). Committed as `rustty: perf — r3-render: hoist mutable
style/grapheme column borrows in update()`.

---

# Rounds 2-3 — OFFICIAL results (idle-gated, N=7 medians, same-session Zig)

## Final standings (r3-SHIP-RF @ 10f2091 vs Zig ReleaseFast, 07:34 session)

| bench | zig ms | rust ms | Δ | verdict |
|---|--:|--:|--:|---|
| terminal-parser | 266.0 | 210.6 | **−20.8%** | WIN (was +22.7% after r1) |
| terminal-stream | 566.2 | 460.8 | **−18.6%** | WIN |
| codepoint-width.noop | 3.5 | 3.0 | −14.1% | WIN |
| codepoint-width.table | 231.0 | 217.9 | −5.7% | WIN |
| codepoint-width.simd | 503.5 | 472.8 | −6.1% | WIN |
| codepoint-width.wcwidth | 259.0 | 247.3 | −4.5% | WIN |
| grapheme-break.noop | 231.7 | 220.2 | −4.9% | WIN |
| grapheme-break.table | 256.1 | 217.5 | −15.1% | WIN |
| is-symbol.table | 238.4 | 208.8 | −12.4% | WIN |
| is-symbol.uucode | 229.9 | 213.2 | −7.3% | WIN |
| osc-parser | 57.4 | 48.5 | **−15.6%** | WIN (was tie after r1) |
| screen-clone.noop | 5.7 | 5.2 | −8.3% | WIN |
| screen-clone.clone | 51.6 | 50.5 | **−2.1%** | WIN (was +60% after r1) |
| screen-clone.render | 15.7 | 16.1 | +2.0% | STATISTICAL PARITY¹ |

**13 WINS / 1 parity / 0 LOSSES. Suite total: Zig 2916 ms vs Rust 2592 ms (−11.1%).**
Session history: pre-branch 0/1/13 → round 1 10/1/3 (−5.8%) → round 2
12/0/2 (−10.0%) → round 3 13/1/0 (−11.1%).

¹ render tie-band evidence: dedicated N=21 interleaved head-to-head gives
zig 16.33 vs rust 16.53 (+1.2%) with fully overlapping ranges (zig
15.47–21.75, rust 15.71–19.46 — Rust's worst case is BETTER). The round-3
instruction-level profile documents the residual floor as parity-locked:
caller-saved constant rematerialization across the by-value
`addWithId(value: T)` ABI surface, which the reference's API shape forces;
diverging is forbidden. Its stage binary measured −1.3% (win) in the same
session — the number is noise around zero.

## Round 2-3 fix attribution (official stage deltas)

| fix | commit | verdict | evidence (official) |
|---|---|---|---|
| r2 fix-render-undefined-cols | b6f5e4f | **ACCEPT** | sc.render 24.1→16.9 (−30%) |
| r2-action-arrayvec (boxed dispatch + InlineVec + state hoist) | e1f4829 | **ACCEPT** | t-parser 323.8→216.6 (−33%); allocator traffic per escape DOWN (1 box vs 2 to_vec) |
| r2-clone (packed Style::eql + coalesced cell copy) | ab20827 | **ACCEPT** | sc.clone 81.0→56.9 (−30%); osc 58.2→49.0 (−16%, shared RefCountedSet path) |
| r2-osc-win (fall-through success store in writeByte) | 9e386ab | **ACCEPT** | holds osc at 49 (flipped the r1 tie) |
| alloc suite (feature seam, equal-scope bench, resize seam, geometry golden) | ada0475/8fb3dd2 | **ACCEPT** | see alloc results below |
| r3-clone (outline cold grapheme/hyperlink arms) | 1abae33 | **ACCEPT** | sc.clone 57.5→53.7 |
| r3-render (hoist mutable style/grapheme column borrows) | e6f27dc | **ACCEPT** | sc.render 17.0→15.5 on its stage binary |
| **parity fix: same-page clone style drop** | 10f2091 | **ACCEPT (functional)** | reference `break :hyperlink` exits only the labeled block; port `continue`d past the style/kitty arms — cell with hyperlink+style lost its style on same-page clone. Regression test fails-before/passes-after (verified both ways). Found by round-3 instruction-level profiling. |

## Memory-allocation benchmarks — OFFICIAL (idle, both ReleaseFast-tier)

Equal-scope standalone PageList workload (init 80x24 → grow ≥2000 rows →
scroll → same-pool clone → reflow 200/40/80 → reset), zigbench rebuilt
`-Doptimize=ReleaseFast`, Rust `--profile release-fast --features alloc-bench`:

| metric | Zig | Rust | Δ |
|---|--:|--:|---|
| page allocs (full workload) | 6 | **6** | **PARITY** (old "7 vs 3" was unequal scope; fix 12 resize-seam removed the reset realloc) |
| peak outstanding page bytes | ~7,502,136 | **5,271,864** | **−30%** (geometry: port carries upstream `b953bb346` BitmapAllocator layout fix the pinned oracle predates; std page 409,600 B vs 589,824 B — documented in ALLOC_BENCH_V2.md §2, NOT a divergence bug) |
| latency (median of 9, idle) | 809 µs | **768.6 µs** | **−5% WIN** |
| peak RSS | 7,864,320 | 10,305,536 | +31% — asymmetric instrumentation: the Rust binary carries the TrackingAllocator seam (feature `alloc-bench`) the Zig side doesn't; outstanding-bytes (−30%) is the apples-to-apples allocator metric |

Per-bench peak RSS (suite, `/usr/bin/time -l`, r2-SHIP-RF): **Rust LOWER than
Zig in 7 of 8 configs** (e.g. terminal-parser 6.29 vs 7.06 MB, sc.clone 139.9
vs 157.2 MB, sc.render 8.04 vs 9.18 MB); only osc-parser higher (7.50 vs
7.01 MB, +7%).

## Round-3 honesty notes

- The r3 profiling agents measured BOTH binaries at instruction level and
  were required to declare NONE-SAFE if the residue was parity-locked; the
  clone residual (+5% at the time) was declared exactly that — then the
  parity FIX (10f2091, a real bug the profiling exposed) and measurement
  noise closed the rest.
- LTO remains banned (round-1 finding stands; re-verify t-parser if ever
  revisited).
- Raw data: `.port/phase-2/perf-v3/` (round-2 sweeps + `r3_*` sweeps).

# Round 4

## r4-render-win — screen-clone.render (mandate: decisive WIN, >= 5% vs Zig)

Target: screen-clone.render zig 15.7 vs rust 16.1 official (post-r3,
statistical parity). Round 3 documented the residual as parity-locked ONLY
for the then-current loop structure; round 4 hunted STRUCTURAL reshapes,
including optimizations Zig itself does not do (sanctioned per PORT_RULES
R10: internal reshape, byte-identical observable behavior).

Files: `crates/rustty-vt/src/ref_counted_set.rs`,
`crates/rustty-vt/src/render.rs`.

Diagnosis (xctrace leaf-PC histogram diff of BOTH binaries): BOTH engines
re-load `p.memory` + the style set's items offset per STYLED cell — the
grapheme arm's `extend_from_slice` call clobbers memory, so LLVM (and Zig's
backend, disasm: reloads at `[x19,#0x10]` / `[x19,#0x7c]`) cannot hoist
them. Rust additionally paid a per-styled-cell bounds check on the indexed
`cells_style[x]` store (`cmp x24,x23 / b.hs`, 771 leaf samples) — the r3
"residual register-only cmp/b.hs (harmless)" note was WRONG; it was hot.

Fix (r4-render-win):

1. `RefCountedSet::get` split into two additive pub unsafe methods (`get`
   itself unchanged): `items_ptr(base)` = the hoistable
   `self.items.ptr(base)` half, and `get_at(items, id)` = the per-id
   remainder carrying `get`'s EXACT asserts (id > 0, id < layout.cap,
   ref_count > 0; same inlineAssert tier, same raw-`*mut Item` SB
   discipline). Identity: `get(base, id) == get_at(items_ptr(base), id)`.
2. `RenderState::update` managed loop: hoist `addr_of!((*p).styles)` +
   `items_ptr((*p).memory_ptr())` once per row (sound: nothing in the loop
   writes through `p` — only the row's own SoA columns and `grapheme_buf`
   are written; `lookup_grapheme` is read-only); replace the indexed
   `enumerate` walk with `page_cells.iter().zip(cells_style.iter_mut())
   .zip(cells_spans.iter_mut())` so the per-write bounds checks fold into
   the shared loop bound (all three slices are exactly `page_cells.len()`,
   proven by `rebuild_raw` + the r3 `[..n]` pre-slices).

Styled path is now 16 instructions with ZERO per-cell set-base loads and
ZERO bounds checks, vs zig's 17 + 2 per-cell base reloads (disasm-verified
on the final binary). Zig does NOT do this hoist — a sanctioned R10 win
over the reference.

Killed candidates (measured/audited, for the record):

- #1 last-style memoization (VALUE cache): +42% REGRESSION (22.64 vs
  15.94) — LLVM SROAs the cached 21-byte Style into ~7 scalar GPRs; the
  hit-path store explodes into 15 scalar stores (disasm-verified).
- #1b memoization (POINTER cache): -6.8% vs zig on official corpus BUT
  regressed the all-miss adversarial corpus +6%; with items_ptr hoisted it
  became strictly worse than no-memo on BOTH corpora. Dominated; killed.
- #2 styled-span batching: killed by analysis + #1 — the per-cell
  content_tag dispatch is semantically required either way.
- #3 fuse memcpy + walk / row flag bitset: no such bitset exists in the
  reference (new state = forbidden); fusing loses the memcpy advantage the
  reference's own comment values at ~300%.
- #4 cross-update invariants: audited, no-op — capacity retained
  everywhere, nothing rebuilt per update.
- #6 layout: stride 28B in both engines; rust copies 21 value bytes vs zig
  28 — equal cost, no action.

Parity: pure loop-invariant hoist + iteration reshape; identical writes to
identical slots in identical order for ANY input; no public-contract change
(RenderState read API untouched; set methods additive); no new unsafe
discipline beyond the pre-existing raw-pointer walk. Regression guards
added (written against the PRE-reshape loop first, pass both sides):
`render::tests::mixed_styles_adjacent_cells` and
`mixed_styles_alternating_per_cell` (two ids alternating EVERY cell + a
mid-row ZWJ grapheme so `extend_from_slice` runs between style
resolutions — the exact clobber the hoist reasons about).

Measured (NON-OFFICIAL, interleaved 20-run batches x8 reps, rep 1 discarded
as binary-cold, medians of 7; commit-day sweep):

- official corpus (ascii-small.bin): zig 15.822 / r3-SHIP 16.248 /
  r4 14.518 ms = -8.2% vs zig, -10.6% vs pre; ranges disjoint (r4 worst
  15.07 < zig best 15.56). Hunt-phase sweeps agreed: -8.3%, -7.5%, -6.1%.
- adversarial corpus (`.perf-bins/corpus-alt-style.bin`, two bg styles
  alternating EVERY cell, 80 full rows — memoization/hoist worst case):
  zig 14.940 / r3 15.258 / r4 14.131 = -5.4% vs zig. No regression.
- canary screen-clone.clone (shares RefCountedSet): r3 50.98 / r4 50.75
  (flat, still beats zig 51.91; hunt-phase numbers).

Gates: rustty-vt 2042 passed / 0 failed (default AND --features
alloc-bench; the round-3 search_thread flake did not fire);
rustty-renderer 64 passed / 0 failed; clippy --no-deps -D warnings clean;
fmt --check clean.

Verified binary: `.perf-bins/rustty-bench-r4-render` (untracked stash, RF
profile) — bit-identical (sha efed4a8f) to the hunt's validated
`rustty-bench-r4-render-hoist-RF`. Official idle-gated N=7 sweep still
owed per protocol.

## r4 FINAL GATE (full-worktree, @ a26a4b1)

- `cargo build --workspace --release`: exit 0.
- `cargo test --workspace` (debug lane canonical): 113 suites, 3807
  passed / 0 failed, exit 0 (two consecutive clean full runs).
- `cargo test -p rustty-vt --features alloc-bench`: 2042 passed / 0
  failed (+ 8/13/2/1 integration suites). The KNOWN pre-existing
  round-3 flake `search::thread::tests::search_thread` (mutex
  PoisonError at search/thread.rs:2249/2093) fired on 2 of 9 parallel
  full runs and passed standalone twice — search subsystem, untouched
  by r4, same signature as the round-3 record above.
- clippy `-p rustty-vt --no-deps -- -D warnings`: clean (default AND
  `--features alloc-bench`).
- `cargo fmt -p rustty-vt --check`: clean. NOTE: workspace-wide
  `cargo fmt --check` fails in rustty-apprt / rustty-apprt-c / xtask —
  PRE-EXISTING on main (rustfmt 1.8.0-stable 2026-02-11 reflows code
  those crates were authored under an older rustfmt; reproduced in the
  clean main checkout at /Users/gael/projects/rustty). No file touched
  by this branch is affected; not introduced or fixable here.
- `check_acyclic.sh`: GREEN (graph acyclic, rustty-font has no
  rustty-vt edge).
- `check_comment_coverage.py`: cli missing=2 / inspector missing=22 /
  vt missing=2 — EXACTLY the recorded baseline, zero NEW misses.
- SHIP stash: `.perf-bins/rustty-bench-r4-SHIP-RF` rebuilt from
  a26a4b1 (`--profile release-fast`), sha efed4a8f — bit-identical to
  the validated `rustty-bench-r4-render` / `-hoist-RF` binaries.
- Gate-day confirmation sweep (NON-OFFICIAL, interleaved 20-run
  batches x8 reps, rep 1 discarded, medians of 7; machine under
  moderate load — absolute values inflated, deltas interleaved):
  zig 18.618 / r4-SHIP-RF 17.315 ms = **-7.0% vs zig**, ranges
  disjoint (r4 worst 17.494 < zig best 18.560). Consistent with the
  hunt's -8.2%. Official idle-gated sweep remains the parent's job.

# Round 5 — Lane D (clone chain)

## r5-clone-style-run — screen-clone.clone (thinnest win -> decisive)

Target: screen-clone.clone zig 51.6 / rust 50.5 official (-2.1%, the
thinnest win). Files: `crates/rustty-vt/src/ref_counted_set.rs`,
`crates/rustty-vt/src/page.rs`, `crates/rustty-vt/src/page_impl.rs`,
`crates/rustty-vt/src/page_tests.rs`. Branch base: a26a4b1 (post
r4-render-win).

Workload reality (official corpus `ascii-small.bin`): the bench setup sets
one SGR bg style then streams plain ascii, so nearly every printed cell in
every cloned row carries the SAME style id — the different-page managed
copy loop runs `add_with_id` per styled cell (~equal zig/rust cost, the
round-3 "wash"), and both engines were paying ~50 ms for what is
structurally one refcount bump per run.

Two coupled reshapes (PERF(port), byte-identical observable behavior for
ALL inputs; the same-page loop is untouched):

1. `RefCountedSet` internals by pointer: `add_context` / `lookup_context` /
   `upsert` / `insert` and new `pub(crate)` entry `add_with_id_ptr` take
   `value: *const T` (Zig passes big params by const reference; the Rust
   by-value ABI marshaled a 28-byte `Style` stack copy per call —
   the round-3 documented "parity-locked" floor, now unlocked WITHOUT
   changing the public by-value surface: `add`/`add_with_id`/`lookup` stay
   as `#[inline]` shims passing `&raw const value`). Raw pointer (not
   `&T`) so no noalias/protector is asserted (rust-mut-args discipline).
   Disasm-verified: the q-register store/reload at the clone call site is
   gone (`x2 = other_page_item_addr + 6` passed directly).
2. Style-run batching in `clone_partial_row_from_inner`'s different-page
   managed path (dst/src are distinct allocations): scan maximal runs of
   consecutive cells with no grapheme + no hyperlink (`is_style_run_cell`)
   sharing one style_id. First cell of a run = the reference loop body
   verbatim (same calls/order/error mapping -> identical error-prefix
   state). The k-1 run-mates collapse into ONE
   `use_multiple(id, k-1)` iff `add_with_id_is_pure_ref_bump` proves each
   per-cell call would take the pure "live equal item at id" fast path
   (only then is per-cell behavior exactly "+1 ref, deleted() no-op" — the
   style context's `deleted` is the trait default; the dead-item and
   foreign-item paths, which also mutate `living`/`next_id` and run the
   dead-trim, fall back to the verbatim per-cell body). The run-mate cell
   writes are then a straight `fastmem::copy` — per cell they would be
   `reset(src)` + `set_style_id(src.style_id())` which reproduces the
   source bits verbatim; machine-checked by `cell_style_run_facts`
   (corner lattice + 10k seeded-random patterns). Default-style runs are
   the degenerate no-set-calls case. Kitty placeholder row flag
   accumulated per run (write-only idempotent bit; batched portion cannot
   error). Zig does NOT do this (sanctioned structural reshape).

Regression guards (pass on BOTH the pre-reshape per-cell loop and the
reshape — verified both ways by reverting src and re-running):
`page_cloneFrom_style_run_refcounts_batched` (exact per-style refcounts =
one per destination cell, across full-row / alternating / half-row runs),
`..._conflicting_dst_id` (live non-equal item at the source id -> fallback
route, exact refcounts + untouched conflicting style),
`..._dead_dst_id` (released item at the source id -> resurrection branch
then batch, exact refcounts + `count()`).

Negative result (measured, reverted): a first-cut "plain-run memcpy +
outlined per-cell handler for every interesting cell" REGRESSED the
official corpus +28% (pre 52.5 -> 67 ms): with ~every cell styled, the
per-cell outlined call + scan overhead swamped the plain-run savings. The
style-RUN formulation is what wins.

Measured (NON-OFFICIAL, machine-shared-lock, interleaved 10-run batches,
medians; same session):

- official corpus: zig 51.3 / pre(r3-SHIP) 50.2 / r5 22.3 ms
  (**-55.6% vs pre, -56.5% vs zig**); second confirmation sweep
  zig 51.6 / pre 50.6 / r5 22.4.
- adversarial `corpus-alt-style.bin` (style alternates EVERY cell — runs
  of length 1, batching never fires): zig 155.2 / pre 184.4 / r5 167.6
  (-9.1% vs pre; the ptr-based add + run scan still pay off; remains
  behind zig on this non-official corpus as before).
- no-data (hello rows): pre 22.1 / r5 20.1 (-9%).
- siblings: screen-clone.render pre(r3) 16.5 / r5 14.9 (r4's win, present
  in base); screen-clone.noop flat.
- canaries: terminal-parser flat (206-213 both), osc-parser flat
  (49.1/49.2), terminal-stream base(a26a4b1) 447.8 / r5 452.9 (+1.1%,
  overlapping ranges; symbol-size diff shows only COLD stream handlers
  moved/shrank — `device_attributes`, `kitty_keyboard_*`, `apc_end` — the
  per-byte path is byte-identical, so this is code-layout noise for the
  official N=7 to arbitrate).

Gates: rustty-vt 2046 passed / 0 failed (default AND --features
alloc-bench); cargo check --workspace clean (20 dependent crates
re-checked); clippy --no-deps --all-targets -D warnings clean; fmt clean.

Verified binary: `.perf-bins/rustty-bench-r5-clone-style-run-RF`
(untracked stash, RF profile, sha256 22196702d5f3ea0c...). Note: the
shared disk filled to 0 mid-round; the lane worktree's target/ was
sacrificed — rebuild from the lane branch reproduces the binary.

---

# Rounds 4-5 — OFFICIAL results (idle-gated, N=7, same-session Zig, 10:39 session)

## FINAL STANDINGS (r5-SHIP-RF @ a3a2b23 vs Zig ReleaseFast)

| bench | zig ms | rust ms | Δ |
|---|--:|--:|--:|
| terminal-parser | 263.8 | 92.5 | **−64.9%** (2.9×) |
| terminal-stream | 551.5 | 343.3 | **−37.7%** |
| screen-clone.clone | 50.6 | 20.9 | **−58.7%** (2.4×) |
| screen-clone.render | 15.4 | 13.8 | **−10.1%** |
| grapheme-break.table | 256.4 | 211.4 | −17.5% |
| is-symbol.table | 238.6 | 200.5 | −16.0% |
| screen-clone.noop | 5.5 | 4.6 | −16.9% |
| osc-parser | 57.1 | 49.4 | −13.6% |
| codepoint-width.table | 234.3 | 207.9 | −11.3% |
| is-symbol.uucode | 229.9 | 206.1 | −10.3% |
| codepoint-width.wcwidth | 258.4 | 237.6 | −8.0% |
| grapheme-break.noop | 231.0 | 211.8 | −8.3% |
| codepoint-width.simd | 501.1 | 466.4 | −6.9% |
| codepoint-width.noop | 3.7 | 3.0 | −18.3% |

**14 WINS / 0 ties / 0 LOSSES. Suite: Zig 2897 ms vs Rust 2269 ms (−21.7%).**
Session history: pre-branch 0/1/13 → r1 10/1/3 (−5.8%) → r2 12/0/2 (−10.0%)
→ r3 13/1/0 (−11.1%) → **r4+r5 14/0/0 (−21.7%)**.

## Round 4-5 accepted fixes

| fix | commit | official evidence |
|---|---|---|
| r4-render-win (style-set base hoist + zip walk) | a26a4b1 | render 15.9→13.8, now −10.1% vs zig; adversarial mixed-style corpus also wins |
| r5-parser-fastpath (state-equal fast path; discarded action array statically drop-free on ~89% of bytes) | be02801 | t-parser 207→92.5 |
| r5-print-outline (cold grapheme/zero-width/wide arms) | 6ca1015 | t-stream −12% share |
| r5-stream-simd (FAIRNESS FIX: ghostty-bench builds simd=true; our bench binary ran the scalar fallback — feature enabled bench-binary-scoped) | 52116f3 | t-stream 450→343 |
| r5-utf8-dfa-pad (panic-free 512-entry pad + per-arm state stores; exhaustive differential test) | ca714e0 | decode family −3…−4% further |
| r5-clone-style-run (same-style run batching + by-pointer set adds) | 745b3c9 | sc.clone 49.9→20.9; adversarial run-length-1 corpus −9% vs pre (was already a pre-existing loss there, improved) |

## Round 5 honest outcomes

- **Lane C (osc): NONE-SAFE** — per-byte loop sits on a measured M4
  store-forward-renaming floor; four variants (hot-word, repr(C),
  state-first, cold-alloc-arm) all measured WORSE (evidence in the lane
  report). osc stays −13.6% vs zig from earlier rounds.
- **Lane B floor documented**: the ≥10% decode target is only reachable via
  a branchless DFA that LLVM DCEs in the noop lane (−94%!) but which
  serializes the loop-carried state chain in every real lane (+12…+22%
  measured) — disqualified by the no-regression rule.
- r4 negative results recorded: style value-memoization +42% (SROA store
  explosion); pointer-memoization dominated by the plain hoist.
- Lane worktrees merged and removed (A/B/D); lane C worktree left (no commits).
- Raw sweeps: `.port/phase-2/perf-v5/` (+ per-lane raw in d812712).

---

# WS7 D8 — compression benchmark numbers (2026-07-29)

Machine: Apple M4 Pro (`Mac16,7`), macOS 25.5.0. Binary:
`cargo build -p rustty-bench --profile release-fast` (the ReleaseFast-parity
profile; `overflow-checks = false`). Timing: single-process monotonic wall clock,
1 warmup + 7–9 reps, **median** — the same methodology as
`.port/phase-0/baseline/run_benchmarks.py`. `hyperfine` is **not installed** on
this host, so the AGENTS.md-preferred harness was not used; the medians below
come from the equivalent single-process harness. No benchmarks were run in
parallel.

## Corpora

| corpus | bytes | provenance |
|---|---|---|
| `.port/phase-0/baseline/corpora/ascii.bin` | 67 108 864 | phase-0 `ghostty-gen +ascii` corpus (random printable ASCII) |
| `/tmp/.../scratchpad/pages.raw` | 49 561 600 | **121 × exactly 409 600 B real page backing-memory dumps**, produced from `ascii.bin` by a scratch tool kept OUTSIDE the repo (oracle `src/benchmark/AGENTS.md`: keep large corpora outside the repository). Reproduce: parse `ascii.bin` into a 120×80 `Terminal` with `max_scrollback = 50_000_000` and concatenate `Page::memory_ptr()[..memory_len()]` for every fully-historical node. |

`pages.raw` is the input the oracle's own `PageCompression` doc-comment asks for
("a raw dump of actual page backing memory is the most representative input").

## Compression ratio

| measurement | pages | raw | encoded | ratio | savings |
|---|---|---|---|---|---|
| `+page-compression --mode=report --data=pages.raw` | 121 | 49 561 600 | 17 421 184 | **35.15 %** | — |
| `+scrollback-compression --mode=report --max-scrollback=50000000 --data=ascii.bin` | 121 | 49 561 600 | 17 421 184 | **35.15 %** | 32 140 416 (**−64.9 %**) |
| `+scrollback-compression --mode=report --data=ascii.bin` (10 MB default) | 23 | 9 420 800 | 3 311 367 | 35.15 % | 6 109 433 |
| `+scrollback-compression --mode=report --data=ascii-small.bin` | 5 | 2 048 000 | 719 967 | 35.15 % | 1 328 033 |
| `+page-compression --mode=report --data=ascii.bin` | 164 | 67 108 864 | 67 372 298 | 100.39 % | — |

The two independent paths (standalone codec over a page dump vs. the live
`PageList` over the same terminal state) agree to 4 significant figures — a
useful cross-check that neither benchmark is measuring the wrong bytes.
The 100.39 % row is **not** a scrollback ratio: random printable ASCII is
incompressible for a pure LZ77 codec, so it measures the format's
one-literal-run expansion bound. It is recorded only as the worst case.

## Codec throughput (`+page-compression`, `pages.raw`)

Per-pass cost isolated by the loop slope `(t(loops=N) − t(loops=1)) / (N−1)`,
which removes process spawn, corpus read, and (for `decompress`) the untimed
pre-encode + round-trip verification.

| mode | t(loops=1) | t(loops=41) | per pass | throughput (49.56 MB raw / pass) | stability |
|---|---|---|---|---|---|
| noop | 0.0109 s | — | — | — | stable (3.1 % spread) |
| compress | 0.0625 s | 2.0107 s | 48.7 ms | **1.02 GB/s** | **stable** — the 11-loop and 41-loop slopes agree to 0.9 % (1.027 vs 1.018 GB/s) |
| decompress | 0.0957 s | 0.6658 s | 14.3 ms | **~3.5 GB/s** | **NOISY** — 8–17 % rep spread; the 11-loop slope gives 3.12 GB/s vs 3.48 GB/s at 41 loops. Treat as **3.1–3.5 GB/s**, not a single figure. |
| store | 0.0606 s | 0.5601 s (loops=11) | 50.0 ms | 0.99 GB/s | stable (4.0 % spread) |

`store` (exact-sized encoded allocation + bounded-ring eviction) costs
**+2.6 %** over the allocation-free `compress` path — i.e. the retained
representation's allocator traffic is not a material part of compression cost.

## PageList transition cost (`+scrollback-compression`, `ascii.bin`)

This benchmark has **no `--loops`** (faithful to the oracle), so the only signal
is whole-process wall clock and the untimed setup (VT parse of 64 MiB) dominates.

| mode | 10 MB scrollback (default) | 50 MB scrollback |
|---|---|---|
| noop (setup baseline) | 0.3483 s | 0.3542 s |
| compress | 0.3596 s (**Δ +11.3 ms**) | 0.4031 s (**Δ +48.9 ms**) |
| incremental (`compress(Drain)`) | 0.3569 s (Δ +8.6 ms) | 0.4071 s (Δ +52.9 ms) |
| restore (visit every cold node through `Node::page`) | 0.3574 s (Δ +9.1 ms) | 0.4194 s (Δ +65.2 ms) |

**Measurement-honesty caveats — read before quoting these deltas:**

1. **At the 10 MB default the deltas are NOT resolvable.** Rep spread on the
   noop baseline is 3.8–4.5 %, i.e. ±14 ms on a 348 ms process — larger than the
   8.6–11.3 ms deltas themselves. The 10 MB column must not be used for
   branch-to-branch comparison; `incremental` appearing *faster* than `compress`
   there is noise, not a result.
2. **At 50 MB the deltas are resolvable** (49–65 ms against a ±13 ms band) and
   are the numbers Phase E should compare against.
3. `compress` at 50 MB: 48.9 ms for 49.56 MB = **1.01 GB/s**, matching the
   standalone codec's 1.02 GB/s to within 1 %. The `PageList` state transition,
   scratch borrow from the page pool, and `madvise` therefore add **no
   measurable cost** on top of the encoder.
4. `incremental` (drain) costs **+8 %** over monolithic `compress` — the
   candidate-bounded steps plus the final no-work verification pass.
5. `restore` (65.2 ms) is **4.6× the decoder's own 14.3 ms**. Restoration is
   dominated by `MADV_FREE_REUSE` recommit and the page faults from re-touching
   49.56 MB, not by LZ4 decoding. Worth stating explicitly so a future
   "decompression is slow" investigation starts in the right place.
6. **Peak RSS is not a reclamation metric.** `/usr/bin/time -l` shows
   `--mode=compress` at 75 399 168 B peak vs `--mode=noop` at 57 114 624 B —
   *higher*, because the 17.4 MB of encoded blocks raise the high-water mark
   while the source pages are still resident, and `MADV_FREE_REUSABLE` does not
   lower an already-set peak. The `savings=` figure the benchmark prints is
   `decommitted_raw_bytes − encoded_bytes`, an **estimate**. Confirming actual
   physical reclamation is Task E5's live check, not D8's.

## First real `madvise` execution

D8 is the first path to run `mem::decommit`'s **real** OS arms (a release binary
⇒ `cfg!(test)` is false, so the zeroing simulation is not used). Evidence that it
works on macOS: `compressed=121` in the release binary. `compress_page` returns
`false` and leaves the node resident whenever `decommit` fails, so a non-zero
compressed count *is* proof that `madvise(MADV_FREE_REUSABLE)` succeeded — 121
times. The Linux `MADV_DONTNEED` arm is **not** covered by this run (macOS only,
per D8's scope here).

## WS7 Phase C review note (M3, not yet the full E3 gate run)

`search::screen::ScreenSearch::prune_history` (called from `select` on every
navigation) is worst-case O(results × chunks_per_result × live_pages) because
`node_is_valid` walks the whole live PageList per candidate chunk. Measured:
not yet benched in isolation (Phase E's E3 perf sweep is the formal gate); this
is a **documented cost, not a regression** — `oracle@main`'s `nodeIsValid` walks
the live list the same way, so rustty is parity, not slower-than-oracle. Left
unchanged per M3 (do not alter the algorithm to chase a synthetic worst case);
flag for E3 to include a deep-scrollback + many-cached-results config in the
14-config non-regression sweep if one isn't already representative of this path.

---

# WS10 — perf series (BL-4), Layer 0 [2026-07-30]

## READ THIS BEFORE QUOTING ANY FIGURE IN THIS FILE

**Every pre-2026-07-29 figure in this file was taken on a box with four cores
pinned.** Four orphaned `yes` processes had each been burning ~99% of a core
since 10 July (~343 CPU-hours apiece) and were killed on 2026-07-29. Any number
in the earlier sections — including the **r5-SHIP record** the WS10 drift
analysis below compares against — was measured under that contention.

**The numbers in THIS section are also PROVISIONAL, for a different reason.**
The `yes` processes are gone (verified: `pgrep -fl '^yes'` → nothing), but the
box was **not idle** during WS10 execution. Instantaneous sampling (`top -l`,
*not* `ps` lifetime averages, which are misleading for long-lived processes)
showed Google Chrome pid 35028 spinning ~93-100% of a core continuously, plus
two `Virtualization.framework` VMs. The 1-minute load average ranged **4.76 to
5.80** across the WS10 session, against an `IDLE_MAX` of **2.0**.
`ws10_suite.py` correctly stamped every run `provisional` and exited **2**.

WS10 therefore **re-measured** rather than inheriting the plan's numbers, as
required — but it could not produce an *authoritative idle* baseline. The
re-measurement is still decisive for the questions asked of it; see "Why these
numbers are still load-bearing".

## Environment

| | |
|---|---|
| machine | Apple M4 Pro (`Mac16,7`), arm64, 14 logical cores |
| OS | macOS 26.5.1 (build 25F80) |
| date | 2026-07-30 |
| 1-min load average | 5.80 (Fence 1 run) / 4.76 (flip-proof run) |
| background load | Chrome pid 35028 ~93-100% of one core; 2 Virtualization.framework VMs |
| oracle binary | `.port/phase-0/baseline/bench-releasefast/bin/ghostty-bench` |
| oracle sha256 | `0fb47d268c7b9a7f9f93ed14834ca221383ba189404b69d15f750ff76abf5cdb` — verified unchanged; backup `~/rustty-oracle-artifacts/ghostty-bench-4749c4e93` verified byte-identical |
| entry binary | `.perf-bins/rustty-bench-ws10-entry-RF` sha256 `4f769059470594fcc2b2307120f150d4bb5fc0d29e7d61f96b83e5cfb3cbad2a` |
| harness | `.port/tools/perf/ws10_suite.py`, N=9 + 1 discarded warmup, paired same-session |
| corpora | 4 files at 67108864 / 67108864 / 67108864 / 262144 B — verified |

**Comparability — non-negotiable.** Every number here is comparable to oracle
revision **`4749c4e93`** and to **no other oracle revision**. The pinned oracle
**no longer builds on this host**: it requires Zig 0.15.2, and the installed Zig
is now **0.16.0**. That binary is irreplaceable — see **V15**.

## Fence 1 — ORACLE, 14 configs, N=9, paired (PROVISIONAL, load 5.80)

| bench | zig ms | rust ms | Δ | rust spread | r5-SHIP rust | drift |
|---|--:|--:|--:|--:|--:|--:|
| terminal-parser | 266.58 | 109.01 | **−59.1%** | 1.097 | 92.5 | **+17.8%** |
| terminal-stream | 543.97 | 345.87 | **−36.4%** | 1.047 | 343.3 | +0.7% |
| codepoint-width.noop | 4.55 | 3.45 | −24.0% | 1.286 | 3.0 | +15.0% |
| codepoint-width.table | 235.25 | 208.50 | −11.4% | 1.056 | 207.9 | +0.3% |
| codepoint-width.simd | 507.87 | 467.87 | −7.9% | 1.028 | 466.4 | +0.3% |
| codepoint-width.wcwidth | 261.09 | 242.81 | −7.0% | 1.083 | 237.6 | +2.2% |
| grapheme-break.noop | 235.99 | 216.62 | −8.2% | 1.040 | 211.8 | +2.3% |
| grapheme-break.table | 257.68 | 219.29 | −14.9% | 1.083 | 211.4 | +3.7% |
| is-symbol.table | 241.35 | 201.89 | −16.4% | 1.060 | 200.5 | +0.7% |
| is-symbol.uucode | 232.11 | 208.29 | −10.3% | 1.044 | 206.1 | +1.1% |
| osc-parser | 59.49 | 51.67 | −13.1% | 1.143 | 49.4 | +4.6% |
| screen-clone.noop | 6.87 | 5.53 | −19.5% | 1.257 | 4.6 | +20.2% |
| screen-clone.clone | 54.65 | 25.07 | **−54.1%** | 1.163 | 20.9 | **+20.0%** |
| screen-clone.render | 16.64 | 14.58 | −12.4% | 1.086 | 13.8 | +5.7% |

**ORACLE FENCE: GREEN — 14/14.** Thinnest margin `codepoint-width.wcwidth` at
−7.0%. Harness exit **2** (provisional), by design.

### The three questions T1 Step 5 asks

**1. Does 14/14 still hold?** **Yes.** Every strict config wins outright; no
config is within its measured spread of losing.

**2. Does the oracle column reproduce?** **Yes — and this is the load-bearing
control.** Against the r5-SHIP zig column: cw.table 235.25 vs 234.3 (+0.4%),
cw.simd 507.87 vs 501.1 (+1.4%), gb.noop 235.99 vs 231.0 (+2.2%), gb.table
257.68 vs 256.4 (+0.5%), is.table 241.35 vs 238.6 (+1.2%), is.uucode 232.11 vs
229.9 (+1.0%), osc 59.49 vs 57.1 (+4.2%), t-parser 266.58 vs 263.8 (+1.1%),
t-stream 543.97 vs 551.5 (−1.4%), sc.render 16.64 vs 15.4 (+8.1%). Ten of
fourteen within ±2.2%.

**3. Is the rustty drift real?** **The suspected drift REPRODUCES, and the
oracle control shows it is not contention.**

- `terminal-parser` **+17.8%** (plan measured +14.4%), rust spread 1.097.
- `screen-clone.clone` **+20.0%** (plan measured +20.1%), rust spread 1.163.
- `screen-clone.noop` +20.2% and `codepoint-width.noop` +15.0% — but these are
  3.4-5.5 ms process-spawn-floor configs with spreads of 1.257 / 1.286. Their
  "drift" is inside their own noise and must **not** be read as signal.
- `terminal-stream` did **not** reproduce: +0.7% here vs +5.7% in the plan.

The asymmetry the plan flagged is confirmed: in the *same session*, on the *same
box*, the oracle column reproduces its historical values to ~2% while two rustty
configs sit 18-20% above their record. **Contention cannot produce that pattern**
— it would move both columns together. The drift is rustty-side and
pre-existing. It is **not** attributable to any WS10 change: WS10 Layer 0 has
modified no hot path (its only non-doc code change is a wasm-only scalar
decoder branch).

**Bisection: NOT RUN — verification debt V17, owner WS10.** It requires building
`rustty-bench` at each post-`a3a2b23` merge point (WS5 `74226dd`, the WS7 Phase
A-E merges, WS8, WS19) and timing only `terminal-parser` and
`screen-clone.clone`. Not run in this session because at load ~5 a 12s build
competes with the measurement, and the two configs' spreads (1.097 / 1.163) are
the same order as the per-step difference a bisection must resolve. Run
contended, it would produce a confident wrong answer. Recipe in V17.

### Why these numbers are still load-bearing despite being provisional

All three questions are **comparative within one session** — rustty vs oracle
measured alternately in the same process, and rustty-now vs the historical
record with the oracle column acting as a control for the box itself. Load
shifts both columns together; it cannot manufacture a 20% one-sided gap. What
the load *does* forbid is treating any absolute figure here as a new baseline,
and attempting the bisection, which must resolve differences near the noise floor.

## Fence 2 — the LIVE FLIP PROOF (FAIL arm #3)

Dropped `features = ["simd"]` from `crates/rustty-bench/Cargo.toml:44`, rebuilt
at `--profile release-fast`, ran Fence 2 against the entry binary:

```
=== FENCE 2: SELF (candidate vs entry binary, N=9) ===
  two-strike: re-running 1 config(s) at N=18
    PASS         terminal-parser/utf8           entry=  106.46ms cand=   94.86ms (-10.9%)
    SELF-REGRESS bench terminal-stream/ascii (+8.2%): entry=345.77ms cand=373.99ms (band +5%)
    PASS         codepoint-width.noop/utf8      entry=    3.42ms cand=    3.18ms (-6.9%)
    PASS         codepoint-width.table/utf8     entry=  208.90ms cand=  211.50ms (+1.2%)
    PASS         codepoint-width.simd/utf8      entry=  472.47ms cand=  474.38ms (+0.4%)
    PASS         codepoint-width.wcwidth/utf8   entry=  240.37ms cand=  240.34ms (-0.0%)
    PASS         grapheme-break.noop/utf8       entry=  214.99ms cand=  218.34ms (+1.6%)
    PASS         grapheme-break.table/utf8      entry=  219.12ms cand=  214.34ms (-2.2%)
    PASS         is-symbol.table/utf8           entry=  202.37ms cand=  202.05ms (-0.2%)
    PASS         is-symbol.uucode/utf8          entry=  209.34ms cand=  206.73ms (-1.2%)
    PASS         osc-parser/osc-parser          entry=   52.05ms cand=   51.84ms (-0.4%)
    PASS         screen-clone.noop/small        entry=    5.65ms cand=    5.91ms (+4.5%)
    PASS         screen-clone.clone/small       entry=   24.18ms cand=   24.00ms (-0.7%)
    PASS         screen-clone.render/small      entry=   15.26ms cand=   15.35ms (+0.6%)
  SELF FENCE: RED
```

**The fence caught it**, after the automatic two-strike re-run at N=18.
Restoring `Cargo.toml` and rebuilding produced a **byte-identical** binary
(sha256 `4f769059…`, same as the entry binary) — a clean revert.

**Two findings that qualify the plan's expectation:**

1. **The magnitude is +8.2%, not the ~24% this file recorded for the same flag
   at r5.** The r5 "FAIRNESS FIX" figure **no longer reproduces**; the simd
   feature's terminal-stream effect is now about a third of what it was.
   Something between r5 and today narrowed the SIMD-vs-scalar `next_slice` gap.
   Not investigated here — recorded so nobody keeps quoting the 24%.
2. **The band was NOT loosened.** +5% still catches +8.2%, so the plan's "if it
   exits 0, tighten the band" branch was not taken. But the margin is **1.6×**,
   not the ~5× the plan assumed, and terminal-stream's measured spread is 1.047.
   **If the simd effect shrinks further this arm stops being a proof.** Whoever
   next touches the stream fast path should re-run it and, if it falls below
   ~+6%, replace the flip proof with a larger known pessimization.

Also: `terminal-parser` ran **−10.9% FASTER** with simd off. Counter-intuitive,
unexplained, not chased — recorded because someone should look.

## Gate findings

**G8's `(id, optimize)` bug — fixed, WS10 T2.** `BASELINE.json` carries 28 rows
(14 ids × {ReleaseFast, Debug}) and `bench_index()` keyed on `id` alone, so the
Debug rows silently overwrote the ReleaseFast ones. Pre-fix probe:

```
total rows: 28
indexed ids: 14
  terminal-stream/ascii    -> ('throughput_MiB_per_s', 2.0730996728385533, 'higher')
  terminal-parser/utf8     -> ('throughput_MiB_per_s', 258.1555555184457,  'higher')
  screen-clone.clone/small -> ('median_s',             5.486308874999992,  'lower')
```

ReleaseFast truths: 118.77 MiB/s, 249.83 MiB/s, 0.04935 s — so terminal-stream
was fenced **57× loose** and screen-clone.clone **111× loose**. Note
terminal-parser is the exception: its Debug row (258.2) is *stricter* than its
ReleaseFast row (249.8), which is why the bug survived spot-checking.

**G8 is STILL a weak fence after that fix — the more important finding.** G8
compares rustty against the *oracle's* numbers, and rustty is 7-60% faster, so
G8 spends that whole margin as headroom:

| config | how much slower rustty may get before G8 fires |
|---|--:|
| screen-clone.clone | **110.6%** |
| terminal-parser | **60.4%** |
| terminal-stream | **40.3%** |
| screen-clone.render | 19.7% |
| is-symbol.table | 18.8% |
| screen-clone.noop | 17.7% |
| grapheme-break.table | 17.5% |
| codepoint-width.table | 16.2% |
| osc-parser / is-symbol.uucode | 12.9% |
| codepoint-width.simd | 11.9% |
| codepoint-width.wcwidth | 11.2% |
| grapheme-break.noop (tightest) | 10.1% |

Demonstrated live: a metrics file with `terminal-stream` throughput cut **30%**
still exits **0** under the fixed, armed G8. **G8 is an oracle-parity fence, not
a regression fence.** G8b/Fence 2 is the only real regression gate. Reproduce
with `.port/tools/perf/headroom_probe.py <metrics.json>`.

**G8 is armed WITHOUT `--strict`.** This harness measures the 14 benchmarks but
not the binary-size axis `BASELINE.json` also carries, and `--strict` RED's on
those 9 absent size labels. Filed as debt (**V16**) rather than faked green —
the axis is genuinely unmeasured, and gates are not weakened to pass.

---

## WS10 T10 — the `terminal-stream` bench corpus swap, and why it did NOT move the number

**Ported:** `258de36d1` (full readonly terminal handler) + `4c5b1d5d5` (64 KiB
chunks) into `crates/rustty-bench/src/terminal_stream.rs`.

### The plan's expectation, and the measured refutation

The WS10 plan (T10 Step 5) states that after the handler swap "the medians moved
(they must — otherwise the handler swap did not take effect, which is the
vacuous-verification trap here)", and T10 Step 6 further directs that
`terminal-stream/ascii` be moved out of Fence 1's STRICT arm into a documented
`INCOMPARABLE` set because "after this task rustty measures more work than the
oracle binary does on this config".

**Both conclusions are wrong for this repo's corpus, and the corpus is the
benchmark.** Measured:

```
$ python3 -c "d=open('.port/phase-0/baseline/corpora/ascii.bin','rb').read(); \
              print(len(d), d.count(b'\x1b'), sum(1 for b in d if b<32 or b>126))"
67108864 0 0
```

`ascii.bin` is 64 MiB of **pure printable ASCII: zero ESC bytes, zero control
bytes.** Every byte is a `print` action, and `print` flows into the same
`Terminal::print` call under both the deleted print-only handler and the full
`stream_terminal::Handler` — which is exactly what the upstream commit message
says ("print throughput is identical under both handlers"). The escape-heavy
work the commit exists to start measuring is **not present in this corpus at
all**.

So:

1. **A neutral median is the CORRECT outcome here**, not evidence that the swap
   failed to take effect.
2. **`terminal-stream/ascii` remains COMPARABLE to the pinned oracle binary.**
   The pinned oracle's `TerminalStream.zig` is the pre-swap print-only form, and
   on a zero-escape corpus the two handlers are observationally identical. There
   is no extra work for rustty to be doing. It is NOT moved to an `INCOMPARABLE`
   set, and no `ACCEPTED_TRADEOFFS` entry is warranted — there is no trade.

Measured on this (loaded, load 4.57 — **provisional**) box, `+terminal-stream
--data=ascii.bin`, release-fast, 3 runs each, entry binary vs post-swap binary:

| binary | run 1 | run 2 | run 3 |
|---|--:|--:|--:|
| pre-swap (print-only handler, 4 KiB) | 0.54 s | 0.33 s | 0.35 s |
| post-swap (full handler, 64 KiB) | 0.57 s | 0.35 s | 0.34 s |

Indistinguishable, as predicted above. The first run of each is cold-cache.

### What this means for the rest of WS10

Every Layer-3 claim of the form "terminal-stream improved by X%" measured
against `ascii.bin` is measuring the **print path only** — not CSI/OSC/ESC
dispatch, not terminal state updates. The bench code now matches upstream; the
corpus does not. Regenerating an escape-heavy corpus (upstream measures against
a 2.6 GB recording of real terminal sessions) would change what
`terminal-stream/ascii` means and would break comparability with every
historical number in this file, so it is NOT done here. It is recorded as the
limiting factor on what this config can ever show.

### The swap IS proven to have taken effect — behaviourally, not by timing

`terminal_stream_uses_the_full_terminal_handler` drives `AB\x1b[2;3HX` through
the benchmark's own `step` function and asserts the cursor reached **(3, 1)**.
Mutation proof: with `HEAD`'s print-only handler restored the same test reports
**(3, 0)** — the CSI ignored, "ABX" on row 0. That is the non-vacuous check the
plan was reaching for; the timing check could not have supplied it.

---

## WS10 T12 — `screen-clone --loops`, and what it can and cannot resolve

**Ported:** `446f80f4e`'s `benchmark/ScreenClone.zig` hunk — the `loops: u32 = 1`
option and `stepRender`'s 1 000 -> `50_000 * loops` iteration count.

**NOT ported here (T19 owns them):** the three new modes `render-locked`,
`render-clean`, `render-partial`. T12 Step 2's own criterion decides this:
`grep -n "fn begin_update" crates/rustty-vt/src/render.rs` -> **0 hits**. All
three modes call `RenderState.beginUpdate`, and the begin/end split IS
`446f80f4e`'s Rust-side substance, which lands in T19. A `Mode` variant that
cannot run is worse than an absent one.

### `--loops` does NOT scale `screen-clone.clone` — so it cannot resolve that drift

T12 Step 3 directs using `--loops` with the per-pass slope method to re-check
T1's `screen-clone.clone` +20.1% drift finding. **That is not possible with this
commit.** Verified against the reference file itself:

```
$ git -C ../ghostty show 446f80f4e:src/benchmark/ScreenClone.zig | grep -n 'for (0\.\.'
160:    for (0..1000) |_| {                                  # stepNoop
171:    for (0..1000) |_| {                                  # stepClone
204:    for (0..50_000 * @as(u64, self.opts.loops)) |_| {     # stepRender
234:    for (0..50_000 * @as(u64, self.opts.loops)) |_| {     # stepRenderLocked
262:    for (0..3_000_000 * @as(u64, self.opts.loops)) |_| {  # stepRenderClean
292:    for (0..2_000_000 * @as(u64, self.opts.loops)) |_| {  # stepRenderPartial
```

`--loops` is a **render-family knob only**. `stepNoop` and `stepClone` keep
their hardcoded 1 000 iterations, so `screen-clone.noop` and
`screen-clone.clone` are byte-for-byte unaffected by this task — and the drift
question for `screen-clone.clone` remains open, owned by the deferred
measurement campaign (V17, `BEYOND-PIN`).

### The slope measurement that IS available

`+screen-clone --mode=render --data=ascii-small.bin --loops=N`, release-fast,
**loaded box (load ~4.6) — PROVISIONAL**:

| loops | wall |
|--:|--:|
| 1 | 0.45 s |
| 5 | 2.28 s |
| 9 | 4.06 s |

Slope `(t(9) - t(1)) / (9 - 1)` = **0.451 s** per loop-unit; the `(5,1)` pair
agrees at 0.458 s. One loop-unit is 50 000 `RenderState::update` calls forcing a
full rebuild, so **~9.0 µs per full-rebuild update** at 120x80 on this host,
with the process-spawn/setup floor falling out at ~0.00 s (it is ~0.45 s of pure
work even at `--loops=1`, which is the entire point of the 50x iteration bump).

That 9.0 µs figure is recorded as the **pre-`446f80f4e` rustty baseline** for
T19's headline claim ("~2.7x to ~11x less lock hold"). It is NOT compared to
upstream's table: those rows are M4 Max numbers over differently-styled content,
and the `ascii-small.bin` corpus is plain text — upstream's own "full rebuild,
plain text" row is the one at the memcpy floor and unchanged by the commit.

---

## WS10 T13/T14 — the CSI fast-path family, and why this host cannot measure it

**Ported:** `1a88f3622` (csiDispatchFinal + csi_entry fast path), `253e4f9c3`
(consumeCsiParams), `300f42c7a` (csiEntryByte hoisted into consumeUntilGround).

### Plan erratum: the task order is backwards

The plan sequences T13 (`253e4f9c3`) before T14 (`1a88f3622`). `253e4f9c3`'s
`consumeCsiParams` **calls** `self.csiDispatchFinal(c)`, which `1a88f3622`
introduces. Upstream's real order:

```
$ git -C ../ghostty log --oneline --pretty="%h %ad %s" --date=short \
    4749c4e93..origin/main -- src/terminal/stream.zig
...
300f42c7a 2026-07-06 terminal: handle CSI entry bytes inline in consumeUntilGround
253e4f9c3 2026-07-06 terminal: bulk-parse CSI parameter bytes at the slice level
1a88f3622 2026-07-06 terminal: dispatch CSI finals directly from stream fast paths
47e26df60 2026-07-06 terminal: batch printed codepoint runs into direct row fills
```

i.e. oldest-first `47e26df60` -> `1a88f3622` -> `253e4f9c3` -> `300f42c7a`.
Ported in the upstream order. (Note this also puts T16's `47e26df60` FIRST in
the family, not last — the plan's Layer-3 ordering
`253e4f9c3 -> 300f42c7a -> 1a88f3622 -> ... -> 47e26df60` is wrong in three
places.)

### These commits are UNMEASURABLE against this repo's corpora

Upstream measures them on a 2.6 GB real-session recording and on purpose-built
100 MB `csi` and `sgr` corpora, reporting +18%/+29%/+2.7% on `terminal-stream`.
rustty has neither corpus:

- **`terminal-stream/ascii`** — `ascii.bin` contains **zero ESC bytes** (see the
  WS10 T10 section above). `consume_until_ground` is never entered, so
  `consume_csi_params`, `csi_entry_byte` and `csi_dispatch_final` are never
  executed. Measured effect: none, by construction.
- **`terminal-parser/utf8`** — the `terminal-parser` benchmark drives
  `Parser::next` directly and never goes through `Stream`, so none of these
  three commits is on its path at all.
- No `csi` or `sgr` corpus exists here.

Measured anyway for the record (loaded box, **load 10.93 — provisional and not
usable as a fence**): `+terminal-parser --data=utf8.bin` 0.30 / 0.09 / 0.09 s
before and after — unchanged, as predicted.

**So the WS10 suite cannot show a win or a loss for this family, and none is
claimed.** The evidence for these three commits is fidelity plus the two
differential tests below. Generating `csi`/`sgr` corpora and re-measuring is
owned by the deferred measurement campaign (V17, `BEYOND-PIN`).

### A vacuity trap this uncovered, worth recording on its own

The first version of `stream_csi_bulk_params_match_per_byte` drove the bulk path
through `Stream::next_slice`. It was **completely vacuous**, because
`next_slice` opens with

```rust
if DEBUG || !crate::options::SIMD { for &c in input { self.next(c); } return; }
```

and `options::SIMD` is `cfg!(feature = "simd")`, **off** in a default
`cargo test -p rustty-vt`. So the "bulk vs per-byte" differential was comparing
the per-byte path with itself. It was caught by mutation, not by review: a
deliberate off-by-one in `consume_csi_params`'s final-byte return left the
**entire 2163-test suite green**. The test now calls `consume_until_ground`
directly (reproducing `next_slice_capped`'s ESC-scan loop) so it binds in BOTH
feature configurations, and the suite is run under `--features simd` as well.

Anything else in this repo that verifies a `next_slice` fast path without either
enabling `simd` or calling the inner function directly is suspect for the same
reason.
