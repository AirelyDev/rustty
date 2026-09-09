# Oracle sync point

**Reference oracle:** Ghostty, checked out locally at `../ghostty` (read-only reference only —
R3 forbids any build-time dependency on it).

## Pinned baseline

```
commit  20abdb50a6216c450d6d4d010c41c7edf5ab15b2   (short: 20abdb50a / 20abdb50)
date    2026-08-31
subject Update VOUCHED list (#14101)
```

There is **one** sync point. The oracle working tree is checked out (detached) at this commit, and
it is the SHA referenced by `.port/PORT_RULES.md` (R12 main-parity tracking) and
`.port/cycle-analysis.md`.

Verify with:

```sh
git -C ../ghostty rev-parse HEAD          # 20abdb50a6216c450d6d4d010c41c7edf5ab15b2
git -C ../ghostty status --porcelain      # empty
```

**This matters mechanically, not just editorially.** `.port/tools/gates/check_comment_coverage.py`
(G3b) and `check_comments_fidelity.py` (G3) read a **directory**, not a SHA: `/Users/gael/projects/ghostty/src`.
Editing this file without moving the working tree changes nothing. Neither gate accepts a
`GHOSTTY_SRC` env override — that override was deleted by `P3-W0 T2 Step 6` because a redirectable
oracle path is the spoof hole `.port/tools/ci.sh:114-129` documents for the test-count gate (point it
at an empty tree and every gate finds zero reference sentences, i.e. vacuous green).

## History of this pin

| advanced | from | to | by |
|---|---|---|---|
| 2026-07-31 | `4749c4e93` (2026-06-20) | `6ad1fe7d8` (2026-07-29) | `P3-W10 T3`, Phase 3 upstream resync |
| 2026-08-17 | `6ad1fe7d8` (2026-07-29) | `12967b68f` (2026-08-17) | Phase 4 upstream resync (advance-first by explicit user direction; before/after gate states in `.port/phase-4/w0/`) |
| 2026-08-25 | `12967b68f` (2026-08-17) | `683d8db64` (2026-08-25) | Phase 5 upstream resync (advance-first, same doctrine; before/after gate states in `.port/phase-5/w0/`) |
| 2026-08-26 | `683d8db64` (2026-08-25) | `88f57ee66` (2026-08-25) | Gap-close drift absorb (advance-first): the 2-commit `stream.zig` ground-state tail — `0f35043c9` (execute C0 0x10-0x1F) + `40a40f848` (ignore UTF-8-decoded C1). See `.port/gap-close/`. |
| 2026-09-01 | `88f57ee66` (2026-08-25) | `20abdb50a` (2026-08-31) | Phase 6 upstream resync (advance-first, same doctrine; before/after gate states in `.port/phase-6/w0/`). 121 commits, 74 non-merge, 75 files, +6,261/−1,269. |

**Phase-4 note on ordering.** Unlike `P3-W10`, this advance happened at the *start* of the
resync (explicit user direction: update the oracle, then port the whole range). The
tree-reading gates therefore measure against the TARGET during Phase-4 porting:
G1b recorded RED with 10 modules / ~36 dropped tests (`w0/g1b-after-at-12967b68f.txt`) and
G3b's missing counts grew by the range's new reference text (`w0/g3b-after-at-12967b68f.txt`).
Those REDs are the honest progress meter for Phase 4, not regressions; both were GREEN at
`6ad1fe7d8` immediately before the advance (`w0/*-before-at-6ad1fe7d8.txt`). They MUST be
green again at the Phase-4 finale.

**FINALE (2026-08-25): re-greened.** G1b GREEN (every module rust ≥ zig; the 10 RED rows
closed by their owning waves). A5 all arms GREEN (194 oracle exports both directions, 0
residuals). A3 re-pinned GREEN. G-BENCH/G-THEMES/G7/D11 GREEN. G3b finale sweep recorded at
`w0/g3b-finale-at-12967b68f.txt`. The absorbed range's execution record is
`docs/superpowers/plans/2026-08-17-phase4-upstream-resync.md` + the `phase-4 *` commit series.

**PHASE-5 FINALE (2026-08-25): re-greened.** G1b GREEN (295 dropped tests closed by their
waves). A5 all arms GREEN (195 exports, 613 discriminants). A3/A4/D11/D11r GREEN;
menu-shortcut parity 51 actions byte-identical. Both platforms: macOS workspace 5,343 tests
+ xcodebuild RusttyTests 356/0; Linux (`rvm`) full lane 0 FAILED. G3b finale sweep recorded
beside the advance evidence in `.port/phase-5/w0/`.

**PHASE-6 FINALE (2026-09-01): re-greened.** G1b GREEN with the `search` row extended to the
NEW `search/terminal.zig` (search zig 72 / rust 77; vt_c 433 / 487 — the NEW `c/search.zig`
was already globbed). G3b GREEN (0 missing in every crate; arrival was 51). Live per-test audit
(`audit_test_coverage.py`) GREEN: 4,090 live-oracle test blocks, 0 unexplained — the range's
27 tests (c/search 17, search/terminal 6, graphics_image 2, ref_counted_set 1, Terminal 1) are
all present by name. A5 headers GREEN (203 exports both directions, 0 residuals); G-VTFEAT
GREEN with `search` as the 10th oracle feature; symbol manifest 91 == 91; menu-shortcut parity
51 actions byte-identical. macOS xcodebuild RusttyTests **403 passed / 0 failed** (356 + 47
new: KeyEventSynthesize 42, KeyboardShortcutTranslation 1, URLTests 4). Census ledger moved to
`20abdb50a` (G-CENSUS was RED by design after the advance). **Both platforms
exercised:** the Linux (`rvm`) lane ran `cargo test --workspace` GREEN and
G-GTKFEAT GREEN (the GTK gate macOS can only report BLOCKED), with a post-fix
G-REL of 5,477 passed / 0 failed; it also exposed three PRE-EXISTING
Linux-only defects that macOS structurally cannot see — the 25 examples'
`xcrun`/`set -e` abort, a `cfg(not(macos))` clippy break, and a G-REL
false-RED on a passing test's stderr — all fixed and re-verified
(`d38b014b`, `d591a985`; see `.port/phase-6/README.md`
§"Linux-only defects found by this lane"). Execution record:
`docs/superpowers/plans/2026-09-01-phase6-upstream-resync.md` + `.port/phase-6/assessments/W1..W6`
+ the `phase-6/*` merge series. CI-lane results are recorded in `.port/phase-6/README.md`
§"Finale".

The previous pin, `4749c4e93`, was a **stale baseline**: work from beyond it had been ported by
WS5–WS8 and tracked in an ad-hoc "beyond-pin series" table (BL-1…BL-7) that existed only because the
pin had not moved. That table is **gone**. Its contents were either absorbed by the advance (the
series are ancestors of `6ad1fe7d8`) or, where genuinely still open, live as normal ledger rows with
a named owner.

## Advancing the pin

Advancing the pin means `git checkout` in the oracle repo, and that is the one irreversible act in
the port. The standing rule is **never `git checkout` the oracle**: every gate and every concurrent
agent depends on that tree standing still. Advancing therefore requires:

1. **No other lane is active.** Verify with `git worktree list` and a per-worktree `git status`.
2. The oracle tree has **no modified files** (`git -C ../ghostty status --porcelain`).
3. A recorded **before** state for the tree-reading gates — at minimum G3b
   (`check_comment_coverage.py`) and G1b (`check_test_counts.py`).
4. A recorded **after** state and a per-crate delta in **both** directions.
5. `.port/ORACLE_SYNC.md`, `.port/PORT_RULES.md` (R12) and `.port/cycle-analysis.md` updated
   together, and a commit message stating which range the advance absorbed.

**Advancing is bidirectional — do not model it as "the stale reference sentences disappear".**
`P3-W10 T3` measured this. The gates re-read the *same pinned file list* (from the phase-0
`crate_map`) at the new SHA, so an advance simultaneously:

* **removes** reference text upstream deleted in the range (G3b `config` went `missing=30 -> 4`;
  `os` `35 -> 12`; `termio` `23 -> 14`; `bench` `8 -> 5`), and
* **adds** reference text upstream *wrote* in the range, which is missing unless it was ported
  (`vt` `116 -> 218`, `global` `0 -> 31`, `unicode` `0 -> 20`, `cli` `2 -> 11`, `vt-c` `4 -> 10`,
  `lib` `0 -> 5`, `simd` `0 -> 4`).

G3b's total therefore went **218 -> 334**, not to zero. See `.port/phase-3/w10/` for the full
before/after reports, the per-crate delta table, and the per-file attribution of all 334.

## Upstream beyond this pin

At the time of the Phase-6 advance (2026-09-01) the pin IS `origin/main` — there is no
beyond-pin gap. Phase 6 absorbs `88f57ee66..20abdb50a` (121 commits, 74 non-merge, 75 files,
+6,261/−1,269; artifacts in `.port/phase-6/`); the gap-close absorbed `683d8db64..88f57ee66`
(`.port/gap-close/`); Phase 5 absorbed `12967b68f..683d8db64` (`.port/phase-5/`); Phase 4
absorbed `6ad1fe7d8..12967b68f` (`.port/phase-4/`). Any future upstream movement is the seed
of the next resync.

```sh
git -C ../ghostty fetch origin main          # once
git -C ../ghostty show origin/main:<path>    # current upstream state of a file
git -C ../ghostty show <hash>                # a specific beyond-pin commit
```
