# Audit coverage and verification ledger

## Audit universe

- Rustty checkout: `/Users/gael/projects/rustty`
- Rustty commit: `5243303fbb2f2cfd0832edfb8fa49d72ce322d81`
- Ghostty checkout: `/Users/gael/projects/ghostty`
- Ghostty commit: `6ad1fe7d8cbda36c77b337a96c9bea8a77883699`
- Host: `aarch64-apple-darwin`, Rust 1.93.1, Zig 0.16.0
- Installed cross targets: `wasm32-unknown-unknown`, `wasm32-wasip1`,
  `x86_64-pc-windows-msvc`; no Linux or FreeBSD Rust standard library.
- The Rustty root comment still names old oracle `4749c4e`. This audit used the
  actual requested current Ghostty checkout, not that stale comment.

## Whole-tree inventory

| Check | Observed result | Interpretation |
| --- | --- | --- |
| Live file coverage | 498 oracle files; 485 counterparts, 13 recorded non-ports, 0 missing/thin | Every current oracle source file is mapped or explicitly dispositioned. This is a triage invariant, not proof of complete behavior. |
| Live public-symbol coverage | 2,554 `pub fn` across 485 oracle files; 0 unexplained, 13 accepted | No unrecorded public function absence. Accepted/documented absences still received manual marker review. |
| Live test coverage | 3,390 oracle tests across 485 files; 0 unexplained, 3 accepted | One accepted current absence is the allocator-failure PageList test promoted as RV-007. |
| Per-module test counts | All mapped modules have Rust test count at least the Zig count | Count parity passes, but it cannot detect weakened/misbinding assertions. |
| Production marker sweep | 260 non-test hits for `unimplemented!`, `todo!`, `compile_error!`, `NOT PORTED`, `DEFERRED`, or `UNSUPPORTED` | Each consequence-backed non-macOS hit was traced; stale historical comments and unreachable/test-only markers were rejected. |

Commands:

```text
python3 .port/tools/audit_file_coverage.py
python3 .port/tools/audit_symbol_coverage.py
python3 .port/tools/audit_test_coverage.py
python3 .port/tools/gates/check_test_counts.py
```

## ABI and behavioral verification

- Current C headers: all published headers compile standalone as C11 and C++17;
  185 declared symbols are real exports; all 185 current Ghostty lib-vt exports
  have Rustty counterparts; 35 `sizeof`, 127 `offsetof`, and 574 enum
  discriminant checks pass.
- Current differential sanity: freshly built current Rustty and Ghostty libraries,
  with an audit-only constructor compatibility shim for the stale driver, matched
  byte-for-byte on all 12 curated VT fixtures. The checked-in gate additionally
  reports 21 C golden cases, but its checked binaries are stale (TI-001), so that
  result is not treated as current-oracle proof.
- Custom allocator: a paired current-library C reproducer verified that Ghostty
  invokes a rejecting allocator and returns OOM while Rustty never invokes it and
  returns success.
- Windows: `cargo check -p rustty-os --target x86_64-pc-windows-msvc --offline`
  reaches Rust source and fails with 25 errors. `rustty-xev` passes for the same
  target as a toolchain/control check.
- WASM: the VT C artifact builds, but inspection finds a private fixed-size table;
  the repository patcher makes it growable without exporting it. Current Ghostty's
  artifact exports an unbounded table.
- Host controls passed for focused OS, shell-integration, terminfo, PageList
  continuation/preservation, and selection-formatting tests.

## Existing gate weaknesses found during the audit

Two old frozen-inventory gates report RED for reasons that do not represent
current product gaps:

1. `check_assertions.py` says `os::desktop environment` is 9 versus 10. Current
   Ghostty changed that test from process-environment mutation to local maps and
   now has a different assertion shape; Rustty matches the current behavior. The
   gate compares a frozen count.
2. `check_tests_ported.py` reports removed/renamed/moved tests (`fromHex`, old OS
   env/args tests, and old VT names). Current live-oracle auditing resolves them;
   many still exist under new names or moved crates. The hard result is stale,
   not evidence of missing functionality.

Conversely, the live test audit accepts the missing allocation-failure PageList
test because it is documented. Manual review established that the accepted
absence corresponds to a real recoverable-OOM semantic gap. A recorded exception
is not the same as parity.

## Platform and runtime coverage

Reviewed in paired source and target graphs:

- Windows OS, page allocation, PTY/ConPTY, VT C embedding, and xev control.
- FreeBSD runtime selection, GTK gates, renderer exports/dependencies, xev kqueue,
  and core consumers.
- Linux GTK application lifecycle, child environment, Wayland/X11 selection,
  desktop resources, OpenGL renderer draw path, scrollbar routing, and inspector.
- Browser/WASM VT, web-canvas and main WASM build reachability, exports/imports,
  indirect table structure, and callback APIs.
- Shared VT, C ABI, allocator/error paths, compression, config/input/OS/termio,
  font discovery, shell integration, terminfo, crash, inspector, and renderer
  markers.

## Limits

- No Linux GUI session or FreeBSD runtime was available. Linux/FreeBSD findings
  are deterministic cfg/dependency contradictions or explicit live-path omissions,
  not claims of interactive execution.
- The FreeBSD target standard library is absent, so later failures beyond the
  three independently proven FreeBSD roots may remain.
- The macOS product was used only for shared-library builds and controls; macOS
  parity itself was excluded by request.
- Windows native SDK-dependent C/C++ failures were not promoted because this host
  cannot distinguish missing SDK/sysroot setup from port defects. The reported 25
  Rust errors do not depend on that SDK.
- No fuzz campaign could be run because Rustty has no fuzz target. Randomized unit
  tests and synthetic corpora are mitigations, not replacements.

