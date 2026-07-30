# Phase 3 assessment — area C: the C API surface (`src/terminal/c`, `include/**`, `src/lib_vt.zig`, `src/lib/**`, `example/**`)

Range: `4749c4e93` (pin, 2026-06-20) → `6ad1fe7d8` (`origin/main`, 2026-07-29).
Rustty side: `crates/rustty-vt-c/`, `include/rustty/`, `include/rustty.h`, `example/`.
Assessment only — **nothing was implemented**. Written per the Phase 3 binding rule
(re-vet, do not assume; "already ported" needs a symbol + `file:line` + a failing gate).

**38 non-merge upstream commits** touch this area. All 38 are individually verdicted below.

---

## 0. Coverage statement — exhaustive vs sampled

| Work | Coverage |
|---|---|
| Upstream commit enumeration for the area paths | **EXHAUSTIVE** (`git log --no-merges --reverse --stat 4749c4e93..origin/main -- src/terminal/c include src/lib_vt.zig src/lib example` → 38 commits, all verdicted) |
| Exported C symbol set, upstream head vs rustty, both directions | **EXHAUSTIVE** (mechanical: upstream `@export` list in `src/lib_vt.zig` vs `grep -rhoE 'extern "C" fn (rustty\|ghostty)_…' crates/rustty-vt-c/src`) |
| Curated header family `include/rustty/vt/**` vs upstream `include/ghostty/vt/**` | **EXHAUSTIVE** (full `diff -ru` of a token-renamed upstream snapshot vs rustty's tree; 33 files; every hunk read) |
| `rustty_type_json()` registered-struct set vs upstream `types.zig` | **EXHAUSTIVE** (set comm both directions) |
| C/C++ example sources + READMEs (26 dirs) | **EXHAUSTIVE** (`diff -ru` per dir, renamed) |
| `include/rustty.h` standalone C compile | **EXHAUSTIVE** (actually compiled; 2 errors reproduced) |
| Gate coverage map (A1/A3/A4/A5, DG5, G1b, `_vt_header_baseline.txt`) | **EXHAUSTIVE by source reading**; A5 step (b) intersection reproduced **statically** from the checked-in headers (no build) — the *dynamic* halves (nm/wasm exports, `rustty_type_json` layouts) were **NOT** executed (no release dylib / wasm artifact present; build deliberately skipped for cost) |
| Per-function *body* fidelity of the ~183 already-ported exports | **SAMPLED**, not exhaustive. Bodies were read for every function the range actually touches (colour, unicode, color_scheme, compression, clipboard, scroll_viewport, resize, grid_ref, graphemes-utf8, cursor defaults). Functions the range does **not** touch were not re-audited. |
| `src/lib/**` (Zig 0.16 `compat/`, `enum/struct/union.zig`) | **EXHAUSTIVE read**, verdicted N/A (Zig-comptime machinery with no Rust counterpart) |
| Zig/Swift examples (`zig-vt`, `zig-vt-stream`, `zig-formatter`, `swift-vt-xcframework`) | **NOT assessed** — no rustty counterparts exist by design |

---

## 1. Summary with numbers

**Exports (C symbols).** Upstream head exports **185**; rustty-vt-c exports **183**.

| | count |
|---|---|
| upstream head exported C symbols (`src/lib_vt.zig` `@export` list, incl. 12 wasm-only) | 185 |
| rustty-vt-c live `#[no_mangle] extern "C" fn rustty_*` | 183 |
| **exports upstream has, rustty ABSENT** | **2** |
| **exports rustty has, upstream no longer has** | **0** |
| exports added by upstream *within this range* | 17 (of which **15 PORTED**, 2 absent) |
| exports removed by upstream within this range | 0 |
| `ghostty_*` aliases surviving in `crates/rustty-vt-c/` (R2 violation) | **0** ✅ |

**Headers.** 33 curated headers; **26 byte-identical** to upstream head after token rename;
**7 differ**, of which 2 are whitespace/doc-only and 1 is a deliberate port-identity divergence:

| header | verdict |
|---|---|
| `terminal.h` | **PARTIAL** — 19 distinct missing/diverged hunks (§4.1) |
| `render.h` | **PARTIAL** — 66 lines absent (2 decls + Two-Phase-Updates docs) |
| `modes.h` | **PARTIAL** — 1 macro absent (`RUSTTY_MODE_VISIBILITY_REPORT`) |
| `key.h`, `mouse.h` | doc-snippet drift, consequence of the `terminal_new` constructor gap |
| `unicode.h` | whitespace-only (continuation-line indent) — **PORTED** |
| `vt.h` | rustty is **AHEAD** (2 extra doc lines upstream lacks) + include-order + `rustty.sh` identity — see §4.4 |
| other 26 (`color.h`, `color_scheme.h`, `kitty_graphics.h`, `sgr.h`, `osc.h`, `selection*.h`, `grid_ref*.h`, `formatter.h`, `screen.h`, `style.h`, `sys.h`, `paste.h`, `focus.h`, `size_report.h`, `device.h`, `types.h`, `point.h`, `wasm.h`, `allocator.h`, `build_info.h`, `key/*.h`, `mouse/*.h`) | **PORTED**, byte-identical |

**`rustty_type_json()` struct registry.** upstream 34, rustty 34; **2 missing**
(`RusttyTerminalDesktopNotification`, `RusttyTerminalProgressReport`), **1 extra**
(`RusttyTerminalOptions`, which upstream *negative-asserts* absent at
`src/terminal/c/types.zig:234`).

**Commit verdicts.** 38 commits: **17 PORTED**, **13 ABSENT**, **3 PARTIAL**, **5 N/A**.

**Soundness finding (new, in-range).** `rustty_terminal_scroll_viewport` receives a closed
`#[repr(C)]` Rust enum discriminant **directly from C with no narrowing point** — instant UB
on any out-of-range tag, and the header itself declares
`RUSTTY_SCROLL_VIEWPORT_MAX_VALUE = RUSTTY_ENUM_MAX_VALUE` as a nominally-valid enumerator
(`include/rustty/vt/terminal.h:255`). The `Row` tag this struct carries was added *by this
range* (`3a2e28329`), so the fix belongs to Phase 3. Three more exports share the defect but
are pre-existing (§4.5).

**By-value struct returns.** The range adds **zero** new by-value aggregate returns anywhere
in `include/**`. It *removes* one by-value aggregate **parameter** (`GhosttyColorRgb` →
`const GhosttyColorRgb*`, `20a1bfa5f` — already ported) and removes another
(`GhosttyTerminalOptions`, `03d5fa268` — **not** ported). No AAPCS64 sret exposure from this
range; the two parameter changes are still ABI-breaking and must land atomically with the
headers.

---

## 2. Per-change verdict table

Gate column names the **real** gate. `A5` = `.port/tools/gates/check_rustty_vt_headers.sh`
(the vt header/export/layout gate). `A4` = `check_embed_h_abi.sh` (the `rustty-embed.h` sret
gate — **irrelevant to this area**, it never reads `rustty/vt.h`). `G1b` =
`check_test_counts.py` (per-module `#[test]` ≥ zig `test{}`; oracle-tree-relative, so it only
becomes an enforcer once the pin advances). `DG5` =
`crates/rustty-apprt-c/tests/panic_policy.rs::no_c_unwind_in_c_api_sources`.
"none" means **no gate can currently fail on this**.

Ordered oldest→newest.

| upstream | what changed | rustty status | evidence | guarding gate | cost | risk |
|---|---|---|---|---|---|---|
| `aea63d71f` | `rowCellsGetGraphemesUtf8`: `needed` accumulator widened `u3`→`usize`; test extended to a 3-case table (combining, 8-byte flag, family emoji, exact-size, `cap == needed-1` no-write) | **PARTIAL** — fix is structurally N/A (Rust `char::len_utf8()→usize`, no overflow possible); the **test-table extension is ABSENT** | fix: `crates/rustty-vt-c/src/render.rs:1304` `let mut needed = cp0.len_utf8();`. Missing tests: `crates/rustty-vt-c/src/render/tests.rs:897` covers only `"e\u{301}"`; `grep -rn '1F1FA\|1F468' crates/rustty-vt-c/src` → only `unicode.rs:96`, nothing in `render/tests.rs` | G1b (post-pin) | S | low |
| `f245cdc66` | `lib_vt.zig`: expose selection gesture to the Zig module surface | **N/A** — Zig-module re-export; rustty publishes Rust modules | `crates/rustty-vt-c/src/selection_gesture.rs` exists and is `pub` in `lib.rs` | none needed | – | none |
| `bdc0b6c19` | kitty/gfx generation stamps; `…DATA_GENERATION=2`, `…IMAGE_DATA_GENERATION=9`; transmit-time removed; example prints generation | **PORTED** | `crates/rustty-vt-c/src/kitty_graphics.rs:123` `Generation = 2`, `:313` `Generation = 9`, `:231`, `:468`; test `:3363 storage_generation_via_get`; header `include/rustty/vt/kitty_graphics.h:166,428` byte-identical to upstream; `example/c-vt-kitty-graphics/src/main.c:133` | A5(a,b) + crate tests | – | none |
| `0a410f18e` | `clipboard_set` effect for OSC 52 | **PORTED (superseded)** — folded into `634ef7198` | see `634ef7198` row | as below | – | none |
| `fc5a72772` | NEW `unicode.zig` + `unicode.h`: `rustty_unicode_codepoint_width` | **PORTED** | `crates/rustty-vt-c/src/lib.rs:425`; impl `crates/rustty-vt-c/src/unicode.rs:1..43`; `include/rustty/vt/unicode.h` == upstream; 4 upstream tests folded into `unicode.rs:51 codepoint_width_cases` | A5(a,b) | – | none |
| `3a2e28329` | scroll-to-row viewport: `ScrollViewportTag.row = 3` + union `row: size_t` | **PORTED**, but see the §4.5 soundness defect it introduces | `crates/rustty-vt-c/src/terminal.rs:1506` `Row = 3`, `:1518` `pub row: usize`, ABI static-asserts `:1533-1535`; export `lib.rs:2494`; header `terminal.h:254,268` | A5(c) covers the *size*; **none** covers the enum-validity defect | – | **HIGH** (UB) |
| `65e61282a` | `rustty_unicode_grapheme_width` | **PORTED** | `crates/rustty-vt-c/src/lib.rs:436`; impl `unicode.rs:10..43`; 4 upstream tests → `unicode.rs:60,68,77,93` | A5(a,b) | – | none |
| `f00e90694` | NEW `color_scheme.zig` + `color_scheme.h` + `c-vt-color-scheme` example: `rustty_color_scheme_report_encode` | **PORTED**, exemplary | `crates/rustty-vt-c/src/lib.rs:252` (with an explicit `match` narrowing the inbound `c_int` → `ColorScheme`, `:259-264`); impl `color_scheme.rs:15`; 5/5 upstream tests `color_scheme.rs:48-103`; `include/rustty/vt/color_scheme.h` == upstream; `example/c-vt-color-scheme/{README.md,build.sh,src}` | A5(a,b) + crate tests | – | none |
| `2970e9a2a` | 10 new colour utility exports (+427 header, +559 zig) | **PORTED** | `crates/rustty-vt-c/src/lib.rs:127,142,155,168,182,191,207,216,225,235,241`; impl `color.rs`; `include/rustty/vt/color.h` **byte-identical** to upstream head; 23 upstream tests → 24 rustty `#[test]` in `color.rs` | A5(a,b,c) + crate tests | – | none |
| `446f80f4e` | render-state two-phase update: `rustty_render_state_begin_update` / `_end_update` + 66 header lines | **ABSENT** | `grep -rn 'begin_update\|end_update' crates/rustty-vt-c/ include/` → **nothing**. `include/rustty/vt/render.h` stops at `rustty_render_state_update` (`:342`). Area A also lacks it: `grep -n 'pending_styles\|fn begin_update' crates/rustty-vt/src/render.rs` → **nothing** | **none** (see §5 — A5 is blind to a symbol missing from *both* header and lib) | **L** | med — blocked on area A (`src/terminal/render.zig` +919, `page.zig` +274) |
| `20a1bfa5f` | RGB inputs by pointer (`GhosttyColorRgb` → `const GhosttyColorRgb*`), removing a by-value aggregate param | **PORTED** | `crates/rustty-vt-c/src/lib.rs:128,207,216,226` all `*const color::RgbC`; header `color.h` == upstream | A5(a,b) | – | none |
| `14c829883` | report OSC colour queries through `write_pty` in lib-vt | **PORTED** | test `crates/rustty-vt-c/src/terminal.rs:4234` cites `(oracle 14c829883)`, fn `:4236 write_pty_receives_osc_color_query_response` | crate test | – | none |
| `421fe8dab` | `grid_ref.zig` migrated `p.node.data.…` → `p.node.page()` (compressed-page boundary) | **PORTED** (with a stale reference comment) | `crates/rustty-vt-c/src/grid_ref.rs:256,267` use `rustty_vt::pagelist::Node::page(p.node)`. **Divergence:** the transcribed oracle comment at `:227-232` still shows the pre-`421fe8dab` `p.node.data.…` body | D9 `check_stale_comments.sh` (advisory) | S | low |
| `172f15da3` | expose compression: `rustty_terminal_compress`, `rustty_terminal_compression_activity`, `CompressionAmount`/`CompressionResult`, `c-vt-compression` example | **PORTED**, exemplary | exports `crates/rustty-vt-c/src/lib.rs:2509,2525`; impl `terminal.rs:867,899`; **inbound `mode` narrowed by explicit `match`** at `terminal.rs:922-926` with the correct PORT NOTE; header block present in `terminal.h`; `example/c-vt-compression/{README.md,build.sh,src/main.c}` | A5(a,b) + crate tests | – | none |
| `634ef7198` | clipboard writes protocol-neutral: `RusttyClipboardWrite`/`ClipboardContent`/`ClipboardWriteResult`, `OPT_CLIPBOARD_WRITE=26` | **PORTED**, exemplary | fn-ptr typedef `terminal.rs:229`; trampoline `:517`; `OPT_CLIPBOARD_WRITE = 26` `:966`; **`WriteResult` (a value returned *from* C) is `#[repr(transparent)] struct WriteResult(pub i32)`** at `crates/rustty-vt/src/clipboard.rs:54-56` with the correct rationale comment; type_json rows `types.rs:201,212`; `example/c-vt-effects/src/main.c:42-71` identical to upstream | A5(a,b,c) + crate tests | – | none |
| `89b103dd5` | `resize` delegated to `wrapper.stream.handler.resize(…)` (cell geometry, sync-render reset, mode-2048 in-band all move into `TerminalStream.resize`); 32 lines deleted from the C wrapper; 2 tests reworked | **ABSENT** | `crates/rustty-vt-c/src/terminal.rs:1587-1660` is verbatim the **pre-**`89b103dd5` body (inline `resize` → `width_px`/`height_px` saturating mul → `modes.set(SynchronizedOutput,false)` → in-band `size_report::encode`) | none | M | med — needs `TerminalStream::resize` (area D) |
| `439d35e27` | semantic stream-failure flag + `DATA_VT_PROCESSING_ERROR = 33` | **ABSENT** | `crates/rustty-vt-c/src/terminal.rs` `TerminalData` stops at `ViewportActive = 32` (`:1789`); no `vt_processing_error`; `include/rustty/vt/terminal.h` `…DATA_MAX_VALUE` follows `VIEWPORT_ACTIVE = 32` | none | S (C side) | low — needs `stream.handler.semantic_failure` (area D) |
| `c594031d5` | cursor defaults moved onto `Terminal`; C wrapper calls `t.setDefaultCursorStyle/Blink`; test extended with a RIS round-trip | **ABSENT** | `crates/rustty-vt-c/src/terminal.rs:1389-1410` still writes `wrapper.stream.handler.default_cursor_style` directly; `grep -rn 'fn set_default_cursor_style' crates/rustty-vt/src/terminal.rs` → **nothing** | none | S (C side) | low — needs area A |
| `e8525c0fd` | Zig 0.16.0. C-API-visible parts: (i) new `src/lib/compat/{fd,thread,reader,file}.zig`; (ii) `TerminalWrapper.io_impl` reshaped; (iii) **`kitty_image_medium_temp_file` changes from `bool` to a *directory* `lib.String`**, adding `tmp_dir_path: [max_path_bytes]u8` to the wrapper | **PARTIAL** — (i)/(ii) **N/A** (no `std.Io` in Rust; verified `TerminalWrapper` at `crates/rustty-vt-c/src/terminal.rs:93-98` has no `io_impl`/`tmp_dir_path`); (iii) **ABSENT** | `crates/rustty-vt-c/src/terminal.rs:1253,1269` treat `KittyImageMediumTempFile` as `bool` (`limits.temporary_file = val`); getter `:2050` returns `bool`; header `terminal.h` documents `bool*` for OPT 17 / DATA 28 | none | M | med — needs kitty `image_limits.temporary_file` → `.enabled{.directory}` (area D) |
| `f2a7652ab` | `lib_vt.zig` touchups | **N/A** — Zig root-module wiring | – | – | – | none |
| `da04b65d4` | `std.Io.Threaded.init_single_threaded` for the C API (no signal handlers, heap-allocated Io) | **N/A** — rustty has no `std.Io` seam; `TerminalWrapper` (`terminal.rs:93`) carries no Io at all | – | – | – | none |
| `495666870` | suppress lib-vt test log spam | **N/A** — Zig `std_options.logFn` | – | – | – | none |
| `b988efcfe` | 0.16 translation regressions; header side = OPT 17 / DATA 28 docs flipped to the directory semantics | **ABSENT** — rides on `e8525c0fd`(iii) | `include/rustty/vt/terminal.h` OPT 17 / DATA 28 still say `bool*` | none | S | low |
| `960c2cca5` | fix kitty temp-directory `@memcpy` length mismatch (`&wrapper.tmp_dir_path` ← `v.ptr[0..v.len]`) | **N/A-until** — the buggy code does not exist in rustty; blocked behind `e8525c0fd`(iii). **Must not be dropped**: if the directory option is ported, the ported code must already carry the fix (copy exactly `v.len`, bounds-check first) | no `tmp_dir_path` in `crates/rustty-vt-c/src/terminal.rs` | none | S | low, but a real overflow class |
| `39ae85f04` | DECRQSS answers routed through the C stream's `write_pty` (+ 1 C-API test) | **ABSENT (test), cross-area (impl)** | `grep -rn 'write_pty receives' crates/rustty-vt-c/src` → only the OSC-colour test (`terminal.rs:4234`); no DECRQSS test. rustty-vt *has* DCS DECRQSS machinery (`crates/rustty-vt/src/dcs.rs:172,340,426`; `terminal.rs:4374`) — unverified whether it reaches the C `write_pty` path | G1b (post-pin) | S | low |
| `1fe1b2d23` | `example/c-vt-static/build.zig` Windows static-link fix | **N/A** — rustty examples use `build.sh` (`.port/phase-2/EXAMPLES_PORT.md`) | `example/c-vt-static/build.sh` | – | – | none |
| `84254a9d8` | MSVC `_fltused` marker in `lib_vt.zig` | **N/A** — Zig/MSVC build artefact; rustty is macOS-first | – | – | – | none |
| `3b4600014` | comment clarification in `lib_vt.zig` | **N/A** | – | – | – | none |
| `86f81fb5b` | 1-line follow-up to the scrollback-line-limit exposure | **ABSENT** — rides on `03d5fa268` | see `03d5fa268` | none | – | – |
| `03d5fa268` | **the largest C-ABI break in the range.** Removes `GhosttyTerminalOptions` entirely: `ghostty_terminal_new(alloc, out, uint16 cols, uint16 rows)`; scrollback limits become `OPT_SCROLLBACK_MAX_BYTES = 27` / `OPT_SCROLLBACK_MAX_LINES = 28`; drops `TerminalOptions` from `type_json` (with a negative assertion); rewrites 684 lines of `terminal.zig`; updates **16 example call sites** + `wasm-vt/index.html` | **ABSENT** | rustty is at baseline: `include/rustty/vt/terminal.h:176-193` still typedefs `RusttyTerminalOptions`, `:1213-1215` still `rustty_terminal_new(…, RusttyTerminalOptions options)`; export `crates/rustty-vt-c/src/lib.rs:2402`; struct `terminal.rs:670-674`; registered in `type_json` (`types.rs`, the row upstream negative-asserts); `Option_` stops at `ClipboardWrite = 26` (`terminal.rs:966`); all 16 examples still build an `opts` struct (`grep -rn rustty_terminal_new example/`) | A5(c) will catch a `RusttyTerminalOptions` **layout** drift but **cannot** see the struct's removal or the new option tags | **L** | **HIGH** — ABI-breaking; a by-value aggregate param disappears from the signature; must land atomically across `terminal.h` + `terminal.rs` + `types.rs` + 16 examples + `wasm-vt/index.html`. Needs area A's scrollback byte/line limit accessors. |
| `a27e04e8f` | readers for the configured limits: `DATA_SCROLLBACK_MAX_BYTES = 34`, `DATA_SCROLLBACK_MAX_LINES = 35` (always primary-screen; `RUSTTY_NO_VALUE` when unlimited) | **ABSENT** | `TerminalData` stops at 32 (`terminal.rs:1789`) | none | S after `03d5fa268` | low |
| `5fd2973b9` | +25 lines of docs on OPT 27 / OPT 28 (page-granularity estimate, byte-vs-line interaction) | **ABSENT** — rides on `03d5fa268` | those options don't exist in `include/rustty/vt/terminal.h` | G3/G3b (comment fidelity) | S | low |
| `c3655ba25` | desktop-notification effect: `RusttyTerminalDesktopNotification` (sized struct: `size`, `title`, `body`), `…DesktopNotificationFn`, `OPT_DESKTOP_NOTIFICATION = 29`, `type_json` row | **ABSENT** | `grep -rn 'DesktopNotification' crates/rustty-vt-c/ include/rustty/` → **nothing**; upstream `types.zig:72` registers it | none | M | med |
| `47d602c42` | progress-report effect: `RusttyTerminalProgressState` (5 values + `MAX_VALUE`), `RusttyTerminalProgressReport` (sized struct: `size`, `state`, `int8_t progress`), `…ProgressReportFn`, `OPT_PROGRESS_REPORT = 30`, `type_json` row | **ABSENT** | `grep -rn 'ProgressReport\|ProgressState' crates/rustty-vt-c/ include/rustty/` → **nothing**; upstream `types.zig:73` | none | M | med |
| `628adaf30` | share the progress-state enum via `src/lib/enum.zig` | **N/A structurally** (Zig comptime `Enum(target, keys)` factory; rustty hand-authors each `#[repr(C)]` projection) but the **enum it shares is the `47d602c42` type**, so it lands with that row | – | – | – | – |
| `2729996ea` | update event tests for the new constructor | **ABSENT** — rides on `03d5fa268` | – | G1b (post-pin) | S | low |
| `6c8c07981` | visibility reports: `RUSTTY_MODE_VISIBILITY_REPORT (rustty_mode_new(2033,false))` | **ABSENT** | `include/rustty/vt/modes.h` jumps 2031 (`:94`) → 2048 (`:95`); `grep -rn '2033\|VisibilityReport' crates/rustty-vt/src/modes.rs` → **nothing** | none | S (header) | low — needs the mode in area A/D's mode table first |

---

## 3. Symbol-level delta (mechanical, both directions)

**Method.** Upstream's authoritative export list is the `@export(&c.X, .{ .name = "ghostty_…" })`
block in `src/lib_vt.zig` (`origin/main` lines 170–260); `src/terminal/c/*.zig` contains no
`export fn` at all (functions are `pub fn … callconv(lib.calling_conv)` and published only by
that block). Names were token-renamed `ghostty_`→`rustty_` and `comm`'d against
`grep -rhoE 'extern "C" fn (rustty|ghostty)_[A-Za-z0-9_]+' crates/rustty-vt-c/src`.

### 3a. Upstream head → rustty (missing exports): **2**

```
rustty_render_state_begin_update      (446f80f4e)
rustty_render_state_end_update        (446f80f4e)
```

Both are absent from `crates/rustty-vt-c/` **and** from `include/rustty/vt/render.h` **and**
from the underlying `crates/rustty-vt/src/render.rs`.

### 3b. rustty → upstream head (exports upstream no longer has): **0**

`comm -13` is empty. In particular **no `ghostty_*` alias survived** in this crate — the R2
check is clean:

```
$ grep -rhoE 'extern "C" fn (rustty|ghostty)_[A-Za-z0-9_]+' crates/rustty-vt-c/src \
    | sed 's/.*fn //' | grep -c '^ghostty_'
0
```

(183 `#[no_mangle]` attributes on 183 `extern "C" fn rustty_*` items, 183 distinct names — a
clean 1:1:1. `grep -c no_mangle` reports 184 because of one prose mention in the crate doc
comment at `crates/rustty-vt-c/src/lib.rs:19`. All 12 wasm-only exports sit behind
`#[cfg(target_arch = "wasm32")]` at `lib.rs:1507`, correctly mirroring upstream's
`if (builtin.target.cpu.arch.isWasm())` guard.)

### 3c. Exports added by upstream *inside* the range: 17 → 15 ported / 2 absent

| symbol | commit | rustty |
|---|---|---|
| `rustty_color_contrast` | `2970e9a2a` | `lib.rs:225` |
| `rustty_color_luminance` | `2970e9a2a` | `lib.rs:207` |
| `rustty_color_perceived_luminance` | `2970e9a2a` | `lib.rs:216` |
| `rustty_color_palette_default` | `2970e9a2a` | `lib.rs:182` |
| `rustty_color_palette_generate` | `2970e9a2a` | `lib.rs:191` |
| `rustty_color_parse` | `2970e9a2a` | `lib.rs:142` |
| `rustty_color_parse_palette_entry` | `2970e9a2a` | `lib.rs:168` |
| `rustty_color_parse_x11` | `2970e9a2a` | `lib.rs:155` |
| `rustty_color_x11_name_count` | `2970e9a2a` | `lib.rs:241` |
| `rustty_color_x11_names` | `2970e9a2a` | `lib.rs:235` |
| `rustty_color_scheme_report_encode` | `f00e90694` | `lib.rs:252` |
| `rustty_unicode_codepoint_width` | `fc5a72772` | `lib.rs:425` |
| `rustty_unicode_grapheme_width` | `65e61282a` | `lib.rs:436` |
| `rustty_terminal_compress` | `172f15da3` | `lib.rs:2525` |
| `rustty_terminal_compression_activity` | `172f15da3` | `lib.rs:2509` |
| `rustty_render_state_begin_update` | `446f80f4e` | **ABSENT** |
| `rustty_render_state_end_update` | `446f80f4e` | **ABSENT** |

### 3d. `rustty_type_json()` registered-struct delta

Upstream `src/terminal/c/types.zig` `structs` map: 34 entries. rustty
`crates/rustty-vt-c/src/types.rs`: 34 entries.

| direction | delta |
|---|---|
| upstream has, rustty lacks | `RusttyTerminalDesktopNotification` (`c3655ba25`), `RusttyTerminalProgressReport` (`47d602c42`) |
| rustty has, upstream removed | `RusttyTerminalOptions` — upstream `types.zig:234` asserts `!root.contains("GhosttyTerminalOptions")`. Rustty must **delete** this row with `03d5fa268`, and should port the negative assertion. |

### 3e. Signature-level delta (declaration text, not just names)

Only two declarations changed shape in the range, both parameter-side:

| function | upstream head | rustty | verdict |
|---|---|---|---|
| `rustty_color_rgb_get` | `(const RusttyColorRgb* color, uint8_t*, uint8_t*, uint8_t*)` | same (`lib.rs:127`) | **PORTED** (`20a1bfa5f`) |
| `rustty_terminal_new` | `(const RusttyAllocator*, RusttyTerminal*, uint16_t cols, uint16_t rows)` | `(…, RusttyTerminalOptions options)` — by-value aggregate (`include/rustty/vt/terminal.h:1213`, `lib.rs:2402`) | **ABSENT** (`03d5fa268`) |

`grep -E '^[-+].*GHOSTTY_API' over the range's `include/` diff yields **18 additions and 1
deletion**, and **no added by-value aggregate return**. The only by-value aggregate return
class in this header family is unchanged by the range.

---

## 4. Partials and divergences, with the exact missing hunks

### 4.1 `include/rustty/vt/terminal.h` — 19 missing/diverged hunks

Deletions (upstream removed; rustty must remove):
1. `typedef struct {uint16_t cols; uint16_t rows; size_t max_scrollback;} RusttyTerminalOptions;` — `:176-193`.
2. `rustty_terminal_new`'s "starts with various reasonable defaults e.g. around scrollback limits" doc paragraph must be dropped along with the `@param cols/@param rows` re-expansion.

Additions (upstream has; rustty lacks):
3. `rustty_terminal_new(const RusttyAllocator*, RusttyTerminal*, uint16_t cols, uint16_t rows)`.
4. `RusttyTerminalDesktopNotification` — `{size_t size; RusttyString title; RusttyString body;}`, sized-struct contract.
5. `RusttyTerminalDesktopNotificationFn` — `void(*)(RusttyTerminal, void*, const RusttyTerminalDesktopNotification*)`.
6. `RusttyTerminalProgressState` — `REMOVE=0, SET=1, ERROR=2, INDETERMINATE=3, PAUSE=4, MAX_VALUE=RUSTTY_ENUM_MAX_VALUE`.
7. `RusttyTerminalProgressReport` — `{size_t size; RusttyTerminalProgressState state; int8_t progress;}` (`-1` = omitted).
8. `RusttyTerminalProgressReportFn`.
9. `RUSTTY_TERMINAL_OPT_SCROLLBACK_MAX_BYTES = 27` (`size_t*`; NULL ⇒ unlimited; `0` ⇒ disable+erase).
10. `RUSTTY_TERMINAL_OPT_SCROLLBACK_MAX_LINES = 28` (`size_t*`; NULL ⇒ unlimited).
11. `RUSTTY_TERMINAL_OPT_DESKTOP_NOTIFICATION = 29`.
12. `RUSTTY_TERMINAL_OPT_PROGRESS_REPORT = 30`.
13. `RUSTTY_TERMINAL_DATA_VT_PROCESSING_ERROR = 33` (`bool*`; not cleared by `rustty_terminal_reset()`).
14. `RUSTTY_TERMINAL_DATA_SCROLLBACK_MAX_BYTES = 34` (`size_t*`; `RUSTTY_NO_VALUE` when unlimited; always primary screen).
15. `RUSTTY_TERMINAL_DATA_SCROLLBACK_MAX_LINES = 35` (same contract).
16. The +25-line `5fd2973b9` doc bodies on OPT 27 / OPT 28.
17. Two rows in the effect-callback doc table: `RUSTTY_TERMINAL_OPT_DESKTOP_NOTIFICATION` and `RUSTTY_TERMINAL_OPT_PROGRESS_REPORT`.

Semantic flips (rustty has the old semantics):
18. `RUSTTY_TERMINAL_OPT_KITTY_IMAGE_MEDIUM_TEMP_FILE = 17`: `bool*` (enable/disable, NULL = no-op) → `RusttyString*` (restrict to a directory, string copied into the terminal, NULL = disable).
19. `RUSTTY_TERMINAL_DATA_KITTY_IMAGE_MEDIUM_TEMP_FILE = 28`: `bool*` → `RusttyString*` (empty when disabled).

Plus a doc flip on `rustty_terminal_set`: rustty says "NULL clears the option to its default";
upstream now says "The behavior of a NULL value is specific to each option and is documented
by the corresponding RusttyTerminalOption value."

### 4.2 `include/rustty/vt/render.h` — 66 lines absent
- The `## Two-Phase Updates` section of the group doc.
- The paragraph in `rustty_render_state_update`'s doc explaining it is the both-phases convenience and that lock-holding callers should prefer the split.
- `RUSTTY_API RusttyResult rustty_render_state_begin_update(RusttyRenderState state, RusttyTerminal terminal);` + its 20-line doc.
- `RUSTTY_API RusttyResult rustty_render_state_end_update(RusttyRenderState state);` + its 12-line doc ("Calling this without a prior begin is a safe no-op").

### 4.3 `include/rustty/vt/modes.h` — 1 line absent
`#define RUSTTY_MODE_VISIBILITY_REPORT (rustty_mode_new(2033, false))`, between the 2031 and
2048 macros (currently `:94` → `:95`).

### 4.4 `include/rustty/vt.h` — rustty is AHEAD of upstream (review, don't blindly "fix")
Three divergences, all rustty-side additions/choices, none an upstream gap:
- rustty adds `- @ref color_scheme "Color Scheme Report Encoding" …` to the module list and an `@example c-vt-color-scheme/src/main.c` block plus its bullet. Upstream's `f00e90694` added only the `#include` and one bullet. **Recommendation: keep** (it is strictly better Doxygen and harmless), but record it as a deliberate divergence so a future mechanical header-diff gate doesn't churn.
- Include ordering: rustty has `device.h` before `color_scheme.h`; upstream has `color_scheme.h` before `device.h`. **Recommendation: match upstream** (zero-risk, kills a permanent diff line).
- `https://rustty.sh` vs upstream's `ghostty.org`→`rustty.org` token rename. Port-identity choice; leave, and add it to the rename carve-out list in `.port/tools/PARITY_NAMING.md` if a header-diff gate is built.

### 4.5 SOUNDNESS: closed `#[repr(C)]` enums received by value from C with no narrowing point

The program's established rule (memory `embed-abi-byvalue-sret-gate`, and the `WriteResult`
precedent at `crates/rustty-vt/src/clipboard.rs:54`) is: **inbound** C integers narrow via an
explicit `match`; values with no narrowing point get `#[repr(transparent)]`. **Four** exports
violate it by declaring the *Rust enum itself* as the C parameter type, so the invalid value is
materialized at the ABI boundary — **before** any `match` can run. Every one of these enums is
declared in the header with a `…_MAX_VALUE = RUSTTY_ENUM_MAX_VALUE` enumerator, i.e. the header
documents an out-of-range value as writable by a conforming C caller.

| export | offending param | type | in this range? |
|---|---|---|---|
| `rustty_terminal_scroll_viewport` (`lib.rs:2494`) | `behavior.tag` inside the by-value `ScrollViewportC` | `ScrollViewportTag` (`terminal.rs:1502`, `#[repr(C)]`, 0..=3) | **YES** — `3a2e28329` added `Row = 3` to this very enum |
| `rustty_focus_encode` (`lib.rs:278`) | `event` | `focus::FocusEvent` (`focus.rs:28`, `#[repr(C)]`, 0..=1) | no (pre-existing) |
| `rustty_sys_log_stderr` (`lib.rs:346`) | `level` | `sys::LogLevel` (`sys.rs:44`, `#[repr(C)]`, 0..=3) | no (pre-existing) |
| `rustty_size_report_encode` (`lib.rs:453`) | `style` | `size_report::Style` (`size_report.rs:29`, `#[repr(C)]`, 0..=3) | no (pre-existing) |

Counter-examples confirming the pattern is understood elsewhere in the crate and that these four
are oversights, not policy: `rustty_mode_report_encode`'s `state` is
`pub struct ReportState(pub c_int)` — a transparent newtype (`modes.rs:26`, with the PORT NOTE at
`:19` explaining the oracle's open `enum(c_int) { _ }`); `rustty_color_scheme_report_encode`,
`rustty_terminal_compress`, `rustty_build_info`, `rustty_sys_set`, `rustty_key_event_set_*` and
`rustty_mouse_event_set_*` all take a raw `c_int` and narrow with an explicit `match`; and
`size_report::Size` is a plain all-integer `#[repr(C)]` struct with no validity invariant.

`ScrollViewportTag` is Phase 3's responsibility (in-range). Recommended shape, matching the
`compress` precedent: change `ScrollViewportC.tag` to `c_int` and narrow inside
`scroll_viewport` with the same explicit `match` used at `terminal.rs:922-926`; keep the
existing `const _: () = assert!(size_of::<ScrollViewportTag>() == size_of::<c_int>())` so the
ABI is provably unchanged. The other three should be swept in the same pass (they are cheap and
the pattern is identical) but are pre-existing, not caused by this range — do not misattribute
them.

DG5 is unaffected: no wrappers are being proposed, and every export stays plain `extern "C"`.

### 4.6 `include/rustty.h` — confirmed pre-existing defect, reproduced

```
$ printf '#include <rustty.h>\nint main(void){return 0;}\n' > /tmp/rh_probe.c
$ clang -fsyntax-only -Wall -Iinclude /tmp/rh_probe.c
include/rustty.h:2455:1: error: unknown type name 'Opaque'
 2455 | Opaque *rustty_wasm_alloc_opaque(void);
include/rustty.h:2457:30: error: unknown type name 'Opaque'
 2457 | void rustty_wasm_free_opaque(Opaque *ptr);
2 errors generated.
```

**Root cause.** `crates/rustty-lib/src/allocator/convenience.rs:30` defines
`pub type Opaque = *mut c_void;` in a **sibling crate**. `crates/rustty-vt-c/cbindgen.toml`
sets `parse_deps = false` (deliberately — parsing the vt crate tree is pathologically slow), so
every cross-crate C-ABI type is hand-pinned in `after_includes`. `Opaque` was **missed**: it is
neither in `after_includes` nor in `[export] exclude`, hence cbindgen's `WARN: Can't find
Opaque` and the dangling type name.

**Fix (fold into the plan).** Add to `crates/rustty-vt-c/cbindgen.toml` `after_includes`:

```c
/** Opaque pointer slot for the wasm allocator convenience API. C: void* */
typedef void *Opaque;
```

and `"Opaque"` to `[export] exclude`, then regenerate with
`cargo run -p xtask -- header-gen`. This yields `void** rustty_wasm_alloc_opaque(void)` /
`void rustty_wasm_free_opaque(void** ptr)`, byte-matching the curated
`include/rustty/vt/wasm.h` (which is already identical to upstream's
`ghostty_wasm_alloc_opaque` returning `void**`) and upstream's
`pub const Opaque = *anyopaque` at `src/lib/allocator/convenience.zig:17`. Then add the
compile probe to A5 (§5).

**Why A5 never caught it:** A5 step (a) compiles only `<rustty/vt.h>`; step (b) only *greps*
`include/rustty.h` for symbol names, never compiles it.

### 4.7 Examples — exhaustively diffed, one systematic gap

All 26 C/C++ example `src/` trees are **byte-identical** to upstream head (after token rename)
**except** for the `rustty_terminal_new` call, which every one of them still makes with the
`RusttyTerminalOptions` struct. That is exactly `03d5fa268` and nothing else. Concretely, the 16
sites needing the constructor change:

```
example/c-vt-cmake/src/main.c:15            example/c-vt-cmake-cross/src/main.c:15
example/c-vt-cmake-static/src/main.c:15     example/c-vt-colors/src/main.c:95
example/c-vt-compression/src/main.c:38      example/c-vt-effects/src/main.c:82
example/c-vt-formatter/src/main.c:15        example/c-vt-grid-ref-tracked/src/main.c:33
example/c-vt-grid-traverse/src/main.c:15    example/c-vt-kitty-graphics/src/main.c:80
example/c-vt-render/src/main.c:34           example/c-vt-selection/src/main.c:57
example/c-vt-selection-gesture/src/main.c:62 example/c-vt-stream/src/main.c:15
example/cpp-vt-stream/src/main.cpp:14       example/wasm-vt/index.html:214
```

`example/wasm-web-canvas/index.html:184` is a rustty-only example and needs the same treatment
for consistency. `c-vt-compression/src/main.c` additionally gains the
`rustty_terminal_set(…, OPT_SCROLLBACK_MAX_BYTES, &n)` call upstream moved the limit into.

Two more example-side items:
- `example/c-vt-effects/README.md` lags upstream `634ef7198`: it lists only
  `write_pty, bell, title_changed` and omits `clipboard_write` plus the "one atomic write with
  decoded, binary-safe MIME representations rather than protocol-specific OSC 52 data"
  paragraph. (`src/main.c` **is** correct — the callback is at `:42-71`.) Cost S.
- Upstream has `example/README.md` (+`swift-vt-xcframework`, `zig-*`). rustty has no
  `example/README.md`. Out-of-scope-ish; flag only.

**Convention confirmed and respected:** rustty examples are `README.md` + `build.sh` + `src/`
(no `build.zig`/`build.zig.zon`), per `.port/phase-2/EXAMPLES_PORT.md`. Upstream's
`build.zig`/`build.zig.zon` for `c-vt-color-scheme` and `c-vt-compression` are correctly **not**
ported; the corresponding `build.sh` files exist. Every example README's build paragraph and
`./build.sh` usage line legitimately differs from upstream — that is **not** drift.

### 4.8 `src/lib/**` — all N/A, verified rather than assumed
`compat/{fd,thread,reader,file,testing}.zig` are vendored Zig-0.15/0.17 stdlib shims;
`enum.zig`/`struct.zig`/`union.zig` changes are the `@Type`→`@Enum` builtin migration plus
`checkGhosttyHEnum`'s log-format fix. None has a Rust counterpart: rustty hand-authors each
`#[repr(C)]` C projection instead of generating it at comptime. **One consequence worth
recording:** upstream's `checkGhosttyHEnum` is a *test* that validates every Zig enum field's
integer against the matching `GHOSTTY_…` macro translated out of the real header. Rustty has
**no equivalent** — no gate compares a `#[repr(C)]` enum's discriminants to the header's
enumerator values (§5).

---

## 5. Gate coverage map

### What A5 (`.port/tools/gates/check_rustty_vt_headers.sh`) actually does

| step | what it checks | what it cannot catch |
|---|---|---|
| (a) headers compile | `clang -fsyntax-only -Wall -Iinclude` on a TU that `#include <rustty/vt.h>` | Anything about `include/rustty.h` (never compiled). Nothing about *completeness* — a header missing a decl compiles fine. |
| (b) declared+cbindgen ⇒ exported | builds `vt_decl` (lowercase `rustty_*` tokens in `include/rustty/vt*`), `vt_cbindgen` (same grep over `include/rustty.h`), intersects them, and requires `intersection ⊆ exports` (native `nm` ∪ wasm module exports) | **One-directional.** ① A symbol upstream has that is missing from *both* the header and the crate is invisible — this is exactly how `rustty_render_state_begin_update`/`_end_update` hide. ② A symbol in `vt.h` but absent from cbindgen output silently *shrinks* the intersection instead of failing (vacuous-shrink hazard). ③ An export with no header declaration is not flagged. ④ Nothing compares against the **oracle's** symbol set. |
| (c) struct layouts | node `.port/tools/check-vt-struct-layouts.mjs`: for each key of `rustty_type_json()` that also appears as a `Rustty…` token in the headers, emit `_Static_assert(sizeof(T)==N)` and per-field `offsetof` asserts, then compile | **Driven by `type_json`, not by the header.** A struct typedef'd in the header but never registered in `crates/rustty-vt-c/src/types.rs` gets **zero** assertions, silently. Structs `type_json` models as one whole-struct `opaque` blob get sizeof-only (no field offsets). And the success line prints the *field-offset* count only, so a run that produced only `sizeof` asserts would print `0 field offsets … OK` and pass. |

**Non-vacuity, verified statically (no build):** reproducing step (b)'s sets from the
checked-in files gives `vt_decl` = 188 tokens, `vt_cbindgen` = 183, intersection = **183** — so
A5(b) really does cross-check 183 symbols today. Not vacuous. The 5 header tokens outside the
intersection are `rustty_mode_new` / `rustty_mode_ansi` / `rustty_mode_value` (function-like
macros, correctly absent from the ABI) and two prose artefacts, `rustty_osc_` and
`rustty_vt_osc_free` (`include/rustty/vt/osc.h:88,99`). **`rustty_vt_osc_free` is a doc typo
inherited verbatim from upstream `ghostty_vt_osc_free` — R1-faithful; do not "fix" it.**

### Gates that do NOT cover this area (named to stop the recurring confusion)

- **A4 = `check_embed_h_abi.sh`** — the `rustty-embed.h` sret/`@embed-abi` gate (the `TriggerC`
  gate). It enumerates aggregate typedefs and by-value-aggregate returns in the **embedding
  umbrella** only. It never reads `include/rustty/vt/**`. It is *not* "the vt ABI gate".
- **A3 = `check_rustty_h_abi.sh`** — validates `include/rustty-embed.h` against the pinned
  oracle `include/ghostty.h` via `CABI_SYMBOLS.tsv`. `include/ghostty.h` is **unchanged in this
  range**, so A3 is unaffected — and it does not cover vt.
- **A1 = `check_symbol_manifest.sh`** — the macOS Swift-app manifest (90 `GHOSTTY_API`
  entrypoints of the embedding header). Not vt.
- **`_vt_header_baseline.txt` — CONFIRMED a dead artifact.**
  `grep -rn '_vt_header_baseline\|_vt_mismatch_report' .port/ xtask/ Makefile` → **no hits**.
  It is a 156-symbol list frozen 2026-07-15 13:31 (vs 183 live exports today), and
  `_vt_mismatch_report.txt` (13:41) is its stale output. **No gate reads either.** Delete both,
  or repurpose deliberately — an unread "baseline" is worse than none because it invites a
  planner to cite it as coverage (which is exactly what happened).
- **G1b = `check_test_counts.py`** — greps the **live oracle working tree**, which is pinned at
  `4749c4e93`. Today it compares rustty-vt-c against the *baseline* 307 zig `test{}` in
  `src/terminal/c`, so it cannot see any of this range's new tests. Concrete numbers:
  baseline **307** → head **363**; rustty-vt-c has **354** `#[test]`. **When the pin advances,
  G1b flips RED for `rustty_vt_c` (354 < 363).** That is a genuine, useful signal — plan for it
  rather than being surprised.
- **DG5 (`crates/rustty-apprt-c/tests/panic_policy.rs`)** — real and non-vacuous: `scan_dir_for_c_unwind`
  covers `crates/rustty-vt-c/src`, and the companion `exported_c_abi_panic_aborts_host`
  subprocess test proves SIGABRT, so the gate cannot pass for the wrong reason. Nothing in this
  range touches it. **No per-function wrappers are proposed anywhere in this assessment.**

### Where a new or strengthened gate is needed

Ranked. Each is written so it **fails on today's tree** (a gate that cannot fail is worthless —
this program has already found 5+ vacuous assertions).

| # | gate | what it must do | fails today on |
|---|---|---|---|
| **1** | **A5(d) — oracle vt-export parity** (new). The single highest-value addition, and the one hole that let this range's gap through. | Extract the oracle's export set from `$ORACLE_TREE/src/lib_vt.zig`'s `@export(&…, .{ .name = "ghostty_…" })` lines (sourcing `.port/tools/parity_names.env` for the `ghostty_`→`rustty_` map), and diff **both directions** against rustty's live `nm`+wasm exports. Print the missing set; exit 1 if non-empty. Must BLOCK (exit 3) if the oracle checkout is absent — never silently green. | the 2 `render_state_*` symbols. And it structurally cannot go vacuous, because the *oracle* drives the loop, not rustty's own artifacts. |
| **2** | **A5(a′) — compile `include/rustty.h` standalone** (one line). `printf '#include <rustty.h>\nint main(void){return 0;}\n' \| clang -fsyntax-only -Wall -Iinclude -x c -` | the 2 `unknown type name 'Opaque'` errors of §4.6. Land the cbindgen fix and this gate together. |
| **3** | **A5(c′) — make the layout gate header-driven, not `type_json`-driven.** Enumerate `typedef … } Rustty…;` aggregates from `include/rustty/vt/**` and REQUIRE each to be a key of `rustty_type_json()`; treat "in the header, absent from `type_json`" as FAIL, not as "skip". Also count and print the `sizeof` asserts separately from the `offsetof` ones so a zero-assert run cannot read as green. | nothing today (rustty's 34 registered structs cover its headers), but it fails the moment `RusttyTerminalDesktopNotification`/`RusttyTerminalProgressReport` are typedef'd without a `types.rs` row — i.e. it guards precisely the work Phase 3 is about to do. |
| **4** | **enum-discriminant parity** (new; the missing analogue of upstream's `checkGhosttyHEnum`). For every `Rustty…` enum in the curated headers, emit `_Static_assert((int)RUSTTY_X_Y == <n>)` from the Rust `#[repr(C)]` discriminants and compile. | nothing today; it is the gate that would have caught a mis-numbered `OPT_SCROLLBACK_MAX_BYTES = 27` / `DATA_… = 33..35` — the exact shape of the work ahead. Cheap. |
| **5** | **closed-enum-across-ABI lint** (new; guards §4.5). Grep exported `extern "C" fn` signatures in `crates/rustty-*-c/src` for parameter types that resolve to a non-`repr(transparent)` fieldless `#[repr(C)]` enum (directly or as a field of a by-value `#[repr(C)]` struct); require an allowlist entry with a justification. | the 4 exports in §4.5 — so it is provably non-vacuous the day it lands. Run it as PROGRESS while the sweep happens, then HARD. |
| **6** | **example compile+run smoke.** `example/*/build.sh` links `target/release/librustty_vt_c.dylib` and only builds it **if absent** — a stale release dylib silently produces a passing link against yesterday's ABI. Any example gate must `cargo build -p rustty-vt-c --release` unconditionally first (or `rm` the dylib). | would fail immediately after `03d5fa268`'s constructor change if the 16 call sites are not updated in the same commit. |

---

## 6. Cross-area dependencies, ordering, open questions

### 6.1 Cross-area dependencies (this area cannot land alone)

| C-surface item | blocked on | area |
|---|---|---|
| `rustty_render_state_begin_update` / `_end_update` (`446f80f4e`) | `render.RenderState::begin_update/end_update` + `pending_styles` + `page::Mask` (`src/terminal/render.zig` +919, `page.zig` +274, `Terminal.zig` −91) — **not ported in `crates/rustty-vt/src/render.rs` either** | A |
| `resize` delegation (`89b103dd5`) | `TerminalStream::resize` | D (stream) / A |
| `OPT_SCROLLBACK_MAX_BYTES/LINES`, `DATA_SCROLLBACK_MAX_BYTES/LINES` (`03d5fa268`, `a27e04e8f`, `86f81fb5b`) | PageList byte/line prune limits + configured-limit readers | A (`PageList.zig` +5088) |
| `DATA_VT_PROCESSING_ERROR` (`439d35e27`) | `stream.handler.semantic_failure` | D |
| kitty temp-file **directory** medium (`e8525c0fd`(iii), `b988efcfe`, `960c2cca5`) | `kitty_images.image_limits.temporary_file` → `.enabled{.directory}` | D (kitty internals) |
| `setDefaultCursorStyle/Blink` (`c594031d5`) | `Terminal` cursor-default state + `DECSCUSR` routing | A |
| `RUSTTY_MODE_VISIBILITY_REPORT` 2033 (`6c8c07981`) | the mode entry in rustty-vt's mode table | A/D |
| DECRQSS through `write_pty` (`39ae85f04`) | DCS handler reaching the C stream's write_pty | D |
| `grid_ref` compressed-page boundary (`421fe8dab`) | **already satisfied** — `pagelist::Node::page()` exists (a concurrent lane owns `crates/rustty-vt/src/{compress,pagelist}/`; **read-only** here) | B |

### 6.2 Recommended ordering

**Wave C0 — independent, do now (no cross-area blockers).**
1. `include/rustty.h` `Opaque` fix + A5(a′) compile probe (§4.6, gate #2). *Unblocks nothing but removes a shipped defect and closes a gate hole in one commit.*
2. New gate A5(d) oracle-export parity (gate #1) — land it RED, as the tracker for the remaining work.
3. `ScrollViewportTag` narrowing (§4.5, in-range soundness, UB) + the 4-export sweep + gate #5.
4. `RUSTTY_MODE_VISIBILITY_REPORT` header macro — only after the mode exists in rustty-vt (else it's a lie in the header).
5. `421fe8dab` stale reference comment in `grid_ref.rs:227-232`.
6. `aea63d71f` grapheme test table in `render/tests.rs:897`.
7. `example/c-vt-effects/README.md` clipboard paragraphs; `include/rustty/vt.h` include-order.
8. Delete `_vt_header_baseline.txt` + `_vt_mismatch_report.txt`.
9. Gates #3 and #4 (header-driven layout; enum-discriminant parity) — land **before** Wave C1 so C1's new typedefs are guarded on arrival.

**Wave C1 — the constructor break, ONE atomic commit** (`03d5fa268` + `a27e04e8f` +
`86f81fb5b` + `5fd2973b9` + `2729996ea`). Needs area A's scrollback limits first. Touches
`terminal.h`, `terminal.rs`, `types.rs` (delete the `RusttyTerminalOptions` row + port the
negative assertion), 16 example sites, `wasm-vt/index.html`, `wasm-web-canvas/index.html`,
`key.h`/`mouse.h` doc snippets. **Must not be split** — a header/impl skew here is an
ABI-parameter mismatch, the same crash class as the sret bug.

**Wave C2 — effects.** `c3655ba25` + `47d602c42` + `628adaf30`. Both new sized structs must be
registered in `types.rs` in the same commit (gate #3 will enforce it once landed).

**Wave C3 — cross-area-gated.** `446f80f4e` (after area A's render rework), `89b103dd5` +
`39ae85f04` (after area D's stream), `439d35e27`, `c594031d5`,
`e8525c0fd`(iii)+`b988efcfe`+`960c2cca5` (after area D's kitty temp-dir medium).

**Wave C4 — pin advance.** Expect G1b to go RED for `rustty_vt_c` (354 < 363) the moment the
oracle pin moves; the missing tests are enumerated per-commit in §2 (DECRQSS, begin/end update
×2, `vt_processing_error`, the resize pair, the cursor-defaults RIS case, the constructor test
updates, the grapheme table).

### 6.3 Open questions, with recommendations

1. **Does `03d5fa268` land as a break or with a compat shim?** *Recommendation: break.* Upstream
   deleted the type outright, R1 fidelity is the rule, and librustty-vt's C ABI has no external
   consumer contract equivalent to the macOS app's append-only rule (that constraint is
   `rustty-embed.h`/A1's, and `include/ghostty.h` is untouched by this range). A shim would
   create a permanent divergence in both the header and `type_json`.
2. **`include/rustty/vt.h`'s extra `color_scheme` doc lines — keep or revert to upstream?**
   *Recommendation: keep, and record the divergence.* They are correct Doxygen. But if a
   mechanical header-diff gate is ever built against the oracle, it needs a documented
   allowlist for exactly this and for the `rustty.sh` identity string.
3. **`rustty.sh` vs upstream's `rustty.org` (token-renamed `ghostty.org`).** Whose call? The
   `.port/tools/PARITY_NAMING.md` map has no URL entry. *Recommendation:* add
   `PORT_URL`/`ORACLE_URL` to `parity_names.env` so the choice is data, not a stray literal.
4. **Should `example/README.md` be ported?** Upstream has one; rustty doesn't. It documents the
   `build.zig` convention rustty deliberately replaced. *Recommendation:* write a rustty-native
   one describing the `build.sh` convention; do not port upstream's text.
5. **Is the `39ae85f04` DECRQSS→`write_pty` path already functional in rustty** (only the test
   missing) or genuinely absent? `crates/rustty-vt/src/dcs.rs` has the DECRQSS machinery and
   `crates/rustty-vt/src/terminal.rs:4374` the attribute-printing responder, but I did **not**
   trace whether the C `vt_write` path (`crates/rustty-vt-c/src/terminal.rs:846`, `stream.next_slice`)
   delivers the reply through the `write_pty` trampoline. *Recommendation:* port the upstream
   test **first** — it is 20 lines and it answers the question definitively either way. This is
   the one item where I am reporting a bounded uncertainty rather than a verdict.
6. **`960c2cca5` must not be lost.** It is `N/A` only because the buggy code doesn't exist yet.
   Whoever ports `e8525c0fd`(iii) must land the fixed form (bounds-check, then copy exactly
   `v.len`, and slice the *directory* as `tmp_dir_path[0..v.len]`). Suggest a `PORT NOTE`
   citing `960c2cca5` at that site so a future reader can't re-introduce it.
