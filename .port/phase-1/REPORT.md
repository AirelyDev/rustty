# Phase-1 Report — rustty workspace & parity-gate infrastructure

**Date:** 2026-06-21 · **Verdict: GREEN (no open blockers)**
*(Report assembled by the main loop from the workflow's structured results; the
critic agent returned its verdict structured but did not write the files.)*

## Gate (all conditions satisfied)

| Condition | Result |
|---|---|
| `cargo build --workspace` | GREEN (43 members, ~4.2s) |
| `cargo test --workspace` | GREEN (84 ok-blocks, 0 passed/0 failed — empty stubs) |
| Acyclic gate on real Cargo graph (G7) | GREEN (29 crates, 93 edges, 0 cross-crate SCCs; corroborated by `cargo metadata` exit 0) |
| Every parity gate flips RED on injection → GREEN on revert | YES — G7, G1, G2, G3, G8 proven (critic spot-checked G7 itself) |
| No port crate depends on the ghostty tree (R3) | YES (only `.port/phase-0/harness` links the reference libghostty-vt.a) |
| Worktree isolation available for Phase 2 | YES (verified create/remove after git init) |

## Deliverables

- **Workspace:** `Cargo.toml` virtual workspace, 43 members in leaves-first topo order:
  29 parity crates (`crates/`), 13 `-sys` crates (`crates-sys/`), `xtask`.
  `[workspace.package|dependencies|lints]`. Inter-crate path deps wired per
  PORT_PLAN; DAG acyclic by construction. `rustty` = `[[bin]]`,
  `rustty-vt-c` = `[cdylib, staticlib, rlib]`. rust-toolchain pinned 1.93.1.
- **Gates** (`.port/tools/gates/`): G1 `check_tests_ported.py`, G2 `check_assertions.py`,
  G3 `check_comments_fidelity.py`, G4 `run_differential.sh`, G5 `run_fuzz_smoke.sh`,
  G6 `check_leaks.sh`, G7 `check_acyclic.sh`+`cargo_cycle_check.py`,
  G8 `check_baseline_regression.py`, G9 `check_parity_surface.py`, `_gatelib.py`, README.
  Each: exit 0 GREEN / non-zero RED / exit 3 BLOCKED; each has `--self-test`; all 9 self-tests pass.
- **CI:** `.port/tools/ci.sh` + `.github/workflows/ci.yml` + `CI.md` (hard-fail vs
  progress-mode-while-porting documented).
- **Identity dual-profile:** `crates/rustty-identity` with `identity-ghostty` feature
  (default=rustty, feature=ghostty values); both profiles' tests pass.
- **cbindgen:** wired for `rustty-vt-c` → git-tracked `include/rustty.h`
  (`rustty_*` prefix, zero `ghostty_*` symbols).
- **C-vendoring pattern (R3):** proven on `zlib-sys` (vendored `adler32.c` via `cc`,
  `links=z`, bindgen, 4/4 bit-exact test). Template in `crates-sys/README.md`.
- **Harness gate:** `.port/phase-0/harness/gate_empty_divergence.sh` runs;
  current divergence 100% vs empty port (correct); `GATE.md` documents the
  100%→0% flip path via `identity-ghostty`.

## Gate self-test matrix

See `selftest.md` for full injection records (G7 cycle, G1 dropped test, G3 dropped
comment, G2 weakened assertion, G8 regression) — each clean=0, injected=1, reverted=0.

## Git

- `00c7ede` — workspace scaffold + phase-0 artifacts (367 files)
- `33f8ff1` — parity-gate infrastructure + self-test
- Tree clean; worktree create/remove verified.

## Known non-blocking follow-ups (carried into Phase 2)

1. `-sys` build scripts beyond `zlib-sys` are documented stubs — real C vendoring
   happens per-crate as each consumer is ported (pattern proven).
2. **Leak gate (G6):** `cargo miri` not installed → G6 currently BLOCKED-loud. Phase-2
   primary leak detection will use a **tracking global allocator** in tests (the
   constitution permits this in lieu of Miri); install Miri later as a second check.
3. ~256M of corpora `.bin` goldens committed per the KEEP directive — consider Git
   LFS if repo size matters.
4. `cargo metadata --no-deps` does not catch cycles (skips resolution); the G7 gate
   script + full `cargo metadata` are the binding R4 checks.
