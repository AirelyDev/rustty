# Contract entries that UPSTREAM DELETED — the other half of the frozen-pin drift

Read by `.port/tools/gates/check_tests_ported.py` (G1).

`.port/phase-0/inventory/` is the port's contract, captured at the ORIGINAL pin
`4749c4e93` and deliberately never regenerated (see `CENSUS_DELTA.md` for why —
rewriting it would restate the denominator of every historical completion claim).

`CENSUS_DELTA.md` records one direction of the resulting drift: the frozen
inventory **cannot see** the 316 test blocks upstream has written since. This file
records the other direction, which the first full `ci.sh` run on merged `main`
surfaced: it **still demands** the tests upstream has since deleted.

That is the same bidirectionality `P3-W10` found for G3b — an advance both adds
and removes reference material — and it is why "an advance takes the gate to zero
by construction" was wrong there too.

Porting a test that no longer exists upstream is not fidelity; it is the opposite.
The port followed upstream correctly in every row below.

## How this is kept honest

A row here does **not** get subtracted on trust. G1 re-derives the exemption at
run time and only honours a row when **both** hold:

1. the test name is absent from the LIVE oracle tree (`../ghostty/src`), and
2. the row names a commit that removed it, reachable in `4749c4e93..6ad1fe7d8`.

So a bogus entry cannot hide a genuinely unported test: if the test still exists
upstream, condition (1) fails and the row is ignored — the gate stays RED and says
so. If someone advances the pin and upstream *restores* a deleted test, the row
stops applying automatically.

## The rows (12)

Verified 2026-07-31. Each `-S` search was run against `4749c4e93..6ad1fe7d8`.

| contract entry | deleting commit | upstream change |
| --- | --- | --- |
| `config::fromHex` | `2970e9a2a` | |
| `os::appendEnv empty` | `e8525c0fd` | env handling moved to the captured `global.environ()` |
| `os::appendEnv existing` | `e8525c0fd` | ditto |
| `os::prependEnv empty` | `e8525c0fd` | ditto |
| `os::prependEnv existing` | `e8525c0fd` | ditto |
| `os::args` | `b5053153f` | |
| `vt::HashMap ensureUnusedCapacity with tombstones` | `fedd42e8d` | backward-shift deletion in page maps — tombstones no longer exist |
| `vt::PageList compact insufficient savings returns null` | `16e4b5e98` | page ownership tracked explicitly on nodes |
| `vt::PageList compact std_size page returns null` | `16e4b5e98` | ditto |
| `vt::Terminal: scrollUp with max_scrollback zero` | `86f81fb5b` | scrollback line limit exposed |
| `vt::Terminal: scrollUp with max_scrollback zero and left/right margin` | `86f81fb5b` | ditto |
| `vt::Terminal: scrollUp with max_scrollback zero and top margin` | `86f81fb5b` | ditto |

`e8525c0fd` is the same commit behind the `TERM_PROGRAM` / `containsUnemptyConstant`
port fix, and `fedd42e8d` behind the `getOrPutAdapted` consolidation recorded in
`SYMBOL_AUDIT.md` — independent evidence that the port tracked these changes and
the contract did not.
