# Frozen phase-0 census vs the live oracle — acknowledged drift

Read by `.port/tools/gates/check_census_staleness.py` (G-CENSUS). Keep the
`` `field`: frozen -> live `` lines exactly as formatted; the gate parses them.

## The numbers

Measured 2026-10-09 by re-running the phase-0 tokenizer
(`.port/phase-0/inventory/extract_tests.py`, the *same* script, so the two sides
are comparable) against the live oracle tree at `246f70287`:

- `total_test_blocks`: 3234 -> 4497 (+1263)
- `total_assertions`: 11975 -> 18291 (+6316)
- `files_with_tests`: 279 -> 360 (+81)

| measured | pin | test blocks | assertions | files with tests |
| --- | --- | --- | --- | --- |
| 2026-07-31 | `6ad1fe7d8` | 3550 (+316) | 13533 (+1558) | 296 (+17) |
| 2026-08-28 | `88f57ee66` | 4262 (+1028) | 16926 (+4951) | 346 (+67) |
| 2026-09-01 | `20abdb50a` | 4288 (+1054) | 17127 (+5152) | 348 (+69) |
| 2026-09-28 | `12752b2ac` | 4397 (+1163) | 17717 (+5742) | 357 (+78) |
| 2026-10-07 | `a60e9e2a5` | 4487 (+1253) | 18258 (+6283) | 360 (+81) |
| 2026-10-09 | `246f70287` | 4497 (+1263) | 18291 (+6316) | 360 (+81) |

The jump is the phase-4 and phase-5 resyncs plus the gap-close advance
(`6ad1fe7d8` → `12967b68f` → `683d8db64` → `88f57ee66`, `ORACLE_SYNC.md`). The
gate went red at the first advance and stayed red until this row was written,
which is exactly the behaviour it exists for — but it also means the drift went
un-recorded across three advances, so the table above now keeps the history
rather than overwriting one line each time. The 2026-09-28 row is the Phase-7
advance (`20abdb50a` → `12752b2ac`, `P7-W0 T3`): +109 blocks, +590 assertions,
+9 files over the previous pin. The 2026-10-07 row is the Phase-8 advance
(`12752b2ac` → `a60e9e2a5`, `P8-W0 T3`): +90 blocks, +541 assertions, +3 files
(`xt_checksum.zig`, `osc/parsers/program_status.zig`, `osc/parsers/unknown.zig`)
over the previous pin. The 2026-10-09 row is the Phase-8 second advance
(`a60e9e2a5` → `246f70287`, `P8-W10 T0`): +10 blocks, +33 assertions, no new
file (DECSTR in `Terminal.zig` and `stream.zig`, origin mode in `formatter.zig`).

## What this means, plainly

G1 (test counts), G1b (per-module counts) and G2 (assertion floor) all measure
the port against `.port/phase-0/inventory/`, captured at the ORIGINAL pin
`4749c4e93`. **G2's 11975-assertion floor is 65% of the live figure** (it was 88%
at the 2026-07-31 measurement — the floor did not move, the tree did). Those three
gates can be fully green while 1263 upstream test blocks are unported, because
those tests are in neither their numerator nor their denominator.

This is the same frozen-inventory blind spot that G-FILES found at file level
(3 post-pin files nobody had examined) and G-TESTS found at test-block level
(21 unported tests in the cold-history compression series). It is not a new
class of problem — it is the third instance of one class.

## Why the inventory is not simply regenerated

The phase-0 inventory is the **contract the whole port was planned against**.
Regenerating it at the new pin would move the denominator of every historical
completion claim: "Phase 2 closed" would silently come to mean something
different from what it meant when it was asserted, and no reader could tell.
That is a worse failure than a stale number, because a stale number is at least
detectable — which is what this gate makes it.

The coverage that regeneration would have bought is instead provided by the
live-tree gates, which read `../ghostty` directly and have no frozen denominator:

| level | gate | live-tree coverage |
| --- | --- | --- |
| file | G-FILES | every oracle file has a counterpart or an argued non-port |
| symbol | G-SYMS | every oracle `pub fn` has a counterpart or a written reason |
| test block | G-TESTS | every oracle `test "..."` has a counterpart or a written reason |
| numbers | G-CENSUS (this) | the frozen-vs-live delta is acknowledged and current |

**Assertion-level coverage remains frozen-only.** G2 checks that no individual
ported test was weakened relative to its phase-0 reference, and that global
floor. There is no live-tree assertion gate, because assertions cannot be matched
across the two languages without a per-test mapping, and G-TESTS already
establishes that every live oracle test *has* a counterpart. The residual risk is
a post-pin test that is ported with fewer assertions than the oracle's — which
G-TESTS will not catch. Recorded here as a known, bounded gap rather than
implied to be covered.

## If upstream advances again

These three numbers change, this file stops matching, and G-CENSUS goes red until
someone re-measures and re-argues. That is the entire point: the drift becomes
impossible to not notice.
