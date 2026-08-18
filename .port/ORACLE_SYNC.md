# Oracle sync point

**Reference oracle:** Ghostty, checked out locally at `../ghostty` (read-only reference only —
R3 forbids any build-time dependency on it).

## Pinned baseline

```
commit  12967b68f7d46bdbfb2cfffb6768332fb9db68c0   (short: 12967b68f / 12967b6)
date    2026-08-17
subject libghostty: option to retain continuations on snapshot decode (#13878)
```

There is **one** sync point. The oracle working tree is checked out (detached) at this commit, and
it is the SHA referenced by `.port/PORT_RULES.md` (R12 main-parity tracking) and
`.port/cycle-analysis.md`.

Verify with:

```sh
git -C ../ghostty rev-parse HEAD          # 12967b68f7d46bdbfb2cfffb6768332fb9db68c0
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

**Phase-4 note on ordering.** Unlike `P3-W10`, this advance happened at the *start* of the
resync (explicit user direction: update the oracle, then port the whole range). The
tree-reading gates therefore measure against the TARGET during Phase-4 porting:
G1b recorded RED with 10 modules / ~36 dropped tests (`w0/g1b-after-at-12967b68f.txt`) and
G3b's missing counts grew by the range's new reference text (`w0/g3b-after-at-12967b68f.txt`).
Those REDs are the honest progress meter for Phase 4, not regressions; both were GREEN at
`6ad1fe7d8` immediately before the advance (`w0/*-before-at-6ad1fe7d8.txt`). They MUST be
green again at the Phase-4 finale.

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

At the time of the Phase-4 advance (2026-08-17) the pin IS `origin/main` — there is no
beyond-pin gap. The absorbed range was `6ad1fe7d8..12967b68f` (543 commits, 359 non-merge,
381 files, +72,554/−8,106); its artifacts live in `.port/phase-4/`. Any future upstream
movement is the seed of the next resync.

```sh
git -C ../ghostty fetch origin main          # once
git -C ../ghostty show origin/main:<path>    # current upstream state of a file
git -C ../ghostty show <hash>                # a specific beyond-pin commit
```
