# Test-level audit — accepted absences

Companion to `.port/tools/audit_test_coverage.py` (pass 3). The tool reads
`test "name" {` blocks out of the **LIVE** oracle tree — unlike G1, which reads
the phase-0 inventory frozen at `4749c4e93` and therefore cannot see a single
test upstream added since.

**3390 live-oracle test blocks across 485 files.** Two were genuinely unported
and have now been written (see below); the rest have counterparts.

## Ported as a result of this audit

- `src/terminal/PageList.zig::PageList cold compression continues after an incompressible page`
- `src/terminal/PageList.zig::PageList compression leaves incompressible pages resident`

Both from `9156ada16` "terminal: add cold-history compression pass" — a commit
`MAIN_PARITY.md:158` still lists as `missing BL-1`, one of the ~15 stale
compression rows `P3-W10 T2` flagged as unreliable rather than striking blind.
The *behaviour* was implemented: `compress_page`'s two `return false` arms leave
the node untouched. What was absent was any assertion that a failed candidate
**does not abort the pass** and **stays a candidate for later passes**, so a
change that bailed on first failure would have been invisible to the other eight
compression tests.

Both are mutation-proved rather than merely passing: replacing the pseudo-random
fill with zeros (making the page compressible) fails `assertion failed:
!s.compress_page(node)` and, in the two-page test, the `memory_stats` equality —
so they key on incompressibility, not on incidental state.

## Note on the matcher — it was wrong three times

Every reported "gap" in this pass was confirmed against the code before being
believed, which is how these were caught:

1. `RS_FN` was anchored `[a-z_]`, so it skipped every ported test that keeps the
   oracle's camelCase type prefix (`fn CircBuf_first_last`, `fn SplitTree_zoom`)
   — **54 false absences**.
2. `norm()` did not split letter↔digit, producing `generate256` where the port
   spells `generate_256_color` — **9 false absences**, all of `generate256Color`.
3. Module-qualified oracle names (`color: parse valid`) do not appear in the Rust
   fn name, since the module is the file — **2 false absences**.

The tool now prefix- and case-matches and splits digit boundaries. The residual
bias is deliberate and stated in the source: a false ABSENT wastes a reader's
time and tempts someone to "fix" the audit instead of the code.
