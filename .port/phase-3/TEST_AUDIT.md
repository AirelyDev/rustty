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

## Second pass — the 16 cold-history compression absences

A later run of this tool (after the 70%-of-tokens tightening described below)
surfaced **16 further absences**, fifteen in `src/terminal/PageList.zig` from the
same `9156ada16` cold-history compression series and one in
`src/terminal/c/terminal.zig`. Each was confirmed against the code before being
believed. Fifteen were genuinely absent and are now ported:

| oracle test | ported as |
| --- | --- |
| `PageList incremental compression skips visible history` | `pagelist_incremental_compression_skips_visible_history` |
| `PageList owns incremental compression state` | `pagelist_owns_incremental_compression_state` |
| `PageList incremental compression bounds inspected pages` | `pagelist_incremental_compression_bounds_inspected_pages` |
| `PageList incremental compression advances after failure` | `pagelist_incremental_compression_advances_after_failure` |
| `PageList incremental compression advances after decommit failure` | `pagelist_incremental_compression_advances_after_decommit_failure` |
| `PageList incremental compression restarts after active boundary resize` | `pagelist_incremental_compression_restarts_after_active_boundary_resize` |
| `PageList incremental compression restarts after prune reuse` | `pagelist_incremental_compression_restarts_after_prune_reuse` |
| `PageList incremental compression restarts after earlier replacement` | `pagelist_incremental_compression_restarts_after_earlier_replacement` |
| `PageList incremental compression keeps progress after tail growth` | `pagelist_incremental_compression_keeps_progress_after_tail_growth` |
| `PageList lazily restores compressed history made active by resize` | `pagelist_lazily_restores_compressed_history_made_active_by_resize` |
| `PageList compression restores through page access` | `pagelist_compression_restores_through_page_access` |
| `PageList reset discards malformed compressed data` | `pagelist_reset_discards_malformed_compressed_data` |
| `PageList deinit discards malformed compressed data` | `pagelist_deinit_discards_malformed_compressed_data` |
| `PageList prune reuses malformed compressed page memory` | `pagelist_prune_reuses_malformed_compressed_page_memory` |
| `clipboard_write without callback is unsupported and silent` | `clipboard_write_without_callback_is_unsupported_and_silent` (`rustty-vt-c`) |

Two oracle tests the tool did **not** flag were absent anyway — one Rust test,
`pagelist_compress_incremental_restarts_after_list_mutation`, was carrying three
oracle names on name similarity alone while asserting only its own (it erases
history; it never replaces a node or calls `reset`). Both are now ported for real
as `pagelist_incremental_compression_restarts_after_replacement` and
`pagelist_incremental_compression_restarts_after_reset`. **A green name match is
not evidence that the assertions exist.**

### Accepted absence (1) — RESOLVED 2026-08-11 (F-015), now ported

- `src/terminal/PageList.zig::PageList incremental compression advances after allocation failure`

The oracle test's entire content is an allocator seam: it wraps the list in
`std.testing.FailingAllocator`, sets `fail_index` so the **exact-sized encoded
allocation** inside `compress.Page.init` fails, and asserts that the pass moves on
to the next page rather than retrying the failed candidate.

The argument recorded below was that rustty had no injectable analogue, because
the encoded buffer was `scratch[..encoded_len].to_vec()` in
`crates/rustty-vt/src/compress/page.rs` — a global-allocator `Vec` that **aborts**
on OOM rather than returning an error. **That argument was wrong on its premise,
not on its reasoning**: the abort was a port divergence, not an unavoidable
property of Rust. `Vec::try_reserve_exact` is a real fallible seam and needs no
allocator parameter. The vet finding F-015 confirmed the divergence (the oracle
degrades gracefully — `error.OutOfMemory => return false`, page stays resident —
where the port aborted the process), and the same review corrected the attribution
of the abort to "the crate's DG5 abort policy": DG5 (`PORT_RULES.md:139`) is the
C-ABI panic-at-boundary policy, not an internal-allocation doctrine.

`CompressedPage::init` now returns `Result<_, compress::InitError>` with an
`OutOfMemory` arm and retains the encoded bytes through `try_reserve_exact`;
`PageList::compress_page` already mapped that `Err` to `return false`. The oracle
test is ported as
`pagelist_incremental_compression_advances_after_allocation_failure`, with the
`FailingAllocator` replaced by the `compress::page::InitFp::Encoded` tripwire fail
point sitting immediately before that allocation (the established idiom for OOM
arms in this crate — `COMPRESS_PAGE_TW`, `Screen`'s `startHyperlink`/`resize`
modules). `compressed_page_reports_a_failed_encoded_allocation` covers the same
arm at the `compress/page.rs` unit level.

The *structural* claim — a failed candidate does not stall the pass, and the next
step continues at the following serial — was already asserted twice over, through
the two failure seams that existed before, and both are mutation-proved:

- `pagelist_incremental_compression_advances_after_failure` (incompressible page,
  `compress_page` returns false at the codec),
- `pagelist_incremental_compression_advances_after_decommit_failure`
  (`COMPRESS_PAGE_TW` tripwire on the strict-decommit step).

Rewinding the continuation marker on failure (`last_serial = None` in the
non-compressing arm of `compress_incremental`) fails **both** of them on
`assertion failed: unsafe { super::Node::is_compressed(second) }`. The
allocation-shaped instance of that same claim is now asserted alongside them.

## Third pass (2026-08-28) — at the `88f57ee66` pin

The run reported 21 absences: 19 in `src/lib/TinyIo.zig`, one in `src/os/i18n.zig`
and one in `src/terminal/Terminal.zig`. One was a real missed port.

### `src/lib/TinyIo.zig` (19) — the TOOL was wrong, not the port

`TinyIo.zig` is a **CONFIRMED non-port** with a written rationale and a reopen
trigger — in `.port/phase-4/NON_PORTS.md`, because that is the phase that
absorbed the commit which introduced it. The tool read
`.port/phase-3/NON_PORTS.md` **only**, an accident of the phase it was written
in, so it reported nineteen tests of a deliberately unported file as unexplained
gaps. It now reads every phase's ledger. Nothing was excused that was not already
argued in writing; the tool simply stopped ignoring where it was argued.

### `src/terminal/Terminal.zig::Terminal: fullReset preserves kitty graphics limits` — REAL, now ported

Upstream `bcbc93a6b` ("terminal/kitty: preserve image limits across full reset":
*"RIS previously cleared configured storage limits and allowed mediums. We now
retain this properly."*). The port was still at the pre-commit behaviour:
`Screen::reset` rebuilt `kitty_images` from `ImageStorage::default()`, so a RIS
threw away the configured `image_limits` and `total_limit` — configuration, not
terminal state. Ported in `screen.rs` (carry both across the reset) with the
oracle test as `terminal_full_reset_preserves_kitty_graphics_limits`.

Mutation-proved: dropping the two carried fields again fails the test at
`left: 1234, right: 320000000` — the default byte limit — so it keys on the
preservation, not on incidental state.

### Accepted absence (1) — `src/os/i18n.zig::_ returns msgid at comptime`

The oracle test is `comptime @"_"("Ghostty")` compared against `"Ghostty"`: it
exercises the `if (@inComptime()) return msgid;` arm of `_`. That arm has no Rust
counterpart and `i18n.rs:134-137` already says why — `gettext_` is not a `const
fn` and could not be (it calls `dgettext` through FFI), so there is no
compile-time evaluation for the arm to guard.

Its observable — an untranslated msgid comes back byte-identical — is asserted at
run time by `crates/rustty-os/src/i18n.rs::gettext_resolves_from_vendored_libintl`
(`assert_eq!(gettext_(msgid), msgid)`). That test is macOS-only, because its other
job is proving the call resolves against rustty's *vendored* libintl rather than
Homebrew's; on Linux libintl comes from libc and the vendored-archive question
does not arise.

- `src/os/i18n.zig::_ returns msgid at comptime`

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
