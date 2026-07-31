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

## Accepted absences (3)

All four oracle `codepoint_width` tests are one Rust test. This is a real
consolidation, not a gap: **every assertion survives, one for one**, in
`crates/rustty-vt-c/src/unicode.rs::codepoint_width_cases` —

| oracle test | assertion | in the Rust test |
| --- | --- | --- |
| `codepoint_width narrow` | `1 == codepoint_width('a')` | line 51 |
| `codepoint_width wide` | `2 == codepoint_width(0x4E00)` | line 52 |
| `codepoint_width zero` | `0 == codepoint_width(0x0301)` | line 53 |
| `codepoint_width out of range` | `1 == codepoint_width(0x110000)` | line 54 |
| `codepoint_width out of range` | `1 == codepoint_width(maxInt(u32))` | line 55 |

The first of the four is matched by its citation and so never reaches this list;
the other three are recorded here because one citation comment cannot name four
tests verbatim. Splitting them back into four Rust tests would add nothing — a
`#[test]` per assertion buys independent failure reporting, which five
`assert_eq!`s over a pure function do not need.

- `src/terminal/c/unicode.zig::codepoint_width wide`
- `src/terminal/c/unicode.zig::codepoint_width zero`
- `src/terminal/c/unicode.zig::codepoint_width out of range`

## Also ported as a result of this audit

- `src/lib/compat/file.zig::readToEndAlloc reads through EOF and permits exact limit`

`compat/file.zig` is a backport of Zig 0.15.2's `std.fs.file` and has no single
Rust counterpart — Rust's std already reads files, so only the byte cap needs
expressing, and it is expressed per call site (`shadertoy.rs:275`,
`kernel_info.rs:23`, `page_compression.rs:826`). The cap logic was therefore
real, and **untested**: nothing in the tree pinned it. The oracle test's point is
in its name — a file of *exactly* `max` bytes must succeed — so a `>=` where the
code has `>` would reject it silently.

Ported onto `read_to_end_limited` (`crates/rustty-bench/src/page_compression.rs`)
and mutation-proved: flipping `>` to `>=` fails the exact-limit case with
`corpus exceeds the benchmark's maximum data size`. The port adds a short-read
arm the oracle gets for free from `readStreaming`, since Rust's `Read` may return
fewer bytes than requested and the cap must apply to the accumulated length.

## Note on the matcher — it was wrong five times

Every reported "gap" in this pass was confirmed against the code before being
believed, which is how these were caught:

1. `RS_FN` was anchored `[a-z_]`, so it skipped every ported test that keeps the
   oracle's camelCase type prefix (`fn CircBuf_first_last`, `fn SplitTree_zoom`)
   — **54 false absences**.
2. `norm()` did not split letter↔digit, producing `generate256` where the port
   spells `generate_256_color` — **9 false absences**, all of `generate256Color`.
3. Module-qualified oracle names (`color: parse valid`) do not appear in the Rust
   fn name, since the module is the file — **2 false absences**.
4. That qualifier strip compared only the FIRST token of the normalized head
   (`glyf` against the stem `glyf_rasterize`), so it never fired for a
   two-word module name and left the qualifier attached — **1 false absence**,
   `contour can start off curve with final on curve point`, present and passing
   at `glyf_rasterize.rs:748`.
5. Citations are matched against the raw source, but rustfmt **rewraps a long
   PORT NOTE mid-citation** (`... returns correct GObject\n    /// types"`), so a
   literal search missed two correctly-cited tests — **2 false absences**. The
   tool now unwraps doc-comment continuations before matching.

Six of the eight residual absences were resolved by fixing (4) and (5) plus
adding citation matching — a verbatim oracle test name in a PORT NOTE now counts
as a match. That is the port's own claim of provenance, the same signal G-FILES
uses at file level, and it is the **only** thing that survives a rename:
`test "Key.Type returns correct types"` is ported as `fn gsettings_key_type`,
which no name-similarity heuristic will ever link back. Requiring the *literal*
name keeps this from becoming another vacuous arm.

The residual bias is deliberate and stated in the source: a false ABSENT wastes a
reader's time and tempts someone to "fix" the audit instead of the code.
