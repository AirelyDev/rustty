# Memory-safety lanes: faster Miri, plus a sanitizer matrix

Status: DRAFT for review (2026-10-01). No gate changes until approved.
Decisions taken by the maintainer on 2026-10-01:

1. Keep Miri.
2. Restructure it around `cargo miri nextest`: Stacked Borrows over the full suite, Tree Borrows only on the
   pointer-heavy modules.
3. Make AddressSanitizer the broad everyday check, with ThreadSanitizer for threaded code.

Also add LeakSanitizer and MemorySanitizer.

## Why

Rustty ports Ghostty's pointer-heavy Zig design onto Rust raw pointers next to references. Only Miri checks
Rust's aliasing rules, and it has found real UB here:

- search's stale screen aliases (9 tests);
- the N437 render-thread overlapping references;
- the WS7 Phase E uninit page mapping;
- N391;
- the 10 violations hidden by blanket ignores (WS19 T6).

Ghostty itself relies on Zig runtime safety, a leak-checking test allocator and a Valgrind CI job. Zig has no
borrow rules, so it has no aliasing-model UB.

The problem is the shape and cost of the lanes, not their value:

- **G6** (`check_leaks.sh --all-done`) runs every crate as one serial libtest process per crate. `rustty-vt`
  alone takes about 8 h.
- **G6c** (debug assertions off) uses 124 libtest substring filters. Under Tree Borrows, libtest's filter phase
  alone took more than 2 h without starting a test (2026-10-01).
- One Miri process stops at the first UB, so a single finding hides every later test in that binary. G6 on
  2026-10-01 lost every vt test after the kitty Wuffs test that way (N494).
- Tree Borrows has only been run by hand (V27–V29, P7-W12 T7), never as a lane.
- **Sanitizers:** `gates/sanitizers.sh` exists (ASan, TSan, LSan, MSan, HWASan; one crate per call), but no
  `ci.sh` gate runs it. ASan is only used ad hoc to back Miri exceptions.

## Design

### A. Miri on nextest (G6, G6c, plus a new G6t)

`cargo miri nextest run` runs each test in its own Miri process, in parallel across cores.

- **No filter phase.** Test selection uses nextest filtersets (`-E 'test(=name)'`, `-E 'test(/^pagelist::/)'`),
  resolved from a native-speed test list instead of interpreting libtest's filter under Miri. This removes the
  G6c TB stall entirely.
- **Parallel:** `--test-threads N` processes; N defaults to the number of performance cores minus 2.
- **Isolation:** one UB stops one test, not the binary. Every test still runs, so one finding no longer hides
  the rest.
- **Leak checks are unchanged.** Miri's leak check runs at each process exit, which is now per test. That is
  stricter, if anything.

The lanes become:

| Lane | Model | Debug assertions | Scope | Today | Target (estimate, to measure) |
|---|---|---|---|---|---|
| **G6** | Stacked Borrows | on | every crate the lane covers today (same crate list, same exceptions) | about 8 h for vt, serial | 1–2 h total |
| **G6c** | Stacked Borrows | off | the 124 `all(miri, debug_assertions)` tests, still derived from the annotations | about 1.75 h, serial | 15–30 min |
| **G6t** (new) | Tree Borrows | on, or off for the G6c set | pointer-heavy modules (below) | only by hand | under 1 h |

**G6t scope:** the modules that hold long-lived raw aliases. Defined as a filterset in one place, with a
self-test that every listed module exists and matches more than 0 tests:

- `rustty-vt`: `page`, `pagelist`, `search`, `render`, `screen`, `terminal`, `snapshot`, `kitty`
- `rustty-vt-c` (FFI handles), `rustty-renderer` (cross-thread), `rustty-xev` (intrusive raw pointers),
  `rustty-core` (surface, renderer seam)

Two consequences:

- **Choice of primary model:** Stacked Borrows stays the full-coverage model. It is stricter, and most of this
  project's real findings were SB. Tree Borrows is the forward-looking model and catches some patterns SB
  accepts (N492). It runs where aliasing is dense.
- **R5 (gates never weakened):** G6 and G6c keep the same test set, the same exceptions and the same pass
  criteria. G6t is purely additive. Keep the libtest path behind a `--libtest` switch for one release, so
  results can be compared.

**Needed:**

- `cargo-nextest` installed on the Mac and on rvm. It's not installed today.
- `check_leaks.sh` rewritten around a `run_miri_nextest <crate> <model> <filterset>` helper.
- Self-tests: a deliberate-UB fixture is RED under both models; an empty filterset is a vacuous RED; a G6c
  annotation that loses its flag is RED. Keep the existing arms.
- `MIRI_EXCEPTIONS.md` and `VERIFICATION_DEBT.md` recipes updated to nextest commands.
- `miri-run.sh` (G-MIRIRUN-SELF) is checked for overlap; fold it into the helper if it's redundant.

### B. Sanitizer matrix (new gates G-ASAN, G-LSAN, G-TSAN, G-MSAN)

`sanitizers.sh` is extended from one crate per call to a workspace matrix, and registered in `ci.sh`.
Platform support comes from the script's own probe plus rustc's supported-target list. Re-verify each entry
when implementing.

| Sanitizer | Catches | Where | Scope |
|---|---|---|---|
| **ASan** (G-ASAN) | OOB, use-after-free, double-free, stack and heap overflow, including the C `-sys` code | macOS and Linux | the whole workspace test suite, including the Miri-excepted tests. It becomes the everyday broad check. |
| **LSan** (G-LSAN) | leaks, including leaks inside C | Linux only (rustc rejects it on macOS) | run as ASan with `detect_leaks=1` on rvm and in CI. A suppressions file for known third-party leaks is allowed only with a cited reason, in `SANITIZER_SUPPRESSIONS.md`, following MIRI_EXCEPTIONS-style rules. |
| **TSan** (G-TSAN) | data races | macOS and Linux (`-Zbuild-std`) | the threaded crates: `rustty-core` (render thread, mailbox), `rustty-termio` (read thread, exec), `rustty-xev`, `rustty-renderer`, the vt search thread, `rustty-vt-c` |
| **MSan** (G-MSAN) | reads of uninitialized memory | Linux only. rustc lists aarch64 and x86_64 Linux; the script's probe says x86_64 only, so verify on rvm (aarch64), otherwise run it in GitHub CI (x86_64). | needs `-Zbuild-std`, and every C dependency instrumented. Phase 1: pure-Rust crates, or crates whose C dependencies we compile ourselves with `CFLAGS=-fsanitize=memory` through `cc`. Crates with uninstrumentable C, such as system libraries (GTK, fontconfig), are excluded with a cited reason. |

**Common rules,** following the existing `sanitizers.sh` rules:

- The non-vacuity rule: zero tests run is RED.
- Every finding is RED.
- A build failure is BLOCKED, not GREEN.
- Each gate prints the per-target test totals.
- Each sanitizer gets its own target dir under `target/san-<name>`.
- On Linux the gates run on rvm. The VM-disk rules apply: delete the clone, then trim.

**Interaction with Miri exceptions:** today a Miri exception claims "covered natively + by ASan", and nothing
checks that. With G-ASAN in `ci.sh` running the whole workspace, that claim becomes verified. Add a check that
every Miri-excepted test actually ran under G-ASAN.

### C. Cadence and where each lane runs

| Tier | When | Lanes |
|---|---|---|
| **Per change** (the post-merge gates) | every task merge | the existing gates, plus G-ASAN scoped to the changed crates, plus G6 on the changed crates only, via nextest |
| **Full** (`ci.sh`) | before merging to main, at phase finales | G6, G6c, G6t, G-ASAN, G-LSAN (rvm), G-TSAN, G-MSAN (rvm or CI) |
| **Release / weekly** | release, upstream resync | everything above, plus G-VALGRIND (Ghostty parity) and a G5 fuzz session under ASan |

GitHub CI (after the next push, V25) gets ASan, LSan, TSan and MSan jobs on x86_64 Linux, plus the Miri lanes
on a schedule.

## Rollout

1. **Measure first.** Install nextest. Run vt `page`/`pagelist`/`search` under `cargo miri nextest` (SB) and
   record the wall time against the libtest run already measured on 2026-10-01:
   - V27 `page::`: 116 tests, 3,054 s;
   - V28 `pagelist`: 270 tests, 8,379 s;
   - V29 `search::`: 52 tests, 289 s.

   Go ahead only if the gain is at least 3×.
2. Rewrite `check_leaks.sh` around nextest (G6 and G6c), with its self-tests, and compare against the libtest
   results.
3. Add G6t.
4. Extend `sanitizers.sh` into the matrix. Add G-ASAN on the Mac, then G-LSAN, G-TSAN and G-MSAN on rvm.
   Triage the first findings.
5. Register the gates in `ci.sh`. Update `MIRI_EXCEPTIONS.md`, `VERIFICATION_DEBT.md` and
   `.port/parity-closure/README.md`.
6. Record findings with the usual ids (UD, N, V).

## Risks and open questions

- **Miri under nextest:** startup cost per test, since each process re-interprets setup. The setup is small for
  vt; measure it in step 1.
- **First runs may surface real findings:** first-ever runs of LSan, MSan and the full ASan suite are likely to
  find things in C dependencies or in Rust code. Budget triage time. A finding in vendored C code is recorded,
  not ignored.
- **MSan on aarch64 Linux is unverified.** The fallback is GitHub CI x86_64, which needs a push (user's call).
- **Disk:** each sanitizer and Miri lane builds its own target, 5–15 GB each. Run the lanes sequentially on
  the Mac and on rvm.
- **G6t's model:** whether G6t should also run with debug assertions on for the G6c set. Proposed: same split
  as G6/G6c.
