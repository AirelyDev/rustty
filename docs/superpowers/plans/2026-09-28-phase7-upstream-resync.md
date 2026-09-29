# Phase 7 — Upstream Resync Implementation Plan (pin `20abdb50a` → `12752b2ac`)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Bring Rustty to parity with Ghostty `origin/main` `12752b2ac` (2026-09-28): 333 commits (234 non-merge), 233 files, +35,723/−9,235. The rule is parity, except Rustty never copies a Ghostty bug. At the finale every pin-sensitive gate is green at the new pin, all 112 new upstream tests are ported by name, and all 40 changed upstream test bodies are re-aligned.

**Architecture:** Advance-first, as in Phases 4–6.

- P7-W0 fixes the one live bug Rustty carries (the macOS display-link deadlock), makes the UD-docs check a tracked gate, moves the oracle pin, and does the phase-start bookkeeping, so every red gate afterwards is attributed.
- Eleven porting workstreams follow. Each runs in its own hand-made worktree off the phase branch `phase-7-upstream-resync`, and each merges back with `--no-ff`.
- The critical paths are:
  - P7-W1, the terminal memory model;
  - P7-W7, the GtkGLArea → EGL/DMA-BUF redesign;
  - P7-W10, Unicode 18 and fontconfig 2.18.3.
- P7-W12 is the finale.

**Tech Stack:** Rust workspace (`crates/`, `crates-sys/`), C via `cc`/bindgen, Swift (`macos/`), gtk4-rs, `khronos-egl`, the `gl` crate, Zig 0.16.0 for the oracle and harnesses. Linux lane is `ssh rvm` (192.168.65.2); Windows lane is `ssh rwin` (192.168.64.5, PowerShell).

**Spec:**

- The five delta reviews in `.port/phase-7/delta-review/`:
  - `R1-terminal-core.md`
  - `R2-gtk-renderer-font-macos.md`
  - `R3-lib-capi-input-core.md`
  - `R4-build-pkg-ci-i18n.md`
  - `R5-records-gates-coverage.md`, with `R5-commits.tsv` and `R5-upstream-tests.tsv`
- The rulings: `.port/phase-7/DECISIONS.md`. Where a review question is settled there, the DECISIONS row wins.

## Global Constraints

- **The per-commit spec is the reviews.** Each task cites review sections. The implementer reads the cited upstream hunks (`git -C /Users/gael/projects/ghostty show <sha>`, or the oracle working tree after P7-W0 T3) and the cited Rustty `file:line`. A task may not contradict a review row or a DECISIONS row without recording why in its commit message.
- **Port the range-diff END STATE, never intermediates.** Superseded within the range:
  - `45aa963d6`, `d00a49827`, `202fa8e39` (GL/GTK), and `11d1cc4fc` (themes hop);
  - `mouse/shape.h`, added by `78494bad8` and folded into `mouse.h` by `67fb0c940`: never create it;
  - the four German-translation duplicates.
- **Parity except bugs** (`.port/phase-7/DECISIONS.md`, memory `decide-by-parity-minus-bugs`). Every confirmed upstream defect is fixed and recorded as a UD, with a test that fails on upstream's behaviour. Next free ids: **N435, UD352, V27**.
- **R1 fidelity.** Every comment, every test assertion for assertion, every error path. Ported tests keep upstream's names (`audit_test_coverage.py` matches by name).
- **R2 renames** per token (`Ghostty`→`Rustty`, `ghostty`→`rustty`), casing preserved per token, including bash identifiers (`__ghostty_status` → `__rustty_status`) and po `msgstr`s.
- **R5.** Gates are never weakened; every new gate arm ships a failing negative control.
- **R7.** Upstream invariants (`assert`, `unreachable` guarding an invariant) port as `assert!`. Upstream `+|`/checked arithmetic ports as `saturating_*`/`checked_*` with `debug_assert!`.
- **R13.** Never port a reachable crash, UB, leak, deadlock, or a non-crash bug that breaks evident intent. Fix it and log it in `.port/parity-closure/UPSTREAM-DEFECTS.md` plus `docs/ghostty-bug-fixes.md`. Private-report crashes get no trigger recipe in the doc.
- **R14.** No silent skips. Target-specific tests use `#[cfg(...)]` with a PORT NOTE, or print a runtime skip. GPU tests go through the strict pivots (`RUSTTY_REQUIRE_GL`, `RUSTTY_REQUIRE_METAL`, and the new `RUSTTY_REQUIRE_DMABUF`).
- **Stacked Borrows discipline** (memories `rust-mut-args-strongly-protected`, `shaper-hook-sb-clean-interleave`, `intrusive-ptr-and-placeholder-fill`):
  - raw-pointer arguments with narrow `&mut` reconstruction;
  - never hold `&mut Page/Row/Screen/Terminal/Loop` across a callback or a raw alias;
  - restructured aliasing gets `LIFETIMES.tsv` rows first, and an isolated non-FFI Miri harness under `.port/phase-2/mirihar-<name>/` that shows UB before the change and a clean run after.
- **Perf** (memory `comptime-flattening-perf-hazard`). Zig comptime tables become `const`/`const fn` tables with `#[inline]`, never `OnceLock`/`HashMap`. Hot-path changes get a paired bench against a zigbench built at `12752b2ac` with `-Doptimize=ReleaseFast`, measured as loop-sweep ns/op. LTO stays banned.
- **C ABI.**
  - New C enums in `include/rustty/vt/` end with `_MAX_VALUE = RUSTTY_ENUM_MAX_VALUE`.
  - Headers are regenerated with `cargo run -p xtask -- header-gen`.
  - `include/rustty-embed.h` stays the exact token-rename of the oracle `include/ghostty.h`.
  - By-value structs are frozen with `// @embed-abi` rows.
  - Option/data discriminants follow upstream order: `OPT_RESIZE_PULL_SCROLLBACK = 40`, `OPT_RENDER_HOLD = 41`, `DATA_MOUSE_SHAPE = 41`.
- **Oracle.** It stays read-only and never builds into the Rustty tree (R3). Its HEAD moves only in P7-W0 T3.
- **Disk** (memory `parallel-worktree-disk-budget`):
  - at most 3 building worktrees on the Mac;
  - check `df -h /Users/gael` before every build and stop below 12 GB;
  - delete each worktree's `target/` after merge;
  - destructive cleanup names literal paths, never shell variables (memory `cleanup-literal-paths`).
- **Linux lane early.** The GtkGLArea removal, EGL/DMA-BUF, fontconfig ELF aliases, GTK scale and 30 N11 sites only compile on Linux.
- **Commit style:** `<subsystem>: <summary> (<sha…>) (P7-W<n> T<m>)`. Every commit keeps `cargo build --workspace` and clippy `-D warnings` green. End each commit message with the session's attribution lines.
- **Privacy:** never take screenshots. Use headless goldens, broadway, `grim`/`wayshot` on a VM display only.

## Review Focus

These are the five inputs a user will hit that no ported upstream test exercises. Each has a test in its owning task.

1. **GTK on a real compositor.** With a DMA-BUF-capable Wayland compositor, frames must show and must not flicker. Where GDK cannot import DMA-BUFs (broadway, some X11), the surface must fall back to CPU readback and still draw. Owner: P7-W7 T3, live checks on rvm for both paths.
2. **Fractional scaling.** At 1.25× and 1.5× with the OpenGL flip, text must be crisp, cells placed on whole device pixels, with no one-pixel seam at the top or bottom. Owner: P7-W7 T3, a pixel check at 1.25× on rvm headless Weston; this also settles DECISIONS M3.
3. **Tab or split close during an error path.** A PageList torn down after a failed allocation must neither leak nor trip the new `live == free.len` pool assertion. Owner: P7-W1 T5, a failing-allocator teardown test on every error path listed in R1 §2.1 item 2.
4. **Unicode 18 characters in the shell.** New wide ranges (Jurchen `18E00..19191`, Small Seal `3D000..3FC3F`) and the new emoji must advance the cursor by 2 cells, and InCB linker graphemes must stay one cluster. Owner: P7-W10 T2, terminal-level print tests for each class.
5. **modifyOtherKeys 2 in real editors.** Numeric-keypad digits must still type digits, alt+é must send UTF-8, and ctrl+letter must send the CSI 27 form. Owner: P7-W4 T1, an end-to-end encoder test through `Surface` key handling that sets MOK2 by escape sequence the way vim does.

---

## Execution order

```
P7-W0  T1 deadlock fix → T2 UD-docs gate → T3 ADVANCE → T4 phase-start records
   │
   ├── P7-W1  memory model (critical path)  ──┬──► P7-W3 render overscan (+bench)
   │                                          ├──► P7-W5 T4 search fix
   ├── P7-W2  C-API tables (after W1 T7)  ────┘
   ├── P7-W4  modes/stream/selection + input encoder
   ├── P7-W5  OSC ints, kitty Windows, wuffs zlib, tmux (T1–T3); T4 after W1
   ├── P7-W6  CSI 8 t cross-tier (after W4 T1 lands stream.rs)
   ├── P7-W7  GL leaves (T1) → renderer lifecycle + GTK RenderSurface (T2–T3) → T4
   ├── P7-W8  font + core small
   ├── P7-W9  macOS Swift (T2 after W6)
   ├── P7-W10 Unicode 18 (before W4 T3's print path bench) · fontconfig · po/themes · CI
   └── P7-W11 config/os/termio/shell/embedding
                         ▼
               P7-W12 FINALE
```

Parallelism: at most 3 building worktrees. The suggested first wave is W1 + W10 (Unicode 18) + W7 T1. Swap in W4, W5, W8, W11 as slots free up. W9 needs no cargo build and can run alongside.

---

# `P7-W0` — Pre-advance fix, gate, advance, phase-start records (coordinator)

### Task 1: macOS display-link deadlock (`72cf50855`)

**Spec:** R2 §5.1; DECISIONS D2.

**Files:**

- Modify: `crates/rustty-renderer/src/generic.rs:6320-6490` (`draw_frame_raw`: split the locked body into `draw_frame_locked`, and call `sync_display_link` after `draw_mutex` is released).
- Modify: the `syncDisplayLink` doc at upstream `generic.zig:1223`.
- Test: `crates/rustty-renderer/src/generic.rs` tests module (macOS).

**Interfaces:**

- Produces `fn draw_frame_locked(this: *mut Self, sync: bool) -> Result<DrawOutcome, _>`. It returns whether the no-redraw path wants the display link stopped, so the caller syncs after unlocking. P7-W7 T2 builds on it.

- [ ] **Step 1: Write the failing test.** Add a seam `between_draw_unlock_and_sync` (test-only hook, same pattern as UD70's `between_display_stores`). The test does the following:
  - thread A holds the draw path in the no-redraw branch;
  - thread B takes `draw_mutex` through `draw_frame(sync = true)`;
  - it asserts that `sync_display_link` is never called while `draw_mutex` is held, by recording a `held_at_sync: AtomicBool` from inside a test double of `sync_display_link`.
- [ ] **Step 2: Run it and confirm it fails.** `cargo test -p rustty-renderer draw_frame_does_not_sync_display_link_under_draw_mutex` → FAIL (`held_at_sync == true`).
- [ ] **Step 3: Port the end state.** Port `generic.zig:1679-1705` from `72cf50855` into `generic.rs`, keeping the upstream comments verbatim.
- [ ] **Step 4: Run and confirm it passes.**
  - The test above → PASS.
  - `RUSTTY_REQUIRE_METAL=1 cargo test -p rustty-renderer` → PASS.
  - The N291 display-link test → PASS.
- [ ] **Step 5: Commit.** `renderer: never stop the display link while holding the draw mutex (72cf50855) (P7-W0 T1)`.

### Task 2: Track the UD-docs check as a `ci.sh` gate

**Spec:** DECISIONS R2.

**Files:**

- Create: `.port/tools/gates/check_ud_docs.py`, a copy of `.superpowers/sdd/2026-09-08-parity-defect-closure/task-S-6-check_ud_docs.py`, with two changes:
  - every UD entry must have exactly one `- upstream status:` line;
  - an entry whose status starts with `fixed upstream in` is exempt from needing a `docs/ghostty-bug-fixes.md` row.
- Modify: `.port/tools/ci.sh` to register `"G-UDDOCS-SELF|HARD|$PY $GATES/check_ud_docs.py --self-test"` and `"G-UDDOCS|HARD|$PY $GATES/check_ud_docs.py"`.

- [ ] **Step 1: Write the self-test with negative controls.** Build synthetic files in a temp dir covering four cases:
  1. a UD with no status line → RED;
  2. a UD with two status lines → RED;
  3. a UD without a docs row and not fixed upstream → RED;
  4. a fixed-upstream UD without a docs row → GREEN.
- [ ] **Step 2: Run it and confirm the self-test passes and each control fails.**
  - `python3 .port/tools/gates/check_ud_docs.py --self-test` → `SELF-TEST GREEN`.
  - Temporarily delete the exemption and confirm case 4 turns RED; then restore it.
- [ ] **Step 3: Run it on the tree.** `python3 .port/tools/gates/check_ud_docs.py` → `CHECK PASS` (178 UDs).
- [ ] **Step 4: Commit.** `gates: G-UDDOCS tracks the UD ↔ docs check in ci.sh (P7-W0 T2)`.

### Task 3: Advance the oracle pin to `12752b2ac`

**Spec:** R5 §6; `.port/ORACLE_SYNC.md` §"Advancing the pin"; memory `phase-6-upstream-resync`.

**Files:**

- Modify:
  - `.port/tools/gates/ORACLE_PIN`
  - `.port/ORACLE_SYNC.md` (pinned block, a history row, "Upstream beyond this pin")
  - `.port/PORT_RULES.md` (R12)
  - `.port/cycle-analysis.md`
  - `.port/tools/oracle-ghostty-h.sha256` → `78232aec570e266b1ccacaa69b0536c9b7757f4cf747ad26b9d7fb2d378848dd`
  - `.port/phase-3/CENSUS_DELTA.md` → 4397 / 17717 / 357, plus a history row
  - `.port/phase-0/inventory/parity-surface.json` (regenerated at the new pin)
- Create:
  - `.port/phase-7/w0/*-before-at-20abdb50a.txt` and `*-after-at-12752b2ac.txt`
  - `.port/phase-7/UPSTREAM_DIFF.patch`, `UPSTREAM_DIFF_STAT.txt`, `UPSTREAM_COMMITS.txt` (234), `UPSTREAM_COMMITS_WITH_MERGES.txt` (333)

- [ ] **Step 1: Pre-flight.**
  - `git worktree list` shows only the main checkout.
  - `git -C /Users/gael/projects/ghostty status --porcelain` is empty.
  - `git -C /Users/gael/projects/ghostty rev-parse HEAD` = `20abdb50a6216c450d6d4d010c41c7edf5ab15b2`.
  - `zig version` = `0.16.0`.
  - Create the phase branch: `git switch -c phase-7-upstream-resync`.
- [ ] **Step 2: Record "before".** For each of these gates, run it and save its output to `.port/phase-7/w0/<gate>-before-at-20abdb50a.txt`:
  - G1b: `python3 .port/tools/gates/check_test_counts.py`
  - G3b: `python3 .port/tools/gates/check_comment_coverage.py`, plus `--strict`
  - G-TESTS: `python3 .port/tools/audit_test_coverage.py --limit 500`
  - G-FILES and G-SYMS (same tool's arms)
  - G-CENSUS: `python3 .port/tools/gates/check_census_staleness.py`
  - G-LOGFILE: `python3 .port/tools/gates/check_log_reconcile.py`
  - G-REFGEOM
  - A3: `bash .port/tools/gates/check_rustty_h_abi.sh`
  - A5: `bash .port/tools/gates/check_rustty_vt_headers.sh`. Record its RED as pre-existing (DECISIONS D1).
- [ ] **Step 3: Write the range artifacts.** For example: `git -C /Users/gael/projects/ghostty diff 20abdb50a 12752b2ac > .port/phase-7/UPSTREAM_DIFF.patch`, `--stat` for the stat file, and `log --no-merges --format='%h %s'` for the commit lists.
- [ ] **Step 4: Advance.**
  - `git -C /Users/gael/projects/ghostty checkout --detach 12752b2ac1bb05ce53402ed8c853ed1f96eef0b1`.
  - Rebuild the oracle `zig-out/include`.
  - Run `bash .port/phase-0/harness/build_ref_driver.sh` (G4's staleness guard cannot see stale drivers).
  - `bash .port/tools/gates/_oracle_root.sh --self-test` → GREEN.
- [ ] **Step 5: Update the files listed above.** Regenerate `parity-surface.json` with its phase-0 generator. Check G4's canonical serialization for page capacity (DECISIONS R11) and write the finding into `w0/README.md`.
- [ ] **Step 6: Record "after".** Run the Step 2 gate set again, saving to `*-after-at-12752b2ac.txt`. Expected arrival (R5 §3):
  - G1b RED, 1 row (`input_key_encode`);
  - G-TESTS 94 unexplained, G-FILES 8, G-SYMS 57;
  - G3b 610 missing;
  - G-LOGFILE RED (6 arms);
  - A3 GREEN;
  - G-CENSUS GREEN with the new delta.

  Any other red is a finding: add it to `w0/README.md`.

- [ ] **Step 7: Commit.** One commit: `oracle: advance pin 20abdb50a → 12752b2ac (333 commits, 234 non-merge) (P7-W0 T3)`.

### Task 4: Phase-start records and gate maps

**Spec:** R5 §§1–3; R1 §3 "Gate bookkeeping"; R3 §3.1; DECISIONS R1–R10.

**Files:**

- Modify:
  - `.port/tools/gates/check_test_counts.py`: add MAP rows for `datastruct/{untouched_pool,comptime_int_set,wasm_page_pool}.zig`, `lib/{parse_int,types}.zig`, `apprt/gtk/scale.zig`, `font/nerd_font_attributes.zig`, `snapshot/terminal.zig`; add `zlib.zig` to the `wuffs` row.
  - `.port/phase-0/inventory/tests-summary.json` (the G3b `crate_map`): the same new files.
  - `.port/phase-4/NON_PORTS.md` (TinyIo row → `src/lib/tinyio/{posix,windows,test}.zig`; reopen trigger re-read for `f6cb8312b`, did not fire).
  - `.port/phase-3/NON_PORTS.md`: rows for `passthrough.glsl`, `7adbb5160`, `7644e6d62`, translate-c/cImport, Zig package and Nix renderings, the GTK build cache, VOUCHED.
  - `.port/parity-closure/UPSTREAM-DEFECTS.md`: statuses for UD150, UD140, UD141, UD31, UD40, UD32, UD200 (DECISIONS M1), plus re-anchoring per DECISIONS D4.
  - `.port/parity-closure/NEW-FINDINGS.md`: new **N435** (search `unreachable!()`, `06178eeaa`) and **N436** (search tick UAF, `b1c264163`), both OPEN, closed in P7-W5 T4 and P7-W2 T1.
  - The LEDGER `oracle` column for the 38 WAIVED rows listed in R5 §2.2 (tab-split edit).
  - `.port/parity-closure/README.md` "Still open": drop the stale N311 line.

- [ ] **Step 1: Negative control for each map extension.**
  - After adding a MAP row, G1b must report the new file's upstream test count against a Rust count of 0. It must go RED where tests are unported: `untouched_pool` 5/0.
  - Record the red rows in `w0/README.md`.
- [ ] **Step 2: Records.**
  - Apply the UD/N/NON_PORTS/LEDGER edits above.
  - `python3 .port/tools/gates/check_ud_docs.py` → PASS.
  - `python3 .port/tools/gates/check_parity_closure.py` → GREEN.
  - The phase-2 closure gate → GREEN.
- [ ] **Step 3: Re-run the audit.** `python3 .port/tools/audit_test_coverage.py --limit 500` → the 18 TinyIo test gaps and 26 symbol gaps are gone (the NON_PORTS row is extended).
- [ ] **Step 4: Commit.** `records: Phase 7 start — gate maps, TinyIo row, UD statuses, N435/N436 (P7-W0 T4)`.

---

# `P7-W1` — Terminal memory model (one worker, sequential; critical path)

**Spec:** R1 §2.1, R1 §3 R1-A; R5 §1.3 (UD64/UD80/UD131 re-derivation).
**Worktree:** `.claude/worktrees/p7-w1`, branch `phase-7/w1`.

Each task follows the same cycle:

1. Port upstream's new and changed tests first (names from `R5-upstream-tests.tsv`).
2. Run them and confirm they fail.
3. Port the end-state code.
4. Run them and confirm they pass.
5. Run the Miri subset for the task.
6. Commit.

Miri subset: `cargo +nightly miri test -p rustty-vt -- 'hash_map|ref_counted_set|bitmap_allocator|page::|pagelist'`, bounded at 30 min per run. Kill any run that exceeds it and record it as V27+.

### Task 1: Bitmap allocator "0 = free" (`ffe015ee5`)

**Files:** `crates/rustty-vt/src/bitmap_allocator.rs:112-123` (`init`, new `init_assume_zeroed`, `find_free_chunks` over `!bitmap`); tests in the same file.

- [ ] Re-align the 15 changed test literals (TSV rows for `bitmap_allocator.zig`) and run them → FAIL.
- [ ] Port the end state, then run `cargo test -p rustty-vt bitmap_allocator` → PASS.
- [ ] Run the Miri subset.
- [ ] Commit `(ffe015ee5) (P7-W1 T1)`.

### Task 2: Hash map and RefCountedSet from zeroed memory (`c0a4f80d8`, `6112935a2`)

**Files:**

- `crates/rustty-vt/src/hash_map.rs:454-499`: `keys`, `values` and `capacity` move into the struct; the in-buffer header shrinks to `{size: u32}`.
- `crates/rustty-vt/src/ref_counted_set.rs`: `init_assume_zeroed`; `assert_integrity` skips item 0 and bucket id 0.
- `LIFETIMES.tsv` rows for the three cached raw pointers.

- [ ] Add `LIFETIMES.tsv` rows. All three pointers derive from the page's raw `base` via `Offset::ptr`; never from a `&mut` projection.
- [ ] Build an isolated harness, `.port/phase-2/mirihar-p7-hashmap/`. It must show that caching `keys` and `values` derived from `&mut Page` is UB, and that the raw-`base` derivation is clean, under both Stacked and Tree Borrows. Command: `bash run-all.sh`.
- [ ] Port the end state. Keep the lz4 uninit-read PORT NOTE on `init` only.
- [ ] Run `cargo test -p rustty-vt hash_map ref_counted_set` → PASS, then the Miri subset.
- [ ] Commit `(c0a4f80d8, 6112935a2) (P7-W1 T2)`.

### Task 3: Page layout — zeroed init, cache-line cells, `MetaLayout` (`d2ff6d77a`)

**Files:**

- `crates/rustty-vt/src/page_impl.rs:480-590`: `init_buf` requires zeroed memory and calls `init_assume_zeroed`.
- `crates/rustty-vt/src/page.rs:1377-1415`: `available_bytes_for_grid`; `max_cols` and `adjust` computed in bytes; `MetaLayout`.
- A per-target `CACHE_LINE` const matching Zig `std.atomic.cache_line`: 128 on aarch64/x86_64, with the other targets taken from Zig's table.
- `crates/rustty-vt/src/page_tests.rs:609` (`page_geometry_matches_the_reference`).

- [ ] Re-probe N88 with the Zig page probe at `12752b2ac`, on macOS and on rvm aarch64, at 512 and 8,192 grapheme bytes (DECISIONS R14). Update the golden table in `page_tests.rs:609`, then run it → FAIL.
- [ ] Port the end state. `MetaLayout` must fold like `Page::layout` (`#[inline(always)]`, const where upstream is comptime).
- [ ] Verify that every `init_buf` caller passes zeroed memory: `init_pages`, `create_page_ext`, grow-prune reuse, `Page::reinit`. Add `debug_assert!` checks that sample the buffer.
- [ ] Run `cargo test -p rustty-vt page` → PASS, both hosts (macOS + rvm).
- [ ] Re-derive the UD64 / UD80 / UD131 constants and thresholds. Update their records, and re-verify the UD131 recipe at the new pin (DECISIONS R3).
- [ ] Commit `(d2ff6d77a) (P7-W1 T3)`.

### Task 4: `UntouchedPool` in rustty-datastruct (`e01e75bbb`)

**Files:**

- Create `crates/rustty-datastruct/src/untouched_pool.rs`: `UntouchedPool<T, A>` (const item size) and `DynamicUntouchedPool<A>` (runtime `std_size`).
  - The free list is the `zig_list.rs` `ArrayList` grown from `gen_alloc` (T3 call shape).
  - `create` reserves the free-list slot first.
  - `release` and `reset(limit)` free items; `deinit` and `reset` `assert!(live == free.len())` (R7).
- Port its 5 upstream tests by name.
- Make the page allocator a generic parameter defaulting to `PageBackingAlloc` (DECISIONS R13).

- [ ] Write the 5 tests (TSV `datastruct/untouched_pool.zig`) and run them → FAIL (module absent).
- [ ] Implement, then run `cargo test -p rustty-datastruct untouched_pool` → PASS.
- [ ] Run `cargo +nightly miri test -p rustty-datastruct untouched_pool` under both models → clean.
- [ ] Commit `(e01e75bbb) (P7-W1 T4)`.

### Task 5: PageList on `UntouchedPool`; per-node pooling; preheat (`e01e75bbb`, `efa3c66ed`, `35a6fb747`)

**Files:**

- `crates/rustty-vt/src/pagelist/mod.rs`:
  - `:722-725`, `:848`, `:1085-1101`: `PagePool`, `NodePool`, `PinPool` preheat 2, `node_preheat = 1`.
  - `:1164`, `:2262`: delete `zero_retained_page_arena`.
  - `:2122-2160`: teardown returns every node; read `(*node).next` before `destroy`.
  - `release_pool_page` and `release_pages`.
- `pagelist/access.rs:888-941`: `clone` appends before `clone_from`.
- `builder.rs:100,254`; `mod.rs:2563`, `:2638`.
- `pagelist/compress.rs:76-99`: the scratch clears only its dirty prefix (`60b43068c` non-Windows half).
- Retire `DynamicMemoryPoolAligned`/`ZigMemoryPool` for pages and nodes; keep `ZigMemoryPool` for pins.

- [ ] Port the new PageList tests "memory pool never touches idle page memory" and "fast path does not allocate", with the injected fixed-buffer / failing allocator. Re-derive the S7 counting tests (`pagelist/tests.rs:19121+`) and the vt-c allocation-failure sweeps. Run → FAIL.
- [ ] **Review Focus 3 test.** For each error path in R1 §2.1 item 2, add `pagelist_teardown_after_<path>_failure_returns_every_node`: a failing `VtAllocator` fails the path, then `deinit` runs. Assert no leak (the counting allocator) and no pool-assert panic. Run → FAIL on at least the clone path before the port.
- [ ] Port the end state. Keep `page_serial_epoch` and fresh serial assignment in `createPage` load-bearing (row ids depend on it).
- [ ] Run:
  - `cargo test -p rustty-vt pagelist` → PASS;
  - the Miri subset;
  - G6b `bash .port/tools/gates/check_pagelist_compress_leaks.sh`;
  - the S7 Miri groups `vt-pagelist` and `vtc`.
- [ ] Re-baseline the alloc and `sc.clone` benches against zigbench at `12752b2ac`, and record them in N89/N360.
- [ ] Commit `(e01e75bbb, efa3c66ed, 35a6fb747) (P7-W1 T5)`.

### Task 6: Windows reclaim (`60b43068c` Windows half)

**Files:** `crates/rustty-vt/src/mem.rs:67-72,160-182,206-246` (`can_reclaim` Windows arm via `DiscardVirtualMemory`); `crates/rustty-os/src/windows.rs` (`ERROR_SUCCESS`, the `DiscardVirtualMemory` extern, with the WI-5 live-caller note).

- [ ] Write a Windows test (`#[cfg(windows)]`, R14 PORT NOTE) asserting that `can_reclaim()` is true and that a decommit smoke run lowers the working set. Run on rwin → FAIL.
- [ ] Port, then on rwin run `cargo test -p rustty-vt mem` and the release smoke run → PASS (DECISIONS D3).
- [ ] Commit `(60b43068c) (P7-W1 T6)`.

### Task 7: W1 merge gate

- [ ] Run, in the worktree:
  - `cargo build --workspace`, clippy `-D warnings`, fmt;
  - `cargo test -p rustty-vt -p rustty-vt-c -p rustty-datastruct`;
  - on rvm: `cargo test -p rustty-vt page pagelist`;
  - `bash /Users/gael/projects/rustty/.superpowers/sdd/2026-09-08-parity-defect-closure/postmerge_gates.sh <worktree>`.
- [ ] Merge `--no-ff` into `phase-7-upstream-resync`, then delete the worktree's `target/` by literal path.

---

# `P7-W2` — libghostty-vt C-API tables (one worker owns every `c/terminal` Effects/Option/Data edit)

**Spec:** R1 §2.3; R3 §§3.5–3.6; DECISIONS B1, B2, B6, B7.
**Worktree:** `p7-w2`. Tasks 5–7 start after P7-W1 merges.
**Every task:** regenerate headers (`cargo run -p xtask -- header-gen`), then run:

- `bash .port/tools/gates/check_rustty_vt_headers.sh`
- `check_closed_enum_abi{,_runtime}.sh`
- `check_abi_schema.sh`
- `check_symbol_manifest.sh`

### Task 1: Search tick after terminal free (`b1c264163`) — closes N436

**Files:** `crates/rustty-vt-c/src/search.rs:501-525` (guard via the raw-self `zig_terminal` helper at `:110-120`); `include/rustty/vt/search.h` doc.

- [ ] Port the test "search tick after terminal free with history" and extend `search_free_after_terminal_free`.
- [ ] Run `cargo +nightly miri test -p rustty-vt-c search::` → UB reported (the negative control; only Miri or ASan discriminate here).
- [ ] Port the guard, re-run Miri → clean, then close N436.
- [ ] Commit `(b1c264163) (P7-W2 T1)`.

### Task 2: Safe pointers for empty output (`dd2edd760`) — DECISIONS B1/B2

**Files:**

- `crates/rustty-lib/src/types.rs:18-23`: `String::init` maps `len == 0` to `static EMPTY: [u8; 1]`.
- `crates/rustty-vt-c/src/allocator.rs:21,49-64`: `alloc(0)` → NULL.
- `formatter.rs`, `selection.rs`, `terminal.rs` (the continuation and kitty temp-file getters).
- Headers: `allocator.h`, `formatter.h`, `selection.h`, `terminal.h`, `types.h`.
- Re-align N331/N361.

- [ ] Port the 6 tests: "String.init empty output", "alloc zero length" (+NULL assertion), "format_alloc empty output", "selection_format_alloc empty output", the continuation exact-suffix test, and "kitty_image_medium_temp_file empty output" (cfg `KITTY_GRAPHICS`, R14).
- [ ] Rewrite the three Rustty-only guard tests to the new behaviour, keeping their still-valid halves.
- [ ] Run → FAIL. Port, run → PASS.
- [ ] Run Miri on `formatter::|selection::|terminal::continuation|allocator::` → clean.
- [ ] Commit `(dd2edd760) (P7-W2 T2)`.

### Task 3: Kitty temp dir shared copy (`105d0a545`) — DECISIONS B6

**Files:** `crates/rustty-vt-c/src/terminal.rs:91-110,215-225,3137-3190`; `crates/rustty-vt/src/kitty/graphics_image.rs:168-174` (screens borrow `*const [u8]`); `LIFETIMES.tsv` row.

- [ ] Add a test asserting that every screen's temp-dir pointer equals the wrapper's single copy, and that clearing the path frees it (counting allocator) → FAIL.
- [ ] Port, run → PASS; Miri the vt-c kitty temp tests.
- [ ] Commit `(105d0a545) (P7-W2 T3)`.

### Task 4: Mouse shape in the C API (`78494bad8` + `67fb0c940` end state)

**Files:**

- `include/rustty/vt/mouse.h`: `RusttyMouseShape` (the token rename of `GhosttyMouseShape`), 34 values plus `RUSTTY_MOUSE_SHAPE_MAX_VALUE = RUSTTY_ENUM_MAX_VALUE`; `#include "types.h"`. No `mouse/shape.h`.
- `crates/rustty-vt-c/src/terminal.rs:3830`: `TerminalData::MouseShape = 41`, reading `t.mouse_shape`.
- The `types.rs` registry row (`initEnum("RusttyMouseShape")`).

- [ ] Port "get mouse_shape" and the manifest assertions (`TEXT`=8, `ZOOM_OUT`=33, `MOUSE_SHAPE`=41) → FAIL.
- [ ] Port, then run → PASS; A5 arm (c) loses its `MouseShape` miss.
- [ ] Commit `(78494bad8, 67fb0c940) (P7-W2 T4)`.

### Task 5: Resize pull scrollback (`c55f213aa`)

**Files:** `crates/rustty-vt/src/pagelist/resize.rs:82-100,265,362,596,736-760` (the temporary tracked pin in `resize_cols`, untracked on every error path); `screen.rs:541-557`; `terminal.rs:330-395,6199,6907` (the flag survives `full_reset`); vt-c `OPT_RESIZE_PULL_SCROLLBACK = 40`; `terminal.h`.

- [ ] Port the 8 tests (1 PageList, 5 Screen, 1 Terminal, 1 vt-c) → FAIL.
- [ ] Port, then run → PASS, plus G6 on `pagelist::resize*|screen::resize*|terminal::resize*`.
- [ ] Commit `(c55f213aa) (P7-W2 T5)`.

### Task 6: Render hold for mode 2026 (`e50779498`)

**Files:** `crates/rustty-vt/src/stream_terminal.rs:1662-1666` (`Effects.render_hold`; `set_mode(synchronized_output)` reports only real transitions; `restore_mode`, `full_reset` and `resize` release the hold); vt-c `RenderHoldFn`, the trampoline via the D3 `offset_of!` seam (memory `vtc-effects-trampoline-seam-d3`), `OPT_RENDER_HOLD = 41`, and C `reset` firing `false` via `addr_of_mut!`.

- [ ] Port the 3 tests. Add a vt-c test that calls `rustty_render_state_update` inside the callback.
- [ ] Add a new Miri target in the `vtc` group for that re-entry → FAIL (feature absent).
- [ ] Port. Run tests → PASS; Miri the re-entry target under both models → clean.
- [ ] Commit `(e50779498) (P7-W2 T6)`.

### Task 7: Dynamic palette shares the default (`d1cd56a56`) — DECISIONS B7

**Files:**

- `crates/rustty-vt/src/color.rs:419-470`: an owner enum; `init`/`change_default` fallible.
- `terminal.rs:366` (deinit frees).
- Snapshot decode (`snapshot/terminal.rs`).
- vt-c `color_palette` set returning `OUT_OF_MEMORY`.
- termio `rustty-termio/src/termio_impl.rs:607,1157`, `stream_handler.rs:3956`: apply after init; on OOM, log and fall back to `reset_default`.

- [ ] Re-align the 7 `DynamicPalette` tests and 3 snapshot tests, and add the new test "DynamicPalette: changeDefault shares the built-in default" (failing `VtAllocator`) → FAIL.
- [ ] Port, then run `cargo test -p rustty-vt -p rustty-vt-c -p rustty-termio color palette snapshot` → PASS.
- [ ] Commit `(d1cd56a56) (P7-W2 T7)`.

### Task 8: W2 merge gate

- [ ] Run the P7-W1 T7 command set for `rustty-vt-c`, `rustty-vt`, `rustty-lib` and `rustty-termio`, plus:
  - `cargo +nightly miri test -p rustty-vt-c` on the groups touched;
  - `bash .port/tools/gates/check_sanitizer_canary.sh`.
- [ ] Merge, then clean up.

---

# `P7-W3` — RenderState overscan and bench (after W1 and W2 T6)

**Spec:** R1 §2.5; R3 §§3.6–3.7; R5 §1.3 (UD315).

### Task 1: RenderState overscan in Zig-API form (`ca4f719f7`)

**Files:** `crates/rustty-vt/src/render.rs:134-180,992-1053,1106-1149,1178-1355,1491,1584,1641,2082` (`overscan_request`/`overscan`; `row_data` sized `above + rows + below`; `viewport_start`/`viewport_y`/`row_data_range`; `Row::Id {serial, y}`; cursor reported only inside the viewport; `overscan` written after the loop; the UD315 forced rebuild also resets overscan).

- [ ] Port the 8 `render.zig` tests and the `testCompareStates` assertions → FAIL.
- [ ] Port, then run → PASS; Miri the `vt-render` group.
- [ ] Restate N315's worked number with overscan.
- [ ] Commit `(ca4f719f7) (P7-W3 T1)`.

### Task 2: Overscan in the C API (`2ea1eeae2`)

**Files:** `crates/rustty-vt-c/src/render.rs:118-145`.

- The row iterator sub-slice is built with `ptr::slice_from_raw_parts_mut(row_data.as_mut_ptr().add(start), len)`, never `&mut row_data[range]`.
- New by-pointer structs `Overscan` (4 B) and `RowId` (16 B).
- `DATA_OVERSCAN = 20`, `DATA_OVERSCAN_REQUEST = 21`, `OPTION_OVERSCAN = 1`, `ROW_DATA_VIEWPORT_Y = 6`, `ROW_DATA_ID = 7`.
- `render.h` prose; registry rows.

- [ ] Port the 10 `c/render.zig` tests and the manifest test "manifest describes render state overscan types".
- [ ] Add a Miri test that interleaves `rustty_render_state_clean` with a live iterator → FAIL, or UB if the iterator is sliced from `&mut`.
- [ ] Port. Run tests → PASS; Miri → clean; A5 arm (c) → GREEN.
- [ ] Commit `(2ea1eeae2) (P7-W3 T2)`.

### Task 3: ScreenClone bench modes (`ca4f719f7` bench half)

**Files:** `crates/rustty-bench/src/screen_clone.rs` (six modes: `render-scroll`, `render-scroll-page`, `render-output`, `render-{scroll,output,clean}-overscan`; `bench_overscan = {above: 4, below: 1}`; 4,000 scrollback lines; direction reversal).

- [ ] Add a CLI test that every mode name parses and runs one iteration → FAIL.
- [ ] Port, then run → PASS; G-BENCH → GREEN.
- [ ] Record a paired `render-*` measurement against zigbench at `12752b2ac`.
- [ ] Commit `(P7-W3 T3)`.

---

# `P7-W4` — Modes, stream, selection, input encoder

**Spec:** R1 §2.4; R3 §3.2; DECISIONS B2, R12.

### Task 1: Legacy key encoder, modifyOtherKeys 2 series (`4406cea3e`, `636a2f35b`, `e7bdda991`, `37e3cdd2d`, `cc3fd8a77`, `587e08f3f`, and `1a8f331f1` doc lines)

**Files:** `crates/rustty-vt/src/input/key_encode.rs:497,552`; `crates/rustty-vt/src/input/function_keys.rs:851` (`kp_keys(suffix, normal)`, F13–F25 rows, Help/ContextMenu rows, the alt+escape `set`/`set_other` split); `crates/rustty-input/src/binding.rs:981-985`.

- [ ] Port the 9 tests ("legacy: ctrl+char with modify other state 2", "…without…", "legacy: keypad 1 with modify other state 2", "legacy: f13 through f25", "legacy: alt+escape with modify other state 2", "legacy: help and context menu", "legacy: alt+unicode", "…only unshifted", "…without unshifted").
- [ ] Rewrite `legacy_alt_prefix_high_byte_is_written_raw` (DECISIONS B2): `0xE9` unshifted now writes `\x1B\xC3\xA9`, and the single raw byte `[0xE9]` is still written verbatim.
- [ ] **Review Focus 5.** Add `surface_mok2_keypad_alt_ctrl_end_to_end` in `rustty-core`. It sets MOK2 by feeding `\x1b[>4;2m` through the terminal, then encodes keypad 1 → `1`, alt+é → `\x1b\xc3\xa9`, and ctrl+p → `\x1b[27;5;112~` through the Surface key path. Run → FAIL.
- [ ] Apply the commits in upstream order, keeping the Zig comptime table's match order in the runtime `Vec<Entry>` (R8).
- [ ] Run `cargo test -p rustty-vt input -p rustty-vt-c key_encode -p rustty-core mok2` → PASS; G1b `input_key_encode` → GREEN.
- [ ] Commit `(P7-W4 T1)`.

### Task 2: DECRQM ANSI and 16-bit modes; `ComptimeIntSet` (`9dc0d974e`, `3beb6d717`, `079502e23`)

**Files:**

- `crates/rustty-vt/src/stream.rs:3402` (`1 | 2 =>`).
- `modes.rs:150,375,407-419` (`Report::Tag {value: u16, ansi}`; `mode_from_int` rejects `> u15` and uses a const sorted-table binary search; keep the runtime `disabled_by_artifact` arm).
- `stream_terminal.rs:2532-2537`; `rustty-termio/src/stream_handler.rs:896-901` (drop `& 0x7FFF`); `rustty-vt-c/src/modes.rs:62-69`.
- The vt-c test rename to "set write_pty callback DECRQM".
- New `crates/rustty-datastruct/src/comptime_int_set.rs`: a `const fn`-built `[(u16, u16); N]` with a const duplicate check.

- [ ] Port the tests:
  - `stream: DECRQM dispatch`, `request mode DECRQM ANSI responses`, 2 extended;
  - `getReport large unknown modes`, `Report.encode maximum size`;
  - the 5 `ComptimeIntSet:` tests;
  - `modeFromInt matches entries exhaustively`, with the printed Miri cap from DECISIONS R12.

  Run → FAIL.

- [ ] Port. Run → PASS. Take a paired bench of the mode-2026 render check against zigbench (upstream measured 10.5 → 3.2 ns).
- [ ] Commit `(9dc0d974e, 3beb6d717, 079502e23) (P7-W4 T2)`.

### Task 3: Print, cursor, selection fixes (`a4f0d9f4a`, `d4f45bee3`, `c4f15c884`, `a3e80a685`)

**Files:** `crates/rustty-vt/src/terminal.rs:180,1355-1366,1388,1541,1647` (`charset_table` in `print_slice_fill`/`print_slice_store_run`; `assert!(charset_table.is_none() || !allow_unicode)`), `:3064-3067` (`if count == 0 { return }` after pending wrap); `screen.rs:5008-5080` (`select_word` hard-break check before the boundary test; new `select_word_codepoint`).

- [ ] Port the 3 + 1 + 3 tests. Check the vt-c `selection_gesture` and core double-click tests for pinned old end points. Run → FAIL.
- [ ] Port, then run → PASS.
- [ ] Take a paired print bench after P7-W1 T3 and P7-W10 T2 have landed (bench ordering).
- [ ] Commit `(P7-W4 T3)`.

---

# `P7-W5` — OSC integers, kitty, tmux, search

**Spec:** R1 §§2.6–2.9; R4 §C; DECISIONS B3, B4, B5, B8.

### Task 1: `parse_int` in rustty-lib at the 9 upstream sites (`73768913b`, `d92e3400e`, `500040d3b`, `31bbb1273`, `3822aa01d`)

**Files:**

- Create `crates/rustty-lib/src/parse_int.rs` with a width-faithful accumulator (`@max(8, bits)`; i3 `"-4"` ok, `"4"` overflows). Reject `_`; reject signs on unsigned types. Carry the upstream rationale doc.
- Callers: `osc/parsers/{color,osc9,kitty_text_sizing,semantic_prompt,kitty_desktop_notification,context_signal}.rs`, `kitty/color.rs:169`, `color.rs:1033` (`RGB::from_hex`).
- Keep `protocol_int.rs` only for the glyph APC and kitty graphics command (UD141 narrowed).

- [ ] Port the 9 tests ("protocol integer parsing", "RGB.parse rejects digit separators", the OSC 21/4/99/66/9/133 separator tests, "OSC 3008: numeric fields reject non-digits"). Flip the pinned tests (`color.rs:805ff` `+11`/`-0`; `kitty/color.rs:236-237`; `color.rs:1857-1859`). Run → FAIL.
- [ ] Port, then run `cargo test -p rustty-lib -p rustty-vt -p rustty-config` → PASS.
- [ ] Update UD141 and its docs row.
- [ ] Commit `(P7-W5 T1)`.

### Task 2: Kitty Windows paths and wuffs zlib (`0b5446364`, `4b86e3594`, `42049f1c4`)

**Files:**

- Create `crates/rustty-vt/src/kitty/windows.rs` (`check_path`, `check_canonical_path`, `is_drive_absolute`).
- `graphics_image.rs:734-800,939-990,1209-1254` (check before open; canonical buffer via `VtAllocator`; `is_path_in_temp_dir` fallible; `decompress_zlib` via wuffs with a size hint, propagating OOM).
- Create `crates/rustty-wuffs/src/zlib.rs` with 16-aligned decoder storage and 5 tests.
- `rustty-vt/Cargo.toml:146-156,257-269`: wuffs mandatory with kitty graphics, including wasm32; drop `zlib-sys` and the wasm stub (DECISIONS B8). Build `wuffs-sys` for wasm32 the way upstream does.

- [ ] Port the tests:
  - 6 pure `windows.zig` tests (all hosts);
  - 7 `image load: windows …` tests (`#[cfg(windows)]`; UNC share uses the R14 runtime skip);
  - the 5 wuffs zlib tests;
  - "image load: png, zlib compressed, direct" with its 95-byte testdata.

  Run → FAIL.

- [ ] Port.
  - Run `cargo test -p rustty-wuffs -p rustty-vt kitty` → PASS.
  - On rwin: `cargo test -p rustty-vt kitty` → PASS (DECISIONS D3).
  - `cargo build -p rustty-vt-c --release --target wasm32-unknown-unknown` → OK.
  - Also run G-VTFEAT, the symbol manifest, G-EXAMPLES, and G-XTARGET.
- [ ] Update UD140 (zlib half fixed upstream) and retire the PORT NOTE at `graphics_image.rs:1221`. Measure DECISIONS M7 (jpeg alignment).
- [ ] Commit `(P7-W5 T2)`.

### Task 3: tmux list-windows and mouse flags (`a41248013`, `9a4ba7d54`, `1ca2858d0`)

**Files:** `crates/rustty-vt/src/tmux/viewer.rs:311-360,1354-1462,1655-1663,2301,2537-2580` (`Action::Windows` borrows the viewer list as a raw slice; append after `sync_layouts`; errdefer truncate; `Display` prints `[N windows]`); `output.rs:196,258,337,664-668` (remove `mouse_any_flag`; shift the hand-indexed fields); rebuild the UD9/UD11 fixtures for the shorter `list-panes` reply.

- [ ] Port the tests: the list-windows lifetime test with its pointer-identity assertion, and the mouse-flag mapping tests. Delete `parse_mouse_any_flag` (upstream −1). Run → FAIL.
- [ ] Add a Miri target for the borrowed slice across a later viewer mutation (must be clean when the action is consumed first, as upstream requires).
- [ ] Port, run → PASS. Retire LEDGER S209-F3 (tab-split edit).
- [ ] Commit `(P7-W5 T3)`.

### Task 4: Search resumes after prepend (`06178eeaa`) — after P7-W1 merges; closes N435

**Files:** `crates/rustty-vt/src/search/screen.rs:618` (`State::Complete => self.state = State::History`, plus the doc updates).

- [ ] Port the 3 tests (`search/{pagelist,screen,terminal}.zig`); the screen test must panic on the current `unreachable!()` → FAIL.
- [ ] Port. Run → PASS; Miri the vt search lane plus `pagelist::builder`. Close N435.
- [ ] Commit `(06178eeaa) (P7-W5 T4)`.

---

# `P7-W6` — CSI 8 t window resize, all tiers (one owner) — DECISIONS R15, M6

**Spec:** R1 §2.4 #25; R2 §5.6, §6 Q7; R3 §§3.3, 3.8; R5 §1.5.

### Task 1: vt, termio, config, core, apprt-api, embed header (`714fe9b90`, `2a987d59d`, config doc of `219cca932`)

**Files:**

- `crates/rustty-vt/src/stream.rs` (`Action::ResizeWindow`, CSI t arm `8`), `stream_terminal.rs` (no-op).
- `crates/rustty-termio/src/stream_handler.rs` (forward as a surface message).
- `crates/rustty-config/src/config_struct.rs:538,4040,8636`: `vt-window-resize-allowed = false`, placed right after `vt-kam-allowed`, with its 16-line doc.
- `crates/rustty-core/src/surface.rs`: `resize_window` using the checked helpers (`u32_from_ceil`, checked sums), plus the minimum resize cells and `DerivedConfig`.
- `crates/rustty-apprt-api`: `Action::ResizeWindow {width: u32, height: u32}`, `Key::ResizeWindow`.
- `include/rustty-embed.h`: `rustty_action_resize_window_s`, `RUSTTY_ACTION_RESIZE_WINDOW`, the union member.
- `crates/rustty-apprt-c/tests/abi_layout.rs:209-211,659-676`: cardinality 69 → 70, plus a new `@embed-abi` row.

- [ ] Port the vt test (`stream.zig:4236`). Add config tests for the default and `show-config` order, and a core test that a program's `CSI 8;rows;cols t` produces `ResizeWindow` only when allowed. Run → FAIL.
- [ ] **Measure M6.** Build the reference at the pin with `adjust-cell-width` large, send `CSI 8;65535;65535 t`, and record whether it traps. If it does, open **UD352** (core) with a fail-first test that the checked Rustty path returns without panicking.
- [ ] Port. Run `cargo test -p rustty-vt -p rustty-termio -p rustty-config -p rustty-core -p rustty-apprt-c` → PASS.
  - A3/A4 → GREEN.
  - The rustty-embed.h exact-rename gate → GREEN.
  - Update the config goldens (`show-config`, completions).
- [ ] Commit `(714fe9b90, 2a987d59d) (P7-W6 T1)`.

### Task 2: GTK half (`219cca932`) — on rvm

**Files:** `crates/rustty-apprt/src/gtk/class/window.rs` (`resize_surface`: refuse quick terminal, maximized, fullscreen, tiled, more than one tab, or splits; convert the device-pixel delta with the integer scale; `set_default_size`; the `resize_requested` one-shot); `application.rs:4019` (action arm).

- [ ] **Measure M6 for GTK** (`resizeDelta` `c_int`). Open **UD353** if it traps, with a fail-first test. Add unit tests for each refusal condition → FAIL.
- [ ] Port. On rvm run the gtk-feature clippy and `cargo test -p rustty-apprt window` → PASS.
- [ ] Commit `(219cca932) (P7-W6 T2)`.

The Swift half is P7-W9 T2.

---

# `P7-W7` — GtkGLArea → EGL + DMA-BUF redesign, fractional scaling (Linux lane)

**Spec:** R2 §§3–4, 5.3–5.5, §6; DECISIONS B9, B10, B11, M2, M3, M4, M5, R4.

### Task 1: GL leaves (stage S1, additive; mergeable alone)

**Files:**

- `crates/rustty-renderer/src/opengl/gl/`: the `adf161473`/`590ab157f` wrapper — `read_pixels`, `blit_framebuffer`, the `BlitMask` bitflags, error-checked `viewport`.
- A per-thread struct-based `gl_generator` binding replacing the process-global `gl::load_with`. It is loaded in `thread_enter` and dropped in `thread_exit` (DECISIONS B9); every GL call site takes the table.
- `opengl/gl/sys.rs`: EGL `Image` create/destroy; `eglExportDMABUFImageQueryMESA`/`eglExportDMABUFImageMESA` via `get_proc_address`; `EGL_SURFACE_TYPE = 0`; no `eglTerminate` (DECISIONS B10).
- Create `crates/rustty-renderer/src/dmabuf.rs` (`OwnedFd` planes, `validate`, fixing both fd leaks under DECISIONS M4, **UD354**).
- `opengl/target.rs`: rewrite to an `SRGB8_ALPHA8` render texture plus FBO and an `RGBA8` export texture plus FBO, with `export_dmabuf` and `read_pixels_alloc`.
- A new R14 pivot `RUSTTY_REQUIRE_DMABUF`.

- [ ] Fail-first tests:
  - `dmabuf_validate_closes_every_fd_after_an_invalid_plane` (inject an invalid middle plane; count open fds) → FAIL on the upstream shape.
  - `read_pixels_alloc` KAT (headless, `RUSTTY_REQUIRE_GL`).
  - `export_dmabuf` KAT behind `RUSTTY_REQUIRE_DMABUF`.
  - The offscreen OpenGL golden must stay byte-identical.
- [ ] **Measure M5.** Run the offscreen golden with `alpha-blending = native` before and after the target rewrite. If the output changes, open **UD355**: keep the linear target for `native`, with a golden.
- [ ] Port. On rvm run `RUSTTY_REQUIRE_GL=1 cargo test -p rustty-renderer` → PASS. Record whether rvm offers `EGL_MESA_image_dma_buf_export` (install `mesa-utils`/`eglinfo` in user space if needed).
- [ ] Update UD32 (obsolete), UD40 (half fixed), and UD250 (new callers).
- [ ] Commit `(P7-W7 T1)`, then merge.

### Task 2 and Task 3: renderer lifecycle (S2) and GTK `RenderSurface` (S3+S4) — one branch, one merge

**Spec:** R2 §3.1, §3.3 (mapping table), §3.4 (staging).

**Files, renderer and core:**

- Both twins: `crates/rustty-renderer/src/generic.rs` (Metal) and `generic/linux_impl.rs` (OpenGL):
  - `ExportedFrame` present;
  - `LatestFrame`, a mutex single slot that is a no-op for Metal;
  - atomic `presentation_health`;
  - lazy shaders (`Shaders::uninit`);
  - GPU release only on the render thread;
  - `thread_exit`;
  - `draw_frame_locked` (from W0 T1);
  - `set_screen_size` plus `set_viewport`;
  - swap chain of 3.
- `opengl/opengl_impl.rs`: one EGL-owning `init`; delete `present_last_target`, `gpu_resources_released`, `display_realized`; `present_to_default` removed or kept test-only with a PORT NOTE.
- `opengl/frame.rs`: present, `finish`, push, `.redraw`.
- `message.rs`: a `presentation_health` variant.
- `crates/rustty-core/src/renderer_seam.rs`: remove the `must_draw_from_app_thread` and `RedrawSurface` paths.
- `crates/rustty-core/src/{surface,app}.rs`: `report_presentation_health`, `Message::Redraw`, `display_realized`/`display_unrealized`; DPI `@round`; fix the UD104 comment and drop the `thread_enter` in `deinit`.
- `crates/rustty-apprt-api/src/seam.rs:87`.

**Files, GTK:**

- Create `crates/rustty-apprt/src/gtk/class/render_surface.rs` (gtk4-rs `WidgetImpl`: `snapshot`, `size_allocate`, realize/unrealize, the `resize` signal, `gdk4::DmabufTextureBuilder` / `MemoryTexture`; on `DmabufBuildFailed` call `report_presentation_health(Unhealthy)`).
- Create `crates/rustty-apprt/src/gtk/scale.rs`: the pure functions and 7 tests.
- `class/surface.rs:386,1523,1963-1964,2010,3516-3525,4401-4493,6028,6059`.
- `assets/ui/1.2/surface.blp`.
- `application.rs:564-568,2579-2700,3644,5000-5011` (env shrinks to `GDK_DISABLE=vulkan`; the `4eb308831` systemd-ready ordering moves to P7-W8 T3).
- `imgui_widget.rs:477,585-595` (GLES `glsl_version`, init-failure teardown, drop `clear_color`); `assets/ui/1.5/imgui-widget.blp:15`.
- Rewrite 5 Rustty GTK live tests: `gtk_first_light`, `gtk_window_live`, `gtk_inspector_dump`, `gtk_input_focus`, `gtk_glyph_dump`. The visible-render proof becomes an inspection of the `GdkTexture` built in `snapshot`.

- [ ] **Step 1: Measure M2 and M3 before porting.**
  - M2: grep `setError(true)` at the oracle HEAD; if it is gone, plan the error-page restore as **UD356**.
  - M3: port `scale.rs` first, then compute the flip-plus-snap origin at 1.25× for y = 13.6 CSS px, and on rvm headless Weston capture the texture origin with `grim`. If snapping is inverted, open **UD357** and fix the transform order.
- [ ] **Step 2: Fail-first tests.**
  - The 7 `scale` tests.
  - Review Focus 1: `render_surface_falls_back_to_cpu_when_dmabuf_import_fails` on broadway, and `render_surface_presents_dmabuf_on_wayland` on headless Weston with `RUSTTY_REQUIRE_DMABUF`.
  - Review Focus 2: `fractional_scale_cells_land_on_device_pixels` at 1.25×, asserting that a glyph edge column is fully opaque with no half-covered seam row.
  - Miri: the `LatestFrame` slot under concurrent push/take.

  Run → FAIL.

- [ ] **Step 3: Port S2, then S3, in this branch.**
  - Apply the `generic.zig` share to both twins in the same commit, so neither twin drifts.
  - Re-adjudicate LEDGER S119-F3 in this commit (DECISIONS R4); re-anchor S113-F1/F4.
- [ ] **Step 4: Verify.**
  - macOS: `RUSTTY_REQUIRE_METAL=1 cargo test -p rustty-renderer -p rustty-core`, `xcodebuild` RusttyTests, the N291 display-link test.
  - rvm: gtk-feature workspace clippy, `cargo test -p rustty-apprt -p rustty-renderer -p rustty-core`, the broadway and Weston live tests, the imgui GLES open.
  - A two-surfaces-plus-inspector session, checking that the per-thread GL tables don't clash.
- [ ] **Step 5: Commit the S2 and S3 commits, then merge once.**

### Task 4: GTK small (`9fc8d9ebd`, `ef7edc4a2` comments, dcimgui comment)

**Files:** create `crates/rustty-apprt/src/gtk/class/overrides.rs` (from `tab.rs:43` `TabOverrides` + `application.rs:2132` `parse_overrides`, with upstream's comments); `crates/rustty-apprt/build.rs:3-50,199-206,293-296` (citations and quoted module docs).

- [ ] Run the existing override-parse tests → PASS (no behaviour change); G3b on the new file → GREEN.
- [ ] Commit `(P7-W7 T4)`.

---

# `P7-W8` — Font and core small

**Spec:** R2 §§5.2, 5.4, 5.7, §7 R2-B/R2-C.

### Task 1: Font keys and nerd-font tables (`b15fe2eea`, `f3276131f`, `a3f3331d4`, `b60eed139`)

**Files:** `crates/rustty-font*/` (`CodepointKey` and the glyph cache key as packed `u64` with a custom `write_u64` hasher; delete the two stale comments); transcribe the 3 nerd-font arrays into `generated/`; add the `rustty-font`→`rustty-unicode` edge (acyclic: unicode is a leaf; confirm with G7); coretext BMP UTF-16 append (macOS).

- [ ] Port "Constraints lookup table" (all codepoints; R14 Miri cap printed) → FAIL.
- [ ] Port. Run `cargo test -p rustty-font -p rustty-font-shaper` on macOS and rvm → PASS; G7 → GREEN.
- [ ] Take a paired measurement against the old `match`.
- [ ] Commit `(P7-W8 T1)`.

### Task 2: Core small (`5dfb67298`, `6674aa3ba`, `3a766ccf5`, `0fb6d2940`, `a1bf5b5e1`, `5beb94c16`)

**Files:** `crates/rustty-core/src/surface_mouse.rs:57-420` (single switch; restore the stored shape; I-beam on shift without tracking; the sub-block count PORT NOTE goes 13 → 12); `surface.rs:7141-7154`; `crates/rustty-apprt-c/src/lib.rs:2201-2279` (route through `perform_binding_action`; new log texts; UD31 embedded half); preedit `thicken` in both twins (`generic.rs:4891-4893`, `linux_impl.rs:3610-3612`).

- [ ] Re-align `keyToMouseShape`, and add an embedded-API test for "error focusing split" → FAIL.
- [ ] Port. Run → PASS; rerun the menu-shortcut parity harness.
- [ ] Update UD31.
- [ ] Commit `(P7-W8 T2)`.

### Task 3: systemd ready ordering (`4eb308831`) — rvm

**Files:** `crates/rustty-apprt/src/gtk/class/application.rs:564-568,3644,5000-5011` (signals before the style manager; a `systemd_ready` flag gates `RELOADING=1`/`READY=1`).

- [ ] Add a test that `reload_config` before startup completes sends no notify (fake notify socket) → FAIL.
- [ ] Port, then run on rvm → PASS.
- [ ] Commit `(P7-W8 T3)`.

---

# `P7-W9` — macOS Swift (no cargo build needed for Task 1)

**Spec:** R2 §7 R2-G.

### Task 1: Swift fixes (`d4a5ff58b`, `451e224c6`, `310797df1`, `a8b0855b6`, `e8936b896` in date order; `aafacb1cb`, `e347482fb`, `74e47a706`, `1a8f331f1`, `e8fe0a5e6`, `b94b3a056`)

**Files:** `macos/Sources/**` per commit. `e8fe0a5e6` adds `NSAppleEventDescriptor+Extension.swift`: add it to `project.pbxproj` membership. `crates/rustty-input/src/binding.rs:983-984` doc; `.port/tools/windows/golden/list-actions-docs.stdout`.

- [ ] Apply the end state with the token rename. Run `swiftlint lint --strict`, `xcodebuild` RusttyTests, and the menu-shortcut parity harness → PASS.
- [ ] Regenerate the list-actions golden.
- [ ] Commit `(P7-W9 T1)`.

### Task 2: CSI 8 t Swift half (`714fe9b90`) — after P7-W6 T1

- [ ] Port the two Swift files. Run `xcodebuild` → PASS.
- [ ] Commit `(P7-W9 T2)`.

---

# `P7-W10` — Build and packages: Unicode 18, fontconfig 2.18.3, po, themes, version, CI

**Spec:** R4 §§A–H; DECISIONS B12, B14, B15, B16, R6.

### Task 1: fontconfig 2.18.3 (`501e7b5c1`)

**Files:** `crates-sys/fontconfig-sys/` — replace `vendor/` with the 2.18.3 subset; copy the 13 override headers verbatim into the reference layout; delete `vendor/override/include/`.

- `build.rs`:
  - the new include order, with a comment on the `../fc-case` lookup;
  - `fcgenericalias.c`;
  - `HAVE_VSNPRINTF`, `HAVE_USELOCALE` (off Windows), `ENABLE_FREETYPE=1`;
  - `FC_VERSION_*` = the reference literals 2/14/2 (DECISIONS B14);
  - bindgen on `override/fontconfig/fontconfig.h`.
- The KAT goes 21402 → 21803; update every "2.14.2" doc line R4 §A lists.

- [ ] Negative control: build without `ENABLE_FREETYPE=1`. The font-shaper `fc` scan test must FAIL (FreeType scanning dropped), proving the define is load-bearing.
- [ ] Port.
  - macOS: `cargo test -p fontconfig-sys -p rustty-font-shaper` and G-FONTBACKEND.
  - rvm (early): `nm` of the archive shows the IA__ aliases (DECISIONS B15); the font-shaper `fc` tests.
  - rwin: redo the N286 W-6 evidence for GNU (reference and Rustty) and MSVC (refusal) (DECISIONS D3).
- [ ] Add the NEW-FINDINGS latent notes (stale `FC_VERSION`, omitted `fcconffile.c`), and re-run G-EOL on the new vendor files.
- [ ] Commit `(501e7b5c1) (P7-W10 T1)`.

### Task 2: uucode → Unicode 18 (`12542b392`; records for `aef7aaeb8`, `7644e6d62`, `7adbb5160`)

**Files:**

- Harness `.port/phase-0/harness/unicode/`: `build.zig.zon`, `uucode_config.zig` re-copied, `zig-pkg/`, `uucode_oracle.zig`.
- `crates/rustty-unicode/src/{props,grapheme,props_uucode,lib}.rs`: the new variants in Zig order; `BreakState` as a packed u3 with `table_index`/`from_table_index`; the new `is_break`.
- Regenerate `generated/{props,symbols,grapheme,case_folding}_data.rs` and re-pin `checksum.rs` in the same commit.
- `crates/rustty-vaxis/`: re-vendor `ucd/` from `9d55524`; the components split; the grapheme algorithm; `PROVENANCE.md`; `tests/ucd_provenance.rs`.
- `crates/rustty-font-shaper/src/uucode.rs:17-30,338,376,399` (+9 emoji); the case-folding users.

- [ ] **Review Focus 4.** Add `terminal_prints_unicode18_wide_ranges_as_two_cells` (Jurchen U+18E00, Small Seal U+3D000, emoji U+1F6D9 → cursor +2) and `terminal_keeps_incb_linker_other_in_one_cluster` (U+1CF5 sequence) in rustty-vt. Run → FAIL.
- [ ] Port in R4 §B's order. G4-uni → 0 diffs over the full range; run the rustty-vaxis differential.
- [ ] Run `cargo test -p rustty-unicode -p rustty-vaxis -p rustty-vt -p rustty-font-shaper` → PASS. Take a paired `grapheme_break` bench.
- [ ] Commit `(12542b392) (P7-W10 T2)`.

### Task 3: Translations, themes, version string

**Files:** the 10 `po/*.po` catalogs, re-mirrored through the identity transform; themes via `xtask/src/res_themes.rs` (606 → 617, counts at `res_themes.rs:110-115`, `.port/tools/ci.sh:129,1044`, `crates/rustty-extra/themes/README.md:12,80`; `check_theme_corpus.py --write`); `crates/rustty-version/src/git_version.rs:65-67,88-99,145-146` (trim `std.ascii.whitespace`; UD150 fixed upstream, remove its docs row).

- [ ] Each catalog shows 0 diff lines against `xf(oracle@head)`; `msgfmt --check` passes; G-I18N and G-THEMES → GREEN.
- [ ] Add a version test with `\t` in the branch name → FAIL, then port → PASS. `python3 .port/tools/gates/check_ud_docs.py` → PASS.
- [ ] Commit `(P7-W10 T3)`.

### Task 4: CI and release workflows (`380778e3c`, action bumps, EGL link and devShell)

**Files:** `.github/workflows/ci.yml:517-531,607,622,674-686` (remove `test-windows` and the apprt-c GNU build; DECISIONS R6); `release-tip.yml`, `release-tag.yml`, `publish-tag.yml`, `clean-artifacts.yml` (`r2-upload-action` v1.5, `action-gh-release` v3.0.3, `gha-remove-artifacts` v1.8.0, with pinact SHAs); `nix/devShell.nix` gains `libglvnd` if P7-W7 links EGL; N376/N378 refreshed.

- [ ] Update the `xtask` workflow tests for the removed legs (negative control: re-adding a leg fails the parity test). Run actionlint and prettier.
- [ ] Commit `(P7-W10 T4)`.

---

# `P7-W11` — Config, os, termio, shell integration

**Spec:** R3 §§3.3–3.4, 3.9; DECISIONS M1.

### Task 1: Shell integration (`5d6615fc4`, `4dfa44ecd`, `ed7f046ee`, `591ccacbd`, `a4031c8e3`)

**Files:** `crates/rustty-extra/shell-integration/bash/{bash-preexec.sh,rustty.bash}`, the end state through the rename transform. The new identifiers become `__rustty_status` and `__rustty_restore_status`.

- [ ] `diff` against `xf(oracle@head)` shows 0 lines.
- [ ] Add a bash test (bats-style script under `crates/rustty-extra/tests/`): `PROMPT_COMMAND` declared with `declare -ax`, and a failing command, must report exit status 1 through the OSC 133 `D` mark. Run → FAIL before, PASS after.
- [ ] Commit `(P7-W11 T1)`.

### Task 2: Config clone comments, cImport refresh, UD200 status

**Files:** `crates/rustty-config/src/value_types.rs:6024-6047` (correct the PORT NOTE; add the test "RepeatableCommand clone rebuilds the C mirror", asserting the clone's `title` pointer differs from the source's); the cImport comment lines (`config_struct.rs:107`, `rustty-os/src/{desktop,passwd}.rs`, `rustty-termio/src/exec.rs:2086-2089`); `crates/rustty-os/src/open.rs:254` (PORT NOTE rewritten per DECISIONS M1; behaviour unchanged); `docs/ghostty-bug-fixes.md` UD200 row reworded.

- [ ] Run `cargo test -p rustty-config repeatable -p rustty-os open` → PASS; G-UDDOCS → PASS.
- [ ] Commit `(P7-W11 T2)`.

---

# `P7-W12` — Finale (coordinator)

### Task 1: Merge, gates and records

- [ ] Every workstream is merged `--no-ff` into `phase-7-upstream-resync`; the worktrees are removed and their `target/` dirs deleted by literal path.
- [ ] Every row of `R5-upstream-tests.tsv` is checked: 112 added tests present by name, 40 changed bodies re-aligned, 2 removals dropped. Record the list as `.port/phase-7/w0/tests-checklist.txt`.
- [ ] Gates GREEN at `12752b2ac`:
  - G-TESTS (0 unexplained), G-FILES, G-SYMS;
  - G1b (extended map), G3b, G-LOGFILE (re-anchored LOG-*.tsv, new files declared);
  - G-CENSUS, A3, A4, A5, G4 (drivers rebuilt);
  - G-REFGEOM, G-EOL, G-I18N, G-THEMES, G-UDDOCS, G-PARITY, G7;
  - the post-merge gate set.
- [ ] Linux full lane on rvm (`cargo test --workspace`, G-GTKFEAT, G-REL, G-EXAMPLES). Windows on rwin: kitty/windows, mem reclaim, fontconfig GNU, xev. macOS: `xcodebuild` RusttyTests and menu-shortcut parity.
- [ ] Records:
  - `.port/phase-7/README.md` finale table;
  - `ORACLE_SYNC.md` finale note;
  - UD/N/V entries closed or opened (UD352–UD357 as measured);
  - `.port/parity-closure/README.md` updated to the new pin.
- [ ] Merge `phase-7-upstream-resync` into `main` (fast-forward if possible) and delete the phase branch.

### Task 2: The deferred long runs (from `.port/parity-closure/README.md`)

- [ ] Run each row of "Not verified here" at the new pin:
  - full `ci.sh` on macOS and rvm;
  - the G-REL doc-test "can't find crate" failure;
  - continuous G6 and G6c;
  - the Task 41 step-3 re-audit;
  - idle-machine perf and a G8 baseline;
  - V25 (push, then read the CI run);
  - V26 (the Windows full workspace);
  - V24 Nix/Snap, if the maintainer un-defers it.
- [ ] Record any red result as a NEW-FINDINGS entry and fix it in follow-up tasks.

---

## Self-review

**1. Spec coverage.**

- **Workstreams:**
  - R1's workstreams A–E map to P7-W1, P7-W2 (B, plus the C halves from R3), P7-W3 (C), P7-W4 (D, plus R3-A), P7-W5 (E, plus R4-C) and P7-W6 (#25).
  - R2's workstreams A–H map to P7-W0 T1, P7-W8 T1–T3, P7-W7 T1–T4, P7-W6 T2, P7-W9 and the P7-W12 checks.
  - R3's workstreams A–G map to P7-W4 T1, P7-W2, P7-W3 T3, P7-W11, P7-W6 T1, P7-W8 T2 and P7-W0 T4.
  - R4's workstreams A–R map to P7-W10 T1–T4, P7-W5 T2 and P7-W0 T4.
  - R5's advance procedure is P7-W0 T3–T4, and its finale list is P7-W12.
- **Commits:** every non-VOUCHED commit in `R5-commits.tsv` has an owner above, or is recorded as N/A in P7-W0 T4. The four German-translation duplicates are covered by P7-W10 T3's end-state re-mirror.
- **Questions:** all ~40 reviewer questions are settled in `DECISIONS.md`, except `CLAUDE.md` (OPEN).

**2. Placeholder scan.** No "TBD" or "similar to". Tasks cite review sections for the per-commit hunk detail instead of inlining it. That is deliberate, as in the Phase 4–6 plans: the review files and upstream diffs are the source of truth for ~230 commits, and duplicating them here would drift. Every task names its files, its upstream tests (by name or by `R5-upstream-tests.tsv` file), its fail-first step, its verification commands and its commit.

**3. Type consistency.**

- Names used across tasks: `draw_frame_locked` (W0 T1 → W7 T2); `UntouchedPool`/`DynamicUntouchedPool` (W1 T4 → T5); `parse_int` in `rustty-lib` (W5 T1); `ComptimeIntSet` in `rustty-datastruct` (W4 T2); `RUSTTY_REQUIRE_DMABUF` (W7 T1 → T3); `Action::ResizeWindow` (W6 T1 → T2, W9 T2).
- UD ids are reserved in order of measurement (UD352–UD357). A candidate that isn't confirmed releases its id, and the finale records which ids were used.

**4. Review Focus.** The five lines above each have a named test in their owning task: W7 T3 (focus 1 and 2), W1 T5 (focus 3), W10 T2 (focus 4) and W4 T1 (focus 5).
