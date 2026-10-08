# Phase 8 delta-review brief (shared by R1–R4)

Range: Ghostty pin `12752b2ac` (2026-09-28, `.port/tools/gates/ORACLE_PIN`) → `origin/main`
`a60e9e2a5` (2026-10-07). 107 commits, 69 non-merge, 85 files, +7,466 / −497. Rustty at
`401b95b0e` (branch `phase-8-upstream-resync`, `/Users/gael/projects/rustty`).

Range records: `.port/phase-8/UPSTREAM_COMMITS.txt`, `UPSTREAM_COMMITS_WITH_MERGES.txt`,
`UPSTREAM_DIFF_STAT.txt`, `UPSTREAM_DIFF.patch`.

## Rules for reviewers

- **Read-only.** Change no Rustty code or record other than your own report file(s) in
  `.port/phase-8/delta-review/`. Build nothing unless your brief says so. Never run git commands
  that change state in either repository.
- **The oracle** is `/Users/gael/projects/ghostty`. Its working tree is detached at the pin
  `12752b2ac` and must stay there: never `checkout`, `switch`, `reset` or `stash` it. Read the new
  side with `git -C /Users/gael/projects/ghostty show <sha>`, `git … show a60e9e2a5:<path>`, and
  `git … diff 12752b2ac a60e9e2a5 -- <path>`.
- **The rule** (maintainer): Rustty matches Ghostty's latest upstream exactly, except it never
  copies a Ghostty bug. Port the range END STATE, not intermediates. A suspected upstream defect
  (reachable crash, UB, leak, deadlock, or a non-crash bug that breaks evident intent) is flagged
  as a UD candidate with the evidence; UD ids are assigned later (next free: UD378, N517, V34).
- **Map each upstream change to Rustty.** For every commit in scope, find the Rustty
  counterpart (`crates/`, `crates-sys/`, `macos/`, `include/`, `.port/`), say whether Rustty is
  ABSENT / PARTIAL / ALREADY-HAS / N/A, and cite Rustty `file:line` at `401b95b0e`. Rustty file
  names follow `foo.zig` → `foo.rs`; large modules may be split into `foo.rs` + `foo/<topic>.rs`.
  Names are renamed per token: `Ghostty`→`Rustty`, `ghostty`→`rustty`, `GHOSTTY`→`RUSTTY`.
  Before claiming something is absent, grep the whole workspace (it may live in another crate).
- **Check existing records** for overlap: `.port/parity-closure/UPSTREAM-DEFECTS.md` (UD),
  `.port/parity-closure/NEW-FINDINGS.md` (N), `.port/phase-2/VERIFICATION_DEBT.md` (V). If an
  upstream commit fixes something Rustty already recorded as a UD, say so (FIXED-SAME /
  FIXED-DIFFERENT / PARTIAL / OBSOLETED) and what Rustty must do.
- **Tests.** List every added, changed or removed upstream `test "…"` block in scope (file, name,
  sha). Ported tests keep upstream's names.
- **Hazards to call out:** Stacked-Borrows-sensitive aliasing (raw `Screen`/`Page`/`Terminal`
  aliases, callbacks holding `&mut`), C ABI changes (`include/rustty/vt/*.h` enums need a
  `_MAX_VALUE = RUSTTY_ENUM_MAX_VALUE` sentinel; option/data discriminants must follow upstream
  order), hot-path perf (Zig comptime tables → `const` tables, never `OnceLock`/`HashMap`), and
  platform-only code (Linux/rvm, Windows/rwin, macOS).
- **Report format** (follow `.port/phase-7/delta-review/R1-terminal-core.md` as the model):
  header (range, scope, method); §1 summary table (# · commit(s) · theme · observable? · Rustty
  status · size S/M/L · top risk); §2 per-theme sections with upstream `file:line` at
  `a60e9e2a5`, Rustty `file:line`, what to port, tests, risks, UD candidates; §3 proposed
  workstreams with ordering and effort; §4 open questions, each with your recommended answer
  under the rule above. Be concrete and complete; do not pad.
