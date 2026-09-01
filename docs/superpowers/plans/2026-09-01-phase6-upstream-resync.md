# Phase 6 — Upstream Resync Master Plan (pin `88f57ee66` → head `20abdb50a`)

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:subagent-driven-development. Execution vehicle: parallel Agent-tool workers, each in its own hand-made git worktree (`git worktree add ../rustty-wt/<ws> -b phase-6/<ws> phase-6-upstream-resync`; no remote exists), merged back onto `phase-6-upstream-resync` by the coordinator. Each worker writes its own assess+port record at `.port/phase-6/assessments/<WS>.md`. Steps use checkbox syntax.

**Goal:** Absorb `88f57ee66..20abdb50a` (121 commits, 74 non-merge, 75 files, +6,261/−1,269) with zero deferrals, ending with G3b GREEN (51 missing today) and G1b GREEN **with its file list extended** to the two new oracle files (23 unseen tests today), both platforms.

**Architecture:** Six workstreams in three layers. The range is dominated by **one new subsystem** — the whole-terminal search orchestration extracted from the search thread (`search/terminal.zig`) and its C API (`c/search.zig` + `include/ghostty/vt/search.h` + `example/c-vt-search`) — plus a native-freestanding libghostty-vt build story, a middle-click/copy-on-select config rework (#12600), two renderer DisplayLink commits, a macOS AppleScript key-synthesis wave, and a po/i18n wave (Serbian ×2 NEW; fr/ru/uk re-mirrors). Layer 1 = W1/W2/W4/W5/W6 in parallel; layer 2 = W3 (after W2); layer 3 = W7 finale.

**Constraints:** `docs/superpowers/plans/2026-08-17-phase4-upstream-resync.md` Global Constraints, binding verbatim (range-diff END STATE, R1/R2/R5/R7/R9/DG5, SB raw-ptr discipline, deletion-with-consumer, escalate upstream bugs, bookkeeping greens are not coverage).

## Range-diff traps (do NOT port intermediates)

- `7a15898bc` "use same ResetZoomAccessoryView" is **REVERTED** by `b6ac6e1d4`; `0a9f47cae` then updates only a comment. END STATE of `TerminalWindow.swift` = pin + the note.
- `10b7158e8` created `.github/workflows/freestanding.yml`; `fc7bfa60b` **DELETED** it and folded the job into `test.yml`; `5e02d0014` made it required. END STATE: no freestanding.yml.
- `166d2fe34` Sparkle 2.9.4 then `c18198325` 2.9.6 + SPM pin — port the 2.9.6 end state only.
- `f9202919f` introduced `GhosttySearchOptions`; `76d9fcefe` **DROPS** it in favour of `ghostty_search_set` — port the end-state header/API only (no options struct).
- `dfccdb2d4` + `e80ce2ed4` ("Fix build error") are one change — port together.
- `search/Thread.zig` loses 1 test that MOVES to `search/terminal.zig` (6 tests there). Net search tests: +5. Do not keep a duplicate.

## SB/UB hazards

1. `search/terminal.zig` holds a `*Terminal` (and through it `*Screen`/`*PageList`) alias across screen/pagelist mutations and the cross-thread hand-off in `Thread.zig`. The vt search subsystem's prior SB cluster (memory `search-screen-sb-ub-finding`, fixed in `9e3fe1c`) came from translating raw `*Screen` params to `&mut Screen` — keep RAW pointers in the new orchestration type; `.port/tools/miri-run.sh rustty-vt search` must stay UB=0.
2. `c/search.zig` `f0c918fc4` "allow freeing a search and its terminal in any order" — an ownership/refcount contract between two C handles; port the exact end-state ownership (the test names it), never an Rc-ification that changes who frees what.
3. `ref_counted_set.zig` `5f5495826` living-count fix — an accounting bug; port with its regression test and confirm the assertion fires on the pre-fix code (negative control).

## Workstreams

### `P6-W1` — vt-core (rustty-vt, rustty-lib, rustty-vt-c/lib.rs freestanding half)

- [ ] `5f5495826` RefCountedSet.addWithId living over-count (+1 test)
- [ ] `eb722cb26` Screen/Terminal: mark previous row dirty when clearing its spacer head (+1 test)
- [ ] `028af92ce` PageList: clarify page allocator fallback (comment; G3b vt)
- [ ] `a5423592c` caller allocation on native freestanding: `PageList.pageAllocator(alloc)`, `lib/allocator.default` failing on freestanding, allocator.h doc (G3b lib 2)
- [ ] `3376153a4` native freestanding build: `build_options.kittyGraphics` (any-arch freestanding), `lib_vt` std_options page-size/stack-tracing/debug_machinery gating — rustty analog + honest record of what `target_os = "none"` can and cannot build
- [ ] `ee8095d37` snapshot: stack fallback for record scratch
- [ ] `090fca451` kitty graphics_image: validate POSIX shared memory names (+2 tests)

### `P6-W2` — search vt-side (rustty-vt/src/search)

- [ ] `149c9f562` extract whole-terminal search orchestration: NEW `search/terminal.rs` (6 tests), `Thread.zig` −560/+ (loses 1 test), `screen.zig` +23, `search.zig` re-export
- [ ] `f0c918fc4` search/terminal.zig half (deinit ordering)
- [ ] G1b: add `search/terminal.zig` to the `search` row in `check_test_counts.py` (coordinator)

### `P6-W3` — search C API (after W2; rustty-vt-c, include/rustty/vt, example)

- [ ] `32601cd79` + `f9202919f` + `f0c918fc4` + `76d9fcefe` + `674abd8a1` END STATE: `c/search.zig` (17 tests), `c/selection.zig` (+12), `c/types.zig`, `c/terminal.zig`, `c/main.zig`, `lib_vt.zig` exports (+ feature gate), `include/ghostty/vt/{search.h NEW, selection.h, types.h}`, `vt.h`, `example/c-vt-search`
- [ ] A5/CABI manifests, `oracle-ghostty-h.sha256`, discriminant/layout checks, G-VTFEAT expectations re-derived; symbol-manifest floor
- [ ] G1b: add `c/search.zig` to the vt-c scope (coordinator)

### `P6-W4` — core tier (rustty-renderer, rustty-core, rustty-config, rustty bin, rustty-apprt gtk, rustty-os)

- [ ] `97f57edcc` renderer: vsync unfocused surfaces while dirty; `6688aa072` park DisplayLink while idle (END STATE of generic.zig)
- [ ] `dfccdb2d4`+`e80ce2ed4` #12600: `middle-click-action` gains `clipboard-paste` (keeps `primary-paste`), `copy-on-select` gains `both|none|primary` with true/false in the compatibility handler; Surface.zig arms; config docs (G3b config 6, core 1)
- [ ] `caf48a41e` main: inverted allow_stack_tracing
- [ ] `6cd684d5d` gtk: stale pointers to property bindings; `3e2c0fa2d`+`860cfb1d7` gtk-xft-dpi −1
- [ ] `7aa476714` os/i18n_locales: sr + sr@latin

### `P6-W5` — macOS (macos/)

- [ ] `6229d4eb6` AppleScript send key for non-control keys (sdef, ScriptKeyEventCommand, Input, Surface, SurfaceView_AppKit, String+Extension; tests: ScriptKeyEventCommandTests, NEW KeyEventSynthesizeTests)
- [ ] `777929a8f` review windows when closing multiple tabs (AppDelegate, TerminalController, NSAlert+Extension)
- [ ] `2de159611`+`1c3a4a831`+`07abbd1e7` normalize action working directory paths (NewTerminalIntent, ServiceProvider, URL+Extension; NEW URLTests)
- [ ] `572fd5837` default BellFeatures = config's
- [ ] `0a9f47cae`/`b6ac6e1d4`/`7a15898bc` TerminalWindow end state; `851751a11` Ventura deprecated toolbar button
- [ ] `c18198325` Sparkle 2.9.6 + SPM pin (pbxproj, Package.resolved, release workflows SPARKLE_VERSION)
- [ ] xcodebuild test lane green

### `P6-W6` — platform tail

- [ ] po: fr/ru/uk re-mirror; NEW sr.po + sr@latin.po (Phase-4 byte-mirror method, msgfmt gate; catalog headers = oracle)
- [ ] `28b5bf905` Noto emoji fonts (binary byte-copy into `crates/rustty-font/res/`; any golden that pins glyph metrics re-derived)
- [ ] `094d175ef` iTerm2 themes tarball `20260824-153547-75c93ee` (xtask res_themes vendored corpus + G-THEMES)
- [ ] `.github`: milestone-action 3.2; freestanding CI lane analog (with W1's finding); CODEOWNERS sync (`74e0bfdf2`, `ec3e384d2`); VOUCHED.td N/A (rustty has none — say so)

### `P6-W7` — finale (coordinator)

- [ ] G1b GREEN with extended file list; G3b GREEN; A5/A3/A4/G7/G-VTFEAT/G-THEMES/G-I18N; Miri search lane; macOS full + xcodebuild; ledgers + `ORACLE_SYNC.md` finale note; worktrees swept; merge to `main`.

## Self-review

Every range file maps to a workstream (README table). G3b arrival map: vt 38 → W1/W2 (PageList/search/graphics_image/ref_counted_set/Terminal comments), config 6 → W4, vt-c 4 → W3, lib 2 → W1, core 1 → W4. Unseen tests: 17 → W3, 6 → W2.
