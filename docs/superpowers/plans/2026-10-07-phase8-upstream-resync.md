# Phase 8 — Upstream Resync Implementation Plan (pin `12752b2ac` → `a60e9e2a5`)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Bring Rustty to parity with Ghostty `origin/main` `a60e9e2a5` (2026-10-07): 107 commits (69 non-merge), 85 files, +7,466/−497. The rule is parity, except Rustty never copies a Ghostty bug. At the finale every pin-sensitive gate is green at the new pin, all 90 new upstream tests are ported by name, all 30 changed upstream test bodies are re-aligned, and the full validation matrix from Phase 7 (Miri, sanitizers, Valgrind, benches, cross-host builds) is green again at the new pin.

**Architecture:** Advance-first, as in Phases 4–7.

- P8-W0 fixes the one live crash Rustty carries (wide character cut by a narrowing resize), repairs the parity-surface generator so the advance does not silently shrink G9, moves the oracle pin, and does the phase-start bookkeeping, so every red gate afterwards is attributed.
- Eight porting workstreams follow. Each runs in its own worktree off the phase branch `phase-8-upstream-resync` (already created at `401b95b0e`) and merges back with `--no-ff`.
- The critical paths are:
  - P8-W4 → P8-W5, the OSC parser then the stream effects and C API (about half the delta);
  - P8-W6, the app-scoped render `Device` (architectural, Linux lane).
- P8-W9 is the finale: gates, cross-host builds, then the long validation runs.

**Tech Stack:** Rust workspace (`crates/`, `crates-sys/`), C via `cc`/bindgen, Swift (`macos/`), gtk4-rs, `khronos-egl`, Zig 0.16.0 for the oracle, zigbench and harnesses, Python 3 for gates and esctest. Linux lane is `ssh rvm` (now 192.168.64.4; the IP can change after a UTM pause/resume — update `~/.ssh/config`). Windows lane is `ssh rwin` (192.168.64.5, PowerShell).

**Spec:**

- The four delta reviews in `.port/phase-8/delta-review/` (brief: `BRIEF.md`):
  - `R1-terminal-core.md`
  - `R2-osc-capi.md`
  - `R3-renderer-apprt-macos.md`
  - `R4-records-gates-build.md`, with `R4-commits.tsv` and `R4-upstream-tests.tsv`
- The rulings: `.port/phase-8/DECISIONS.md`. Where a review question is settled there, the DECISIONS row wins.

## Global Constraints

- **The per-commit spec is the reviews.** Each task cites review sections. The implementer reads the cited upstream hunks (`git -C /Users/gael/projects/ghostty show <sha>`, or the oracle working tree after P8-W0 T3) and the cited Rustty `file:line` (at `401b95b0e`; re-grep, lines drift). A task may not contradict a review row or a DECISIONS row without recording why in its commit message.
- **Port the range END STATE, never intermediates.** Within the range: `9272a2f7c`'s `xt_checksum: rect …` tests were renamed `selection …` by `5f0d4febc`; `934ef8140`'s `TerminalControllerRestorationTests.swift` was removed by `0e75d015f` (net none); `520d8f55a` revises `7b11f3dca`'s unknown-OSC end handling.
- **Parity except bugs** (DECISIONS rule; memory `decide-by-parity-minus-bugs`). Every confirmed upstream defect is fixed and recorded as a UD with a test that fails on upstream's behaviour. Reserved ids: N517–N522 (DECISIONS table). Next free: **UD378, N523, V34**. Concurrent workers use the provisional ranges in DECISIONS and the coordinator renumbers at merge.
- **R1 fidelity.** Every comment, every test assertion for assertion, every error path, every log line (G-LOGFILE). Ported tests keep upstream's names (`audit_test_coverage.py` matches by name).
- **R2 renames** per token (`Ghostty`→`Rustty`, `ghostty`→`rustty`, `GHOSTTY`→`RUSTTY`), casing preserved per token.
- **R5.** Gates are never weakened; every new gate arm ships a failing negative control.
- **R7.** Upstream invariants port as `assert!`. Upstream `+|`/checked arithmetic ports as `saturating_*`/`checked_*` with `debug_assert!`.
- **R13.** Never port a reachable crash, UB, leak, deadlock, or a non-crash bug that breaks evident intent. Fix it and log it in `.port/parity-closure/UPSTREAM-DEFECTS.md` plus `docs/ghostty-bug-fixes.md` (exactly one `- upstream status:` line per UD). Private-report crashes get no trigger recipe in the doc.
- **R14.** No silent skips. Target-specific tests use `#[cfg(...)]` with a PORT NOTE, or print a runtime skip. GPU tests go through the strict pivots (`RUSTTY_REQUIRE_GL`, `RUSTTY_REQUIRE_METAL`, `RUSTTY_REQUIRE_DMABUF`).
- **Stacked Borrows discipline** (memories `rust-mut-args-strongly-protected`, `shaper-hook-sb-clean-interleave`, `intrusive-ptr-and-placeholder-fill`):
  - raw-pointer arguments with narrow `&mut` reconstruction;
  - never hold `&mut Page/Row/Screen/Terminal/App` across a callback or a raw alias;
  - new C effect callbacks that can re-enter the terminal use the existing `LiveHandler` pattern;
  - restructured aliasing gets `LIFETIMES.tsv` rows first and an isolated non-FFI Miri harness under `.port/phase-2/mirihar-<name>/` showing UB before and a clean run after.
- **Perf** (memory `comptime-flattening-perf-hazard`). Zig comptime tables become `const` tables with `#[inline]`, never `OnceLock`/`HashMap`/`LazyLock`. Hot paths touched this phase: print (P8-W1 T2), OSC parser (P8-W4), CSI dispatch and stream effects (P8-W3, P8-W5). Their benches run at the finale against a zigbench built at `a60e9e2a5` with `-Doptimize=ReleaseFast`, measured as loop-sweep ns/op. LTO stays banned. Release profile keeps `panic = "abort"`.
- **C ABI.**
  - Every new C enum value is an append at upstream's number (R2 header table): terminal options 42–46, terminal data 42, OSC command 27/28, OSC data 2–4, unknown-sequence `OSC = 1`, snapshot decoder option 2 / data 9.
  - New C enums (`RusttyOscOption`, `RusttyOscTerminator`, `RusttyProgramStatusKind`, `RusttyProgramStatusState`, `RusttySemanticPromptKind`, `RusttySemanticPromptPromptKind`) end with `_MAX_VALUE = RUSTTY_ENUM_MAX_VALUE`.
  - New by-value structs (program status 88 B, semantic prompt 56 B, unknown OSC 32 B, memory usage 112 B on 64-bit) get `types.rs` manifest rows; the unknown-sequence union stays 128 B.
  - Headers are regenerated with `cargo run -p xtask -- header-gen`. `include/rustty-embed.h` is unchanged this phase (A3/A4 stay GREEN).
- **Oracle.** It stays read-only and never builds into the Rustty tree (R3). Its HEAD moves only in P8-W0 T3. rvm's oracle is fetched and detached to the same sha in T3 (N486).
- **Disk** (memories `parallel-worktree-disk-budget`, `vm-disk-images-eat-mac-disk`):
  - at most 3 building worktrees on the Mac; VM lanes one at a time;
  - check `df -h /Users/gael` before every build and stop below 12 GB;
  - after each merge: remove that worktree, its `worktree-agent` branch and its merged branch, and delete its `target/` (memory `cleanup-as-you-go`);
  - after each heavy VM lane: delete the VM clone and `fstrim`;
  - destructive cleanup names literal full paths, never shell variables or relative paths, and lists before deleting (memory `cleanup-literal-paths`). Never touch `/Users/gael/projects/rustty` itself.
- **Agent worktrees** may start at a stale base: every worker runs `git log -1` first and resets to the phase-branch head if needed. Merges into the phase branch and into `main` are done by the coordinator from `/Users/gael/projects/rustty`.
- **Long runs are deferred to P8-W9 T2** (DECISIONS D6). Inside tasks: targeted `cargo test -p`, clippy/fmt, gate scripts, Miri runs bounded at 30 min.
- **Commit style:** `<subsystem>: <summary> (<sha…>) (P8-W<n> T<m>)`. Every commit keeps `cargo build --workspace` and `cargo clippy --workspace --all-targets -- -D warnings` green, and `cargo fmt` clean. End each commit message with the session's attribution lines.
- **Never push, never create an issue or a PR** (AGENTS.md). Hosted CI legs stay push-gated under V25.
- **Privacy:** never take screenshots. Use headless goldens, broadway, `grim`/`wayshot` on a VM display only.

## Review Focus

These are the five inputs a user will hit that no ported upstream test exercises. Each has a test in its owning task.

1. **Narrowing a window over CJK text in a full-screen app.** In vim (alternate screen, no reflow), shrinking the window so a wide character straddles the new right edge, then inserting or erasing (`CSI @`, `CSI 1 K`), must not panic, must not leave half a glyph in the last column, and must not write past the row. Owner: P8-W0 T1, a stream-level test that drives the whole sequence through `Terminal` in debug and release.
2. **DECRQCRA from an untrusted program.** With default config, a program sending `CSI 1;1;1;1;24;80*y` or XTCHECKSUM must get no reply at all (screen read-back is a disclosure channel). With `vt-xt-checksum-report = true`, the reply must equal xterm's for a rectangle holding wide characters, a grapheme cluster and styled cells. Owner: P8-W3 T1/T2, a termio-level test through `StreamHandler` with the default `DerivedConfig` and with reporting on.
3. **A huge unknown OSC.** A 1 MiB OSC 1337 payload with unknown capture enabled at a small limit must truncate at the limit, keep memory bounded, set the truncated flag, and leave the next supported OSC (e.g. OSC 0 title) parsed correctly; with capture disabled it must cost nothing. Owner: P8-W4 T2, a parser test feeding 1 MiB in 4 KiB slices that asserts the capture length, the truncated flag, the following title, and that the parser's buffer never grew past the limit.
4. **Closing tabs while other tabs render.** With the shared render `Device`, closing one surface while another keeps drawing, then quitting the app, must not use the device after free and must not race. Owner: P8-W6 T2, a test that creates two surfaces on one `App`, destroys one, draws the other, then deinits the App; it runs under Miri (with the test constructor and a fake device) and under G-TSAN at the finale.
5. **Single shift before a wide or combined character.** `ESC N` (SS2) or `ESC O` (SS3) followed by a wide CJK character, an emoji with VS16, or a base plus combining mark must apply the shifted charset to exactly one printed codepoint and leave the next codepoint in GL. Owner: P8-W1 T2, terminal print tests for each of the three shapes.

---

## Execution order

```
P8-W0  T1 wide-char crash fix → T2 parity-surface generator fix → T3 ADVANCE → T4 phase-start records
   │
   ├── P8-W1  resize + print (T1 resize, T2 print)
   ├── P8-W2  small terminal/input/core fixes
   ├── P8-W3  DECRQCRA/XTCHECKSUM, all tiers (after W2 T1's fullReset reorder)
   ├── P8-W4  OSC parser: CAN/SUB, OSC 105, unknown capture, OSC 7501
   │      └──► P8-W5 T1 memory usage + snapshot compression core (independent; may start any time)
   │           P8-W5 T2 stream effects + termio arms (after W4)
   │           P8-W5 T3 vt-c + headers (after T1, T2 and W3 T3)
   │           P8-W5 T4 examples + Windows DLL proof + CI step (after T3; rwin)
   ├── P8-W6  WebGL removal → render Device + EGL diagnostics → thread-exit + UD20 test → GTK warn-once (Linux lane)
   ├── P8-W7  ssh cache port, macOS Swift, iTerm2 themes
   └── P8-W8  esctest package + CI job + G-ESCTEST (after W3 and W5 T3)
                         ▼
               P8-W9 FINALE (merge + gates → cross-host builds → long validation runs → records → main)
```

Parallelism: at most 3 building worktrees. Suggested first wave after W0: W1 + W4 + W6 (W6 drives the rvm lane early). Swap in W2, W3, W5, W7 as slots free. W7's Swift and themes half needs no cargo build and can run alongside.

---

# `P8-W0` — Pre-advance fix, generator fix, advance, phase-start records (coordinator)

### Task 1: Wide character cut by a non-reflow narrow (`e6db5b633`) — N518

**Spec:** R1 §2.1; DECISIONS D2, N518.

**Files:**

- Modify: `crates/rustty-vt/src/pagelist/resize.rs:708-712` (`resize_without_reflow`, the cols-less arm).
- Test: `crates/rustty-vt/src/terminal/tests/resize.rs` (next to `:429`).
- Records: `.port/parity-closure/NEW-FINDINGS.md` (N518, CLOSED by this commit).

- [ ] **Step 1: Write the failing tests.**
  - Port `Terminal: resize less cols without reflow cutting wide char tail` (upstream `Terminal.zig:15939`) verbatim, assertion for assertion.
  - Add the Review Focus 1 test `Terminal: resize less cols on alt screen then ICH and EL1 with a cut wide char` (Rustty-only, PORT NOTE naming N518): enter the alt screen (`CSI ? 1049 h`), print `"abcdefgh"` then a wide `"橋"` so its head sits at the column that becomes `cols - 1` after narrowing by one, resize to `cols - 1`, then feed `CSI @` and `CSI 1 K` through the stream. Assert: no panic; the last column is a blank narrow cell (not a wide head); no cell at index `>= cols` was written (read the row's raw cells up to `capacity.cols`).
- [ ] **Step 2: Run them to see them fail.** `cargo test -p rustty-vt resize_less_cols_without_reflow_cutting_wide_char_tail` and the alt-screen test → FAIL (debug: the `debug_assert!` in `Screen::clear_cells`, `screen/clear.rs:196-199`).
- [ ] **Step 3: Port the fix.** In the `.lt` arm, before clearing from `cols`, set `start = cols - 1` when the cell at `cols - 1` is `Wide::Wide`, else `cols`; read it through the raw row pointer (`cells.add(cols - 1)`), the same raw discipline as the surrounding block. Carry upstream's comment.
- [ ] **Step 4: Verify.**
  - `cargo test -p rustty-vt resize` → PASS; `cargo test -p rustty-vt --release resize_less_cols` → PASS.
  - `cargo +nightly miri test -p rustty-vt resize_less_cols` (≤ 30 min) → clean.
  - `cargo clippy -p rustty-vt --all-targets -- -D warnings`.
- [ ] **Step 5: Record and commit.** Add N518 (CLOSED, cites `e6db5b633`, the reachable recipe in one line, the fix commit). Commit: `vt: clear a wide head cut by a non-reflow narrow (e6db5b633) (P8-W0 T1)`.

### Task 2: Parity-surface generator reads `stream.zig` by function, not line window — N517

**Spec:** R4 §5.3; DECISIONS D3, N517.

**Files:**

- Modify: `.port/phase-0/inventory/parse_csi.py` (`START`/`END` at `:15-16`), `.port/phase-0/inventory/parse_esc.py` (`:13-14`).
- Create: a self-test arm in each script (`--self-test`) and a negative-control fixture under `.port/tools/gates/fixtures/parity-surface/` (a trimmed `stream.zig` with `'m'` removed from `csiDispatch`).
- Modify: `.port/tools/gates/check_parity_surface.py` only if it invokes the generator (wire `--self-test` into its self-test).

- [ ] **Step 1: Failing self-test.** The self-test runs the generator on the oracle's `src/terminal/stream.zig` at the current pin and asserts that the CSI set contains `e g h l m n p q r s t u @ c d` and that the ESC set contains no CSI final recorded from `csiDispatch`'s line range. Run it → FAIL (13 CSI finals missing, 5 ESC rows are CSI arms).
- [ ] **Step 2: Fix.** Locate `fn csiDispatch(` and `fn escDispatch(` by header and brace-match to the function end; drop the hard-coded line numbers. Keep the output format byte-identical for rows that were already correct.
- [ ] **Step 3: Verify.** `--self-test` → GREEN; the negative-control fixture (missing `'m'`) → the self-test FAILS on it (R5). Re-run G9 against the **frozen** surface (unchanged, still GREEN): `python3 .port/tools/gates/check_parity_surface.py`.
- [ ] **Step 4: Record and commit.** N517 (CLOSED by this commit; the regeneration happens in T3). Commit: `gates: parity-surface generator finds csiDispatch/escDispatch by function (N517) (P8-W0 T2)`.

### Task 3: Advance the oracle pin to `a60e9e2a5`

**Spec:** R4 §7 steps 1–7; `.port/ORACLE_SYNC.md` §"Advancing the pin"; Phase 7 plan P7-W0 T3.

**Files:**

- Modify:
  - `.port/tools/gates/ORACLE_PIN` → `a60e9e2a57f73e1eef2bd1cf2995a467f69e7fb0`
  - `.port/ORACLE_SYNC.md` (pinned block, a history row, "Upstream beyond this pin")
  - `.port/PORT_RULES.md` (R12)
  - `.port/cycle-analysis.md`
  - `.port/phase-3/CENSUS_DELTA.md` → 4,487 / 18,258 / 360, plus a history row
  - `.port/phase-0/inventory/parity-surface.json` (regenerated with the T2 generator)
  - `.port/phase-0/corpora/cvt/c-vt-effects.golden` (re-captured)
  - **Not** `.port/tools/oracle-ghostty-h.sha256` (`include/ghostty.h` is unchanged; A3 stays GREEN).
- Create: `.port/phase-8/w0/*-before-at-12752b2ac.txt`, `*-after-at-a60e9e2a5.txt`, `.port/phase-8/w0/README.md`.
- Already written: `.port/phase-8/UPSTREAM_DIFF.patch`, `UPSTREAM_DIFF_STAT.txt`, `UPSTREAM_COMMITS.txt` (69), `UPSTREAM_COMMITS_WITH_MERGES.txt` (107).

- [ ] **Step 1: Pre-flight.**
  - `git worktree list` shows only the main checkout.
  - `git -C /Users/gael/projects/ghostty status --porcelain` is empty; `rev-parse HEAD` = `12752b2ac1bb05ce53402ed8c853ed1f96eef0b1`.
  - `zig version` = `0.16.0`; `df -h /Users/gael` ≥ 12 GB free.
- [ ] **Step 2: Record "before"** (each to `.port/phase-8/w0/<gate>-before-at-12752b2ac.txt`):
  - G1b `python3 .port/tools/gates/check_test_counts.py`;
  - G3b `python3 .port/tools/gates/check_comment_coverage.py`, plus `--strict`;
  - G-TESTS / G-FILES / G-SYMS `python3 .port/tools/audit_test_coverage.py --limit 500` (and its file/symbol arms);
  - G-CENSUS `python3 .port/tools/gates/check_census_staleness.py`;
  - G-LOGFILE `python3 .port/tools/gates/check_log_reconcile.py`;
  - G-REFGEOM `python3 .port/tools/gates/check_ref_item_geometry.py`;
  - A3 `bash .port/tools/gates/check_rustty_h_abi.sh`;
  - A5 `bash .port/tools/gates/check_rustty_vt_headers.sh` — record its RED as pre-existing (DECISIONS D1);
  - G9 `python3 .port/tools/gates/check_parity_surface.py`.
- [ ] **Step 3: Advance.**
  - `git -C /Users/gael/projects/ghostty checkout --detach a60e9e2a57f73e1eef2bd1cf2995a467f69e7fb0`.
  - In the oracle tree: `zig build -Demit-lib-vt=true` (rebuild `zig-out/include`).
  - `bash .port/phase-0/harness/build_ref_driver.sh` (G4's staleness guard cannot see stale drivers).
  - `bash .port/phase-0/run-cvt-corpora.sh` to re-capture the c-vt goldens; only `c-vt-effects.golden` is expected to change (R4 §5 G4 row). Any other diff is a finding.
  - `bash .port/tools/gates/_oracle_root.sh --self-test` → GREEN.
  - rvm: `ssh rvm 'git -C ~/projects/ghostty fetch origin && git -C ~/projects/ghostty checkout --detach a60e9e2a57f73e1eef2bd1cf2995a467f69e7fb0'` (adjust the path to rvm's oracle; read it from `ssh rvm cat ~/.rustty-oracle` or the N486 record).
- [ ] **Step 4: Update the files listed above.** Regenerate `parity-surface.json` with the T2 generator.
- [ ] **Step 5: Record "after"** (same set, `*-after-at-a60e9e2a5.txt`). Expected arrival (R4 §5):
  - G-TESTS 73 unexplained, G-FILES 6, G-SYMS 8;
  - G1b RED, 2 rows (`input_mouse_encode`, `cli_ssh`);
  - G3b 411 missing (`--strict` 433) + 6 NOT-CARRIED-INVALID;
  - G-LOGFILE RED on 5 arms;
  - G-CENSUS GREEN with the new delta; A3 GREEN; G-REFGEOM GREEN;
  - G9 RED only on the new config keys, OSC kinds and the CSI finals the old generator hid (list them in `w0/README.md`; each is owned by a workstream below or mapped in T4).

  Any other red is a finding: add it to `w0/README.md`.

- [ ] **Step 6: Commit.** `oracle: advance pin 12752b2ac → a60e9e2a5 (107 commits, 69 non-merge) (P8-W0 T3)`.

### Task 4: Phase-start records and gate maps

**Spec:** R4 §§2, 5.2, 6 (R4-B); DECISIONS D4, M1–M7, P1, N519.

**Files:**

- Modify:
  - `.port/tools/gates/check_test_counts.py`: MAP row for `terminal/xt_checksum.zig`.
  - `.port/phase-0/inventory/tests-summary.json` (the G3b `crate_map`): rows with `test_blocks` 0 for `terminal/xt_checksum.zig`, `terminal/osc/parsers/{program_status,unknown}.zig`, `renderer/{metal,opengl}/Device.zig`, `lib/windows_dll.zig` (Phase 7 T4 precedent, so G1's frozen total does not move).
  - `.port/tools/gates/LOG-PORTMAP.tsv` (or its current path; grep `LOG-PORTMAP`): rows for `renderer/opengl/Device.zig`, `renderer/metal/Device.zig`, `terminal/osc/parsers/program_status.zig`.
  - `COMMENT-NOT-CARRIED.tsv` rows 40–45 (`lib_vt.zig`, +9 lines): re-anchor.
  - `.port/phase-3/NON_PORTS.md` (or the phase-4 `DllMain` row it extends): `src/lib/windows_dll.zig` as **provisional** NON-PORT pending P8-W5 T4's rwin proof (DECISIONS P1); the 15 VOUCHED shas; the `build.zig.zon.{json,nix,txt}` and `flatpak/zig-packages.json` renderings.
  - `.port/parity-closure/UPSTREAM-DEFECTS.md`: UD20 and UD312 statuses (M1, with their `docs/ghostty-bug-fixes.md` rows in the same commit); UD6/UD60/UD160 and the 47 Appendix A re-anchors (D4); UD113 gains the `c/osc.zig set` site (M3); UD356 note (M4).
  - `.port/parity-closure/NEW-FINDINGS.md`: N519 (record-only); N520–N522 opened by their owners, not here.
  - The LEDGER S133-F1 row: a note that `a573781c6` reverses the behaviour, ported in P8-W2 (M6). LEDGER `oracle` column re-anchors for the WAIVED rows R4 §2 lists.
  - `.port/phase-2/VERIFICATION_DEBT.md`: V25 gains `test-esctest` and the Windows DLL steps (M7).
  - `.port/parity-closure/README.md`: pin, id register ("Next free ids: UD378, N523, V34").

- [ ] **Step 1: Negative control for each map extension.** After adding the `xt_checksum` MAP row, G1b must report 12 upstream tests against Rust 0 and go RED on that row. Record the red rows in `w0/README.md`.
- [ ] **Step 2: Records.** Apply the edits above. Then:
  - `python3 .port/tools/gates/check_ud_docs.py` → PASS;
  - `python3 .port/tools/gates/check_parity_closure.py` → GREEN;
  - `python3 .port/tools/gates/check_phase2_closure.py` → GREEN.
- [ ] **Step 3: Re-run the audit.** `python3 .port/tools/audit_test_coverage.py --limit 500` → G-FILES no longer lists `windows_dll.zig`; G-SYMS no longer lists `windows_dll.DllMain`.
- [ ] **Step 4: Commit.** `records: Phase 8 start — gate maps, NON_PORTS, UD statuses and re-anchors, N519 (P8-W0 T4)`.

---

# `P8-W1` — Resize and print

### Task 1: Pending wrap and saved cursor across resize (`afded91df`, `f9ab34f10`)

**Spec:** R1 §2.2.

**Files:**

- Modify: `crates/rustty-vt/src/pagelist/resize.rs:1511-1540` (`ReflowCursor` tracked-pin loop; `dst_x` at `:1533`), `crates/rustty-vt/src/screen/clear.rs:723-756` (after `cursor_reload`).
- Test: `crates/rustty-vt/src/pagelist/tests/resize_noreflow_b.rs` (near `:1869`), `crates/rustty-vt/src/terminal/tests/resize.rs` (after `:1307`), `crates/rustty-vt/src/screen/tests/resize.rs` (12 changed tests).

- [ ] **Step 1: Failing tests.** Port the three added tests by upstream name: `PageList resize reflow pin in blank cells after line break` (3 cases), `Terminal: saved cursor survives repeated widening`, `Terminal: resize pending wrap live and saved cursors` (7 cases × restore on/off). Re-align the 12 changed `Screen: resize …` tests listed in R1 §2.2 exactly as upstream. Run `cargo test -p rustty-vt resize` → the new ones FAIL.
- [ ] **Step 2: Port `afded91df`.** `dst_x = if self.new_rows > 0 || self.pending_wrap { 0 } else { self.x }`; the blank-pin clamp uses `size.cols - 1 - dst_x`.
- [ ] **Step 3: Port `f9ab34f10`.** After `cursor_reload()`: if `cursor.pending_wrap && cursor.x != cols - 1` then clear `pending_wrap` and `cursor_right(1)`, mirroring the saved-cursor block; carry upstream's comment.
- [ ] **Step 4: Verify.** `cargo test -p rustty-vt resize` → PASS. `cargo +nightly miri test -p rustty-vt -- resize_pending_wrap saved_cursor_survives reflow_pin_in_blank` (≤ 30 min) → clean. Clippy.
- [ ] **Step 5: Commit.** `vt: repair pending wrap after resize; keep saved cursor across widening (afded91df, f9ab34f10) (P8-W1 T1)`.

### Task 2: Charsets above 0xFF and single shift once per codepoint (`6220a3617`, `c706451fe`)

**Spec:** R1 §2.3; Review Focus 5.

**Files:**

- Modify: `crates/rustty-vt/src/terminal/print.rs` (`print` `:984`, `print_inner` `:1003`, `print_cell` `:1589`, call sites `:1070`, `:1277`, `:1360`, `:1369`, `:1394`, `:1568`, `:1570`, `:1576`, `:1578`, `:1583`).
- Test: `crates/rustty-vt/src/terminal/tests/print.rs`.

**Interfaces:**

- Produces: `fn write_cell(&mut self, c: u21-as-u32, wide: Wide)` (`#[inline(always)]`), the old `print_cell` body without mapping; `print_cell(unmapped_c, wide, set)` maps through `set` and returns the codepoint unmapped above 0xFF.

- [ ] **Step 1: Failing tests.** Port `Terminal: print wide codepoint in a charset prints it unmapped`, `Terminal: print single shift is used up by a combining character`, `Terminal: VS16 moving a character to the next line keeps it unmapped`; re-align `Terminal: print charset outside of ASCII` (`"◆ "` → `"◆😀"`). Add the Review Focus 5 test `Terminal: single shift applies to exactly one codepoint` (Rustty-only): designate DEC special graphics into G2, then for each of (a) `ESC N` + `"橋"` + `"q"`, (b) `ESC N` + `"q"` + `U+FE0F` + `"q"`, (c) `ESC N` + `"q"` + `U+0301` + `"q"`, assert the shifted mapping applies only to the first codepoint and the final `"q"` prints from GL as `q`. Run → the ported tests FAIL.
- [ ] **Step 2: Port.** Resolve `key = single_shift.take().unwrap_or(gl)` and the charset once at the top of `print_inner` (after the status-display check); pass it into the narrow and wide mapping calls; add `write_cell`; switch the 9 non-mapping call sites to it; rename the `printCell` → `writeCell` comment above `print wide char at right edge with hyperlink`.
- [ ] **Step 3: Verify.** `cargo test -p rustty-vt print` → PASS. The batched fast path still refuses a pending single shift (`print.rs:318`): grep it. Clippy. Do not bench here; P8-W9 T2 benches `t-parser` and print.
- [ ] **Step 4: Commit.** `vt: charsets leave codepoints above 0xFF unmapped; single shift is used once per codepoint (6220a3617, c706451fe) (P8-W1 T2)`.

---

# `P8-W2` — Small terminal, input and core fixes

### Task 1: Clears, selection, RIS, tmux (`a573781c6`, `13b5ab204`, `bb20f8e45`, `2b1edded2`, `e8b858cc6`)

**Spec:** R1 §2.4 (first four bullets); DECISIONS M2, M6.

**Files:**

- Modify: `crates/rustty-vt/src/screen/clear.rs:286-319`, `crates/rustty-vt/src/page_impl.rs:2453-2470` (placeholder flag), `crates/rustty-vt/src/screen/select.rs:334-337`, `crates/rustty-vt/src/terminal/state.rs:502-593` (`full_reset`), `crates/rustty-vt/src/tmux/control.rs:176-179`.
- Test: `terminal/tests/attrs.rs` (after `:511`), `page_tests.rs`, `screen/tests/select.rs` (after `:283`), `tmux/control.rs` tests (after `:1487`), a RIS palette test in `terminal/tests/`.
- Records: UD6 citations already re-anchored in W0 T4; LEDGER S133-F1 note already added.

**Interfaces:**

- Produces: the reordered `full_reset` with a marked slot where P8-W3 T1 adds `flags.xt_checksum = self.default_xt_checksum`.

- [ ] **Step 1: Failing tests.** Port `Terminal: eraseLine complete clears kitty placeholder flag`, `Page clearCells full row clears kitty placeholder flag` (both behind the kitty-graphics feature, as upstream skips without it; R14 cfg + PORT NOTE), `Screen: selectLine semantic boundary around unwritten cells`, `tmux notification exactly max_bytes`, `tmux block terminator exactly max_bytes`, `tmux exceeding max_bytes`. Add a Rustty-only `Terminal: RIS resets the palette` (OSC 4 a color, `ESC c`, assert the default). Run → FAIL.
- [ ] **Step 2: Port.** Unconditional `row.kitty_virtual_placeholder = false` on full-row clear at both sites; `if end_pin.before(p) { return None; }` inside the forward whitespace trim; `colors.palette.reset_all()` after `set_cursor_style(Default)` plus the `fullReset` reorder with upstream's comments; tmux limit check moved after the state switch.
- [ ] **Step 3: Verify.** `cargo test -p rustty-vt -- placeholder select_line tmux full_reset ris` → PASS. Clippy.
- [ ] **Step 4: Commit.** `vt: placeholder flag, selectLine bound, RIS palette, tmux max_bytes (a573781c6, 13b5ab204, bb20f8e45, 2b1edded2, e8b858cc6) (P8-W2 T1)`.

### Task 2: Input encoders and header doc (`e55c687a4`, `157c44f64`, `9a4787d90`)

**Spec:** R1 §2.4 (last two bullets), R1 §1 row 22.

**Files:**

- Modify: `crates/rustty-vt/src/input/function_keys.rs:433-438`, `crates/rustty-vt/src/input/mouse_encode.rs:245-263`, `include/rustty/vt/terminal.h:2177` (via the header source + `cargo run -p xtask -- header-gen`).
- Test: `input/key_encode.rs` (after `:3389`), `input/mouse_encode.rs` (after `:892`).

- [ ] **Step 1: Failing tests.** Port `legacy: ctrl+alt+shift+backspace` and `utf8 encodes extended mouse buttons`. Run → FAIL.
- [ ] **Step 2: Port.** Insert `m(true, true, true, false)` → `"\x1b\x08"` after the alt+ctrl backspace entry, keeping table order. Encode `32 + button` as UTF-8 via `char::from_u32` **after** UD60's coordinate validation (no partial `ESC [ M`). Update the resize doc comment ("a same-size resize is not a no-op") at its source and regenerate headers.
- [ ] **Step 3: Verify.** `cargo test -p rustty-vt -- backspace extended_mouse` → PASS; `cargo test -p rustty-vt-c` header tests → PASS; `bash .port/tools/gates/check_rustty_vt_headers.sh` shows no new diff from this task. G1b: `input_mouse_encode` row GREEN again.
- [ ] **Step 4: Commit.** `input: ctrl+alt+shift+backspace legacy; UTF-8 1005 mouse button; resize doc (e55c687a4, 157c44f64, 9a4787d90) (P8-W2 T2)`.

### Task 3: Relative prompt click row (`5170b06f6`)

**Spec:** R3 §2.7.

**Files:**

- Modify: `crates/rustty-core/src/surface/mouse_input.rs:1892-1903`.
- Test: same file's test module (or `crates/rustty-core/tests/` where the surface mouse tests live).

**Interfaces:**

- Produces: `fn prompt_click_relative_row(pages: &PageList, click: Pin, prompt: Pin) -> u32` = `point_from_pin(Screen, click).y - point_from_pin(Screen, prompt).y + 1`.

- [ ] **Step 1: Failing test.** Port `promptClickRelativeRow` (four layouts, upstream `Surface.zig:6652-6691`) under its upstream name. Run → FAIL (no such function).
- [ ] **Step 2: Port** the helper over `PageList::point_from_pin` (`crates/rustty-vt/src/pagelist/pin.rs:134`) with `.expect` for the `.?` unwraps, and call it at the click site.
- [ ] **Step 3: Verify.** `cargo test -p rustty-core prompt_click_relative_row` → PASS; `cargo +nightly miri test -p rustty-core prompt_click_relative_row` → clean.
- [ ] **Step 4: Commit.** `core: relative prompt click row in screen space (5170b06f6) (P8-W2 T3)`.

---

# `P8-W3` — DECRQCRA / XTCHECKSUM, all tiers (one owner; DECISIONS P8)

### Task 1: vt core — `xt_checksum.rs`, flags, CSI `y` dispatch, `stream_terminal` reply

**Spec:** R1 §2.5; DECISIONS P7, P8, N520.

**Files:**

- Create: `crates/rustty-vt/src/xt_checksum.rs` (file-for-file with `xt_checksum.zig`; all 12 tests, end-state `selection …` names).
- Modify: `crates/rustty-vt/src/terminal/` (flags `xt_checksum`, `default_xt_checksum`, `set_default_xt_checksum`, `rect_xt_checksum`; `full_reset` slot from P8-W2 T1), `crates/rustty-vt/src/stream.rs` (+ its split modules; the two CSI `y` cases, `Handler::request_xt_checksum`, `Handler::xt_checksum_extension` with no-op defaults), `crates/rustty-vt/src/stream_terminal.rs` (reply encoding), `crates/rustty-vt/src/lib.rs` (re-exports per `main.zig`).
- Records: N520 (record-only).

**Interfaces:**

- Produces: `xt_checksum::Flags` = `#[repr(transparent)] struct Flags(u8)` with const accessors `positive`, `no_attributes`, `no_trim`, `undrawn`, `full` and `from_bits`/`bits` (u5 semantics); `xt_checksum::Request { id, top, left, bottom, right: u16 }` (`#[repr(C)]`); `Request::selection(&self, pages, origin) -> Option<Selection>`; `Terminal::set_default_xt_checksum(&mut self, Flags)`; `Terminal::rect_xt_checksum(&self, Request) -> u16`; terminal option field `default_xt_checksum: Flags`.

- [ ] **Step 1: Failing tests.** Port the 12 `xt_checksum.zig` tests, `stream: DECRQCRA dispatch`, `stream: XTCHECKSUM dispatch`, and the `stream_terminal` tests `DECRQCRA disabled`, `DECRQCRA enabled`, `XTCHECKSUM ignored without xt checksum report`. Run `cargo test -p rustty-vt xt_checksum decrqcra xtchecksum` → FAIL (module absent).
- [ ] **Step 2: Port** `xt_checksum.rs` (carry every comment and both xterm source links), the flags, the CSI arms, the stream_terminal reply and the `full_reset` line. No `HashMap`/`OnceLock`/`LazyLock` on the CSI path.
- [ ] **Step 3: Verify.** Tests → PASS. `cargo +nightly miri test -p rustty-vt xt_checksum` (≤ 30 min) → clean. G3b: the `xt_checksum.zig` comments now count (crate_map row from W0 T4). G1b `xt_checksum` row GREEN. Clippy.
- [ ] **Step 4: Commit.** `vt: DECRQCRA and XTCHECKSUM (9272a2f7c, 5f0d4febc, b2f557c6c, cc0276778, 01b9f4958) (P8-W3 T1)`.

### Task 2: Config keys and termio plumbing (`00a19db51`; termio halves of `9272a2f7c`, `01b9f4958`)

**Spec:** R3 §2.9; DECISIONS P8, P15; Review Focus 2.

**Files:**

- Modify: `crates/rustty-config/src/config_struct.rs:3148-3164` (after `vt_window_resize_allowed`), `:4325`, `config_struct/key.rs:438`, `c_get.rs:448`, `config_struct/defaults.rs`, `finalize_clone.rs`, `format.rs`, a new packed type `XtChecksumExtension` beside `value_types/packed.rs:45-80` (`negate`, `attributes`, `trim` default true; `undrawn`, `full` default false; `no-` parsing); `crates/rustty-termio/src/termio_impl.rs:300-380,474` (`xt_checksum_flags`, `DerivedConfig` fields), `stream_handler.rs:150-160,265-280`, `stream_handler/handler_impl.rs`, `stream_handler/osc_handlers.rs` (`report_xt_checksum` beside `send_size_report`).
- Test: config parse/format tests; termio stream-handler tests.

- [ ] **Step 1: Failing tests.**
  - Config: `vt-xt-checksum-report` defaults false; `vt-xt-checksum-extension` defaults `negate,attributes,trim,no-undrawn,no-full`; `no-negate` parses; format round-trips; key order matches upstream (G9 config list).
  - Review Focus 2 (Rustty-only, termio): with the default `DerivedConfig`, feed `CSI 1;1;1;1;24;80*y` and `CSI > 0 ; 1 # y`-style XTCHECKSUM through `StreamHandler` → assert **nothing** is written to the pty. With `xt_checksum_report = true`, fill the screen with `"a橋"`, a ZWJ emoji cluster and a bold red cell, request the rectangle → assert the reply equals the value computed by `rect_xt_checksum` for xterm's default flags and the exact `DCS <id> ! ~ XXXX ST` framing.
  - Run → FAIL.
- [ ] **Step 2: Port** the config fields with their doc comments (token-renamed, xterm links kept), `xt_checksum_flags` (inverts `negate`/`attributes`/`trim`), `DerivedConfig`, `change_config` (calls `set_default_xt_checksum`), the two actions (`xt_checksum_extension` only when reporting is enabled), `report_xt_checksum` (early return when disabled, `write_small`). Port the new log sites (G-LOGFILE).
- [ ] **Step 3: Verify.** `cargo test -p rustty-config xt_checksum`, `cargo test -p rustty-termio xt_checksum` → PASS. `python3 .port/tools/gates/check_parity_surface.py` → the two config keys close.
- [ ] **Step 4: Commit.** `config,termio: vt-xt-checksum-report and -extension (00a19db51, 9272a2f7c, 01b9f4958) (P8-W3 T2)`.

### Task 3: C options and exports (`7e2d1793d`, `25b04b410`)

**Spec:** R2 §2.8; DECISIONS P8.

**Files:**

- Modify: `crates/rustty-vt-c/src/terminal*.rs` (options `XT_CHECKSUM_REPORT`/`XT_CHECKSUM_EXTENSION` at upstream numbers 44/45 — confirm against `include/ghostty/vt/terminal.h` at `a60e9e2a5`), the header source, `include/rustty/vt/terminal.h` (regenerated), the `lib_vt` exports of `xt_checksum` and `device_attributes`.
- Test: `checksum report requires explicit opt in`, `checksum extension survives resets` (vt-c).

**Interfaces:**

- Produces: `RUSTTY_TERMINAL_OPT_XT_CHECKSUM_*` at upstream's numbers; P8-W5 T3 appends options 42, 43, 46 around them without renumbering.

- [ ] **Step 1: Failing tests.** Port both vt-c tests → FAIL.
- [ ] **Step 2: Port** the option setters (`c_int` narrowing per UD113) and the exports; regenerate headers (`cargo run -p xtask -- header-gen`).
- [ ] **Step 3: Verify.** `cargo test -p rustty-vt-c checksum` → PASS; `cargo +nightly miri test -p rustty-vt-c checksum` → clean; `bash .port/tools/gates/check_abi_schema.sh` and the enum-discriminant check (`node .port/tools/check-vt-enum-discriminants.mjs`) → GREEN for the new values.
- [ ] **Step 4: Commit.** `vt-c: XT checksum options and exports (7e2d1793d, 25b04b410) (P8-W3 T3)`.

---

# `P8-W4` — OSC parser

### Task 1: CAN/SUB cancel and OSC 105 (`520d8f55a`, `36953bca8`)

**Spec:** R2 §2.2; R1 §1 rows 19–20.

**Files:**

- Modify: `crates/rustty-vt/src/osc/mod.rs` (`Parser::end`), `crates/rustty-vt/src/osc/parsers/color.rs` (route 105).
- Test: `osc/mod.rs` tests, `parser.rs` tests, `stream_terminal` tests.

- [ ] **Step 1: Failing tests.** Port `Parser end with CAN or SUB cancels the command`, `osc: 105 empty`, `window_title not changed by cancelled OSC`. Run → FAIL.
- [ ] **Step 2: Port** the one-line CAN/SUB arm in `end` and the OSC 105 routing.
- [ ] **Step 3: Verify.** `cargo test -p rustty-vt -- osc cancel 105` → PASS.
- [ ] **Step 4: Commit.** `vt: CAN and SUB cancel an OSC; route OSC 105 to the color parser (520d8f55a, 36953bca8) (P8-W4 T1)`.

### Task 2: Unknown OSC capture (`7b11f3dca` parser half, end state after `520d8f55a`)

**Spec:** R2 §2.1; DECISIONS P4; Review Focus 3.

**Files:**

- Create: `crates/rustty-vt/src/osc/parsers/unknown.rs` (13 tests).
- Modify: `crates/rustty-vt/src/osc/mod.rs` (`Command::Unknown`, `unknown_max_bytes`, `unknown_or_invalid`, `capture_failed`, the bridge states at `end`; fast path at `:1284` compares `len < cap.max_bytes`), a `const` prefix table for consumed-state names (never `State::tag_name()`), `crates/rustty-vt/src/bench/osc_parser.rs` (`--unknown-max-bytes`).

**Interfaces:**

- Produces: `osc::Command::Unknown { content: &[u8], truncated: bool, terminator: Terminator }` (borrowing the parser buffer), `Parser::set_unknown_max_bytes(usize)` (0 = disabled, the default).

- [ ] **Step 1: Failing tests.** Port the 13 `OSC unknown: …` tests. Add Review Focus 3 (Rustty-only): enable capture at 4 KiB, feed `ESC ] 1337 ; File=` + 1 MiB of `A` + `BEL` in 4 KiB slices via `next_slice`, then `ESC ] 0 ; title BEL`. Assert: `content.len() == 4096 - prefix`, `truncated`, the parser buffer capacity never exceeds the limit (instrument via the allocator test hook used by `fixed tier does not allocate`), and the following OSC 0 yields `title`. With capture disabled the same input allocates nothing. Run → FAIL.
- [ ] **Step 2: Port** the end state. Supported fixed captures keep `max_bytes == MAX_BUF`, so their fast path is unchanged.
- [ ] **Step 3: Verify.** `cargo test -p rustty-vt osc_unknown` → PASS. `cargo +nightly miri test -p rustty-vt osc_unknown` → clean. Quick sanity (not a fence): `cargo run -p rustty-bench --release -- osc-parser` runs with and without `--unknown-max-bytes`.
- [ ] **Step 4: Commit.** `vt: capture unknown OSC sequences (7b11f3dca, 520d8f55a) (P8-W4 T2)`.

### Task 3: OSC 7501 program status parser (`bae2c3cdb` parser half)

**Spec:** R2 §2.3.

**Files:**

- Create: `crates/rustty-vt/src/osc/parsers/program_status.rs` (15 tests), plus a shared base64 helper only if no existing one fits (grep `base64` in `rustty-vt` first).
- Modify: `crates/rustty-vt/src/osc/parsers.rs` (or `mod.rs`), `osc/mod.rs` (`Command::ProgramStatus`).

- [ ] **Step 1: Failing tests.** Port the 15 `OSC 7501: …` tests (including `text must decode to safe UTF-8` and `no allocator`). Run → FAIL.
- [ ] **Step 2: Port** the parser and its two log sites (declared in LOG-PORTMAP by W0 T4).
- [ ] **Step 3: Verify.** `cargo test -p rustty-vt osc_7501` → PASS; Miri on the module → clean; `cargo fuzz` is not required, but run the existing OSC proptest if present.
- [ ] **Step 4: Commit.** `vt: OSC 7501 program status parser (bae2c3cdb) (P8-W4 T3)`.

---

# `P8-W5` — Memory usage, snapshot compression, stream effects, C API, examples

### Task 1: Memory usage and snapshot history compression, core halves (`8834e161c`, `88e66cbc6`)

**Spec:** R1 §1 rows 13–14 and its §2 sections for them; R2 §2.7; DECISIONS P5.

**Files:**

- Modify: `crates/rustty-vt/src/pagelist/` (`memory_usage`, `PageAllocation::finalize(.prepend, opts)` signature at its 6 call sites), `crates/rustty-vt/src/screen/` (`memory_usage`), `crates/rustty-datastruct/src/{untouched_pool,wasm_page_pool}.rs` (`free_count`; wasm returns 0), `crates/rustty-vt/src/snapshot/{history,main,snapshot}.rs` (`DecodeOptions.compress_history`), `search/{pagelist,screen}.rs` (`finalize` signature).
- Test: `PageList memory usage`, `PageList PageAllocation compresses prepended pages unless visible`, `HISTORY decode compresses history pages when requested`, changed `UntouchedPool: create, destroy, reuse`, `PageList PageAllocation finalizes pages and preserves live state`, the 3 changed `snapshot/history` tests, the 2 `search` signature changes.

- [ ] **Step 1: Failing tests** (ports above) → FAIL.
- [ ] **Step 2: Port.** The memory walk is read-only over the existing node walk; compression during restore goes through the existing raw page pointer.
- [ ] **Step 3: Verify.** `cargo test -p rustty-vt -- memory_usage compresses finalize history` and `cargo test -p rustty-datastruct` → PASS. Miri: the `vt-pagelist` group and the search lane (`bash .port/tools/gates/check_leaks.sh --crate rustty-vt:=search` or the group selector the script documents) ≤ 30 min → clean.
- [ ] **Step 4: Commit.** `vt: memory usage query and snapshot history compression (8834e161c, 88e66cbc6) (P8-W5 T1)`.

### Task 2: Stream effects and termio arms

**Spec:** R2 §2.4, §2.9; R1 §1 rows 15–18; R3 §2.10; DECISIONS P6, P7, P9.

**Files:**

- Modify: `crates/rustty-vt/src/stream.rs` (+ split modules: `Options.osc_unknown_max_bytes`; actions `osc_unknown`, `mouse_shape_reset`, `program_status`; `UnknownSequence::Osc`), `crates/rustty-vt/src/stream_terminal.rs` (`SemanticPrompt` event, effects `semantic_prompt`, `reset`, `program_status`, `osc_unknown`; RIS clears program-status records; empty OSC 22 → `mouse_shape_reset`; the semantic-prompt effect fires only when the terminal accepted the sequence — fix the `vt()` helper that swallows the error), `crates/rustty-vt/src/lib.rs` (crate-root `SemanticPrompt` = event), `crates/rustty-termio/src/stream_handler/{handler_impl,osc_handlers}.rs` (`mouse_shape_reset` → default/text; `osc_unknown` and `program_status` join the ignored list), `crates/rustty-vt/src/bench/terminal_stream.rs` (`--osc-unknown-max-bytes`).
- Test: `unknown OSC effect callback`, `program_status effect callback`, `semantic_prompt effect callback`, `reset effect callback`, changed `unknown APC effect callback`; termio `mouse_shape_reset` test.

- [ ] **Step 1: Failing tests** (ports above, plus a Rustty-only termio test that an empty OSC 22 restores `default` with mouse reporting on and `text` with it off) → FAIL.
- [ ] **Step 2: Port.** Effects hand out text borrowed from the OSC parser; callbacks reach the terminal only through the raw-pointer path (no `&mut Terminal` held across the callback).
- [ ] **Step 3: Verify.** `cargo test -p rustty-vt effect_callback`, `cargo test -p rustty-termio mouse_shape` → PASS. Miri on the effect tests → clean. G9: OSC kinds `program_status`, `unknown` close.
- [ ] **Step 4: Commit.** `vt,termio: unknown-OSC, program-status, semantic-prompt and reset effects; empty OSC 22 resets the shape (7b11f3dca, bae2c3cdb, 7bb45ba34, b1bfd1d9c) (P8-W5 T2)`.

### Task 3: vt-c and headers

**Spec:** R2 §2.5, §2.6, §2.7 (C halves), §2.9; DECISIONS M1 (UD312), M3, M5, N521.

**Files:**

- Modify: `crates/rustty-vt-c/src/` (osc, terminal, paste, snapshot, sys, types), the header sources and regenerated `include/rustty/vt/{osc,terminal,snapshot,sys,allocator}.h`.
- Records: N521 (open + close here).

**Interfaces:**

- Produces (append-only, upstream numbers): `rustty_osc_set`, `RusttyOscOption`, `RusttyOscTerminator`, `RUSTTY_OSC_COMMAND_UNKNOWN = 27`, `…_PROGRAM_STATUS = 28`, `RUSTTY_OSC_DATA_UNKNOWN_{CONTENT,TRUNCATED,TERMINATOR} = 2..4`, `RUSTTY_TERMINAL_UNKNOWN_SEQUENCE_OSC = 1`, terminal options 42, 43, 46, `RUSTTY_TERMINAL_DATA_MEMORY_USAGE = 42`, snapshot decoder option 2 / data 9, structs `RusttyTerminalUnknownOscSequence`, `RusttyTerminalProgramStatus`, `RusttyTerminalSemanticPrompt`, `RusttyTerminalMemoryUsage`.

- [ ] **Step 1: Failing tests.** Port: `command data null` (rename Rustty's UD312 test to `command_data_null`), `unknown` (c/osc); `get memory_usage`, `get memory_usage size rule`, `set program_status callback`, `set semantic_prompt callback`, `set reset callback`; changed `set unknown_sequence callback`, `get mouse_shape`, `manifest uses public enum names`; `terminal_paste refused write ignored by the reader`; `decode_png output is zeroed before the callback`; `snapshot decoder compresses history when requested`; changed `decoder option and empty source`. Add Rustty-only Miri re-entry tests: each of the four new callbacks calls back into the terminal (`rustty_terminal_get`, a `vt_write`) while it holds the borrowed text. Run → FAIL.
- [ ] **Step 2: Port.** `rustty_osc_set` clears the stored parser command first (SB); `UNKNOWN_MAX_BYTES` sets both limits through a raw place; the four trampolines use `LiveHandler`; the paste fix (N521); `_MAX_VALUE` sentinels on every new enum; `types.rs` manifest rows for the four structs; allocator alignment doc (`82ae67b9f`); regenerate headers.
- [ ] **Step 3: Verify.**
  - `cargo test -p rustty-vt-c` → PASS; `cargo +nightly miri test -p rustty-vt-c -- callback osc_set paste memory_usage` (≤ 30 min) → clean.
  - `bash .port/tools/gates/check_rustty_vt_headers.sh` (A5) → GREEN (it reads `origin/main` = `a60e9e2a5`).
  - `bash .port/tools/gates/check_abi_schema.sh`, `node .port/tools/check-vt-enum-discriminants.mjs`, `node .port/tools/check-vt-struct-layouts.mjs` → GREEN.
  - G-CENSUS unaffected; G-SYMS: `freeCount`, `memoryUsage` closed.
- [ ] **Step 4: Commit.** `vt-c: unknown OSC, program status, semantic prompt, reset, memory usage, snapshot compression, paste fix (…) (P8-W5 T3)` (list the shas from R2 §1 rows covered).

### Task 4: Examples, Windows DLL proof, CI step (`a141f9bdb`, `a7cf951eb`)

**Spec:** R2 §2.10, R2 W4; R4 §4.3; DECISIONS P1.

**Files:**

- Modify: `example/c-vt-effects/src/main.c` (unknown-OSC arm + multi-byte write), `example/c-vt-compression/{src/main.c,README.md}` (memory usage), `example/c-vt-snapshot/{src/main.c,README.md}` (compress), `.github/workflows/ci.yml` (`windows` job: build `rustty-vt-c` as a DLL and run `c-vt-effects` against `rustty_vt_c.dll`).
- Records: finalize the `windows_dll.zig` NON_PORTS row (or replace it with a port).

- [ ] **Step 1: G4 leg B.** `bash .port/tools/gates/check_examples.sh` and `bash .port/tools/cvt-parity.sh` → RED on `c-vt-effects` before the example edit (golden re-captured in W0 T3), GREEN after.
- [ ] **Step 2: rwin proof.** On rwin, fresh clone of this branch: `cargo build -p rustty-vt-c --release --target x86_64-pc-windows-msvc`, build `c-vt-effects` against the DLL, run it; also `-gnu`. Record output in `.port/phase-8/w5/rwin-dll-proof.txt`. If either target crashes on the multi-byte write, stop and port the constructor walk (`src/lib/windows_dll.zig`) instead of the NON_PORTS row.
- [ ] **Step 3: CI step.** Port `a7cf951eb`'s steps into the `windows` job (push-gated; V25 already extended in W0 T4).
- [ ] **Step 4: Cleanup.** Delete the rwin clone by literal path.
- [ ] **Step 5: Commit.** `examples,ci: memory usage, compression, unknown-OSC effects; Windows DLL check (a141f9bdb NON-PORT proven, a7cf951eb) (P8-W5 T4)`.

---

# `P8-W6` — Render device and renderer lifecycle (Linux lane early)

### Task 1: Remove the WebGL stub (`05392dcc9`)

**Spec:** R3 §2.1; DECISIONS P2.

**Files:**

- Delete: `crates/rustty-renderer/src/webgl.rs`.
- Modify: `crates/rustty-renderer/src/lib.rs:3,62-66,165-169,229,259-264`, `crates/rustty-build-config/src/renderer.rs:15,20-24` (keep the `wasm_target` parameter, discard it), `crates/rustty-apprt/src/lib.rs:241,728`, `crates/rustty-cli/src/version.rs:59-66`, `crates/rustty-crash/src/sentry.rs:795`, `.port/tools/gates/check_leaks.sh:1016` (`rustty-renderer:webgl`), any record listing the file.

- [ ] **Step 1: Negative control.** `grep -rniw 'webgl' crates/ --include=*.rs` lists the 10 sites (excluding the wasm module and its consumer gate, which stay).
- [ ] **Step 2: Remove** each site; port `backend.zig`'s end state literally.
- [ ] **Step 3: Verify.** `cargo build --workspace`; `cargo build -p rustty-wasm --target wasm32-unknown-unknown` (or the wasm build script `bash .port/tools/build-wasm.sh`); `bash .port/tools/gates/check_leaks.sh --self-test` (the scope list still validates); the grep from Step 1 is empty outside the kept wasm paths.
- [ ] **Step 4: Commit.** `renderer: remove the stub WebGL renderer (05392dcc9) (P8-W6 T1)`.

### Task 2: App-scoped render `Device` (`40d5b860d`) and EGL diagnostics (`2e36658c6`)

**Spec:** R3 §2.2, §2.3; DECISIONS P10, P11, P12, M4, N522; Review Focus 4.

**Files:**

- Create: `crates/rustty-renderer/src/metal/device.rs`, `crates/rustty-renderer/src/opengl/device.rs`; `.port/phase-2/mirihar-device-box/` (isolated Miri harness); `LIFETIMES.tsv` rows for `App.device` and `Options.device`.
- Modify: `crates/rustty-renderer/src/metal/metal_impl.rs:196-355,481,688-902,966`; `crates/rustty-renderer/src/opengl/opengl_impl.rs:186-201`; `crates/rustty-renderer/src/opengl/gl/sys.rs:156-330` (split `OffscreenContext`: app-scoped half into `opengl::Device`), `:188-200` (drop the pre-check; `log_unsupported_platform` with the three verbatim messages); `crates/rustty-renderer/src/generic.rs:1025-1050` (`Options.device: *const Device` first); `crates/rustty-renderer/src/lib.rs` (`pub use … Device` per target); `crates/rustty-core/src/renderer_seam.rs:376-430`; `crates/rustty-core/src/surface/lifecycle.rs:275-288`; `crates/rustty-core/src/app.rs:150-327` (`DeviceBox`, `CreateError { GridSet(InitError), DeviceFailed }`, warmup on `Device`, `#[cfg(test)]` null-device constructor); `crates/rustty-apprt-c/src/lib.rs:563` (`AppNewError`); GTK and `main_rustty.rs` error paths; `crates/rustty-apprt/tests/…/gtk_render_surface.rs` (UD356 trigger moves to a per-surface context failure).
- Records: N522 (PORT-ONLY), UD356 note.

**Interfaces:**

- Consumes: nothing from other workstreams.
- Produces: `rustty_renderer::Device` (Metal on macOS, OpenGL elsewhere) with `init(&mut self) -> Result<(), DeviceError>`, `deinit(&mut self)`, Metal-only `warmup()`; `generic::Options { device: *const Device, … }`; `rustty_core::app::CreateError::DeviceFailed`.

- [ ] **Step 1: Miri harness first.** In `.port/phase-2/mirihar-device-box/`, model `App { device: Device }` by value with a render thread reading `&app.device` while main takes `&mut App` → Miri reports UB; then `DeviceBox(*mut Device)` → clean. Commit the harness with its two outputs.
- [ ] **Step 2: Failing tests.**
  - Rustty-only `App::init` creates and destroys a real device (macOS; rvm under llvmpipe; R14 runtime skip elsewhere via `RUSTTY_REQUIRE_METAL`/`RUSTTY_REQUIRE_GL`).
  - Review Focus 4: two surfaces on one App (test constructor + a fake device under Miri; real device on Mac/rvm), destroy one, draw a frame on the other, deinit the App; assert no error and, on the real-device run, that the device outlives both renderers (drop-order probe).
  - EGL diagnostics: a split `fn classify(exts: Option<&str>) -> UnsupportedPlatform` unit-tested for all three arms; on rvm, `__EGL_VENDOR_LIBRARY_FILENAMES=/nonexistent.json` makes `App::init` fail with `DeviceFailed` and logs "no EGL driver could be loaded; …".
  - Run → FAIL.
- [ ] **Step 3: Port** the two `Device` files file-for-file (moved doc comments travel; `opengl/Device.zig:69` token-renamed), the renderer changes, `Options`, core seam, `App` (create the device at the reference's point, log `failed to initialize render device err={}`, free it in `deinit` after the surface loop and the grid-set assert). Write the thread-safety argument at `DeviceBox` (MTLDevice thread-safe; EGL display calls thread-safe; fields written once before any surface). No `eglTerminate`.
- [ ] **Step 4: Verify (Mac).** `cargo test -p rustty-renderer -p rustty-core -p rustty-apprt-c` → PASS; `cargo +nightly miri test -p rustty-core app::` (the 17 App tests on the null-device constructor) → clean; `xcodebuild` the app builds.
- [ ] **Step 5: Verify (rvm, early).** Fresh clone of the worktree branch on rvm: `cargo clippy --workspace --all-targets --features rustty/gtk,rustty/x11,rustty/wayland -- -D warnings`; `cargo test -p rustty-renderer -p rustty-core -p rustty-apprt`; the live GTK harness (`bash .port/tools/gtk_live_headless_sway.sh`) including the moved UD356 test and G-LSAN-GTK (`bash .port/tools/gates/sanitizers.sh --gate lsan-gtk`). Delete the clone and fstrim.
- [ ] **Step 6: Verify (rwin build).** `cargo build --workspace --all-targets --target x86_64-pc-windows-msvc` (CI exclusions); list any Windows test that now fails at `App::init` and give it an R14 runtime skip with a PORT NOTE (upstream has the same shape). Delete the clone.
- [ ] **Step 7: Commit** (two commits): `renderer,core: app-scoped render Device shared by every renderer (40d5b860d) (P8-W6 T2)` and `opengl: say why the surfaceless display can't be created (2e36658c6) (P8-W6 T2)`.

### Task 3: Thread exit unrealizes the display (`140feb86a`); UD20 upstream test (`80456f057`)

**Spec:** R3 §2.4, §2.5; DECISIONS M1.

**Files:**

- Modify: `crates/rustty-renderer/src/generic/macos_impl/lifecycle.rs:547-566`, `generic/linux_impl/lifecycle.rs:292-313` (`thread_exit_locked` in both twins), the draw early-return comments (`linux_impl/draw.rs:538` and the macOS twin); `crates/rustty-renderer/src/shadertoy.rs:158-202` (PORT NOTE → plain port comment), its tests.

- [ ] **Step 1: Failing tests.** Rustty-only: after `thread_exit`, `display_realized` is false and the shaders are defunct (Linux `defunct` flag; add a Metal probe), and a following `draw_frame` returns early. Port `custom shader selection uniform layout matches compiled GLSL` (uses `spirv_block_member_offsets`). Run → the thread-exit test FAILS; the UD20 test passes already (record that in the commit message).
- [ ] **Step 2: Port** `display_realized = false` before `release_gpu_resources()` in both twins and the comment edits.
- [ ] **Step 3: Verify.** `cargo test -p rustty-renderer -- thread_exit selection_uniform` → PASS on Mac and rvm.
- [ ] **Step 4: Commit.** `renderer: render-thread exit unrealizes the display; upstream UD20 test (140feb86a, 80456f057) (P8-W6 T3)`.

### Task 4: GTK DPI warnings once (`0d82239b5`) — rvm

**Spec:** R3 §2.6; DECISIONS P14.

**Files:**

- Modify: `crates/rustty-apprt/src/gtk/class/surface/core_callbacks.rs:475-503`; a `log_warn_once!` macro next to it (per-site `static ONCE: AtomicBool`, `Relaxed`), with the helper's doc comments.

- [ ] **Step 1: Failing test.** A unit test calling the DPI path twice with an invalid value asserts one captured warning (use the crate's log-capture helper). → FAIL.
- [ ] **Step 2: Port.** Re-anchor `LOG-*` rows for the three sites and the N20 comment at `:475` (G-LOGFILE arm 5).
- [ ] **Step 3: Verify (rvm).** `cargo test -p rustty-apprt --features gtk dpi`; G-LOGFILE → no new red from these sites.
- [ ] **Step 4: Commit.** `gtk: warn once about missing or invalid DPI (0d82239b5) (P8-W6 T4)`.

---

# `P8-W7` — CLI, macOS Swift, themes

### Task 1: ssh-terminfo cache key includes the port (`6fbb560cd`, `dfd7bafcd`)

**Spec:** R3 §2.8; DECISIONS P13.

**Files:**

- Modify: `crates/rustty-cli/src/ssh.rs:788-` (`parse_destination`), `crates/rustty-cli/src/ssh_cache/disk_cache.rs:531-,672` (`is_valid_cache_key`, new `HostPort`, `split_host_port`, `is_valid_host_port`, `is_valid_port`), `crates/rustty-cli/src/ssh_cache.rs:37-60,363,393,490,593` (doc + usage line, three error strings, `matches_query`).
- Test: `split_host_port` (new), extended `is_valid_cache_key` (+15 asserts) and `matches_query` (+18), `parseDestination: non-default port`, `… port before hostname`, `… IPv6 hostname with non-default port`, `… result is a valid cache key`.

- [ ] **Step 1: Failing tests** (ports above; byte-slice APIs, `&[u8]`) → FAIL.
- [ ] **Step 2: Port** the end state of both commits; keep `host` vs `host:22` distinct for add/remove/contains and equal in queries.
- [ ] **Step 3: Verify.** `cargo test -p rustty-cli ssh` → PASS; `cargo run -p rustty -- +ssh-cache --help` shows the new usage line. G1b `cli_ssh` row GREEN.
- [ ] **Step 4: Commit.** `cli: ssh-terminfo cache key carries the port (6fbb560cd, dfd7bafcd) (P8-W7 T1)`.

### Task 2: macOS Swift (`136029788`, `934ef8140`, `0e75d015f`, `e2721d093`, `be595d8f0`)

**Spec:** R3 §2.11.

**Files:**

- Modify: `macos/Sources/Features/Terminal/TerminalController.swift` (`syncRestoration`), `macos/Sources/App/AppDelegate.swift`, `macos/Sources/Features/ClipboardConfirmation/{ClipboardConfirmationController,ClipboardConfirmationView}.swift`, `macos/Sources/Rustty/Rustty.ClipboardConfirmationRequest.swift`, `macos/Sources/Rustty/Rustty.App.swift`, `macos/Sources/Features/Terminal/Window Styles/{TitlebarTabsTahoeTerminalWindow,TransparentTitlebarTerminalWindow}.swift` (names per R2 rename; grep).

- [ ] **Step 1: Port** the end state of each commit (no `TerminalControllerRestorationTests.swift`; it was removed in-range).
- [ ] **Step 2: Verify.** `swiftlint lint --strict --fix`; `xcodebuild` the app and RusttyTests on this Mac (Xcode 26.6 / Swift 6.3.3 must still build without the `#if compiler(>=6.4)` guards — if it does not, keep the guard with a PORT NOTE naming the toolchain, and record an N); the menu-shortcut parity harness → GREEN.
- [ ] **Step 3: Commit.** `macos: restoration honors window-save-state=never; hide Kitty clipboard program name; drop compiler checks (136029788, 934ef8140, 0e75d015f, e2721d093, be595d8f0) (P8-W7 T2)`.

### Task 3: iTerm2 themes (`406f2e544`)

**Spec:** R4 §4.1; DECISIONS P16.

**Files:**

- Modify: the vendored themes directory and its manifest (657 files at `20260928-151043-99d9701`), and the four files that hard-code `617` (R4 §4.1 lists them).

- [ ] **Step 1: Negative control.** `python3 .port/tools/gates/check_theme_corpus.py` with the count bumped but files not re-vendored → RED.
- [ ] **Step 2: Re-vendor** (40 added, 1 changed, 0 removed); update the manifest and the four counts.
- [ ] **Step 3: Verify.** G-THEMES → GREEN (657); G-EOL → GREEN.
- [ ] **Step 4: Commit.** `themes: update iTerm2 color schemes to 20260928-151043-99d9701 (406f2e544) (P8-W7 T3)`.

---

# `P8-W8` — esctest (after P8-W3 and P8-W5 T3)

### Task 1: `test/esctest` package, report-only CI job, G-ESCTEST

**Spec:** R2 §2.11; R4 §4 (esctest), §8 Q1; DECISIONS P3.

**Files:**

- Create: `test/esctest/` as a separate Cargo package **outside** the workspace (`[workspace]` stanza of its own, or `exclude` in the root `Cargo.toml`): `Cargo.toml`, `src/main.rs` (port of `test/esctest/src/main.zig`: a pty-less driver that feeds esctest's output into `rustty_vt_c` and answers its queries), `README.md` (port of upstream's, token-renamed), a fetch script pinned to the esctest2 commit upstream's `build.zig.zon` names, verified by hash.
- Create: `.port/tools/gates/check_esctest.sh` (G-ESCTEST): run the Rustty harness and the oracle harness (built in the oracle tree at the pin with `zig build` under `test/esctest`), compare failing-test sets; RED if Rustty fails a test the oracle passes, or passes one the oracle fails (parity both ways; a Rustty pass where Ghostty fails is recorded and investigated, never silently accepted). `--self-test` with a fixture pair that differs → RED.
- Modify: `.github/workflows/ci.yml` (report-only `test-esctest` job, as upstream's `edcaf6670` enables DECRQCRA), `.port/tools/ci.sh` (add G-ESCTEST as a local gate), `.port/phase-0/corpora/MANIFEST.md` (close TODO-2), `.port/tools/CI.md`.

- [ ] **Step 1: Self-test first.** `bash .port/tools/gates/check_esctest.sh --self-test` → the differing fixture pair is RED, the matching pair GREEN.
- [ ] **Step 2: Port the harness** and fetch script; first local run on the Mac. Save the tally to `.port/phase-8/w8/esctest-rustty.txt` and the oracle's to `esctest-oracle.txt`.
- [ ] **Step 3: G-ESCTEST.** Run it. Each difference is a finding: a Rustty-only failure is fixed in this task (fail-first) or opened as an N with an owner; a Ghostty-only failure is checked against the rule (likely a UD candidate, measured before filing).
- [ ] **Step 4: Verify** `cargo build --manifest-path test/esctest/Cargo.toml` on Mac and rvm; G7 (acyclic) unchanged.
- [ ] **Step 5: Commit.** `test: esctest harness against librustty-vt, report-only CI job, G-ESCTEST (438d584f0, edcaf6670) (P8-W8 T1)`.

---

# `P8-W9` — Finale (coordinator)

### Task 1: Merge, gates and records

- [ ] Every workstream is merged `--no-ff` into `phase-8-upstream-resync`; after each merge `cargo build --workspace` (reused `CARGO_TARGET_DIR`), then its worktree, `worktree-agent` branch, merged branch and `target/` are removed by literal full path. rsync any git-ignored `.superpowers/` ledger out of a worktree before removing it.
- [ ] Every row of `R4-upstream-tests.tsv` is checked by exact name: 90 added tests present, 30 changed bodies re-aligned, 0 removals. Record as `.port/phase-8/w0/tests-checklist.txt` (N519: the 17 masked names are ticked by hand).
- [ ] Gates GREEN at `a60e9e2a5`, each saved as `.port/phase-8/w0/<gate>-finale-at-a60e9e2a5.txt`, and a one-line verdict file `gates-finale-summary-at-a60e9e2a5.txt` in Phase 7's format:
  - G-TESTS (0 unexplained), G-FILES, G-SYMS;
  - G1b (extended map), G3b and `--strict`, G-LOGFILE (and their self-tests);
  - G-CENSUS, A3, A4, A5 (and its masking self-test);
  - G4 (drivers rebuilt, goldens re-captured);
  - G-REFGEOM, G-EOL, G-I18N, G-THEMES (657), G-UDDOCS, G-PARITY, G7;
  - G9 (regenerated surface), G-P2, G-ORACLE-SH-SELF, G-SKIP (`check_silent_skips.py`), G-ESCTEST;
  - `bash /Users/gael/projects/rustty/.superpowers/sdd/2026-09-08-parity-defect-closure/postmerge_gates.sh` (git-ignored; build, clippy, fmt, vt-headers, embed-abi, rustty-h-abi, g-skip, g-logfile, oracle-self, test-counts, acyclic, vtalloc, phase2, g3b-self, ud-docs, g-parity); `check_parity_closure.py`; `check_phase2_closure.py`.
- [ ] Targeted cross-host checks: rvm GTK-feature clippy and `cargo test -p` for every touched crate plus the live GTK tests; rwin `rustty-vt-c` DLL + `c-vt-effects`; macOS `xcodebuild` RusttyTests and the menu-shortcut parity harness.
- [ ] Records:
  - `.port/phase-8/README.md` with the finale table (per workstream: scope, commits, merges, UDs/Ns/Vs opened and closed), in Phase 7's format;
  - `.port/ORACLE_SYNC.md` finale note;
  - `.port/parity-closure/README.md` id register updated;
  - every UD/N/V opened this phase closed or carried with an owner.

### Task 1b: Cross-platform build check — before any long run

- [ ] **Linux (rvm):** fresh clone of the phase branch: `cargo build --workspace --all-targets --features rustty/gtk,rustty/x11,rustty/wayland`; the same clippy with `-D warnings`; `cargo build --release -p rustty`.
- [ ] **Windows (rwin):** fresh clone: `cargo build --workspace --all-targets --target x86_64-pc-windows-msvc` (excluding `libxml2-sys`, `fontconfig-sys`, `sentry-sys`) plus clippy; `x86_64-pc-windows-gnu` for `rustty-font-shaper` with `fontconfig_freetype`; `rustty-vt-c --release`.
- [ ] **macOS:** `cargo build --workspace --all-targets` and `xcodebuild` for the app.
- [ ] A build failure on any host is fixed here (fail-first reproduction, fix, rebuild on that host), never deferred.
- [ ] Delete the VM clones and every `target/` by literal path; fstrim rvm.

### Task 2: The long validation runs (same matrix as Phase 7's W12 T2 and its follow-ups)

Run each lane at the finale head; one VM lane at a time; benches with the VMs paused (the maintainer pauses UTM). Save each result under `.port/phase-8/w9/`. A RED result becomes a NEW-FINDINGS entry and a fail-first fix task before the merge to `main`; a gate is never relaxed to pass (R5).

| lane                         | command                                                                                                                                                                                                                                                | host          | last recorded runtime                                   |
| ---------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ------------- | ------------------------------------------------------- |
| Full Linux lane              | `cargo test --workspace` with GTK features; G-GTKFEAT `check_gtk_feature_lane.sh`; G-REL `check_release_lane.sh`; G-EXAMPLES `check_examples.sh`                                                                                                       | rvm           | ~1–2 h                                                  |
| Full `ci.sh`                 | `bash .port/tools/ci.sh`                                                                                                                                                                                                                               | Mac, then rvm | hours (never recorded end to end — record it this time) |
| G6 Miri Stacked Borrows      | `bash .port/tools/gates/check_leaks.sh --all-done`                                                                                                                                                                                                     | Mac           | ~3.7 h                                                  |
| G6c debug assertions off     | `bash .port/tools/gates/check_leaks.sh --debug-assertions-off`                                                                                                                                                                                         | Mac           | ~19 min                                                 |
| G6t Tree Borrows             | `bash .port/tools/gates/check_leaks.sh --tree-borrows` (scope self-check runs every run)                                                                                                                                                               | Mac           | ~1–2 h (record wall time)                               |
| G-ASAN                       | `bash .port/tools/gates/sanitizers.sh --gate asan`                                                                                                                                                                                                     | Mac and rvm   | ~15 min (Mac)                                           |
| G-LSAN                       | `… --gate lsan`                                                                                                                                                                                                                                        | Mac and rvm   | 33 min / 20 min                                         |
| G-MSAN                       | `… --gate msan`                                                                                                                                                                                                                                        | rvm           | ~50 min                                                 |
| G-HWASAN                     | `… --gate hwasan`                                                                                                                                                                                                                                      | rvm           | ~56–77 min                                              |
| G-TSAN                       | `… --gate tsan`                                                                                                                                                                                                                                        | Mac and rvm   | 38 min / 21 min                                         |
| G-LSAN-GTK                   | `… --gate lsan-gtk` (headless sway; ceilings 7,600 / 2,420)                                                                                                                                                                                            | rvm           | minutes                                                 |
| G-SAN-SELF                   | `… --self-test`                                                                                                                                                                                                                                        | Mac and rvm   | minutes                                                 |
| G-VALGRIND                   | `bash .port/tools/gates/check_valgrind.sh` (4 shards, `--jobs 4`)                                                                                                                                                                                      | rvm           | ~3 h 44 min                                             |
| Windows full workspace (V26) | the V26 command plus the DLL step                                                                                                                                                                                                                      | rwin          | ~1 h                                                    |
| G-ESCTEST                    | `bash .port/tools/gates/check_esctest.sh`                                                                                                                                                                                                              | Mac and rvm   | ~1 min                                                  |
| Perf suite vs Ghostty        | `python3 .port/tools/perf/ws10_suite.py` against a zigbench rebuilt at `a60e9e2a5` with `-Doptimize=ReleaseFast` (update the suite's oracle sha); must hold 14/14 wins, with particular attention to `osc-parser`, `terminal-stream`, `t-parser`/print | Mac, idle     | ~1–2 h                                                  |
| G8 baseline                  | re-base `.port/phase-0/baseline/BASELINE.json` on the oracle at the new pin (V17 recipe, ReleaseFast rows only, per-label size tolerances per V16), then `python3 .port/tools/gates/check_baseline_regression.py --strict` and G8b `--build`           | Mac, idle     | minutes after the suite                                 |
| Re-audit                     | Task 41 step 3 over `src/terminal`, `src/terminal/c`, `src/renderer` at the new pin                                                                                                                                                                    | Mac           | —                                                       |

- [ ] Each lane GREEN, or its RED turned into a recorded, fixed and re-run finding.
- [ ] Clean up: VM clones deleted and fstrimmed after each VM lane; every `target/` outside the main checkout deleted by literal path; `df -h /Users/gael` recorded.

### Task 3: Close out

- [ ] `.port/phase-8/README.md` finale and validation tables complete; `.port/parity-closure/README.md` "Not verified here" updated (push-gated V25 rows stay the maintainer's call).
- [ ] Merge `phase-8-upstream-resync` into local `main` (fast-forward if possible) and delete the phase branch. **Do not push.**
- [ ] `git worktree list` shows only `/Users/gael/projects/rustty`.
- [ ] Update memory `phase-7-resync-in-progress` (or a new `phase-8-…` memory) with the new pin, `main` head, next free ids and lessons.

---

## Self-review

**1. Spec coverage.**

- **R1** themes 1–22: #1 → W0 T1; #2–3 → W1 T1; #4, #5, #8, #10 → W2 T1; #6–7 → W1 T2; #9 → W3 T1 (+ T2/T3 for other tiers); #11, #12, #22 → W2 T2; #13–14 → W5 T1; #15–18 → W5 T2; #19–20 → W4 T1; #21 → W2 T3. R1-G verification → each task's Miri step and W9.
- **R2** items 1–20 and workstreams W1–W5 → P8-W4 T1–T3 (parser), P8-W5 T2 (stream/effects), P8-W5 T3 (vt-c/headers), P8-W5 T4 (examples/Windows), P8-W8 (esctest); #17 (XT checksum C options) → P8-W3 T3.
- **R3** items 1–14 → W6 T1 (#1), W6 T2 (#2, #3), W6 T3 (#4, #5), W6 T4 (#6), W2 T3 (#7), W7 T1 (#8), W3 T2 (#9), W5 T2 (#10, #11), W7 T2 (#12–14).
- **R4** items 1–14 and R4-A…F → W0 T3 (advance), W0 T4 (records), W7 T3 (themes), W5 T4 (Windows DLL CI), W8 (esctest), W0 T2 (generator, N517); `check_leaks.sh` webgl entry → W6 T1; finale checklist → W9.
- **Commits:** every non-VOUCHED commit in `R4-commits.tsv` has an owner above; the 15 VOUCHED commits and the build renderings are NON_PORTS rows in W0 T4.
- **Questions:** all open questions from R1 (6), R2 (7), R3 (10) and R4 (8) are settled in `DECISIONS.md`; none needs the maintainer.

**2. Placeholder scan.** No "TBD". Tasks cite review sections for hunk-level detail instead of inlining it, deliberately, as in Phases 4–7: the reviews and upstream diffs are the source of truth, and duplicating them would drift. Each task names its files, its upstream tests by name, its fail-first step, its verification commands and its commit.

**3. Type consistency.** `write_cell` (W1 T2); `xt_checksum::Flags`, `Request`, `set_default_xt_checksum`, `rect_xt_checksum` (W3 T1 → T2, T3); `full_reset` slot (W2 T1 → W3 T1); `osc::Command::Unknown`, `set_unknown_max_bytes` (W4 T2 → W5 T2, T3); `Command::ProgramStatus` (W4 T3 → W5 T2); `SemanticPrompt` event (W5 T2 → T3); `Device`, `DeviceBox`, `Options.device`, `CreateError::DeviceFailed` (W6 T2 → T3); `prompt_click_relative_row` (W2 T3). C numbering: options 44/45 in W3 T3, 42/43/46 in W5 T3, all at upstream's numbers.

**4. Review Focus.** The five lines each have a named test in their owning task: W0 T1 (1), W3 T2 (2), W4 T2 (3), W6 T2 (4), W1 T2 (5).
