# Phase 4 — Upstream Resync Master Plan (pin `6ad1fe7d8` → head `12967b68f`)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. In this program the established execution vehicle is the Workflow tool with scripts at `.port/phase-4/wf-*.js` (memory `workflow-scripts-in-port`); each workstream below maps to one or more workflow runs with implementer/verifier/fixer splits per R11. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Absorb the complete upstream range `6ad1fe7d8..12967b68f` (543 commits, 359 non-merge, 381 files, +72,554/−8,106) into rustty with **zero deferrals**, ending with G1b and G3b GREEN again at the new pin and every live bug found by the assessments fixed.

**Architecture:** Ten workstreams in five layers. **`P4-W0` is the live-bug hotfix wave** — the assessments found 8+ bugs *shipping in rustty today* (one security: unconditional CSI 21 t title echo; one UAF: display-callback on freed renderer ctx; two data bugs: kitty range-delete wipes all placements, OSC color reset pins default; plus VS15 underflow, hyperlink-reflow crash, CircBuf shrink corruption, frame-counter perf bug) — all S/M, all independent, all land first with fail-first tests. **`P4-W1` is foundations** (datastruct/lib/os leaves everything else consumes). **`P4-W2`–`P4-W4` are the vt mass** (terminal core → kitty/formatter/input/modes → the 16k-line snapshot subsystem). **`P4-W5` is the C-API surface** (7 atomic bundles, sequenced after their vt-side halves). **`P4-W6`–`P4-W8` are platform/UI/config tiers** (largely parallel with W3–W5). **`P4-W9` is the finale** (gates green, ledgers, census).

**Tech Stack:** Rust 1.93.1 (pinned); Python 3 for gates; cargo +nightly miri for SB/TB lanes; clang from Xcode for ABI probes; xcodebuild for the macOS Swift tests; Fedora aarch64 VM over `ssh rvm` for GTK; oracle at `/Users/gael/projects/ghostty`, working tree AT the pin `12967b68f` (advanced 2026-08-17, evidence in `.port/phase-4/w0/`).

## Global Constraints (binding, inherited + Phase-4-specific)

- **The per-commit spec is the assessments.** `.port/phase-4/assessments/{A-terminal-core,B-snapshot-io,C-capi-lib-wasm,D-terminal-subsystems,E-os-termio-cli-app,F-ui-tiers,G-config-i18n-datastruct,RECONCILIATION}.md`. Every task below cites its assessment rows; the implementer reads the cited upstream hunks (`git -C /Users/gael/projects/ghostty show <hash>` or the working tree, which IS the target) and the cited rustty file:line. A task may not contradict an assessment row without recording why in the task's commit.
- **Port from the RANGE DIFF end state, never commit-by-commit.** Known superseded chains this range (do NOT port intermediates): theme-tilde (f748b17e2→c12173429→end), notification-truncation (2 intermediates), metal-warmup (`threadedWarmup` dissolved by 931a538a3), grid codec (rewritten 3× — goldens final only), render fast-path C (rewritten repeatedly), kitty addImage/addPlacement/evictImage, PageList bulkRunLength/copyRun (88ed6bebf supersedes c249b9de3's shape). Carry EVERY commit's regression tests against the end state.
- **R1 full fidelity:** every comment, every test assertion-for-assertion, every error path. Hex goldens under `src/terminal/snapshot/testdata/` are comptime artifacts: **bit-for-bit**.
- **R2 renames** per token (Ghostty→Rustty, ghostty→rustty); po/pot msgids included (9 `Ghostty` occurrences in the oracle pot); test strings included (`👻Ghostty` → `👻Rustty` in the un-escape round-trip test).
- **R5:** gates are never weakened. New gate arms in this plan (A5 reverse arm, G1b `stream_continuation` lane) only tighten. Every new arm ships a proven FAIL arm (negative control).
- **R7 arithmetic:** every upstream `+|`/`-|`/`@abs`/`std.math.add catch` hunk lands as `saturating_*`/`unsigned_abs`/`checked_*` **plus** `debug_assert!` (the saturate-and-assert pairing, `.port/phase-3/REVIEW_CLASSES.md`).
- **SB/UB discipline** (memories `rust-mut-args-strongly-protected`, `shaper-hook-sb-clean-interleave`): `&mut Page/Row/Cell/Screen` re-read across mutation = UB. Raw-pointer args with narrow `&mut` reconstruction. Named hazards this range (each needs LIFETIMES.tsv rows FIRST and an isolated non-FFI Miri harness where aliasing is restructured): kitty `s: *Screen` params (ImageStorage lives inside Screen); 46276d046 reflow reusing src page memory as dst; snapshot decode-into-Page + decoder objects holding Terminal aliases across C calls; dirty-row iterator vs `render_state_clean`; termio MemoryPool intrusive `xev::WriteRequest` addresses; RefCountedSet iterator holding `[*]Item`.
- **DG5:** C-ABI panic = abort via `extern "C"`, no wrappers. New ABI enums (ModeConfig, snapshot decoder tags, RenderCursor) use the c_int-narrowing convention; `check_closed_enum_abi{,_runtime}.sh` stay green.
- **Zig byte-slices → `Vec<u8>`/`&[u8]`** (config hex escapes are the live instance this range).
- **LTO stays banned; no new `libc` edges** — hand-rolled `extern "C"` per `mem.rs` precedent.
- **Deletion-ordering:** upstream-deleted files (`datastruct/{lru,segmented_pool,array_list_collection}`, `lib/{string,allocator/convenience}`, macOS `AnySortKey/CrossKit/HostingWindow`/iOS files, inspector DetachableHeader) are deleted **with** their consumer migrations, in the same commit as the rewrite that unblocks them. `lru.rs` has zero callers → delete immediately (W1).
- **Escalate, don't silently diverge:** 15c50c1db's inverted macOS `XDG_CACHE_HOME` crash-cacheDir test (comment contradicts code) gets an `.port/phase-4/UPSTREAM_BUGS.md` row and the a82637b53 behavior; do not blind-port the inversion.
- **Bookkeeping greens are not coverage:** G1b `vt_c` "ok" (407 ≤ 417) and `stream_terminal`/`terminal`/`pagelist`/`page` "ok" rows mask ~90 unported upstream tests (surplus port tests pad them). The authoritative new-test checklist is the per-row "tests carried" columns in the assessments, tracked per task below — G1b alone cannot certify Phase 4.
- Commit style: `subsystem: summary (P4-W<n> T<m>)`. Work directly on `main` (session is configured to work in place; keep every commit green: `cargo build --workspace` minimum, full test sweep at each workstream finale).
- **DO NOT run `no_homebrew_gettext_gate.sh`.** Never checkout/pull in the oracle (it is already at the pin).

## Execution order

```
P4-W0  live-bug hotfixes (8 independent fixes, fail-first tests)   ← FIRST
   │
P4-W1  foundations: lib enum values, limited_allocator, wasm_page_pool,
   │   TinyIo, crc32c, fraction, fastprint, os/{stderr,edit}, lru deletion,
   │   simd scalar-utf8 rewrite
   │
   ├──► P4-W2  terminal core (A): stream-continuation bundle, PageList
   │           Builder/allocatePage, reflow end-state, Mask/printSlice,
   │           render fast-path (core side), terminal behavior tail
   │       │
   │       ├──► P4-W3  kitty + formatter + input + modes (D)
   │       ├──► P4-W4  snapshot subsystem (B) — needs W2 Builder + W1
   │       │        │
   │       │        └──► P4-W5  C-API bundles T→R/M/K→S→W→E (C) + A5 arms
   │       │
   ├──► P4-W6  os / termio / cli / app (E)     ) parallel with W3–W5,
   ├──► P4-W7  UI tiers: macOS, GTK, font (F)  ) seams named per task
   └──► P4-W8  config / i18n / repo-meta (G)   )
                                    │
                                    ▼
                    P4-W9  FINALE — G1b/G3b/A5/ci.sh green, ledgers, census
```

---

# `P4-W0` — Live-bug hotfix wave (do this first)

One workflow run, 8 parallel implementer agents + verify. Every fix: fail-first test proven RED, fix, GREEN, commit per fix. Assessment citations in brackets.

### Task 1: CSI 21 t title-report security fix — `38e891e6c` [A §Terminal/stream]
**Files:** Modify `crates/rustty-vt/src/stream_terminal.rs:1431` region; config option + C-API option follow in W5-M/W8 (port the vt-side end state: title report **gated off** unless the option is enabled; the option default is off upstream).
- [ ] Fail-first test: title query with `write_pty` set echoes the title today (the exposure). Port upstream's amended test asserting NO response without opt-in.
- [ ] Port the end-state gate; wire the existing option plumbing shape (bool on Terminal like upstream's, set from config in W8's row — default off NOW).
- [ ] Commit `vt: gate CSI 21 t title report behind opt-in (38e891e6c) (P4-W0 T1)`.

### Task 2: VS15 cursor underflow crash — `33d34cf5c` [A]
**Files:** `crates/rustty-vt/src/terminal.rs:2208` region.
- [ ] Fail-first: VS15 on a wide base under cursor at column 0 → debug panic today.
- [ ] Port fix + upstream test; commit.

### Task 3: hyperlink reflow capacity crash — `d5c7e54ae` [A A-2]
**Files:** `crates/rustty-vt/src/pagelist/resize.rs:1652` region.
- [ ] Fail-first per upstream's repro (ghostty #13522); port the split capacity check + test; commit.

### Task 4: OSC capture cap + grapheme cap DoS — `727b8a02f` [A + RECONCILIATION O1]
**Files:** `crates/rustty-vt/src/osc/mod.rs` (8 MiB `max_allocating_bytes` cap + tests), `crates/rustty-vt/src/terminal.rs`/page grapheme path (`grapheme_max_len = 64` cap). D's two parser call sites follow the API shape.
- [ ] Fail-first both halves; port; commit.

### Task 5: kitty range-delete wipes all placements — `d866fa455` [D]
**Files:** `crates/rustty-vt/src/kitty/graphics_storage.rs:673` (`||` → the upstream `&&` semantics) + the amended assertions of the EXISTING test (gate-invisible class).
- [ ] Fail-first; port; commit.

### Task 6: OSC color reset pins default — `7cd2f65f5` [D]
**Files:** `crates/rustty-vt/src/color.rs:687` (reset → `None`, not copy of current default) + stream_terminal regression test half.
- [ ] Fail-first; port; commit.

### Task 7: renderer frame-counter reset + IOSurface callback UAF — `9e6e2ea96`, `4b4a5b241` [E]
**Files:** `crates/rustty-renderer/src/...` frame-counter reset (one line + test); IOSurfaceLayer release/Drop clears the display callback (R9 callback-ctx UAF).
- [ ] Fail-first where testable (frame counter unit test; UAF gets a Drop-clears-callback assertion); port both; commit.

### Task 8: CircBuf shrink corruption + keyboard.sequence_set reorder + hotkeys deinit leak + SKILL.md typo — [G, E P4-E17(c), F fd47b15cd, RECONCILIATION O15]
**Files:** `crates/rustty-datastruct/src/circ_buf.rs` (+3 dropped zig tests, 24 vs 21 measured); `crates/rustty-core/src/surface.rs:~6743` (assign `sequence_set` AFTER fallible `encodeKey`); `crates/rustty-apprt/src/.../hotkeys.rs:265` (deinit, not clear); `.agents/skills/writing-commit-messages/SKILL.md` double negative.
- [ ] Fail-first each; port; one commit per fix.

**W0 finale:** `cargo test --workspace --no-fail-fast` green; push nothing (local main commits fine).

---

# `P4-W1` — Foundations

### Task 1: `e77c2612e` lib enum stable values FIRST [C bundle T precondition]
`crates/rustty-lib/src/enum.rs:98` still has old value semantics. Port the stable-value semantics + tests. Everything in bundle T depends on this.

### Task 2: `src/datastruct/limited_allocator.zig` (+115) and `src/datastruct/wasm_page_pool.zig` [C #9/#49, G, D dependency]
New crates/rustty-datastruct modules with full tests. WasmPagePool feeds PageList raw-ptr pages — R9 SAFETY tags + LIFETIMES rows. LimitedAllocator unblocks D's `433b16bcb` (apc glyf caps) and kitty png caps.

### Task 3: `src/lib/TinyIo.zig` (1,522) + `src/crc32c.zig` (slicing-by-16 + interleaved streams end state `36d8e3f77`+`eb09bf829`) [B]
Full ports with every upstream test. TinyIo's module export rider (`82df79ec8` one-liner, O5) rides here.

### Task 4: `src/terminal/fraction.zig` + `src/fastprint.zig` [A A-4, D coupling]
fraction.parse REJECTS exponent/hex-float syntax `f64::parse` accepts — port exactly; D's `946422dbc` call sites follow in W3.

### Task 5: `src/os/stderr.zig` + `src/os/edit.zig` [E]
POSIX raw-write stderr seam (joint E/C decision recorded: port the POSIX arm); `edit.getConfigEditCommand` (VISUAL/EDITOR + `default_editor`) — App.Message.OpenConfig payload consumers land in W6/W7.

### Task 6: deletions available now + simd scalar-utf8 rewrite [G]
Delete `crates/rustty-datastruct/src/lru.rs` (zero callers, verified) + its `pub mod`; port the scalar UTF-8 decoder rewrite in `crates/rustty-simd/src/vt.rs` (existing differential test is the regression check).

**W1 finale:** workspace build + tests green; commit per task.

---

# `P4-W2` — Terminal core (assessment A; the vt spine)

### Task 1: stream-continuation bundle [A A-3 — atomic with D's apc half + C's io surface later]
NEW `src/terminal/stream_continuation.zig` (606 lines) + ground-state/APC-exit rework (`68beeeb3f`,`70e41e96d`,`a69a591af`,`d7bb4b863`,`afb351f83`,`b53728241`). `ApcEnd{terminated}` signature ripples: port the vt+apc halves together (D's apc drop=4 tests land here), termio `.unknown` arm in W6, C API in W5-S. All 16 new stream.zig tests + 8 stream_continuation tests + 2 apc Glossary tests. Inspector's `68beeeb3f` init-API slice (F) must land in the same build or `cargo build` breaks — sequence in one commit series.

### Task 2: PageList Builder/allocatePage end state [A A-1 — snapshot enabler]
`fc1bd06a1`+`35db32078`+`457c5a0a6` end state (`allocatePage`, `Builder`, `cancel`→`deinit`). 7 upstream tests. `PageAllocation` holds `*mut PageList` — LIFETIMES rows + raw-ptr shape. Plus `b61fd5fbb` RefCountedSet iterator (raw `[*]Item` — SAFETY tag) + `d44baa914` main.zig re-exports.

### Task 3: reflow end-state chain [A A-2 — biggest single mass, top Miri hazard]
`d5c7e54ae`(done W0)+`4a88cc594`+`c5ca2db1b`+`c249b9de3`+`179161c08`+`46276d046`+`0fb3565c7`+`d4e446c48`+`ec5b36961`+`88ed6bebf` as ONE end-state port of PageList reflow/bulk-copy (bulkRunLength/copyRun final shape; eql_any Mask extension; src-page-memory-recycling). 12 upstream tests; do NOT double-port the moved "Pin row movement clamps across mixed-width pages" test. Isolated Miri harness `.port/phase-4/mirihar-reflow-recycle` for the 46276d046 aliasing before merging; run vt Miri lane after.

### Task 4: terminal behavior/perf tail [A §Terminal/stream + §render-state]
`bfd40c84b` (CSI 2 K wrap state), `f973bd53b` (overline printAttributes + amended dcs test w/ D), `c992658b2` (base64 — rustty-simd ready), `74efadb44` (XTGETTCAP source side — **run `check_acyclic.sh` first**, new terminal→terminfo edge; coordinate W6 terminfo Task), `4371871bc` signature half (infallible `setLimit`, atomic with W3 kitty), `51a4311ef` inlining cleanup, `5b70f208b`, `ac4dc1205`, `c092b2bcf` (w/ D modes), `946422dbc` call sites (fraction from W1), `3d9b2b483` native arms (grapheme capacity projection, bitmap search_start, printSlice check-hoist), `e3056658d`/`74a233b54`/`0d37f2d34` core-side render-state work, `0e8b7bea6` packed-cell layout core half, `1fdbb8c91` `build_options.glyph_protocol` gate sites + `src/terminal/build_options.zig` Features machinery (O4).

**W2 finale:** full vt test suite + vt Miri lane; G1b `stream` RED row goes green; commit series.

---

# `P4-W3` — Kitty, formatter, input, modes (assessment D)

### Task 1: kitty end-state bundle [D clusters 1–2 + E renderer side + A setLimit]
Port `graphics_{storage,exec,image}.zig` END STATE as one unit (~2,100 lines, ~23 new tests): pin-lifecycle + pending payloads + security hardening (TOCTOU open-first via `fcntl(F_GETPATH)`/`/proc/self/fd`; `/tmpX` prefix-boundary; shm range validation; png LimitedAllocator cap from W1; saturating geometry per R7). `s: *Screen` params as raw pointers (follow `delete()`'s existing shape) + Miri harness for sweep paths. Keep collect-keys-first for remove-while-iterating. `af2faa311` before `ec04900ab` within the series. E's renderer isPending gating + Termio try-drop ride at the end. G1b kitty drop=11 → green.

### Task 2: formatter end-state bundle [D cluster 3 + A A-4]
Pin-map RLE redesign (PinMap in formatter.zig + StringMap Vec<Pin>→PinMap.Map), tabstops-before-screen + cursor-home (`e523cf810`), pending-wrap cursor restore (`997a2aff2` — **also delete** the first-pass extra.screen emission or output doubles), html trailing-newline, buffer-assembled VT/CSS emission. G1b formatter drop=2 → green.

### Task 3: input bundle [D cluster 5 + F Swift flag]
Release-fallback guard (`key_encode.rs:281`, drop=3), `8cfbaf545`+`b67f8ef51` un-escape + round-trip pair (`👻Rustty`), three new Binding.Action variants (prompt_window_title, set_window_title, move_tab_to_new_window — C-ABI halves in W5-E/W7), `f4309055f` Swift `0x7F` half recorded for W7.

### Task 4: modes DECECM + configurable defaults [D cluster 6]
`get_report` 117/permanently-reset arm (drop=1), `cfc19e805` setDefault/default_configurable — feeds W5-M `GHOSTTY_TERMINAL_OPT_MODE_DEFAULT`.

### Task 5: color/dcs/hyperlink/KV tail [D cluster 7]
`f719af00c` dense KV table (replaces HashMap<u8,u32>, arena removal), hyperlink zero-len free guards (snapshot decode relies on them), remaining dcs/color rows.

**W3 finale:** vt tests + Miri; G1b apc/kitty/formatter/modes/input_key_encode rows green.

---

# `P4-W4` — Snapshot subsystem (assessment B; the 16k-line new mass)

Port `src/terminal/snapshot/*` bottom-up along its module DAG, END STATE only (no BLAKE3, decode-vs-decodeExact, Decoder=init/ready/next), as `crates/rustty-vt/src/snapshot/`:

### Task 1: codec scaffold [B C1] — framing, envelope, checkpoint, continuation, io, record, main + `snapshot.ksy` + AGENTS.md (rename-faithful), hex goldens bit-for-bit.
### Task 2: page/style/hyperlink records [B C2] — transactional rollback releases style/hyperlink refs (carry every release-on-error test); decode-into-Page is the search-SB shape → raw-ptr discipline + LIFETIMES rows + Miri harness `.port/phase-4/mirihar-snapshot-decode`.
### Task 3: grid codec end state [B C3] — 8-byte cells, blank elision, variable width, vectorized enc/dec, grapheme suffix batching; final goldens only.
### Task 4: screen/terminal/history + robustness + kitty hunks [B C4] + fixture.zig test infra.
### Task 5: benchmarks + synthetic [B C8 + G riders]: TerminalSnapshot/TerminalFormatter/TerminalResize benches, synthetic Styled, bench action registrations (G-BENCH both-directions parity gate must stay green — it will name the new actions).
### Task 6: G1b lane [R5 tighten-only]: add `snapshot` module rows (and `stream_continuation` from W2) to `check_test_counts.py`'s module list with a negative control (comment out one rust test → RED).

**W4 finale:** snapshot tests green incl. goldens; Miri lane on the new modules; G3b vt missing count drops by the snapshot share (~600+ of 921).

---

# `P4-W5` — C-API surface (assessment C; 7 atomic bundles, in order)

### Task 1: Bundle T — ABI type metadata [C]: types.zig registry 45→135, types.schema.json + verify script (register in ci.sh as a PROGRESS gate so it is read), packed.zig GhosttyPackedCell (couples W2's cell layout), types.h C++/underlying-type fixes, self-contained device/point headers, sgr.zig CUnion field_renames (O3), Doxyfile rider (O12).
### Task 2: Bundle R — render fast-path rework end state: chunked updates, dirty-row iteration (`render_state_clean`/`_next_dirty` — raw-ptr iterator seam), structured cursor read, **REMOVE `rustty_render_state_colors_get`**.
### Task 3: Bundle M — mode defaults (**REMOVES `mode_get`/`mode_set`**), title_report option (W0 T1's C half), XTGETTCAP `terminfo_name` (bounds-checked memcpy, 960c2cca5 class), unknown-sequence C API, cursor_at_prompt (A's `cursorIsAtPrompt`), `-Dvt-features` C guards.
### Task 4: Bundle K — kitty C surface [rides W3 T1].
### Task 5: Bundle S — snapshot/io/streaming C API: snapshot.h (525) + io.h (107) + c/snapshot.zig (1,533) + c/io.zig, 15 new exports, `terminal_continuation_*`, `vt_write_until_ground`, `formatter_format` streaming writer, example/c-vt-snapshot. Decoder-aliasing SB discipline.
### Task 6: Bundle W — wasm allocator break, ATOMIC: wasm_page_pool + allocator/wasm + c_abi + wasm.h rewrite + DELETE convenience.rs & the 12 stale per-type exports + rewrite the 3 wasm examples + port test/wasm-alloc.mjs + wasm release log gating; sweep rustty-wasm crate + `ws14_webgl_browser_gate.sh` in the same series.
### Task 7: Bundle E — embed header enums (positional, oracle order) + A3 sha re-pin; delete `rustty-lib/string.rs` after `new_window.rs` → `strip_prefix` (with W6).
### Task 8: A5 arms [R5 tighten]: re-run arm (d) (expect the 22 missing exports gone), ADD reverse arm (d′) failing on **stale** rustty exports the oracle removed (negative control: keep colors_get → RED), re-float the intersection floor upward.

**W5 finale:** `check_rustty_vt_headers.sh` fully green incl. new arms; layout/enum arms green; examples build; G3b vt-c 187 → 0.

---

# `P4-W6` — os / termio / cli / app (assessment E)

Tasks (each = its assessment cluster, with the coupling notes binding):
1. **ssh-terminfo cache bundle** (4 commits, interlocked signatures; G1b cli_ssh_cache_diskcache → green).
2. **Desktop-notification truncation end state** with F's apprt Message ctor (+4 apprt_api_surface tests → G1b green); delete the old termio truncation site.
3. **Renderer cell-storage rewrite** + delete `array_list_collection.rs` (same commit); G1b renderer_cell → green.
4. **Metal warmup chain end state** (App.create detached warmup threads; font Discover.warmup with F).
5. **Kitty renderer side** — riders on W3 T1.
6. **Surface write-lifecycle + misc**: OSC52/long-key buffer transfer, pwd stackFallback, readonly-queueIo test, tempdir close(CloseMode)+tests, `77537c806` OSC8 fail-closed (atomic: os/open Kind::Osc8 guard + Surface switch + apprt-api Kind C-ABI with W5-E/W7; macOS policy consumer in W7).
7. **Crash/sentry restructure**: owned env-map snapshot, init-thread dir resolution, static dirs, Transport alloc; UPSTREAM_BUGS.md row for the XDG_CACHE_HOME inversion (port a82637b53 behavior).
8. **termio infra**: MemoryPool replacing SegmentedPool (LIFETIMES rows FIRST, raw-ptr handoff, Miri harness) + delete `segmented_pool.rs` same commit; apcEnd `.unknown` arm (W2 T1 rider); Stream init options.
9. **new_tab CLI** + edit_config rewrite (os/edit from W1) + list-themes g/G aliases + LineIterator regression assertions + `1d6bc6829` strip_prefix migration (bundle with W5 T7 deletion).
10. **terminfo overline + XTGETTCAP source** (regenerate encode golden + version hash; cycle_check first — coordinate with W2 T4).

---

# `P4-W7` — UI tiers (assessment F)

1. **macOS Swift wave** bundles B1–B4 + palette/IME/CommandLineOpenFileFilter + the ~15 fix commits + DELETE AnySortKey/CrossKit/HostingWindow/iOS files (CrossKit deletion atomic with OSColor→NSColor sweep) + carry UntrustedURLTests/CommandLineOpenFileFilterTests/CommandPaletteTests; `f4309055f` 0x7F fix at `SurfaceView_AppKit.swift:1468`. Gate: `xcodebuild test`.
2. **apprt core seam**: action/ipc/surface additions (export_terminal_io, set_window_title, move_tab_to_new_window, OpenConfig payload, osc8 kind, ipc NewTab CValue 8→16) — every enum/struct add lands WITH its W5-E header KAT in one motion.
3. **GTK wave** B5–B9 (+new-tab D-Bus with W6 T9, split DnD + flicker, bell relocation, native blur pair, window-title renaming, open-config-in-window, teardown fixes — verify gtk4-rs WeakRef semantics before copying `1dbc8ca30`, note in-code either way) + `applyCommandOverrides` 7-assertion test (G1b-invisible — carried explicitly). Gate: `ssh rvm` build+tests.
4. **font**: shaper grapheme-candidate rewrite (`ca8868a29` test needs W2 RenderState scaffolding; G1b font_shaper_harfbuzz → green), Apple Color Emoji exact lookup, CoreText warmup/null/OOM, display-name fallback; `a177ba90a` display-link error fix **deleting** the cvdisplaylink.rs:148-156 PORT NOTE (Phase-3 metal/buffer precedent).
5. **inspector**: DetachableHeader deletion, terminal-IO copy/export, stream-continuation init API slice (sequenced inside W2 T1's commit series).
6. **build/pkg/packaging**: `-Dvt-features` cargo-feature plumbing (with W5), wasm stack 128KB, wuffs-sys bump to v0.4.0-alpha.10, flatpak/snap pins, xtask/translations.rs N_ keyword (`df23bef0e`).

---

# `P4-W8` — config / i18n / repo meta (assessment G)

1. **config hex-escape byte preservation** (`29b82dd80`; zig-byteslice rule; + the new test G1b masks).
2. **theme tilde/HOME end state** (Config-level expandHome + Theme.finalize + Config.finalize + WorkingDirectory refactor + rewritten test; NO intermediates).
3. **config tail**: drag-handle, link-osc8 (pairs W6 T6), edit.zig paths, open_config default, cmd-translation loop (input `translated()` with W3 T3), 4 doc rewrites.
4. **po/ wave**: pot 79→253 msgids + all 33 catalogs mirrored to END STATE + NEW da.po + README N_ note; R2 renames per token; mechanical mirror measured by file-level diff, not read line-by-line (stated filter per assessment).
5. **.github mirrors**: delete ISSUE_TEMPLATE/{config.yml,preapproved.md}, triage checkbox, paths-filter 4.0.3.
6. **themes re-vendor** (O7): corpus → `20260810-152212-0173c3c`, re-run G-THEMES (expect count change; update the gate's expected number with evidence, tighten-only).
7. **typos.toml/PACKAGING.md riders** with their owning bundles.

---

# `P4-W9` — Finale

- [ ] `cargo build --workspace` + `cargo test --workspace --no-fail-fast` green (macOS) and `ssh rvm` (GTK lane) green.
- [ ] Full `.port/tools/ci.sh` run: G1b GREEN (all 10 RED rows + new snapshot/stream_continuation lanes), G3b GREEN (1,230 → 0 missing), A5 all arms, A3 re-pinned, G-BENCH, G-LZ4, acyclicity, closed-enum, leaks lanes.
- [ ] Miri: vt lane + the three new harnesses (`mirihar-reflow-recycle`, `mirihar-snapshot-decode`, termio pool).
- [ ] R9 census: `count-unsafe.sh` snapshot + UNSOUND_SAFE.md pass; LIFETIMES.tsv rows landed for every new raw-ptr seam.
- [ ] Ledgers: `.port/phase-2/MAIN_PARITY.md` + `DEFERRALS.md` rows struck; `.port/phase-4/UPSTREAM_BUGS.md` (XDG inversion) recorded; assessments' "tests carried" columns reconciled.
- [ ] `.port/ORACLE_SYNC.md`: append the finale note (gates re-greened at `12967b68f`).
- [ ] Final commit series on `main`.

## Self-review notes

- Spec coverage: every ABSENT/PARTIAL row in the 7 assessments + all 15 reconciliation orphans (O1→W0 T4, O3→W5 T1, O4→W2 T4, O5→W1 T3, O7→W8 T6, O12→W5 T1, O15→W0 T8; N-A orphans need no task) maps to a task above. The 60 N-A rows need none by definition; the 10 PARTIAL rows are inside W6 T6/T9, W7 T6, W8 T3.
- G1b RED rows → tasks: formatter(W3 T2), apc(W2 T1), stream(W2 T1), kitty(W3 T1), modes(W3 T4), input_key_encode(W3 T3), cli_ssh_cache_diskcache(W6 T1), font_shaper_harfbuzz(W7 T4), renderer_cell(W6 T3), apprt_api_surface(W6 T2).
- Type-consistency: signature forks are named at their source task (ApcEnd W2 T1; setLimit W2 T4/W3 T1; Builder W2 T2; PinMap W3 T2; CValue 8→16 W7 T2/W5-E).
