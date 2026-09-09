# Phase-6 pin advance — gate evidence (2026-09-01)

Oracle advanced `88f57ee66` → `20abdb50a` (= `origin/main`) at Phase-6 start.
Preconditions: one foreign scratch worktree (`wt-base`, detached at `e7daf257`, clean — not a
lane), oracle clean, rustty branch `phase-6-upstream-resync` clean at `4348771d` (= `main`).

| file | state |
|---|---|
| `g3b-before-at-88f57ee66.txt` / `g1b-before-at-88f57ee66.txt` | both GREEN |
| `g1b-after-at-20abdb50a.txt` | GREEN — **by slack**: `vt_c` globs and already counts the NEW `c/search.zig` (416→433 vs rust 470); the explicit `search` row lacked the NEW `search/terminal.zig` until the coordinator extended it (then 72/72). See `../README.md` |
| `g3b-after-at-20abdb50a.txt` | RED — **51 missing**: vt 38, config 6, vt-c 4, lib 2, core 1 |

These REDs (and the hand-counted 27 range tests that the per-module floor cannot see) are the Phase-6 progress meter; both gates
MUST be GREEN — with the extended file list — at the Phase-6 finale.
