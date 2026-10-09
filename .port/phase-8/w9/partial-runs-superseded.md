# Partial W9 validation runs (superseded, kept)

The user stopped these runs on 2026-10-09 to add upstream scope: a second
oracle advance past `a60e9e2a5`, then one final validation at the very end.
Nothing here is a verdict for the final head. The archive
`partial-runs-superseded-2026-10-09.tar.gz` keeps them verbatim:

- `partial-mac-26d23100b/`: three Mac `ci.sh --fast` logs at `26d23100b`.
  - Run 1 is partial: it was stopped at the disk floor.
  - Run 2 used a custom `CARGO_TARGET_DIR`, and 5 gates failed for that reason.
  - Run 3 used the default dir and was red only on G-SKIP (N544, fixed in
    `b1ac88552`) and on G-LINT (it scanned a Miri target dir).
  - `findings-delta.md`: the runner's notes.
- `partial-rvm-c0adec970/`: the rvm full `ci.sh` at `c0adec970`. Every gate
  before G-VALGRIND passed; it was stopped at the single-core inline Valgrind.
- `partial-rvm-0175bc789/`: the rvm delta lanes at `0175bc789`.
  - GREEN: build, clippy, the changed-crate tests (including GTK),
    G-GTKFEAT, ASan, LSan, G-SAN-SELF, and vt Valgrind (scoped, 206 tests).
  - TSan was green on renderer and input, and every other lane was stopped
    mid-run. The runner scripts are kept as `*.sh.txt`.

Mac Miri verdicts reported by the runner before the stop (also in the SDD
ledger), at `26d23100b`:

| Lane | Scope | Result |
|---|---|---|
| G6 SB | bench | 38/38 |
| G6 SB | vt-c | 586/586 |
| G6 SB | vt (page, print, effects) | 221/221 |
| G6 ASan | input | 126 |
| G6 ASan | renderer | 146 |

`rustty-core` SB and G6t were stopped mid-run.
