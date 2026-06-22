# Batch-1 Parity Gate Report

Date: 2026-06-21
Reference (READ-ONLY oracle): /Users/gael/projects/ghostty/src @ 4749c4e
Workspace: /Users/gael/projects/rustty (branch phase-1-parity-gates)

## VERDICT: GREEN

All 4 done crates pass tests==contract, assertion-for-assertion, comment fidelity,
Miri (UB+leak) clean, clippy `-D warnings` zero, fmt clean, and all three reviews
land at PARITY (the one NOT_PARITY blocker was fixed — see below). The full
workspace builds and tests green and the crate graph is acyclic.

## Workspace-level gates (exit codes captured to a var, never piped through tail)

| Check | Command | Result |
|---|---|---|
| fmt | `cargo fmt -- --check` | GREEN (RC=0) |
| clippy | `cargo clippy --workspace -- -D warnings` | GREEN, ZERO warnings (RC=0) |
| build | `cargo build --workspace` | GREEN (RC=0) |
| test | `cargo test --workspace` | GREEN (RC=0) |
| G7 acyclic | `check_acyclic.sh` | GREEN — no cross-crate cycle |
| G4 differential | `run_differential.sh` (control) | GREEN |
| G6 Miri | `check_leaks.sh --all-done` | GREEN — all done crates clean |
| R3 no-ghostty-dep | grep Cargo.toml + sources | SATISFIED (only doc comments name ghostty as oracle) |

ci.sh itself reports G6 as a HARD FAILURE only because ci.sh invokes
`check_leaks.sh` with no argument (it prints usage and exits 2). The canonical
Miri gate per the task is `check_leaks.sh --all-done`, which is GREEN (RC=0).
Every other ci.sh check (fmt, clippy, build, test, G7, G4) is PASS; G1/G2/G3/G9/G8/G5
are progress-green.

## Tests ported

Contract total test blocks: 3234. Batch-1 done crates (crate_map): util=4,
build-config=0, lib=13, datastruct=76. **Batch-1 tests ported = 93 / 3234.**
Per-crate `cargo test -p` counts match the contract exactly.

## Per-crate

| Crate | Tests (ported/contract) | Assertions (rust/ref) | Comments G3 | Miri | clippy | fmt | Review |
|---|---|---|---|---|---|---|---|
| rustty-util | 4 / 4 | 13 / 5 | green (317>=218) | green (4) | green | green | PARITY |
| rustty-build-config | 0 / 0 | 0 / 0 | green (288>=33) | green (0) | green | green | PARITY (fixed) |
| rustty-lib | 13 / 13 | 30 / 27 | green (395>=123) | green (13) | green | green | PARITY |
| rustty-datastruct | 76 / 76 | 530 / 311 | green (1418>=855) | green (76) | green | green | PARITY |

All G1 (check_tests_ported --strict) report `count == contract; name-sets match`.
All G2 (check_assertions --strict) report `rust >= ref everywhere`.
All G3 (check_comments_fidelity --strict) report `per-crate delta >= 0`.

## NOT_PARITY review finding ADDRESSED

The independent adversarial review of **rustty-build-config** returned NOT_PARITY
with one blocker:

> DROPPED reference decl + doc comment: ghostty src/build_config.zig lines 23-28
> define `pub const mode_string` with the doc comment `/// The optimization mode
> as a string.`. This public const and its doc comment were ENTIRELY ABSENT from
> the Rust port, masked by the aggregate per-crate comment-count gate.

Verified against the pinned reference (build_config.zig:23-28) and confirmed the
grep across crates/ returned nothing. This is a genuine R1 fidelity loss.

**FIX** (crates/rustty-build-config/src/lib.rs): ported `pub const fn mode_string()
-> &'static str` with the doc comment `/// The optimization mode as a string.`
preserved verbatim and the full reference body reproduced inside a ```text block.
The optimization mode is reconstructed from `cfg!(debug_assertions)` (Rust has no
`OptimizeMode` reflection), consistent with how `IS_DEBUG` and
`SLOW_RUNTIME_SAFETY` are derived in the same file: `"Debug"` in debug builds,
`"ReleaseFast"` in optimized builds.

Re-verification after fix: grep now finds both `mode_string` and the doc comment;
fmt/clippy/build/test/G1/G2/G3/Miri all GREEN; comment count rose 270 -> 288.
rustty-build-config now lands at PARITY.

## Prior FIX batch (clippy/fmt + raw-pointer soundness, commit 730fb79)

Carried forward and re-confirmed green. Highlights:
- clippy warnings fixed: approx_constant x3, not_unsafe_ptr_arg_deref x2,
  should_implement_trait x3, needless_range_loop x5, enum_variant_names x1,
  len_without_is_empty x1, empty_line_after_doc_comments x2, unused_braces x3.
- soundness: split_tree init -> `pub unsafe fn`; convenience.rs FFI module-level
  allow; lib ZigSlice to avoid SB UB.
- parity_allows: kept 3.14159/3.14158 literals, inherent next(), Backtrack variant,
  index border loops, added is_empty() without removing empty().

## Soundness decisions

- split_tree init: `pub unsafe fn` encodes caller-must-pass-valid-view (mirrors
  reference `init(gpa, view: *View)`); 37 test call sites wrapped in unsafe.
- convenience.rs: module-level allow for extern "C" Wasm/C allocator exports (FFI
  boundary, ABI-neutral).
- lib allocator: ZigSlice(ptr,len) instead of &mut[u8] to avoid Stacked-Borrows
  UB on realloc/free.
- datastruct lru placeholder_value: mem::zeroed bounded V:Copy — sound for the
  integer value types stored, always overwritten before read; Miri clean.

## Parity allowances (verified against reference)

- comparison.rs kept float literals 3.14159/3.14158 (comparison.zig:262-263).
- inherent next() public API kept (circ_buf.zig:34, blocking_queue.zig:171,
  split_tree.zig:202); not converted to std::iter::Iterator.
- split_tree Backtrack variant name kept (split_tree.zig:322).
- circ_buf added is_empty() delegating to empty(); empty() retained.
- build-config mode_string reconstructed from debug_assertions; doc comment +
  reference body preserved.

## R3 (self-contained)

No rustty crate has a Cargo path/git dependency on the ghostty tree. The only
matches for "ghostty" in Cargo.toml/sources are doc/comment annotations declaring
the reference is an external oracle only.
