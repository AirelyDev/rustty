# Parity closure record

Closure of the functional-parity audit of Rustty against Ghostty at pin
`20abdb50a` (`docs/parity-audit/README.md`). Plan:
`docs/superpowers/plans/2026-09-08-parity-defect-closure.md`. Work ran from
2026-09-08 to 2026-09-28.

## Result

- **Ledger (`LEDGER.tsv`), 497 audited findings: 403 CLOSED, 94 WAIVED, 0 OPEN.**
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
  so the numbering has gaps).
  Their states were re-triaged on 2026-09-28 (task T-1). What is still open is
  listed below.
- **Reference defects (`UPSTREAM-DEFECTS.md`).** 178 Ghostty defects (UD ids)
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

## Not verified here (long runs, deferred by the user on 2026-09-28)

These were deliberately not run to finish the closure. Each has an exact
command in `.port/phase-2/VERIFICATION_DEBT.md` or the named record.

| What | Where recorded | Command |
|---|---|---|
| Full `ci.sh` end to end on macOS and Linux (Task 41 step 4) | here | `bash .port/tools/ci.sh`; on Linux `ssh rvm 'cd ~/projects/rustty && bash .port/tools/ci.sh'` |
| G-REL: 17 release doc-test targets failed with "can't find crate" in the last full run, and pass when run alone; cause unknown | task-C-1 report | `bash .port/tools/gates/check_release_lane.sh` |
| G6 Miri as one continuous lane (verified crate by crate only; about 8 h for rustty-vt) | `MIRI_EXCEPTIONS.md` | `bash .port/tools/gates/check_leaks.sh --all-done` |
| G6c as the gate runs it (119 tests, about 105 min; its hang limit was raised from 5400 s to 10800 s, pass criteria unchanged) | task-C-1 report | `bash .port/tools/gates/check_leaks.sh --debug-assertions-off` |
| Re-audit of the most-changed areas (Task 41 step 3) | here | the parity audit workflow over `src/terminal`, `src/apprt`, `pkg`, `include` |
| Idle-machine performance runs and a new G8 baseline | V16–V18 | see `task-S-4-report.md` |
| The 16 new CI jobs and release-workflow changes on GitHub's runners | V25 | push, then read the run |
| Windows full-workspace tests on the clang-cl-built C/C++ dependencies | V26 | `cargo test --workspace --exclude libxml2-sys --exclude fontconfig-sys --exclude sentry-sys --target x86_64-pc-windows-msvc` |
| Nix and Snap packaging | V24 | user-deferred |
| FreeBSD, flatpak execution, macOS 27 AppKit, Windows console/font discovery | V10–V12, V19, V23 | environment-blocked |
| The new Valgrind CI job's runtime and software-GL noise on GitHub's x86_64 runners | N412, N413 | the `valgrind` job |

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
- The app-runtime error type carries no detail, so a keybind-triggered config
  reload logs a generic error where Ghostty logs the error name (N311).

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
- Gates: `bash .port/tools/ci.sh` runs them all; each gate script in
  `.port/tools/gates/` has a `--self-test` where it matters. After merging
  records, re-run `check_parity_closure.py` and the phase-2 closure gate.
- Machines: the Linux lane ran on a Linux VM (Valgrind installed in the user's
  home), the Windows lane on a Windows 11 ARM64 VM over ssh and PowerShell,
  the Android lane needs the NDK r29 (removed from the Mac after the closure).
- Disk: a full `ci.sh` grows `target/` to about 48 GB; run at most three
  building checkouts at once on the Mac.
