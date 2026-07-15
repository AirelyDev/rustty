# WS9 — ledger hygiene: DEFERRALS.md drift reconciliation + stale port-status comments

Status: **VERIFIED ALREADY RESOLVED**, with 2 small residual stale-comment
findings (not in the two files the task named as examples, but caught by the
"and anywhere else" clause).

## Method

1. Read `.port/phase-2/GAP_ANALYSIS.md` §6 ("Untracked gaps & ledger drift")
   and the corrected workstream table at its top (which itself claims WS9 was
   done 2026-07-10, T14).
2. For every item GAP_ANALYSIS §6 names as missing-from-ledger, `grep`'d
   `.port/phase-2/DEFERRALS.md` (4033 lines) for a corresponding entry and read
   it in full to confirm it's substantive, not a stray keyword hit.
3. For the app-core over-claim and the terminal.rs/screen.rs stale-comment
   claims, read the actual current file headers and diffed their prose
   against ground truth (grep for real `todo!()`/`unimplemented!()` call
   sites, not just the word appearing in a comment).
4. Cross-checked `git log` on the specific files to confirm the reconciliation
   commits exist and are the most recent touches (i.e. nothing has drifted
   again since).
5. Swept every crate for `PORT STATUS: ... IN PROGRESS` / `partial` headers
   and manually adjudicated each against its crate-root header and DEFERRALS.md
   to find any OTHER stale claims beyond the two files named in the prompt.

## Finding: (a) "real deferrals with no ledger entry" — ALL ALREADY RECONCILED

Every item GAP_ANALYSIS §6 flags as untracked now has a substantive
DEFERRALS.md entry. This was done in commit `6386a88` ("ledger: DEFERRALS.md
reconciled to zero phantom open seams — ... T14, VALIDATION_REPORT M1-M5
§3b") 2026-07-10, with follow-on additions through 2026-07-14 (`9a5d616` most
recent touch). Verified line-by-line:

| §6 item | DEFERRALS.md anchor | Verified content |
|---|---|---|
| `src/helpgen.zig` (190 LOC, "in no file list") | DEFERRALS.md:2616-2629 | `help_strings`/`Action::help_text` entry explicitly names "R1 port of `src/helpgen.zig` + `HelpStrings.zig`" as the `rustty-helpgen` leaf — tracked, RESOLVED. |
| `src/terminal/apc/glyph.zig` (module root omitted from inventory) | DEFERRALS.md:177-189 | "Deferred: apc cluster (font-blocked) — RESOLVED (Stage A.3)" explicitly says "`apc/glyph/mod.rs` now carries the verbatim `glyph.zig` header + re-exports." |
| `rustty-wuffs/src/swizzle.rs:26,39,51,63,75` (5 unreached swizzlers) | DEFERRALS.md:3741-3761 | "`rustty-wuffs` swizzle — 5 unreached pixel-format seams [added 2026-07-10, T14 reconciliation]" — all 5 fns named, same "unreached, sole consumer never constructs the variant" rationale as GAP_ANALYSIS. |
| `rustty-z2d` subset guards (painter.rs:128,322; stroke_plotter.rs:46; polyline_plotter.rs:47; surface.rs:355,392) | DEFERRALS.md:3762+ | "rustty-z2d subset guards — 6 by-design `unimplemented!()` arms [added 2026-07-10, T14 reconciliation]" |
| `src/synthetic/**` (1157 LOC, "ledger completely silent") | DEFERRALS.md:3368-3396 | "Sub-project #3 (unused-subset): `src/synthetic/**` — RESOLVED (ported 2026-07-09); 4 binary-entry seams tracked [reconciled 2026-07-10, T14]" — full entry, 9-file breakdown, un-defer trigger documented. |
| `main_gen.zig`/`main_wasm.zig`/`main_build_data.zig` (app-core over-claim) | DEFERRALS.md:2956-2965 (C6) + 3515-3528 (correction note) | C6 entry defers all three explicitly; a dedicated "App-core over-claim CORRECTION" section at 3515 states the inventory's "done" claim was wrong and points back at C6. **This is the (b) app-core over-claim item from the task — already corrected, not open.** |
| Benchmark harness (`src/benchmark/**`, `main_bench.zig`, `ghostty_benchmark_cli`) | DEFERRALS.md:3397-3418 | "Sub-project #3 (unused-subset): `src/benchmark/**` + `main_bench.zig` + `ghostty_benchmark_cli`" — 12 files ported, 8 tests, 2 residual sub-seams (os_signpost, C-name alias) explicitly tracked. |
| `apprt-gtk` subsystem (~52 files, ~22k LOC) | DEFERRALS.md:3496+ | "Sub-project #4 (gtk): the `apprt-gtk` subsystem (`src/apprt/gtk/**`)" — dedicated entry exists (not a full port, but the *deferral itself* is now ledgered, which is what §6 asked for). |
| `ghostty_*`→`rustty_*` alias shim + signed `.xcframework` repackaging | DEFERRALS.md:3468-3494 | Two dedicated entries: "the `ghostty_*`→`rustty_*` C-ABI compatibility ALIAS layer" and "signed `.xcframework` repackaging xtask". |
| `src/build/mdgen/**` + `src/build/webgen/**` | DEFERRALS.md:3440-3452 | Dedicated low-priority entry exists. |
| `src/stb/stb_image_resize.h` | DEFERRALS.md:3453-3467 | Dedicated entry; confirms `rustty-font` does not yet cover it, gives the `cc` build.rs plan. |

**Conclusion for (a) and (b): zero remaining edits needed.** GAP_ANALYSIS.md's
own header (lines 1-14) already flags itself as superseded/stale relative to
DEFERRALS.md — it is a frozen 2026-07-08 snapshot describing a gap that was
closed within 48 hours of being written. No new DEFERRALS.md entries or
corrections are required by this workstream; the ledger is current.

## Finding: (c) stale port-status header comments

The task named `crates/rustty-vt/src/terminal.rs` and `screen.rs` as
candidates. Both were checked and are **already accurate**:

- `crates/rustty-vt/src/terminal.rs:9-11` — `PORT STATUS: done (vt-12b) —
  struct, method bodies, and tests are ported and filled; no `todo!()` bodies
  remain (the only rustty-vt seam left is the dcimgui-gated `imguiKey` in
  `input/key.rs`).` — matches reality (`terminal-vt` is GAP_ANALYSIS's
  "complete" subsystem).
- `crates/rustty-vt/src/screen.rs:5-6` — `PORT STATUS: done (vt-11) — Screen
  struct + nested types + method bodies are ported and filled; no `todo!()`
  bodies remain.` — also accurate.

**No edit needed on either named file.**

However, a repo-wide sweep for `PORT STATUS: ... IN PROGRESS` / crate-header
vs. sibling-header mismatches (the "and anywhere else" clause) turned up two
real, still-uncorrected stale headers:

### Edit 1 — `crates/rustty-vt-c/src/terminal.rs:5-16` (stale, HIGH confidence)

Current text (lines 5-16):
```
//! PORT STATUS: IN PROGRESS (mutual-atom Stage B v4 — D3 SEAM landed). The D3
//! trampoline seam + `ZigTerminal` lifecycle machinery are filled: `zig_terminal`,
//! `new`/`new_`/`free` (Box-based lifecycle), `vt_write`, `resize`, `reset`,
//! `mode_get`/`mode_set`, `scroll_viewport` (the `ScrollViewport` `[2]u64` union
//! dispatch), `get`/`get_multi`/`get_typed` (the non-deferred-type arms), and the
//! 9 `Effects` trampolines (each recovers the owning `TerminalWrapper` from the
//! stream handler via the exposed-provenance `container_of` and calls the C
//! fn-ptr). The get/set + grid_ref sub-batches have LANDED (VALIDATION_REPORT
//! M3): `set`/`setTyped`, every `get_typed` arm, and
//! `grid_ref`/`grid_ref_track`/`point_from_grid_ref` are live — zero
//! `todo!()` remains in this crate. The formerly irreducible mutual cluster
//! (terminal<->grid_ref<->grid_ref_tracked<->selection) is fully resolved.
```

**Why stale:** the header's own tag line says "IN PROGRESS" but its own body
(last 2 sentences) says "zero `todo!()` remains in this crate" and "fully
resolved". Cross-checked against:
- `crates/rustty-vt-c/src/lib.rs:8-12` (crate root): `PORT STATUS: done
  (flipped per VALIDATION_REPORT.md M3: all 34 c/ modules are landed ...
  with zero live todo!(); completion ledgered in DEFERRALS.md under "vt-c")`.
- `DEFERRALS.md:150-157`: `**STATUS 2026-06-25 — rustty-vt-c COMPLETE (Stage
  B DONE).**`
- Every sibling file in the crate (`grid_ref.rs`, `selection.rs`, `point.rs`,
  `row.rs`, `cell.rs`, `style.rs`, `render.rs`, `selection_gesture.rs`) says
  `PORT STATUS: FILLED` or `done`; `terminal.rs` is the only holdout still
  reading `IN PROGRESS`.
- `git log --oneline -- crates/rustty-vt-c/src/terminal.rs` shows the last
  substantive commit (`ae70b82`, a rebrand sweep) predates none of the
  completion commits — the header was never flipped after Stage B landed.
- Literal grep: the only occurrence of the string `todo!()` in the file is
  inside this very comment (mentioning its absence); `grep -c
  "unimplemented!()"` = 0.

**Proposed edit:** change the tag from `IN PROGRESS (mutual-atom Stage B v4 —
D3 SEAM landed)` to `done (mutual-atom Stage B v4 — D3 SEAM + get/set +
grid_ref sub-batches all landed; zero live todo!())`, keeping the existing
descriptive body (it is accurate) — i.e. a one-line tag fix, not a rewrite.

### Edit 2 — `crates/rustty-font/src/opentype/mod.rs:5` (stale, MEDIUM confidence)

Current text:
```
//! PORT STATUS: in progress (sfnt, head, glyf, hhea, os2, post, svg)
```

**Why stale:** all 7 named submodules independently carry `PORT STATUS: done`
headers (`sfnt.rs:5`, `head.rs:5`, `hhea.rs:5`, `os2.rs:5`, `post.rs:5`,
`svg.rs:5`, `glyf.rs:5` all say `done`). The parenthetical in the parent
header lists exactly these 7 files as if they were the still-open work items,
but each is individually complete. This is lower severity than Edit 1 (the
crate root `rustty-font/src/lib.rs:7` correctly says `in progress` for the
*crate as a whole*, which remains true — fontconfig/stb_image_resize/wasm
gaps are real and tracked — so no functional claim is broken), but the
submodule aggregator header itself is misleading in isolation.

**Proposed edit:** change to `PORT STATUS: done (sfnt, head, glyf, hhea, os2,
post, svg — the full pure-parsing OpenType slice is ported and filled; the
crate-level `in progress` in `lib.rs` reflects other rustty-font modules, not
this one)`.

## Full exact edit list

| # | File | Line(s) | Action |
|---|---|---|---|
| 1 | `crates/rustty-vt-c/src/terminal.rs` | 5 (tag only; body 6-16 unchanged) | Flip `PORT STATUS: IN PROGRESS (...)` → `PORT STATUS: done (...)` |
| 2 | `crates/rustty-font/src/opentype/mod.rs` | 5 | Flip `PORT STATUS: in progress (...)` → `PORT STATUS: done (...)` |

No `DEFERRALS.md` line additions, deletions, or "mark-resolved" edits are
needed — every item GAP_ANALYSIS §6 lists is already present and accurate as
of the 2026-07-10 (T14) reconciliation commit `6386a88` plus follow-ups
through `9a5d616` (2026-07-14). No edits needed to
`crates/rustty-vt/src/terminal.rs` or `crates/rustty-vt/src/screen.rs` — the
two files the task named as candidates are already correct.

## Risk / residual note

This verification was targeted (GAP_ANALYSIS §6 items + a header-string sweep
for `IN PROGRESS`/stale-skeleton claims), not a from-scratch re-run of the
18-agent `full-gap-analysis` workflow. It is possible a handful of other
crate-internal module headers drifted in the same way as the two found above
(a submodule header not flipped when its sibling files completed) without
tripping the `IN PROGRESS` grep (e.g. if worded differently). A full sweep
would require grepping every `PORT STATUS:` line against actual
`todo!()`/`unimplemented!()` call-site counts per file — out of scope for
this ledger-hygiene pass but flagged as a possible future micro-workstream if
desired.
