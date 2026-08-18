# Phase 4 assessment — area C: the C API / lib / wasm tier (`src/lib/**`, `src/lib_vt.zig`, `include/**`, `src/terminal/c/**`, `example/**`, wasm alloc infra, `src/global.zig`, `src/main_ghostty.zig`)

Range: `6ad1fe7d8` (Phase-3 pin, 2026-07-29) → `12967b68f` (`origin/main` = the NEW pin, 2026-08-17).
Oracle working tree is checked out AT the new pin, so file reads are target-state reads.
Rustty side: `crates/rustty-vt-c/`, `crates/rustty-lib/`, `crates/rustty-datastruct/`,
`include/rustty*`, `example/`, `crates/rustty-global/`, `crates/rustty/src/main_rustty.rs`,
`crates/rustty-core/src/logging.rs` (the `main_ghostty.zig` logFn seam).
Assessment only — **nothing was implemented**. Written per the Phase-4 binding rules
(port from the RANGE DIFF / END STATE, never commit-by-commit; "already ported" needs
symbol + `file:line` + a gate that fails on regression).

**52 non-merge upstream commits** touch this area's paths. All 52 are individually
verdicted below. **Headline: this area has effectively received NONE of the range yet** —
0 PORTED, 0 PARTIAL, **47 ABSENT**, 5 N-A. The Phase-3 backlog (constructor break,
desktop-notification/progress effects, begin/end update, VT_PROCESSING_ERROR,
scrollback limits...) **was** remediated — rustty's `Option_` now ends at
`ProgressReport = 30` (`crates/rustty-vt-c/src/terminal.rs:1267`) and `TerminalData` at
`ScrollbackMaxLines = 35` (`terminal.rs:2255`), matching the OLD pin exactly. Everything
this range added on top is missing.

---

## 0. Coverage statement — exhaustive vs sampled

| Work | Coverage |
|---|---|
| Upstream commit enumeration for the area paths | **EXHAUSTIVE** (`git log --no-merges 6ad1fe7d8..12967b68f -- <13 area paths>` → 52 commits; per-commit `--stat` restricted to area paths captured for all 52) |
| Exported C symbol set, oracle head vs rustty, **both directions** | **EXHAUSTIVE** (mechanical: oracle `@export(..., .{ .name = "ghostty_..." })` in `src/lib_vt.zig` at the new pin = 194 names, vs `grep -rhoE 'extern "C" fn (rustty\|ghostty)_...' crates/rustty-vt-c/src` = 185; `comm` both ways — §3) |
| Curated header family `include/rustty/vt/**` vs oracle `include/ghostty/vt/**` at the new pin | **EXHAUSTIVE at file granularity** (full `diff` of a token-renamed oracle snapshot per file; per-file changed-line counts in §4; the two headers with no rustty counterpart identified). Individual hunks read for the small headers; `terminal.h` (453 diff lines) / `render.h` (149) / `types.h` (114) / `wasm.h` (158) hunks attributed to commits via the per-commit stats, not re-read line-by-line |
| `include/ghostty.h` (embed umbrella) commits | **EXHAUSTIVE** — all 9 commits' hunks read line-by-line; each symbol grepped in `include/rustty-embed.h` (all 0 hits) |
| `src/global.zig`, `src/main_ghostty.zig`, `src/lib/enum.zig`, `src/lib/string.zig` commits | **EXHAUSTIVE** — hunks read line-by-line; rustty counterparts (`crates/rustty-apprt/src/lib.rs` init, `crates/rustty-core/src/logging.rs`, `crates/rustty-lib/src/{enum,string}.rs`) read at the exact seam |
| One-liners / small C-wrapper commits (`68beeeb3f`, `4371871bc`, `b5e86a428`, `b53728241`, `579db4189`, `d61920d80`, `f36854345`, `d524a0fe6`, `51992ab01`, `d930c74c4`, kitty trio) | **EXHAUSTIVE** — full hunks read |
| The 5 giant commits (`d7bb4b863` 3,633 in-area lines; `9673a22b0` ~1,240 in-area; `0e8b7bea6` 767; `a8e9b413f` 473; `9be6c2ea2` 385) plus `cfc19e805`/`1fdbb8c91`/`e3056658d`/`b4079f00c`/`74a233b54`/`846d24e12`/`a69a591af`/`6b990de5b`/`38e891e6c`/`74efadb44`/`bdb566068` | **SAMPLED**: `--stat`, all `lib_vt.zig` export-list hunks (read in full), enum-tag/type-registry additions (extracted mechanically from the oracle tree at head — the END STATE), and representative body/test hunks. **Not every body line was read.** This is safe for the verdicts because absence in rustty is established by exhaustive symbol greps that returned zero hits (§2 evidence column) — no verdict depends on an unread hunk's internals. The unread internals DO matter for the eventual port and are flagged per-cluster in §5 |
| Rustty-side absence greps | **EXHAUSTIVE per feature**: `snapshot`, `xtgettcap`, `title_report`, `terminfo_name`, `unknown_sequence`, `mode_default`, `until_ground`, `cursor_at_prompt`, `continuation` (vt-c), `pending` (kitty C), `wasm_page_pool`, `limited_allocator`, `packed`/`c_abi` (rustty-lib), `formatter_format`, plus the embed-h enum tokens — commands and hits recorded in §2 |
| Oracle `TerminalOption`/`TerminalData` enums, type-registry census (`initStruct`/`initEnum`/`initUnion`/`initOpaque`/`initAlias`/`initPacked` counts) | **EXHAUSTIVE** (read from the target tree) |
| Gate source reading (A5 all arms incl. (d), A3 sha-pin, G1b module map, ws14 wasm gate artifact path) | **EXHAUSTIVE by source reading**; **no gate was executed** (no build run for this assessment) |
| `example/**` | Directory-set comparison (oracle 36 dirs vs rustty 32) + grep of the stale wasm-alloc API in the 3 wasm examples. Per-file diffs of example sources **NOT** redone this phase (Phase-3 did; the only in-range example changes ride on absent features and are attributed per-commit) |
| `src/lib/TinyIo.zig`, `src/lib/compat/**` | **NOT body-assessed here** — TinyIo is **area B's** per the fan-out; only its area-C wiring (lib_vt exports, wrapper `io_impl`, snapshot/formatter writer plumbing) is verdicted |

---

## 1. Summary with numbers

**Commit verdicts.** 52 commits: **0 PORTED, 0 PARTIAL, 47 ABSENT, 5 N-A.**

**Exports (C symbols).** Oracle at the new pin exports **194** (`src/lib_vt.zig` `@export`
list, incl. wasm-only). rustty-vt-c exports **185** live `extern "C" fn rustty_*`.

| | count |
|---|---|
| **oracle exports rustty is MISSING** (all added in this range) | **22** (§3a: 11 snapshot_decoder_* + 3 snapshot_encode* + 3 terminal_continuation_* + terminal_vt_write_until_ground + formatter_format + render_state_clean + render_state_row_iterator_next_dirty + wasm_alloc/wasm_free/wasm_take_opaque) |
| **rustty exports the oracle REMOVED in this range** (must be deleted) | **15** (§3b: terminal_mode_get/set ← `cfc19e805`; render_state_colors_get ← `b4079f00c`; the 10 per-type wasm_alloc/free_* + the 2 wasm_*_sgr_attribute ← `a8e9b413f`) |
| `ghostty_*` aliases in rustty-vt-c (R2) | **0** ✅ |

**Consequence for gates:** A5 arm (d) (`check_rustty_vt_headers.sh`) reads the oracle at
`origin/main` and its documented-residual allowlist is **EMPTY**, so **A5 is now RED with
exactly those 22 names**. Arm (d) is one-directional (oracle→rustty): the **15 stale
rustty exports are invisible to every current gate** (§6, gap #1).

**Headers.** Token-renamed oracle-vs-rustty diff, changed lines per file:
`terminal.h` **453**, `wasm.h` **158**, `render.h` **149**, `types.h` **114**,
`screen.h` 34, `sgr.h` 25, `formatter.h` 25, `kitty_graphics.h` 12, `vt.h` 13,
`osc.h` 3, `device.h` 2, `point.h` 2 — plus **`snapshot.h` (525 lines) and `io.h`
(107 lines) have NO rustty counterpart at all**. `unicode.h`'s 4-line diff is the
pre-existing Phase-3 whitespace note, NOT in-range. All other headers byte-identical.

**Terminal option/data tags.** rustty `Option_` ends at 30, oracle at **37**
(missing: `continuation_max_bytes=31, title_report=32, mode_default=33, mode=34,
unknown_sequence=35, unknown_max_bytes=36, terminfo_name=37`). rustty `TerminalData`
ends at 35, oracle at **39** (missing: `continuation_max_bytes=36, mode=37,
vt_ground=38, cursor_at_prompt=39`).

**Type registry.** rustty `types.rs`: **45** entries, structs only. Oracle `types.zig`:
**135** entries across six kinds (45 `initStruct` + 59 `initEnum` + 19 `initOpaque` +
5 `initUnion` + 6 `initAlias` + 1 `initPacked`) **plus** a checked-in JSON Schema
(`types.schema.json`, 348→503 lines) and a CI verifier (`types-schema-verify.py`) —
none of which exist in rustty.

**Upstream DELETIONS rustty has not performed** (README rule: delete or justify):
`src/lib/string.zig` (`1d6bc6829`) — rustty still ships
`crates/rustty-lib/src/string.rs` (31 lines, `cut_prefix` + test);
`src/lib/allocator/convenience.zig` (`a8e9b413f`) — rustty still ships
`crates/rustty-lib/src/allocator/convenience.rs` (120 lines) and the 12 wasm
convenience exports built on it.

**G1b honesty note (measured).** `vt_c` reads **ok (zig 407 ≤ rust 417)** in
`w0/g1b-after-at-12967b68f.txt`, but the zig side of that row is
`glob(src/terminal/c/*.zig)` (`check_test_counts.py:466`) and **already includes** the
range's new tests (snapshot.zig 6, io.zig 11, plus the new terminal/render/kitty tests).
The green is **bookkeeping**: rustty's pre-existing surplus (+63 extra `#[test]`s vs the
old-pin baseline) masks ~60+ unported range tests. Same for `lib_vt` (0 zig) and
`main_c`. **No area-C module is RED in G1b, and that must not be read as coverage** —
this is exactly the memory-`derived-gate-green-can-be-bookkeeping` failure shape.
`apprt_api_surface drop=4` is NOT this area's (see §5 couplings).

---

## 2. Per-commit verdict table

Ordered oldest→newest. "Evidence" cites the rustty location that proves the verdict;
a grep listed with **0 hits** was run over `crates/` + `include/` unless narrowed.
Gate column: A5 = `check_rustty_vt_headers.sh` (arms a/a2/a3/b/d/c/e);
A3 = `check_rustty_h_abi.sh`; G1b = `check_test_counts.py`; "none" = no current gate
can fail on it.

| # | upstream | what changed (in-area) | verdict | evidence | gate | cost |
|---|---|---|---|---|---|---|
| 1 | `ad96613a8` | `ghostty.h`: `ghostty_action_export_terminal_io_s` struct + `GHOSTTY_ACTION_EXPORT_TERMINAL_IO` tag + union arm (inspector IO export) | **ABSENT** | `grep EXPORT_TERMINAL_IO include/rustty-embed.h` → 0 | A3 (sha-pin now mismatches, §6) | S (header) — impl is area F (inspector/apprt) |
| 2 | `e77c2612e` | `lib/enum.zig`: `Enum()` values are ALWAYS the key index (`values_raw[to] = from` unconditionally); Zig tag int sized from `keys.len` incl. trailing holes; test renamed `abi by removing a key`→`stable values when removing a key`, `@intFromEnum(T.d)` expectation 2→3; NEW test `zig backing integer includes trailing holes` | **ABSENT** | `crates/rustty-lib/src/enum.rs:98` still `Target::C => from` / else `to`; `:110` still `int_fitting_range(to - 1)`; no `stable values`/`trailing holes` test (grep 0) | G1b won't see it (module aggregate); **none** binds | S |
| 3 | `68beeeb3f` | C wrapper slice of stream-continuation tracking: `TerminalWrapper.stream = Stream.init(.{ .allocator, .handler })` (options-struct ctor replacing `.initAlloc`) | **ABSENT** | `crates/rustty-vt-c/src/terminal.rs:1116` still `Stream::init_alloc(...)` | none | S — rides area D's `stream_continuation.zig`; port as part of cluster C4-snap |
| 4 | `15c50c1db` | `global.zig` call-site slice: crash init gated on `build_options.sentry`, takes an owned `environ.createMap(alloc)` snapshot | **ABSENT** | `crates/rustty-apprt/src/lib.rs:312` still `rustty_crash::init()` — no environ map, no sentry build gate | none | S at this seam — crash-side is area E |
| 5 | `d7bb4b863` | **THE snapshot C API**: NEW `src/terminal/c/snapshot.zig` (1,253) + `io.zig` (594) + `include/ghostty/vt/{snapshot.h 525, io.h 107}` + `terminal.h` +123 + `types.h` +11 + `terminal.zig` +795 (continuation write/buf/alloc) + `result.zig` +2 result codes + 15 new exports + `example/c-vt-snapshot` + types.zig rows | **ABSENT** | 15 exports in §3a all missing (comm); `include/rustty/vt/{snapshot.h,io.h}` do not exist; `ls example` → no `c-vt-snapshot`; `grep -rn rustty_snapshot crates/rustty-vt-c/src/lib.rs` → only the unrelated `tracked_grid_ref_snapshot` | **A5(d) RED** (15 of the 22 names) | **XL** — blocked on areas A/B/D: `src/terminal/snapshot/*` (~16k), `crc32c.zig`, `stream_continuation.zig`, TinyIo (B) |
| 6 | `6760c6482` | kitty C surface: `imageGetTyped` `.data_ptr` returns `.no_value` while payload pending (`image.data.bytes() orelse`), `.data_len` via `image.data.len()`; kitty_graphics.h doc contracts for pending payloads; NEW test `image_get exposes pending metadata without a data pointer` | **ABSENT** | `crates/rustty-vt-c/src/kitty_graphics.rs:512-545` still transcribes `image.data.ptr` / `.len` (pre-change body); `grep pending kitty_graphics.rs` → comments only | G1b masked (vt_c aggregate) | M — blocked on area D `kitty/graphics_image` `data: Data` enum |
| 7 | `25b2d8a38` | `ghostty.h`: `GHOSTTY_ACTION_MOVE_TAB_TO_NEW_WINDOW` | **ABSENT** | grep 0 in `include/rustty-embed.h` | A3 | S (header) — impl area F |
| 8 | `e5840bb9b` | kitty C surface tests for hardened placement geometry (+35, tests) | **ABSENT** | no counterpart tests in `kitty_graphics.rs` (grep `harden\|geometry` 0 in tests) | G1b masked | S — rides area D |
| 9 | `590d669c4` | NEW `src/datastruct/limited_allocator.zig` (124) — allocation-budget wrapper used to cap kitty PNG decode | **ABSENT** | `ls crates/rustty-datastruct/src` → no `limited_allocator.rs` | none | M — consumer is area D kitty PNG path |
| 10 | `402b9227d` | kitty C surface: reclaim pruned placements (+18, incl. test) | **ABSENT** | as #6/#8 | G1b masked | S — rides area D |
| 11 | `38e891e6c` | opt-in title reports: `TerminalOption.title_report = 32` (bool), handler wiring, test `title report requires explicit opt in` (CSI 21t answers only when opted in) + terminal.h +12 | **ABSENT** | `grep -i title_report crates/rustty-vt-c include/rustty/vt/terminal.h` → 0; `Option_` ends at 30 (`terminal.rs:1267`) | none (A5(e) would catch a mis-numbered port once typedef'd) | S — needs area D handler flag |
| 12 | `4371871bc` | C wrapper slice: `kitty_images.setLimit(...)` becomes infallible (drops `catch return .out_of_memory`) | **ABSENT** | `crates/rustty-vt-c/src/terminal.rs:1550` still handles a `Result` from `set_limit` (`graphics_storage.rs:228` still fallible) | none | S — rides area D's evict-without-scratch change |
| 13 | `77537c806` | `ghostty.h`: `GHOSTTY_ACTION_OPEN_URL_KIND_OSC8` | **ABSENT** | grep 0 | A3 | S — impl area F |
| 14 | `b5e86a428` | kitty C: 1-line — release evicted placement pins | **ABSENT** | as #6 | none | S — rides area D |
| 15 | `1d6bc6829` | **DELETES `src/lib/string.zig`** (cutPrefix; callers move to `std.mem.cutPrefix`) | **ABSENT (deletion not performed)** | `crates/rustty-lib/src/string.rs` still exists (31 lines, `cut_prefix` at `:8` + test `:19`) | none | S — consumers: `cli/new_window.zig` (area G), gtk (area F); Rust callers should use `strip_prefix` |
| 16 | `cfc19e805` | **removes `terminal_mode_get`/`terminal_mode_set` exports**; adds `ModeConfig` sized struct (types.zig row), `Option.mode_default = 33` + `.mode = 34` (set current AND/OR default), `TerminalData.mode = 37`; terminal.h ±89; RIS/DECSTR reset-to-configured-default semantics; ~7 tests | **ABSENT** | rustty still exports both (`comm` §3b); no `ModeConfig`/`mode_default` anywhere (grep 0) | **none for the stale exports** (§6 gap #1); A5(c/e) once typedef'd | M — needs area A/D `Modes` configurable-defaults storage |
| 17 | `219173ab3` | snapshot: remove BLAKE3 digests (supersedes part of `d7bb4b863` — **port the END STATE, digest-free**) | **ABSENT** (rides #5) | as #5 | A5(d) | – (folded into #5) |
| 18 | `034506f14` | `ghostty.h`: `GHOSTTY_IPC_ACTION_NEW_TAB` | **ABSENT** | grep 0 | A3 | S — impl area F/G (+new-tab cli) |
| 19 | `b53728241` | lib_vt.zig +1: `pub const UnknownSequence = terminal.UnknownSequence` (module surface for #21) | **ABSENT** | `grep UnknownSequence crates/rustty-vt-c crates/rustty-vt/src/lib.rs` → 0 | none | S — rides #21 |
| 20 | `74efadb44` | XTGETTCAP answering: `Option.terminfo_name = 37` (`lib.String` into fixed `terminfo_name_buf: [max_terminfo_name_bytes]u8` on the wrapper, len-checked `@memcpy`), handler answers XTGETTCAP; terminal.h +16; test `set terminfo_name option` | **ABSENT** | `grep -i xtgettcap\|terminfo_name crates/rustty-vt-c include/rustty/vt` → 0 | none | M — handler side is area D. **Carry the bounds-check** (same overflow class as Phase-3's `960c2cca5` note) |
| 21 | `6b990de5b` | unknown-sequence C API: `UnknownSequence.C` + `UnknownStringSequence` sized structs, `Option.unknown_sequence = 35` (callback) + `unknown_max_bytes = 36`, 2 types.zig rows + negative/field asserts, terminal.h +107, `c-vt-effects` example +42 | **ABSENT** | grep 0 (`UNKNOWN_SEQUENCE`, `UnknownSequence`) | A5(c/e) once typedef'd; none today | M — parser plumb is area D (`b53728241`'s apc reporting) |
| 22 | `f36854345` | lib_vt.zig: release panic handler = single-raw-write message + `@trap()`, no stack unwind (`tinyPanicImpl` via `os/stderr.zig`) | **N-A** — Zig root-module `pub const panic` machinery; rustty's C-ABI panic policy is already print-message + SIGABRT with no unwind, proven by a live gate | DG5: `crates/rustty-apprt-c/tests/panic_policy.rs::exported_c_abi_panic_aborts_host` (subprocess SIGABRT) | DG5 | – |
| 23 | `579db4189` | `c/sys.zig` `logStderr`: single raw `stderr.write` per line (bounded buffer + piecewise fallback), avoiding `std.debug.lockStderr`; doc rewritten | **ABSENT** | `crates/rustty-vt-c/src/sys.rs:456-458` still documents+uses `eprint!` (Rust stderr lock), old doc "Uses std.debug.lockStderrWriter" still at `:419` | none | S — wants area E's `os/stderr.zig` port (`rustty-os` raw-write seam); R1 comment update mandatory |
| 24 | `82df79ec8` | replace `Io.Threaded` with TinyIo; in-area slice: lib_vt wiring, wrapper `io_impl` type, snapshot.zig −27 | **N-A (this area)** — TinyIo itself is **area B's** per the fan-out; rustty has no `std.Io` seam at all (`TerminalWrapper` `terminal.rs:93-98` carries no `io_impl`, Phase-3 precedent `da04b65d4`). The wrapper/snapshot hunks are part of #5's END STATE | wrapper verified io-free | – | the snapshot port (#5) must decide the Rust reader/writer seam; coordinate with area B |
| 25 | `d524a0fe6` | lib_vt.zig: guard release builds against `std.debug` Io machinery | **N-A** — Zig `std.Options.debug_io` machinery; no Rust analogue | – | – | – |
| 26 | `a15dddae2` | TinyIo CI test fix | **N-A (this area)** — `src/lib/TinyIo.zig` only; area B owns TinyIo | – | – | – |
| 27 | `a82637b53` | `global.zig` slice: environ-map ownership moves to `crash.init` (freed on init thread); comment updated | **ABSENT** | same seam as #4 — `rustty_crash::init()` takes nothing | none | S — rides #4; crash-side area E |
| 28 | `3225e9ebb` | `main_ghostty.zig`: NEW `macosLogger(scope)` — per-scope `os_log` cached in an atomic, cmpxchg race handling, release-on-lose; logFn uses it | **ABSENT** | `crates/rustty-core/src/logging.rs:70-77` still creates+releases a logger per call, with the superseded comment "This is slow to do on every operation" | none | S — seam is `rustty-core::logging` (shared with area F's bin); OnceLock-per-scope map is the Rust shape |
| 29 | `d6248a32d` | `ghostty.h` header doc rewritten: "internal embedder API, a.k.a. libghostty-internal ... external embedders should use libghostty-vt" | **ABSENT** | `include/rustty-embed.h:5-11` still carries the OLD paragraph ("isn't meant to be a general purpose embedding API (yet)...") | A3 sha-pin | S (R1 comment fidelity) |
| 30 | `0a183c923` | `ghostty.h`: `ghostty_action_open_config_e` (OS_OPEN / NEW_WINDOW) + union arm | **ABSENT** | grep `OPEN_CONFIG_OS_OPEN` → 0 | A3 | S — impl area F |
| 31 | `8b7c57c75` | `ghostty.h`: `GHOSTTY_PROMPT_TITLE_WINDOW` + `GHOSTTY_ACTION_SET_WINDOW_TITLE` | **ABSENT** | grep 0 | A3 | S — impl area F |
| 32 | `a69a591af` | ground-state APIs: export `terminal_vt_write_until_ground` (+212 terminal.zig: parse-to-ground, partial-consume return), `TerminalData.vt_ground = 38`, terminal.h ±91 | **ABSENT** | §3a comm (export missing); `TerminalData` ends at 35; `grep until_ground crates/rustty-vt-c` → 0 (rustty-vt's `consume_until_ground` at `stream.rs:825` is the PARSER primitive, not this API) | **A5(d) RED** (1 name) | M — parser primitive exists in area D's stream; C API + tests are this area's |
| 33 | `bdb566068` | semantic prompt: `TerminalData.cursor_at_prompt = 39` (`t.cursorIsAtPrompt()`, OSC 133) + terminal.h +11 + test `get cursor_at_prompt`; ALSO `ghostty.h` +`ghostty_surface_is_at_prompt` | **ABSENT** (vt part) / ghostty.h line **N-A** — reverted by `c80c37362` in-range | `grep cursor_at_prompt` → 0; `TerminalData` ends at 35 | none | S — needs area A `Terminal.cursorIsAtPrompt` + area E/F surface plumbing |
| 34 | `c80c37362` | removes `ghostty_surface_is_at_prompt` from `ghostty.h` (+ Surface/embedded, not my paths) | **N-A** — reverting half of the `bdb566068` pair; rustty never added the symbol, END STATE already holds | grep `is_at_prompt include/rustty-embed.h` → 0 ✅ | A3 | – |
| 35 | `51992ab01` | `device.h`/`point.h`: +`#include` lines to make each self-contained | **ABSENT** | renamed-diff: 2 changed lines each in `include/rustty/vt/{device,point}.h` | A5(a) can't catch (umbrella still compiles) | XS |
| 36 | `e4ec4f0f9` | `types.h`: fix enum underlying-type detection (±36: `GHOSTTY_ENUM_TYPED` C/C++ dance) | **ABSENT** | part of the 114-line `types.h` diff | A5(a2) compiles both C11+C++17 — would catch a BOTCHED port, not the absence | S |
| 37 | `309440e07` | types.zig: include ALL public structs in type JSON (+66) | **ABSENT** | rustty `types.rs` = 45 entries (counted) vs oracle 135 | none (arm (c) is registry-driven, blind to missing rows — Phase-3 §5 finding still true) | M — rides #45 |
| 38 | `d930c74c4` | `types.h`: sized-init macro valid in C++ (+9) | **ABSENT** | part of `types.h` diff | A5(a2) | XS |
| 39 | `e3056658d` | `c/render.zig` rewrite (+211/−86): faster render-state updates & C reads (chunked row updates, cached spans) | **ABSENT** | rustty `render.rs` unchanged since old pin (no counterpart hunks; `render.h` 149-line diff) | G1b masked | L — END STATE with #40/#43/#44; area A `render.zig` coupling |
| 40 | `74a233b54` | wasm-targeted render-state fast reads: `render.h` +13, `screen.h` +17, `cell.zig` +6, `style.zig` ±124 (style read API), `render.zig` +81 | **ABSENT** | `screen.h` 34-line diff; no counterpart | G1b masked | M — same cluster as #39 |
| 41 | `d61920d80` | lib_vt.zig: wasm release builds ship `log_level = .err` + noop logFn (~200KB savings); Debug keeps `os/wasm/log.zig` | **ABSENT** | no wasm log gating in rustty-vt-c (`lib.rs` routes all logging to `sys::log_fn`; no `cfg(target_arch="wasm32")` log arm) | none | S — verify against area F's `rustty-wasm` console-log path |
| 42 | `1fdbb8c91` | `-Dvt-features`: lib_vt.zig restructured (±315) so kitty-graphics/etc compile out; `build_options.kitty_graphics` guards + `SkipZigTest` arms in c files | **ABSENT** | `crates/rustty-vt-c/Cargo.toml` features = `simd` + identity only; no per-feature gating | none | M — Rust shape: cargo features on rustty-vt/-vt-c; touches every kitty test |
| 43 | `b4079f00c` | structured cursor read: NEW `GhosttyRenderCursor` (types.zig +14) via `render_state_get`; **removes export `render_state_colors_get`**; render.h ±79; `c-vt-render` example −37 | **ABSENT** | rustty still exports `rustty_render_state_colors_get` (§3b); no `RenderCursor` (grep 0) | **none for the stale export** | M — cluster with #39/#40/#44 |
| 44 | `0d37f2d34` | dirty-row iteration: NEW exports `render_state_clean` + `render_state_row_iterator_next_dirty` (+153 render.zig, render.h +54, example −27) | **ABSENT** | §3a comm (2 names) | **A5(d) RED** | M — same cluster |
| 45 | `9673a22b0` | ABI metadata expansion: types.zig ±1,058 (enums/unions/aliases/opaques in JSON, kinds, `checkGhosttyHEnum` interplay), `lib/union.zig` +94 (`TaggedUnion` C-metadata), types.h ±47, osc.h +3, `wasm-vt/index.html` reads metadata | **ABSENT** | rustty `types.rs` structs-only (45 rows); `crates/rustty-lib/src/union.rs` (250 lines) models the OLD factory | none | **L** — with #37/#46/#47; rustty-lib descriptor models must grow the same observables |
| 46 | `c75559589` | ABI manifest schema: NEW `types.schema.json` (348) + `types-schema-verify.py` (114) + types.h +3 | **ABSENT** | no schema/verifier anywhere in rustty (find → 0) | none | M — decide: port the schema + a Rust/CI verifier step |
| 47 | `0e8b7bea6` | packed cell layout: NEW `src/lib/packed.zig` (290: comptime packed-struct field extraction), types.zig +300 (`initPacked`, `GhosttyPackedCell`), schema +155, render.h +9, screen.h ±19, types.h +7 | **ABSENT** | `ls crates/rustty-lib/src` → no `packed.rs`; grep `PackedCell` → 0 | none | **L** — exposes `page.Cell`'s packed layout: area A coupling + R7/R9 care |
| 48 | `a8e9b413f` | **wasm alloc API break**: DELETES `lib/allocator/convenience.zig` and the 12 per-type exports; NEW `lib/allocator/wasm.zig` (98) with `wasm_alloc`/`wasm_free` (byte-based) + `wasm_take_opaque`; NEW `lib/c_abi.zig` (28); `wasm.h` rewritten (±168), `sgr.h` −25, types.h/schema rows; 3 wasm example index.html rewrites; NEW `test/wasm-alloc.mjs` (136) | **ABSENT** | rustty still exports the 12 old names (§3b) + keeps `convenience.rs`; `wasm.h` diff = 158 lines; no `c_abi.rs`; no `test/` dir in rustty at all | **A5(d) RED** (3 new names); nothing catches the 12 stale | **M** — atomic with #49/#41 + examples + ws14 (§5) |
| 49 | `492c26067` | NEW `src/datastruct/wasm_page_pool.zig` (157): page-recycling wasm allocator backing `wasm.zig` | **ABSENT** | no `wasm_page_pool.rs` in rustty-datastruct (ls) | none | M — pairs with #48 |
| 50 | `924c8a90d` | streaming formatter: NEW export `formatter_format` (formatter output through a `GhosttyWriter`), formatter.zig +111, formatter.h +25, example ±39 | **ABSENT** | rustty formatter exports = terminal_new/format_buf/format_alloc/free only (grep §2); §3a comm | **A5(d) RED** (1 name) | M — needs #5's `io.zig` writer adapter |
| 51 | `846d24e12` | buffer the writer adapter used by streaming C APIs (io.zig ±189, formatter/snapshot/terminal call sites) — supersedes parts of #5/#50; **port the buffered END STATE** | **ABSENT** (rides #5/#50) | as #5 | A5(d) | – |
| 52 | `9be6c2ea2` | snapshot decode continuation retention: `snapshot.h` ±60, snapshot.zig +313 (decoder option + tests), terminal.zig ±37 | **ABSENT** (rides #5) | as #5 | A5(d) | – |

---

## 3. Symbol-level delta (mechanical, both directions)

Method identical to Phase 3 §3: oracle `@export(&c.X, .{ .name = "ghostty_..." })` lines
in `src/lib_vt.zig` **at the new pin** (194 names), token-renamed, `comm`'d against
`grep -rhoE 'extern "C" fn (rustty|ghostty)_[A-Za-z0-9_]+' crates/rustty-vt-c/src`
(185 names, all `rustty_*` — zero `ghostty_` aliases).

### 3a. Oracle → rustty missing: **22** (every one added in this range)

```
formatter_format                        924c8a90d
render_state_clean                      0d37f2d34
render_state_row_iterator_next_dirty    0d37f2d34
snapshot_decoder_decode                 d7bb4b863
snapshot_decoder_free                   d7bb4b863
snapshot_decoder_get                    d7bb4b863
snapshot_decoder_get_multi              d7bb4b863
snapshot_decoder_new                    d7bb4b863
snapshot_decoder_new_buf                d7bb4b863
snapshot_decoder_next                   d7bb4b863
snapshot_decoder_ready                  d7bb4b863
snapshot_decoder_set                    d7bb4b863   (decode options incl. 9be6c2ea2 retention)
snapshot_encode                         d7bb4b863
snapshot_encode_alloc                   d7bb4b863
snapshot_encode_buf                     d7bb4b863
terminal_continuation_alloc             d7bb4b863
terminal_continuation_buf               d7bb4b863
terminal_continuation_write             d7bb4b863
terminal_vt_write_until_ground          a69a591af
wasm_alloc                              a8e9b413f   (wasm-only)
wasm_free                               a8e9b413f   (wasm-only)
wasm_take_opaque                        a8e9b413f   (wasm-only)
```

**A5 arm (d) is RED with exactly this set today** (allowlist empty; oracle read at
`origin/main` = the new pin). That is the intended tracker — do NOT allowlist these
(R5); port them.

### 3b. rustty → oracle stale (oracle REMOVED them in-range; rustty must delete): **15**

```
terminal_mode_get             removed by cfc19e805 (mode defaults rework)
terminal_mode_set             removed by cfc19e805
render_state_colors_get       removed by b4079f00c (structured cursor read)
wasm_alloc_opaque             KEPT upstream — NOT stale (listed for clarity: survives in wasm.zig)
```
Wait — precisely: `wasm_alloc_opaque`/`wasm_free_opaque` **survive** upstream
(`allocator/wasm.zig`). The stale 15 are:

```
terminal_mode_get, terminal_mode_set                       cfc19e805
render_state_colors_get                                    b4079f00c
wasm_alloc_u8, wasm_free_u8                                a8e9b413f
wasm_alloc_u8_array, wasm_free_u8_array                    a8e9b413f
wasm_alloc_u16_array, wasm_free_u16_array                  a8e9b413f
wasm_alloc_usize, wasm_free_usize                          a8e9b413f
wasm_alloc_sgr_attribute, wasm_free_sgr_attribute          a8e9b413f
```

**No gate fails on these** — A5(d) is one-directional. §6 gap #1 proposes the reverse
arm. Deleting them is part of the R1 END-STATE port, and each deletion is ABI-breaking
for the wasm examples until those are rewritten in the same commit (§5 bundle W).

---

## 4. Headers — per-file changed-line counts (token-renamed oracle @ new pin vs rustty)

| header | Δ lines | owner commits | note |
|---|---|---|---|
| `snapshot.h` | **MISSING** (525) | d7bb4b863, 219173ab3, 9be6c2ea2 | port digest-free END STATE |
| `io.h` | **MISSING** (107) | d7bb4b863, 846d24e12 | `GhosttyWriter` ABI |
| `terminal.h` | 453 | d7bb4b863, cfc19e805, 6b990de5b, a69a591af, 74efadb44, 38e891e6c, bdb566068 | options 31–37, data 36–39, continuation + until-ground + unknown-seq docs |
| `wasm.h` | 158 | a8e9b413f | full rewrite to byte-based API |
| `render.h` | 149 | 0d37f2d34, b4079f00c, 74a233b54, 0e8b7bea6 | |
| `types.h` | 114 | 9673a22b0, e4ec4f0f9, d930c74c4, c75559589, 0e8b7bea6, d7bb4b863 | |
| `screen.h` | 34 | 74a233b54, 0e8b7bea6 | |
| `sgr.h` | 25 | a8e9b413f | wasm convenience decls REMOVED |
| `formatter.h` | 25 | 924c8a90d | |
| `vt.h` | 13 | d7bb4b863 (+4: snapshot/io includes) + Phase-3 recorded divergences | |
| `kitty_graphics.h` | 12 | 6760c6482 | pending-payload doc contracts |
| `osc.h` | 3 | 9673a22b0 | |
| `device.h`, `point.h` | 2+2 | 51992ab01 | self-contained includes |
| `unicode.h` | 4 | — | pre-existing Phase-3 whitespace note; NOT in-range |
| all others (17 files + key/ + mouse/) | 0 | — | byte-identical ✅ |

`include/rustty-embed.h`: 9 in-range `ghostty.h` commits, net 8 hunks to port
(the `is_at_prompt` pair cancels). `include/rustty.h` (cbindgen) regenerates from the
crate — arm (a3) enforces freshness once the crate changes land.

---

## 5. Atomic bundles, cross-area couplings, UB/SB hazards

### Bundles (must land together or be worse than absent)

- **Bundle S (snapshot/io/streaming)** — `68beeeb3f` + `d7bb4b863` + `219173ab3` +
  `846d24e12` + `924c8a90d` + `9be6c2ea2` + `a69a591af` (+N-A riders `82df79ec8`,
  `a15dddae2`, `d524a0fe6`). Port the END STATE: digest-free (`219173ab3`), buffered
  writer (`846d24e12`), TinyIo-or-equivalent behind the C wrapper. Blocked on:
  **area A/B** `src/terminal/snapshot/*` (~16k lines incl. `.ksy` + hex goldens),
  `src/crc32c.zig`, **area B** TinyIo, **area D** `stream_continuation.zig`.
  `snapshot.h` + `io.h` + 15 exports + `result.zig` codes + `example/c-vt-snapshot`
  (as `build.sh` per EXAMPLES_PORT convention) + types.zig rows are ONE commit-set;
  a header/impl skew here is the sret crash class (memory `embed-abi-byvalue-sret-gate`).
- **Bundle W (wasm alloc)** — `492c26067` + `a8e9b413f` + `test/wasm-alloc.mjs` +
  deletion of `convenience.rs` + the 12 stale exports + `wasm.h`/`sgr.h` +
  rewrites of `example/{wasm-vt,wasm-sgr,wasm-key-encode}/index.html` (they call
  `rustty_wasm_alloc_u8_array` etc. today — verified `wasm-vt/index.html:221`) +
  rustty-only `example/wasm-web-canvas` for consistency. **Coupling:** area F's
  `rustty-wasm`/`rustty-web-canvas` crates and `ws14_webgl_browser_gate.{sh,mjs}`
  (drives `target/.../rustty_wasm.wasm`) — sweep them for the old names in the same
  change. Also the Phase-3 `Opaque` cbindgen pin: `wasm_take_opaque` returns the
  opaque slot — update `crates/rustty-vt-c/cbindgen.toml` `after_includes` in the
  same commit.
- **Bundle T (type metadata)** — `e77c2612e` + `9673a22b0` + `c75559589` +
  `309440e07` + `e4ec4f0f9` + `d930c74c4` + `0e8b7bea6`. `e77c2612e` FIRST (enum
  values stable) — the JSON's enum values depend on it. The schema verifier
  (`types-schema-verify.py`) should enter CI with the schema or it's an unread
  artifact. `0e8b7bea6` needs `rustty-lib/packed.rs` + the `page.Cell` packed-layout
  exposure (**area A** owns the cell struct; R7 narrowing discipline on the packed
  fields).
- **Bundle R (render fast-path)** — `e3056658d` + `74a233b54` + `0d37f2d34` +
  `b4079f00c`: same functions rewritten repeatedly; port END STATE; the
  `render_state_colors_get` DELETION and the `GhosttyRenderCursor` ADDITION and the
  `c-vt-render` example rewrite are one ABI unit. **Area A** render.zig coupling.
- **Bundle M (mode defaults)** — `cfc19e805` alone is atomic: export deletions +
  `ModeConfig` + header + types rows + tests. Needs area A/D configurable-default
  mode storage.
- **Bundle K (kitty C surface)** — `6760c6482` + `e5840bb9b` + `402b9227d` +
  `b5e86a428` + `4371871bc` + `590d669c4` ride **area D**'s graphics_storage churn
  (pending payloads, pin release, scratch-free evict, limited png alloc). The C hunks
  are small; port them WITH the area-D landing, not before (they reference
  `image.data.bytes()`/infallible `setLimit` that don't exist yet).
- **Bundle E (embed header)** — the 7 surviving `ghostty.h` hunks ride **area F**
  apprt action/IPC enums (`ExportTerminalIO`, `MoveTabToNewWindow`, `OpenUrl OSC8`,
  `IPC NEW_TAB`, `OpenConfig`, window-title pair) — header + `rustty-apprt-api` +
  trampolines together, appended in oracle ORDER (enum tags are positional; A3/A4
  guard layout). Also re-pin `.port/tools/oracle-ghostty-h.sha256` — **A3 mismatches
  the advanced oracle header TODAY** (9 in-range commits changed `ghostty.h`).

### Cross-area couplings (named areas)

| item | blocked on / shared with | area |
|---|---|---|
| snapshot codec, crc32c, `.ksy`+goldens | `src/terminal/snapshot/*` | A/B |
| TinyIo + compat | `src/lib/TinyIo.zig` | **B** (explicit fan-out split) |
| stream continuation, `Stream.init` options | `src/terminal/stream_continuation.zig`, stream handler | D |
| kitty pending payloads / eviction / limited_allocator consumer | kitty graphics_storage | D |
| XTGETTCAP, title_report, unknown sequences, mode defaults handler side | TerminalStream handler | D |
| `cursorIsAtPrompt` | `Terminal` OSC-133 state | A |
| `os/stderr.zig` raw-write seam (579db4189, f36854345) | new `os/stderr.zig` | E |
| crash init environ-map/init-thread (15c50c1db, a82637b53) | `crash/` | E |
| macOS logger cache (3225e9ebb) | `rustty-core::logging` (shared logFn) | F (bin) |
| ghostty.h action/IPC enums; inspector IO export | apprt actions/inspector | F |
| `+new-tab` cli, `new_window.zig` cutPrefix caller | cli | G |
| wasm examples/gate, `rustty-wasm` crate | ws14 lane | F |
| **`apprt_api_surface` G1b drop=4** | the 4 NEW `src/apprt/surface.zig` tests (`DesktopNotification init`, 3× `copyUtf8Z`) — `rustty-apprt-api` surface has 0. NOT this area's files; flagged here because the task named it | E/F |

### UB/SB hazards for the coming ports (PORT_RULES raw-pointer discipline)

1. **Snapshot decoder object** (`snapshot_decoder_*`): a heap C object that holds an
   allocator + streaming state and, on `decode`, writes INTO a `TerminalWrapper`'s
   Terminal/Screen. Long-lived raw alias + interleaved `&mut` through another path is
   the exact shape of memories `shaper-hook-sb-clean-interleave` and
   `search-screen-sb-ub-finding`. Route all mutation through the decoder's OWN raw
   pointers; add a LIFETIMES.tsv row (memory `adopt-lifetimes-tsv`) and prove on a
   Miri harness before trusting hand-reasoning.
2. **Dirty-row iterator + `render_state_clean`** (`0d37f2d34`): the iterator caches
   page/row positions while `clean` mutates dirty bits — re-read across mutation must
   use raw-ptr args, not `&mut Row`/`&mut Page` (memory
   `rust-mut-args-strongly-protected`).
3. **Render fast reads** (`e3056658d`/`74a233b54`): cached row/style spans into `Page`
   memory across `update` calls — same class.
4. **`terminfo_name_buf` `@memcpy`** (`74efadb44`): fixed-buffer copy from a C string —
   carry the `str.len > buf.len ⇒ invalid_value` check verbatim (Phase-3 `960c2cca5`
   overflow-class note).
5. **New enums/typed options across the ABI** (`ModeConfig`, snapshot decoder
   options/data tags, `GhosttyRenderCursor` fields): inbound `c_int` + explicit
   `match` narrowing per the D11 convention — `check_closed_enum_abi.sh` +
   `check_closed_enum_abi_runtime.sh` now exist and must stay green over the new
   surface.
6. **wasm page pool** (`492c26067`): global mutable allocator state — fine on
   single-threaded wasm; document the invariant, keep it `cfg(target_arch="wasm32")`.

---

## 6. Gate coverage deltas this range creates

| # | hole | proposal |
|---|---|---|
| 1 | **Stale-export blindness**: A5(d) is oracle→rustty only; the 15 removed-upstream exports (`§3b`) can survive forever | add arm (d′): `comm -13 oracle rustty` non-empty ⇒ FAIL (allowlist the deliberate rustty-only exports, currently none expected) |
| 2 | **A3 sha-pin stale**: `.port/tools/oracle-ghostty-h.sha256` pins the OLD `ghostty.h`; the oracle working tree is the NEW one ⇒ A3 is RED/BLOCKED right now | re-pin as part of Bundle E, never before the embed-header port lands (R5: the red is currently telling the truth) |
| 3 | **G1b vt_c aggregate masking** (§1): per-file deficits invisible under the module glob | when Bundle S lands, verify snapshot/io test counts per-file against `src/terminal/c/{snapshot,io}.zig` (6+11 zig tests today), not via the module total |
| 4 | **arm (c) registry-driven blindness** (Phase-3 §5 finding, still open): new header typedefs without `types.rs` rows get zero layout asserts | unchanged recommendation: header-driven enumeration; becomes urgent with Bundle T's 90 new registry rows |
| 5 | **schema verifier**: upstream runs `types-schema-verify.py` in CI; rustty has no analogue | port schema + verifier into a gate in Bundle T |

## 7. Dropped-test targets (g1b-after file) belonging to this area

**None of the 10 RED rows are this area's rustty modules** (`vt_c`, `lib_vt`, `main_c`
all read ok). The area's REAL test debt is masked (§1): oracle `src/terminal/c` today
carries snapshot.zig **6**, io.zig **11**, terminal.zig **116** (rustty terminal.rs has
113 `#[test]`), render.zig **46**, kitty_graphics.zig **42**, plus the `SkipZigTest`
feature-gate arms from `1fdbb8c91` — all owed by the bundles above, enumerable per
commit in §2. `apprt_api_surface drop=4` is the `src/apprt/surface.zig` test set
(area E/F) — named in §5 so it is not double-counted here.
