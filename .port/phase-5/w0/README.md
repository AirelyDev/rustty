# Phase-5 pin advance — gate evidence (2026-08-25)

Oracle advanced `12967b68f` → `683d8db64` (= `origin/main`) at Phase-5 start, minutes after
the Phase-4 finale re-greened everything. Preconditions: no worktrees, oracle clean, rustty
`main` clean at `d63dd9fc`.

| file | state |
|---|---|
| `g3b-before-at-12967b68f.txt` / `g1b-before-at-12967b68f.txt` | both GREEN (the Phase-4 finale state) |
| `g1b-after-at-683d8db64.txt` | RED — 5 modules, **295 dropped tests**: kitty 202, osc 47, stream_terminal 38, input_paste 7, parser 1 |
| `g3b-after-at-683d8db64.txt` | RED — **1,536 missing**: vt 1,217, core 89, vt-c 78, simd 74, termio 63, config 13, os 2 |

These REDs are the Phase-5 progress meter (advance-first doctrine, `.port/ORACLE_SYNC.md`);
both MUST be GREEN at the Phase-5 finale.
