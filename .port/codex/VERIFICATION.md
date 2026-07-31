# Final verification record

Run date: 2026-07-31. All commands below were rerun after canonical synthesis.

## Whole-tree inventory

| Check | Fresh result |
| --- | --- |
| `audit_file_coverage.py` | Exit 0: 498 oracle files, 485 OK, 13 written non-ports, 0 missing, 0 thin. |
| `audit_symbol_coverage.py` | Exit 0: 2,554 public functions, 0 unexplained, 13 accepted. |
| `audit_test_coverage.py` | Exit 0: 3,390 live-oracle tests, 0 unexplained, 3 accepted. |
| `check_test_counts.py` | Exit 0: every mapped module has Rust test count at least current Zig count. |

These count/inventory results are coverage constraints, not proof of behavioral
parity. The accepted PageList test gap was manually promoted as F-015.

## ABI and targeted behavior

`check_rustty_vt_headers.sh` exited 0 and reported `VT-HEADERS: GREEN`:

- every published header compiled standalone as C11/C++17;
- 185 declared symbols were real exports and all 185 oracle exports were present;
- 35 `sizeof` and 127 `offsetof` assertions passed;
- 574 enum discriminant assertions passed.

Targeted controls all exited 0:

- PageList preserved-state success: 1 passed;
- PageList alternative continuation failures: 2 passed;
- Kitty-filtered VT tests: 232 passed across the unit/integration binaries;
- renderer derived background-image/custom-shader mapping: 2 passed.

The positive controls narrow the reported defects; they do not exercise the
missing branches.

## Fresh finding reproducers

- Explicit renderer/xev predicates: 11/11 passed.
- GTK/FreeBSD predicates: 12/12 passed.
- Shared target-contract verifier: confirmed FreeBSD cfg, Linux-only renderer,
  Windows compile stop, and live Ghostty Windows paths.
- Windows source build: `rustty-os` exited 101 with 25 errors. The same-target
  `rustty-xev` control exited 0.
- Terminal allocator paired C probe:
  - Rustty: `result=0 terminal_nonnull=1 alloc_calls=0 free_calls=0`.
  - Ghostty: `result=-1 terminal_nonnull=0 alloc_calls=1 free_calls=0`.
- PageList failing-allocator probe exited 0 after observing the intended child
  failures: a preserved-page panic with compressed state retained, followed by
  encoded-buffer `SIGABRT` and `memory allocation of 4626 bytes failed`.
- Fuzz smoke exited 0 in its explicitly labeled progress path and reported that
  `crates/rustty-vt/fuzz` does not exist while `cargo fuzz` is available.

## Report integrity and repository boundaries

- The canonical table and `findings.json` contain the same 20 unique IDs and
  severity distribution: 5 P1, 9 P2, 3 P3, 3 P4.
- Every relative Markdown file link under `.port/codex/` resolves.
- Rustty has no tracked modification and no untracked file outside
  `.port/codex/`.
- Ghostty is clean.
- No audit worktree was created. The only secondary worktree is the pre-existing
  locked `miri-adversarial` worktree recorded in the baseline; it was untouched.
- 3.8 GB of generated Cargo/Zig caches were removed. Reports, raw logs, reduced
  repro source, and the inspected patched WASM artifact were retained.

