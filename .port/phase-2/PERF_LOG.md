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
