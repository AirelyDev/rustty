# Oracle sync point

**Reference oracle:** Ghostty, checked out locally at `../ghostty` (read-only reference only —
R3 forbids any build-time dependency on it).

## Pinned baseline

```
commit  4749c4e93731067049bfbf2e4572061cef2bdd17   (short: 4749c4e93 / 4749c4e)
date    2026-06-20
subject i18n(kk): translate missing "Close Split" string (#13052)
```

This is the **baseline** the port was taken against: every ported file was ported against the tree
as of this commit, and it is the SHA referenced by `.port/PORT_RULES.md` (R12 main-parity tracking)
and `.port/cycle-analysis.md`.

Verify with:

```sh
git -C ../ghostty log -1 --format='%H %ad %s' --date=short 4749c4e93
```

## Important: the pin is a baseline, NOT the boundary of what has been ported

As of 2026-07-29 the oracle's `origin/main` is **365 commits ahead** of this pin, and several
workstreams have deliberately ported selected commits from *beyond* it. Reading only the pinned
checkout will therefore give you a **stale** picture of what the port targets.

Beyond-pin work ported so far (see `.port/phase-2/MAIN_PARITY.md` for the authoritative
per-commit ledger with ported/missing/partial verdicts):

| Series | What | Status |
|---|---|---|
| BL-5 | lib-vt C-API expansion (colors, color-scheme, OSC/Kitty queries, unicode width, scroll-to-row, OSC-52 clipboard) | ported (WS5) |
| BL-7 | OSC 72 kitty drag-and-drop parser | ported (WS8) |
| BL-3 | 12 runtime-safety bugfix commits | ported (WS8) |
| BL-2 | free-list page memory return-to-OS (`madvise`) | ported (WS8), later consolidated into `mem.rs` by BL-1 Phase B |
| BL-6 | kitty/gfx generation stamps | ported (WS8) |
| BL-1 | scrollback page compression (LZ4 + compressed `Page` + generation stamps) | Phases A/B/C done, D/E in progress (WS7) |

**Consequence for anyone reading the oracle:** commits in these series are **not** in the pinned
checkout. Read them explicitly from `origin/main`:

```sh
git -C ../ghostty fetch origin main          # once
git -C ../ghostty show origin/main:<path>    # current upstream state of a file
git -C ../ghostty show <hash>                # a specific beyond-pin commit
```

Never `git checkout` the oracle — other agents and gates depend on the working tree staying at the
pinned baseline.

## Advancing the pin

Do not bump this SHA casually. Advancing it means re-triaging every intervening upstream commit
through `.port/phase-2/MAIN_PARITY.md` (R12) and re-running the parity gates; the current process is
to port selected series from `origin/main` and record them in that ledger while the baseline stays
put. If the pin is ever advanced, update this file, `.port/PORT_RULES.md` (R12), and
`.port/cycle-analysis.md` together, and state in the commit message which commits the advance
absorbed.
