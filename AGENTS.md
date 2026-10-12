# Agent Development Guide

A file for [guiding coding agents](https://agents.md/).

## Commands

- **Build:** `cargo build`
  - To skip the macOS app and speed up compilation, just build the workspace
    crate you need — `cargo build -p <crate>` (e.g. `cargo build -p rustty-vt`).
- **Test:** `cargo test`
  - Prefer to run targeted tests because the full suite is slow. Scope to a
    crate with `-p <crate>` and filter by name with `cargo test <name>`.
- **Test filter:** `cargo test -p <crate> <test name>`
- **Formatting (Rust):** `cargo fmt` (and `cargo clippy` for lints)
- **Formatting (Swift):** `swiftlint lint --strict --fix`
- **Formatting (other):** `prettier -w .`

## librustty-vt

- Build: `cargo build -p rustty-vt-c --release`
- Build WASM: `cargo build -p rustty-vt-c --release --target wasm32-unknown-unknown`
- Test: `cargo test -p rustty-vt-c <filter>`
  - Prefer this when the change is in a librustty-vt file. Run the affected
    tests under Miri (`cargo +nightly miri test -p rustty-vt-c`) when touching
    raw-pointer or FFI code.
- All C enums in `include/rustty/vt/` must have a `_MAX_VALUE = RUSTTY_ENUM_MAX_VALUE`
  sentinel as the last entry to force int enum sizing (pre-C23 portability).

## Directory Structure

- Cargo workspace crates: `crates/` — the crown-jewel crates are `rustty-vt`
  (VT/terminal engine), `rustty-core`, and `rustty-renderer`, alongside the
  supporting crates (`rustty-config`, `rustty-font`, `rustty-input`, …).
- macOS app: `macos/`
- GTK (Linux and FreeBSD) app: `crates/rustty-apprt`.

## Syncing with upstream Ghostty

Rustty is a port of Ghostty, pinned to one upstream commit (the oracle). The
oracle is a read-only checkout at `../ghostty`, and the pin is in
`.port/tools/gates/ORACLE_PIN`. The rule for every decision is parity with
Ghostty at the pin, except never port a Ghostty bug (`.port/PORT_RULES.md`
R13).

To absorb new upstream commits (a "resync"):

1. **Scope the range.** In the oracle, `git fetch origin main`, then list the
   commits since the pin. Split them into workstreams by area (vt, termio,
   renderer, config, apprt, and so on).
2. **Advance the pin first,** following `.port/ORACLE_SYNC.md`, "Advancing the
   pin". Do it only when no other lane is active. Record the gates' before and
   after state, and update `ORACLE_PIN`, `.port/ORACLE_SYNC.md`,
   `.port/PORT_RULES.md` (R12) and `.port/cycle-analysis.md` in one commit.
3. **Port each upstream change exactly** into its Rust counterpart:
   - keep the same structure, comments and tests;
   - use the renames in R2;
   - follow the fidelity rules in R1 and R7–R11;
   - if the upstream change has a reachable crash, UB, leak or deadlock, fix it
     instead, and record it in `.port/parity-closure/UPSTREAM-DEFECTS.md` and
     `docs/ghostty-bug-fixes.md` (R13).
4. **Record the phase** under `.port/phase-<n>/`: what was absorbed, the new
   finding and defect ids, and the evidence. `.port/phase-8/README.md` is a
   worked example.

## Validating a change

All checks run through `.port/tools/ci.sh`; `.port/tools/CI.md` documents each
gate. The hosted workflow `.github/workflows/ci.yml` calls the same script.

- **While working:** run only what your change touches:
  - targeted `cargo test -p <crate> <filter>`;
  - the gate scripts for the area you changed;
  - `bash .port/tools/ci.sh --changed-since <rev>`, which scopes the long gates
    (Miri, sanitizers, Valgrind) to the crates changed since `<rev>`. It is
    fix verification, never a release verdict (exit 5).
  - `--tier product` runs only the generic project gates and skips the
    oracle-parity ones (`--tier parity` does the reverse). Either is a subset,
    never a full pass (exit 4).
- **At the end of a resync,** do one full run. Fix-only reruns verify the
  fixes found in it; don't repeat the full run.
  - `bash .port/tools/ci.sh`, which runs every gate in parallel (`--jobs auto`),
    including Miri (G6, G6c, G6t), the sanitizers and the oracle-parity gates.
  - The lanes the host can't run report BLOCKED (exit 3), never green: Valgrind,
    MSan, HWASan and the GTK tests need Linux; the Windows build and tests need
    Windows. Run them on such a host.
  - Perf: on an idle machine, the perf suite must keep winning against the
    oracle built at the pin (`.port/tools/perf/`), then re-base
    `.port/phase-0/baseline/BASELINE.json` and run G8 `--strict` and G8b.
    Store or refresh G8b's entry binary (`check_perf_fence.sh --store-entry`)
    only on purpose, at a phase close-out after a GREEN perf verdict, never
    automatically.
  - Each RED becomes a recorded finding, then a fix verified by a scoped rerun.
- **Never weaken a gate to pass (R5).** BLOCKED is not green, a narrowed run is
  not a full pass, and a skipped test needs a written reason (R14).

## Issue and PR Guidelines

- Never create an issue.
- Never create a PR.
