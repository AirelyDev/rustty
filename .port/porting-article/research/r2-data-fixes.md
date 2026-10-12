# Round 2: data fixes and small measurable gaps

Snapshot: HEAD `1d22ba80d` (branch `phase-8-upstream-resync`), oracle pin `a60e9e2a5`, 2026-10-08.
Hardware for every MEASURED number taken here: the author's MacBook Pro M4. Ghostty checkout at
`/Users/gael/projects/ghostty` is at `a60e9e2a5` with a clean tree. Nothing was committed. Scripts
used for this pass are in the session scratchpad (`rsskip.py`, `zigskip.py`, `reclass.py`); their
logic is described below so the numbers can be reproduced.

Provenance: MEASURED (command given), DOCUMENTED (path given), ESTIMATED (method given).

## Files written, and what they supersede

| r2 file | supersedes | change |
|---|---|---|
| `data/r2-crate-birth.csv` | `data/timeline-crate_birth.csv` | 60 rows (44 crates + 16 -sys). The `README.md` row is gone. Method is now "first commit that added `<dir>/Cargo.toml`". Dates and ≥1k-LOC dates are unchanged for every real crate. |
| `data/r2-lifetimes-rows.csv` | `data/strategy-lifetimes-rows.csv` | Every first-parent commit that touched the ledger, with blank lines excluded. Ends at **314**. |
| `data/r2-gate-registry.csv` | (new; settles `strategy-gate-selftest-growth.csv` vs codebase dossier) | All 101 registry entries with tier and self-test flag. |
| `data/r2-skip-summary.csv`, `data/r2-rust-skip-tests.csv`, `data/r2-zig-skip-tests.csv` | (new) | The "0 tests skipped" analogue. |
| `data/r2-build-times.csv` | (new) | Recorded build and lane durations, plus two fresh measurements. |
| `data/r2-libvt-sizes.csv` | extends `data/perf-binary-size.csv` | Native dylib and wasm sizes at HEAD. |
| `data/r2-bugs-upstream-method.csv`, `data/r2-bugs-upstream-method-x-class.csv`, `data/r2-bugs-upstream-method-detail-x-class.csv` | `data/bugs-upstream-method-x-class.csv` (and the method column of `bugs-upstream-defects.csv`) | Hand pass over the 96-entry default bucket. |

`data/bugs-upstream-by-week-method.csv` still uses the round-1 labels. Rebuild it from
`r2-bugs-upstream-method.csv` joined on `id` with `first_date` if the weekly chart is kept.

## 1. Crate birth (N5)

- MEASURED: `for d in crates/*/ crates-sys/*/; do git log --reverse --diff-filter=A --format='%h|%ad' --date=short -- $d/Cargo.toml | head -1; done`.
- 60 packages: 44 `crates/*` + 16 `crates-sys/*`. 43 were born on 2026-06-21: 29 crates and 14 -sys.
  Of the -sys crates, 13 came in `15c1a9bd7` and gettext-sys in `f7eba28e9`. The two later -sys
  crates are libxml2-sys (07-17) and breakpad-sys (08-11). The last crate born was rustty-version
  (09-23).
- No crate directory was ever deleted. `git log --diff-filter=D -- 'crates/*/Cargo.toml' 'crates-sys/*/Cargo.toml'`
  returns nothing.
- `first_date_rust_loc_ge_1000` was recomputed from `timeline-loc_by_day_long.csv` and agrees
  with round 1 for every crate.

## 2. LIFETIMES ledger growth (N15, A8)

- MEASURED: `git log --first-parent --reverse -- .port/audits/GHOSTTY_LIFETIMES.tsv`, then per
  commit `git show <c>:<file> | grep -v '^#' | grep -v '^file' | grep -vc '^[[:space:]]*$'`.
- Root cause of the 315: a single blank line entered the file on 07-30 (`401c2dbdf`) and stayed.
  Round 1's count included it from that date on, so 226, 235 and 254 were also one too high (the
  true values are 225, 234 and 253).
- Corrected series on main's first-parent line (34 points): 216 on 07-10 (creation, `8f6d781dc`),
  220 on 07-23, 225 on 07-30, 229/232/234 on 08-18, 242/245/250/253 on 08-25, 254 on 09-28, then
  a burst on 09-29 (257 → 300) as the Phase-7 soundness workstreams merged, 302/303 on 09-30, 305
  on 10-07, and **314** on 10-08 (`897fb3566`, the libvt C API merge).
- Use first-parent. With `--all`, the per-commit counts on 09-29 zig-zag (for example 272 → 260 → 267),
  because parallel worktree branches each held a different subset of rows.
- The class histogram sums to 314: BORROW_FIELD 68, BORROW_PARAM 49, OWNED 34, FFI 31, SHARED 27,
  INTRUSIVE 25, ARENA 19, GHOSTTY_C 17, STATIC 11, BORROWED_PTR 10, BACKREF 9, INLINE_OWNED 5,
  OWNER_TRANSFER 3, OWNED_FD 2, and 1 each for SHARED_CELL, OWNED_HEAP, BORROWED_SLICE and
  BORROWED_HANDLE. A8 is confirmed ready.

## 3. Self-test gate entries: 24, not 22 (BRIEF §4)

- MEASURED by parsing every `"<ID>|<HARD|PROGRESS>|<cmd>"` entry in `.port/tools/ci.sh`:
  - 101 entries, all with distinct IDs: 75 HARD and 26 PROGRESS.
  - **24** entries are self-tests (`-SELF`): 23 HARD and 1 PROGRESS. Every self-test entry runs its
    gate's `--self-test` arm, or for A5, `--self-test-masking`. No non-SELF entry runs a self-test.
  - 77 entries are not self-tests: 63 G-gates, 11 regression-class detectors (D1-D6, D8-D11, D11r)
    and 3 C-ABI header gates (A3, A4, A5).
  - 20 self-tests guard a gate that is also in the registry. The other 4 test shared machinery:
    the Miri runner, the sanitizer harness, the oracle locator, and A5's masking control.
- Why round 1 said 22: the codebase dossier grepped `\bG[0-9]...|\bG-[A-Z0-9-]+` over the whole of
  `ci.sh`. That pattern misses `A5-MASK-SELF`, which does not start with G, and `G1-SELF`, which the
  `G[0-9][0-9a-z.-]*` branch truncates to `G1-`. The same grep also produced "86 distinct gate IDs",
  because it skips the 12 D-gates and 4 A-gates and adds one regex artifact. That figure should be
  dropped.
- The script-level count of 57 of 90 gate scripts with a self-test arm is re-confirmed: `ls .port/tools/gates/*.{sh,py,mjs}`
  gives 90, and grepping for `self-test|self_test` gives 57.
- Publishable wording: "101 registered gates (75 hard, 26 progress). 24 of them exist only to
  prove another gate can fail. 57 of the 90 gate scripts carry such a self-test."

## 4. Skipped tests: the analogue of Bun's "0 tests skipped"

Rust side: MEASURED with `rsskip.py`. It reads every test fn (`#[test]`, `#[tokio::test]`,
`#[rstest]` and `#[test_case]`) in `git ls-files 'crates/*.rs' 'crates-sys/*.rs' 'xtask/*.rs'`
together with its whole attribute block, including multi-line rustfmt attributes.

| | count |
|---|---|
| test fns | 7,270 (same as round 1) |
| no skip mechanism of any kind | **6,325** (87%) |
| plain `#[ignore]`, skipped by default and run on request | **133**: 100 are rustty-apprt tests that need a live display, a compositor, GTK, D-Bus or a device. The remaining 33 cover release-only tests, `/dev/tty`, the oracle checkout and a few others. |
| `cfg_attr(<pred>, ignore)` naming Miri, so ignored only under Miri | **475**. 128 of them are `all(miri, debug_assertions)` and run in the debug-assertions-off Miri lane. The Miri exception cross-check doc says 468 (`.port/phase-2/MIRI_EXCEPTIONS.md`), and its lane says 124. |
| `cfg_attr(<pred>, ignore)` scoped by feature or platform | 136 |
| test fns compiled only on some platforms or features (`#[cfg(..)]`) | 259; separately, 35 test modules are platform- or feature-gated |
| silent early-return skips at HEAD | **0** (the no-silent-skips gate at the pin: 0 sites, and 265 guarded returns accepted because they assert or pivot), DOCUMENTED `.port/phase-8/w0/g-skip-finale-at-a60e9e2a5.txt` |

Correction: round 1's "136 `cfg_attr(miri, ignore)`" (`codebase.md`) counted only single-line
attributes. Most Miri exceptions are rustfmt'd onto three lines, and the true count is about 475.

**The silent-skip story.** On 2026-09-23 (`1096fa4cd`) a new lint was run over the tree as it
stood before the fix. It found **185** test sites that reported `ok` while checking nothing
whenever a resource was missing: 145 early `return`s, 22 bodies that were entirely `#[cfg]`-gated,
17 unpivoted "skipping" prints in helpers, and 1 test whose every assertion sat in an else-less
`if`. The affected tests needed Metal, system fonts, a display, the oracle, a feature flag or the
target OS. DOCUMENTED in `.port/parity-closure/NEW-FINDINGS.md` (N196).

After that day, a skip may take only one of three forms:
- a compile-time attribute;
- a strict-mode pivot, `require_<x>_or_skip`, where setting `RUSTTY_REQUIRE_<X>=1` turns the skip
  into a failure. There are 18 such helpers and 88 call sites, and CI sets the variables wherever
  the resource exists;
- an `#[ignore]`d test that fails loudly when it is run.

A hard gate keeps the class out. Its allow-list is capped at 6 sites and 2 guards, and it holds 4
sites and 1 guard today (MEASURED: `ALLOW_SITES`/`ALLOW_GUARDS` in `.port/tools/gates/check_silent_skips.py`).

Runtime ignored counts:
- full Linux Valgrind run: 156 ignored, 6,614 passed (DOCUMENTED `.port/parity-closure/README.md`);
- `cargo test -p rustty-vt` at the Phase 6 pin: 3 ignored, 2,974 passed (DOCUMENTED `.port/phase-6/README.md:81`).

Zig side: MEASURED with `zigskip.py` over `ghostty/src/**/*.zig` at `a60e9e2a5`. It brace-matches
test bodies after stripping comments and strings.
- 4,423 test blocks, which reproduces round 1's regex count.
- **199** test blocks contain `error.SkipZigTest` directly. This is a floor, because skips reached
  through helper functions are not counted. The raw string occurs 217 times in 45 files.
- Of the 199, 167 are guarded by a comptime platform or build option and 32 by a runtime condition
  (ESTIMATED, using a keyword classifier on the 300 characters before the skip). Runtime conditions
  include `runningOnValgrind()` (10 tests), a missing system font or shaper result, a null
  `decode_png`, and `gtk.initCheck() == 0`.

Parity: every live oracle test block has a Rust counterpart or a written reason.
- 4,277 of 4,277, with 0 unexplained (DOCUMENTED `.port/phase-8/w0/g-tests-finale-at-a60e9e2a5.txt`).
- 211 modules have Rust ≥ Zig counts (BRIEF #8).
- At the pin, the 120 upstream test changes absorbed were 90 adds present and 30 changes
  re-aligned, with 0 gaps (DOCUMENTED `.port/phase-8/w0/tests-checklist.txt`).

Publishable claim: "No Zig test was dropped. Ghostty's own suite has 199 tests that can skip
themselves at run time or build time, and Rustty mirrors those as compile-time scopes or
strict-mode pivots. Since 2026-09-23 a hard gate rejects any Rust test that returns early without
asserting. 133 tests are ignored by default because they need a live display or device, and
dedicated lanes run them."

## 5. Build times

No end-to-end clean-build timing was ever recorded. `ci.sh` end to end is listed as "never
recorded". What exists is in `data/r2-build-times.csv`. Highlights:

- MEASURED (2026-10-08, M4, empty `CARGO_TARGET_DIR` under `research/tmp/target`, deleted
  afterwards): `cargo build -p rustty-vt-c --release` took **31.9 s wall** (70.4 s user) from
  cold, dependencies included. The wasm32 build of the same crate right after took **14.3 s**.
- DOCUMENTED (`.port/phase-8/w9/cross-host-build-at-a60e9e2a5.txt`, fresh clones at the pin):
  - rvm (Fedora aarch64 VM): full workspace `--all-targets` with GTK/X11/Wayland in 3 m 11 s;
    clippy 54 s; release app 1 m 50 s.
  - rwin (Windows 11 ARM64 VM, x86_64-msvc): workspace in 2 m 37 s; clippy 1 m 52 s; libvt release
    36 s; gnu font-shaper leg 1 m 56 s.
  - macOS worktree: workspace in 49.6 s (cache state not recorded).
- DOCUMENTED verification lanes:
  - Miri Stacked Borrows over 38 crates: 13,354 s (3 h 43 min), of which rustty-vt took 9,525 s.
    Moving to nextest cut the same groups from 11,722 s to 1,687 s (6.9×).
  - Debug-assertions-off Miri: 1,110 s.
  - Sanitizers: ASan about 15 min on the Mac; LSan 33 min on the Mac and 20 min on rvm; MSan 50
    min; HWASan 56-77 min; TSan 38 min on the Mac and 21 min on rvm.
  - Valgrind: 13,451 s (3 h 44 min) for 914 memcheck processes.

## 6. libvt sizes (native and WASM)

MEASURED at HEAD on the M4 (same temp target, now deleted):

| artifact | bytes | vs Ghostty |
|---|---|---|
| `librustty_vt_c.dylib` (release) | **2,108,032** (__text 1,290,996) | 1.095× libghostty-vt.0.1.0.dylib ReleaseFast (1,924,784 B). The oracle number is at the older pin 12752b2ac (DOCUMENTED BASELINE.json); the regression fence tolerance is 1.10. |
| `rustty_vt_c.wasm` (`wasm32-unknown-unknown`, the flags from `.port/tools/build-vt-wasm.sh`, before the growable-table patch) | **1,385,595** | no oracle wasm measured (building one would write into the read-only Ghostty checkout) |
| `librustty_vt_c.a` (static) | 26,633,672 | not like-for-like (unpruned staticlib; BASELINE.json drops the static axis for this reason) |

- History (DOCUMENTED BASELINE.json): the dylib was 2,498,112 B (1.30×) before the size work,
  2,071,760 at the re-base on 10-07, and 2,108,032 now (+1.75%, after the libvt C API merge).
- The 1.08× in BRIEF #30 is the re-base value. Quote "about 1.1×".
- Older wasm records: the July wasip1 build with C deps was about 1.02 MB with 157 exports
  (`.port/phase-2/DEFERRALS.md`), and `rustty_wasm.wasm` was 992,196 B (`.port/parity-closure/DECISION-logging.md`).
- Ghostty's own `zig-out/lib/libghostty-vt.0.1.0.dylib` in the checkout is 7,994,752 B. It is not
  a ReleaseFast build, so do not compare against it.

## 7. Upstream-defect discovery method: hand pass

Method: each of the 96 entries in round 1's default bucket ("line-by-line port review") was read
in `.port/parity-closure/UPSTREAM-DEFECTS.md`. Each entry's `parity row(s)` field was resolved:
- an `S…-F…` ledger row means the parity audit (an agent audit comparing each port file side by
  side with its reference);
- `inventory M…` means the dedicated defect sweep run after the never-port-a-bug rule;
- an `N…` row was followed to the `found by` line in `.port/parity-closure/NEW-FINDINGS.md`.

The entry's own text ("found by …", "measured on the VM", "found while porting …") broke ties.
Confidence is high for 73 entries and medium for 23. The other 109 entries keep round 1's keyword
label, mapped onto the same taxonomy. Data: `data/r2-bugs-upstream-method.csv`. Each row carries
`method_r1`, `method_r2`, `method_r2_detail`, `confidence` and `basis`.

All 205 entries, by discoverer (class mix in brackets):

| how found | entries | classes |
|---|---|---|
| review and reading, all kinds | 92 | 38 latent, 17 crash, 16 intent-breaking, 11 UB, 7 leak, 3 hang |
| → code reading (entry says so; round 1's bucket) | 25 | 9 of the 19 UB |
| → dedicated defect sweep of the reference and of past port notes | 24 | |
| → parity audit, file by file | 23 | |
| → noticed while translating (session logs, records, port notes) | 11 | |
| → adversarial reviewer of a task | 6 | |
| → re-audit after a pin advance | 2 | |
| → security review | 1 | |
| checked arithmetic, casts and bounds | 30 | 29 crash, 1 UB (round 1's 29, plus an unbounded many-item pointer that became a bounds-checked slice) |
| cross-platform bring-up (Linux VM, Windows VM, FreeBSD target) | 25 | 6 crash, 2 UB, 2 hang, 10 intent-breaking, 5 latent |
| compiling previously-unanalysed code (Zig's lazy analysis) | 17 | 14 latent, 3 intent-breaking; still no crashes |
| fault injection (failing allocator, C-API failure sweep) | 12 | 5 leak, 4 crash |
| test port (reference tests that cannot fail) | 11 | 11 test-weakness |
| type and borrow checker | 5 | 4 UB, 1 crash |
| porting the build system (build.zig, CMake, packaging) | 4 | 3 intent-breaking, 1 latent |
| live testing of the port (hangs, assertion fired) | 4 | 3 hang, 1 crash |
| sanitizers and Valgrind | 3 | 2 leak, 1 UB |
| withheld (private disclosure) | 2 | |
| Miri | **0** as discoverer. It was used 6 times to confirm a Rust model of a pattern found by reading. | |
| fuzzing | **0** (no entry credits a fuzzer) | |

How the 96 were reassigned:
- 67 stay review/reading, now split by kind: 23 parity audit, 24 defect sweep, 11 while
  translating, 6 adversarial reviewer, 2 re-audit and 1 security review. That is 67 in total,
  because the 25 "code reading" entries already sat outside the 96.
- 9 moved to cross-platform bring-up: the GStreamer finalize race on Linux, the Linux-lane crash
  and the CPU-feature finding, a headed GTK callback on the VM, three Linux OpenGL/GTK
  measure-first findings, the FreeBSD kqueue stub, and a Windows file-I/O review.
- 5 moved to type and borrow checker:
  - a `bool` shared across threads, which Rust's `Send`/`Sync` rules forbid, so the port uses
    `AtomicBool`;
  - C enum arguments, which Rust cannot hold as an enum unchecked;
  - three non-UTF-8 byte strings reaching GLib/GTK string APIs, where Rust's `&str` forced an
    explicit conversion that the audit then questioned.
- 5 moved to fault injection.
- 4 moved to live testing of the port.
- 4 moved to porting the build system: the libxml2 version macro, the fontconfig version defines,
  a CMake install, and a stale `.envrc` path.
- 1 moved to checked bounds.
- 1 moved to compiling previously-unanalysed code: a non-default font backend that the port made
  buildable.

Round 1's "16 cross-platform" and "16 compile-unanalysed" become 25 and 17. The article's point
holds and is now sharper: type-level and arithmetic checks (35) and cross-platform runs (25) each
find a distinct kind of bug. Reading still finds the most (92). Miri and fuzzing found none of the
reference's defects, because Zig has no aliasing model for Miri to check.

Chart N31 should be rebuilt from `data/r2-bugs-upstream-method-x-class.csv`, with "review and
reading" optionally expanded from `r2-bugs-upstream-method-detail-x-class.csv`. Keep it labelled
"hand classified; 23 entries medium confidence".

## Remaining gaps

- No Ghostty wasm (`ghostty-vt.wasm`) size to compare against. Building one needs writing into
  the Ghostty checkout or a copy of it.
- No end-to-end clean build of the whole workspace on the Mac. The cross-host record does not say
  whether its target dirs were cold.
- The runtime ignored count on macOS (`cargo test --workspace` summary) was not re-run. Only the
  static count (133) and the Linux Valgrind run's 156 exist.
- The Zig skip count is a floor (direct `SkipZigTest` in the test body only).
- `bugs-upstream-by-week-method.csv` was not regenerated with the new labels.

## Fact-check

Independent re-derivation, 2026-10-08, HEAD `1d22ba80d`, Ghostty checkout clean at `a60e9e2a5`. I used
different methods where I could: an awk row filter in place of grep, a Python parser of my own for the
Rust and Zig test scans, and a fresh build in a scratchpad target dir (deleted afterwards).

1. **Crate birth: CONFIRMED.** There are 44 `crates/*/Cargo.toml` and 16 `crates-sys/*/Cargo.toml`.
   Using `git log --follow --diff-filter=A` (oldest add), 29 crates and 14 -sys were born on 2026-06-21.
   gettext-sys is 06-21, libxml2-sys 07-17, breakpad-sys 08-11, and the last is rustty-version on
   09-23. `git log --all --diff-filter=D` on both Cargo.toml globs is empty.
2. **LIFETIMES: value CONFIRMED, point count CORRECTED.** An awk filter (`!/^#/ && !/^file/ && NF>0`)
   gives 314 at HEAD and the same series: 216 on 07-10 → 225 on 07-30 → 253 on 08-25 → 254 on 09-28 →
   300 on 09-29 → 314 at `897fb3566`. The blank line first appears on the first-parent line at merge
   `18fc25569` (07-30), which brings in `401c2dbdf`. The 5th column histogram (`class`) reproduces
   exactly and sums to 314. Correction: `git log --first-parent` returns **35** commits that touched
   the ledger, not 34. `data/r2-lifetimes-rows.csv` also has 35 data rows, and 216 repeats on three of
   them (07-10 twice, 07-12). Say "35 first-parent commits" or "33 distinct values".
3. **Gate registry: CONFIRMED, with one sub-detail CORRECTED.** My own regex
   `"ID|(HARD|PROGRESS)|` gives 101 entries and 101 distinct IDs: 75 HARD and 26 PROGRESS. 24 are
   `-SELF` (23 HARD, 1 PROGRESS). The 77 others split 63 G, 11 D and 3 A. Gate scripts: 90, of which
   57 match `self-test|self_test`. The round-1 G-grep re-run yields exactly 22 `-SELF` IDs.
   Correction to the §3 explanation: there are **11** D entries (D1-D6, D8-D11, D11r, and no D self-test),
   not 12. The arithmetic behind 86 is 101 − 11 D − 4 A (including A5-MASK-SELF) = 86 G entries. The
   truncation swaps `G1-SELF` for the artifact `G1-` without changing the count. My approximation of the
   dossier grep gives 90 distinct G-strings, because the file also names non-registry G IDs. Either way,
   drop "86".
4. **Rust skip census: CONFIRMED with one wording CORRECTION and one small count difference.** My own
   parser finds 7,270 test fns, 133 plain `#[ignore]` and 475 with a Miri `cfg_attr` ignore. It gets
   131 `all(miri, debug_assertions)`, but 3 of those are false positives from `debug_assertions` inside
   an ignore-reason string, so 128 holds. It gets 266 `#[cfg]`-gated tests. The 7 extra over 259 are
   `#[cfg(test)]` on free test fns, which r2 rightly excludes, and they explain the gap between my
   6,318 and r2's 6,325.
   - CORRECTED: **152** test fns carry a non-Miri `cfg_attr(…, ignore)`. The 136 counts only those whose
     ignores are *all* non-Miri; 16 carry both kinds, for example `canonicalizeLocale_darwin` and
     `read_proc_sys_kernel_osrelease`, and r2 files them under Miri. Reword as "136 ignored only by a
     platform or feature cfg_attr (152 including 16 that also have a Miri ignore)".
   - Minor: of the 133 plain ignores, **101** are in rustty-apprt, and all 101 reasons name a display,
     GTK, D-Bus, a compositor or a device. The text says 100.
5. **Silent-skip story: CONFIRMED, with the helper count UNVERIFIABLE as stated.** `1096fa4cd` is dated
   2026-09-23. N196 (NEW-FINDINGS.md line 4084 on) states 185 = 145 + 22 + 17 + 1. An `ast` parse of
   `check_silent_skips.py` gives `ALLOW_SITES` 4 and `ALLOW_GUARDS` 1, with `MAX_SITES = 6` and
   `MAX_GUARDS = 2`. G-SKIP finale: 0 sites, 265 guarded returns. Pivot helpers: I find 89 call sites,
   close to 88. My call-site count includes wrapper→`_impl` calls. Helper definitions depend on the
   counting method: 15 public wrappers, 25 definitions including `_impl` and `device_or_skip`, and 17
   distinct names. None of these is 18. Say "about 90 call sites" or give the counting rule.
6. **Zig skips: 199 CONFIRMED, the 4,423 total CORRECTED (scope).** My brace matcher, which strips
   comments, strings and `\\` lines, reproduces 4,423 string-named and anonymous `test` blocks, 199 of
   which contain `SkipZigTest`. `git grep -c SkipZigTest` gives 217 hits in 45 files. But Ghostty also
   has **64 identifier-named decltests** (`test WeakRef {`, `test Benchmark {`, …), which the
   `test\s*("|{)` regex misses. All test blocks: **4,487**, of which **201** contain `SkipZigTest`
   (2 of them identifier tests). The 167/32 split was not re-derived (ESTIMATED). The parity record
   reads `4277 live-oracle test blocks across 540 files; 0 unexplained, 1 accepted with a written
   reason`. That is a different scope (live oracle tests), so "4,277 of 4,277" stands.
7. **Runtime ignored counts: CONFIRMED.** `.port/parity-closure/README.md:108` gives "6,614 tests
   passed, 156 ignored". `.port/phase-6/README.md:81` gives "2,974 passed / 0 failed / 3 ignored".
8. **Cold libvt build: CONFIRMED in kind, wall time varies.** In an empty scratchpad target, my re-run
   took **27.2 s** wall (73.3 s user) for the native release build and **11.4 s** for the wasm32 build.
   r2 reported 31.9 s and 14.3 s. Quote "about 30 s cold (27-32 s over two runs)" and "about
   11-14 s" for wasm.
9. **Cross-host builds: CONFIRMED.** The record's lines 7, 12, 17, 23, 27, 31, 35 and 41 give 3m 11s,
   54.20s, 1m 50s, 2m 37s, 1m 52s, 36.15s, 1m 56s and 49.60s. Caveat to add: the rwin "workspace"
   build excludes `libxml2-sys`, `fontconfig-sys` and `sentry-sys`. The rvm clone was a fresh
   `git archive`, but its target-dir state is not recorded. R4 lists `ci.sh` end to end as "never
   recorded".
10. **Lane durations: CONFIRMED.** These match parity-closure README lines 91-93 and 110, and the R4
    table at lines 505-515: 13,354 s; 9,525 s; 11,722 → 1,687 s; 1,110 s; ASan ~15 min; LSan 33/20
    min; MSan 50 min; HWASan 56–77 min; TSan 38/21 min; Valgrind 13,451 s.
11. **Native dylib size: CONFIRMED exactly.** My fresh build gives 2,108,032 B, with `size -m`
    `__TEXT` 1,654,784 and `__text` 1,290,996. BASELINE.json gives the oracle as 1,924,784 B at pin
    `12752b2ac`, the re-base value as 2,071,760, and the gap attribution as 2,498,112. The ratio is
    1.0952. The growth since the re-base is +1.75%.
12. **WASM size: CONFIRMED exactly.** I get 1,385,595 B with the `build-vt-wasm.sh` link args
    (`--export-table`, `-zstack-size=131072`).
13. **Defect discovery method: CONFIRMED as internally consistent; the classification is judgment
    (ESTIMATED is correct).** `UPSTREAM-DEFECTS.md` has 205 unique `UD` headings. The CSV tallies
    reproduce every row of the table: 92/30/25/17/12/11/5/4/4/3/2; the review sub-split
    25/24/23/11/6/2/1; confidence 73 high, 23 medium, 109 round-1. They also reproduce the class mixes
    (arithmetic 29 crash + 1 UB; type checker 4 UB + 1 crash; cross-platform 10/6/5/2/2).
    - Fuzz 0 is confirmed: the word "fuzz" appears in no UD entry.
    - Miri 0 as discoverer holds. Only UD352, UD365 and UD366 mention Miri, each as a proof or harness
      after the find. UD352 reads "Found by the P7-W0 T1 task review", so it belongs under "adversarial
      reviewer of a task" rather than its round-1 "code reading" label. It is one of the 109 entries the
      hand pass did not cover, so expect ±1 noise in those buckets.
14. **Type-system finds: CONFIRMED as described, interpretive.** UD108 (AtomicBool, `loop_.rs`), UD113
    (`from_c_int`, D11), UD170 (IPC argument check, crash), UD171 and UD172 (`from_utf8_lossy` preview)
    match the descriptions. But the entries' own provenance points elsewhere: UD108's parity row is
    "inventory M40" (the defect sweep), and UD170 to UD172 cite parity-audit ledger rows S59-F1, S59-F5
    and S56-F3 and "MEASURED on the VM". The claim that the type system *found* them is the hand pass's
    reading of how the bug surfaced, and r2's medium confidence for UD170 to UD172 is right. Phrase it
    as "the type system forced the question that exposed them".
