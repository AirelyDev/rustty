# Parity closure record

Closure of the functional-parity audit of Rustty against Ghostty at pin
`20abdb50a` (`docs/parity-audit/README.md`). Plan:
`docs/superpowers/plans/2026-09-08-parity-defect-closure.md`. Work ran from
2026-09-08 to 2026-09-28.

**Current pin: `a60e9e2a5`** (Phase 8 upstream resync, started 2026-10-07; the advance is `P8-W0 T3`,
evidence in `.port/phase-8/w0/README.md`). It was `12752b2ac` through Phase 7 (2026-09-28 to
2026-09-30; `.port/phase-7/README.md`). The records below were carried forward through Phase 7, and the
counts are as of its finale. Phase 7 added UD352–UD372, N435–N465 and V27–V29.

## Result

- **Ledger (`LEDGER.tsv`), 497 audited findings: 404 CLOSED, 93 WAIVED, 0 OPEN**
  (403 / 94 at the closure; one waiver was closed in Phase 7).
  - By severity: all 12 high are closed; medium 135 closed and 5 waived; low
    256 closed and 89 waived.
  - `G-PARITY` (`check_parity_closure.py`) is **HARD** in `ci.sh` since Task
    41: a reopened row, a waiver without an owner or reason, or an unknown state
    fails CI. Its self-test covers all three red arms.
- **Waivers.** Each `WAIVED` row names its reason in the `owner` column, and the
  reasoning is in `ADJUDICATION.md` and the area files. By class: 54 INDIV, 13
  LOG, 12 ABI, 6 TEST, 5 ALLOC, 3 DEBUGONLY, 1 DOC. By the ledger's `kind`
  column: 35 behavioral-difference, 15 other, 13 missing-function, 12
  missing-branch, 6 different-constant, 4 missing-error-path, and 9 of other
  kinds.
- **Findings outside the ledger (`NEW-FINDINGS.md`).** 256 entries, found while
  closing the ledger (ids up to N434; ids were reserved in blocks per batch,
  so the numbering has gaps). Phase 7 added 25 more (N435–N465), for 281.
  Their states were re-triaged on 2026-09-28 (task T-1). What is still open is
  listed below.
- **Reference defects (`UPSTREAM-DEFECTS.md`).** 178 Ghostty defects (UD ids) at the
  closure, 199 after Phase 7 (UD352–UD372)
  that Rustty does not port (rule R13): each is fixed in Rustty, tested, and
  listed for users in `docs/ghostty-bug-fixes.md`. Two are crashes any program
  writing to the terminal can trigger, and are marked for **private** report to
  Ghostty: UD131 and UD351. Their user-facing entries withhold the trigger.
  None has been reported upstream yet.

## Gate state at closure

Measured on the closing commit with `postmerge_gates.sh` (build, clippy, fmt,
VT headers, embed ABI, header parity, G-SKIP, G-LOGFILE and its self-test,
oracle self-test, test counts, G7 acyclic, G-VTALLOC, phase-2 closure, G3b
self-test, UD docs, G-PARITY): all green.

Gates fixed during the final tail (task C-1) with targeted runs: G-LINT, G-REL
(including N390, release builds dropping info/warn logs), G6b, D1, G6c, G4
(which had been passing against stale driver binaries; goldens re-based on the
pin), G-P2-SELF, G-KAITAI (now HARD).

## Sanitizer gates (P7-W14, 2026-10-01)

`.port/tools/gates/sanitizers.sh` became the sanitizer matrix of the memory-safety-lanes design
(`docs/superpowers/specs/2026-10-01-memory-safety-lanes-design.md` §B), registered in `ci.sh`:

| gate | tier | where | what |
|---|---|---|---|
| G-SAN-SELF | HARD | everywhere (needs nightly) | a synthetic crate's heap overflow, leak and uninitialized read are RED under ASan, LSan and MSan (the last two on Linux only), a clean control GREEN, a filter matching nothing a vacuous RED; plus the classifier, cross-check, `NO_TESTS_ON` and BLOCKED arms over fixtures |
| G-ASAN | HARD (`--fast` skips it) | macOS and Linux | AddressSanitizer over every workspace member's tests plus the `rustty-vt:wuffs` lane, then the cross-check that every Miri-excepted test (468) ran and passed, or is on the per-OS expected-absent list |
| G-LSAN | HARD (`--fast` skips it) | macOS (since P7-W16a) and Linux; BLOCKED elsewhere | ASan with `detect_leaks=1` (rustc's ASan runtime has a working leak checker on Apple Silicon; the standalone `-Zsanitizer=leak` and Apple clang's runtime do not), the same scope plus, on Linux, the GTK apprt and Flatpak feature lanes; suppressions only through `.port/phase-2/SANITIZER_SUPPRESSIONS.md`: `lsan.supp` on Linux (two, both Mesa), `lsan-darwin.supp` on macOS (sixteen: libobjc, CoreFoundation, SkyLight, CoreText, CoreAnimation, ColorSync, Metal and dyld TLV state LSan on Darwin does not see as roots; Apple's `leaks` reports 0 for the same binaries) |
| G-HWASAN (P7-W16b) | HARD (`--fast` skips it) | aarch64 Linux (rvm); BLOCKED elsewhere | HWAddressSanitizer (`-Zbuild-std`, tagged globals, lld) with our C/C++ instrumented, over G-ASAN's scope plus the GTK and Flatpak lanes, minus `xtask` (H1), and the same Miri-exception cross-check. Its pointer tags catch an access landing inside another live block, which ASan's redzones miss (G-SAN-SELF A4). No hosted job yet (V31) |
| G-MSAN | HARD (`--fast` skips it) | Linux; BLOCKED elsewhere (compiler-rt has no MSan for Darwin) | MemorySanitizer with `-Zbuild-std` and our C/C++ instrumented, over an allow-list of 59 members (rustty-renderer with its EGL/GL/glslang test modules skipped by name); 2 excluded with reasons (glslang-sys, xtask) |
| G-TSAN (P7-W17) | HARD (`--fast` skips it) | macOS (run) and Linux (V32); BLOCKED elsewhere | ThreadSanitizer (`-Zbuild-std`, our C/C++ instrumented and checked for `__tsan_` references) over the threaded crates (`TSAN_SCOPE`: core, termio, xev, renderer, vt with `wuffs,oniguruma`, vt-c, highway-sys); empty `tsan.supp`. First runs found UD377 (N508); GREEN on the Mac, 4,380 tests, 38 min. G-SAN-SELF A5 makes a Rust and a C data race RED |

In ci.yml's `ci` job the three report BLOCKED (`RUSTTY_SANITIZERS_OWN_JOB`), naming the new `sanitizers`
job (one leg per sanitizer on x86_64 Linux, never run yet: V30). Results of P7-W14's runs:
G-SAN-SELF green on macOS and rvm (every arm on Linux); G-ASAN (macOS arm64, 2026-10-02) GREEN, 9,826
tests over 61 members and the `rustty-vt:wuffs` lane, 117 of 117 ASan claims verified, 15 min with one
shared target dir; G-LSAN (rvm) GREEN, 10,087 tests over 61 members and three feature lanes, 86 claims
verified and 31 macOS-only, 20 min; G-MSAN (rvm) GREEN, 6,183 tests over 58 members, 50 min. Fix round 1 (2026-10-02): the cross-check covers all 468 Miri-excepted tests (macOS: 459 ran, 9 expected absent; Linux: 412 ran, 56 expected absent), G-ASAN GREEN with 9,850 tests including a `rustty-vt:wuffs,oniguruma` lane, and `rustty-renderer` joins G-MSAN (52 tests, its EGL/GL/glslang modules skipped by name). Fix round 2: the renderer's skips narrowed to the twelve modules or tests that create an EGL/GL context or run glslang (81 tests under MSan); the G-LSAN matrix of round 1 ran on 7ddf62ffd, and the full G-LSAN gate re-ran GREEN on 05478107 (10,111 tests, cross-check 412 ran / 56 expected absent / 0 problems). The first
runs found N495-N499 (register below); the suppressions, lane options and exclusions are in
`.port/phase-2/SANITIZER_SUPPRESSIONS.md`. P7-W16a (2026-10-02) enabled G-LSAN on macOS (N501). Its first
pass found UD376, N502 (per-surface leaks in the render thread's and renderer state's teardown, on every
platform; only macOS tests reached them, no Linux test closes a core `Surface`: an open coverage gap)
and N503 (the renderer's macOS test helpers, N411's twins), all fixed; after them G-LSAN on macOS is GREEN:
9,850 tests, 0 findings, 33 min, cross-check 459 ran / 9 expected absent / 0 problems.

## Miri lanes on nextest (P7-W15, 2026-10-02)

`check_leaks.sh` runs its Miri lanes on `cargo miri nextest` through `run_miri_nextest` (one Miri process
per test, performance cores minus 2 in parallel; `--libtest` keeps the old runner for one release):

| gate | tier | what |
|---|---|---|
| G6 | PROGRESS | Stacked Borrows over every claimed-done crate's tests (same crate list, exceptions and pass criteria), plus a libtest `--doc` arm and the ASan arms |
| G6c | PROGRESS | the 124 `cfg_attr(all(miri, debug_assertions), ignore)` tests, debug assertions off, each resolved to an exact path that must PASS |
| G6t | PROGRESS (new) | Tree Borrows over `G6T_SCOPE` (vt `page`, `pagelist`, `search`, `render`, `screen`, `terminal`, `snapshot`, `kitty`; `rustty-vt-c`, `rustty-renderer`, `rustty-xev`, `rustty-core`), G6/G6c's debug-assertions split |

Measured against libtest on V27-V29's groups: 11,722 s down to 1,687 s (6.9x). All three lanes GREEN at
`04f0a894c` (2026-10-06, 6 processes): G6 over 38 crates in 13,354 s (`rustty-vt` 3,104 tests in 9,525 s;
two gate-side REDs fixed and rerun GREEN, N507); G6c 124 of 124 in 1,110 s; G6t complete (N500, N505 fixed).
V27, V28 and V29 discharged. Self-tests (`--self-test`,
`--debug-assertions-off --self-test`, `--tree-borrows --self-test`) GREEN. Details:
`.port/phase-2/MIRI_EXCEPTIONS.md`, "P7-W15".

## G-VALGRIND full run (P7-W18, 2026-10-07)

The parity counterpart of the reference's `valgrind` job (`test.yml`, `zig build test-valgrind`,
`build.zig:412-423`: the whole test executable under `valgrind --leak-check=full --error-exitcode=1
--num-callers=50 --suppressions=valgrind.supp`). `check_valgrind.sh`'s default scope: all 62 entries
(every workspace member but `xtask`, plus the `rustty-apprt:gtk,wayland,x11` and `rustty-os:flatpak`
lanes), run on `rvm` (Fedora 44 aarch64, valgrind 3.27.1, Mesa 26.1.4 llvmpipe) at `1e84e3ff5` as
ci.yml's four shards (`--shard K/4 --jobs 4`, two shards at a time) with `RUSTTY_REQUIRE_GL=1
LIBGL_ALWAYS_SOFTWARE=1 RUSTTY_REQUIRE_FUZZ_CORPUS=1`:

- 914 memcheck processes (377 + 208 + 181 + 148), 6,614 tests passed, 156 ignored (as in every lane),
  0 failed; the one N416 named skip; 50 `harness = false` binaries run whole.
- Wall time 13,451 s (3 h 44 min): prebuild 324 s, then shards 1+2 (3,923 s and 8,793 s) and 3+4
  (3,250 s and 4,214 s).
- RED on one crate only, `rustty-renderer`'s lib (16 parts): N509 (Mesa EGL's driver data of the
  never-terminated display, two definite leaks per process: suppressed, VG2/VG3 in
  `.port/phase-2/SANITIZER_SUPPRESSIONS.md`, C reproducer with no Rustty code) and N510 (one read-back
  test compared llvmpipe pixels memcheck marks undefined: fixed in the test with `make_mem_defined`). No
  invalid access, no uninitialised value and no leak in Rustty code anywhere; no production change; no UD.
- After the fixes `check_valgrind.sh rustty-renderer --jobs 4` is GREEN (7 memcheck processes, 151 + 3
  tests); the other 61 entries were GREEN in the full run, and the change touches only the renderer's
  test and Mesa-only suppressions, so the gate is **GREEN** at the P7-W18 head.

## Not verified here (long runs, deferred by the user on 2026-09-28)

These were deliberately not run to finish the closure. Each has an exact
command in `.port/phase-2/VERIFICATION_DEBT.md` or the named record.

| What | Where recorded | Command |
|---|---|---|
| Full `ci.sh` end to end on macOS and Linux (Task 41 step 4) | here | `bash .port/tools/ci.sh`; on Linux `ssh rvm 'cd ~/projects/rustty && bash .port/tools/ci.sh'` |
| G-REL: 17 release doc-test targets failed with "can't find crate" in the last full run, and pass when run alone; cause unknown | task-C-1 report | `bash .port/tools/gates/check_release_lane.sh` |
| ~~G6 Miri as one continuous lane~~ DONE on nextest, P7-W15: 38 crates in 13,354 s at `04f0a894c`, GREEN after N507 (`rustty-vt` 3,104 tests in 9,525 s; about 8 h under libtest) | `MIRI_EXCEPTIONS.md` "P7-W15" | `bash .port/tools/gates/check_leaks.sh --all-done` |
| ~~G6t to completion~~ DONE, P7-W15: GREEN after N500 and N505 (`rustty-vt` 1,597 + 119, `rustty-xev` 22, `rustty-vt-c` 570, `rustty-renderer` 60, `rustty-core` 67) | `MIRI_EXCEPTIONS.md` "P7-W15" | `bash .port/tools/gates/check_leaks.sh --tree-borrows` |
| ~~G6c as the gate runs it~~ DONE on nextest, P7-W15: 124 of 124 GREEN in 1,110 s (libtest: 119 tests, about 105 min) | `MIRI_EXCEPTIONS.md` "P7-W15" | `bash .port/tools/gates/check_leaks.sh --debug-assertions-off` |
| Re-audit of the most-changed areas (Task 41 step 3) | here | the parity audit workflow over `src/terminal`, `src/apprt`, `pkg`, `include` |
| ~~Idle-machine performance runs and a new G8 baseline~~ DONE, P7-W23 (V17 DISCHARGED): idle run (load 1.65-1.85 at each start, Linux VM stopped) 14/14 wins, suite -27.9% wall; BASELINE.json's timing rows re-based to the pinned oracle, the Debug rows dropped by parity; G8b and G8 (`--strict`) GREEN. (P7-W19: oracle rebuilt at the pin, V18 DISCHARGED with N511. P7-W22: the size axis, V16 DISCHARGED with N512) | V17 | V17's P7-W19 recipe |
| The 16 new CI jobs and release-workflow changes on GitHub's runners | V25 | push, then read the run |
| Windows full-workspace tests on the clang-cl-built C/C++ dependencies | V26 | `cargo test --workspace --exclude libxml2-sys --exclude fontconfig-sys --exclude sentry-sys --target x86_64-pc-windows-msvc` |
| Nix and Snap packaging | V24 | user-deferred |
| FreeBSD, flatpak execution, macOS 27 AppKit, Windows console/font discovery | V10–V12, V19, V23 | environment-blocked |
| The new Valgrind CI job's runtime and software-GL noise on GitHub's x86_64 runners | N412, N413 | the `valgrind` job |
| Phase 7 deferrals: the paired benches against a zigbench built at `12752b2ac` (alloc, `sc.clone`, `render-*`, mode lookup, print, nerd-font lookup, `grapheme_break`), and the full Linux lane (the V27–V29 Miri debt was DISCHARGED by P7-W15 on 2026-10-06) | P7-W12 T2 | `docs/superpowers/plans/2026-09-28-phase7-upstream-resync.md` §P7-W12 Task 2 |

## Still open

Code gaps left after the final triage, none a ledger row:

- **N130**: GTK window titles that are not valid UTF-8 are decoded lossily.
  About 20 sites in 6 files. The GTK Rust bindings (glib-rs 0.22.8) build
  strings from GTK without a UTF-8 check, so reading such a title back through
  them is undefined behaviour; the fix needs raw `GValue` reads.
- **N11**: 65 renderer log arguments across about 15 error types still print
  Rust's `Debug` form where Ghostty prints `error.<Name>`. Log text only; 30
  sites are in Linux-only code, so the change needs the Linux VM. The shared
  error-name helper it needs (`rustty-zigstd` `ErrorName`) exists since N121.
- **N35**: a load-dependent `search_thread` test failure, never diagnosed; needs
  a long run under load.
- **N354**: a Windows test failure seen once, not reproduced.
- **N376**, 5 of 21 CI gaps: the wasm ABI-manifest step, Apple dylib install
  names, a Nix test Ghostty has and Rustty lacks, Ghostty's 5 MB tarball limit
  (Rustty's tarball vendors its crates, 43 MB), and the translations regenerate
  step.
- **N432**: a race Ghostty's own code shares (`async.zig:568`), unreachable in
  Ghostty; record only.
- Build difference: with the default `fontconfig` feature, Rustty builds
  fontconfig on GNU-ABI Windows and on macOS even when the chosen font backend
  does not use it; Ghostty builds it only for backends that do (N286).
- Opened at Phase 7 start (P7-W0 T4), each owned by a Phase 7 task, and all
  CLOSED by the finale:
  - **N435**: a reachable `unreachable!()` in the screen search's `feed`, which
    upstream fixed in `06178eeaa` (closed by P7-W5 T4).
  - **N436**: a use-after-free in `ghostty_search_tick` after the terminal is
    freed, which upstream fixed in `b1c264163` (closed by P7-W2 T1).
  - **N437**: on macOS the render thread formed whole-renderer references that
    overlapped the main thread's sync draw (aliasing-model UB; closed by P7-W7 T2).
- Phase 7's other findings (N438–N465) are all CLOSED except **N438**, which is
  record-only (not a UD, unreachable).
- **Sanitizer lane limits (P7-W14)**: G-ASAN and G-LSAN compile the vendored C/C++ uninstrumented
  (ASan sees its heap through the allocator interceptors, not its own loads and stores), and G-MSAN
  excludes `glslang-sys` and `xtask` and skips `rustty-renderer`'s EGL/GL/glslang test modules. Both are in
  `.port/phase-2/SANITIZER_SUPPRESSIONS.md` "Known limits". G-TSAN (P7-W17) runs ThreadSanitizer
  over the threaded crates, macOS only so far (V32). G-HWASAN (P7-W16b) instruments that C/C++ on aarch64
  Linux, but runs on rvm only (V31).
- **V27–V29**: Phase-7 Miri runs over the 30-minute task bound. DISCHARGED 2026-10-06 by P7-W15
  (G6, G6c and G6t GREEN at `04f0a894c`).

## Continuing this work

The closure was done on the `parity-closure` branch, merged into `main` on
2026-09-28; that branch and all batch branches are deleted. Continue on
`main`. The per-batch briefs, reports and controller log from the closure lived
in `.superpowers/sdd/2026-09-08-parity-defect-closure/`, which is gitignored
and local to the original machine; everything needed to continue is in the
committed records named here.

### Decisions only the maintainer can make

1. Whether and when to report the two private-report crashes, **UD131** and
   **UD351**, to Ghostty through a private security advisory
   (`UPSTREAM-DEFECTS.md` has the full recipes; do not open public issues).
   Before the repository is published, decide how the recipes in
   `UPSTREAM-DEFECTS.md` are handled; `docs/ghostty-bug-fixes.md` already
   withholds them.
2. The G6c hang limit, raised from 5400 s to 10800 s during the closure: keep,
   or split the lane.
3. The Sentry project name the release workflows assume (`rustty`, V25).

### Suggested order

1. **Run the deferred long runs** in the table above, starting with the full
   `ci.sh` on macOS and on the Linux VM, and the G-REL "can't find crate"
   failure. Any red result becomes a `NEW-FINDINGS.md` entry.
2. **Push once CI can run** and read the first run of the new CI jobs (V25);
   the ones most likely to need a fix are the musl target, Xcode 27 on the
   macOS image, `build-nix`, the `wasm-tools` version and `test-debian-13`'s time
   limit.
3. **Re-audit** the most-changed areas (Task 41 step 3 of the plan).
4. **Fix the open gaps** above: N11 (4–6 h, needs the Linux VM), N130 (about a
   day, Linux VM), then the N376 gaps.
5. **Performance**: idle-machine runs and a new G8 baseline (V16–V18).

### How work was run

- Rules: `.port/PORT_RULES.md` (R1 fidelity, R13 reference defects are fixed
  and recorded, never ported, R14 no silent skip). Procedure:
  `.port/parity-closure/PROCEDURE.md`.
- Records: `LEDGER.tsv` (11 tab-separated columns; edit by splitting on tabs,
  never by regex over the line), `NEW-FINDINGS.md` (N ids, kept in id order),
  `UPSTREAM-DEFECTS.md` (UD ids; every entry needs exactly one
  `- upstream status:` line) with `docs/ghostty-bug-fixes.md`,
  `.port/phase-2/VERIFICATION_DEBT.md` (V ids), `MIRI_EXCEPTIONS.md`,
  `ADJUDICATION.md`, `.port/phase-3/NON_PORTS.md`.
- Next free ids at closure: N435, UD352, V27.
- **Id register after Phase 7 (pin `12752b2ac`). Next free ids: UD373, N466, V30.**
  (The P7-W12 T1 brief named N465 as next free; the final review's follow-up,
  ruling 61, used it for the `Surface::close` finding.) Deliberate gaps in
  Phase 7's range, reserved during renumbering and never used, so a later audit
  need not look for them: **N439, N445, N446, N447, N457, N460.** UD352–UD372 and
  V27–V29 have no gaps.
- **P7-W12 T3** (the W12.T2 long-run fixes) used UD373, UD374 and N477–N482,
  the ids its brief assigned. N466–N476 were reserved for the W12.T2 long-run
  lanes (A, B and C) and never used, because every finding the lanes raised was
  recorded under W12.T3's ids. They are deliberate gaps, like the ones above.
  **Next free ids: UD375, N483, V30.**
- **P7-W12 T4** (the W12.T2 lane B2 Windows findings) used N484 and N485, the
  ids its brief assigned. N483 was reserved for P7-W12 T5 (gate portability).
- **P7-W12 T5** (gate portability) used four ids. N483 is CLOSED (the gate paths).
  N486 is OPEN (the Linux VT-headers gate and rvm's stale oracle `origin/main`).
  N487 is CLOSED (the Nix bindgen hook). N488 is CLOSED (three more Nix-lane
  defects behind N487).
- **P7-W12 T5 fix round 1** used UD375 (upstream's CMake soname links, the
  reference side of N488 item 1), N489 (OPEN: nixpkgs' wasmtime blocks the dev
  shell on aarch64-linux) and N490 (OPEN: `check_symbol_manifest.sh`'s header
  override and WARNING).
- **P7-W12 T6** (Phase 7 Miri annotations) used N491 (CLOSED: Phase 7 tests missed the G6-only
  annotation). **P7-W12 T6b** reopened and closed it again (one more missed Phase 7 test, now
  excepted in both lanes; three S7-6 tests measured and kept in G6); no new id. **Next free ids:
  UD376, N492, V30.**
- **P7-W12 T8** (a byte-wise Phase 7 test helper under Miri) used N493 (CLOSED: `raw_all_equal`
  compared byte by byte). N492 belongs to T7 and is not used here. **Next free ids: UD376, N494,
  V30.**
- **P7-W12 T7** (Tree Borrows UB in the `rustty-vt` search tests) used N492 (CLOSED: five tests
  with test-side Tree Borrows UB, plus one Miri-ignored test with the same shape, all fixed with
  the raw-root idiom; no production UB, so no UD).
- **P7-W12 T9** (Wuffs FFI under Miri in the `rustty-vt` kitty tests) used N494 (CLOSED: one test
  given a Miri corpus, 24 excepted). **Next free ids: UD376, N495, V30.**
- **P7-W14** (the G-ASAN, G-LSAN and G-MSAN gates) used N495 (CLOSED: `rustty-cli`'s Mach-O constructor
  could not link under ASan), N496 (CLOSED: the first sanitizer runs' test-side defects and lane
  configuration), N497 (CLOSED: a Miri exception claimed ASan coverage for a test no ASan run executed),
  N498 (CLOSED: `rustty_app_free` leaked the cloned configuration), N499 (CLOSED: the `screen-clone`
  benchmark's in-process tests leaked the `clone` step's copies) and V30 (the new `sanitizers` CI job,
  never run on GitHub). No UD: fix round 1 found the benchmark's process-exit freeing deliberate in the
  reference, so the UD376 first drafted for it was withdrawn and never merged.
  **Next free ids: UD376, N500, V31.**
- **P7-W15 / P7-W16** (Miri on nextest; the macOS leak lane; HWASan) ran in parallel and were
  given id ranges up front: P7-W15 N500 and N505; P7-W16a UD376 and N501–N503; P7-W16b N504 and V31
  (merged first: G-HWASAN, N504 the gate's first runs, V31 no hosted aarch64 CI job).
- **P7-W16a** (why sanitizers were BLOCKED on macOS) used, provisionally (other tasks may take ids in
  parallel; N500 is P7-W15's), N501 (CLOSED: G-LSAN was BLOCKED on macOS on a wrong premise; it runs there now) and UD376
  (the inspector's Dear ImGui Metal backend built without ARC leaks per frame; fixed with `-fobjc-arc`),
  N502 (CLOSED: per-surface teardown leaks on every platform, reached only by macOS tests; the Linux
  coverage gap, no Linux test closing a core `Surface`, stays OPEN; the render-thread and renderer-state blocks are
  now dropped after their `deinit`, proven on `mirihar-n444`; port-only, Ghostty allocates none of it,
  so no UD), N503 (CLOSED: the renderer's macOS test helpers, N411's twins) and, in fix round 2,
  N506 (an intermittent ASan-runtime deadlock in `fork` on macOS; the gate bounds each crate and
  reports a hang RED; CLOSED in P7-W21: ASan builds on macOS spawn the pty child with `posix_spawn`). No V id.
- **P7-W15** (the Miri lanes on `cargo miri nextest`, plus G6t) used N500 (CLOSED: test-side Tree Borrows UB in
  `snapshot::codec::tests::complete_snapshot_preserves_every_supported_continuation_cut`, the N492
  shape, found by G6t's first run, fixed with the raw-root idiom), N505 (CLOSED: the same shape in seven `rustty-vt-c` and `rustty-core` tests, found by the second G6t run; drafted as N501 and renumbered, since N501-N503 are reserved for P7-W16a) and N507 (CLOSED, gate-side: two G6 arms of the nextest port were stricter than the libtest lane, on `rustty-cli`'s disabled doc tests and `rustty-wasm`'s source-ignored test). P7-W15 discharged V27, V28 and V29.
- **Next free ids: UD377, N508, V32.** (P7-W15 and P7-W16a/b merged; P7-W15 took no UD or V id.)
- **P7-W17** (G-TSAN, ThreadSanitizer over the threaded crates) used UD377 (vendored Highway's
  chosen-target mask is a plain `int64_t` under `HWY_NO_LIBCXX`, so concurrent first SIMD dispatches race;
  fixed with relaxed atomic builtins), N508 (CLOSED: G-TSAN's first runs, UD377 and TSan's allocator
  limit) and V32 (G-TSAN has run on macOS only; the Linux run on rvm and the hosted `tsan` leg are pending).
- **P7-W18** (the full G-VALGRIND run on `rvm`) used N509 (CLOSED: Mesa EGL's driver data of the
  never-terminated display, suppressed with evidence) and N510 (CLOSED: a renderer read-back test,
  fixed in the test); no UD, no V id. Drafted as N520/N521 from a range set aside while P7-W17 (TSan)
  took ids in parallel, and renumbered at review to follow P7-W17's N508.
- **Next free ids: UD378, N511, V33.**
- **P7-W19** (lane E of P7-W12 T2: idle perf, G8 baseline, V16-V18) used N511 (OPEN: `apc-parser` 1.8x and
  `hyperlink-map` churn/lookup 1.26x/1.17x slower than Ghostty; the APC scan is a scalar port of a vector scan).
  V18 DISCHARGED; V16 and V17 stay open (V16: two of three mapped sizes larger than Ghostty's at 0% tolerance;
  V17: no idle window, so BASELINE.json was not re-based and G8 is RED on the retired series' `wcwidth` row).
- **Next free ids: UD378, N512, V33.**
- **P7-W20** (N511's perf work) CLOSED N511: `consume_apc_string`'s group test spelled with the NEON
  intrinsics the reference's `@Vector(16, u8)` lowers to (portable form elsewhere and under Miri), and four
  `#[inline]` hints plus a register sink for the hyperlink map. No new id. **Next free ids: UD378, N512, V33.**
- **P7-W22** (V16, binary size) used N512 (drafted as N520, renumbered at merge; CLOSED: `librustty_vt_c.dylib` 1.30x -> 1.076x Ghostty's, by
  `static` lookup tables and `[profile.release] panic = "abort"`). V16 DISCHARGED: BASELINE.json's size axis
  is the oracle at the pin, three labels mapped with per-label tolerances (the static `.a` dropped), G8b builds
  the counterparts (`--build`) and G8 runs `--strict`.
- **Next free ids: UD378, N513, V33.**
- **P7-W23** (V17, the idle perf run) DISCHARGED V17: 14/14 wins idle (suite -27.9% wall), BASELINE.json's
  `benchmarks` re-based to the oracle at the pin (Debug rows and the retired series kept beside them, not
  compared), `BASIS_CHANGED` emptied, G8 self-test arms [e2]/[f] made synthetic so they stay non-vacuous, G8b and
  G8 `--strict` GREEN. V17's drift bisection closed as outside the parity scope. N511's residuals re-measured idle
  (`apc-parser` and lookup about 1.10x stand; churn is parity, 0.99-1.03x). No new id.
- **Next free ids: UD378, N513, V33.**
- **P7-W24** (the three rvm items) CLOSED N486 (the VT-headers gate reads the `.so` with `nm -D` on Linux;
  `cbindgen` on rvm; rvm's oracle `origin/main` was already the pin; GREEN on the Mac and rvm), DISCHARGED
  V32 (G-TSAN GREEN on rvm; the hosted `tsan` leg waits on a push, as V30) and CLOSED N502's Linux coverage
  gap (new gate G-LSAN-GTK: a live GTK surface created and closed under ASan + LSan in a headless sway; RED
  with N502's fix reverted, GREEN with it). It used N513 (CLOSED: G-SAN-SELF's A4 Rust overflow arm could go
  GREEN under HWASan, 1 in 256), N514 (CLOSED: TSan's fd model reported `rustty-xev` tests run side by
  side; that crate's tests run one at a time under G-TSAN, `TSAN_SERIAL`), N515 (CLOSED: `rustty-xev`'s
  `dynamic_udp_read_write` could hang, one SO_REUSEPORT port for both backend iterations), N516 (OPEN:
  gtk4-rs's `BuilderRustScope` leaks every template-callback closure; suppressed in the GTK lane only, as
  `lsan-gtk.supp` G2, until fixed) and V33 (G-LSAN-GTK has no hosted job). No UD.
- **Next free ids: UD378, N517, V34.**
- **P7-W25** CLOSED N516: the GTK apprt binds its 111 template callbacks through GTK's C builder scope
  (`gtk_widget_class_bind_template_callback_full` with `extern "C"` trampolines,
  `gtk/class/template_callback.rs`), as the reference does, instead of gtk4-rs's leaking
  `BuilderRustScope`; `surface.blp` is the reference's again (no `swapped`). `lsan-gtk.supp` G2 removed;
  G-LSAN-GTK GREEN without it on rvm (the base with only G2 removed: RED, 279 allocations); the live GTK KATs
  pass. Also: G-LSAN-GTK pins each remaining suppression's match count under a ceiling
  (`LSAN_GTK_CEILINGS`; LSan treats suppressed blocks as live), the live KATs' `close_surfaces` asserts the
  window and every Rustty widget in it are finalized, and `TSAN_SERIAL` applies on Linux only (N514's
  record updated). No new id. **Next free ids: UD378, N517, V34.**
- **Phase 8 start (P8-W0, pin `a60e9e2a5`).** N517 (the parity-surface generator's stale line windows, P8-W0 T2)
  and N518 (a non-reflow narrow leaves a wide head in the last column, P8-W0 T1) are CLOSED. N519 (G-TESTS' four-token
  matcher masks 17 of the 90 added tests) is RECORD-ONLY (P8-W0 T4). N520 (the snapshot omits `flags.xt_checksum`),
  N521 (`rustty_terminal_paste` kept writing after a refused write) and N522 (the render `Device` lives in its own
  allocation) are reserved for their owners and not yet opened. N523, N524 and N525 are reserved for the gate defects
  P8-W0 T3 found; P8-W0 T5 opens and closes them. UD20 and UD312 are "fixed upstream" (their `docs/ghostty-bug-fixes.md`
  rows are gone); the reference fields of R4 Appendix A's 50 UDs are re-anchored at `a60e9e2a5` (UD32, UD250 and UD343 only in part: their old lines are history anchors of deleted code). No UD or V id is taken yet.
  **Next free ids: UD378, N526, V34.**
- **P8-W0 merged** (`9c42ba5e6`): N523 and N524 (G9 never checked CSI/ESC/C0; OSC `unknown` counted by a common
  word) are OPEN until their owners port `y`, `unknown` and `program_status`; N525 (c-vt corpus scripts wrote into the
  main checkout) and N526 (the freestanding wasm build broken since `ca2ef434b`; now guarded in the default `ci.sh`
  set) are CLOSED. **Next free ids: UD378, N527, V34.**
- **P8-W1..W7 merged** (to `4b171279a`): N518 CLOSED (W0 T1); N520 RECORD-ONLY (DECRQCRA flag not in the snapshot);
  N521 CLOSED (paste refused write); N522 PORT-ONLY (DeviceBox); N523 and N524 CLOSED by the W3/W4/W5 ports (G9
  777/777); N527 CLOSED (build-rustty.sh LDFLAGS); N528 CLOSED (C getter rejected MOUSE_SHAPE; provisional N550);
  N529 CLOSED (cbindgen same-name collisions made `include/rustty.h` declare wrong layouts, incl.
  `rustty_size_report_encode`; provisional N551; A5 arm (f) + arm (a5) guard it). No UD taken.
  **Next free ids: UD378, N530, V34.**
- Gates: `bash .port/tools/ci.sh` runs them all; each gate script in
  `.port/tools/gates/` has a `--self-test` where it matters. After merging
  records, re-run `check_parity_closure.py` and the phase-2 closure gate.
- Machines: the Linux lane ran on a Linux VM (Valgrind installed in the user's
  home), the Windows lane on a Windows 11 ARM64 VM over ssh and PowerShell,
  the Android lane needs the NDK r29 (removed from the Mac after the closure).
- Disk: a full `ci.sh` grows `target/` to about 48 GB; run at most three
  building checkouts at once on the Mac.
