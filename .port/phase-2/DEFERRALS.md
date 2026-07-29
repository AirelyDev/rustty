# rustty-vt — Tracked Deferrals (must be completed by the named sub-batch)

Hard tracker for code/tests/comments deferred from an earlier sub-batch because a
dependency wasn't ported yet. Each MUST be completed by its target sub-batch — the
target workflow consumes this file. Nothing here is "dropped" (R1); it is scheduled.

## Deferred to vt-12 (Terminal/Screen cluster)

From **vt-7 (kitty)** — these need `Screen`/`Terminal`/`ImageStorage`:

1. ~~**`kitty/graphics_unicode.rs` — `render_placement()` + `grid()` methods.**~~
   **RESOLVED (vt-12c-kitty).** `render_placement` (coordinate-space / scaled-rect /
   aspect-fit logic) + `grid` (placement lookup by id, else first virtual placement;
   image-size fallback; `CellCountInt` bounds check) are ported verbatim to
   `kitty/graphics_unicode.rs` from `ghostty/src/terminal/kitty/graphics_unicode.zig`,
   carrying all **58 inner comments**. Scoped graphics_unicode.zig G3b → missing=0.
   - **14 ported tests**: 11 placement-iterator (`unicode placement: …`) + 3 render
     (`unicode render placement: dog 4x2 / 2x2 with blank cells / 1x1`), all
     assertion-for-assertion and passing (1380 → 1394 total, no regression).
   - Memory safety: `unicode_placement_single_row_col` + `unicode_render_placement_dog_1x1`
     are Miri-clean (UB=0) in isolation; all 14 pass under ASan.

2. ~~**`kitty/graphics_storage.zig` (1364 loc, 19 tests)** — full file. Needs `Screen`.~~
   **RESOLVED (vt-12c-kitty).** `ImageStorage` (+ `Placement`/`PlacementKey`/
   `Location`/`pixelSize`/`gridSize`/`rect`/`delete`/eviction) is fully ported in
   `kitty/graphics_storage.rs`; all **19 tests** are ported assertion-for-assertion
   and pass (no regression in the 1353 prior). `Screen.kitty_images` is wired to the
   real `ImageStorage` (init `setLimit`/`image_limits`, deinit, reset, and the 4
   `dirty=true` scroll/resize hooks) under the `crate::options::KITTY_GRAPHICS` gate;
   the self-referential `&mut Screen` borrows are made sound via `mem::take`
   move-out/move-back (Miri-clean). Scoped graphics_storage.zig G3b → missing=0.

3. ~~**`kitty/graphics_exec.zig` (575 loc, 8 tests)** — full file. Needs `Terminal`.~~
   **RESOLVED (vt-12c-kitty).** `execute` (+ `query`/`transmit`/`display`/`delete`/
   `loadAndAddImage`/`encodeError`) is ported verbatim to `kitty/graphics_exec.rs`;
   all **8 tests** are ported assertion-for-assertion and pass (no regression in the
   1372 prior → 1380 total). `Terminal::kitty_graphics` forwards to `execute`, and
   `set_kitty_graphics_size_limit`/`set_kitty_graphics_loading_limits` iterate
   `screens.all` under the `crate::options::KITTY_GRAPHICS` gate (move-out/move-back
   for the storage-borrows-screen aliasing). Scoped graphics_exec.zig G3b → missing=0.
   Memory safety: the 2 data-less tests (`a=p` query/display) are Miri-clean; the 6
   that transmit `;`-data cross the simdutf base64 C FFI (Miri can't execute foreign
   functions — R6) and are covered by ASan (8/8 graphics_exec tests, 0 findings).

4. **`kitty/graphics.rs` re-exports** — `ImageStorage` (from `graphics_storage`)
   **RESOLVED (vt-12c-kitty)**: `pub use storage::ImageStorage` restored + exercised
   in the aggregator `ref_all_decls` test. `execute` (from `graphics_exec`)
   **RESOLVED (vt-12c-kitty)**: `pub use exec::execute` restored + exercised in the
   aggregator `ref_all_decls` test (typed `fn` reference).

**vt-12 acceptance gain:** +~41 tests (19+8+14) and graphics_unicode G3b → 0 missing.

From **vt-11b (formatter production + Screen dump)** — **RESOLVED in vt-12c-fmt:**

5. ~~**ALL ~100 `formatter.zig` `test{}` blocks.**~~ **RESOLVED (vt-12c-fmt).**
   All 100 formatter `test{}` blocks are ported as `#[test]`, assertion-for-
   assertion, in `crates/rustty-vt/src/formatter.rs` (`mod tests`). Each builds its
   fixture via `Terminal::init(...)` + `t.vt_stream().next_slice(...)` (now
   available) and asserts the round-trip/output. Span: `"Page …"`, `"PageList …"`,
   `"Screen …"`, `"TerminalFormatter …"`, plus HTML/VT/style/hyperlink/charset/
   codepoint-map variants. Scoped formatter.zig G3b → missing=0.

6. ~~**`TerminalFormatter` (formatter.zig:141).**~~ **RESOLVED (vt-12c-fmt).**
   `TerminalFormatter::{init,format}` are filled (no more `todo!()`): the
   `terminal: *const Terminal` field is restored and `format` composes the
   already-ported `ScreenFormatter`/`PageListFormatter`/`PageFormatter` while
   emitting the `palette`/`modes`/`scrolling_region`/`tabstops`/`pwd`/`keyboard`
   Extra. The 5 `TerminalFormatter`-driven (`"Terminal vt …"`) Screen tests are
   ported alongside it.

**vt-12c-fmt acceptance gain (formatter):** +100 formatter tests (incl. the 5
`Terminal vt …` tests), `TerminalFormatter::{init,format}` → 0 `todo!()`, and
scoped `formatter.zig` G3b → 0 missing.

## Deferred Screen.zig tests (not-yet-ported test bodies) — vt-11/vt-12 cluster

~~From **vt-11 (Screen)** — 152 of 187 `Screen.zig` `test{}` blocks are ported; the
remaining ~35 are scheduled, not dropped (R1). 23 of these are flagged by G3b as
test-region "missing"...~~ **RESOLVED (vt-12c-screen).** All 23 deferred
`Screen.zig` test bodies below are ported as `#[test]`, assertion-for-assertion, in
`crates/rustty-vt/src/screen.rs` (`mod tests`), each carrying its inner comments
verbatim. The OOM/capacity edge tests drive `increaseCapacity`/`setAttribute`-split/
`cursorSetHyperlink` via direct page-churn + style-map-fill fixtures (no tripwire
needed — these exercise the real OutOfSpace path, not allocator fault-injection; the
one tripwire-based alloc-failure test, `selectionString map allocation failure
cleanup`, was already ported in vt-11d). The select-replaces-pins test uses
`Selection`/`countTrackedPins`. 1394 → 1417 tests (no regression). Scoped
`Screen.zig` G3b → **missing=0** (the prior residual). The 3 remaining production
comments (the elided `std.valgrind.runningOnValgrind()` Valgrind-hook comments in
`assertIntegrity`) are now preserved verbatim in the screen.rs PORT NOTE → G3b GREEN.

- ~~`Screen: cursorDown to page with insufficient capacity` (15)~~ RESOLVED
- ~~`Screen setAttribute increases capacity when style map is full` (13)~~ RESOLVED
- ~~`Screen: increaseCapacity non-cursor page returns early` (11)~~ RESOLVED
- ~~`Screen: increaseCapacity cursor style ref count preserved` (6)~~ RESOLVED
- ~~`Screen: increaseCapacity cursor with both style and hyperlink preserved` (5)~~ RESOLVED
- ~~`Screen: increaseCapacity cursor hyperlink ref count preserved` (3)~~ RESOLVED
- ~~`Screen setAttribute splits page on OutOfSpace at max styles` (6)~~ RESOLVED
- ~~`Screen: cursorSetHyperlink OOM + URI too large for string alloc` (6)~~ RESOLVED
- ~~`Screen: hyperlink cursor state on resize` (3)~~ RESOLVED
- ~~`Screen: hyperlink reuse` (2)~~ RESOLVED
- ~~`Screen: scrollback various cases` (5)~~ RESOLVED
- ~~`Screen: scrollback doesn't move viewport if not at bottom` (4)~~ RESOLVED
- ~~`Screen: scroll above same page but cursor on previous page last row` (5)~~ RESOLVED
- ~~`Screen: scroll above same page` (4)~~ RESOLVED
- ~~`Screen: scroll above with cursor on non-final row` (4)~~ RESOLVED
- ~~`Screen: scroll above same page but cursor on previous page` (3)~~ RESOLVED
- ~~`Screen: scroll above creates new page` (2)~~ RESOLVED
- ~~`Screen: scroll above no scrollback bottom of page` (1)~~ RESOLVED
- ~~`Screen: scroll down from 0` (1)~~ RESOLVED
- ~~`Screen: scrolling` (3)~~ RESOLVED
- ~~`Screen: scrolling when viewport is pruned` (2)~~ RESOLVED
- ~~`Screen: scrolling with a single-row screen with scrollback` (1)~~ RESOLVED
- ~~`Screen: select replaces existing pins` (1)~~ RESOLVED

**Acceptance gain (REALIZED):** porting these test bodies drove the Screen.zig
test-region G3b "missing" to 0; with the Valgrind-hook comments now preserved
verbatim, the full scoped Screen.zig G3b is GREEN (missing=0).

## Deferred to Dispatch/SIMD phase

From **vt-9 (stream)**:

1. **`stream.rs` — `Stream::next_slice` SIMD fast path (`nextSliceCapped` + the
   `simd.vt.utf8DecodeUntilControlSeq` loop).** Reference:
   `ghostty/src/terminal/stream.zig:471-577` (`nextSlice` SIMD arm +
   `nextSliceCapped`). **RESOLVED (Stage A.1).** The `todo!()` SIMD arm has been
   replaced with the faithful port of `nextSliceCapped` + `consumeAllEscapes` +
   `consumeUntilGround` + the `nextSlice` SIMD branch, wired to
   `rustty_simd::vt::utf8DecodeUntilControlSeq` (the now-ported `simd/vt.zig`
   analogue). The seam remains gated on `options::SIMD` (the reference's
   `comptime debug or !build_options.simd`): `SIMD` defaults to `false`
   (`cfg!(feature = "simd")`), so the scalar fallback — the reference's own
   non-SIMD arm — still runs unconditionally by default and the SIMD arm is
   exercised under `--features simd`. All 38 stream tests pass in both modes.
   The `cp_buf`/max-codepoints chunk-size consts and the `@setEvalBranchQuota`
   test-comptime comments are restored verbatim. `initAlloc` was already ported
   (stream.rs `init_alloc`) as part of closing the vt-9 comment-fidelity gap.

   **MIRI:** per `MIRI_EXCEPTIONS.md`, the SIMD arm calls `rustty-simd` C FFI
   (utf8 SIMD) which Miri cannot execute, so SIMD-arm tests are ASan-covered
   (R6 FFI→ASan); the scalar (default) tests remain Miri-clean.

## Deferred to rustty-vt-c (C ABI layer)

Embedded C-ABI declarations inside vt `.zig` files belong to the C FFI layer
(`terminal/c` → the separate `rustty-vt-c` crate, PORT_PLAN module 12), not the
pure-VT `rustty-vt` crate:

**STATUS 2026-06-25 — rustty-vt-c COMPLETE (Stage B DONE).** All 28 `terminal/c/`
files ported (307 ref tests, 308 rust), 156 non-WASM `rustty_*` C exports wired
(1:1 with lib_vt.zig; the 12 `ghostty_wasm_*` are WASM-gated, absent in the non-WASM
ABI), cbindgen `include/rustty.h` regenerated + C/C++ self-consistency compile clean,
ABI const-asserts pinned (ScrollViewport `[2]u64`, sgr Attribute `[8]u64`), the D3
Effects trampoline (offset_of! container_of, exposed-provenance) **Miri-clean (SB+TB)**,
**ASan-clean** (after fixing the SIMD test-buffer sentinel over-read, 387d04c). rustty-vt
untouched (2019). ScrollViewport + sgr-Attribute C-ABI surfaces are now RESOLVED here.

1. **`stream.zig` `c_union`** (the Action type's `lib.TaggedUnion(...)` C-ABI
   representation, ref stream.zig:228-234) — carries the
   "TODO: Before shipping an ABI-compatible libghostty, verify this." +
   "This was just arbitrarily chosen for now." comments. **STILL DEFERRED** (NOT
   resolved by Stage B): no `terminal/c/*.zig` file references `stream.Action`, so it
   is not part of the vt-c C export surface. Its `[16]u64`=128 size verification +
   const-assert land if/when an ABI-compatible `Action` is actually shipped.

## Pre-existing deferred seams (lower priority, noted)
- `sgr.rs` local `SepList` — can be retargeted to re-export `parser::SepList` (vt-5
  defined the canonical one). Cosmetic; tracked PORT NOTE in sgr.rs.
- `sys.decode_png` — PNG codec hook intentionally null in the VT crate (faithful;
  Ghostty tolerates null). A real decoder rides a later FFI crate.

## Rule
A sub-batch's verify MUST treat G3b "missing" that maps to an entry here as
EXPECTED (not RED), and MUST NOT add new deferrals without recording them here.

## ~~Deferred: apc cluster (font-blocked)~~ — RESOLVED (Stage A.3)

**RESOLVED (Stage A.3 — apc complete).** `apc.zig` + `apc/glyph/{request,
Glossary,execute,response}` are fully ported and gate-green. The D1 layering
(Option C, signed off) made `rustty-font` PURE & BELOW `rustty-vt`, so `rustty-vt`
deps `rustty-font` DIRECTLY (no crate cycle, no decoder trait seam): glyf-decode
is mandatory via `rustty_font::{opentype::glyf::Glyf, glyph::*}`. `apc.zig` (the
14-test `Handler` multiplexing kitty graphics + glyph) lands at
`crates/rustty-vt/src/apc/mod.rs` (all 14 tests pass, Miri-clean on 2, scoped G3b
missing=0). The glyph submodules (request 19t / Glossary 9t / execute 12t /
response 12t) were ported earlier; `apc/glyph/mod.rs` now carries the verbatim
`glyph.zig` header + re-exports. The original font-block BLOCKER (rustty-font was a
stub depending on rustty-vt) was dissolved by D1.

## Deferred: vt-12a Terminal/stream_terminal stubs (font-block continuation + C-ABI)

The vt-12a struct-first skeleton (`terminal.rs` <- `Terminal.zig`,
`stream_terminal.rs` <- `stream_terminal.zig`) lands with these stubs, all of
which CONTINUE existing deferrals (no new capability is being dropped):

- ~~**apc_handler ZST** (`stream_terminal::ApcHandler`)~~ **RESOLVED (Stage A.3).**
  `ApcHandler` is now a type alias for the real `apc::Handler`. The `apc_start`/
  `apc_put`/`apc_end` trait arms drive `start`/`feed`/`end`, and `apcEnd`
  dispatches the kitty graphics + glyph responses via `write_pty`. The previously
  apc-gated stream_terminal tests are un-deferred and ported: `full reset`,
  `glyph protocol APC with write_pty callback`, `kitty graphics APC response`,
  `kitty graphics via APC` (all pass). **stream_terminal apc_deferred → 0.**
- ~~**GlyphGlossary / GlyphRequest / GlyphResponse ZSTs** (`terminal.rs`)~~
  **RESOLVED (Stage A.3).** The ZST stand-ins are dropped; the field is the real
  `glyph::Glossary` and `Terminal::glyph_protocol` runs `glyph::execute` and sets
  `flags.dirty.glyph_glossary`. The `Terminal: glyph APC stores session glossary
  entries` test is ported and passes.
- **kitty graphics**: `Terminal::kitty_graphics` forwards to `Screen.kitty_images`
  (a deferred `ImageStorage` stub); reactivated in vt-12c-kitty (graphics_exec
  needs Terminal✓ which now exists). Feature-gated, no new deferral.
- **ScrollViewport C-ABI → rustty-vt-c** (NEW deferral): the reference
  `ScrollViewport = union(Tag)` carries a `lib.TaggedUnion` C-ABI surface
  (`C` / `CValue` / `cval`). In pure-VT the port is a plain Rust enum
  (`Top` / `Bottom` / `Delta(isize)`); the C-ABI tagged-union representation is
  hoisted into the `rustty-vt-c` crate alongside the other `lib.TaggedUnion`
  C surfaces, and is deferred until that crate is carved. No VT behavior is lost
  (the plain enum is the source of truth; the C-ABI is a pure FFI projection).
  G3b for `ScrollViewport.C`/`.CValue`/`.cval` maps here and is EXPECTED-missing.
  This also covers the `c_union = lib.TaggedUnion(...)` padding comments
  ("Padding: largest variant is isize (8 bytes on 64-bit)." and
  "Use [2]u64 (16 bytes) for future expansion."), which live on the deferred
  C-ABI tagged-union surface and are therefore EXPECTED-missing in `terminal.rs`.

## Deferred from vt-14 (input leaves + key)

The vt-14 input leaf slice (`input/{mouse,paste,key,function_keys}.rs` +
`input/key_mods.rs`) lands with these deferrals, all apprt/config-layer seams
(no VT behavior is dropped):

- **`Key::imgui_key` — dcimgui (cimgui) binding (apprt GUI-glue, R3).** The
  reference `key.zig` `imguiKey` maps a `Key` to a `cimgui.c.ImGuiKey_*`
  constant via `@import("dcimgui")`. dcimgui is an apprt-layer GUI binding, not
  a VT dependency. In the port it is gated behind the off-by-default `imgui`
  cargo feature (added to `rustty-vt/Cargo.toml`); the `imgui_key` body is a
  `unimplemented!()` placeholder and the full reference mapping is preserved
  verbatim as a comment for the future apprt port. **Target: apprt layer.** No
  `imguiKey`-specific `test{}` exists in the reference, so there is no skipped
  test (the SkipZigTest mirror pattern is N/A here).
- ~~**`input/key_mods.rs` is a PARTIAL port.**~~ **RESOLVED.** The `Mods` packed
  struct + VT-relevant methods (`int`/`empty`/`equal`/`keys`/`binding`/`unset`/
  `without_locks`/`ctrl_or_super` + `from_int`/`Side`/`Keys`/`SIDE_MASK` + the
  bit-layout `test{}`) remain in `rustty-vt`'s `input/key_mods.rs`; `Mod` + its
  `alias` table + `ModSide` also live there (added for `Binding.zig` trigger
  parsing, consumed by vt's `function_keys`/`key_encode` and `rustty-input`'s
  `binding`). The remaining config-dependent slice — `Mods.translation` (needs
  `config.OptionAsAlt`) and the `RemapSet` type with its `parseCLI`/`parse`/
  `finalize`/`clone`/`equal`/`formatEntry`/`apply`/`Mask` machinery (needs the
  config `formatter`) — is now PORTED in `rustty-input`'s `key_mods.rs`, since
  both seams are available (`rustty-input::config::OptionAsAlt` + `rustty-cfg-fmt`).
  `translation` is ported as a free fn (its `Mods` host lives in vt); the
  `AutoArrayHashMapUnmanaged(Mods, Mods)` map uses `rustty-zigstd` (gaining a
  faithful `sort`/`clone` for `finalize`/Config-clone). All 23 deferred `test{}`
  blocks — the 22 `RemapSet:` tests + `translation macos-option-as-alt` — are
  ported assertion-for-assertion (`check_test_counts.py` `input_key_mods`
  apc_deferred 23→0; both rust files count against the one zig file, 24≥23).
  Scoped G3b for `key_mods.zig` → missing=0.
- **`function_keys` `keys` test — `termio.Message.WriteReq.Small.Max`.** The
  reference test reads the termio small-message capacity (`MessageData(u8, 38)`
  ⇒ `38`); termio is an apprt/termio-layer module the reference explicitly keeps
  out of libghostty-vt. The ported test inlines the constant `38` (documented as
  the reference value) so it runs in-crate; no termio dependency is introduced.

## config C1 (leaf value-types) — oniguruma-gated test — RESOLVED

The config C1 leaf value-types (`rustty-config`:
string/RepeatableStringMap/io/path/command/conditional/ClipboardCodepointMap/
ErrorList/edit/file_load/url) are fully ported (36 non-deferred tests pass,
scoped G3b missing=0). The one previously-deferred test is now ported:

- **`url.zig` `url regex` test — oniguruma. RESOLVED.** The single
  `test "url regex"` in `config/url.zig` builds an `oni.Regex` from the ported
  `regex` string and asserts 80 match + 10 no-match cases. `oniguruma-sys` +
  `rustty-oniguruma` (the `oni` safe wrapper) have landed, so the test is now
  PORTED assertion-for-assertion into the `#[cfg(all(test, feature =
  "oniguruma"))]` module of `url.rs` (every match case's span + every no-match
  case's `Error.Mismatch`, faithful to the reference control flow). It is gated
  behind the off-by-default `oniguruma` cargo feature on `rustty-config`, which
  pulls in `rustty-oniguruma` (`oniguruma = ["dep:rustty-oniguruma"]`) — exactly
  as vt's string_map search surface is gated — so the C Oniguruma dependency
  stays out of the default build. `cargo test -p rustty-config --features
  oniguruma` runs the ported test (all 90 cases pass); the default
  `cargo test -p rustty-config` is unchanged (feature off). The `regex`
  string-assembly logic itself has no oniguruma dependency and is compiled
  always (`url::regex()` builds the byte-for-byte-identical pattern). **RESOLVED.**

## StringMap oniguruma search surface — RESOLVED (vt `oniguruma` feature wired)

`terminal/StringMap.zig`'s regex search surface (`searchIterator`/`SearchIterator`/
`Match`) and its 3 `searchIterator` tests were gated behind vt's off-by-default
`oniguruma` feature because the `oni` binding crate did not exist. `oniguruma-sys`
+ `rustty-oniguruma` (the `oni` wrapper) have landed and vt's `oniguruma` feature
is wired to them, so the gated surface now COMPILES and its tests PASS under
`cargo test -p rustty-vt --features oniguruma` (`string_map::tests::string_map_search_iterator`
+ the two `..._url_detection` / `..._url_with_click_position` tests). **RESOLVED.**
Three pre-existing bugs in the off-by-default gated code were fixed so
`--features oniguruma` builds + passes (default build stays 2019, unaffected):

- **`tmux/control.rs` (8 sites) — nonexistent `oni::RegexOptions::capture_group()`.**
  `capture_group` is a struct FIELD (rustty-oniguruma types.rs), not a ctor.
  Rewritten to the faithful struct-literal
  `oni::RegexOptions { capture_group: true, ..Default::default() }` (mirrors
  ghostty control.zig's `.{ .capture_group = true }`). Also (same gated blocks)
  the 8 `let region` bindings became `let mut region` — `oni::Region::deinit`
  takes `&mut self`; faithful to Zig's `var region`.
- **`string_map.rs` URL tests imported `crate::config::url` — a vt->config cycle.**
  config is ABOVE vt, so that edge is forbidden (check_acyclic.sh). CHOSE OPTION
  (a) (inline, preferred): the URL/path regex is inlined as a local test fixture
  `tests::url::regex()`, mirroring `config/url.zig`'s `regex` assembly
  byte-for-byte (a verbatim copy of the already-ported `rustty-config::url`
  string-build) with a config-layering PORT NOTE. No vt->config edge; both URL
  tests run in vt with identical behavior. The canonical copy stays in
  `rustty-config::url::regex()`.
- **`string_map.rs` move-after-use (E0382) in the gated tests.** `Match::selection`
  was `self` (consuming), so `match_.deinit()` (frees the `oni::Region`) could not
  run after `.selection()`; changed to `&self` — the faithful mirror of Zig's
  by-value, non-consuming `selection(self: Match)`. And `re.deinit()` after `re`
  was moved into `search_iterator(re)` is now `it.regex.deinit()` (the iterator
  owns the sole `Regex` handle; faithful to Zig where `re`/`it.regex` alias the
  same OnigRegex).

## input `Link.oniRegex` oniguruma seam — RESOLVED (`oni` wrapper landed)

`rustty-input`'s `link.rs` (`ghostty src/input/Link.zig`) `oniRegex` was a deferred
seam (`unimplemented!()` + a placeholder `OniRegex` enum) because the `oni` binding
crate did not exist. `oniguruma-sys` + `rustty-oniguruma` (the `oni` wrapper) have
landed, so the seam is filled with the faithful
`oni::Regex::init(&self.regex, oni::RegexOptions::default(), oni::Encoding::utf8(),
oni::Syntax::default(), None)` returning `Result<oni::Regex, oni::Error>`, and the
placeholder enum is removed. **RESOLVED.** Unlike `rustty-vt`/`rustty-config` (which
gate oniguruma behind an off-by-default feature because the standalone libghostty-vt
build disables it), the reference imports `oniguruma` UNCONDITIONALLY in
`src/input/Link.zig` — `src/input/` is always built with oniguruma present — so
`rustty-oniguruma` is a **NON-optional** dependency of `rustty-input` and `oni_regex`
is always compiled (no cargo feature gate). This adds a transitive C-FFI edge
(oniguruma-sys), so `rustty-input` is now an FFI crate for G6 (routed to ASan, not
Miri; added to `check_leaks.sh` `FFI_CRATES`). `Link.zig` has **0 `test{}` blocks**,
so no test is un-deferred (`check_test_counts.py` `input_link` stays a
normally-counted 0 — never a deferral/exclusion); the vt `string_map.rs` tests
already exercise the `oni::Regex::init` surface.

## RESOLVED (2026-07-10, G1/G2 honest-green pass): DCS <-> tmux enter/exit wiring — test `dcs::tests::tmux_enter_and_implicit_exit`

The wiring sub-batch targeted below LANDED: `dcs.rs`'s `TmuxState`/`TmuxCommand`
are now the real `tmux::ControlParser`/`ControlNotification`; the `b'p'` `'tmux`
hook builds the parser (`ControlParser::init_options(self.max_bytes, 128)`) and
returns `.{ .tmux = .enter }`; `try_put`'s tmux prong forwards to
`ControlParser::put`; `unhook`'s tmux prong deinits the parser and returns
`.{ .tmux = .exit }`. The `TMUX_DCS_WIRED` marker const is deleted and both
`DEFERRED ASSERT`s are restored as live asserts — the test carries all 4 of the
reference's expects; it runs live under `--features oniguruma` and skips
(mirroring `error.SkipZigTest`) in the default build. Original entry kept below
for history:

`dcs.rs`'s `TmuxState`/`TmuxCommand` are still `()` stubs (the reference's
`tmux.ControlParser`/`ControlNotification`, `= void` while `tmux_control_mode` is
off), and the DCS `b'p'` `'tmux` hook is an `unreachable!()` placeholder — the real
DCS<->tmux `enter`/`exit` construction (real `ControlParser` buffer + `enter`/`exit`
`ControlNotification`) lands in a later vt tmux control-mode sub-batch. Enabling
`--features oniguruma` flips `build_options::TMUX_CONTROL_MODE` (`= ONIGURUMA`) on,
which would otherwise make the faithful `tmux_enter_and_implicit_exit` test assert
against that stub (it panicked at the `unreachable!()`). The test now ALSO defers
while `!TMUX_DCS_WIRED` — a `false` marker const in `dcs.rs` `mod tests`, flipped
to `true` by the wiring sub-batch — so it stays a passing no-op in BOTH the default
(2019) and `--features oniguruma` (2030) builds. ASSERTION INVENTORY (honest count,
VALIDATION_REPORT §3e): the dormant body carries 2 of the reference's 4 expects
(`cmd == .tmux`, twice); the 2 VARIANT asserts (`cmd.tmux == .enter`,
`cmd.tmux == .exit`) are inexpressible against the `()` `TmuxCommand` stub and are
held as `DEFERRED ASSERT` comment lines at their sites — the wiring sub-batch MUST
restore them as live asserts. This is NOT one of the StringMap gaps above — it is a
pre-existing stub exposed by turning the flag on. **Target: the vt tmux
control-mode DCS wiring sub-batch (replaces the `()` stubs + `unreachable!()` and
flips `TMUX_DCS_WIRED` to `true`, enabling the test body).**

## Config C2 seams (Config.zig value-type DI seams)

The three Config.zig value-type dependency seams were resolved/scheduled as
follows (R4 — break crate cycles with minimal glue, never weaken):

- ~~**`RepeatableCommand` (Config.zig:8685) — `Command: cli.args.AutoStruct`
  seam.**~~ **RESOLVED.** `RepeatableCommand.parseCLI` calls
  `cli.args.parseAutoStruct(inputpkg.Command, ...)`, which needs the *external*
  `rustty_input::command::Command` to implement `rustty_args::args::AutoStruct`.
  Both types are foreign to `rustty-config`, so the orphan rule forces the impl
  into `rustty-input` (alongside `Command`, in `command.rs`). That impl + the
  `parseTaggedUnion(Action)` path it needs (a new `pub(crate)`
  `Action::parse_auto_tagged_union` in `binding.rs`, with faithful `AutoStruct`
  impls for the `CursorKey`/`WriteScreen`/`SplitResizeParameter` struct payloads)
  are now ported; `rustty-input` deps `rustty-args` (acyclic — `rustty-args` has
  no `rustty-input` dep). `RepeatableCommand.{init,parseCLI,formatEntry}` are
  filled in `rustty-config` (consuming `command::{defaults,defaults_c}`,
  `Command.cval`, `Command.format`); `std.zig.fmtString` was ported as
  `rustty_args::args::zig_string_escape`. All **7 reference `RepeatableCommand`
  `test{}` blocks** are ported assertion-for-assertion and pass (parseCLI,
  formatConfig empty/single/multiple, parseCLI commas, cval, cval cleared).

- **`RepeatableCodepointMap` (Config.zig) — RESOLVED.**
  `RepeatableCodepointMap` wraps `fontpkg.CodepointMap` (`font/CodepointMap.zig`),
  now ported **PURE** in `rustty-font` (`crates/rustty-font/src/codepoint_map.rs`).
  The discovery-descriptor seam (`Descriptor`) was extended from `family`-only to
  carry `family` + `monospace` (the two fields config's `parseCLI` sets:
  `.{ .family = valueZ, .monospace = false }`), faithful to `discovery.Descriptor`
  (discovery.zig:43,63). rustty-font stays pure — the full fontconfig/CoreText
  discovery lands later in the shaper tier; no C/vt edge was pulled in. Config's
  `parseCLI`/`clone`/`equal`/`formatEntry` are now filled against the map's
  `add`/`entries`/`descriptor` API (the map gained pure `len`/`is_empty`/`entries`
  read-accessors mirroring the reference's public `list` field). The 4 `test{}`
  blocks ("parseCLI", "formatConfig single/range/multiple", Config.zig:8160-8241)
  are ported assertion-for-assertion in `value_types.rs`; `check_test_counts.py`
  `config_struct` apc_deferred dropped 4 → 0. NOTE: `equal` compares `family` by
  CONTENT (not `std.meta.eql`'s slice pointer+len identity) — a deliberate,
  documented divergence (pointer identity is neither reproducible nor meaningful
  across the port; content equality is the intended `changed`/`deepEqual`
  semantics).

- **`theme.zig` `.resources` arm — `internal_os.ResourcesDir`: dependency is REAL.**
  Confirmed for the Machinery phase: `rustty_os::{resourcesDir, ResourcesDir}`
  (`crates/rustty-os/src/resourcesdir.rs`) is a fully-ported, real implementation
  (env-var `RUSTTY_RESOURCES_DIR` lookup + exe-dir sentinel climb across
  macOS/Linux/BSD + the debug/release ordering), NOT a stub. So when `theme.zig`
  is ported in the Machinery phase, its `.resources` arm has a real `ResourcesDir`
  to build on — **no `.resources` deferral is needed.**

## rustty-renderer R-b/R-c — shadertoy + size + link

### `renderer/link.zig` oniguruma deferral — RESOLVED

`renderer/link.zig` (the `oni.Regex`-backed renderer link `Set`/`renderCellMap`)
was DEFERRED behind the Oniguruma binding (`oni.Regex` was unported tree-wide). It
is now RESOLVED: `oniguruma-sys` + the `oni` safe wrapper (`rustty-oniguruma`) have
landed, so `crates/rustty-renderer/src/link.rs` ports `Link`/`Set.fromConfig`
(real `oni::Regex::init`) + `renderCellMap` (real `regex.search`) + all **3
`test{}`** (`renderCellMap` / `renderCellMap hover links` / `renderCellMap mods no
match`) assertion-for-assertion; they pass. Like `rustty-input`'s `Link` (the
reference imports oniguruma unconditionally in the renderer link path),
`rustty-oniguruma` is a **NON-optional** dep of `rustty-renderer`, adding a C-FFI
edge (`oniguruma-sys`) — so `rustty-renderer` routes to ASan for G6 (already in
`check_leaks.sh` `FFI_CRATES`). `check_test_counts.py` `renderer_link` apc_deferred
3 → 0. Scoped `link.zig` G3b → missing=0.

### `renderer/shadertoy.zig` — ported; `loadFromFiles`/`loadFromFile` DEFERRED

`shadertoy.zig` (the custom-shader transpile path GLSL → glslang → SPIR-V →
spirv-cross → MSL/GLSL) is ported in `crates/rustty-renderer/src/shadertoy.rs`
against the vendored `glslang-sys` + `spirv-cross-sys` FFI, with all **4 `test{}`**
(`spirv` / `spirv invalid` / `shadertoy to msl` / `shadertoy to glsl`) passing
against real glslang + spirv-cross. `check_test_counts.py` gains `renderer_shadertoy`
(4/4). Scoped `shadertoy.zig` G3b → missing=0. The pkg/glslang safe-wrapper surface
(`Shader`/`Program`/`init`/`testing.ensureInit`) is inlined (the tree vendors only
raw `glslang-sys`), byte-faithful to pkg/glslang/{init,shader,program,test}.zig.

- **DEFERRED: `loadFromFiles` / `loadFromFile`.** They take a
  `configpkg.RepeatablePath` (`@import("../config.zig")`), but `rustty-config`
  ALREADY depends on `rustty-renderer` (config/Cargo.toml), so a
  `rustty-renderer → rustty-config` edge would form a crate cycle (G7 RED). The two
  functions are NOT GPU-coupled and carry **0 `test{}`** (no test-count impact);
  their verbatim bodies are preserved as a commented block in `shadertoy.rs` (R5,
  never a silent drop). **Target:** when the renderer↔config custom-shader load
  layering is resolved (the surface/apprt wiring milestone). The `Uniforms` extern
  struct's exact per-field `align(16)` std140 layout is likewise pinned when the
  GPU draw path (R-d/Phase 3) consumes it (declared `#[repr(C)]` now; unused this
  milestone) — this is part of the already-tracked GPU-draw-path deferral, not a
  new one.

### RESOLVED: parallel `Terminal::init` corruption — root cause was concurrent `onig_initialize()`, NOT the arena/pool

**Symptom.** `cargo test -p rustty-renderer` runs `#[test]`s CONCURRENTLY (whereas the
reference `zig test` runs a file's tests SERIALLY). At full-suite level a rare
(~1/25) crash appeared — a process-wide SIGTRAP heap-corruption abort, and
occasionally an `ArenaAllocator::usable` `data - buf_node_size()` underflow from a
torn `BufNode.data`. This was originally masked with a process-global
`TERMINAL_TEST_LOCK` serializing the 8 `Terminal`-constructing tests, on the
hypothesis of a latent race in `rustty-vt`/`rustty-zigstd`'s arena/memory-pool.

**Actual root cause (proven by bisection, not the arena).** The corruption is in
`rustty-oniguruma`, not the terminal allocator layer:

- `oni::testing::ensure_init()` (`crates/rustty-oniguruma/src/testing.rs`) was a
  faithful port of the Zig reference's `ensureInit`, which reads a
  `var initialized: bool = false` that it **never writes back** — so
  `onig_initialize()` ran on *every* call. Harmless under Zig's serial `zig test`;
  broken under `cargo test`'s parallel runner.
- `onig_initialize()` is **not thread-safe**: it initializes Oniguruma's
  process-global tables (encodings, default syntax, shared char-class table)
  non-atomically. The 3 `render_cell_map*` link tests each call `ensure_init()`
  before compiling a regex, so running them concurrently invoked
  `onig_initialize()` *simultaneously*, racing on those global tables and
  corrupting the C heap. When the clobbered bytes happened to land next to a
  terminal memory-pool arena node, the downstream symptom was the
  `ArenaAllocator::usable` underflow — i.e. **the arena was the victim, not the
  cause**, exactly as the old `lib.rs` note suspected but mislocated.

**Evidence.**
- Bisection over test subsets (`--test-threads=24`): removing the link tests →
  **0 crashes**; the link tests *alone* crashed at a *higher* rate (3/24) than the
  full suite (1/24); skipping `shadertoy` had no effect. A homogeneous
  `Terminal::init`+`RenderState::update`+`deinit` stress with NO Oniguruma never
  crashed in ~2.9M single-thread and ~1.4M multi-thread iterations, ruling out a
  per-instance arena/pool UAF (a single-threaded latent UAF would have surfaced).
- Instrumenting `ArenaAllocator::usable` to abort-with-backtrace on a torn `data`
  showed the SIGTRAP fired *first* (libmalloc/global-table corruption), before any
  arena read — the arena underflow is a secondary manifestation.

**Fix (one change, at the source).** `ensure_init()` now calls `onig_initialize()`
**exactly once per process, thread-safely**, via `std::sync::OnceLock` (concurrent
callers block until the one-time init completes; the `Copy` result is cached). This
is the documented, correct usage of the C library — a real correctness fix, not
serialization of the tests. It is a deliberate, documented divergence from the Zig
reference, required because Rust's test harness is concurrent.

**Proof.** With the fix and `TERMINAL_TEST_LOCK` **removed**: the previously
crash-prone `link`-only subset went 3/24 → **0/60**, the full suite 1/24 →
**0/120** (plus 0/40 at `--test-threads=48`). `rustty-vt` (2019 passed), `-zigstd`,
`-os`, and `-oniguruma` suites remain green.

The `TERMINAL_TEST_LOCK` + `terminal_test_guard()` + the 8 `let _terminal_guard`
call sites have been deleted. The unrelated `SHADER_FFI_LOCK` (glslang/spirv-cross
ARE genuinely non-reentrant) is intentionally retained.

### rustty-renderer R-d — offscreen Metal draw path (Chunks 0-4) DONE; residual = inspector overlayFeatures + GL/WebGL/GTK [RECONCILED 2026-07-10, T14]

**DONE.** The monomorphic `Renderer<Metal>` RENDERS a terminal state offscreen
(macOS/Metal). The whole `metal/*` backend (device/queue/buffer/texture/pipeline/
render-pass/target/frame), `SwapChain`/`FrameState`, `update_frame`, the
`rebuild_cells` CPU cell-fill, the uniforms math, and `draw_frame` (Metal
encode+submit → IOSurface readback) are ported and gated by the golden test
`generic::macos_impl::tests::golden_renders_cursor_fg_bg` ("Ab" + cursor, real pixel
asserts). The `size_of::<CellText>()==32` un-defer landed. The `draw_frame` raw-ptr
aliasing discipline is proven clean under **both** Stacked and Tree Borrows by
`.port/phase-2/mirihar-chunk3b`.

**RECONCILIATION (2026-07-10, VALIDATION_REPORT §3b — this block previously
deferred "five named Goal-2 tiers, 18 live `unimplemented!()` call sites"; four
of the five tiers have since LANDED and the stale list was never resolved here):**
- ~~**windowing-host / apprt-surface**~~ **RESOLVED by W0–W5** (macOS-windowing
  sub-project #1, commits 540c924/74c5720/accbd4a/8e604eb/73acefa/9f16b9d):
  `Metal::{init,loop_enter}` live, async `Frame::complete` present half live,
  CAMetalLayer/IOSurfaceLayer + display-link windowed present live,
  resize/config-swap live. The ONE residual windowing gap is core-side, NOT in
  this crate: `renderer_seam::Renderer::init` never routes the host view into
  `Metal::init(view)` — tracked at **W-ATTACH** (below in this file, Task 23).
- ~~**renderer↔config (G7 cycle)**~~ **RESOLVED** via the `RendererConfig`
  accessor trait defined in this crate and impl'd IN `rustty-config` (no
  renderer→config edge, G7 stays acyclic; memory
  `renderer-config-accessor-trait-acyclic`): `Renderer.DerivedConfig::init`,
  `Message::initChangeConfig`, `Thread.DerivedConfig::init`,
  `shadertoy::{load_from_files,load_from_file}` (live fns, shadertoy.rs:141/188),
  and `prep_background_image` (generic.rs:2586) are ALL live.
- ~~**Kitty-pixel / image-upload**~~ **RESOLVED** (R-d P1 fill,
  `RENDERER_KITTY_IMAGE_PATH_DESIGN.md`): `Image::convert` re-homed as a pure-Rust
  RGBA_PREMUL swizzle loop + the four `kitty_*` terminal-walk methods live
  (image.rs). NOTE: the `rustty-wuffs` crate's own `swizzle.rs` seams are a
  SEPARATE, still-open entry (see "rustty-wuffs swizzle" below).
- ~~**overlay-render**~~ **RESOLVED**: the four z2d `Overlay` `highlight_*` draws
  are live (overlay.rs; z2d `Context` gained `anti_aliasing_mode`/`hairline`).

- ~~**rustty-inspector `overlayFeatures`**~~ **RESOLVED**: `overlayFeatures` is
  live (generic.rs:4619 — `match state.inspector { Some(insp) => (*insp).overlay_features() }`
  through the renderer-owned `*mut dyn InspectorOverlay` fat pointer, the R2
  accessor-trait edge). The inspector arm is inhabited; no `unimplemented!()`.
- ~~**GTK / OpenGL3 inspector backend**~~ **RESOLVED 2026-07-19**: the GTK Dear
  ImGui inspector is implemented over the OpenGL3 backend — dcimgui-sys
  `imgui_impl_opengl3` vendored + compiled + FFI; the `ImguiWidget` /
  `InspectorWidget` / `InspectorWindow` gtk4-rs classes; `Key::imgui_key` filled;
  `toggle-inspector` / `control_inspector` / `redraw_inspector` wired. Verified by
  `tests/gtk_inspector_dump.rs` (framebuffer→PPM KAT: panels render + live input)
  and a live-app toggle shakedown (Ctrl+Shift+I opens/closes the inspector window).
  See `docs/superpowers/plans/2026-07-19-gtk-inspector-opengl.md`.

**STILL DEFERRED (the honest residual):**
- the **WebGL** backend (wasm) — out of the monomorphic Metal scope; a
  whole-backend follow-up port, no in-crate `unimplemented!()`. (Metal = macOS and
  OpenGL3 = GTK/Linux are both done.)

## rustty-crash (integration-tier stage 1) — `ThreadStateSurface` seam; concrete `ThreadState` impl deferred

`src/crash/` is ported in full (dir.zig + main.zig + sentry.zig +
sentry_envelope.zig; the `pkg/sentry` Zig binding package is hosted in-crate as
the `sentry_pkg` module over the vendored `sentry-sys`). One seam, per the
integration-tier blueprint:

- **Seam (landed):** the reference `ThreadState.surface` field is a concrete
  `*Surface` (src/Surface.zig — the core tier ABOVE crash) and `beforeSend`
  reads `surface.size.{screen, grid(), cell}` off it LIVE at crash time. That
  read surface is seamed behind the crash-local trait
  `sentry::ThreadStateSurface { screen_size/grid_size/cell_size -> (u32, u32) }`;
  `ThreadState<S: ThreadStateSurface>` holds a raw `*const S`, preserving the
  reference's live (possibly-torn) read semantics (the Zig comment calls it
  "likely unsafe... most likely fine"; a snapshot would change semantics, so
  none is taken). Plain `(u32, u32)` tuples are acyclicity-REQUIRED:
  rustty-renderer already deps rustty-crash, so crash must NOT dep renderer for
  its size types. This is deliberately NOT apprt-api's `CoreSurfaceHandle`
  (different concern).
- **DEFERRED (untested-only, 0 tests — R1-safe) — RESOLVED (core C8 + C10):**
  the concrete `ThreadStateSurface` impl. **rustty-core's `Surface` implements
  the trait** (landed with core C8 ahead of its C10 row, confirmed closed by
  the C10 row that owned it: `impl ThreadStateSurface for Surface<A>` +
  `Surface.crashThreadState()` Surface.zig:6014 + the
  `crash.sentry.thread_state = ...` sites — sizeCallback :2469 /
  preeditCallback :2530 (C8) / keyCallback :2663 (C10), via
  `sentry::setThreadState`/`clearThreadState` and the [ADV 6.1] Drop guard
  `ThreadStateGuard` mapping the paired `defer ... = null`). The remaining
  Surface.zig sites port with their owning chunks (C11-C13); the renderer
  Thread.zig / termio Thread.zig/Exec.zig sites stay with their owning files
  (GPU/D6 deferrals).
- Note: tests never call the real `sentry_init` (the reference gates init on
  build options and threading; the vendored build is `transport = .none`) —
  the sentry tests exercise `beforeSend` + the seam through the PURE value API
  only, and `sentry_pkg`'s reify test mirrors the sentry-sys KAT idiom.

## rustty-termio stage 2 (integration tier) — dep wiring + struct shapes; deferred live-thread bodies + D6 xev seam [D6 RESOLVED — xev Milestone B DONE, live shell runs]

Stage 2 of the integration-tier blueprint lands the termio Cargo edges
(rustty-config / rustty-renderer / rustty-apprt-api / rustty-global — all
downward-safe, G7-checked), the struct SHAPES of termio/{Options, Termio,
Thread, backend, mailbox, message}.zig, the fully-ported message.zig (both
anonymous `test{}` blocks), and the minimal rustty-renderer cross-thread
surface (renderer/message.zig `Message` + `Thread::Mailbox` =
`BlockingQueue<Message, 64>` + thin `DerivedConfig` placeholders). All
deferrals below are untested-only (0 reference `test{}` blocks — R1-safe);
every deferred body is preserved VERBATIM in comments at its Rust signature
with an `unimplemented!()` marker (NOT fabricated logic).

**STATUS 2026-07-03 — D6 xev Milestone B DONE (the port RUNS A LIVE SHELL).**
Every D6 xev/live-thread termio seam below is now RESOLVED: `rustty-xev` (the real
kqueue `Loop`/`Async`/`Timer`/`Process`/`Stream`/`WriteQueue`/`Completion`) landed
(Milestone A), and Milestone B (chunks B1–B6, XEV_MILESTONE_B_BLUEPRINT.md) wired
the termio writer thread onto it and spawns a real `/bin/sh` end-to-end. The
un-deferred bodies (all preserved-verbatim reference comments intact, R3;
`unimplemented!()` replaced BELOW the comment): `Subprocess::start` (posix, the
pid-only-handle scoped-borrow spawn, no pty double-close — [F1]/[F3]/[F4]),
`Exec::{threadEnter,threadExit,focusGained,queueWrite,ttyWrite}` +
`processExit`/`processExitCommon`/`termiosTimer`, `ReadThread::threadMainPosix`,
`Termio::{init (in-place out-param, [F1]),threadEnter,processOutput,
processOutputLocked}`, and `Thread::{threadMain,threadMain_}` + `drainMailbox` +
the timer/async callbacks (`syncResetCallback`/`coalesceCallback`/`wakeupCallback`/
`stopCallback`/`selectionScrollCallback`/`startScrollTimer`/`stopScrollTimer`/
`startSynchronizedOutput`/`handleResize`). [F2] `Thread::threadMain` +
`Termio::processOutput` take RAW `*mut` (not `&mut`) to avoid the protected-`Unique`
aliasing UB against the concurrent ReadThread. Proof: the bounded live-shell KAT
(`crates/rustty-termio/tests/live_shell.rs`, macOS-only) drives a real `sh -c 'echo
…'` through the full Exec→pty→xev→terminal round-trip and asserts the output lands
in the `Screen` — deterministic (20/20 no hangs/flakes; bounded `run(NoWait)` +
wall-clock deadline + tick cap, never `run(.until_done)`). termio = 47 unit tests
(≥ 44, no drop) + the live-shell KAT (rustty-only integration margin); scoped-G3b
`missing=0` (888 distinctive ref comments). The fork/exec + kqueue KATs are
Miri-excluded (real syscalls; MIRI_EXCEPTIONS.md). **What STILL stays deferred**
(genuinely out-of-scope, NOT xev IO wiring): `renderer.Thread`'s own event loop
(GPU + xev, **Phase-3**; the io thread's `renderer_wakeup.notify()` still fires a
real mach send onto an Async whose loop nobody pumps yet — non-blocking, fails-safe);
the DCS↔tmux `.tmux` enter/exit arm in `stream_handler::dcsCommand` (vt-side
`TmuxCommand = ()` stub — the **vt tmux control-mode DCS wiring** sub-batch); and the
Windows/Flatpak stubs (`Subprocess::start` windows spawn, `ReadThread::
threadMainWindows`, `killCommandFlatpak`, `Backend::childExitedAbnormally` — the
last uninstantiated in the reference via Zig lazy compilation), all `#[cfg]`/
const-false and unrelated to the macOS live-shell milestone. The remaining
`[UPDATED …]` notes below are retained for provenance.

- **D6 xev seam (`rustty-termio/src/xev_seam.rs`).** Every termio file imports
  `@import("../global.zig").xev` (libxev, `xev.Dynamic`). Per the blueprint's
  D6 decision, libxev ports later as its own low crate (`rustty-xev`,
  Zig-source precedent: rustty-z2d). Until then the handle TYPES
  (`Async`/`Loop`/`Completion`/`Timer` + error sets) are placeholders whose
  methods are `unimplemented!()` *dependency* markers (rustty-os
  cf_release_thread `xev_seam` precedent), anchored to `rustty_global::xev`.
  [UPDATED core C0: the types moved verbatim FROM the termio-local module INTO
  the new shared `crates/rustty-xev` (CORE_BLUEPRINT §5 "xev handle
  unification" — core hands `render_thread.wakeup` to
  `termio::Options.renderer_wakeup`, so both must name ONE type; `Async`
  gained `Clone`/`Copy`, the reference copies the handle by value);
  `termio::xev_seam` is now a `pub use rustty_xev::*` re-export. Still
  D6-deferred — the crate is libxev's future home, filled in place.]
  [UPDATED xev Milestone A/B — RESOLVED: `rustty-xev` is a REAL working kqueue
  loop and `termio::xev_seam` re-exports it; all termio bodies now call the live
  `Loop`/`Async`/`Timer`/`Process`/`Stream`/`WriteQueue`/`Completion` API (no
  `unimplemented!()` seam methods remain in the IO path).]
- **Deferred with verbatim bodies (rustty-termio):**
  - `Termio.zig`: `init` (constructs the stage-4 `StreamHandler`),
    `threadEnter` (builds `ThreadData` around the live loop; `.backend =
    undefined` out-param protocol), `changeConfig`/`processOutput`/
    `processOutputLocked` (stream handler + renderer mutex),
    `resize`/`sizeReport`/`sizeReportLocked`/`resetSynchronizedOutput`/
    `clearScreen`/`scrollViewport`/`jumpToPrompt`/`focusGained`/
    `colorSchemeReport`/`colorSchemeReportLocked` (all execute on the IO
    thread holding the live renderer-state mutex; several end in xev
    wakeups). Ported REAL: the struct shapes, `MutexState`, `DerivedConfig`
    (init/deinit in full), `ThreadEnterState` (create/destroy/prepareInput in
    full), `ThreadData::deinit`, `deinit`, `queueMessage`, `queueWrite`,
    `threadExit`, and the `getProcessInfo` comptime split
    (`getForegroundPid`/`getTtyName`, rustty-os pty.rs precedent).
    [UPDATED core C-T — RESOLVED: the mutex-only method family
    `changeConfig`/`resize`/`sizeReport`/`sizeReportLocked`/
    `resetSynchronizedOutput`/`clearScreen`/`scrollViewport`/`jumpToPrompt`/
    `focusGained`/`colorSchemeReport`/`colorSchemeReportLocked` is now ported
    LIVE against the guard-less C0 `rustty_zigstd::thread::Mutex` (the `defer
    unlock` is the explicit unlock before every return; `resize`/
    `jumpToPrompt`/`resetSynchronizedOutput` end in the xev `renderer_wakeup.
    notify()` placeholder leaf — live-now, runtime-panics only once the render
    thread runs). `changeConfig` takes `DerivedConfig` BY VALUE (the reference
    `self.config = config.*` move). `clearScreen`'s Kitty
    `screens.active.kitty_images.delete(&self.terminal, ...)` uses the vt
    graphics_exec move-out/move-back for the storage↔terminal alias. STILL
    D6-deferred: `init`/`threadEnter`/`processOutput`/`processOutputLocked`
    (xev loop + read-thread inspector slow path).]
    [UPDATED xev Milestone B — RESOLVED: `init` is now the in-place out-param
    `init(self_: *mut Termio<M>, …) -> Result<()>` ([F1] — re-seats the
    `StreamHandler` `terminal`/`size`/`termio_mailbox` raw ptrs at the FINAL
    address, no dangle); `threadEnter` builds `ThreadData { loop_, … }` and calls
    `backend.threadEnter` then `queueWrite`s prepared input; `processOutput`
    (RAW `*mut Termio<M>`, [F2]) + `processOutputLocked` lock the renderer mutex,
    drive `terminal_stream.nextSlice`, `queueRender()`, `mailbox.notify()`. All
    LIVE — the read-thread inspector slow path is ported as-is. Proven by the
    live-shell KAT.]
  - `Thread.zig`: the run loop (`threadMain`/`threadMain_`/`CallbackData`/
    `drainMailbox`/`startSynchronizedOutput`/`handleResize`/`startScrollTimer`/
    `stopScrollTimer` + the five xev callbacks) — D6. Ported REAL: the struct
    shape, `Coalesce` (+ `MIN_MS`), `SYNC_RESET_MS`/`SELECTION_SCROLL_MS`,
    `Flags`, `init`, `deinit`.
    [UPDATED xev Milestone B — RESOLVED: the whole run loop is LIVE.
    `threadMain(self_: *mut Thread, io: *mut Termio<M>)` (RAW `*mut`, [F2]) runs
    `threadMain_`, which frame-pins `CallbackData`/`ThreadData` (stable ptr for
    the loop's life — the reference's "stack here so it has a stable pointer"),
    arms `mailbox.wakeup.wait`/`stop.wait`, and runs `loop.run(.until_done)`.
    `drainMailbox` + `startSynchronizedOutput`/`handleResize`/`startScrollTimer`/
    `stopScrollTimer` + the five callbacks (`syncResetCallback`/`coalesceCallback`/
    `wakeupCallback`/`stopCallback`/`selectionScrollCallback`) are ZST fn-items
    recovering state through the `*mut CallbackData` userdata (never a long-lived
    `&mut` across a tick — rust-mut-args-strongly-protected). Proven by the
    live-shell KAT.]
  - `mailbox.zig`: `send` — its slow path unlocks/relocks a CALLER-held
    `std.Thread.Mutex` (manual lock/unlock), which guard-based
    `std::sync::Mutex` cannot express from the callee frame; the renderer
    `State.mutex` modeling is decided ONCE when the threads go live (D6 /
    core tier), not fabricated now. Ported REAL: `initSPSC`/`deinit`/`notify`.
    [UPDATED core C0: the mutex MODEL is now decided — renderer `State.mutex`
    retyped to `*mut rustty_zigstd::thread::Mutex` (guard-less Zig-shaped
    lock/try_lock/unlock, CORE_BLUEPRINT §3) and `send`'s parameter carries
    it; the body itself stays deferred until the core-tier termio pass (C-T).]
    [UPDATED core C-T — RESOLVED: `send` is ported LIVE; the instant→forever
    retry uses the new `BlockingQueue::push_or_return` (Zig's by-COPY "keep
    your copy" — Rust moves, so a failed instant push hands the message BACK)
    and the guard-less mutex `unlock()`/`lock()` for the caller-held-lock
    inversion. `wakeup.notify()` is the xev placeholder leaf.]
    [UPDATED xev Milestone B — RESOLVED: `wakeup.notify()` is now a LIVE
    `rustty_xev::Async::notify` mach-port poke, and the io thread's `threadMain_`
    actually pumps the mailbox `wakeup` Async — the mailbox→writer wakeup path is
    whole (exercised end-to-end by the live-shell KAT).]
- **Stage-4 placeholders (rustty-termio) — RESOLVED (stage 4):**
  `backend::{Exec, ExecConfig, ExecThreadData}` are now the REAL
  `exec::{Exec, Config, ThreadData}` types (Exec.zig ported, 10 tests) and
  the `Backend`/`ThreadData` delegation arms are live;
  `termio_impl::StreamHandlerStream` is now the real
  `stream_handler::Stream<'static, M>` alias (stream_handler.zig ported).
- **rustty-renderer:** `message::Message::initChangeConfig` — takes
  `*const configpkg.Config`, but config already deps renderer (G7 cycle, same
  wall as `shadertoy::loadFromFile`); verbatim body preserved. The two
  `DerivedConfig` pointee types (`thread::DerivedConfig`,
  `RendererDerivedConfig` for `renderer.Renderer.DerivedConfig`) are
  uninhabited placeholders (the real structs are config-coupled and belong to
  the unported Thread event loop / GPU renderer), so `Message::ChangeConfig`
  is not yet constructible and `Message::deinit`'s destroy arm is statically
  unreachable. renderer/Thread.zig itself is ported ONLY as the minimal
  cross-thread surface (`Mailbox` + `DerivedConfig`); its event loop defers
  with D6 + the GPU tier (0 tests).
  [UPDATED core C0: `thread::DerivedConfig` RETYPED uninhabited `enum {}` ->
  INHABITED field-less struct with `unimplemented!()` `init<C>` (CORE_BLUEPRINT
  §5 [ADV-FIX 2] — core's renderer thread seam embeds it BY VALUE; an
  uninhabited field would make core `Surface<A>` uninhabited). Its
  `custom_shader_animation` field + real `init` body stay deferred with the
  config-side placement. `Message::initChangeConfig` gained its generic-`C`
  placeholder SIGNATURE (+ `InitChangeConfigError` opaque marker); body still
  deferred verbatim. `RendererDerivedConfig` stays uninhabited, so
  `Message::ChangeConfig` remains unconstructible.]
- **Adjusted-constant note (R1):** termio message.zig's anonymous
  `@sizeOf(Message) == 40` anti-growth test is pinned at the equivalent
  Rust-layout constant (56) with the mapping documented at the assertion
  (usize `len` in `MessageData::Small`, unpacked discriminant).

## rustty-termio stage 3 (integration tier) — relocated Command.zig (break #13b)

Stage 3 ports src/Command.zig (911 loc, 10 `test{}` blocks) into
`rustty-termio/src/command.rs` (blueprint §1: Command-in-core would cycle —
termio/Exec constructs+starts Command while core already deps termio;
`rustty_termio::Command` is re-exported at the crate root so core's Surface
reaches the reference's flat `Command` symbol). All 10 reference tests are
ported LIVE (R1; `termio_command` MAP entry). Notes:

- **GTK growth point (`RtPreExecInfo`/`RtPostForkInfo`).** The reference's
  comptime `@hasDecl(apprt.runtime, "pre_exec"/"post_fork")` branches are
  realized ONLY by the GTK apprt (ported last). Until GTK lands and
  specializes these types IN PLACE, the trivial fallback structs
  (`init(_: &Config) -> Self` on unit-like structs, needing only
  rustty-config's `Config` type) are the definitions — marked at the types;
  NOT added to rustty-apprt-api (blueprint §1). 0 tests affected.
- **Windows spawn path (cfg-gated structural port, unverified).**
  `startWindows`, the windows `wait`/`Exit`/`pseudo_console`/`pid` branches,
  and the windows halves of the test literals are ported line-for-line under
  `#[cfg(windows)]` (rustty-os pty.rs WindowsPty precedent) but are UNVERIFIED
  on a real Windows target (none in CI; the reference itself `SkipZigTest`s 6
  of the 10 tests on windows — mirrored as cfg-gated bodies, F3 DirectWrite
  precedent). `createWindowsEnvBlock`/`windowsCreateCommandLine` are compiled
  (and type-checked) on all targets. One shim decision, documented at the call
  site: the reference's `windows.OpenFile(\Device\Null, ...)`
  (std.os.windows' NtCreateFile wrapper) is realized as the
  identical-behavior Win32 `CreateFileW("NUL", ...)` — "NUL" is the Win32
  alias of the NT `\Device\Null` object — rather than re-binding ntdll.
  `WaitForSingleObject`/`GetExitCodeProcess` are declared as Command-local
  kernel32 shims (not yet in rustty-os::windows).
- **Zig-std shims (Rust-only glue, R1 no-loss, in command.rs):**
  `posix.execvpeZ` (Zig's USERSPACE PATH search — macOS has no execvpe(3) —
  with the execveZ errno mapping reproduced), `posix.chdir` (stack-buffer
  copy), `posix.dup2`/`waitpid`/`fork` errno mappings, `std.c.environ`
  (`_NSGetEnviron` on Apple), and the fixed 1024-byte stack stderr writer.
  None heap-allocate: the reference's allocation-after-fork prohibition is
  preserved (all allocation happens before `fork()`; the child touches only
  stack buffers + syscalls).

## rustty-termio stage 4 (integration tier) — Exec / shell_integration / stream_handler testable logic

Stage 4 ports the termio testable logic: termio/Exec.zig (10 `test{}` blocks,
all LIVE — `termio_exec` MAP entry), termio/shell_integration.zig (21 blocks,
all LIVE, byte-exact per-shell strings — `termio_shell_integration`), and
termio/stream_handler.zig (0 blocks — the `StreamHandler` vt-`Handler` impl,
`termio_stream_handler`), completing the crate's R1 contract of 44 tests
(Exec 10 + shell_integration 21 + message 2 + root 1 + Command 10). All
deferrals below are untested-only (0 reference `test{}` blocks — R1-safe);
every deferred body is preserved VERBATIM in comments at its Rust signature
with an `unimplemented!()` marker (NOT fabricated logic).

- **Exec.zig D6 xev wiring (exec.rs).** `threadEnter` (xev.Process/Stream/
  Timer + read-thread spawn), `threadExit` (quit-pipe write + join-by-value —
  the D6 thread-lifecycle modeling owns the JoinHandle), `focusGained`
  (termios timer state), `processExitCommon`/`processExit`/`flatpakExit`/
  `termiosTimer` (xev callbacks; surface pushes at the opaque-Timeout
  AppMailbox seam), `queueWrite`/`ttyWrite` (xev write stream/pools), and
  `ReadThread.threadMainPosix`/`threadMainWindows` (spawned only by
  threadEnter; drive the deferred `Termio.processOutput` + the crash-sentry
  `thread_state` whose `surface` reads the reference Mailbox's surface
  pointer). The xev_seam gains the `Stream`/`Process`/`WriteRequest`/
  `WriteQueue` handle types (same dependency-marker contract as stage 2).
- **`Subprocess.start` (exec.rs).** Everything up to the spawn is expressible,
  but the final `self.process = .{ .fork_exec = cmd }` stores a Command whose
  path/args/env are the reference's raw ARENA POINTERS into the subprocess's
  own fields (env is even freed on successful start — Zig tolerates the
  dangling pointer because the stored Command only ever reads `.pid`).
  As Rust references that storage is exactly the strongly-protected-reference
  UB the port forbids (memory `rust-mut-args-strongly-protected`). The
  stored-process modeling (raw-ptr Command variant or a pid-only handle) is
  decided WITH the D6 live-thread wiring — `threadEnter` (itself deferred) is
  start's only caller. Verbatim body preserved. Ported REAL: `Subprocess.init`
  (the full argv+env assembly), `execCommand` (+ its `comptime passwdpkg`
  parameter as the `PasswdPkg` trait with inferred-error associated type),
  `deinit`/`externalExit`/`stop`/`resize`/`killCommand`/`killPid`/`getpgid`/
  `childPreExec`, the `getProcessInfo` split, `Config`, `ThreadData` (+
  `deinit`), and `ArgsFormatter`.
- **Flatpak comptime-false branches (exec.rs).** `build_config.flatpak` is
  const `false` (rustty-build-config), so `FlatpakHostCommand` resolves to the
  reference's zero-size fallback; the flatpak-gated arms (`stop`'s `.flatpak`,
  `start`'s host-command spawn, `killCommandFlatpak`) are compiled out exactly
  as Zig's lazy compilation leaves them unanalyzed — bodies verbatim; they
  re-bind to rustty-os's FlatpakHostCommand with the Linux/flatpak tier.
- **`Backend.childExitedAbnormally` (backend.rs) — Zig lazy compilation.** The
  reference delegates to `exec.childExitedAbnormally(...)`, a method
  `termio.Exec` does NOT declare at this reference version; the Backend method
  has no callers so Zig never instantiates it. Ported as the verbatim
  delegation comment + an `unimplemented!()` marker (fires only if a caller
  appears before the reference regrows the Exec method).
- **stream_handler.zig cross-thread writers (stream_handler.rs).**
  `surfaceMessageWriter` (fast path constructs the OPAQUE core
  `App.Mailbox.Queue.Timeout` — the AppMailbox associated type filled when
  core lands; slow path unlocks/relocks the caller-held renderer mutex) and
  `rendererMessageWriter` (same manual-mutex slow path; the reference
  declaration is itself uncalled in this file) — both follow the stage-2
  `mailbox::send` deferral precedent, bodies verbatim. [UPDATED core C0: both
  blockers are now RESOLVED at the type level — `AppMailbox::push` takes the
  concrete `rustty_datastruct::blocking_queue::{Timeout, Size}` and the mutex
  model is `rustty_zigstd::thread::Mutex` — so the two bodies UN-DEFER with
  the core-tier termio pass (C-T), per CORE_BLUEPRINT §3.] [UPDATED core C-T —
  RESOLVED: both ported LIVE. `surfaceMessageWriter` retries via the new
  `AppMailbox::push_or_return` (the trait's `push` is now the drop-on-failure
  default over it); `rendererMessageWriter` retries via
  `BlockingQueue::push_or_return`; both do the guard-less-mutex unlock/relock
  inversion. `rendererMessageWriter` keeps `#[allow(dead_code)]` (uncalled in
  the reference too). `surfaceMessageWriter` takes `SurfaceMessage<'static>`
  (a storing push cannot take a borrow).] Every OTHER handler
  path is LIVE (the full `vtFallible` dispatch as the vt `Handler` trait impl,
  `setMode`, `dcsHook/Put/Unhook` + DECRQSS/XTGETTCAP, `apcEnd` kitty/glyph,
  OSC 4/7/10-12/21/52/133/777, kitty keyboard/color, mode/size/DA/DSR
  reports); the writer calls inside them compile and defer at the call
  boundary (Termio.queueMessage precedent).
- **`dcsCommand` tmux arm (stream_handler.rs).** Deferred WITH rustty-vt
  dcs.rs's `TmuxCommand = ()` stub (the vt-side control-mode DCS enter/exit
  wiring is a documented vt deferral): the payload carries no notification to
  route into the viewer yet. Body verbatim; the `tmux_viewer` field and its
  `deinit` block are live (`TMUX_ENABLED` = `options::TMUX_CONTROL_MODE`).
- **Zig-std shims (Rust-only glue, R1 no-loss).** exec.rs:
  `std.fs.path.dirname`, the `[:0]` `cstring` sentinel model, `posix.errno`;
  killPid reuses command.rs's `posix.waitpid` shim. stream_handler.rs:
  `std.Uri.getHost`/`host_name_max` (over rustty-os uri `Component::toRaw`),
  fixed-buffer `std.Io.Writer` (`write_fixed`/`FixedFmt`), Vec-backed
  fmt/os-Writer adapters, and `terminfo.ghostty.xtgettcapMap()` as a
  process-lifetime `OnceLock` (comptime-static analogue — the `write_stable`
  `'static` contract).
- **EnvMap boundary (shell_integration.rs/exec.rs).** The crate's `EnvMap`
  models `std.process.EnvMap` with Rust `String`s (the stage-3 Command
  decision); the reference's `[]const u8` env values convert at the put
  boundary (`env_string`) — non-UTF-8 panics, matching rustty-os `getEnvMap`'s
  `std::env::vars` behavior. R2 identity: the `GHOSTTY_*` env names and the
  shell-integration script/module names derive from rustty-identity
  (`env_name`, resourcesdir precedent); the tests compare against the same
  identity-derived strings.

## rustty-core C0 (core tier) — seam prep: xev crate, zigstd Mutex, apprt-api growth, type-Core decision

C0 of CORE_BLUEPRINT §6 lands the seams the core chunks build on. It creates
NO new verbatim-body deferrals; it RESOLVES/OWNS the following:

- **RESOLVED (moved): D6 xev handle types -> `crates/rustty-xev`.** The
  termio-local `xev_seam` placeholder types moved verbatim into the new shared
  leaf crate (deps `rustty-global` only); `termio::xev_seam` re-exports them.
  `Async` gained `Clone`/`Copy` (the reference copies the handle by value at
  `.renderer_wakeup = render_thread.wakeup`). The D6 deferral itself is
  UNCHANGED — rustty-xev is the crate the libxev port fills in place.
- **RESOLVED (decision): the renderer-state mutex model.** New
  `rustty_zigstd::thread::Mutex` — the guard-less Zig-shaped
  `std.Thread.Mutex` (lock/try_lock/unlock, no RAII guard, no poisoning; std
  `Mutex<bool>`+`Condvar`; 3 rustty-only unit tests). Renderer `State.mutex`
  retyped `*mut std::sync::Mutex<()>` -> `*mut rustty_zigstd::thread::Mutex`;
  `termio::Mailbox::send`'s parameter follows. Unblocks (at C-T, not yet
  un-deferred): `mailbox::send`, `surfaceMessageWriter`,
  `rendererMessageWriter`, `Termio.queueMessage` locked path + the mutex-only
  Termio method family. xev-blocked Termio paths stay D6.
- **RESOLVED (growth points): apprt-api `App`/`Surface`/`AppMailbox`.**
  `App` gained `config`/`keyboard_layout`; `Surface` gained `type Core` +
  `core`/`rt_app`/`close`/`get_content_scale`/`get_size`/`get_title`/
  `supports_clipboard`/`clipboard_request` (returns `Result<bool, _>`,
  [ADV-FIX 1])/`set_clipboard`/`get_cursor_pos`/`default_termio_env`
  (CORE_BLUEPRINT §2 signatures, embedded.zig line comments). `AppMailbox`
  dropped its opaque `type Timeout`/`type Size` growth placeholders for the
  concrete `rustty_datastruct::blocking_queue::{Timeout, Size}`, and `push`
  now takes `Message<'static>` (all queued payloads verified owned;
  `ChangeConfig(&Config)` is only ever direct-dispatched).
- **RETYPED placeholder: renderer `thread::DerivedConfig`** uninhabited
  `enum {}` -> inhabited field-less struct + `unimplemented!()` `init<C>`
  ([ADV-FIX 2]); `Message::initChangeConfig<C>` placeholder signature added
  (body still deferred verbatim — config G7 cycle). See the annotated stage-2
  entries above.
- **DECIDED: `type Core` = OPTION 1** (self-referential bound) — prototype
  compiled + runtime-tested in `rustty-core/src/lib.rs
  type_core_prototype`; see CORE_BLUEPRINT.md "## C0 DECISION".
- **[ADV-FIX 5]:** rustty-core Cargo deps completed (rustty-util/-unicode/
  -oniguruma/-datastruct/-zigstd/-xev + allocator-api2).

## rustty-inspector C1 (core tier) — key-widget data surface; ImGui GUI deferred to Phase 3

**RESOLVED (WS6, 2026-07-28).** `rustty_inspector::Inspector` is no longer an
uninhabited enum: the crate is a fully inhabited 13-module port of oracle
`src/inspector/**` (~11.6k Rust LOC vs 4 873 Zig LOC), landed by the 2026-07-14
inspector series (`b344e09`…`d6be9b2`). The widget-render residual is proven
ZERO by `python3 .port/tools/gates/check_comment_coverage.py --crate
rustty-inspector --strict` (WS6 T2), which reports `distinctive ref
comments=192 missing=0 review=0` → GREEN. The `key.zig` ImGui section named
below is live, not a comment body. Core stores a live
`Option<*mut rustty_inspector::Inspector>`. The historical text below is kept
for provenance only.

Chunk C1 of CORE_BLUEPRINT §6 (§4 "Inspector seam", [ADV-FIX 7]) carves ONLY
the surface core's input hot path needs. Deferrals created (all 0 tests —
R1-safe; owner: the Phase-3 inspector port):

- **PARTIAL-FILE: `src/inspector/widgets/key.zig` ImGui section**
  (`Event.render`/`renderUtf8`/`renderPty`, `modsTooltip`, the `Stream`
  widget, `actionColor`/`keyColor`/`hsvToRgba`) — preserved as verbatim
  comment bodies at the bottom of
  `crates/rustty-inspector/src/widgets/key.rs`; fills in place when dcimgui
  is wired. The pure-data `Event` + `EventRing = CircBuf<Event>` + `label()`
  are LIVE (the datastruct `CircBuf` bound was relaxed `T: Copy` -> the
  faithful `T: Clone` for the heap-owning element; the Zig CircBuf has no
  comptime constraint — `append_slice_assume_capacity` keeps a per-method
  `T: Copy` for its `fastmem` memcpy path).
- **`src/inspector/Inspector.zig` -> uninhabited `pub enum Inspector {}`
  placeholder** in `crates/rustty-inspector/src/lib.rs` (exact precedent:
  `rustty_renderer::state::Inspector`). Unconstructible until Phase 3; core
  stores `Option<*mut rustty_inspector::Inspector>` (always `None`) and the
  single `renderer_state.inspector` assignment casts between the two
  uninhabited placeholders (edge direction: inspector -> renderer, never the
  reverse). `activateInspector`/`deactivateInspector` defer verbatim in core
  (chunk C6) — they are the only constructors.
- **`src/inspector/widgets.zig` remainder** (the page/pagelist/renderer/
  screen/style/surface/terminal/termio sub-module re-exports + `helpMarker`/
  `DetachableHeader`/`DetachableHeaderState`/`detachableHeader`) — module
  wiring stub `crates/rustty-inspector/src/widgets.rs`; only the `key`
  re-export is live. `units.zig` and the other `widgets/*.zig` files are not
  started (crate stays PORT STATUS: partial).
- **RESOLVED: `main.zig`** — `KeyEvent = widgets.key.Event` re-export + the
  refAllDecls test (reify smoke over the carved surface) are live in lib.rs.

## rustty-core C3 (core tier) — renderer_seam dependency placeholders (Renderer, Thread<A>, Options<A>)

Chunk C3 of CORE_BLUEPRINT §6 (§5 "Renderer/GPU-coupled paths + xev (D6)
map") lands `crates/rustty-core/src/renderer_seam.rs` — core-local
DEPENDENCY PLACEHOLDERS (the termio `xev_seam` precedent: inhabited structs,
faithful field/signature shapes, `unimplemented!()` bodies; NO reference
logic stubbed), NOT verbatim-body deferrals. All 0 tests — R1-safe.
Owner of the fill: the Phase-3 R-d GPU renderer tier + the D6 libxev port.

- **DEFERRED (dependency placeholder): `renderer.Renderer`**
  (src/renderer/generic.zig `GenericRenderer(Metal|OpenGL)`, comptime-chosen
  at renderer.zig:38) -> inhabited unit struct `renderer_seam::Renderer` with
  the §5 signatures `surfaceInit<S>` / `init(alloc, options)` ([ADV-FIX 3]:
  alloc stays) / `finalizeSurfaceInit<S>` / `threadEnter<S>` / `drawFrame` /
  `deinit`, plus the `renderer.Renderer.DerivedConfig` placeholder
  (generic.zig:536; `init(alloc_gpa, &Config)`/`deinit`) and the opaque
  `RendererError` marker (inferred reference error unions; `ApprtError`
  growth-point precedent). The GraphicsAPI/SwapChain/FrameState/
  CustomShaderState machinery and the rest of generic.zig land WITH the
  Phase-3 port in rustty-renderer — nothing of it lives in core.
- **DEFERRED (dependency placeholder): `renderer.Thread` run loop** —
  `renderer_seam::Thread<A>` carries the FAITHFUL renderer/Thread.zig field
  shape (xev loop/wakeup/stop/render_h/draw_h/draw_now/cursor_h handles +
  completions from rustty-xev; `surface: *mut A::Surface`; `renderer: *mut
  Renderer`; `state: *mut renderer::State` (REAL); `mailbox: *mut
  renderer::thread::Mailbox` (REAL); `app_mailbox: app::Mailbox<A>`;
  `config: renderer::thread::DerivedConfig` ([ADV-FIX 2] inhabited); `flags`
  with the reference defaults `visible/focused = true` via manual `Default`)
  and `init`/`threadMain`/`deinit` as `unimplemented!()` placeholders. The
  run loop (`threadMain_`, `drainMailbox`, render/draw/cursor callbacks,
  `DRAW_INTERVAL`/`CURSOR_BLINK_INTERVAL`, `must_draw_from_app_thread`) is
  Phase-3 R-d + D6 work that lands WITH the real port (renderer/Thread.zig
  imports configpkg AND core's App.Mailbox — both edges cycle from
  rustty-renderer, which is WHY the shape lives core-local).
- **Ported live (not deferred): `renderer.Options`** (renderer/Options.zig,
  full field shape + comments) as `renderer_seam::Options<A>`; the blueprint
  sketch's `Options<'_, A>` lifetime is dropped — the reference file has no
  borrowed field (E0392), documented at the struct.
- **SEQUENCING SKELETONS (not deferrals; next chunks fill IN PLACE):**
  `src/app.rs` — minimal `Mailbox<A>`/`Queue<A>`/`Message<A>` shell (C4 owns
  ALL of App.zig; renderer_seam::Thread embeds `app::Mailbox<A>` one chunk
  early — the split the blueprint's C3 row sanctions); `src/surface.rs` —
  `Surface<A>` shell + `SurfaceMailbox<A>` field shape (C5 owns Surface.zig;
  §1 pins `apprt.surface.Mailbox` to surface.rs and renderer_seam::Options
  names it). Both carry manual no-`A`-bound `Clone`/`Copy` ([ADV-FIX 6a]).
  rustty-core gained the `rustty-font-shaper` dep (Options.zig `font_grid:
  *font.SharedGrid` — the D1 shaper-tier type).

## rustty-core C4 (core tier) — App.zig full port (mailbox, dispatch, seam tests)

Chunk C4 of CORE_BLUEPRINT §6 lands ALL of App.zig (604 loc, 0 `test {}`
blocks) in `crates/rustty-core/src/app.rs` (`App<A>`, `Message<A>` +
documented `unsafe impl Send`, `NewWindow<A>`, `Queue<A>`, `Mailbox<A>` with
manual no-`A: Copy` `Clone`/`Copy` [ADV-FIX 6], create..performAllAction)
plus the §7 rustty-only `#[cfg(test)] mod test_runtime` (TestApp/TestSurface
recording `perform_action` in a `Mutex<Vec<_>>`, counting `wakeup`s — test
glue, NOT a port) with 13 seam tests. Deferral ledger:

- **DEFERRED (documented, in-file): the App.zig Wasm block** (App.zig:579
  `pub const Wasm = if (!builtin.target.isWasm()) ...`) — wasm-target-gated
  in the reference and its entire body (beyond two imports) is commented out
  IN the reference; preserved verbatim as comments at the bottom of app.rs
  (config `wasm.rs` precedent). Joins the rustty wasm app-runtime
  (off-by-default `wasm` feature family). 0 tests, R1-safe.
- **RESOLVED in part: input `Binding.Action.Scoped`/`scoped`** (Binding.zig:
  1436/1474, previously wholly deferred at the binding.rs PORT NOTE): the
  `Scoped(.app)` instantiation now lands in rustty-input as the hand-written
  `ScopedApp` fieldless enum (Action-declaration-order fields; all app-scoped
  actions are void) + `Action::scoped_app`, WITH its first consumer
  (App.performAction/keyEvent). ~~**Still deferred: `Scoped(.surface)`**~~
  **RESOLVED (core C14a):** the `Scoped(.surface)` instantiation now lands in
  rustty-input as the hand-written `ScopedSurface<'a>` enum
  (Action-declaration-order surface fields; byte-slice/`CursorKey` payloads
  borrowed, `Copy` payloads by value) + `Action::scoped_surface`, WITH its
  consumer core C14a `Surface.performBindingAction` (Surface.zig:4809). 0 tests
  either way.
- **SEQUENCING STUBS (not deferrals; later chunks fill IN PLACE):** the four
  core-`Surface` methods App dispatches into — `close` (C6, Surface.zig:842),
  `needsConfirmQuit` (C6, :938), `handleMessage` (C7, :960),
  `performBindingAction` (C14, :4775) — are `unimplemented!()` stubs in
  surface.rs with the reference doc comments + an opaque `surface::Error`
  growth marker (termio `Error` precedent); C4 also added the minimal
  Surface fields App touches (`id`, `rt_surface`) and the landed
  `CoreSurfaceHandle` impl (§1).
- **Ported live (additive growth, not deferred):** apprt-api `Surface` trait
  gained `deinit(&mut self)` (embedded.zig:597) — the §2 growth mechanism;
  App.zig:107 (`App.deinit` shutdown sweep) is a callsite the blueprint's
  Surface.zig-scoped grep predates. rustty-datastruct `BlockingQueue` gained
  the by-value `.{}` constructor as a manual `Default` (App.zig:99
  `.mailbox = .{}` embeds the queue by value; `create()` now wraps it).

## rustty-core C5 (core tier) — Surface skeleton + DerivedConfig (field-parity audit)

Chunk C5 of CORE_BLUEPRINT §6 lands Surface.zig:1-456 in
`crates/rustty-core/src/surface.rs`: the full `Surface<A>` field set (§1),
`InputEffect`/`Search`/`Mouse` (+`activeLeftClickPin`)/`Keyboard`
(+`TableStackEntry`, the anonymous `table_stack` element struct),
`DerivedConfig` (+`DerivedConfigLink`, `init`, `deinit`, `scaledPadding`),
the `Mailbox`/`Message` re-exports (Surface.zig:15-16), and the
`SurfaceMailbox<A>` `push`/`AppMailbox` impl (apprt/surface.zig:135, §1/§3).
Field-parity audit (config-cluster lesson): Surface 31==31, Mouse 10==10,
Keyboard 4==4 (+2 TableStackEntry), Search 2==2, DerivedConfig 46==45+
collapsed `arena` (documented), DerivedConfig.Link 3==3, Mailbox 2==2 —
names and order 1:1. Deferral ledger:

- **DEFERRED (pre-existing font-shaper scaffold, new callsite): freetype
  load flags through the break-#9 DI bridge.** The `.font = try
  font.SharedGridSet.DerivedConfig.init(alloc, config)` line (Surface.zig:
  368) is realized as core-local glue (`font_shared_grid_set_derived_config`)
  because the reference init is duck-typed over the GLOBAL config while
  rustty-font-shaper's `DerivedConfig::init` takes its break-#9 subset type;
  core is the first crate that sees both. All fields convert exactly (both
  sides model the SAME reference config types; `Modifier`/`CodepointMap` are
  literally shared rustty-font types) EXCEPT `freetype-load-flags`: on macOS
  the shaper type is `()` (the reference `void` arm — exact); on freetype
  targets the shaper still carries its `_private` scaffold (face/mod.rs), so
  the config value flows only when the Linux freetype backend DI lands.
  0 tests, R1-safe.
- **SEQUENCING allows (not deferrals; later chunks consume IN PLACE):**
  `#[allow(dead_code)]` on `MAX_ACTIVE_KEY_TABLES` (first consumer C14a,
  Surface.zig:5495), `Mouse.activeLeftClickPin` (C8 :1594, C13+ :4650) and
  `DerivedConfig.scaledPadding` (C6 :536, C8 :2514, C11 :3630) — each allow
  is removed when its first callsite lands.
- **Documented shape mappings (not deferrals):** `renderer_thr`/`io_thr`
  `std.Thread`-undefined-until-spawn -> `Option<JoinHandle<()>>` (§8.5);
  `Search.deinit` by-value receiver (`JoinHandle::join` consumes; callers
  `take()` the `Option<Search>` — same drop point); `DerivedConfig.arena`
  collapsed to owned fields (font-shaper precedent; `deinit` keeps the
  explicit regex teardown, and the on-error regex-leak behavior of the
  reference links loop is reproduced exactly by the `Vec` drop);
  `DerivedConfig.title` owned clone of the reference's shallow slice copy.
- **Rust-only test glue (app.rs, not a port):** the two C4 seam tests that
  fabricated a 2-field `Surface` literal now use `fabricateSurface` — a
  `MaybeUninit<Surface<TestApp>>` with ONLY the two App-read fields (`id`,
  `rt_surface`) written via field-projected raw pointers (the reference
  hands App pointers to `undefined` Surface memory; `Surface<A>` is
  untestable-by-construction per §7).

## rustty-core C6 (core tier) — Surface init/deinit/close/draw lifecycle (inspector activate deferred verbatim)

Chunk C6 of CORE_BLUEPRINT §6 lands Surface.zig:457-957 in
`crates/rustty-core/src/surface.rs`: `init` (the full pty/renderer/io wiring,
LIVE against the placeholder leaves — GPU `Renderer.surfaceInit/init/
finalizeSurfaceInit`, renderer/termio `Thread.init` xev handles,
`termio.Termio.init` (D6), where the runtime panics are confined), `deinit`,
`close`, `surfaceMailbox`, `queueIo`, `draw`, `needsConfirmQuit`, plus the
break-#1 `GlobalState`/`STATE`/`global_state()` wiring in lib.rs (the
core-wired `rustty_global::GlobalState<(), rustty_os ResourcesDir>`; `Action`
stays the deferred cli seam). Deferral ledger:

- **RESOLVED (WS6, 2026-07-28) — was: DEFERRED (verbatim, CORE_BLUEPRINT §4)
  `activateInspector`/`deactivateInspector`** (Surface.zig:890/915, ~45 loc).
  Both are now LIVE bodies at `crates/rustty-core/src/surface.rs:2873` /
  `:2967` (commit `2dfcc32`): they heap-box the inspector, publish the
  `*mut dyn InspectorOverlay` + type-erased `inspector_ptr` under
  `renderer_state.mutex`, and push `Message::Inspector(true/false)` to the
  renderer mailbox and termio. `deinit`'s inspector teardown arm is reachable
  and real — the `unreachable!()` is gone. The `keyCallback` inspector
  defer-block and the `cursorPosCallback` Some-arm are likewise reachable and
  exercised: the WS6 T5 shakedown shows typed keys landing in the inspector's
  Keyboard panel and VT traffic in its Terminal IO panel.
- **Rust-only glue (documented in-file, not deferrals):**
  `InitError` — the typed union of `init`'s *inferred* reference error set
  (oni/renderer/apprt/font-grid/xev/exec/termio/spawn/surface sources);
  `initErrdefers`/`initTry` — the post-`self.*`-write errdefer chain
  realized by reading the moved-in fields back OUT of `self_` (Zig unwinds
  through local struct copies aliasing the same state; LIFO order preserved,
  pre-write sites fire the armed errdefers explicitly per site);
  `RendererThreadArg`/`IoThreadArgs` — the [ADV-FIX 6b] documented
  Send-wrapper newtypes for the two `std.Thread.spawn` raw-pointer captures
  (whole-wrapper capture forced against edition-2021 disjoint captures).
- **Documented shape mappings (not deferrals):** `init` stays an out-pointer
  (`self_: *mut Surface<A>`, unsafe fn — the body is self-referential:
  `&self.renderer_thread`/`&self.io.terminal`/the `self` mailbox pointer are
  handed out mid-init), with the `self.* = .{...}` write realized
  field-by-field via `ptr::write` in reference literal order (`.io =
  undefined` placement-written by the io block; `renderer_thr`/`io_thr`
  `undefined` -> the §8.5 `Option` `None`); `deinit` is an unsafe
  invalidate-in-place (`ptr::read` for the consuming `termio.Thread`/
  `Termio`/grid-set-key sub-deinits; owned containers dropped explicitly as
  the collapsed Zig-allocator frees; caller frees without dropping — the
  reference `alloc.destroy` contract); thread names via the pre-spawn
  `Builder` (no post-spawn setName; the ignored `setName catch {}`
  collapses); `finalizeSurfaceInit` through the moved-into field (the
  reference calls its post-copy LOCAL — Zig value-copy, same GPU handles);
  `needsConfirmQuit` takes `&mut self` (non-const reference receiver;
  `cursorIsAtPrompt` mutates — the C5 `&self` stub note was wrong, app.rs
  callsite updated).
- **Additive growth (upstream crates, Rust-only glue):** rustty-os
  `file::crypto_random_bytes` widened to `pub` (the shared
  `std.crypto.random` shim — surface id, Surface.zig:584); rustty-config
  `Command` gained `#[derive(Clone)]` (Zig copies the union by value;
  Surface.init's Exec-config + title-block copy sites); rustty-core gained
  the leaf `rustty-identity` dep (R2: `GHOSTTY_LOG`/`GHOSTTY_SURFACE_ID` ->
  `<APP_NAME upper>_*`, the termio `env_name` precedent).
- **SEQUENCING STUBS (not deferrals; later chunks fill IN PLACE):**
  `recomputeInitialSize` (C8, Surface.zig:1826) and `resize` (C8, :2485) —
  `unimplemented!()` stubs with the opaque `surface::Error` marker (init's
  callsites land now: resize propagates, recomputeInitialSize is caught);
  `#[allow(dead_code)]` on `surfaceMailbox`/`queueIo` (first live callsites
  C7 `handleMessage`; the deferred inspector bodies name `queueIo`
  meanwhile) — each allow is removed when its first consumer lands.

## rustty-core C7 (core tier) — handleMessage / childExited / search callbacks

Chunk C7 of CORE_BLUEPRINT §6 lands Surface.zig:958-1523 in
`crates/rustty-core/src/surface.rs`: `handleMessage` (all 22 apprt surface
`Message` arms, checked exhaustively against the reference switch),
`selectionScrollTick`, `childExited` + `childExitedAbnormally`,
`passwordInput`, and `searchCallback`/`searchCallback_` (fully REAL per
blueprint §5 — the vt search thread is ported; the three `wakeup.notify`
sites are the xev D6 leaves, runtime panics confined there). NO new
deferral was created and none resolved. Ledger:

- **Ported ahead of its row (not a deferral):** `queueRender`
  (Surface.zig:2461-2465, C8's range) — the one-line xev-blocked leaf
  (`renderer_thread.wakeup.notify()`); selectionScrollTick/passwordInput in
  THIS range call it, so the real body (typed `Result<(),
  rustty_xev::NotifyError>`) lands now instead of a stub.
- **Growth point realized (documented in-file, not a deferral):**
  `surface::Error` grew from the opaque unit marker into the typed union of
  the concrete sources C7's bodies reach — `Apprt(ApprtError)` (performAction
  / getCursorPos), `Xev(NotifyError)` (the notify leaves), `OutOfMemory`
  (vt `Flattened.clone`); later chunks keep growing it.
  `childExitedAbnormally` is typed precisely as `Result<(),
  terminal::pagelist::OutOfMemory>` (its inferred set is Allocator.Error-only
  after the join/allocPrint OOM collapse).
- **Documented shape mappings (not deferrals):** the guard-less zigstd-Mutex
  `defer unlock`s realized explicitly at every exit (selectionScrollTick /
  passwordInput / childExited's terminal block), and via the
  immediately-invoked-closure error scoping for childExitedAbnormally's
  ~20-`try` locked tail (the vt search/screen.rs precedent,
  `#[allow(clippy::redundant_closure_call)]`); `std.mem.sliceTo(..., 0)` ->
  the `iter().position(|&b| b == 0).unwrap_or(len)` slice (rustty-os pty.rs
  precedent) at set_title/desktop_notification; the searchCallback userdata
  `?*anyopaque`/`ud.?` -> the vt seam's bare `*mut c_void` (unwrap collapses
  into the non-null registration contract); `alloc.dupe + in-place clone` ->
  one clone-into-Vec pass (searchCallback_ viewport matches, same failure
  points); R2 identity in childExitedAbnormally's user-facing "Ghostty failed
  to launch..." -> `rustty_identity::APP_NAME`.
- **Additive growth (upstream crate, Rust-only glue):** rustty-xev
  `NotifyError` gained `Clone/Copy/PartialEq/Eq` derives (required by the
  `surface::Error` union's derives; faithful — Zig error sets are trivially
  copyable/comparable, survives the real libxev fill).
- **SEQUENCING STUBS (not deferrals; later chunks fill IN PLACE):**
  `updateRendererHealth`/`updateScrollbar`/`updateConfig` (C8,
  Surface.zig:1677/1689/1714), `clipboardWrite`/`setSelection` (C9,
  :2162/:2335 — **FILLED by C9**), `posToViewport` (C13, :4726),
  `startClipboardRequest`/`showDesktopNotification`/`presentSurface` (C15,
  :5822/:5968/:6023) — `unimplemented!()` stubs carrying the reference doc
  comments and exact signatures; C7's arms are their first callsites. The C6
  `#[allow(dead_code)]`s on `surfaceMailbox`/`queueIo` are removed (first
  consumers landed); `searchCallback`/`searchCallback_` carry the allow until
  the C14 search registration names them.

## rustty-core C9 (core tier) — text / selection / clipboard-write tier

Chunk C9 of CORE_BLUEPRINT §6 lands Surface.zig:1867-2395 in
`crates/rustty-core/src/surface.rs`: `Text` (+ hoisted `TextViewport`),
`dumpText`/`dumpTextLocked`, `hasSelection`, `selectionString`, `pwd`,
`resolvePathForOpening`, `imePoint`, `clipboardWrite`,
`copySelectionToClipboards`, `setSelection`, `setSelectionAndCopy` — all
LIVE (no GPU/xev leaf in this range; the C7 SEQUENCING STUBS
`clipboardWrite`/`setSelection` above are filled). NO new deferral was
created and none resolved. Ledger:

- **Mutex fidelity:** `imePoint` locks/unlocks WITHOUT defer exactly as the
  reference does — one lock line, cursor + preedit width read under it, one
  unlock line, verbatim pairing. The `defer unlock`-shaped bodies
  (`dumpText`/`hasSelection`/`selectionString`/`pwd`) realize the defer as
  the explicit unlock at every exit (the C7/C8 convention).
- **PORT helpers (Rust-only std glue, file scope, not deferrals):**
  `mod zig_base64` — the Zig std.base64 STANDARD codec Decoder surface
  (`calcSizeForSlice`/`decode`) reproduced bit-for-bit from lib/std/base64.zig
  0.15.2 (u12 accumulator emulated masked; the 16-/4-char fast loops elided
  as accelerative-only — pads never reach them, the scalar tail is the
  semantic core; error split `InvalidPadding`/`InvalidCharacter` preserved —
  the reference branches on it); `is_absolute`/`resolve`/`os_str_from_bytes`
  (std.fs.path analogues — the rustty-config theme.rs/path.rs helpers,
  verbatim); `copy_options` (the Zig `Options` value-copy realization,
  map KEPT — unlike vt formatter.rs's extras-only `copy_options`).
- **Additive growth (upstream crate, Rust-only glue):** rustty-vt
  `formatter::CodepointMap`/`Replacement` gained `Clone` derives (Zig copies
  the `Options` MultiArrayList header shallowly at `var copy = opts`; the
  owned Rust `Options` clones the entries — same observable content).
- **Documented shape mappings (not deferrals):** `ClipboardContent` BORROWS
  its bytes, so `copySelectionToClipboards` collects owned `(mime, data)`
  pairs (the collapsed arena's allocations) and materializes the borrowed
  view at the `setClipboard` loop; `aw.toOwnedSliceSentinel(0)` ->
  `mem::take(&mut aw)` on the `Vec<u8>` allocating-writer sink (vt
  `selectionString` convention, length excludes the NUL sentinel);
  `allocSentinel(u8, size, 0)` -> `vec![0u8; size + 1]` with the explicit
  `buf[size] = 0` sentinel write; `pwd`/`resolvePathForOpening`
  `Allocator.Error!` collapses (infallible Rust allocation, app.rs
  precedent). `resolvePathForOpening` (C13 processLinks :4396) and
  `setSelectionAndCopy` (C12 :3866 / C13 :4086) carry `#[allow(dead_code)]`
  until their callsites land.

## rustty-core C10 (core tier) — key tier + crash ThreadStateSurface un-defer

Chunk C10 of CORE_BLUEPRINT §6 lands Surface.zig:2605-3281 + 6014-end in
`crates/rustty-core/src/surface.rs`: `keyEventIsBinding`, `keyCallback`,
`maybeHandleBinding`, `deactivateAllKeyTables`, `catchAllIsIgnore`,
`endKeySequence` (the C8 SEQUENCING STUBS filled; the `KeySequenceQueued`/
`KeySequenceMemory` allows removed), `encodeKey`, `encodeKeyOpts` — all
LIVE — plus the 6014-end tail: `presentSurface` LIVE and `getProcessInfo`
split into `getForegroundPid`/`getTtyName` (the rustty-os pty.rs /
`Termio::getProcessInfo` comptime-return precedent). NO new deferral was
created; ONE deferral RESOLVED-confirmed: the rustty-crash
`ThreadStateSurface` entry above closes (the impl + `crashThreadState`
landed with C8 ahead of this row; C10 — the owning row — adds keyCallback's
`crash.sentry.thread_state` site, the third [ADV 6.1] `ThreadStateGuard`
site). Ledger:

- **Inspector defer block (CORE_BLUEPRINT §4):** keyCallback's
  `defer if (insp_ev) |ev| ev: {...}` maps onto the `InspEvGuard` Drop
  (ThreadStateGuard family; declared AFTER the crash guard so it drops
  FIRST — the reference LIFO defer order). The None-arm (`ev.deinit`) is
  LIVE; the Some-arm (`insp.recordKeyEvent` + queueRender kick) is
  `unreachable!()` with the verbatim body preserved — `Inspector` is
  uninhabited until Phase 3, so the guard's raw surface pointer is never
  dereferenced today (documented at the type).
- **Entry representation:** the reference `Binding.Set.Entry` (key/value
  POINTER pair) is the `(*const Set, usize)` pair (the ported `getEvent`
  returns the entry INDEX — app.rs keyEvent precedent);
  `value_at`/`key_at` are the `entry.value_ptr.*`/`entry.key_ptr.*` reads.
- **Additive growth (upstream crates, Rust-only glue — blueprint risk 7):**
  rustty-input `binding::Set::key_at` (the `value_at` sibling; the
  `.key_sequence` trigger payload read); core `TableStackEntry` gained
  `Clone`/`Copy` derives (the reference copies elements out by value,
  `const table = table_items[rev_i];`).
- **Mutex fidelity:** keyCallback's KAM check / link-refresh /
  scroll-to-bottom blocks and `encodeKeyOpts` realize each `defer unlock`
  as the explicit unlock at every exit (the C7/C8/C9 convention).
- **Comptime mappings:** `encodeKeyOpts`'s
  `if (comptime builtin.os.tag != .macos) return opts;` is the `cfg!` early
  return (the init xdg-terminal-exec precedent); the two `OptionAsAlt`
  enums (rustty-input config-side vs rustty-vt key_encode-side, one type in
  the reference) bridge through a documented 1:1 variant map at the single
  assignment site.
- **Writer mappings:** the `.fixed(&data)` small-array writer is an
  `io::Cursor` over `[u8; WriteReq::MAX]` (overflow == the reference
  `error.WriteFailed` arm); the `Allocating` writer is the growable `Vec`
  sink (its OOM-only `try` collapses; `WriteReq.init`'s OOM likewise —
  termio message.rs precedent).
- **Ported ahead / stubs:** `closingAction` (C14's row, Surface.zig:5633)
  ports LIVE — maybeHandleBinding's closing check needs it; STUBS added for
  `mouseShiftCapture` (C12, :3712) and `hideMouse` (C13, :4741) —
  keyCallback is their first caller. `modsChanged`/`mouseRefreshLinks`
  allows removed (first callsites live). textCallback's doc comment
  (:3276-3281, C10/C11 boundary straddle) is preserved verbatim for C11.
- **C0-DECISION propagation:** the main `impl<A> Surface<A>` block gained
  the `A::Surface: ApprtSurface<App = A, Core = Surface<A>>` where-clause
  (app.rs precedent) — maybeHandleBinding calls the constrained
  `App::performAllChainedAction`.

## rustty-core C11 (core tier) — focus/scroll/content-scale callback tier

Chunk C11 of CORE_BLUEPRINT §6 lands Surface.zig:3282-3639 in
`crates/rustty-core/src/surface.rs`: `textCallback`, `occlusionCallback`,
`focusCallback`, `refreshCallback`, `scrollCallback`, `contentScaleCallback`
— all LIVE (the C10-preserved textCallback doc-comment straddle is realized
as the live `///` docs). NO new verbatim-comment deferral was created; NO
deferral resolved. Ledger:

- **File-scope hoist:** the reference declares `ScrollAmount` (+ the
  anonymous `enum { down_left, up_right }` return of `direction`) BETWEEN
  refreshCallback and scrollCallback (Surface.zig:3405); Rust impl blocks
  cannot hold type items, so both port at module scope in the trailing
  cluster (`ScrollAmount`/`ScrollAmountDirection` — the KeySequenceQueued /
  TextViewport precedents), with a pointer PORT NOTE at the reference
  position inside the impl.
- **Comptime unroll:** focusCallback's modifier-release
  `inline for (keys)` × `inline for (sides)` meta-loop (no `@field` in
  Rust) replays through a fn-local `macro_rules! release_mod` — same mods
  order (shift, ctrl, alt, super), same side order (right, left), same
  keyname map (ctrl -> control, super -> meta). The
  `assert(self.keyCallback(...) catch .ignored != .closed)` sites hoist the
  side-effecting keyCallback out of the assert (it always evaluates;
  `debug_assert!` gates only the comparison), the caught error collapsing
  to the `.ignored` fallback with the log.warn as comment.
- **Crash metadata:** six more [ADV 6.1] `ThreadStateGuard` sites (one per
  callback); scrollCallback documents the LIFO defer order (guard drops
  AFTER the explicit mutex unlocks).
- **Mutex fidelity:** scrollCallback's `defer unlock` block realizes the
  explicit unlock at every exit — 2 setSelection error exits, 2
  getCursorPos error exits, the alternate-scroll return, the mouse-report
  return, the block end (the C7/C8/C9 convention). focusCallback's
  lock/assign/unlock has no defer — paired verbatim.
- **Comptime mappings:** scrollCallback's
  `builtin.target.os.tag.isDarwin()` is `cfg!(target_vendor = "apple")`
  (the childExitedAbnormally precedent); `@intFromFloat`/`@trunc`/`@round`/
  `@abs` map to `as`/`trunc()`/`round()`/`abs()`+`unsigned_abs()`.
- **Ported ahead / stubs:** STUBS added for `isMouseReporting` (C12, :3640)
  and `mouseReport` (C12, :3646) — scrollCallback is their first caller —
  and `completeClipboardPaste` (C15, :5844) — textCallback is its first
  caller. `keyboardLayout` needed NO new work: the apprt-api
  `App::keyboard_layout` growth method's only core callsite (encodeKeyOpts,
  Surface.zig:3271) landed with C10.

## rustty-core C12 (core tier) — mouse report/button tier

Chunk C12 of CORE_BLUEPRINT §6 lands Surface.zig:3640-4278 in
`crates/rustty-core/src/surface.rs`: `isMouseReporting`, `mouseReport`,
`mouseShiftCapture`, `mouseCaptured`, `mouseButtonCallback`,
`maybePromptClick` — all LIVE (the C11-declared `isMouseReporting`/
`mouseReport` stubs filled IN PLACE). NO new verbatim-comment deferral was
created; NO deferral resolved. Ledger:

- **mouse_encode seam:** `mouseReport` is the first core caller of
  vt's `input.mouse_encode` — the ported `Options::from_terminal` takes the
  two terminal-flags reads (`mouse_event`/`mouse_format`) directly (the
  mouse_encode.rs constructor-seam note); `opts.last_cell =
  &self.mouse.event_point` realizes the reference `?*?point.Coordinate` as
  the `Option<&mut Option<Coordinate>>` borrow. The fixed `.fixed(&data)`
  writer over `WriteReq.Small.Array` is a `std::io::Cursor` (the encodeKey
  precedent); its buffer-full failure is io's `WriteZero` — the reference's
  `error.WriteFailed` log-and-return arm.
- **Error growth point:** `maybePromptClick`'s `try std.fmt.bufPrint`
  (`error.NoSpaceLeft`) grows the core `Error` union with `Io(std::io::
  Error)` (the documented C7-precedent growth; the click_events response is
  written through the same fixed-Cursor mapping).
- **Zig defer scoped to an `if` block:** mouseButtonCallback's
  `if (self.inspector != null) { defer self.queueRender() catch {}; }`
  fires at the block's closing brace — immediately; ported as the direct
  discarded call with a PORT NOTE stating the scope semantics.
- **Mutex fidelity:** the four `defer unlock` regions (left-release,
  report, left-press click, right-click sel) realize the explicit unlock at
  EVERY exit (the C7..C11 convention) — 39 unlock sites total, including
  the `break :click`/`break :sel` early exits. The right-click
  `copy-or-paste`/`paste` arms' inverted `unlock(); defer lock();` is
  replayed verbatim: relock at the arm end on success, and on the
  startClipboardRequest error exit the LIFO defer order (arm relock, then
  outer unlock) is replayed as the paired lock();unlock() before the
  `return Err` — observable-behavior faithful (the brief reacquire is what
  Zig executes). `mouseShiftCapture`'s CONDITIONAL `defer if (lock)
  unlock()` is the `if lock { unlock }` at each of its four exits.
- **Instant collapse:** the two `std.time.Instant.now()` error arms
  (extend_selection's `else |err| break`, the click block's
  `catch |err| time: null`) collapse — Rust `Instant::now` is infallible;
  both arms preserved verbatim as comments. `now.since(click_time)` maps to
  `duration_since(..).as_nanos() as u64` (the selection_gesture
  `time_since` mapping).
- **runtime-safety unreachable:** the four
  `if (comptime std.debug.runtime_safety) unreachable;` sites map to
  `if cfg!(debug_assertions) { unreachable!(); }` (debug crash, release
  fall-through — the vt screen.rs precedent).
- **Keyword collision:** maybePromptClick's `const move =` local is the
  raw identifier `r#move` (keeps the reference name; the `r#ref`/`r#match`
  precedent). The log-only `key` half of the click_events destructure is
  consumed via `let _ = key;` at its log-comment site (the alloc_gpa
  precedent).
- **Ported ahead / stubs:** STUBS added for the C13 helpers this range
  calls — `cursorPosCallback` (:4512, the extend_selection callsite),
  `linkAtPin` (:4316, the double-click URL callsite), `processLinks`
  (:4386, the release link-click callsite). `setSelectionAndCopy`'s
  C9-landed `#[allow(dead_code)]` removed (mouseButtonCallback :3866 is its
  first live caller); `showMouse` (C13 stub) gained its :3771 callsite.

## rustty-core C13 (core tier) — link/cursor-pos/color-scheme tier

Chunk C13 of CORE_BLUEPRINT §6 lands Surface.zig:4279-4774 in
`crates/rustty-core/src/surface.rs`: `linkAtPos`, `linkAtPin`,
`mouseModsWithCapture`, `processLinks`, `openUrl`, `osc8URI`,
`mousePressureCallback`, `cursorPosCallback`, `colorSchemeCallback`,
`posToViewport`, `scrollToBottom`, `hideMouse`, `showMouse` — all LIVE (the
C12-declared stubs filled IN PLACE, relocated to their reference order:
`linkAtPos`/`linkAtPin` sit after `maybePromptClick` where the reference
declares them). NO new verbatim-comment deferral was created; NO deferral
resolved. Ledger:

- **vt oniguruma feature turned ON for core:** `linkAtPin`'s
  `strmap.searchIterator(link.regex)` is vt's `build_options.oniguruma`-gated
  StringMap search surface. The reference build compiles Ghostty WITH
  oniguruma, so rustty-core (the reference-binary tier) deps
  `rustty-vt = { features = ["oniguruma"] }`. Per-crate
  `cargo test -p rustty-vt` still runs the default feature set (2019 —
  unchanged; the 3 gated string_map tests stay feature-gated, faithful to the
  Zig `SkipZigTest` arms, and pass under `--features oniguruma`).
- **Zig value-copy handles (searchIterator):** the reference copies BOTH the
  `StringMap` slice headers and the `oni.Regex` handle by value into the
  iterator (shallow, non-owning). The vt `search_iterator` takes both by
  value, so the regex copy is the aliasing handle struct
  (`oni::Regex { value: link.regex.value }` — only `DerivedConfig`'s explicit
  deinit chain frees the OnigRegex; no `Drop`) and the map MOVES through the
  iterator, recovered after each link's scan (`strmap = it.map`); the
  reference `defer strmap.deinit` is the ownership drop at every exit
  (including the moved-into-iterator error/return paths). `defer
  match.deinit()` is explicit at both iteration exits.
- **rustty-os `open` retyped to the reference byte URL (R3):** the landed
  `os::open(kind, url: &str)` had narrowed the reference's
  `url: []const u8`; `openUrl`'s fallback (`try internal_os.open(...)`) is
  its first caller, so `open` is retyped to `url: &[u8]` (POSIX argv takes
  the bytes as `OsStr` via the path.rs `os_str_from_bytes` mapping, now
  `pub(crate)`; OSC8 URIs are arbitrary bytes — never a lossy `String`).
  Zero external callers existed; rustty-os tests unchanged. The apprt
  `OpenUrlKind` maps arm-for-arm onto the os-local R4-seam `Kind` (the
  os/open.rs break-#14 parity image).
- **Error growth points:** `linkAtPin`'s `try it.next()` grows the core
  `Error` union with `Oni(oni::Error)`; `openUrl`'s fallback grows it with
  `OsOpen(internal_os::open::OpenError)` (the documented C7-precedent
  growth).
- **osc8URI unbound-lifetime return:** the reference returns a slice into
  the pin's PAGE memory (`entry.uri.slice(page.memory)`) and literally
  discards `self` (`_ = self;`). The port states that contract with an
  explicit caller-chosen lifetime (`fn osc8URI<'a>(&self, ..) -> Option<&'a
  [u8]>` — vt's `OffsetSlice::slice` is itself caller-lifetime): valid while
  the renderer mutex is held and the page node is alive. This is what lets
  `processLinks` pass the borrowed URI to `openUrl(&mut self, ..)` exactly as
  the reference does.
- **Inspector Some-arm (CORE_BLUEPRINT §4):** cursorPosCallback's
  `if (self.inspector) |insp| { insp.mouse.last_* = ..; }` block writes the
  Phase-3 `Inspector.mouse` state — the pointee is uninhabited, so the arm is
  preserved verbatim above `unreachable!()` (the C6 deinit teardown-arm
  precedent). Resolved by the Phase-3 inspector fill.
- **[ADV 6.1] completion:** mousePressureCallback/cursorPosCallback/
  colorSchemeCallback add the LAST three `crash.sentry.thread_state`
  guard sites — all 13 reference sites are now live (`ThreadStateGuard`).
- **Mutex fidelity:** the four `defer unlock` regions (mousePressure's
  deep-press select block, cursorPos's negative-pos block, cursorPos's
  function-scope remainder incl. the mouse-report early `return` and the
  runtime-safety `return`, scrollToBottom's caller-held precondition) realize
  the explicit unlock at EVERY exit — the C7..C12 convention.
- **`@enumFromInt(i)` (cursorPos button scan):** the click_state indices ARE
  the `Button` discriminants (0..=10; the array length is `Button.max`) —
  realized as the exhaustive index match, `unreachable!()` past 10.
- **Keyword collision:** mouseModsWithCapture's `var final` local is the raw
  identifier `r#final` (the C12 `r#move` precedent).
- **Stale-note fixes:** `notifyConfigConditionalState`/`resolvePathForOpening`
  `#[allow(dead_code)]` removed (colorSchemeCallback :4720 / processLinks
  :4396 are their first live callers); the C9 `setSelectionAndCopy` note
  misattributed :4086/:4092 to C13 — they are C12 mouseButtonCallback sites
  (live); the C14 (:5450) callsite is the one that lands later.
  `scrollToBottom` carries the only NEW `#[allow(dead_code)]` (its
  :4829/:4856/:4872 performBindingAction and :5891 completeClipboardPaste
  callsites are C14a/C15).

## rustty-core C14a (core tier) — performBindingAction first half (the highest-review-load switch)

Chunk C14a of CORE_BLUEPRINT §6 lands Surface.zig:4775-5202 in
`crates/rustty-core/src/surface.rs`: `performBindingAction`'s app-scoped
forwarding block + the surface-scoped switch arms `.csi`/`.esc` through
`.scroll_to_row`. The second half (`.scroll_to_selection`.. Surface.zig:5204-5625)
+ `closingAction` land with C14b. Every dispatch arm was checked against the
CORE_BLUEPRINT §2 [ADV-FIX 4] regenerated Action checklist; `copy_title_to_clipboard`
(:5065) is live. Scoped Surface.zig:4775-5202 G3b → 45 distinctive comments,
missing=0. No new tests (0 in the reference; Surface<A> is untested-by-construction).

- **`Scoped(.surface)` machinery (rustty-input):** `ScopedSurface<'a>` +
  `Action::scoped_surface` added to `binding.rs` — the C4-deferred surface
  scoped-union, landed with this consumer (see the C4 note). rustty-input test
  count unchanged (113; the reference has no `test {}` for these).
- **`app::Error` -> `surface::Error` bridge:** the app-scoped block calls
  `App.newWindow`/`App.performAction` (returning `app::Error`); a new
  `From<app::Error> for surface::Error` folds it back into the surface error set
  (`Apprt` passes through, `Surface` unwraps) — the reference's single inferred
  error union. Non-circular (opposite edge already exists).
- **`scrollToBottom` now LIVE:** the csi/esc/cursor_key arms are its first live
  callers (:4829/:4856/:4872), so its `#[allow(dead_code)]` is removed (the C15
  completeClipboardPaste :5891 caller is still pending but no longer gates it).
- **write_stable -> writeReq (`.cursor_key`, :4880/:4882):** the reference
  `.{ .write_stable = ck.normal }` aliases the keybind config's STABLE slice
  storage. The Rust `CursorKey` OWNS its `Vec<u8>` sequences (byte-slice
  convention) and `action` is a cloned, stack-local copy dropped at return, so
  no stable pointer exists; `queueIo` also requires `Message<'static>`. The
  bytes are COPIED via `termio::Message::writeReq` (write_small/write_alloc) —
  a necessary, documented deviation preserving observable behavior (bytes reach
  the pty either way).
- **`dupeZ` collapse (`.set_surface_title`/`.set_tab_title`, :5145/:5155):** the
  reference `dupeZ`+`free` NUL-terminates for the `[:0]const u8` apprt field; the
  Rust `SetTitle.title` models that as borrowed `&[u8]` (sentinel deferred to the
  C-ABI pinning, apprt-api), so the owned copy collapses and the borrowed binding
  bytes pass directly to the synchronous `performAction`.
- **`@max(0, @min(255, delta))` NaN-fidelity (`.increase`/`.decrease_font_size`,
  :5083/:5098):** kept as nested `f32::max`/`f32::min` (NOT `.clamp()`): Zig
  `@max`/`@min` and Rust `f32::max`/`f32::min` return the non-NaN operand on NaN
  (NaN delta -> 255), which `f32::clamp` does not (returns NaN). `manual_clamp`
  lint allowed at each site to preserve the semantics.
- **Mutex fidelity:** the six `defer unlock` regions (csi/esc scroll block, text
  scroll block, cursor_key `normal:` block, reset, copy_to_clipboard, copy_url,
  clear_screen alternate-check, scroll_to_row) realize the explicit unlock at
  EVERY exit — the C9..C13 convention; the copy_to_clipboard/copy_url fallible
  calls unlock before propagating/returning.

### RESOLVED (`.search` arm, Surface.zig:4933) — SEARCH_BINDING_ARM_DESIGN

The `.search` arm now lands LIVE (the `unimplemented!()` + its verbatim body
are replaced by the faithful translation, reference lines preserved as `//`
comments — R1). Both blockers are resolved per
`.port/phase-2/SEARCH_BINDING_ARM_DESIGN.md`:

1. **Search-thread mutex-type unification — DONE.**
   `terminal::search::thread::Options.mutex` is retyped from
   `*const std::sync::Mutex<()>` to `*const rustty_zigstd::thread::Mutex` (the
   shared guard-less C0 mutex, so `self.renderer_state.mutex` is
   type-compatible). The four internal lock sites (`thread_main_loop`/`select`/
   `change_needle`/`refresh_callback`) use a local `defer`-unlock guard
   (`lock_deferred`, exit-safe across `?`/early-`return`); the vt `search_thread`
   test's `mutex` local is retyped (still GREEN, the load-bearing regression
   gate). No new crate edge (rustty-vt already deps rustty-zigstd — R4 clean).
2. **Self-referential construct-then-spawn — DONE.** `Search.thread` retyped to
   `Option<JoinHandle>` (`None` == the reference `.thread = undefined`); the
   `Search` is stored FIRST, a stable `*mut Thread` is projected from the stored
   state (`&raw mut (*search_ptr).state`), the thread is spawned on THAT raw
   pointer, and the handle is written back through a disjoint raw sub-place
   (`(*search_ptr).thread = Some(handle)`), never a `&mut Search` reborrow. Post-
   spawn producers reach the running thread ONLY through DECOUPLED handles on
   `Search` (`mailbox: *const Mailbox`, `wakeup`/`stop: xev::Notifier`) — proven
   SB+TB clean on `.port/phase-2/mirihar-chunk3b/` (`search_shipped`). The
   producer/deinit/`navigate_search` retrofits + the reverse-direction
   `searchCallback_` retype off `&mut Surface` to `*mut Surface` (narrow raw
   projections, proven `callback_shipped`) are the substantive part. **Harness
   finding (corrected design §2.4):** the decoupled mailbox pointer must be
   derived from the POST-MOVE `state_ptr` lineage, NOT `addr_of!(*state.mailbox)`
   off the pre-move local — the pre-move Box provenance is popped by the child's
   `Box<Mailbox>` noalias (SB+TB UB); the corrected lineage is what shipped.

Testability: the vt search-thread machinery is exercised by the retyped
`search_thread` test (spawn-search-over-a-Screen, GREEN); the Surface-level arm
is COMPILE-faithful (a full `Surface` is windowing-blocked at
`Renderer::surfaceInit`, so it cannot be driven headlessly) with its two risky
ingredients independently verified (the vt test for the machinery, the mirihar
harness for the borrow shape). No `unimplemented!()` remains on the arm.

Every OTHER search-related first-half arm was already LIVE: `.start_search` /
`.search_selection` (apprt-only notify), `.end_search` (`self.search.take()` +
`deinit` + apprt notify), `.navigate_search` (now retrofit to the decoupled
producer handles). Their Some-branches, dormant while `self.search == None`, now
run against a real thread once `.search` spawns one.

## rustty-core C14b (core tier) — performBindingAction second half + closingAction

Chunk C14b of CORE_BLUEPRINT §6 lands Surface.zig:5204-5642 in
`crates/rustty-core/src/surface.rs`: the surface-scoped switch arms
`.scroll_to_selection` through `.adjust_selection` (the second half of
`performBindingAction`), completing the switch. `closingAction` (:5633) was
already ported ahead of its row in C14a (this chunk restored its dropped `///`
doc comment). **Arm-count diff (mechanical):** the reference
`switch (action.scoped(.surface).?)` has 67 case-label groups (26 first-half +
41 second-half; `.csi,.esc` / the `inline` tab group / the `inline` key-table
group each count once); the Rust `match action.scoped_surface().unwrap()` has
67 arms (26 C14a + 41 C14b, mirroring the same three combined arms) and is
EXHAUSTIVE with no `_` wildcard — restoring the reference's exhaustive-switch
safety. `performBindingAction` is now arm-complete. Every dispatch arm was
checked against the CORE_BLUEPRINT §2 [ADV-FIX 4] Action checklist
(`new_split`/`goto_split`/`goto_window`/`resize_split`/`inspector`/`key_table`/
`float_window`/`secure_input`/`readonly`/`toggle_fullscreen`/`goto_tab`/
`close_tab`/`move_tab` all live). Scoped Surface.zig:5204-5642 G3b → 28
distinctive comments, missing=0. No new tests (0 in the reference; `Surface<A>`
is untested-by-construction). `#[allow(dead_code)]` on `MAX_ACTIVE_KEY_TABLES`
removed — the `.activate_key_table` arm (:5495) is now its live consumer.

- **SEQUENCING STUB (not a deferral; C15 fills IN PLACE): `writeScreenFile` +
  the file-scope `WriteScreenLoc` enum.** The three write arms
  (`.write_screen_file`/`.write_scrollback_file`/`.write_selection_file`,
  Surface.zig:5250-5263) call `self.writeScreenFile(loc, v)`, but the callee
  body (Surface.zig:5651-5777 — `internal_os.TempDir` + `terminal.formatter`
  `ScreenFormatter` + `openUrl` file I/O) is C15's row (5651-6036). C14b
  forward-declares the `writeScreenFile` method as an `unimplemented!()`
  skeleton (the C3/C6 skeleton precedent — startClipboardRequest/
  showDesktopNotification are the same pattern) plus `WriteScreenLoc` at file
  scope (a Rust `enum` cannot live in the `impl` block; the reference declares
  it at file scope between `closingAction` and `writeScreenFile`, :5644). C15
  fills the body IN PLACE (all deps — TempDir, ScreenFormatter, openUrl — are
  already ported, so it is a straight port, not a deferral). The three write
  arms THEMSELVES are fully ported (they are in C14b's range).
- **`get_ptr` -> `get` for the key-table lookup (`.activate_key_table`,
  :5478):** the reference `self.config.keybind.tables.getPtr(name)` returns a
  mutable `*Set`; the stack only STORES and compares the pointer, never mutates
  through it, so the shared `get` (`?&Set` cast to a raw `*const Set`, matching
  `TableStackEntry.set`) is sufficient. The `Vec<u8>`-keyed `AutoArrayHashMap`
  takes `&Vec<u8>`, so the `[]const u8` name is copied into a lookup key (a
  small temporary, behavior-preserving).
- **`.adjust_selection` copy-out/write-back (:5577):** the reference `sel` is a
  `*Selection` aliasing `screen.selection` that `sel.adjust(screen, ...)`
  mutates in place while ALSO passing `screen` as `&Screen` — a `&mut`-field /
  `&`-parent overlap that is UB in Rust (rust-mut-args memory). The port copies
  the selection out (`Selection: Copy`), adjusts the independent copy against
  `&*screen`, then writes it back — behavior-identical because `adjust` reads
  only `s.pages`, never `s.selection`. The subsequent `scroll:` labeled block
  and `dirty.selection = true` read the (written-back) adjusted value.
- **Mutex fidelity:** the four `defer unlock` regions (`.scroll_to_selection`
  inner block, `.select_all`, `.adjust_selection`) realize the explicit unlock
  at EVERY exit — the C9..C14a convention; the fallible `setSelectionAndCopy`/
  `queueRender` calls unlock before propagating. `.scroll_to_selection`'s
  `orelse return false` unlocks the inner block before returning.
- **`-1 * rows` -> `-rows` (`.scroll_page_up`, :5219):** clippy `neg_multiply`
  rejects the literal-multiply; the negation is semantically identical (the
  reference comment preserves `-1 * rows`).

## rustty-core C15 (core tier) — clipboard completions / screen-file / notifications

Chunk C15 of CORE_BLUEPRINT §6 lands the Surface.zig file TAIL (5651-6036) in
`crates/rustty-core/src/surface.rs`: `writeScreenFile`, `completeClipboardRequest`,
`startClipboardRequest`, `completeClipboardPaste`, `completeClipboardReadOSC52`,
`showDesktopNotification` — all LIVE (no GPU/xev leaf in this range).
`crashThreadState`/`presentSurface`/`getProcessInfo` had already landed with
C8/C10 (the 6014-end tail). With this chunk the Surface.zig body is COMPLETE.
Scoped Surface.zig:5651-6036 G3b → 65 distinctive comment lines + 176 distinctive
code lines, missing=0. No new tests (0 in the reference; `Surface<A>` is
untested-by-construction). NO new deferral was created. RESOLVED: the C14b
`writeScreenFile` + `WriteScreenLoc` SEQUENCING STUB and the C7 SEQUENCING STUBS
`startClipboardRequest`/`showDesktopNotification` (`completeClipboardPaste`,
declared as a C11 stub, is filled here too) — all filled IN PLACE. Ledger:

- **`error.UnsafePaste` / `error.UnauthorizedPaste` grown onto the `surface::Error`
  union** — the two are LOAD-BEARING: apprt (embedded.zig:714-716) catches them
  specifically to show a clipboard-confirmation prompt rather than logging, so
  they must stay distinguishable variants (not collapsed into a generic error).
  Unit variants (no wrapped payload, no `From` impl); constructed directly at
  `completeClipboardPaste` (:5886) / `completeClipboardReadOSC52` (:5935). The
  exhaustive `Display` impl gained arms for both.
- **`writeScreenFile` errdefer/defer → `TmpDirGuard` Drop guard.** The reference
  `errdefer tmp_dir.deinit()` PLUS the null-selection explicit `tmp_dir.deinit()`
  are realized by a local `struct TmpDirGuard { td, armed }` whose `Drop` deinits
  the TempDir (deleting the tree) unless `armed` is cleared. Only the
  success-WITH-DATA path disarms it — the reference deliberately leaves the temp
  file on disk for the copy/open/paste consumer; every `?` error exit and the
  null-selection early-return leave it armed, exactly reproducing errdefer + the
  explicit deinit (the `ThreadStateGuard` Drop-guard precedent). `defer
  file.close()` == the `BufWriter<File>` dropping BEFORE `tmp_dir` (reverse-
  declaration drop == Zig defer LIFO). Minor, unix-safe reordering documented at
  the null-selection exit: the guard's deinit runs AFTER the mutex unlock / file
  close, vs the reference's explicit-deinit-first; deleting an open file is fine
  on POSIX and the resources are independent.
- **`.mode = 0o600` re-applied via fchmod.** rustty-os `Dir::createFile(sub_path,
  read)` collapsed the reference `CreateFlags` to just `read`; the security bit
  `.mode = 0o600` (non-windows) is re-applied with `File::set_permissions`
  (fchmod on the open handle) right after creation — byte-identical END state for
  a freshly-created temp file (the transient window is inside a private temp dir).
  `read = false` in both the reference `.{}`/`.{ .mode = 0o600 }` arms.
- **`std.fs.max_path_bytes` bufPrint → owned String / PathBuf.** The `filename`
  fixed-buffer `bufPrint("{s}.{s}", .{@tagName(loc), ext})` → a `format!` String
  (the SURFACE_ID precedent; `@tagName(loc)` = the lowercase enum-tag match).
  `tmp_dir.dir.realpath(filename, &path_buf)` → the rustty-os `realpath` returns
  an owning `PathBuf`; its raw bytes are recovered by the new file-scope
  `path_to_bytes` helper (POSIX paths are bytes — R3, never a lossy String).
- **`input.paste.encode` comptime-mutability split.** The reference `encode` is
  comptime-dispatched on `@TypeOf(data)`: const `[]const u8` may return
  `error.MutableRequired`, the mutable `[]u8` retry cannot. rustty-vt's paste.rs
  already splits this into `encode` (const, fallible) + `encode_mut` (mutable,
  infallible); `completeClipboardPaste` uses both exactly as the two branches.
  `data_duped: Option<Vec<u8>>` is declared before `vecs` so it outlives the
  borrow (`encode_mut`'s output aliases the duped buffer); `Option::insert` fuses
  the reference `data_duped = buf` store with the `encode(buf, ...)` call (and
  sidesteps the `unused_assignments` on a `= Some(..)` reassignment). The `defer
  if (data_duped) |v| free` == the owned `Vec` dropping at scope end.
- **`std.base64.standard.Encoder` added to `mod zig_base64`.** C9 ported only the
  Decoder surface; `completeClipboardReadOSC52` needs the Encoder (`calcSize` +
  `encode`), added bit-for-bit from lib/std/base64.zig 0.15.2 (standard alphabet
  + `=` pad, reusing the module's `ALPHABET_CHARS`/`PAD_CHAR`). The 128-bit
  big-endian `readInt` fast loop is elided (accelerative-only, needs an aligned
  lookahead); the 32-bit `readInt` loop is scalarized to the identical byte
  shuffle (its 4th read byte's low 8 bits are unused) — the three tail branches
  (3/2/1 leftover bytes) + pad fill are ported verbatim. The OSC-52 `bufPrint`
  prefix `"\x1b]52;{c};"` (7 bytes) is written directly (the SURFACE_ID / encodeKey
  bufPrint precedent).
- **`std.hash.Wyhash` → `std::collections::hash_map::DefaultHasher`
  (`showDesktopNotification` de-dup digest).** The digest is an internal
  rate-limit key compared only against the previous notification within the same
  process (never serialized/observed cross-implementation), so the exact
  algorithm is not part of the parity contract (the font-shaper
  `shared_grid_set::Key::hashcode` precedent). SipHash's `write` is unframed, so
  `write(title); write(body)` streams `title++body` exactly as Wyhash's
  `update; update`. `std.time.Instant.now()` → infallible `Instant::now` (the
  RingBell `catch unreachable` collapse); `now.since(last)` →
  `saturating_duration_since(..).as_nanos()` vs `NS_PER_S` (the config_struct.rs
  precedent). `self.app.*` is written through the `*mut App<A>` raw pointer.
- **`getProcessInfo` comptime split** had already landed with C10
  (`getForegroundPid`/`getTtyName` — the rustty-os pty.rs comptime-return
  precedent); `crashThreadState`/`presentSurface` with C8/C10.

## rustty-core C-T (core tier) — termio mutex-family un-defers + newConfig realized

C-T of CORE_BLUEPRINT §6 (unblocked by C0's zigstd Mutex + concrete Timeout)
un-defers the termio "mailbox / mutex-only" family and realizes apprt-api's
`newConfig`. It creates NO new deferrals; it RESOLVES:

- **RESOLVED: `termio::mailbox::send`** — LIVE. Instant→forever retry via the
  new `BlockingQueue::push_or_return` (below); guard-less-mutex unlock/relock
  for the caller-held-lock inversion; `wakeup.notify()` is the xev leaf.
- **RESOLVED: `StreamHandler::{surfaceMessageWriter, rendererMessageWriter}`**
  — LIVE (see the stage-4 stream_handler cross-thread-writers entry above).
- **RESOLVED: the Termio mutex-only method family** — `changeConfig`, `resize`,
  `sizeReport`/`sizeReportLocked`, `resetSynchronizedOutput`, `clearScreen`,
  `scrollViewport`, `jumpToPrompt`, `focusGained`,
  `colorSchemeReport`/`colorSchemeReportLocked` — LIVE against the guard-less
  C0 renderer mutex (see the stage-2 Termio.zig entry above). STILL D6:
  `init`/`threadEnter`/`processOutput`/`processOutputLocked` (xev loop + the
  read-thread inspector slow path).
- **RESOLVED: `apprt.surface.newConfig`** — realized as
  `rustty_core::app::new_config` (core app glue; needs `App::focusedSurface` +
  `Surface::pwd`, both landed). The apprt-api `surface.rs` module-doc deferral
  note is updated to point here; the verbatim reference body is retained in
  apprt-api/`surface.rs` for provenance (R1).
- **NEW primitive (Rust-only glue, R1-additive — NOT a deferral):**
  `BlockingQueue::push_or_return(value, timeout) -> Result<Size, T>` hands the
  value BACK on a failed push. The reference passes messages BY COPY and
  re-pushes its copy after a failed instant push; Rust MOVES, so the retry
  needs the value returned rather than dropped (an unconditional clone would
  double-allocate owned-payload messages and diverge from Zig's shallow copy).
  The plain `push` is now the drop-on-failure wrapper over it; `AppMailbox`
  (apprt-api) and `app::Mailbox` (core) gained the parallel `push_or_return`
  (their `push` is the drop-on-failure default/delegate; `SurfaceMailbox`
  unwraps the app envelope on failure to return the caller's `SurfaceMessage`).
  +1 rustty-only datastruct unit test (`push_or_return_hands_back_on_failure`).

Gate deltas: termio **44** (unchanged — Termio/mailbox/stream_handler have 0
reference `test{}`), datastruct **76→77** (the push_or_return test), core 15,
apprt-api 20, renderer 19, crash 12 — all unchanged; workspace + clippy + fmt +
acyclic + test-counts green; scoped G3b for Termio.zig/mailbox.zig/
stream_handler.zig/apprt-surface.zig = 0 missing.

## rustty-core C-F (core tier) — FINALE: CONSOLIDATED OPEN-DEFERRAL LEDGER

Chunk C-F of CORE_BLUEPRINT §6 closes the core milestone. It creates NO new
deferrals and RESOLVES none of the per-chunk entries above by CODE — it is the
finale audit + roll-up. All of Surface.zig (6036) / App.zig (604) /
surface_mouse.zig (336) is ported LIVE (C0..C-T). This section is the single
authoritative list of what remains OPEN for core, so a future Phase-3/D6 sweep
has one place to look. Everything below is **0-test, R1-safe** — the reference
carries no `test{}` on any deferred surface.

**Full-file G3b sweep (C-F, missing=0 all three files):**
- `Surface.zig` ↔ `surface.rs`: distinctive ref comments **1000**, missing **0**.
- `App.zig` ↔ `app.rs`: distinctive ref comments **130**, missing **0**.
- `surface_mouse.zig` ↔ `surface_mouse.rs`: distinctive ref comments **39**,
  missing **0**.

**`unimplemented!()` audit (C-F, surface.rs + app.rs):** every production
`unimplemented!()` is a documented placeholder leaf or a DEFERRALS entry below;
NO stragglers.
- `surface.rs` — **0** — the two inspector entries were closed by WS6
  (2026-07-28); re-measured at that time with
  `grep -c "unimplemented!" crates/rustty-core/src/surface.rs` → `0`, so there
  is no remaining site to list. (Was 2: `activateInspector` /
  `deactivateInspector`; was 3 before the `performBindingAction .search` arm
  landed LIVE — see "RESOLVED (`.search` arm)" above.)
- `app.rs` — **8**, ALL inside `#[cfg(test)] mod test_runtime` (the §7
  TestSurface fake methods the C4 seam tests do not drive — honest test glue,
  NOT production leaves and NOT deferrals).
- `renderer_seam.rs` — **15**, the C3 dependency-placeholder bodies (below).

**OPEN core deferrals (roll-up — targets: Phase-3 inspector tier / Phase-3 R-d
GPU renderer / D6 libxev port):**

1. **Inspector activate/deactivate — RESOLVED (WS6, 2026-07-28).**
   `Surface.activateInspector` (Surface.zig:890) + `deactivateInspector` (:915)
   are LIVE at `crates/rustty-core/src/surface.rs:2873` / `:2967` (commit
   `2dfcc32`): they heap-box the inspector, publish the
   `*mut dyn InspectorOverlay` + type-erased `inspector_ptr` under
   `renderer_state.mutex`, and notify renderer + termio via
   `Message::Inspector(true/false)`. `rustty_inspector::Inspector` is inhabited
   (13 modules, G3b GREEN — WS6 T2), so nothing here is unconstructible any
   more; the `unimplemented!()`/`unreachable!()` pair is gone. The
   `keyCallback` inspector defer-block Some-arm and the `cursorPosCallback`
   Some-arm are reachable and exercised — the WS6 T5 shakedown shows real
   keystrokes in the inspector's Keyboard panel and live VT events in its
   Terminal IO panel.

2. **`performBindingAction .search` arm (Surface.zig:4933) — RESOLVED.** Landed
   LIVE per SEARCH_BINDING_ARM_DESIGN: (a) the vt `Options.mutex` retyped to the
   shared guard-less `rustty_zigstd::thread::Mutex`; (b) the self-referential
   construct-then-spawn via a stable-address raw-ptr discipline (store `Search`
   first with `thread: None`, spawn on a `*mut Thread` projected from the stored
   state, assign the handle through a disjoint raw sub-place) + decoupled
   producer handles (SB+TB clean on `mirihar-chunk3b`). See "RESOLVED
   (`.search` arm)" above for the full detail. `navigate_search`/`end_search`/
   `deinit` retrofit to the decoupled handles; `searchCallback_` retyped off
   `&mut Surface` to `*mut Surface`.

3. **`renderer_seam.rs` dependency placeholders (C3, §5).** Inhabited structs
   with `unimplemented!()` bodies (termio `xev_seam` precedent; no reference
   logic stubbed):
   - `Renderer` (generic.zig GenericRenderer) — `surfaceInit`/`init`/
     `finalizeSurfaceInit`/`threadEnter`/`drawFrame`/`deinit` + the
     `Renderer.DerivedConfig` placeholder. **Target: Phase-3 R-d GPU tier.**
   - `Thread<A>` run loop — faithful renderer/Thread.zig field shape (rustty-xev
     handles + REAL state/mailbox); `init`/`threadMain`/`deinit` unimplemented.
     **Target: Phase-3 R-d + D6.**
   (`renderer_seam::Options<A>` is LIVE, not deferred.)

4. **D6 xev leaves in surface.rs (live calls into the `rustty-xev`
   placeholder).** These are COMPILED, real logic — they panic only at runtime
   once the render/io threads spin up, exactly like the GPU leaves. They are NOT
   commented-out code:
   - `queueRender` xev `wakeup.notify()` (Surface.zig:2461) — the one-line leaf
     transitively behind updateConfig/setFontSize/focus flows AT RUNTIME
     (surface.rs:3756/3817/3860/5573).
   - `searchCallback_` `state.wakeup.notify()` sites (surface.rs:10454/10483).
   - `deinit`: `renderer_thread.stop.notify()` + `io_thread.stop.notify()`
     (surface.rs:2489/2512) and `close`'s `state.stop.notify()` (:509).
   - `init`: the renderer/termio `Thread.init` xev construction + `Termio.init`
     (D6) — the runtime-panic leaves `init` is written LIVE against.
   **Target: the D6 libxev port** (fills `rustty-xev` in place).

5. **App.zig Wasm block (C4, in-file).** `App.zig:579 pub const Wasm` is
   wasm-target-gated and commented out IN the reference; preserved verbatim at
   the bottom of app.rs. **Target: the rustty wasm app-runtime** (off-by-default
   `wasm` feature family).

**Gate deltas (C-F): NONE** — no code changed; core stays **15** tests (R1-owed
1 = surface_mouse `keyToMouseShape`; +13 app.rs `mod test_runtime` seam tests +
1 lib.rs `type_core_prototype`). Whole-workspace gates green (build, all crate
tests, clippy `-D warnings`, fmt, acyclic G7, test-counts G-gate); full-file G3b
missing=0 on all three core files. Doc updates only: this ledger,
`PORT_PLAN.md` crate-24 (Tests 1→15 actual, marked DONE),
`check_test_counts.py` core_app/core_surface MAP comments (landed-state),
`NEXT_PHASE_PLAN.md` CORE DONE checkpoint. **CORE MILESTONE COMPLETE.**

## rustty-apprt embedded E0 (scaffold) — apprt-c crate + none/browser + rustty_init

Chunk E0 of the libghostty C-ABI milestone (EMBEDDED_BLUEPRINT §3/§4/§7): the
`rustty-apprt-c` top-sink crate scaffold, the `rustty-apprt::{embedded,none,browser}`
module shells, and the global-state precursor. `none.zig` (19 loc) + `browser.zig`
(4 loc) are ported FULLY here (§4 — no trait impls, faithful to the reference).
The deferrals E0 CREATES (all scheduled, nothing dropped — R1):

1. **LANDED (WS4, 2026-07-23):** the full body is `rustty_apprt::init_with_action`
   (logging config via RUSTTY_LOG, initSignals SIGPIPE, rlimits, crash.init,
   ensureLocale, glslang.init, oni.init, i18n.init, the init info-log lines);
   detection stays above apprt (ADV-FIX 2). Remaining under OTHER entries:
   tree-wide `std.log.scoped` facade routing (the per-crate scoped-log sites —
   pre-existing, not created here).

2. **LANDED (WS4).** `rustty_init` stores argv in the apprt-c `ARGV` OnceLock
   (`static ARGV: OnceLock<Vec<Vec<u8>>>`, `crates/rustty-apprt-c/src/lib.rs`)
   and detection (`detect_from_argv`) consumes it.

3. **The embedded backend + the 71 libghostty C-ABI exports.** `embedded.rs` is an
   E0 shell (only `resourcesDir`). `embedded::App`/`Surface`/`Platform`/`Inspector`
   + the `impl apprt_api::{App,Surface}` trait bodies + the surface callbacks land
   in **E1..E4/E10**; the 71 `rustty_*` `ghostty_*` wrappers + CAPI extern structs
   + D2 const-asserts land in **E5..E10** in `rustty-apprt-c`; the cbindgen
   `after_includes` type-pins (Options/Platform/KeyEvent/SurfaceSize/…), the
   `xtask header-gen` wiring for `include/rustty-apprt.h`, and the C compile+link
   smoke KAT land in **E11**. The GPU (`draw`/`refresh` → `renderer_seam`), xev
   (event loop), and macOS objc/Metal/CGS (Platform messaging, Inspector Metal,
   quicklook, `set_window_background_blur`) `unimplemented!()` leaves are CREATED
   by E3/E4/E10 — **not by E0** (E0 creates no runtime leaf; `none`/`browser` are
   inert stubs and `rustty_init` is runtime-live). **Targets: Phase-3 R-d (GPU) +
   D6 (xev) + E10 (macOS objc/CGS), per EMBEDDED_BLUEPRINT §1.**

**Gate deltas (E0):** new crate `rustty-apprt-c` (cdylib/staticlib/rlib) —
`librustty_apprt_c.{a,dylib,rlib}` build + export `_rustty_init` (verified via
`nm`). `rustty-apprt` gains `allocator-api2` (the `none::App::perform_ipc`
allocator param). MAP rows `apprt_none`/`apprt_browser` (both 0-test, GREEN).
Whole-workspace gates green (build, touched-crate tests, clippy `-D warnings`,
fmt, acyclic G7 with apprt-c a clean TOP SINK, test-counts G-gate); scoped-G3b on
none.zig/browser.zig **missing=0** (none's 1 distinctive comment matched; browser
has none). No regression (vt 2019, core 15, apprt-api 20, renderer 19, crash 12,
termio 44).

## rustty-apprt embedded E1 (App backend) — `impl apprt_api::App` + E1→E2 Surface stub

Chunk E1 of the libghostty C-ABI milestone (EMBEDDED_BLUEPRINT §2/§7): ports
`embedded.zig:29-342` into `crates/rustty-apprt/src/embedded.rs` — the
`embedded::App` struct + `Options` (the `#[repr(C)]` host-callback config) +
`KeyEvent`/`KeyTarget` + every `App` method (init/terminate/hasGlobalKeybinds/
focusEvent/keyEvent/reloadKeymap/keyboardLayout/wakeup/wait/newSurface/
closeSurface/redrawInspector/performAction/performPreAction/performIpc) + the
first-concrete `impl apprt_api::App for App` (the seam is now inhabited by a REAL
backend). Deferred out of E1:

1. **The E1→E2 `Surface` SEQUENCING STUB.** `impl apprt_api::App` binds
   `type Surface = Surface`, which forces `Surface: apprt_api::Surface` to exist
   NOW. E1 therefore carries a MINIMAL `Surface`/`SurfaceOptions` stub whose
   `init`/`queueInspectorRender` and every `apprt_api::Surface` method
   (`core`/`rt_app`/`close`/`deinit`/`get_content_scale`/`get_size`/`get_title`/
   `supports_clipboard`/`clipboard_request`/`set_clipboard`/`get_cursor_pos`/
   `default_termio_env`) are `unimplemented!()` leaves, and only the two E1-read
   fields (`core_surface`, `title`) are present. These are NOT permanent
   GPU/xev/objc leaves — they are replaced wholesale by the faithful port in
   **E2** (Surface struct + `init`/`deinit` + `impl apprt_api::Surface`, the §2
   growth-point table), **E3** (surface event callbacks), and **E4**
   (`queueInspectorRender` = Inspector, Phase-3 dcimgui/Metal). `App::newSurface`
   reaches `Surface::init` (the stub), so it panics until E2 — no E1 gate exercises
   it. **Targets: E2/E3/E4, EMBEDDED_BLUEPRINT §7.** (This is a chunk-sequencing
   stub, per the constitution's "faithful sequencing stub" rule, not a Phase-3
   runtime leaf.)

2. **`Options.write_clipboard`'s `ClipboardContent` hoist.** The reference nests
   the `ghostty_clipboard_content_s` extern struct in the `CAPI` namespace (which
   the blueprint maps to `rustty-apprt-c`, E5), but `Options` — a BACKEND field —
   references it, so E1 defines it in `rustty-apprt::embedded` (apprt-c deps apprt,
   not the reverse). **E5/apprt-c hand-pins / re-exports this exact struct** for
   the generated header (ADV-FIX 9). Likewise, `Options`'s callback signatures
   carry apprt-api's `ActionC`/`TargetC`, whose byte-exact-C FFI-safety is the
   apprt-api "C-ABI PROJECTION (deferred pinning)" — `improper_ctypes_definitions`
   is silenced on `Options` pending the **E5** D2 const-asserts + cbindgen.

3. **`config` interior mutability.** `embedded::App.config` is an
   `UnsafeCell<Config>` (not the reference bare `config: Config`) so the reference's
   `performPreAction` `config_change` reassign (`self.config = …`, embedded.zig:313,
   taking a mutable `self: *App`) is expressible under the apprt-api
   `App::perform_action(&self)` shared-ref trait. Sound because the app is
   heap-owned + single-threaded via the C-ABI; mirrors Zig's `*App` mutability. No
   deferral — a faithful `&self`↔`*App` reconciliation.

**Prerequisite (cross-crate, faithful port):** `rustty_input::Keymap` — the
`input.zig:39-44` `pub const Keymap = switch (os) { .macos => KeymapDarwin, else =>
KeymapNoop }` platform alias — was added to `rustty-input/src/lib.rs` (its first
consumer, `embedded::App.keymap`, has landed). Additive alias; no behavior/test
change.

**Gate deltas (E1):** `embedded.rs` grows from the E0 shell to the `App` backend
(+ E1→E2 Surface stub). MAP row `apprt_embedded` added (embedded.zig 0-test →
embedded.rs, GREEN). `rustty-input` gains the `Keymap` alias. Whole-workspace gates
green (build, touched-crate tests — input 113, apprt 0; clippy `-D warnings`, fmt,
acyclic G7 with apprt-c still a clean TOP SINK, test-counts G-gate); scoped-G3b on
embedded.zig **1-342 missing=0 review=0**. No regression (vt 2019, core 15,
apprt-api 20, renderer 19, crash 12, termio 44).

## rustty-apprt embedded E2 (Surface backend) — `impl apprt_api::Surface` (heap-pinned)

Chunk E2 of the libghostty C-ABI milestone (EMBEDDED_BLUEPRINT §2/§7): ports
`embedded.zig:343-609` into `crates/rustty-apprt/src/embedded.rs`, REPLACING the
E1→E2 `Surface`/`SurfaceOptions` sequencing stub wholesale with the faithful port:
`Platform`/`PlatformTag`/`EnvVar` + `PlatformC` (the C-ABI union) + the full
`Surface` struct + `SurfaceOptions` (the `ghostty_surface_config_s` extern struct)
+ `init` (out-pointer, heap-pinned) + `deinit`/`freeInspector` + the faithful
first-concrete `impl apprt_api::Surface for Surface` growth-point table
(`core`/`rt_app`/`close`/`deinit`/`get_content_scale`/`get_size`/`get_title`/
`supports_clipboard`/`clipboard_request`/`set_clipboard`/`get_cursor_pos`/
`default_termio_env`). This proves the self-referential `type Core =
rustty_core::Surface<embedded::App>` bound against a REAL backend (core's C0 gate
used a dummy `A`). The E1 leaves at `core`/`rt_app`/… are now RESOLVED (real
bodies). Deferred out of E2:

1. **`Inspector` E2→E4 SEQUENCING STUB (Phase-3 dcimgui/Metal leaf).** `Surface`
   carries `inspector: Option<*mut Inspector>` and `deinit` calls `freeInspector`,
   so the `Inspector` type must exist NOW. E2 defines a MINIMAL opaque `Inspector`
   (`#[allow(dead_code)]`) whose `deinit` is an `unimplemented!()` leaf; `deinit`/
   `freeInspector` are ported faithfully but the `if let Some(v) = self.inspector`
   arm is a no-op while `inspector` is always `None` (nothing constructs an
   `Inspector` until **E4**'s `initInspector`, which needs the dcimgui/Metal
   context). **Target: E4, EMBEDDED_BLUEPRINT §7** (`Inspector`, embedded.zig
   992-1243). Chunk-sequencing + Phase-3 leaf, not a permanent gap.

2. **`Surface::queueInspectorRender` — E3/E4 `unimplemented!()` leaf (carried from
   E1).** Reaches the `render_inspector` action (dcimgui/Metal Phase-3). Called by
   `App::redrawInspector`; kept an `unimplemented!()` placeholder. **Target: E3/E4.**

3. **GPU/xev runtime leaves reachable THROUGH E2's `init`/`deinit`.**
   `Surface::init` calls core `Surface::init` (surface.rs) which constructs the
   `renderer_seam::{Renderer,Thread}` Phase-3 placeholder leaves (blueprint §1/§3),
   and `deinit` calls the core `Surface::deinit` renderer/io-thread teardown. These
   **compile + link now** but PANIC at the renderer leaf the moment a LIVE surface
   is created — which requires `ghostty_surface_new` (**E7**) AND the renderer land
   (**Phase-3 R-d + D6**). No E2 gate exercises a live surface (the smoke KAT, E11,
   stays app-level per ADV-FIX 2/3). This is the milestone's documented honest line.

4. **macOS objc / Platform messaging — NOT E2 (raw pointers only).** Per blueprint
   §5/§8.1, `Platform`/`MacOS`/`IOS` store the RAW `*mut c_void` nsview/uiview
   (reference `objc.Object`); the view is never *messaged* until Phase-3 rendering,
   so E2 needs NO `objc2`/`coretext` dependency. The `if isDarwin() struct{…} else
   void` gating is preserved via `#[cfg(any(target_os = "macos", target_os =
   "ios"))]`. The Darwin metal/quicklook/CGS block that WOULD message objc is the
   **E10** `#[cfg]` Phase-3 leaf. No deferral introduced here — a faithful
   pointer-only Platform.

**Identity (R2):** `defaultTermioEnv` removes `RUSTTY_MAC_LAUNCH_SOURCE` (reference
`GHOSTTY_MAC_LAUNCH_SOURCE`), matching the rustty-os `desktop.rs`
`launchedFromDesktop` precedent. Comment `ghostty`/`Ghostty` → `rustty`/`Rustty`
per R1's identity-rename allowance.

**Dep added:** `rustty-apprt` gains `rustty-args` (for `zig_string_escape`, the
rustty port of `std.zig.stringEscape`, used by `init` to escape `initial_input`).
Acyclic: rustty-args deps only {os, util, build-config}, none reach apprt; G7 stays
green with apprt-c a clean TOP SINK. Reused, not reimplemented (R3/R5).

**Gate deltas (E2):** `embedded.rs` grows the full Surface backend (E1→E2 stub
gone). `apprt_embedded` MAP row unchanged (still embedded.zig 0-test → embedded.rs,
GREEN — rust ≥ zig). Whole-workspace gates green (build, touched-crate tests —
apprt 0, apprt-c 0; clippy `-D warnings`, fmt, acyclic G7 with apprt-c still a clean
TOP SINK, test-counts G-gate); scoped-G3b on embedded.zig **343-609 distinctive=39
missing=0 review=0**. No regression (vt 2019, core 15, apprt-api 20, renderer 19,
crash 12, termio 44).

## rustty-apprt embedded E3 (Surface event-sink callbacks) — refresh/draw/mouse/…

Chunk E3 of the libghostty C-ABI milestone (EMBEDDED_BLUEPRINT §7): ports
`embedded.zig:610-990` into `crates/rustty-apprt/src/embedded.rs` — the Surface
event-sink callbacks the CAPI drives (`refresh`/`draw`/`updateContentScale`/
`updateSize`/`colorSchemeCallback`/`mouseButtonCallback`/`mousePressureCallback`/
`scrollCallback`/`cursorPosCallback`/`preeditCallback`/`textCallback`/
`focusCallback`/`occlusionCallback`), each delegating to a `core_surface.*Callback`,
plus `completeClipboardRequest` (pub-widened for the E9 wrapper), `newSurfaceOptions`,
and the private `cursorPosToPixels` helper. These are real core logic (not GPU/xev
leaves) EXCEPT `draw`/`refresh` (item 1). The E2 growth-point trait method
`default_termio_env` (also inside the 610-990 range, at 953) was already landed in
E2 — E3 does not re-port it. Deferred out of E3:

1. **`draw`/`refresh` reach the Phase-3 GPU renderer leaf at RUNTIME.**
   `Surface::draw`/`refresh` delegate to core `Surface::draw`/`refreshCallback`,
   which drive the `renderer_seam` Phase-3 `unimplemented!()` leaves (blueprint §1).
   The delegation is ported LIVE (byte-faithful) — the panic is in the leaf, not
   here; it fires only once a LIVE surface exists (`ghostty_surface_new`, **E7**) AND
   the renderer lands (**Phase-3 R-d + D6**). Same honest line as E2 item 3 — E3
   introduces NO new leaf, only LIVE delegation to an already-tracked one.

2. **`initInspector` (embedded.zig:611-623) — E4, NOT E3.** Although it falls inside
   the E3 line-range (610-990), `initInspector` constructs the dcimgui/Metal
   `Inspector` (`alloc.create(Inspector)` + `Inspector.init`) — the E4 Phase-3 leaf.
   Its 3 doc-comment lines are carried by **E4**; the E3 scoped-G3b is therefore
   computed over the complement **624-990**. `queueInspectorRender` likewise stays the
   E3/E4 `unimplemented!()` leaf carried from E1 (E2 item 2), untouched here.

**Identity / ABI notes (R3):** `completeClipboardRequest`'s `str: [:0]const u8` →
`&CStr` (the NUL sentinel is LOAD-BEARING — `str.ptr` is handed to the host
`confirm_read_clipboard` callback as `[*:0]const u8`; the sans-sentinel `.to_bytes()`
view goes to core). The reference's 4th confirm-callback arg `state.*` relies on
Zig's implicit tagged-union→tag coercion (`ClipboardRequest` → `ClipboardRequestType`);
Rust has none, so the tag is projected via an explicit `match`. `newSurfaceOptions`
spells out `Surface.Options`'s Zig field defaults explicitly (Rust has no struct field
defaults); `platform = undefined` → a zeroed `PlatformC` placeholder (never read — the
C host overwrites `platform`/`platform_tag` before `ghostty_surface_new`); the
inherited `working_directory` is a leaked `CString` owned by the C host (faithful to
the reference's un-freed GPA `dupeZ`). `mouseButtonCallback`'s error arm carries the
`#[allow(clippy::manual_unwrap_or, clippy::manual_unwrap_or_default)]` PARITY marker
(the arm IS the reference `catch { log; return false }`, comment-only log → `false`).

**Gate deltas (E3):** `embedded.rs` grows the Surface event-sink callbacks +
`completeClipboardRequest`/`newSurfaceOptions`/`cursorPosToPixels`. `apprt_embedded`
MAP row unchanged (still embedded.zig 0-test → embedded.rs, GREEN — rust ≥ zig).
Whole-workspace gates green (build, touched-crate tests — apprt 0, apprt-c 0; clippy
`-D warnings`, fmt, acyclic G7 with apprt-c still a clean TOP SINK, test-counts
G-gate); scoped-G3b on embedded.zig **624-990 (E3; initInspector 611-623 → E4)
distinctive=28 missing=0 review=0**. No regression (vt 2019, core 15, apprt-api 20,
renderer 19, crash 12, termio 44).

## rustty-apprt embedded E4 (Inspector — dcimgui + Metal) — Phase-3 inhabited placeholder

**RESOLVED IN FULL (WS6, 2026-07-28).** dcimgui-sys IS wired; there is no
Phase-3 leaf left anywhere on the E4 path. Each of the four deferred items
below is closed — see the per-item RESOLVED notes. Backend rasterization is
gated by the offscreen KAT `dcimgui-sys::metal_offscreen_render_writes_pixels`
(WS6 T1), and ⌘⌥I opens a live, rendering inspector in `Rustty.app`
(WS6 T5, screenshot-verified). The historical prose below is provenance only.

Chunk E4 of the libghostty C-ABI milestone (EMBEDDED_BLUEPRINT §7, row E4): ports
`embedded.zig:992-1243` into `crates/rustty-apprt/src/embedded.rs` — the `Inspector`
(the dcimgui / Dear ImGui + Metal terminal inspector, 1:1 with a Surface) as a
**Phase-3 INHABITED PLACEHOLDER** per the placeholder-leaf doctrine. The `Inspector`
struct + the `Backend` enum are INHABITED with their real fields; E4 also lands
`Surface::initInspector` (deferred INTO E4 by the E3 section, item 2) and replaces
E2's `Inspector` E2→E4 sequencing stub (E2 item 1 below is now RESOLVED). dcimgui-sys
is **NOT wired** this milestone — every `cimgui.*`/`ImGui_ImplMetal_*`/objc body is
preserved VERBATIM as comments and lands with the GPU/dcimgui render path (Phase-3
R-d). What is REAL vs leaf:

- **REAL (non-GPU):** the `Inspector`/`Backend` structs (all fields — `surface`,
  `ig_ctx` as an opaque `*mut c_void` for the un-bound dcimgui `*ImGuiContext`,
  `backend`, `content_scale`, `instant`); `updateContentScale` (caches
  `content_scale`, the read `cursorPosCallback` needs — **the dcimgui
  style-scaling is no longer elided: WS6 T3 filled it**, so the method now also
  runs `ImGui_SetCurrentContext` + `ImGuiStyle_ImGuiStyle` /
  `ImGuiStyle_ScaleAllSizes` / `ImGui_GetStyle` exactly as the oracle
  `embedded.zig:1111-1126`, gated by the KATs
  `update_content_scale_rescales_active_imgui_style` and
  `update_content_scale_is_idempotent_not_compounding`); `updateSize` (accepts the
  size; the imgui IO `DisplaySize` write is a dcimgui leaf, method does NOT panic —
  the Inspector has no Rust-side size field, `DisplaySize` lives in imgui's IO);
  `queueRender` (forwards LIVE to `Surface::queueInspectorRender`, the draw/refresh
  delegation precedent); `Surface::initInspector` (real heap-`Box` pin + LIVE call to
  the leaf `Inspector::init`).

Deferred out of E4 (all Phase-3 leaves; NO new capability dropped — R1; every body
preserved verbatim as comments, never fabricated):

1. **`Inspector::init` / `deinit` — RESOLVED (WS6).** LIVE at
   `crates/rustty-apprt/src/embedded.rs:1933` / `:1985` (commit `027f695`):
   `init` = `ImGui_CreateContext` + `CoreInspector::setup()` +
   `activateInspector`; `deinit` = `deactivateInspector` +
   `ImGui_DestroyContext`. No `unimplemented!()` remains, live `Inspector`s are
   constructed routinely, and `freeInspector` / `Inspector::deinit` really run.

2. **`Inspector::initMetal` / `renderMetal` + `Backend::deinit` — RESOLVED
   (WS6).** LIVE at `embedded.rs:2018` / `:2107` / `:1889` (commits `86b04b4`,
   `0b8f7af`): real `ImGui_ImplMetal_Init`, the double-frame loop, the
   `renderCommandEncoderWithDescriptor:` objc2 msgSends,
   `ImGui_ImplMetal_RenderDrawData`, and `ImGui_ImplMetal_Shutdown` on the
   `.metal` arm. The two objc handles still cross the C ABI as opaque
   `*mut c_void`. That the vendored backend genuinely RASTERIZES (not merely
   links) is proven by the real-`MTLDevice` offscreen readback KAT
   `dcimgui-sys::metal_offscreen_render_writes_pixels` (WS6 T1). Driven by the
   live `rustty_inspector_metal_init`/`_metal_render`/`_metal_shutdown` exports.

3. **`Inspector::{mouseButtonCallback,scrollCallback,cursorPosCallback,focusCallback,
   textCallback,keyCallback}` + the private `newFrame` — dcimgui event/IO Phase-3
   leaves** — **RESOLVED (WS6).** All six callbacks plus the private `newFrame`
   are LIVE (commit `c62ab07`): each drives the real
   `dcimgui_sys::ImGuiIO_Add*Event` (+ `Key::imgui_key`, itself now bound).
   `textCallback`'s `[:0]const u8` → `&CStr` (NUL sentinel handed to imgui).
   `newFrame` is called by the live `renderMetal`, so the `#[allow(dead_code)]`
   crutch is gone. Proven end-to-end by the WS6 T5 shakedown: typing
   `echo hello-ws6` populates the inspector's Keyboard panel with the real
   press/release rows (`key_o`/`key_l`/`key_w`/`key_s`/`enter`, with UTF-8 and
   PTY-encoding columns) and its Terminal IO panel with the resulting VT events.

4. **`Surface::initInspector` — RESOLVED (WS6).** Always LIVE (real
   `Box::into_raw(Box::new(...))` heap-pin, mirroring the reference
   `alloc.create` + placement `.init`); it no longer reaches any leaf, because
   item 1 is live. The `?` on `Inspector::init` is once again the reference's
   own init-failure path (surfaced as a `null` return from
   `rustty_surface_inspector`, which the macOS host null-guards), NOT a port
   placeholder. The `Inspector` back-pointer to `Surface` is sound: the embedded
   `Surface` is itself heap-pinned (ADV-FIX 8).

**RESOLVED by E4 (were E2 deferrals):** E2 item 1 (`Inspector` E2→E4 sequencing stub)
— the opaque `_e4: ()` stub + `unimplemented!()` `deinit` are replaced by the real
inhabited `Inspector`. E3 item 2 (`initInspector` → E4, 3 doc-comment lines) — landed
here.

**Identity / ABI notes (R3):** `Backend` is a file-private `const` enum in the
reference; Rust's pub-field visibility (it appears in the `pub backend` field of the
`pub Inspector`) forces it `pub` (`#[derive(Debug, Clone, Copy, PartialEq, Eq)]`,
value semantics matching the Zig enum). The dcimgui `*ImGuiContext` handle → opaque
`*mut c_void` (dcimgui-sys not wired). `Inspector::init`'s `!Inspector` (sole error
`error.OutOfMemory` from `ImGui_CreateContext`) → `Result<Inspector, ApprtError>`;
`renderMetal`/`keyCallback`/`newFrame`'s `!void` → `Result<(), ApprtError>`.
`updateContentScale`'s `_ = y;` (vertical scale unused, `x` drives both axes) →
`let _ = y;`.

**Gate deltas (E4):** `embedded.rs` replaces the E2 `Inspector` stub with the full
Inspector struct + `Backend` enum + 14 methods, and adds `Surface::initInspector`.
`apprt_embedded` MAP row unchanged (still embedded.zig 0-test → embedded.rs, GREEN —
rust ≥ zig; NO new MAP entry). Whole-workspace gates green (build incl. apprt-c +
rustty + cli; touched-crate tests — apprt 0, apprt-c 0; clippy `-D warnings`, fmt,
acyclic G7 with apprt-c still a clean TOP SINK, test-counts G-gate); scoped-G3b on
embedded.zig **992-1243 (E4) distinctive=13 missing=0 review=0**. No regression (vt
2019, core 15, apprt-api 20, renderer 19, crash 12, termio 44).

## rustty-apprt-c embedded E6 (CAPI app-lifecycle exports) — runtime-live, NO leaf

Chunk E6 of the libghostty C-ABI milestone (EMBEDDED_BLUEPRINT §1/§3/§7): ports
`embedded.zig:1398-1535` — the 13 app-lifecycle + app/config `export fn ghostty_*`
into `crates/rustty-apprt-c/src/lib.rs` as `#[no_mangle] pub unsafe extern "C" fn
rustty_*`: `rustty_app_new` (+ the `app_new_` helper), `_tick`, `_userdata`, `_free`,
`_set_focus`, `_key`, `_keyboard_changed`, `_open_config`, `_update_config`,
`_needs_confirm_quit`, `_has_global_keybinds`, `_set_color_scheme`, and
`rustty_config_key_is_binding`. These are the **runtime-EXERCISABLE core** — `app_new`
builds `CoreApp` + keymap + config clone with **NO renderer and NO xev** (the GPU/xev
leaf boundary is `ghostty_surface_new`, E7+), so **E6 creates NO new GPU/xev/objc
leaf** (nothing added to the Phase-3/D6/E10 deferral set). The bodies forward to the
already-ported `rustty-apprt` embedded backend (E1) + `rustty-core` App.

**No deferrals** — every export is live. Two supporting COMPLETIONS (not deferrals),
both required by E6 and both faithful ports:

1. **`rustty-config`: `Config::key_event_is_binding`** (port of `Config.zig:6401`
   `keyEventIsBinding`, consumed by `ghostty_config_key_is_binding`). Renamed to
   snake_case per config_struct.rs's method convention. Its dep `keybind.set.getEvent`
   (`rustty_input::binding::Set::get_event`) was already ported. `&self` (body only
   reads). **0 reference `test{}`** → no test-count/MAP change; `config` suite 187,
   green.
2. **`rustty-apprt`: `embedded::KeyEvent::core` widened to `pub`** (ADV-FIX 9
   pub-widening — the reference `core` is file-private, but the apprt-c wrapper
   `ghostty_config_key_is_binding` reaches it via `event.keyEvent().core()`).

**Identity / ABI notes (R3):** the reference `app_new_`'s inferred `!*App` error set
(`CoreApp.create`'s `CreateError` ∪ `App.init`'s `KeymapError`) is modeled as one
local `AppNewError` enum so `rustty_app_new`'s single `catch |err|` log site is
preserved (`#[allow(dead_code)]` — the payloads are read only via the derived-`Debug`
log, invisible to dead-code analysis; on non-macOS `KeymapError` is `()`).
`global.alloc.create(App)`/`errdefer destroy` collapse into boxing `App::init`'s
returned value (rustty `init` is by-value, not placement-init); `opts.*` → `ptr::read`
(Options is `#[repr(C)]`). `ghostty_app_free`'s `global.alloc.destroy(v)` frees the App
box via `ManuallyDrop` (terminate already tore down — the `closeSurface` precedent).
`ghostty_app_open_config`'s `performAction(.app, .open_config, {})` → the `apprt_api::App`
trait `perform_action(Target::<CoreSurface>::App, Action::OpenConfig)`. The
`app_set_color_scheme` `intToEnum` guard lives on the raw `c_int` (0=light/1=dark); the
reference warn string names `ghostty_surface_set_color_scheme` (a ghostty copy-paste
quirk inside the app fn) — preserved verbatim in the comment.

**Gate deltas (E6):** new deps on `rustty-apprt-c`: `rustty-core`, `rustty-config`,
`rustty-apprt-api`, `allocator-api2` (all far below apprt-c — it stays a clean TOP
SINK). `apprt_embedded` MAP row unchanged (embedded.zig 0-test; the exports land under
the existing `{RSAPPRTC}/**/*.rs` glob — GREEN, rust ≥ zig; NO new MAP entry).
Whole-workspace gates green (build incl. apprt-c + rustty + cli; touched-crate tests —
config 187, apprt 0, apprt-c 0; clippy `-D warnings`, fmt, acyclic G7 with apprt-c
still a clean TOP SINK, test-counts G-gate); scoped-G3b on embedded.zig **1398-1537
(E6) distinctive=18 missing=0 review=0**. No regression (vt 2019, core 15, apprt-api
20, renderer 19, crash 12, termio 44).

## rustty-apprt-c embedded E7 (CAPI surface lifecycle + state/query + render exports)

Chunk E7 of the libghostty C-ABI milestone (EMBEDDED_BLUEPRINT §1/§3/§7): ports
`embedded.zig:1538-1757` — the surface lifecycle (7), state/query (8), and
render/window (7) `export fn ghostty_surface_*` — into
`crates/rustty-apprt-c/src/lib.rs` as `#[no_mangle] pub unsafe extern "C" fn
rustty_surface_*`: `_config_new`, `_new` (+ the `surface_new_` helper), `_free`,
`_userdata`, `_app`, `_inherited_config`, `_update_config`, `_needs_confirm_quit`,
`_process_exited`, `_has_selection`, `_read_selection`, `_read_text` (+ the
`readTextLocked` helper), `_free_text`, `_refresh`, `_draw`, `_set_size`, `_size`,
`_foreground_pid`, `_tty_name`, `_set_color_scheme`, `_set_content_scale`,
`_set_focus`, `_set_occlusion`. Bodies forward to the already-ported `rustty-apprt`
embedded backend (E1/E2/E3) + `rustty-core` Surface. `ghostty_surface_new`
heap-boxes + pins the backend `Surface` via the backend `App::newSurface`
(ADV-FIX 8 — done in E1/E2, reached here).

**Runtime-live split (EMBEDDED_BLUEPRINT §1):** only `rustty_surface_config_new` (a
pure defaulted `Surface.Options`) is runtime-live. **Every other E7 export needs a
LIVE surface**, which only exists past `rustty_surface_new` → core `Surface::init` →
`renderer_seam::{Renderer,Thread}` (surface.rs:324/333 — the Phase-3 R-d GPU + D6 xev
leaves, already in the core C3 deferral set above). So the whole E7 surface set
**COMPILES + LINKS + header-generates NOW and PANICS when driven against a real
surface**. E7 adds **NO new `unimplemented!()` leaf in apprt-c** — it is a pure FFI
projection that REACHES the existing renderer/xev leaves at runtime:

1. **`rustty_surface_new`** → `App::newSurface` → `Surface::init` → core
   `Surface::init` constructs `renderer_seam::{Renderer,Thread}` (Phase-3 R-d/D6
   leaf). LINKS now; a live-surface call panics in the core init leaf.
2. **`_refresh`/`_draw`/`_set_size`/`_set_content_scale`/`_set_focus`/`_set_occlusion`/
   `_set_color_scheme`** → the backend `Surface::{refresh,draw,updateSize,
   updateContentScale,focusCallback,occlusionCallback,colorSchemeCallback}` (E3) →
   core surface callbacks that reach the renderer leaf at runtime (`draw` most
   directly). LINK now; panic only when driven against a live surface.
3. **`_read_selection`/`_read_text`/`_has_selection`/`_process_exited`/
   `_needs_confirm_quit`/`_foreground_pid`/`_tty_name`/`_size`/`_update_config`/
   `_inherited_config`** → live core-surface state (mutex/terminal/io/size/pty). Real
   core logic, NO GPU, but unreachable without a live surface (so effectively Phase-3
   until R-d+D6). No NEW leaf.

**Two supporting COMPLETIONS (not deferrals), both faithful ports of `main_c.zig`:**

1. **`main_c.zig` `String` (`ghostty_string_s`) + `rustty_string_free`.** embedded.zig
   imports `String` (`const String = @import("../main_c.zig").String;`,
   embedded.zig:23) and `ghostty_surface_tty_name` returns it. This crate root IS the
   `main_c.zig` analog, so its items are folded in as the exports that need them land
   (the **E0 `rustty_init` precedent**). Ported the full struct (`ptr`/`len`/`sentinel`)
   + `empty`/`from_slice`/`deinit` + the `rustty_string_free` freeing export
   (main_c.zig:155). `from_slice` fuses the reference `dupeZ` copy + `fromSlice` wrap +
   the sentinel append (the config c_api `from_path` precedent); `deinit` reconstructs
   the leaked `Box<[u8]>` via Rust's global allocator (the E5 `Text::deinit` seam).
   rustty-config ships its own *local deferred* projection of this same struct (no
   `deinit`, `c_api.rs`); this is the canonical `main_c` port with the freeing pair.
   NOT a deferral — fully implemented + host-freeable.
2. **read-text NUL discipline.** core `dumpTextLocked` returns `Text { text: Vec<u8> }`
   whose length EXCLUDES the sentinel (the vt `selection_string` convention) and whose
   buffer carries NO trailing NUL; `readTextLocked` pushes the NUL and leaks a
   `Box<[u8]>` of exactly `text_len + 1` bytes so the E5 `Text::deinit`
   (`ghostty_surface_free_text`) reconstruction frees the same allocation. Faithful to
   the reference `[:0]const u8` allocation shape (documented at both E5 and E7 sites).

**Identity / ABI notes (R3):** `ghostty_surface_config_new`'s `.{}` → the
`Surface.Options` Zig field defaults spelled out (the E3 `newSurfaceOptions`
precedent; `platform` = reference `undefined` → a zeroed union placeholder).
`surface_new_`'s `opts.*` → `ptr::read` (`SurfaceOptions` is `#[repr(C)]`).
`ghostty_surface_free`'s `ptr.app.closeSurface(ptr)` captures the `app` raw pointer
before handing `ptr` to `closeSurface` (raw-pointer args). `read_selection`/`read_text`
realize the reference `defer mutex.unlock()` as a single unlock before the one return
(both the no-selection and read arms flow through it). `getProcessInfo(.foreground_pid)`/
`(.tty_name)` → the core comptime-return split (`getForegroundPid`/`getTtyName`).
`readTextLocked` keeps the reference camelCase identifier (`#[allow(non_snake_case)]`).
The `surface_set_color_scheme` `intToEnum` guard lives on the raw `c_int` (0=light/
1=dark), the E6 precedent.

**Gate deltas (E7):** new dep on `rustty-apprt-c`: `rustty-vt` (via `capi`, already
present; E7 adds `rustty-core::surface::TextViewport` + `rustty-vt::selection` uses —
all far below apprt-c, still a clean TOP SINK). `apprt_embedded` MAP row unchanged
(embedded.zig 0-test; the exports land under the existing `{RSAPPRTC}/**/*.rs` glob —
GREEN, rust ≥ zig; NO new MAP entry). Whole-workspace gates green (build incl. apprt-c
+ rustty + cli; touched-crate tests — apprt-c 0; clippy `-D warnings`, fmt, acyclic G7
with apprt-c still a clean TOP SINK, test-counts G-gate); scoped-G3b on embedded.zig
**1538-1757 (E7) distinctive=27 missing=0 review=0**. No regression (vt 2019, core 15,
apprt-api 20, renderer 19, crash 12, termio 44).

## rustty-apprt-c embedded E8 (CAPI input exports) — live-surface leaf, NO new leaf

Chunk E8 of the libghostty C-ABI milestone (EMBEDDED_BLUEPRINT §1/§3/§7): ports
embedded.zig **1758-1920** — the 11 raw input exports — into
`crates/rustty-apprt-c/src/lib.rs` as `#[no_mangle] pub unsafe extern "C" fn
rustty_surface_*`: `_key_translation_mods`, `_key`, `_key_is_binding`, `_text`,
`_preedit`, `_mouse_captured`, `_mouse_button`, `_mouse_pos`, `_mouse_scroll`,
`_mouse_pressure`, `_ime_point`. Each marshals the wire form (the `c_int` mods
reinterpreted as `input.Mods`; the `input.Mouse{ButtonState,Button,PressureStage}` +
`input.ScrollMods` value types) and forwards to the ported `rustty-apprt` backend
`Surface` callback methods (E3: `keyEvent`/`textCallback`/`preeditCallback`/
`mouseButtonCallback`/`cursorPosCallback`/`scrollCallback`/`mousePressureCallback`) or
straight to the core surface (`mouseCaptured`/`keyEventIsBinding`/`imePoint`).

**Runtime split (EMBEDDED_BLUEPRINT §1):** every E8 export needs a **LIVE surface**
(created via `rustty_surface_new`, the Phase-3 GPU-leaf boundary — core `Surface::init`
constructs `renderer_seam::{Renderer,Thread}`, the R-d/D6 leaves already in the core C3
deferral set above). The wrappers LINK now; a live-surface call reaches the renderer
leaf only when driven (key/mouse callbacks schedule a render). E8 adds **NO new
`unimplemented!()` leaf in apprt-c** — it is a pure FFI projection (the E6/E7 precedent),
so there is nothing to un-defer here beyond the existing renderer/xev leaves.

**One supporting COMPLETION (not a deferral) — faithful `@bitCast` helper in vt:**
`ghostty_surface_mouse_scroll` reinterprets the wire `scroll_mods` bits as
`input.ScrollMods` (reference `@bitCast(@as(u8, @truncate(@as(c_uint, @bitCast(...)))))`).
The port models `ScrollMods` as an unpacked struct with an `int()` backing serializer;
E8 adds its inverse **`ScrollMods::from_int(u8)`** to `rustty-vt`
(`crates/rustty-vt/src/input/mouse.rs`), mirroring the established `key_mods::Mods::from_int`
/ `Keys::from_int` / `Side::from_int` idiom (the port form of `@bitCast(backing_int) ->
packed struct`) — so the apprt-c export calls `ScrollMods::from_int(...)` exactly as
`capi.rs` calls `Mods::from_int(...)` (E5). The `momentum` `u3` sub-field only names
values 0..=6; the one unrepresentable pattern (`7`) — which `int()` / a real platform
scroll event never produces — traps via `unreachable!()`, the Rust-safe analog of the
reference `@bitCast`-to-exhaustive-`enum(u3)` illegal behavior. Additive pure fn; the
vt 2019-test suite is unchanged (no `#[test]` added, no MAP delta). NOT a deferral.

**Identity / ABI notes (R3):** the `c_int` mods reinterpret is the E5 `KeyEvent::keyEvent`
precedent (`Mods::from_int(mods as c_uint as ModsBacking)`), shared by
`_key_translation_mods`/`_mouse_button`/`_mouse_pos`. `_key_translation_mods`'s
`Mods.translation` is the `rustty-input` free fn `key_mods::translation` (it needs the
apprt-layer `OptionAsAlt`, so it cannot live on vt's `Mods`); the `orelse` fallback to
`keyboardLayout().detectOptionAsAlt()` is lazy (`unwrap_or_else`). `_key`'s target is
`KeyTarget::Surface(surface)` (vs E6 `_app`'s `KeyTarget::App`). `_key_is_binding`'s
`?*input.Binding.Flags.C` out-param is a raw `*mut FlagsC` written under a null-check
(the `if (c_flags) |ptr|` capture); `Flags.cval()` is the `rustty-input` serializer.
`_mouse_pressure`'s `intToEnum(MousePressureStage, ...)` guard is inlined on the raw
`u32` (the E7 `set_color_scheme` precedent). `_ime_point` writes the four `apprt.IMEPos`
fields through the caller's `*mut f64` out-params. `_text`/`_preedit` build `&[u8]`
(preedit: `Option<&[u8]>`, `None` when `len == 0`) from the `[*]const u8`+`len` pair.

**Gate deltas (E8):** new dep on `rustty-apprt-c`: **`rustty-input`** (via `key_mods::translation`
+ `binding::FlagsC` — far below apprt-c, still a clean TOP SINK). E8 also touches
`rustty-vt` (the additive `ScrollMods::from_int`). `apprt_embedded` MAP row unchanged
(embedded.zig 0-test; the exports land under the existing `{RSAPPRTC}/**/*.rs` glob —
GREEN, rust ≥ zig; NO new MAP entry). Whole-workspace gates green (build incl. apprt-c
+ rustty + cli; touched-crate tests — apprt-c 0, vt 2019; clippy `-D warnings`, fmt,
acyclic G7 with apprt-c still a clean TOP SINK, test-counts G-gate); scoped-G3b on
embedded.zig **1758-1920 (E8) distinctive=18 missing=0 review=0**. No regression
(vt 2019, core 15, apprt-api 20, renderer 19, crash 12, termio 44).

## rustty-apprt-c embedded E9 (CAPI surface-action + clipboard + inspector exports) — live-surface / live-inspector leaf, NO new leaf

Chunk E9 of the libghostty C-ABI milestone (EMBEDDED_BLUEPRINT §1/§3/§7): ports
embedded.zig **1924-2100** (the E9 range 1924-2106) — the 6 surface-action exports,
the 1 clipboard-completion export, and the 10 inspector exports — into
`crates/rustty-apprt-c/src/lib.rs` as `#[no_mangle] pub unsafe extern "C" fn
rustty_*`: `_surface_request_close`, `_surface_split`, `_surface_split_focus`,
`_surface_split_resize`, `_surface_split_equalize`, `_surface_binding_action`,
`_surface_complete_clipboard_request`, `_surface_inspector`, `_inspector_free`,
`_inspector_set_size`, `_inspector_set_content_scale`, `_inspector_mouse_button`,
`_inspector_mouse_pos`, `_inspector_mouse_scroll`, `_inspector_key`, `_inspector_text`,
`_inspector_set_focus`. The surface-action exports forward to the backend `App`'s
`perform_action` (the apprt-api trait, E6 `open_config` precedent — here with a
`Target::Surface`) or straight to the core surface (`_request_close` →
`core_surface.close()`, `_binding_action` → `core_surface.performBindingAction`); the
clipboard sink forwards to the backend `Surface::completeClipboardRequest` (E3); the
inspector sinks forward to the E4 `Inspector` methods.

**Runtime split (EMBEDDED_BLUEPRINT §1):** every E9 export needs a **LIVE surface**
(created via `rustty_surface_new`, the Phase-3 GPU-leaf boundary — the R-d/D6 leaves in
the core C3 deferral set). The surface-action + clipboard exports are pure FFI
marshaling once a surface exists (the split exports reach the host `action` callback;
`_binding_action` runs real core logic; `_complete_clipboard_request` runs the real E3
backend, reaching the host `confirm_read_clipboard` callback on the unsafe/unauthorized-
paste arm). The **inspector per-callback exports additionally reach the E4 `Inspector`
`unimplemented!()` leaves** (dcimgui `ImGuiIO_Add*Event`, `Inspector::init`): a LIVE
inspector cannot yet exist because `_surface_inspector` → `Surface::initInspector` →
`Inspector::init` is a Phase-3 dcimgui leaf, so `_inspector_{mouse_button,mouse_pos,
mouse_scroll,key,text,set_focus}` are unreachable this milestone (their leaves are E4's,
already documented above). E9 adds **NO new `unimplemented!()` leaf in apprt-c** — it is
a pure FFI projection (the E6/E7/E8 precedent), so there is nothing to un-defer here
beyond the existing renderer/xev (R-d/D6) and E4 dcimgui-inspector leaves.

**Identity / ABI notes (R3):** the split-action value types cross the ABI BY VALUE
(`apprt.action.{SplitDirection,GotoSplit}` are `#[repr(i32)]`; `ResizeSplit` is
`#[repr(C)]`); `apprt.action.ResizeSplit.Direction` is the hoisted sibling
`ResizeSplitDirection` (Rust cannot nest a type in a struct's value namespace), and
`ResizeSplit` is reassembled by field name from the `{direction, amount}` params. The
reference `ptr.app.performAction(.{ .surface = ... }, .new_split, v)` maps to
`(*app).perform_action(Target::Surface(&core_surface), Action::NewSplit(v))` — the
E6 precedent, now with a surface-carrying `Target` (`C` inferred as `CoreSurface`).
`_split_focus`'s reference log message **"error creating new split"** is a verbatim
copy-paste of `_split`'s (preserved faithfully). `_binding_action`: `binding::Action`
is **module-qualified** (`binding::Action::parse`) to avoid clashing with the apprt-api
`Action`; Zig passes the parsed `action` union to `performBindingAction` **by value** (a
copy) so the local stays live for the `catch`'s `action={f}` log — Rust's by-value
`performBindingAction` consumes its arg, so **`action.clone()`** reproduces that copy and
keeps `action` live for the error branch (rendered via `Debug`, the analog of the
reference `{f}` `Action.format`); the crate-local C-API `String` (main_c.zig heap-string)
shadows `std::string::String`, so the parse-error log fully-qualifies
`std::string::String::from_utf8_lossy`. `_complete_clipboard_request`'s
`std.mem.sliceTo(str, 0)` → a borrowed `&CStr` (`CStr::from_ptr`; the NUL sentinel is
load-bearing — the backend re-hands `.as_ptr()` to the host callback), with `state` a raw
`*mut ClipboardRequest` (the `Box` from `Surface::clipboard_request`, E3). The reference
**`ghostty_inspector_free` takes a `*Surface`** (NOT a `*Inspector`) — it frees the
surface-owned inspector via `freeInspector`. `_surface_inspector`'s `?*Inspector` → a
nullable `*mut Inspector` (`null` on the `initInspector` error). The inspector mods
reinterprets (`_inspector_mouse_button`/`_inspector_key` `c_int` → `Mods::from_int`,
`_inspector_mouse_scroll` `c_int` → `ScrollMods::from_int`) are the E8 precedent;
`_inspector_key`'s `action: input.Action`/`key: input.Key` cross by value as
`#[repr(C)]` `key::{Action,Key}` enums.

**Gate deltas (E9):** NO new `rustty-apprt-c` Cargo dep — E9's imports come from deps
already present: the split value types + `ClipboardRequest` from `rustty-apprt-api`, the
`key` module (`key::{Action,Key}`) from `rustty-vt`, and the `binding` module
(`binding::Action`) from `rustty-input` (added at E8). `apprt_embedded` MAP row unchanged
(embedded.zig 0-test; the exports land under the existing `{RSAPPRTC}/**/*.rs` glob —
GREEN, rust ≥ zig; NO new MAP entry). Whole-workspace gates green (build incl. apprt-c
+ rustty + cli; touched-crate tests — apprt-c 0; clippy `-D warnings`, fmt, acyclic G7
with apprt-c still a clean TOP SINK, test-counts G-gate); scoped-G3b on embedded.zig
**1924-2106 (E9) distinctive=12 (E9-owned) missing=0** — the 3 remaining distinctive
lines in the range are the `ghostty_set_window_background_blur` doc-comment (2102-2106;
its `export fn` is at 2108, the E10 macOS objc/CGS leaf) and defer with E10. No
regression (vt 2019, core 15, apprt-api 20, renderer 19, crash 12, termio 44).

## rustty-apprt-c embedded E10 (misc + Darwin-gated exports) — macOS objc/CGS/Metal/coretext Phase-3 leaves; 71 == 71

Chunk E10 of the libghostty C-ABI milestone (EMBEDDED_BLUEPRINT §1/§3/§7, row E10):
ports embedded.zig **2108-2250** — the misc export (1) + the Darwin-gated block (6) —
into `crates/rustty-apprt-c/src/lib.rs` as `#[no_mangle] pub unsafe extern "C" fn
rustty_*`: `_set_window_background_blur`, `_surface_set_display_id`,
`_surface_quicklook_font`, `_surface_quicklook_word`, `_inspector_metal_init`,
`_inspector_metal_render`, `_inspector_metal_shutdown`. **With E10 ALL 71 exports
exist** — a mechanical name-set diff vs the reference's 71 `export fn ghostty_*` is
IDENTICAL (71 == 71; the two `main_c.zig` precursors `rustty_init`/`rustty_string_free`
are excluded from the tally, matching that they are not embedded.zig `export fn`s).

**E10 is the leaf-CREATING chunk (unlike the pure-marshaling E6-E9).** The E0 deferral
recorded that the "macOS objc/Metal/CGS ... `unimplemented!()` leaves are CREATED by
E3/E4/E10". E10 lands the macOS objc/CGS/Metal/coretext seams as documented Phase-3
leaves — every reference body preserved VERBATIM as comments (R1 no-loss); NO
objc/CGS/Metal/coretext logic fabricated. Runtime split + targets:

1. **`rustty_set_window_background_blur` — macOS objc/CGS leaf; non-macOS no-op
   [ADV-FIX 3].** The export EXISTS on ALL platforms (the reference `@export`s it
   unconditionally; only its BODY is comptime-gated). The reference `if (comptime
   target != .macos) return;` splits by `#[cfg]`: `#[cfg(not(target_os = "macos"))]`
   is the no-op early return; `#[cfg(target_os = "macos")]` is an `unimplemented!()`
   leaf (objc `msgSend(windowNumber)` + private CGS `CGSSetWindowBackgroundBlurRadius`).
   The two private CoreGraphics `extern "c"` decls (`CGSSetWindowBackgroundBlurRadius`/
   `CGSDefaultConnectionForThread`, embedded.zig:2128-2130) are preserved as comments
   (they land as live `extern "C"` fns WITH the leaf in Phase-3). NOT in the runtime-safe
   list; the E11 smoke KAT MUST NOT call it. **Target: Phase-3 (macOS objc/CGS glue).**
2. **`rustty_surface_set_display_id` — renderer/xev leaf.** Pushes a `.macos_display_id`
   message to `renderer_thread.mailbox` + `wakeup.notify()`; both the renderer thread
   and its xev `wakeup` are placeholders. `unimplemented!()` leaf. **Target: Phase-3 R-d
   (renderer) + D6 (xev).**
3. **`rustty_surface_quicklook_font` — objc/coretext + renderer leaf.** Reaches the
   renderer `font_grid` primary CoreText face + `copyAttribute`/`copyWithAttributes`
   (coretext). `unimplemented!()` leaf. **Target: Phase-3 R-d (renderer) + coretext.**
4. **`rustty_surface_quicklook_word` — renderer_state live-surface leaf.** Real terminal
   logic (`getCursorPos`/`posToViewport`/`pages.pin`/`selectWord` + the E7 `readTextLocked`
   helper) under `renderer_state.mutex` — grouped with the Darwin block per the blueprint,
   ported as an `unimplemented!()` leaf (reaches the live-surface renderer_state seam).
   **Target: Phase-3 R-d (renderer_state).**
5. **`rustty_inspector_metal_init` / `_metal_render` / `_metal_shutdown` — delegate to
   the E4 backend leaves (NO new apprt-c leaf).** The reference bodies are pure
   delegations, so the wrappers forward to the already-landed E4 backend methods
   `Inspector::{initMetal, renderMetal}` + `Backend::deinit` (DEFERRALS.md E4 item 2,
   which named these three as "Targets of the E10 Darwin CAPI"). `_metal_render` catches
   + logs the error exactly as the reference `catch |err| { log.err(...); return; }`;
   `_metal_shutdown` is a faithful `if let Some(v) = ptr.backend { v.deinit(); ptr.backend
   = None }` (a no-op this milestone, since `initMetal` is a leaf so `backend == None`).
   The panic is in the E4 backend leaf. **Target: Phase-3 R-d (objc/Metal + dcimgui).**

**Darwin gating (§3/§7 + E5 `capi.rs` decision):** the six Darwin exports are
`#[cfg(target_os = "macos")]`-gated free functions (the compiler pulls them into the C
API when targeting Darwin — no runtime analogue for the reference `comptime { if
isDarwin() _ = Darwin; }` force-reference, already ported as a `capi.rs` comment note in
E5). NOTE the reference gate is `isDarwin()` (macOS OR iOS); rustty uses `target_os =
"macos"` (the objc/Metal/coretext seams these reach are macOS-only in practice). The
71 == 71 completeness therefore holds on a Darwin (macOS) build, matching the reference
(which likewise emits all 71 only on Darwin).

**Gate deltas (E10):** NO new `rustty-apprt-c` Cargo dep — E10's `App`/`Surface`/
`Inspector`/`Text` are all already imported (E6/E7/E9). `apprt_embedded` MAP row
unchanged (embedded.zig 0-test; the exports land under the existing `{RSAPPRTC}/**/*.rs`
glob — GREEN, rust ≥ zig; NO new MAP entry). Whole-workspace gates green (build incl.
apprt-c; touched-crate tests — apprt-c 0; clippy `-D warnings`, fmt, acyclic G7 with
apprt-c still a clean TOP SINK, test-counts G-gate); scoped-G3b on embedded.zig
**2108-2250 (E10) missing=0** (whole-file embedded.zig G3b GREEN). Mechanical
export-count diff **71 == 71** (identical name sets). No regression (vt 2019, core 15,
apprt-api 20, renderer 19, crash 12, termio 44). This CLOSES the E9-deferred
`ghostty_set_window_background_blur` doc-comment (2102-2106) — ported with its export.

## rustty-apprt embedded E11 (C-ABI smoke KAT + finale) — header-gen + cc compile+link
Chunk E11 of the libghostty C-ABI milestone (EMBEDDED_BLUEPRINT §3/§6/§7 [ADV-FIX 2]):
the verification finale. It creates **NO new production leaf** — it wires header
generation + the C compile+link+run KAT and audits the completed 71-export surface.

**What landed (no reference logic; tooling + tests + pins):**
1. **cbindgen header generation.** `xtask` gains a `header-gen-apprt` subcommand
   emitting `include/rustty-apprt.h` from `rustty-apprt-c` (mirrors the vt-c
   `header-gen`). `cbindgen.toml` `after_includes` is filled (was empty through E10):
   the cross-crate C types are hand-pinned (`parse_deps=false`, vt-c precedent) — the
   opaque handles (`App`/`Surface`/`Inspector`/`Config`/`ClipboardRequest`/`FlagsC`),
   the input `Action`(3)/`Key`(176)/mouse `ButtonState`/`Button` + apprt-api
   `SplitDirection`/`GotoSplit`/`ResizeSplitDirection`/`NewSurfaceContext` enums, the
   `ClipboardRequestType` (`uint8_t`, `#[repr(u8)]`), `TargetC`(16)/`ActionC`(32) (the
   `ActionCValue` union pinned size/align-faithful — apprt-api's byte-exact C-ABI
   projection is its OWN documented deferral), `ClipboardContent`/`EnvVar`/`Platform.C`,
   and `Options`(64)/`SurfaceOptions`(88). The header compiles standalone as **C AND
   C++** (`clang -std=c11` / `clang++ -std=c++17 -Wall -Wextra`, exit 0).
2. **The smoke KAT** (`tests/smoke.c` + `tests/smoke.rs`). See EMBEDDED_BLUEPRINT §6.3.
   **DEVIATION-WITH-RATIONALE:** the KAT links the **cdylib** (`librustty_apprt_c.dylib`),
   not the staticlib. `rustty-apprt-c`'s dep closure is the whole app (core → renderer/
   font/… → vendored `-sys` C libs), and rustc's `staticlib` does NOT bundle those
   `cc`-built archives into the `.a` (verified: `_deflate`/`_OnigEncodingKOI8`/
   `_bindtextdomain` have 0 defs in it), so standalone static linking would need every
   `-sys` crate's hashed `OUT_DIR` archive — fragile + non-portable. The cdylib is
   self-contained (all native deps resolved in) and is the standard C-host consumption
   path; linking+running it equally proves "the published `librustty` + header, callable
   by a host, exit 0" (EMBEDDED_BLUEPRINT §1 rec 1). The staticlib artifact is still
   asserted to EXIST + export the C-ABI symbols (a lighter static check), so both
   published artifacts are covered.
3. **`main_c.zig` config-header note.** rustty-config ships the `rustty_config_*` symbols
   but not yet its own cbindgen header (a separate config-milestone deliverable), so
   `smoke.c` forward-declares the 3 config functions it uses (against the opaque `Config`
   from rustty-apprt.h). **Target: config C-ABI header (own milestone).**
4. **Cross-crate ABI-pin const-asserts** (`capi.rs`, the E11 audit): `Options`==64,
   `SurfaceOptions`==88, `EnvVar`==16, `PlatformC`==8, `TargetC`==16, `ActionC`==32 +
   the enum-width pins — the drift gate that keeps the hand-pinned header honest.

**Leaf audit (E11 item 5) — apprt-c has exactly 4 `unimplemented!()` code-leaves, ALL
E10-created, ALL documented above:** `rustty_set_window_background_blur` (macOS
objc/CGS), `rustty_surface_set_display_id` (renderer/xev), `rustty_surface_quicklook_font`
(objc/coretext + renderer), `rustty_surface_quicklook_word` (renderer_state). The
inspector `_metal_*` exports delegate to the E4 backend leaves (no apprt-c leaf). E11
adds none.

**Gate deltas (E11):** NO new `rustty-apprt-c` Cargo dep (uses apprt-api types already
in the closure). `apprt_embedded`/`apprt_none`/`apprt_browser` MAP rows unchanged +
GREEN (all 0-test; the new `tests/smoke.rs` `#[test]` lives outside the mapped
`{RSAPPRTC}/src/**` glob, so it is uncounted — rust ≥ zig holds). Whole-workspace build
clean; apprt-c tests GREEN (smoke KAT compile+link+run exit 0); clippy `-D warnings` +
fmt (apprt-c + xtask); acyclic G7 with apprt-c a clean TOP SINK; full-file scoped-G3b
GREEN (embedded.zig 235 / none.zig 1 / browser.zig 0 distinctive, missing=0); no
regression (apprt-api 20, core 15, crash 12, renderer 19, termio 44). **This CLOSES the
apprt/embedded milestone: C-ABI 71/71 compiles+links+header+smoke; runtime-live subset
≈15/71; GPU/xev/objc/coretext deferred to Phase-3 (R-d + D6).**

## rustty-cli C1 (Phase-2 CLI milestone) — dispatcher; version(partial) + help_strings + C2–C5 stubs

C1 (CLI_BLUEPRINT.md §3+§7) lands the action dispatcher into `rustty-cli`:
`action.zig` (7t → `action.rs`), `ghostty.zig` Action enum + `detectSpecialCase`
+ `run`/`runMain` (4t → `ghostty.rs`), `cli.zig` re-export (`lib.rs`), `help.zig`
(`help.rs`), `version.zig` (PARTIAL, `version.rs`). All gate-green: cli 12
`#[test]` (7+4+1), test-count GREEN (cli_action 7/7, cli_ghostty 4/4, cli_root
0/1, cli_help 0/0, cli_version 0/0), scoped-G3b missing=0 on all 5 ported files,
acyclic G7 GREEN (cli deps only strictly-earlier members — no `apprt→cli` edge;
detection lives in the bin, ADV-FIX 2). New cli Cargo deps: `rustty-build-config`
+ `rustty-identity` (ADV-FIX 4). Deferrals introduced/continued:

- **`version.zig` is PARTIAL (ADV-FIX 1) — `renderer.Renderer` + `xev.backend`
  STUBBED. ✅ RESOLVED (2026-07-23, WS3-B) for the renderer/libxev/toolchain
  lines — real values via build_config::renderer() type-name map,
  rustty_xev::backend(), and build_config::RUSTC_VERSION; the cli_smoke
  `version_plus` KAT now ASSERTS their content. Still open: the OSC-8
  commit-hash hyperlink (no `+BUILD` metadata / repo-URL identity constant).**
  `version.rs` emits documented STUB strings for the `- renderer` line
  (`RENDERER_STUB`; rustty-renderer's `Renderer` is a Phase-3 leaf, commented out)
  and the `- libxev` line (`LIBXEV_STUB`; rustty-xev unimplemented, D6). The
  `builtin.zig_version_string` line has no Rust analog (no Zig toolchain / no
  build script capturing rustc) → `TOOLCHAIN_VERSION_STUB`. The OSC-8 commit
  hyperlink reads `build_config.version.build`, which the Rust runtime
  `SemanticVersion` view drops (no `+BUILD`), so `commit_hash()` is always `None`
  and the branch never emits; the reference commit-URL host
  (`github.com/ghostty-org/ghostty/commit/…`) is an R2 product-identity value not
  yet in `rustty-identity`. 0-test file → no contract breakage; the `+version`
  KAT (C6) must NOT assert the renderer/libxev/toolchain line CONTENT. The GTK
  block (`app_runtime == .gtk`) is comptime-false for the headless `.none` build,
  preserved verbatim as commented reference (R1). **Target:** un-defer the
  renderer/libxev line content when R-d (renderer) / D6 (libxev) land; add a
  rustty repo-URL constant to `rustty-identity` for the hyperlink.

- **`help_strings` / `Action::help_text` — the `+action --help` text. ✅ RESOLVED
  (help_strings doc-table generator landed).**
  `ghostty::Action::run`'s `help_error` path (a faithful direct-match port of the
  reference `inline for` over `Action` fields) prints `self.help_text()`, which
  indexes the build-generated `help_strings.Action.<field>` doc table. This is now
  the ported `crate::help_strings::action_help` — a `build.rs` codegen (over the
  shared `rustty-helpgen` leaf, R1 port of `src/helpgen.zig` + `HelpStrings.zig`)
  that extracts each action module's `pub fn run` `///` doc block. `help_text`
  returns the real per-action doc text. The SAME milestone landed the config
  `help_strings.Config` (`config_help`) and input `help_strings.KeybindAction`
  (`keybind_help`) tables (replacing `formatter_file.rs`'s deferred `debug_assert`
  and input's `NoHelpStrings` seam with `RealHelpStrings`), un-deferred
  `explain_config`'s 1 test, and enabled `+help` / `show_config --docs` /
  `list-actions --docs` (all previously 0-test).

- **C5 action-impl sequencing stubs (0 tests, no MAP entry yet).** The 6
  not-yet-ported action impls (`list_fonts`, `list_keybinds`, `list_themes`,
  `list_colors`, `show_face`, `boo`) are faithful sequencing stubs — each
  `pub fn run() -> Result<u8, ghostty::Error>` with the reference signature +
  `unimplemented!()` body — so `runMain` compiles. Their full logic, comment
  coverage (scoped-G3b), and `check_test_counts.py` MAP entries land with chunk
  C5 (per CLI_BLUEPRINT.md §7 — the vaxis/zf-blocked plain/enumeration paths +
  the deferred TUI render leaves). Until then, the WHOLE-CRATE
  `check_comment_coverage.py --crate rustty-cli` reports their reference comments
  as "missing" — this is EXPECTED per the DEFERRALS "Rule"; the C1–C4
  scoped-G3b (ported files) is missing=0. (C2 landed show_config /
  validate_config / list_actions / explain_config + Pager; C3 landed the ssh
  subsystem — ssh / ssh_cache / DiskCache / Entry; C4 landed crash_report /
  edit_config + the none-backed IPC actions new_window / toggle_quick_terminal +
  the tui.zig `can_pretty_print` seam — so all are no longer stubs.)

- **vaxis/zf TUI leaves (C5, forward-scheduled — not in C1).** `boo` (pure vaxis
  animation), `list_themes`' interactive picker (vaxis+zf), and the
  `prettyPrint` branches of `list_colors`/`list_keybinds` need the unported Zig
  `vaxis`/`zf` deps (same class as libxev D6 / z2d). Their plain/enumeration
  paths land in C5; the interactive render paths are deferred placeholder leaves
  (GPU/GTK/xev precedent). All 0-test → no contract impact.
  - **`list_colors`/`list_keybinds` `prettyPrint`. ✅ RESOLVED (WIRE1).** Now
    that `rustty-vaxis` (the full vaxis core: Vaxis/Window/Tty/ctlseqs/Style/
    Color/prettyPrint) is landed, both interactive render paths are filled from
    their preserved verbatim Zig bodies (`crates/rustty-cli/src/{list_colors,
    list_keybinds}.rs`) — including `list_keybinds`' `TriggerNode`/`ChordBinding`/
    `iterateBindings` helpers (the intrusive `std.SinglyLinkedList` ported as an
    owned `Vec<Trigger>`; `lessThan` → `ChordBinding::cmp` for the stable
    `sort_by`). One new cli dep: `rustty-vaxis` (leaf, acyclic G7). One cli-local
    `Error::Tty(TtyError)` variant + `From`. The render is factored into a
    `render_{colors,keybinds}<W: Write>` seam (pure over a `Vec<u8>` sink →
    smoke-tested + Miri-clean) behind the `pretty_print` device wrapper (opens a
    real `/dev/tty` → `#[ignore]`-on-device). The reference `alloc` arena is
    ported as the `pretty_arena::Arena` (a typed-arena-style byte arena; the
    vaxis `Window` stores each `Segment`'s `text: &'a [u8]` BY REFERENCE, so the
    `allocPrint`'d hex/key/action bytes must outlive the screen). cli now
    2 (`list_colors`) + 4 (`list_keybinds`) new `#[test]`s (device smokes
    `#[ignore]`d); test-count/scoped-G3b/acyclic/clippy-D/fmt/Miri all GREEN.
    `boo` + `list_themes`' picker remain deferred (`zf` still unported for the
    latter).
  - **`list_themes`' interactive fuzzy-finder preview picker. ✅ RESOLVED
    (WIRE2, cli C5 finale).** Now that `rustty-vaxis` (full core incl. `Loop` +
    `widgets::TextInput`) AND `rustty-zf` (`zf::rank`) are landed, the picker is
    filled LIVE in `crates/rustty-cli/src/list_themes.rs` from the preserved
    verbatim Zig bodies: the `Event` + `LoopEvent` impl, `Preview` (→ run-scope
    `tty`/`vx` locals + a `DrawState`), `init`/`run`/`deinit`, `update` (every
    key/mouse arm), `up`/`down`, `updateFiltered` (factored into the PURE
    `rank_filtered` zf-ranking kernel + `reselect`), `draw`/`drawPreview` (the
    theme list, the swatch/ziggzagg/lorem-ipsum preview panes), the `ui_*` color
    helpers, `color`, `toUri`, `resolveAutoThemePath`/`writeAutoThemeFile`/
    `saveSelectedTheme`. One new cli dep: **`rustty-zf`** (leaf, acyclic G7).
    `lorem_ipsum.txt` (`@embedFile` → `include_bytes!`) copied into the tree (R3).
    Key correctness pins: ONE accumulating `Config` for the color gate (issue 2);
    all-spaces query routes through `zf::rank(&[])` via `has_query`/real_length
    (issue 5); empty-filter `wrapping_sub`/`!filtered.is_empty()` guards (issue 4);
    `{err:?}` err-pane text (no `Display` on `LoadError`, issue 3, cosmetic).
    **Aliasing (the substantive finding):** the `vaxis.Loop` holds raw `*mut
    Tty`/`*mut Vaxis`; the naive `Loop::new(&mut tty, &mut vx)` + body
    `tty.writer()` shape is **Stacked-Borrows UB** (the local `&mut` reborrow pops
    the read thread's live raw tag) — caught + fixed via the `&raw mut` + transient-
    reborrow discipline, PROVEN by the fresh `.port/phase-2/mirihar-wire2` harness
    (GREEN under Stacked AND Tree Borrows; the pre-fix shape is RED). `text_input.
    drawWithStyle` (needs `&'a mut TextInput`/invariant `Window<'a>`) is rendered
    as a direct arena-duped buffer segment (visible search string identical; cursor
    glyph omitted — documented forced divergence). New `#[test]`s: `rank_filtered`
    (inclusion/all-spaces/color-gate/ordering), `reselect`, `to_uri`, four
    render-to-`Vec<u8>` `draw` smokes; the interactive `run` path + the two file-
    loading draw smokes are `#[ignore]`/`#[cfg_attr(miri, ignore)]`-on-device.
    test-count/scoped-G3b (missing=0)/acyclic/clippy-D/fmt/Miri all GREEN. **`boo`
    remains the last cli vaxis-deferred leaf.**
  - **`boo` — the `+boo` ghost animation. ✅ RESOLVED (cli C5 finale; the crate's
    LAST vaxis-deferred leaf).** Un-deferred by porting the vxfw widget-framework
    SUBSET that boo consumes + reproducing the `framedata` compressed frame asset:
    * **vxfw subset** — `rustty-vaxis/src/vxfw/` (NEW `pub mod vxfw`): `mod.rs`
      ports `vxfw.zig` core (value types + `Event`/`Command`/`Tick`/`EventContext`/
      `DrawContext`/type-erased `Widget`/`Surface`/`SubSurface` + the `VxfwArena`
      bump-allocator-with-`reset`), `app.rs` ports `App.zig` (the run loop +
      `MouseHandler`/`FocusHandler`), `text.rs` ports `Text.zig` (Text + Line/
      Softwrap iterators + all 10 tests), `center.rs` ports `Center.zig` (+ doctest).
      YAGNI: the other 13 vxfw widgets are unused by ghostty → OUT of scope. 12
      ported vxfw tests GREEN (≥ reference: 10 Text + 1 Center + 2 vxfw.zig). Two
      signed-off divergences: **(a)** `Surface.buffer`/`.children` are RAW slices
      (`*mut [T]`), forced by `App` storing `mouse_handler.last_frame` across
      `arena.reset(.free_all)`; **(b)** `DrawContext.arena` is a NEW vxfw-local
      `VxfwArena` (Rust std has no resettable arena) — the faithful image of
      `std.heap.ArenaAllocator`.
    * **framedata asset** — `crates/rustty-cli/assets/framegen/frames/*.txt` (235
      files, 1,390,810 B, vendored byte-faithfully). `build.rs` EXTENDED with a
      second generator porting `GhosttyFrameData.zig` + `framegen/main.c`: sort
      (strcmp/byte order) → join with `\x01` → raw-DEFLATE (`zlib_sys::deflate_raw`,
      windowBits=-15/level=-1/memLevel=8/strategy=0, exactly framegen) →
      `$OUT_DIR/framedata.compressed` (`boo` `include_bytes!`s it). `zlib-sys`
      EXTENDED with the DEFLATE FFI (`deflateInit2_`/`deflate`/`deflateEnd`/
      `compressBound` + `deflate_raw()`) and `inflate_raw()` (windowBits=-15), over
      the verbatim vendored zlib 1.3.1 `deflate.c`/`trees.c`/`compress.c` (byte-
      identical to the reference's zlib); round-trip KAT GREEN.
    * **`boo.rs`** — rewritten LIVE from the preserved verbatim Zig body: `Boo`
      struct, `widget()`/`typeErasedEventHandler` (init/tick→updateFrame+redraw+
      tick; Ctrl-C/Esc→quit)/`typeErasedDrawFn` (size-warn Text+Center path, else
      centered child Surface aliasing `self.buffer` + arena SubSurface), the 4-state
      span-parser `updateFrame` (assert cell_idx==4100), `run` (platform gate,
      Options/help, decompressFrames, App init/run), `decompressFrames`
      (`include_bytes!` FRAMEDATA → `inflate_raw` → split on `\x01` → 235 frames).
      R2 product-name comments retargeted Ghostty→Rustty.
    * **Aliasing (H1/H2/H3):** the type-erased `*mut Boo` re-entry (buffer alias vs
      `updateFrame` raw writes), `last_frame`-across-`arena.reset`, and the raw
      Surface slices are PROVEN on the fresh `.port/phase-2/mirihar-vxfw` harness
      (GREEN under Stacked AND Tree Borrows; `run` + `#[test]`s). `updateFrame` takes
      a `*mut Boo` and writes through the buffer's own data pointer
      (`Vec::as_mut_ptr` — narrow `&mut Vec` retag, never a `&mut [Cell]` over the
      aliased heap), so no whole-`&mut Boo` ever spans the buffer alias
      (rust-mut-args-strongly-protected + shaper-hook-sb-clean-interleave). The
      App.run FFI+thread tty/vx/Loop shape rides the existing mirihar-wire2 proof
      (SB+TB); the new zlib deflate/inflate C calls are ASan-clean
      (`RUSTFLAGS=-Zsanitizer=address cargo +nightly test`). One new cli dep:
      **`zlib-sys`** ([dependencies] runtime inflate + [build-dependencies]
      build-time deflate; a leaf -sys crate, acyclic G7). New tests: `boo`
      decompressFrames round-trip (235 frames, every frame's span-parse == 4100
      cells), zlib `deflate_raw`⇄`inflate_raw` round-trip KAT. Interactive
      animation is not headlessly verifiable (like the other TUI paths).
      test-count/scoped-G3b/acyclic/clippy-D/fmt/Miri/ASan all GREEN. **The
      rustty-cli crate now has ZERO live vaxis-deferred leaves.**

## rustty-cli C2 (Phase-2 CLI milestone) — pure config/introspection actions + Pager

C2 (CLI_BLUEPRINT.md §7) lands the pure config/font/introspection actions into
`rustty-cli`: `Pager.zig` (2t → `pager.rs`), `show_config.zig` (0t →
`show_config.rs`), `validate_config.zig` (0t → `validate_config.rs`),
`list_actions.zig` (0t → `list_actions.rs`), and `explain_config.zig` (1t →
**DEFERRED**, `explain_config.rs`). Gate-green: cli now 14 `#[test]` (12 C1 + 2
Pager); test-count GREEN (cli_pager 2/2, cli_show_config 0/0,
cli_validate_config 0/0, cli_list_actions 0/0, cli_explain_config 1→0
apc_deferred=1); scoped-G3b missing=0 on all 5 ported files; acyclic G7 GREEN.
New cli Cargo dep: `rustty-cfg-fmt` (member 14 — the `Writer` trait that
`FileFormatter::format` / `helpgen_actions::generate` are generic over, adapted
to the Pager/stdout sinks). Enabling change in `rustty-args`: an additive
`Error::ActionHelpRequested` variant (see below). Deferrals introduced/continued:

- **`explain_config.zig`'s 1 test ("explain"). ✅ RESOLVED (un-deferred).** The
  test asserts `explainOption("font-size") != null` and
  `explainKeybind("copy_to_clipboard") != null`, which require the build-generated
  `help_strings.Config` / `help_strings.KeybindAction` doc tables. Those tables
  now exist (`config_help` / `keybind_help` `build.rs` codegen over the
  `rustty-helpgen` leaf), so `explain_option`/`explain_keybind` return the doc
  text (the config-key / keybind-action-tag VALIDATION via `Key::string_to_enum`
  / `KeybindAction::FIELD_NAMES` is preserved). The test is restored as a live
  `#[test]` and passes; `check_test_counts.py` `apc_deferred` for
  `cli_explain_config` flipped 1 → 0 (cli now 45 `#[test]`). The prerequisite —
  porting the ~193 `Config` field `///` doc blocks into `config_struct.rs`
  (previously deferred to "the fill sub-batch") — landed with this milestone,
  clearing the whole-crate `check_comment_coverage.py --crate rustty-config
  --strict` RED to missing=0.

- **`show_config --docs`. ✅ RESOLVED (same milestone).** `show_config.rs` threads
  `opts.docs` into `FileFormatter { docs }`; the formatter's `docs=true` branch
  now reads the build-generated `help_strings.Config` (`config_help`, wired into
  `formatter_file.rs` in place of the deferred `debug_assert!(!do_docs, …)`), so
  `+show-config --docs` prints each option's doc as `# ` comment lines. The
  keybind-doc path (`Keybinds::format_entry_docs`) likewise now uses
  `RealHelpStrings` → `keybind_help`.

- **`rustty-args` enabling change: additive `Error::ActionHelpRequested`.** C2 is
  the first chunk whose actions route a `help` decl through `args.parse`
  (show_config/validate_config/list_actions `Options` impl `Target::help` /
  `has_help_decl`). The reference's `args.parse` error set INFERS
  `error.ActionHelpRequested` (`cli.ghostty.Action.help_error`, a cli-layer error
  args.zig never names); Rust needs a concrete return type, so the sentinel is
  carried as a new `Error::ActionHelpRequested` variant that `Target::help`
  returns and the cli's `From<ParseError> for ghostty::Error` recovers into
  `ghostty::Error::ActionHelpRequested` (caught by `Action::run`'s `help_error`
  arm). Additive-only: the two internal `match err` sites in `parse`/`parse_self`
  gain an arm restoring the reference's omitted `else => "unknown error {}"`
  catch-all (the sentinel never reaches them — help is matched before
  `parse_into_field`); all 65 rustty-args tests + config(187)/input(113) stay
  green. This completes the C1 `help()`/`has_help_decl()` trait hook (which C1
  established but left unexercised).

- ~~**`show_face.zig` (0t) + `list_fonts.zig` (0t) remain sequencing stubs —
  BLOCKED on rustty-font discovery/SharedGridSet (NOT deferred-by-choice).**~~
  **RESOLVED — the block was a MIS-ATTRIBUTION.** The C2 note above claimed the
  font-discovery + `SharedGridSet` + `SharedGrid`/resolver/collection + `face.name`
  + `Style`/`Presentation`/`DesiredSize`/`DerivedConfig` subsystem was "unported in
  rustty-font". It is NOT unported — it is FULLY PORTED in **rustty-font-shaper**
  (the D1 shaper tier ABOVE vt+font): `discovery.rs` (the CoreText discoverer +
  `Descriptor` + `DiscoverIterator`), `collection.rs` (`getFace`/`Index`/
  `SpecialHasNoFace`), `codepoint_resolver.rs` (`getIndex`), `shared_grid_set.rs`
  (`SharedGridSet`/`ref`/`deref`/`DerivedConfig`), `shared_grid.rs` (`SharedGrid`/
  `resolver`), `face/{mod,coretext}.rs` (`DesiredSize`/`name`), `deferred_face.rs`
  (`familyName`/`name`), and public `Style`/`Presentation`. rustty-font stays the
  PURE leaf below; the discovery subsystem was carved into the shaper tier (memory
  `apc-glyf-font-layering-d1`). Both files are now REAL ports in
  `crates/rustty-cli/src/{list_fonts,show_face}.rs` against `rustty-font-shaper`
  (added as a downward cli dep — acyclic, G7). MAP entries `cli_list_fonts` /
  `cli_show_face` added (both 0-test); scoped-G3b on `list_fonts.zig` +
  `show_face.zig` → missing=0. **One documented seam** (NOT a stub): the reference
  `SharedGridSet.DerivedConfig.init(alloc, config)` is duck-typed over the GLOBAL
  config while the shaper models `DerivedConfig` via break-#9 DI stand-in config
  types, so the config→DerivedConfig conversion is cli-local glue
  (`show_face.rs::font_shared_grid_set_derived_config`), byte-for-byte identical to
  rustty-core's proven `font_shared_grid_set_derived_config` (surface.rs) — all
  fields convert exactly. ~~**Non-macOS Fontconfig discovery** is still DEFERRED in
  the shaper (`discovery.rs`); on those targets both actions map the reference's
  `font.Discover == void` arm (print "no discovery mechanism", exit 1) — the macOS
  CoreText path is the live port (gates run on macOS).~~ **RESOLVED (2026-07-19).**
  The shaper's Fontconfig `Discover` + `DiscoverIterator` + the `DeferredFace` `fc`
  arms (`familyName`/`name`/`load`) were already fully ported; the only residual
  was that both CLI actions still hard-mapped the `font.Discover == void` arm on
  `not(target_os = "macos")`. Re-gated so the live discovery body runs on every
  real-discoverer target — macOS (CoreText) AND Linux/BSD (Fontconfig) — and the
  void arm now maps ONLY to `target_arch = "wasm32"` (web_canvas, no font
  enumeration). `list_fonts` threads the Linux `Library.init` `ft::FtError` and the
  `discovery::Error` through new `Error::{FontLibrary, FontDiscover}` arms (the
  latter un-`cfg`'d from macOS-only to `not(wasm)`); `show_face` reproduces the
  reference `SharedGridSet.init catch { print; return 1 }` on Linux. Runtime-
  verified on Fedora: `+list-fonts` enumerates 112 families via fontconfig,
  `+show-face --cp/--string` resolves real faces, no-arg → exit 1.

## rustty-cli C3 (Phase-2 CLI milestone) — the ssh subsystem (30 tests)

C3 (CLI_BLUEPRINT.md §4) lands the self-contained ssh subsystem into
`rustty-cli` as an `ssh_cache` submodule tree + the `ssh` action:
`ssh-cache/Entry.zig` (3t → `ssh_cache/entry.rs`), `ssh-cache/DiskCache.zig`
(12t → `ssh_cache/disk_cache.rs`), `ssh_cache.zig` (5t → `ssh_cache.rs`), and
`ssh.zig` (10t → `ssh.rs`) = **30 tests**, all live `#[test]`, assertion-for-
assertion. Gate-green: cli now 44 `#[test]` (14 C1+C2 + 30 C3); test-count GREEN
(cli_ssh_cache_entry 3/3, cli_ssh_cache_diskcache 12/12, cli_ssh_cache 4/5,
cli_ssh 9/10 — the anon `Joined`/`{_=DiskCache;_=Entry;}` reifies are skipped by
zig_count, ported for R1 completeness); scoped-G3b missing=0 on all 4 ported
files; clippy/fmt/acyclic GREEN. New cli Cargo dep: `libc` (the DiskCache
`flock(2)` lock — R3, the SAME syscall the reference's `file.tryLock` uses).
**[ADV-FIX 5]** the `xterm-ghostty` TERM token → `xterm-rustty` and the DiskCache
program dir `ghostty` → `rustty` (Entry default terminfo, DiskCache.add's cached
terminfo, ssh's `TERM`/`SetEnv`/`infocmp` remote script + `ghostty-ssh-`
control-path prefix, and every ported test fixture) route through
`rustty-identity` (`TERMINFO_PRIMARY_NAME` / `APP_NAME`), so the tests stay
self-consistent under both the default and `identity-oracle` profiles. No new
deferrals; two convention notes (NOT deferrals — no follow-up scheduled):

- **The 2 DiskCache `FailingAllocator` OOM tests (`disk cache reads/add survives
  allocation failure`) run their single success path.** Rust allocation is
  infallible (aborts), so there is no `error.OutOfMemory` to inject and the
  reference's `while (fail_index)` injection loop collapses to one run. The
  no-double-free / no-leak guarantee the reference proves via FailingAllocator is
  provided statically by ownership (each `Entry` owns its `hostname`/
  `terminfo_version` `Vec<u8>`; the `HashMap` frees each once on drop), with Miri
  (R6) as the UB/leak backstop for the pure logic. Both reference loop bodies are
  preserved verbatim as commented blocks (R1). Not a deferral — no injection
  harness is owed (the guarantee is structural, not test-driven).

- **The ssh runtime spawn paths (`resolveDestination`/`installRemoteTerminfo`/
  `childExec`/`runInner` session setup) are 0-test by design — I/O the harness
  invokes, not a dependency on a running app.** They are fully ported (faithful
  comments, `std::process::Command` over `ssh -G` / ControlMaster / piped-stdin
  `tic`), G3b-gated on comments, and exercised only in real use. `parseManuallyHook`
  routes the reference's inline `iter`-drain through the arg driver's `ConsumeRest`
  seam (`record_replay_arg` → `_ssh_args`, `apply_e_command` → halt) — the
  aliasing-safe analogue of the `-e` path (memory `rust-mut-args-strongly-protected`).

## rustty-cli C5 (Phase-2 CLI milestone) — vaxis/zf-touching actions [RECONCILED 2026-07-10, T14: TUI wiring landed; `boo.rs:22` is the crate's ONLY live seam]

C5 (CLI_BLUEPRINT.md §1/§7 + Risk 1/2) lands the four vaxis/zf-touching actions
into `rustty-cli`: `list_colors.zig` (0t → `list_colors.rs`), `list_keybinds.zig`
(0t → `list_keybinds.rs`), `list_themes.zig` (1741 loc, 0t → `list_themes.rs`),
and `boo.zig` (0t → `boo.rs`). All 0-test → cli stays **44/45 now + 1 deferred**
(explain_config), no contract change. Gate-green: test-count GREEN (cli_list_colors
0/0, cli_list_keybinds 0/0, cli_list_themes 0/0, cli_boo 0/0), scoped-G3b
**missing=0** on all four ported `.zig` files (incl. list_themes' 1741 loc — the
picker is preserved VERBATIM as a commented block), clippy `-D warnings` / fmt /
acyclic G7 GREEN, rustty-vt 2019 (no regression). The plain/enumeration paths are
LIVE; the vaxis/zf render paths are documented deferred placeholder leaves.

**RECONCILIATION (2026-07-10, VALIDATION_REPORT §3b): the blocker below is
GONE.** `vaxis` and `zf` are no longer "unported Zig-source dependencies" —
`rustty-vaxis` is `PORT STATUS: done` (chunks 1a–4b) and `rustty-zf` is ported,
and the TUI-CLI wiring commits (e55e39d — `list_colors`/`list_keybinds`
`prettyPrint`; 1f88eec — the `list_themes` interactive vaxis+zf fuzzy picker,
Miri-proven) FILLED three of the four deferred render leaves. Of the items
below, only **`boo`** remains deferred: `boo.rs:22` `unimplemented!()` is the
crate's ONLY live seam (it needs the vaxis `vxfw` widget framework tier — NOT
ported by chunks 1a–4b, which stop at the `widgets`/TextInput tier — plus the
`framedata` compressed asset). Un-defer trigger: a vaxis vxfw follow-up chunk.
The per-leaf notes below are kept for history with their outcomes marked:

- ~~**`vaxis` + `zf` are unported Zig-source dependencies**~~ **STALE — both
  ported** (TTY UI + fuzzy match; the same class as libxev (D6) and z2d, NOT
  ghostty src and NOT C libs). Historical rationale: every code path that needs
  them is 0-test and sits behind the `tui.can_pretty_print && !plain && isatty`
  gate, so the plain/enumeration paths — the ONLY paths the KAT and any
  redirected pipe hit — landed LIVE first and the render paths deferred:
  - **`list_colors` `--plain`/non-tty path — LIVE.** Iterates `x11_color.map.keys()`
    (sorted `orderIgnoreCase`) and prints `name = #RRGGBB`. ~~The `prettyPrint`
    (vaxis) branch is a deferred leaf~~ **RESOLVED (e55e39d): `prettyPrint` is
    LIVE over rustty-vaxis.** **Enabling change in `rustty-vt` (x11_color.rs):** the ported
    `x11_color` map stored keys *lowercased* (for case-insensitive `get`), which
    would corrupt `+list-colors`' displayed names (e.g. `GhostWhite`→`ghostwhite`).
    Added an additive `Map::keys()` returning the ORIGINAL-case names in rgb.txt
    order (a second lazy parse; `get`/`build_color_map` and the `x11 color map`
    test are untouched — rustty-vt stays 2019, x11_color.zig G3b stays missing=0).
  - **`list_keybinds` `--plain`/non-tty path — LIVE.** Streams
    `config.keybind.formatEntryDocs(entryFormatter("keybind", writer), opts.docs)`
    over the ported `rustty_cfg_fmt::EntryFormatter` (`--docs` rides the same
    `help_strings`/`NoHelpStrings` deferred seam already tracked for C1/C2 — no new
    deferral). ~~The `prettyPrint` (vaxis) + its `ChordBinding`/`TriggerNode`/
    `iterateBindings` helpers are a deferred leaf~~ **RESOLVED (e55e39d): all
    LIVE over rustty-vaxis.**
  - **`list_themes` theme ENUMERATION — LIVE.** Built on the ported
    `config/theme.zig` `LocationIterator` + `Config.loadFile` + `shouldIncludeTheme`
    (luminance filter). ~~The interactive vaxis+zf fuzzy-finder picker (`Preview`
    and all its methods, `preview`, `resolveAutoThemePath`/`writeAutoThemeFile`,
    `color`, `ThemeListElement.toUri`) is a deferred leaf~~ **RESOLVED (1f88eec):
    the interactive fuzzy picker is LIVE over rustty-vaxis + rustty-zf
    (Miri-proven), `lorem_ipsum.txt` included.**
  - **`boo` — STILL DEFERRED (the crate's ONLY live seam, `boo.rs:22`).** A 100%
    vaxis/**vxfw** ghost animation (needs the `framedata` compressed asset too);
    `run` is an `unimplemented!()` marker and the whole reference (`Boo` widget,
    span-parser, flate decompression) is preserved verbatim as comments
    (comment-fidelity, scoped-G3b missing=0). NOT unblocked by the vaxis port:
    chunks 1a–4b stop at the `widgets`/TextInput tier and do NOT port the `vxfw`
    widget framework `Boo` is built on. Un-defer trigger: a vaxis vxfw follow-up
    chunk + the `framedata` asset (R3 copy).

- **`list_themes` resources-dir two-seam note (NOT a new deferral).** The reference
  reads `global_state.resources_dir.app()` (the process-global) for BOTH the
  "resources dir not found" stderr warning AND `theme.zig`'s `.resources`
  LocationIterator arm. In the port the warning reads `rustty_core::STATE` (the
  `OnceLock<GlobalState>`, ADV-FIX 2) while the config `theme.rs` `.resources` arm
  reads a SEPARATE config-crate seam (`theme::set_resources_dir`, BREAK #1). Both
  are populated from the same `GlobalState.resources_dir` by the bin's init (C6), so
  they stay consistent; the C5 warning path uses `STATE.get()` (None until C6 →
  collapses to the reference's `resources_dir == null` warning, not an
  uninitialized-global deref). No follow-up owed beyond the already-scheduled C6 bin
  wiring.

- ~~**`show_face.zig` (0t) + `list_fonts.zig` (0t) STILL sequencing stubs**~~
  **RESOLVED** — see the C2 note above (corrected): the "blocked on rustty-font
  discovery/SharedGridSet" claim was a mis-attribution; that subsystem is fully
  ported in **rustty-font-shaper**, and both files are now real ports against it
  (`cli_list_fonts`/`cli_show_face` MAP entries + scoped-G3b missing=0).

## rustty bin C6 (Phase-2 CLI milestone) — the `rustty` bin (action dispatch + usage banner + Phase-3 GUI leaf)

C6 (CLI_BLUEPRINT.md §5/§6 + ADV-FIX 2/6) lands the `rustty` bin: `main.zig`
(the entrypoint selector → `main.rs`) + `main_ghostty.zig` (`main()` dispatch,
`logFn`, `std_options` → `main_ghostty.rs`). **[ADV-FIX 2]** action DETECTION
lives in the BIN (bin deps cli): `main_ghostty::state_init` runs
`cli::action::detect_args::<cli::ghostty::Action>` over `os::args` and builds the
bin's OWN `rustty_global::GlobalState<Action>` (NOT `rustty_core::STATE`, bound
`Action=()`); `rustty_apprt::init` stays cli-free (no `apprt→cli` cycle). Dispatch:
detected action → `exit(action.run())`; `app_runtime == .none` + no action → the
usage banner + `exit(0)`; the GUI `else` branch (App.create + apprt.App.init/run
over xev, D6) is the Phase-3 leaf (`unimplemented!()`). The headless product runs
for real (the "it runs" KAT `crates/rustty/tests/cli_smoke.rs`: +version / --version
/ +list-actions / +validate-config / +ssh-cache / +new-window / no-args banner /
+bogus, all asserted end-to-end over the spawned binary). Gate-green: bin 2
`#[test]` (main 1 ref_entrypoint + main_ghostty 1 ref_all_decls) + 9 KAT smoke
tests; test-count GREEN (main 0/1, main_ghostty 0/1, main_c 3/3→apprt-c,
lib_vt 0/308→vt-c); scoped-G3b **missing=0** on the two ported `.zig` files
(main.zig + main_ghostty.zig); clippy/fmt/acyclic GREEN (no `apprt→cli` edge — the
bin is the top sink over cli+apprt). New bin Cargo deps: `rustty-global`,
`rustty-build-config`, `rustty-identity`. Deferrals introduced/continued:

- **Phase-3 GUI launch leaf (`main_ghostty.rs`).** The reference `main()`'s GUI
  path (`App.create(alloc)` + `apprt.App.init/run` the event loop, `startQuitTimer`)
  reaches the concrete embedded/GTK apprt backend + the xev event loop (D6). It is
  ported as a documented `unimplemented!()` leaf with the reference body preserved
  verbatim as comments (the GPU/GTK/xev "deferred placeholder leaf" precedent). The
  reference reaches it only for a GUI build (`app_runtime != .none`); the headless
  `.none` bin (the current darwin/Windows default) never does — it runs the action
  or prints the usage banner. **Target:** un-defer when the embedded/GTK apprt
  backend + xev (D6) land (Phase-3). The no-args KAT is scoped to `.none`
  (`app_runtime()` guard) so it stays honest on a future `.gtk` build.

- **`main_c.zig` (3t) re-homed to rustty-apprt-c; `lib_vt.zig` (1t) re-homed to
  rustty-vt-c [ADV-FIX 6].** The 3 `ghostty_string_s` tests are ported
  assertion-for-assertion into `rustty-apprt-c/src/lib.rs`'s `main_c_tests` mod
  (they exercise the `String`/`ghostty_string_s` C hub the apprt-c crate ports); the
  `check_test_counts.py` crate_map entry `main_c → apprt-c/lib.rs` keeps the count
  exact (3/3). `lib_vt.zig`'s sole `test {}` is the anonymous refAllDecls-style reify
  (zig_count anon-skip → zig=0), re-homed `lib_vt → vt-c` (already covered by vt-c's
  C-ABI reify surface). **The bin does NOT dep apprt-c** (apprt-c is the top-sink).
  NOTE: the phase-0 `tests-summary.json` crate_map still partitions main_c.zig /
  lib_vt.zig under `bin`, so a WHOLE-CRATE `check_comment_coverage.py --crate rustty`
  reports their (and main_gen/main_wasm/main_build_data's) reference comments as
  "missing" — this is EXPECTED per the DEFERRALS "Rule" (main_c's non-test comment
  fidelity — ghostty_init/ghostty_info/ghostty_translate/DllMain/the comptime export
  block — is the rustty-apprt-c E-series' concern, not C6). The C6 scoped-G3b on the
  two files the bin actually ports (main.zig + main_ghostty.zig) is missing=0.

- **`main_gen.zig` / `main_wasm.zig` / `main_build_data.zig` (0t each) DEFERRED.**
  These are standalone build-tool / wasm-module entrypoints, NOT on the `rustty`
  exe's `exe_entrypoint` switch (main.zig selects only `.ghostty` → main_ghostty for
  the `rustty` binary; helpgen/mdgen/webgen are the codegen tools). All 0-test → no
  test-count contract impact; no MAP entry yet. **Target:** port with the
  gen/wasm-artifact tooling milestone (the same milestone that builds the
  `help_strings` codegen — see below).

- **`logFn` / `std_options` — the log-facade seam is a deferred seam (continued).**
  `main_ghostty.rs` ports `logFn` (the stderr branch is LIVE, gated on the bin's
  `GlobalState.logging.stderr` + the debug-only-in-Debug rule, exactly as the
  reference) and `std_options` (the compile-time `LOG_LEVEL`). The macOS unified-
  logging branch (`macos.os.Log`) is the SAME log-facade seam the rest of the tree
  defers (rustty-os stubs `std.log.scoped`) — preserved verbatim as comments (incl.
  the `sudo log stream …` view predicate, R2-renamed to the `io.airely.rustty`
  bundle id). The action path sets `logging.stderr = false` (reference global.zig
  line 107) so action stdout stays clean; on the headless `.none` build stderr
  logging is off by default regardless. NOTE: the config/os crates emit their OWN
  `info(config):` log lines (a separate pre-existing facade) that the bin's
  `logging.stderr` flag does not gate — a documented consequence of the rustty
  log-facade split, not a C6 regression; the KAT reads stdout (diagnostics/banner)
  separately from that stderr noise. **Target:** un-defer with the unified log-facade
  follow-up.

- **`version.zig` renderer/libxev/toolchain STUB lines (continued from C1, ADV-FIX
  1).** The `+version` KAT asserts ONLY exit 0 + stdout starts `rustty ` — it does
  NOT assert the `- renderer` (`RENDERER_STUB`), `- libxev` (`LIBXEV_STUB`), or
  `- Rust version` (`TOOLCHAIN_VERSION_STUB`) line CONTENT, which stay documented
  stubs until R-d (renderer) / D6 (libxev) land. Unchanged by C6.

- **`explain_config` (1t DEFERRED) + `help_strings` codegen (continued, ADV-FIX 3).**
  The bin does not un-defer these; `Action::help_text` still returns "" (the
  `+action --help` text) and the explain_config test stays SkipZigTest-style until
  the `help_strings` codegen follow-up lands (which also enables +help /
  show_config --docs / list_keybinds --docs, all 0-test). The KAT does not exercise
  `+explain-config` / `+help`.

- **vaxis/zf TUI leaves (continued from C5).** `boo` (pure vaxis animation),
  `list_themes`' interactive vaxis+zf picker, and the `prettyPrint` branches of
  `list_colors`/`list_keybinds` remain deferred placeholder leaves (their
  plain/enumeration paths are LIVE and are the only paths the KAT and any redirected
  pipe hit). The bin's dispatch reaches every action's `run()`; the deferred render
  paths are 0-test and only entered on an interactive TTY (which the KAT is not).
  Un-deferred with the `vaxis`/`zf` port.

- **`rustty_apprt::init` resources-dir wiring (resolves the C5 note).** C6's
  `state_init` calls `rustty_apprt::init()` — the one live upward call the reference
  `global.zig` init makes (`self.resources_dir = try apprt.runtime.resourcesDir`) —
  which populates `rustty_core::STATE.resources_dir`. This is the "C6 bin wiring" the
  C5 `list_themes` resources-dir note scheduled: the bin's own `GlobalState<Action>`
  carries the action, core STATE carries the resources dir, both set here so the
  list_themes `.resources` warning seam stays consistent. The remaining reference
  init side effects (GHOSTTY_LOG parse, signal setup, xev detect, the version/
  optimize/runtime/font log lines, rlimit init, crash.init, ensureLocale,
  glslang.init, oni.init, i18n.init) — **LANDED (WS4, 2026-07-23)** in
  `rustty_apprt::init_with_action` (`crates/rustty-apprt/src/lib.rs`), which the
  bin now calls instead of the bare `rustty_apprt::init()` precursor.

---

## rustty-xev C3 (loop core + Completion/Operation + tick) — deferrals

Chunk C3 landed the kqueue loop core: `loop.rs` (Options/RunMode/Callback/
NoopCallback/CallbackAction/CompletionState), `kqueue.rs` (the `Loop`,
`Completion`, the 17-variant `Operation`/`Result` unions, `available`,
`kevent_syscall`/`kevent_init`, `Wakeup`, and the whole `tick` — process_
cancellations → submit → timer-heap firing → thread-pool MPSC migration [dead
branch] → completions drain + EV.DELETE disarm staging → kevent_syscall → event
dispatch), `common.rs` (`userdata_value`), the `AsyncMachPort` init/deinit/notify
subset (`async.rs`), the `std.net.Address` subset (`net.rs`), the `Timer` watcher
init/deinit/run subset (`timer.rs`), and the `ThreadPool` TYPE SURFACE
(`thread_pool.rs`). All 12 kqueue `test{}`s land (10 timers/cancel/socket tests +
`mach port` — runnable in C3 since the `machport` op + darwin bindings are ported —
+ the `file IO on thread pool` skip-stub) plus KAT 2 (Timer fires). Gate-green in
isolation.

DEFERRED (documented; not needed to unit-prove the loop):

- **`ThreadPool.zig` work-stealing BODY** (§1/§6). Only the TYPE SURFACE
  (`Task`/`Batch::from`/`schedule`/`init`/`deinit`/`shutdown` + a Rust-glue
  `Task::default`) is ported so `kqueue.rs` compiles; ghostty passes NO
  thread_pool (`Loop.init(.{})`), so the kqueue thread-pool branch is a faithfully-
  ported DEAD branch (never taken while `thread_pool` is None). The
  `kqueue: file IO on thread pool` test is a **SkipZigTest-style skip-stub `#[test]`**
  (count holds, reference comments preserved) — it activates with the pool body.
  HONEST-SKIP wiring (VALIDATION_REPORT §3e): the stub is gated on a
  `THREAD_POOL_BODY_PORTED = false` marker const (the `TMUX_DCS_WIRED`
  precedent) and its body `include_str!`s THIS file, asserting this entry still
  exists — deleting this entry without porting the pool body fails the test.
  The pool-body chunk must port the live reference body AND flip the const.
- **`Loop.init(options: Options)` → `Loop::init()` (no-arg).** ghostty's macOS
  build only ever calls `Loop.init(.{})` (default Options, no thread_pool) and
  termio's D6 seam fixes the signature to no-arg; the Options-carrying form + the
  `thread_pool` path are deferred with the pool body. The `thread_pool:
  options.thread_pool` assignment is preserved (default `null`).
- **`watcher/timer.zig`: `reset`/`cancel` + its 5 `test{}`s** (`timer`, `reset`,
  `reset before tick`, `reset after trigger`, `cancel`). C3 ports only
  `init`/`deinit`/`run` (termio's `Timer::init()` + KAT 2). The loop-level
  `timer`/`timer_reset` machinery the watcher rides IS ported and exercised by the
  kqueue timer x-family tests. **→ RESOLVED in C7:** `Timer::reset` (delegates to
  `loop.timer_reset`, reusing the `run` trampoline) + `Timer::cancel` (a `.cancel`-
  op `Completion::new_op` + `loop.add`, with its own `cancel_trampoline` mapping
  `Result::Cancel → CancelError`) landed, and all 5 `TimerTests` are live `#[test]`
  (timer.rs=6 with KAT 2; `xev_timer` zig=5 → rust=6).
- **`watcher/async.zig`: `wait`/`drain` (the `machport`-op trampoline) + the
  non-Darwin `AsyncEventFd`/`AsyncLoopState`/`AsyncIOCP` variants + the 3
  `AsyncTests` + KAT 1** → chunk C4. C3 ports the `AsyncMachPort`
  init/deinit/notify lifecycle (the loop `Wakeup` embeds it) + the loop `machport`
  op arm (start/kevent/perform/syscall_result), so the wiring is present.
- **`watcher/process.zig` (`ProcessKqueue`) → C5; `watcher/stream.zig`
  (`Stream`/`WriteQueue`/`WriteRequest`) → C6.** `xev::Stream`/`Process`/
  `WriteRequest`/`WriteQueue` remain lib.rs sequencing placeholders (F3 trait
  surface preserved: `WriteRequest`/`WriteQueue: Default`).
- **The non-Darwin `Wakeup` else-branch** (eventfd BSD TODO) — cfg-elided (macOS-
  only); its verbatim comments are preserved in the `Wakeup` PORT note.
- **`dynamic.zig`** — comptime-elided on single-candidate macOS (§0); N/A, not
  weakened. **Other backends** (epoll/io_uring/iocp/wasi_poll), **`watcher/file`
  /`tcp`/`udp`**, **`c_api.zig`** — whole follow-up ports (§1). Non-macOS handle
  types stay lib.rs placeholders.
- **`@sizeOf(Completion) == 256` → `size_of::<Completion>() == 232`
  (divergence).** Rust's tagged-enum `Operation` (160B) is laid out differently
  from Zig's `union(OperationType)` (176B); `flags` is a 2-byte struct vs Zig's
  bit-packed 1 byte; the deferred-ThreadPool `Task.node` is a ZST placeholder vs
  Zig's 8-byte Zap node. `Result` (24B) + `Address` (108B) match Zig. The layout
  tripwire's PURPOSE is preserved — 232 is pinned so any accidental Completion
  bloat trips the `const _: () = assert!(...)` exactly as Zig's `@compileLog`.
- **`check_test_counts.py` rustty-xev mapping** — deferred to chunk C7 ([ADV-FIX
  F7]: the count-gate maps ghostty `src/`, but libxev lives in the zig cache; a
  libxev base-path / in-repo manifest is added in C7 with the full in-scope
  contract). The per-crate `cargo test -p rustty-xev` count (21 = 8 C1 + 12
  kqueue + KAT 2) is the authoritative backstop today.

## rustty-xev C4 (Async mach-port wakeup: wait/drain + tests) — deferrals

Chunk C4 completes `watcher/async.zig`'s `AsyncMachPort` on macOS: `wait` (the
`.machport`-op trampoline — a monomorphized `wait::<UD, CB>` with a ZST comptime
`cb`, `Completion::new_op` reset, `loop.add`) + `drain` (the `MACH_RCV_MSG |
MACH_RCV_TIMEOUT` receive loop that coalesces N queued notifies → 1 wakeup,
discarding message bodies) + the 3 reference `AsyncTests` (`async`, `async: notify
first`, `async batches multiple notifications`) + rustty KAT 1 (cross-thread
wakeup round-trip: `notify` from a spawned thread — the [ADV-FIX F2] `Async`
`port:u32` auto-`Send` handle — then `run(.once)` wakes exactly once). The loop
`machport` op arm + `Wakeup` wiring were already ported in C3. A `pub(crate)`
`Completion::new_op(op, userdata, callback)` helper was added to `kqueue.rs` (the
faithful equivalent of the reference `c.* = .{ .op, .userdata, .callback }` full
reset) so the sibling `async_watcher` module builds a completion without seeing
the module-private internal fields. `xev_async` (zig=3, rust=4) added to
`check_test_counts.py`. Per-crate count now 25 (21 + async×3 + KAT 1).

DEFERRED (documented; not needed to boot a shell on macOS):

- **The non-Darwin `Async` variants** — `AsyncEventFd` (Unix/Linux eventfd,
  `waitRead`/`waitPoll`), `AsyncLoopState` (WASI loop-state), `AsyncIOCP`
  (Windows), and `AsyncDynamic` (multi-backend union). Comptime-elided on macOS
  (§0: the single kqueue+Darwin candidate selects `AsyncMachPort` only) and
  deferred with their whole backends (§1). Their reference doc comments are
  preserved verbatim in `async.rs` (R3; G3b on `async.zig` is missing=0), and each
  variant's `test { _ = AsyncTests(xev, Self); }` is an anon refAllDecls block that
  `zig_count` skips (only the 3 quoted `AsyncTests` count).
- **MIRI:** `async.rs` `wait`/`drain` + all 4 tests issue live mach syscalls
  (`mach_port_*`, `mach_msg`) — not runnable under Miri (documented in
  MIRI_EXCEPTIONS.md alongside the C3 kqueue/mach entry); the live KAT is the
  correctness backstop.

## rustty-xev C7 (package surface + gate mapping + KAT finale — Milestone A DONE) — deferrals

Chunk C7 is THE FINALE of the isolated `rustty-xev` port (Milestone A). It fills
the package re-export surface = the kqueue `Api()`: `main.zig`'s forwards + `api.zig`
(the `Xev(be, T)` decl-forwarder, which on macOS collapses to this crate's public
surface, §0) + `backend.zig` (the `Backend` enum + `Default::default() == Kqueue` +
single-element `candidates()` — the §0 fact that comptime-elides `dynamic.zig`). It
un-defers `Timer::reset`/`cancel` (above), ports `api.zig`'s 2 `test{}`
(`ref_all_decls` reify + `completion is zero-able`) into `lib.rs`, and finalizes
[ADV-FIX F7] — the full `check_test_counts.py` rustty-xev mapping (`xev_kqueue`,
`xev_timer`, `xev_api`, `xev_backend`, `xev_main`, `xev_loop`, `xev_common`,
`xev_thread_pool` added to the existing queue/mpsc/heap/darwin/async/process/stream
entries), so R1 is enforced across the whole in-scope surface (rust `#[test]` ≥ zig
`test{}` per module, all GREEN). The placeholder handle types were already filled in
place by C3–C6 (downstream `use` paths never changed); C7's `cargo build --workspace`
+ `cargo test -p rustty-termio -p rustty-core -p rustty-cli` prove termio(44)/core(15)/
cli(45)/vt(2019) still compile+test green ([ADV-FIX F3] the `Default`/`Copy`/`Debug`
trait surface matches what the placeholder provided).

**R1 tally (mechanical, gate-counted = quoted `test "..."` + behavioral named-unquoted,
anon `test {` skipped):** in-scope zig `test{}` = 1 (queue) + 1 (queue_mpsc) + 6 (heap)
+ 12 (kqueue) + 3 (async) + 5 (timer) + 3 (process) + 4 (stream) + 1 (api) = **36**
(the blueprint's "~35" + api.zig's `completion is zero-able`). Rust `#[test]` total =
**42** = 36 R1-parity + 4 KATs (async/timer/process/stream live round-trips) + 1
`ref_all_decls` reify (anon-refAllDecls port, +margin) + 1 `backend` sanity (+margin).
Every mapped module is rust ≥ zig.

DEFERRED (documented; not needed to boot a shell on macOS — the permanent finale list):

- **`ThreadPool.zig` work-stealing BODY** (§1/§6). Only the TYPE SURFACE
  (`Task`/`Batch`/`schedule`/`init`/`deinit`/`shutdown` sigs) is ported in
  `thread_pool.rs` so `kqueue.rs` compiles; ghostty passes NO `thread_pool`
  (`Loop.init(.{})` with no pool), so the pool is unreachable on macOS. Consequence:
  the `kqueue: file IO on thread pool` test is a **SkipZigTest-style skip-stub
  `#[test]`** (`kqueue_file_io_on_thread_pool` — the count stays 12, honest R5-safe:
  it defers genuinely-unused functionality, not a live path). G3b on `ThreadPool.zig`
  is intentionally NOT missing=0 (only the type surface is ported; the deferred body's
  ~700 comments are the documented gap, this bullet).
- **`dynamic.zig`** (525 loc, 5 tests) — comptime-elided on single-candidate macOS
  (§0: `Backend::candidates()` is `&[Kqueue]`, so `Xev(bes)` collapses to
  `bes[0].Api()`); N/A, not weakened. The `Dynamic`/`DynamicXev`/`Sys`/`dynamic`/
  `backend`/`available` main.zig decls are noted (reference doc preserved, R3) but not
  re-exported.
- **Other backends** (`backend/epoll` 12t, `io_uring` 13t, `iocp` 14t, `wasi_poll` 9t),
  **`watcher/file`** (8t, `xev.File` — zero ghostty uses), **`watcher/tcp`** (5t),
  **`watcher/udp`** (6t), **`c_api.zig`** (1t, libxev's C ABI — rustty consumers use the
  Rust API) — whole follow-up ports (§1). Non-macOS handle types stay `lib.rs`
  placeholders.
- **[ADV-FIX F6] `os/flatpak.rs` + vt-search xev refs** — NOT resolved by making
  `rustty_global::xev` re-export `rustty-xev` (that would cycle
  `global → xev → global`). They resolve LATER via DIRECT edges: `os → xev` (flatpak)
  and `vt → xev` (search), both acyclic (rustty-xev deps neither os nor vt).
  `rustty_global::xev` stays the `()` package anchor exactly as `global.zig`'s
  `pub const xev` is.
- **`net.rs`** (the `std.net.Address` subset for connect/sendto/recvfrom) is a std
  port, not a libxev source file, so it is intentionally UNMAPPED in
  `check_test_counts.py` (no `.zig` reference oracle).
- **Milestone B (NEXT — not this chunk):** un-defer TERMIO ONLY ([ADV-FIX F8]) —
  `Thread.run` loop + `Termio.init/threadEnter/processOutput` + `Exec`
  `Subprocess.start`/`ReadThread`/`queueWrite` + the core `Async::notify()` leaves —
  capped with a live-shell KAT. `renderer.Thread` stays Phase-3 GPU tier (GPU-and-xev
  blocked; landing rustty-xev does not unblock it). See NEXT_PHASE_PLAN.md.

## rustty-xev LINUX port L1 LT8 (per-target Dynamic re-export + gate wiring) — deferrals

Task LT8 is the finale of the Linux half of `rustty-xev` (the Milestone-A-equivalent
for Linux). It wires the per-target package surface in `lib.rs`: on Linux the crate
re-exports `dynamic::*` under the SAME public names the macOS block exports, so
`rustty_xev::Loop`/`Completion`/`Async`/... resolve to `Dynamic` on Linux and stay
flat kqueue on macOS — which is exactly what the reference does, since ghostty
imports the package as `@import("xev").Dynamic` and `Xev(bes)` short-circuits to
`bes[0].Api()` only when there is ONE candidate (dynamic.zig:36-38 — true on macOS
`&.{.kqueue}`, false on Linux `&.{.io_uring, .epoll}`). The pre-LT8
`#[cfg(not(target_os = "macos"))]` placeholder seam (no-op `unimplemented!()`
handles) is NARROWED to `not(any(macos, linux))` and renamed `deferred_backend_seam`
— it now stands in only for the `wasi_poll`/`iocp` targets it actually describes
(notably the wasm32 build, which has no event loop at all).

**[ADV-FIX F7] gate wiring:** `check_test_counts.py` gains four rows against the
SAME pinned `GHXEV` libxev package the macOS rows use (the Linux read-only oracle
checkout is the same commit, verified by reproducing every existing xev row's
authored zig count exactly): `xev_epoll` (zig=12, rust=15), `xev_io_uring` (zig=13,
rust=16), `xev_dynamic` (zig=4 gate-counted of 5 `test{}` — the 5th is the anon
`refAllDecls`, rust=14), `xev_timerfd` (zig=1, rust=1). All 19 xev rows are rust ≥
zig. This moves `backend/epoll` / `io_uring` / `dynamic.zig` OUT of the C7 finale's
"Other backends ... whole follow-up ports (§1)" DEFERRED bullet above — that bullet
is now only `iocp` (14t) and `wasi_poll` (9t).

**R1 tally (Linux side, gate-counted):** in-scope zig `test{}` = 12 (epoll) + 13
(io_uring) + 4 (dynamic) + 1 (timerfd) = **30**. Rust `#[test]` over the same four
mapped files = **46** (15 + 16 + 14 + 1) = 30 R1-parity + 16 rustty KATs/+margin
(live loop/timer/async/process/pty round-trips, each run under BOTH candidate arms).
Per-crate `cargo test -p rustty-xev` on aarch64-unknown-linux-gnu = **59** = those 46
+ 8 target-shared (queue 1 + queue_mpsc 1 + heap 6) + 1 `linux/eventfd.rs` + 2
`backend.rs` + 2 the new `tests/surface_parity.rs` integration tests. (`lib.rs`'s
`api_tests` are macOS-gated and do not run here — `api.zig`'s R1 test is instead
carried on Linux by `dynamic_completion_zero_able`, the same `test "completion is
zero-able"` at dynamic.zig:202.)

**SUPERSEDED by Tasks 1-6** (`xev-libxev-completion` branch, 2026-07-19): the 59
figure above predates the ThreadPool body (Task 1), epoll pool wiring (Task 2),
and the TCP/UDP/File watchers (Tasks 3-5) — all landed with their own
`#[test]`s and are NOT re-tallied against zig `test{}` counts here (each task's
commit is the record of what it added; see the RESOLVED bullets immediately
below for the file/commit map). The current authoritative count is `cargo test
-p rustty-xev`'s own summary line: **81** unit tests + **2**
`tests/surface_parity.rs` integration tests = **83**, all green (Task 6
verification pass, including the one net-new test this task adds —
`dynamic_tcp_accept_connect_send_recv_close`).

STILL DEFERRED on Linux (documented; each is a REFERENCE decision or a ledgered
follow-up, not a weakening of a live path):

- ~~**`watcher/file.zig` (8t), `watcher/tcp.zig` (5t), `watcher/udp.zig` (6t)** — the~~
  ~~`xev.File`/`xev.TCP`/`xev.UDP` watchers stay deferred on Linux exactly as they are~~
  ~~on macOS (the C7 finale bullet above): zero ghostty uses. `main.zig:35,38-39` +~~
  ~~`dynamic.zig:83,86-87` forward them; `lib.rs` forwards them on NEITHER target, so~~
  ~~the two surfaces stay identical in what they omit as well as what they export.~~
  **RESOLVED on Linux [2026-07-19, `xev-libxev-completion` branch, Tasks 3-5]:**
  `File`/`TCP`/`UDP` are now fully ported per-backend (`epoll.rs`+`io_uring.rs`),
  wrapped as the Dynamic `File`/`TCP`/`UDP` enums (`dynamic.rs`), and re-exported
  from `lib.rs`'s `pub use dynamic::{...}` block (Task 6 confirmed the export was
  already wired and closed the one gap in test coverage — `dynamic_tcp_accept_
  connect_send_recv_close`, the missing `TCPDynamic` KAT per tcp.zig:498-500,
  mirroring the existing `dynamic_udp_read_write`/`dynamic_file_*`). Commits:
  fd70843 (TCP), 999dd54 (UDP), ac87a72 (File). **STILL deferred on macOS** (zero
  ghostty uses there — unchanged, out of scope for the Linux port): the two
  targets' surfaces now deliberately DIVERGE on this point, pinned by
  `tests/surface_parity.rs`'s Linux-only `needs_sized::<File/TCP/UDP>()` block.
- ~~**`ThreadPool` (body AND type surface, on Linux only)** — macOS forwards~~
  ~~`xev::ThreadPool` (type surface, body deferred). `dynamic.zig:56` forwards~~
  ~~`main.ThreadPool` too, but the Linux side exports NOTHING: R5 forbids importing~~
  ~~the macOS-gated `thread_pool` module, and neither `epoll::Loop` nor~~
  ~~`io_uring::Loop` carries a pool (epoll.rs:149-152 — the reference's `thread_pool:`~~
  ~~`?*ThreadPool` + `thread_pool_completions` fields are omitted, and~~
  ~~`linux::Options::thread_pool` is an opaque `Option<*mut c_void>` instead of the~~
  ~~macOS-gated type name). Consequence: epoll's threadpool-requiring ops yield~~
  ~~`CloseError::ThreadPoolRequired` exactly as the reference's `thread_schedule` does~~
  ~~when `Loop.thread_pool == null`. Same status as the macOS "kqueue: file IO on~~
  ~~thread pool" skip-stub — genuinely-unused functionality, not a live path. Un-defer~~
  ~~together with the macOS ThreadPool body.~~
  **RESOLVED (body ported + wired on Linux) [2026-07-19, `xev-libxev-completion`
  branch, Tasks 1-2, exported Task 6]:** the full `ThreadPool.zig` work-stealing
  body (kprotty's Zap pool) is now ported in `thread_pool.rs`, shared by BOTH
  targets, and proven Stacked- AND Tree-Borrows clean on
  `.port/phase-2/mirihar-threadpool/` (2/2 tests, both models, re-verified green
  by this same task). `linux::Options::thread_pool` is now the concrete
  `Option<*mut crate::thread_pool::ThreadPool>` (no more opaque `*mut c_void`),
  and `epoll::Loop`/`io_uring::Loop` both accept + wire a real pool
  (epoll.rs:149-152's field is the typed pointer; `io_uring::Loop` accepts but
  ignores it — native SQEs need no blocking-op pool). `epoll`'s threadpool-
  requiring ops (File/TCP/UDP `.close`, File `pread`/`pwrite`) now actually RUN on
  a pool when one is attached, and still yield `CloseError::ThreadPoolRequired`/
  `ReadError::ThreadPoolRequired`/`WriteError::ThreadPoolRequired` when none is
  (both paths covered by `epoll_close_on_thread_pool` and its no-pool half).
  `ThreadPool` is re-exported from `lib.rs` on Linux under the SAME name the
  macOS block already used (Task 6 — `pub use thread_pool::ThreadPool;`, gated
  `#[cfg(target_os = "linux")]`, library-completeness parity: `dynamic::Options`'s
  `thread_pool` field needs the type nameable by any caller constructing one).
  Commits: 89dde0f/e6885c0 (ThreadPool body + the ledgered `Event` shutdown-wake
  deviation below, Task 1), 99f02ec/6338419 (epoll pool wiring, Task 2). The ONE
  entry this does NOT resolve is narrower and stays open on macOS only: the
  kqueue "file IO on thread pool" skip-stub (macOS's `xev.File` watcher never
  actually routes through the pool — zero ghostty uses of `xev.File` there); see
  that entry above/DEFERRALS.md's ThreadPool `Event` shutdown-wake deviation entry
  below, which is UNCHANGED by this resolution and stays exactly as Task 1 wrote
  it.
- **`Callback` / `noopCallback` / `Result` / `Operation` / `Sys`** — the macOS block
  exports these (there the surface IS the static kqueue `Api()`); the Linux block
  does not, because `dynamic.zig`'s forwarded-decl list (dynamic.zig:56-88) contains
  none of them. This is the reference's own "must be limited to the least common
  denominator of all backends" statement (dynamic.zig:20-23), not a gap: a Dynamic
  `Completion`'s op/callback are backend-typed and private, so no consumer could
  build one to hand to `Loop.add`. Same reason `Loop::tick` (epoll-only),
  `Loop::submit`/`cancel` (io_uring-only) and `Loop::add`/`timer`/`timer_reset` are
  absent from `dynamic::Loop` (dynamic.rs's "OMITTED `Loop` methods" note).
- **`available`** — `main.zig:11` forwards the *static* Api()'s availability probe.
  `Dynamic` has no such decl; `detect`/`prefer` (exported) consult each candidate's
  `available()` internally (dynamic.zig:98/113), so `epoll::available`/
  `io_uring::available` stay module-private.
- **`Timer::cancel` on Linux** (timer.zig:369) — carried over from LT7 (see
  dynamic.rs's PORT NOTE at the end of `impl Timer`). `TimerLoop.cancel` must WRITE a
  backend `Operation` (`epoll::Operation::Cancel` / `io_uring::Operation::TimerRemove`)
  into a completion, and neither backend exposes a `Completion::new_op` seam to
  `dynamic.rs` the way `kqueue::Completion::new_op` (`pub(crate)`) does for macOS's
  `timer.rs`. Nothing in the Linux shell path calls it (`Timer::reset`, which IS
  ported, is what termio's scroll/coalesce/sync timers use) and the underlying loop
  machinery is ported + tested inside each backend (`epoll_timer_cancellation`,
  `io_uring_timer_remove`). To un-defer: make `Completion::new_op` `pub(crate)` in
  epoll.rs/io_uring.rs and add the two arms. NOTE `rustty-core`'s renderer_seam
  (`cursor_h.cancel(...)`, renderer_seam.rs:1060) DOES call it — that is a
  core-tier/Phase-3 path, not termio's, but it means core will not build on Linux
  until this is un-deferred.
- **`io_uring::available()`'s false path is untestable on this host** (carried over
  from LT5), and so is the `detect()` FALLBACK arm that depends on it: this host has
  io_uring enabled, so `detect()` always lands on the FIRST candidate and the
  "io_uring unavailable → fall back to epoll" branch never executes. NOT papered over
  with a mock seam: `available()` is a live `io_uring_setup` probe, and a seam that
  let a test force it false would be testing the seam, not the fallback.
  `dynamic_detect` therefore asserts the reference's own invariant in a form that
  holds either way (`detect()` lands on the FIRST AVAILABLE candidate, computed from
  the same `available()` the code uses), and `dynamic_prefer`/every KAT drive BOTH
  arms explicitly via `prefer()` — so the epoll arm itself is fully exercised; only
  the automatic *transition* to it is not. The reference has the same hole (its own
  `test "prefer"` assumes `prefer(bes[0])` succeeds, dynamic.zig:218-227). To close:
  run the suite on a kernel with `io_uring_disabled=2`.

**KNOWN CROSS-TARGET SURFACE DIVERGENCES (pinned by `tests/surface_parity.rs`, not
hidden — both are consequences of R5 forbidding edits to macOS-gated code):**

1. **`Loop::init` arity.** The reference takes `Options` on BOTH sides
   (`kqueue.zig`'s `Loop.init(options: Options)`, `dynamic.zig:127`'s
   `Loop.init(opts: Options)`). `kqueue.rs:345-351` specialized the macOS form to
   NO-ARG (ghostty's macOS build only ever calls `Loop.init(.{})`, and the
   `Options.thread_pool` path is deferred with the ThreadPool body). The Linux side
   is NOT specialized — it is faithful to dynamic.zig:127, and `Options::entries` is
   load-bearing there (io_uring-only, loop.zig:15; it is what
   `dynamic_loop_init_dispatches_on_the_global` discriminates the two arms with).
   `surface_parity.rs` asserts the signature each target actually has, so neither can
   drift silently. **CONSEQUENCE FOR LT9:** termio calls `xev::Loop::init()` with no
   args (thread.rs:160, exec.rs:4563); on Linux it must pass
   `xev::Options::default()`. LT9 must either cfg that call site or (better) revisit
   the macOS specialization when R5 is liftable.
2. **`Completion::state` vs `state_of`.** The reference names the decl `state` on
   every backend (kqueue.zig:1035 / epoll.zig:941 / io_uring.zig:649) and on the
   Dynamic completion (dynamic.zig:270). `kqueue.rs` could not use that name for its
   public `&self` accessor — its raw-pointer discipline already owns `state` for the
   `fn state(c: *mut Completion)` associated fn — so macOS exports `state_of`
   (kqueue.rs:1648-1659), and termio's completions already call it (exec.rs:799,
   thread.rs:1022/1173). A `pub use` can rename a type but not a method, so LT8 gives
   `dynamic::Completion` a `state_of` INLINE ALIAS of its faithful `state`: both names
   resolve on Linux, `state_of` resolves on both targets, and `surface_parity.rs` pins
   it. This is the only method added beyond the reference's decl list.

## rustty-renderer GPU R-d P1 (Chunk-3 prereqs) — `image.zig` + `Overlay.zig`

Ports `src/renderer/image.zig` → `crates/rustty-renderer/src/image.rs` and
`src/renderer/Overlay.zig` → `crates/rustty-renderer/src/overlay.rs`
(both `#[cfg(target_os = "macos")]`, per `GPU_RD_P1_DESIGN.md`). New crate edge
`rustty-renderer → rustty-z2d` (pure-Rust sink; acyclic — G7 GREEN, no new FFI).

BOTH reference files carry **0** `test{}` blocks (confirmed by grep) — this tier
is **golden-image-gated** (the Chunk-4 offscreen `drawFrame` is the binding gate).
The rustty-side smoke `#[test]`s are NOT ghostty parity tests: `image_empty_lifecycle`
(FULL container path over the empty map + `None` overlay — exercised for real when a
Metal device is present), `id_z_less_than_ordering`, `pixel_format_bpp`,
`overlay_surface_roundtrip` (pure z2d), `overlay_invalid_dimensions`, `color_alpha_pixels`.

~~DEFERRED~~ **ALL THREE P1 SEAM GROUPS RESOLVED (reconciled 2026-07-10, T14 —
they landed after this entry was written and it was never marked):**

- ~~**`Image::convert`** (image.rs)~~ **RESOLVED** — filled as a pure-Rust
  per-pixel g/ga/rgb/bgr/bgra→RGBA_PREMUL swizzle loop (the "wuffs has no Rust
  home" blocker was resolved by re-homing the conversion, NOT by filling
  `rustty-wuffs` — whose own `swizzle.rs` seams remain open, see the
  "rustty-wuffs swizzle" entry below). `Image::upload`/`prep_for_upload` now run
  for real; the `bg_image` path (`prep_background_image` →
  `prep_for_upload` → `convert`) is live. See image.rs header
  ("FILLED (R-d)") + `RENDERER_KITTY_IMAGE_PATH_DESIGN.md`.
- ~~**`State::{kitty_update, prep_kitty_placement, prep_kitty_virtual_placement,
  prep_kitty_image}`** (image.rs)~~ **RESOLVED** — the Kitty terminal-walk is
  live (image.rs:466/713/…), exercised by the offscreen APC-driven Kitty
  pixel-path test.
- ~~**`Overlay::{highlight_hyperlinks, highlight_semantic_prompts,
  highlight_grid_rect, highlight_pixel_rect}`** (overlay.rs)~~ **RESOLVED** —
  all three former forcings were lifted: the z2d `Context` exposes
  `anti_aliasing_mode`/`hairline` (public fields) + the hairline stroking
  fast-path is ported, and the `RenderState` row/cell metadata is public. The
  four draws render for real (overlay.rs header "now FILLED"), covered by
  host-only pixel tests.

FULL (ported, compiles against the landed metal leaves + z2d, no `GenericRenderer`):
`State::{empty, deinit, upload, draw, overlay_update, kitty_requires_update,
prep_image}`, all of `Image`'s state machine incl. `upload`/`prep_for_upload`
(FULL wiring above the `convert` seam), and Overlay's `Color`/`init`/`deinit`/
`pending_image`/`reset`/`apply_features`. Seam count = **9** (1 `convert` + 4 Kitty +
4 overlay render; the design prose's "6" is a doc miscount — the enumerated method
list, ported here, governs).

PORT-NOTE divergences from the design (both R1-driven, documented in-source):
- `State::overlay_update` takes `Option<&Overlay>` (NOT the design's by-value
  `Option<Overlay>`): Zig passes a shallow `?Overlay` copy that SHARES the caller's
  z2d buffer and never deinits it; a Rust by-value move would drop/free that buffer.
  Chunk-3 `rebuildOverlay` passes `self.overlay.as_ref()`.
- `State::draw` is `&self` (no mutation; the Zig `*State` is non-mutating).
- `Overlay::init`/`deinit` drop the Zig `alloc` (the z2d subset owns its buffer).

## macOS windowing W0 (objc2-quartz-core + cf_release_thread xev rewire + IOSurfaceLayer) — no open deferral [added 2026-07-10, T14 reconciliation]

Chunk W0 (commit 540c924, `W0_DESIGN.md`): the `objc2-quartz-core` dep
(`CALayer`+`objc2-core-foundation`+`CAMetalLayer` features), the
`os/cf_release_thread` rewire onto the REAL `rustty-xev` (retiring its
`xev_seam` placeholder markers), and the `IOSurfaceLayer` CALayer-subclass port
(`crates/rustty-renderer/src/metal/iosurface_layer.rs` — `set_surface`
main-thread hop, `display` override, `kCAGravityTopLeft` contents wiring).
**No `unimplemented!()` seam was introduced or left by W0.** Divergence ledger
lives in `W0_DESIGN.md` §0 (objc2 feature-gate corrections, `Send`-wrapper
`SetSurfaceBlock` analogue). The `IOSurfaceLayer` unsynchronized-`Cell` ivar
note is tracked by VALIDATION_REPORT §3c (AtomicPtr fix landed in 9f87df1).

## macOS windowing W1 (Metal on-screen present + windowed init — FIRST PIXELS) — residual folded into W-ATTACH [added 2026-07-10, T14 reconciliation]

Chunk W1 (commit 74c5720, `W1_DESIGN.md`): `Metal::init(view)` (the on-screen
attach ctor — sends `setLayer:`/`setWantsLayer:` to the type-erased host view,
R4: `&AnyObject`, no apprt edge), the sync + async (`Frame::complete`) present
halves, `surfaceSize`, and the windowed `Renderer` init plumbing — proven by
the on-device first-pixels KAT. **No `unimplemented!()` seam remains in
`rustty-renderer` from W1.** The W1-7 divergence (generic `Renderer::init`
constructs Metal via the offscreen-portable `init_offscreen`, generic.rs:1840)
became the CORE-side host-attach gap once core's `renderer_seam::Renderer::init`
landed against it — that residual is the **W-ATTACH** entry below (Task 23),
NOT a W1 in-crate seam.

## macOS windowing W2 (CVDisplayLink) — divergence ledger + one deferred leaf

W2 lands the CoreVideo `CVDisplayLink` primitive (`crates/rustty-renderer/src/metal/
cvdisplaylink.rs`) + the `generic.rs` display-link block (field wiring, `init` create
+ errdefer, `deinit` teardown un-defer, `loop_enter`/`loop_exit`/`thread_enter`/
`thread_exit`/`has_vsync`/`set_macos_display_id`/`set_focus`/`set_visible`) + the vsync
KAT. All apprt-free (R4). Build/clippy -D/fmt/acyclic/comment-coverage green; the
on-device KAT passes (real vsync tick, TICKS>=1). Divergences (all documented in-source):

- **`loop_enter(&mut self, draw_now: xev::Async)` vs reference `loopEnter(self,
  thr: *renderer.Thread)` [R4].** The reference passes `*Thread` SOLELY to reach
  `&thr.draw_now`; R4 forbids `rustty-renderer` naming `renderer.Thread`, so the port
  takes that exact field — a `Copy`+`Send` leaf-crate handle (NOT an apprt/core type).
  Behavior identical. `rustty-xev` is a leaf (deps `rustty-global`+`libc`) ⇒ the
  renderer→xev edge is acyclic.
- **`DisplayLink.draw_now: Option<Box<Async>>` wrapper field vs reference bare
  `opaque` [R4, soundness].** The CoreVideo output callback needs a stable `void*`
  userInfo; the reference is `&thr.draw_now` in the separate `renderer.Thread`. The
  port boxes the `Async` in the wrapper — a SEPARATE heap allocation whose address is
  stable and lies OUTSIDE the `Renderer` `&mut` footprint (the faithful analogue of the
  reference's separate `Thread` home). Not merely a field add: the `Box` is what makes
  the callback-aliasing shape sound under ASan (separate alloc + `Copy`-by-value read,
  never `&mut`-reborrowed). No `Drop` on `DisplayLink` — release is manual (deinit /
  errdefer), faithful to the Zig `opaque`.
- **`create` error `generic::Error` (via `From<cvdisplaylink::Error>`,
  `InvalidOperation → OutOfMemory`) vs reference `Allocator.Error` [error-channel].**
  The renderer init error channel is `generic::Error`, not an allocator error; the
  `From` impl folds the sole CVDisplayLink error into `OutOfMemory`, keeping the
  reference `try … catch error.OutOfMemory` faithful.
- **`pub use async_watcher::WaitError;` added to `rustty-xev/src/lib.rs`
  [export-gap fix].** Same class as the existing `TimerRunError` re-export — the
  `WaitError` alias of the already-exported `MachPortError` was un-exported; the KAT's
  typed `Async.wait` callback names it. Not a behavioral divergence.

**Deferred leaf — RESOLVED (by W5, not W3; reconciled 2026-07-10, T14):**
`Renderer::loop_enter` was filled and faithful at W2, with its transitive
`self.api.loop_enter()` (metal_impl.rs) left `unimplemented!("windowing-host: …")`.
That Metal hook was FILLED by chunk **W5** (commit 9f16b9d — the display-callback
trampoline; `metal_impl.rs:493` is live, concurrency Miri-proven), so the old
"PANICS until W3" wording here was stale twice over (wrong chunk, then wrong
state). Nothing in this entry's scope remains deferred. The KAT never needed the
Metal leaf — it drives the `DisplayLink` primitive + the callback→`Async` seam
directly. `loop_exit` has no Metal hook (`@hasDecl(Metal,"loopExit")` false) and
was always fully runnable.

## macOS windowing W3 (E2 renderer-Thread event loop) — one deferred leaf

Chunk W3 fills the core-resident `renderer_seam::Thread<A>` render event loop
(`init`/`deinit`/`threadMain`/`threadMain_` + every callback + `drainMailbox`/
`syncDrawTimer`/`setQosClass`/`drawFrame`/`changeConfig`/`cursorBlinkInterval`)
over rustty-xev, driving the REAL `rustty_renderer::Renderer`. Everything is
faithfully ported; ONE deferred leaf:

**`drainMailbox` `@autoreleasepool` (Thread.zig:338–346) — deferred no-op leaf.**
The reference wraps the mailbox drain in a Darwin `@autoreleasepool{}` block
(`const pool = if (isDarwin) @import("objc").AutoreleasePool.init() else void;
defer if (isDarwin) pool.deinit();`) — memory hygiene, NOT semantics. rustty-core
does NOT dep objc2 (only rustty-renderer/os do), so adding an objc2 edge to core
solely for the pool is heavier than the leaf. Ported as a no-op scope carrying
the verbatim reference block as a comment (comment-coverage anchors on it). The
drain itself + every message arm is REAL. Re-arm the pool when core gains an
objc2 edge (or via a `rustty_os` autoreleasepool shim). This is the same
class as the io-thread's deferred autoreleasepool.

**RESOLVED (WS2-C, 2026-07-23):** realized exactly via the named re-arm path — the
`rustty_os::autorelease::autoreleasepool` shim (objc2-backed on macOS, identity
elsewhere; core still has no objc2 edge). `drainMailbox` is now the pool wrapper
around the hoisted `drainMailboxBody`; drain semantics unchanged (rustty-core +
macos_window_host suites green). Pool-drain property pinned by
`rustty-os/src/autorelease.rs` `pool_drains_autoreleased_references`.

No other W3 E2 deferrals: `cursorBlinkInterval`'s valgrind `* 5` branch is kept
verbatim-inert (no `std.valgrind` shim, in-place comment + PORT NOTE, not a
DEFERRALS entry); the `MUST_DRAW_FROM_APP_THREAD` app-mailbox `drawFrame` arm is
dead-but-live (`false` this milestone, flips when GTK lands) — both arms compile,
no `unimplemented!`.

## macOS windowing W4 (apprt-c host handshake: blur + display_id) — no open deferral [added 2026-07-10, T14 reconciliation]

Chunk W4 (commit 73acefa, `W4_DESIGN.md`): filled the two remaining macOS
`#[cfg(target_os="macos")]` C-ABI leaves in `crates/rustty-apprt-c/src/lib.rs` —
`rustty_surface_set_window_background_blur` (private-CGS, explicit
`#[link(name = "CoreGraphics", kind = "framework")]`) and
`rustty_surface_set_display_id` (renderer mailbox → `set_macos_display_id`) —
plus the config blur-radius `pub` accessor and the NSWindow layer-host
integration harness. **Both former E10 leaves are LIVE; W4 left no new seam.**
The `quicklook`/`secure-input`/`inspector` apprt-c leaves were explicitly OUT of
W4 scope and stay tracked by the E9/E10 sections above (Phase-3).
`set_display_id_pushes_to_render_thread_panic_free` is un-ignored and green on a
Metal host (see W-ATTACH below).

## macOS windowing W5 (Metal::loop_enter display-callback trampoline) — no open deferral [added 2026-07-10, T14 reconciliation]

Chunk W5 (commit 9f16b9d, `W5_DESIGN.md`): filled the LAST live-windowing seam
inside `rustty-renderer` — `Metal::loop_enter` (`metal_impl.rs:493`, live)
installs the `IOSurfaceLayer` display callback via the raw-self draw chain
(container_of / `@fieldParentPtr` idiom, aliasing proven on
`.port/phase-2/mirihar-chunk3b`). **Zero `unimplemented!()` windowing seams
remain in `rustty-renderer`** (the crate's single live seam is the inspector
`overlayFeatures` arm — see the R-d reconciliation above). The remaining
windowed-present gap is CORE-side host-attach wiring, tracked at **W-ATTACH**
(below, Task 23). Stale references to "Metal::loop_enter unimplemented" in
older entries were reconciled 2026-07-10 (the W2 entry above; VALIDATION_REPORT
M4).

---

# Ledger-hygiene reconciliation (2026-07 — GAP_ANALYSIS drift backlog §6)

The gap-analysis completeness critics found several real deferrals with NO ledger
entry, plus one app-core over-claim. These entries reconcile that drift. Each
maps to a GAP_ANALYSIS §3 workstream ("sub-project"); the numbering below is the
sub-project id the critics used (macos-swift-app = #2, unused-subset = #3, gtk = #4).

## Sub-project #3 (unused-subset): `src/synthetic/**` — RESOLVED (ported 2026-07-09); 4 binary-entry seams tracked [reconciled 2026-07-10, T14]

~~**Rustty counterpart:** only a 9-line scaffold (crate exists, body unported).~~
**RESOLVED — the body was PORTED the day this entry was written (commit
fb12303, `SYNTHETIC_DESIGN.md`)**: all of ghostty `src/synthetic/**` (9 `.zig`
files, ~1157 LOC) is ported module-per-file into `crates/rustty-synthetic`
(`generator`/`bytes`/`osc`/`utf8`/`cli` + `cli/{ascii,osc,utf8}` + the package
root, plus 3 net-new support modules `rand`/`enums`/`io`), PRNG bit-exact
(Xoshiro256++ / SplitMix64 / the std.Random 0.15.2 helper chain, verbatim), 12
Zig `test{}` ported assertion-for-assertion (15 Rust tests), Miri-clean, G3b
missing=0, `PORT STATUS: done`.

**Residual (4 live `unimplemented!()` — the BINARY-ENTRY seams,
`crates/rustty-synthetic/src/cli.rs`, previously tracked only in
`SYNTHETIC_DESIGN.md`; each keeps its verbatim Zig body as comments):**

1. **`cli.rs:56` `main()`** — needs `cli.action.detectArgs` (generic `+action`
   detection), which lives ABOVE `synthetic` in the acyclic Rust split (a
   synthetic→cli edge would cycle). No ported `test{}` exercises it.
2. **`cli.rs:85/88/91` `main_action_impl_{ascii,osc,utf8}`** — the
   option-parsing (`cli.args.parse` over argv/string iterators) + stdout wiring
   of `mainActionImpl`. The substantive dispatch (`main_action`) and every
   generator body are LIVE; only the binary-entry glue defers.

**Un-defer trigger:** the `rustty +synthetic`-style binary entry (the same
milestone that wires generator binaries into the differential-fuzz harness, G5).
Placeholder-leaf doctrine: shapes fixed, bodies panic if called, no test reaches
them.

## Sub-project #3 (unused-subset): `src/benchmark/**` + `main_bench.zig` + `ghostty_benchmark_cli`

**Ghostty source:** `src/benchmark/**` (12 files: `Benchmark.zig`, `CApi.zig`,
`CodepointWidth.zig`, `GraphemeBreak.zig`, `IsSymbol.zig`, `OscParser.zig`,
`ScreenClone.zig`, `TerminalParser.zig`, `TerminalStream.zig`, `cli.zig`,
`main.zig`, `options.zig`) + `src/main_bench.zig`, together ~1441 LOC; plus the
`export fn ghostty_benchmark_cli` C-ABI entrypoint (`src/benchmark/CApi.zig:8`).
**Rustty counterpart:** `crates/rustty-bench` — **PORTED (PORT STATUS: done)**: all
12 `src/benchmark/*.zig` files ported module-per-file, 8 `test{}` assertion-for-assertion
(ScreenClone has none; `main.zig`'s anonymous `refAllDecls` was initially dropped as
"a Zig reflection idiom" — RESOLVED 2026-07-10: ported as `tests::ref_all_decls` in
`crates/rustty-bench/src/lib.rs` per the workspace convention, bench 8/8 vs contract),
Miri-clean, `rustty_benchmark_cli` extern "C" export landed. **Residual sub-seams
(narrow, tracked):** (1) the macOS `os_signpost` Instruments interval (`Benchmark.zig`
44-65) — `#[cfg(target_os="macos")]` no-op with the verbatim Zig body preserved,
awaiting the macOS binding tier; (2) the `ghostty_benchmark_cli` C-name alias —
sub-project #2's `ghostty_*→rustty_*` alias layer (rustty exports `rustty_benchmark_cli`
per R2); (3) `src/main_bench.zig` — the thin binary entry (re-export of the bench CLI
main), a Phase-2 CLI-bin seam; (4) `cli.main`'s `detectArgs` replicated locally (the
shared helper lives in the heavy `rustty-cli`; `main()` is only reached by the deferred
`main_bench.zig` bin). 0-test regression.

## Sub-project #3 (unused-subset): `src/extra/{bash,fish,zsh,vim,sublime}.zig` — shell-completion / editor-syntax generators

**Ghostty source:** `src/extra/bash.zig`, `fish.zig`, `zsh.zig`, `vim.zig`,
`sublime.zig` — ~941 LOC of shell-completion + editor-syntax string constants,
emitted by `main_build_data.zig`'s `+gen-*` actions (the `bash`/`fish`/`zsh`/
`sublime`/`@"vim-syntax"`/`@"vim-ftdetect"`/`@"vim-ftplugin"`/`@"vim-compiler"`
gen switch). **Rustty counterpart:** **PORTED** — new crate `crates/rustty-extra`
(PORT STATUS: done, R4-acyclic leaf over rustty-config + rustty-cli): all 5
generators ported line-for-line as pure `-> Vec<u8>` fns (never lossy String),
proven **byte-exact against ghostty's real `zig build` output** (golden fixtures in
`testdata/`) for bash/zsh/vim(×4)/sublime; fish is byte-exact except the R2
`Ghostty→rustty` brand rename inside config-doc descriptions (correct under R2,
gated by a brand-canonicalizing compare). Both `test{}` (fish `getDescription`,
vim anon) ported + additive golden tests; Miri-clean. Required reflection surfaces
added faithfully next to the reified types (config_struct `config_fields()` +
`config_completion_kind()` parallel to `value_type`; `VALUE_NAMES` on the enum
macro; `PackedStruct::field_names()` mirroring `AutoStruct`; rustty-cli
`Action::option_specs()` reifying the commented-out `options()`). **Residual
deferral:** the `+gen-*` CLI/build.rs file-emit wiring (writes the scripts to disk
at packaging time) stays deferred with `main_build_data` (C6). 2 `test{}` ported.

## `src/build/mdgen/**` + `src/build/webgen/**` — man-page + website doc generators (low priority)

**Ghostty source:** `src/build/mdgen/**` (`mdgen.zig`, `main_ghostty_1.zig`,
`main_ghostty_5.zig`, the `ghostty_{1,5}_{header,footer}.md` fixtures) +
`src/build/webgen/**` (`main_actions.zig`, `main_commands.zig`,
`main_config.zig`) — ~345 LOC of `.zig` (plus the `.md` header/footer fixtures).
**Rustty counterpart:** NONE. **Why deferred:** build-time documentation
generators (man pages + website config/action/command reference) — pure packaging
tooling, never linked into the terminal, and dependent on the same `help_strings`
codegen the CLI already defers (C6). **What unblocks it:** LOW priority; port with
the doc/codegen tooling milestone if/when man-page + website generation is wanted.
0-test impact.

## `src/stb/stb_image_resize.h` — glyph resize (`stbir_resize_uint8`); rustty-font does NOT yet cover it

**Ghostty source:** `src/stb/stb_image_resize.h` (single-header stb library),
invoked as `stb.stbir_resize_uint8(...)` in the FreeType glyph-resize path
(`src/font/face/freetype.zig:640`, "resizes the glyph"). **Rustty counterpart:**
NOT covered — `grep -rn 'stbir|stb_image_resize' crates/rustty-font/src` finds
only `Vec::resize`/`atlas.resized` (unrelated). The glyph/atlas resize seam has no
Rust analogue yet. **Why deferred:** the live FreeType glyph-rasterization path
(which is itself windowing/font-face-tier and gated on the Phase-3 renderer) is
where this resize is called; the offscreen port did not need it. **What unblocks
it:** when the live glyph-rasterization path lands, wire glyph resize — either a
pure-Rust resize (e.g. an image-resize crate) or a `stb-sys` cc build compiling
`stb_image_resize.h` (a `cc` build.rs, the -sys precedent). Note a **stb-sys cc
build may be needed** if bit-exact parity with stb's resampler is required.

## Sub-project #2 (macos-swift-app): the `ghostty_*`→`rustty_*` C-ABI compatibility ALIAS layer

**Context:** the reused macOS Swift app hard-codes ~163 `ghostty_`-prefixed
symbols, but the rustty C-ABI (rustty-apprt-c, the E-series) exports `rustty_`-
prefixed symbols (R2 identity rename). **What's deferred:** a compatibility ALIAS
layer that re-exports each `rustty_*` C export under its original `ghostty_*` name
(so the unmodified Swift app links against `librustty_apprt_c`). No rustty crate
provides this yet. **Locked decisions (per the critics):** identity is
`io.airely.rustty`; **iOS is in scope** (the alias layer + xcframework must cover
the iOS targets too). **Why deferred:** only needed when the Swift frontend is
actually reused (Phase-3, after R-d windowed present + D6 land). **What unblocks
it:** sub-project #2 (macos-swift-app, GAP_ANALYSIS §3.3) — generate the ~163
`ghostty_*` = `rustty_*` alias exports (comptime/attribute export aliases) as part
of the Swift-app reuse milestone.

## Sub-project #2 (macos-swift-app): signed `.xcframework` repackaging xtask (`GhosttyKit.xcframework`)

**Context:** the Xcode project consumes a `GhosttyKit.xcframework`; the rustty
build produces `librustty_apprt_c` (the apprt-c staticlib/dylib). **What's
deferred:** an `xtask` (build-orchestration) subcommand that repackages
`librustty_apprt_c` (+ the generated C header + the `ghostty_*` alias layer above)
into a **signed** `GhosttyKit.xcframework` consumable by the Xcode project (macOS +
iOS slices). No such xtask exists yet (the current `xtask` only does
`header-gen-apprt` cbindgen, see E11/E-series). **Why deferred:** only needed for
the Swift-app reuse build. **What unblocks it:** sub-project #2 (macos-swift-app,
GAP_ANALYSIS §3.3) — add the `xtask xcframework` packaging + codesign step
alongside the alias layer.

## Sub-project #4 (gtk): the `apprt-gtk` subsystem (`src/apprt/gtk/**`)

**Ghostty source:** `src/apprt/gtk/**` — ~52 `.zig` files, ~22k LOC (`App.zig`,
`Surface.zig`, the `class/**` GObject widgets, `ext/**`, `cgroup.zig`,
`adw_version.zig`, …) plus the `.blp` Blueprint UI + `.css` theme assets. **Rustty
counterpart:** NONE (zero counterpart — the single largest unported chunk).
**Why deferred:** the GTK4/libadwaita Linux frontend is a whole windowing/toolkit
subsystem that also drives the OpenGL renderer backend; it is out of the
monomorphic Metal/macOS scope of the current milestones (the renderer's
OpenGL/WebGL backends + GTK were already noted out-of-scope at the R-d monomorphic
Renderer boundary). No unit-gate is expected for the GTK tier (per the
heavy-tier-strategy: GTK is ported LAST and accepts no unit-gate). **What unblocks
it:** sub-project #4 (gtk, GAP_ANALYSIS §3.7) — the GTK/Linux frontend milestone,
scheduled after the macOS windowed path is runnable. This is the dedicated ledger
entry the drift-analysis found missing (prior GTK mentions were only in-passing
seam notes: the `RtPreExecInfo`/`RtPostForkInfo` growth point, the C6 usage-banner
`.gtk` guard, and the `MUST_DRAW_FROM_APP_THREAD` flip — none of which tracked the
subsystem itself).

## L5 GTK baseline pin (AD-3) — `v4_14` API ceiling costs `prefers-color-scheme` [added 2026-07-17, L5.0 T1 review]

**Ghostty source:** `src/apprt/gtk/class/application.zig:1565-1583` (inside the
style-manager color-scheme handler). **Rustty counterpart:**
`crates/rustty-apprt/src/gtk/class/application.rs` `handle_style_manager_dark`
(`gtk` feature). **Status:** RESOLVED via a ghostty-mirrored OPT-IN feature (see
the UPDATE below) — the DEFAULT `v4_14` build still omits the push, so the
original deferral analysis remains the authority for that build.

**UPDATE [2026-07-19, L5 T3 — ghostty-mirrored opt-in].** The push is now PORTED
behind a `gtk-v4_20` Cargo feature (default OFF), which faithfully mirrors
ghostty's own comptime+runtime gate rather than staying wholesale-deferred:
* `#[cfg(feature = "gtk-v4_20")]` is the **comptime analog** of ghostty building
  against a >= 4.20 header — it raises the gtk4-rs API CEILING to `v4_20` so
  `gtk4::InterfaceColorScheme` exists. OFF (the default) → the branch is absent,
  exactly like ghostty compiled on the 4.14 floor (its `comptime` fold in
  `gtk_version.atLeast` collapses the branch away).
* the live `gtk_version::at_least(4, 20, 0)` runtime guard is preserved INSIDE
  that cfg — the **runtime analog** — so even the opt-in build self-disables on a
  < 4.20 shared library. (This port has no `comptime_version` header constant, so
  the runtime check is never folded out, unlike ghostty.)
The AD-3 floor is UNCHANGED: `gtk4` stays pinned to `v4_14` by default, the
minimum system gtk is still 4.14, and the stock `--features gtk` build carries no
`v4_20` symbol. `--features gtk,gtk-v4_20` builds green on the 4.22 dev box and
lights the push. The un-defer trigger below is thus partially discharged (path 1
is now available as a feature flip); the DEFAULT-build loss it describes still
holds verbatim for anyone who does not opt in.

**What is deferred.** The reference runs:

```zig
if (gtk_version.atLeast(4, 20, 0)) {
    const gtk_scheme: gtk.InterfaceColorScheme = switch (scheme) { .light => …, .dark => … };
    var value = gobject.ext.Value.newFrom(gtk_scheme);
    gobject.Object.setProperty(priv.css_provider.as(gobject.Object), "prefers-color-scheme", &value);
    for (priv.custom_css_providers.items) |css_provider| {
        gobject.Object.setProperty(css_provider.as(gobject.Object), "prefers-color-scheme", &value);
    }
}
```

i.e. it propagates the resolved light/dark scheme onto the app's own
`css_provider` **and** every entry of `custom_css_providers`, so that GTK 4.20+
CSS `@media (prefers-color-scheme: …)` queries resolve correctly in user CSS.

**Why it is deferred (this is a REAL loss, not a no-op).** `gtk_version.atLeast`
folds on the **build headers**, not on the call site's context: it opens with
`if (comptime comptime_version.order(threshold) == .lt) return false;`
(`gtk_version.zig:58-62`) — a compile-time-constant condition inside an `inline
fn` with `comptime` params, so it collapses at *every* call site, runtime ones
included (`gtk_version.zig:47-48`: "This is inlined so that the comptime checks
will disable the runtime checks if the comptime checks fail"). What matters is
`comptime_version`, which is read from the build box's headers
(`gtk_version.zig:7,14-18`, `@cInclude("gtk/gtk.h")`). The L5 dev/target host is
**gtk 4.22.4**, so `atLeast(4, 20, 0)` does *not* fold there and upstream ghostty
genuinely executes this branch on this host. It is therefore *not* covered by
AD-3's "comptime branches collapse" argument — but note that the reason is the
4.22 headers, **not** the fact that the site is spelled as a plain `if` (see the
sibling note on `application.zig:1261` below, which is spelled `if (comptime …)`
and is nonetheless the same case).

**What blocks it.** AD-3 pins the compat floor at gtk >= 4.14, expressed as
gtk4-rs's `features = ["v4_14"]`. gtk4-rs gates the required type behind a
strictly higher feature: `InterfaceColorScheme` is `#[cfg(feature = "v4_20")]`
(gtk4-0.11.4 `src/auto/enums.rs:5985`, re-export `src/auto/mod.rs:1262`). At
`v4_14` the type does not exist, so the branch cannot be written at all — the
ceiling, not the floor, is what bites. A pure runtime predicate (which is what
T2 ports) cannot recover it; only raising the Cargo feature can.

**Decision (ratified here rather than left silent).** Keep `v4_14`. The pin is a
*compatibility floor* — what rustty is allowed to assume of a user's system —
and bumping it to `v4_20`/`v4_22` to match this dev box would raise rustty's
minimum system gtk to 4.20 and break the 4.14 baseline AD-3 exists to hold. The
cost is bounded and cosmetic: user/custom CSS `@media (prefers-color-scheme: …)`
blocks will not see the app-resolved scheme on gtk >= 4.20 runtimes. Everything
else in the handler (the `adw.StyleManager` dark/light resolution and the theme
application itself) is unaffected and IS ported. **Scope: exactly one branch;
zero tests affected** (the reference has no test over this site).

**Un-defer trigger (either, whichever comes first):** (1) the AD-3 floor is
formally raised to gtk >= 4.20 — then set `gtk4 = { features = ["v4_20"] }` and
port the branch behind the existing runtime `at_least(4, 20, 0)` predicate, which
is the exact shape the reference already uses; or (2) gtk4-rs's optional-feature
story allows naming `v4_20` types under a lower floor. Until then this row is the
authority: an L5 verify that flags `application.zig:1565` as G3b "missing" must
treat it as **EXPECTED**, per §Rule.

**Sibling site, same ceiling — `application.zig:1261` [corrected 2026-07-17,
L5.0 T1 re-review].** `if (comptime gtk_version.atLeast(4, 16, 0))` guards a
richer CSS-parse-error log line that calls `css_section.getBytes()`. On this
host's 4.22 headers the fold does not fire and the `comptime` arm resolves
**true**, so upstream ghostty takes the *modern* arm here — exactly as at
`:1565`. rustty cannot: `CssSection::bytes()` is `#[cfg(feature = "v4_16")]`
(gtk4-0.11.4 `src/auto/css_section.rs:57`), so at `v4_14` the arm does not
compile and T2 must port the plain `log.warn` arm. **This is the same ceiling
loss as the row above, and is ratified on the same terms and the same un-defer
trigger.** Cost: CSS-parse-error logs omit the offending source bytes (the
location string, level, and message are all still logged). Zero tests affected.
Recording it here dissolves an earlier asymmetry in which `:1261` was treated as
"comptime-forced, therefore free" while `:1565` was treated as "live, therefore a
loss" — the `comptime` vs plain `if` spelling was never the discriminator; the
`v4_14` **ceiling** is, and it bites both sites identically.

**Related note for L5.0 T2 (not a deferral — a correctness warning):** the other
two `gtk_version.atLeast` sites must NOT be deleted as "comptime branches".
`window.zig:695` (`!gtk_version.atLeast(4, 16, 0)` gating the
`window-theme-ghostty` CSS class) and `window.zig:1181` (`if (!gtk_version
.atLeast(4, 10, 0)) return;`) need no post-4.14 symbol and are not compile-time
artifacts, so both port live onto `gtk4::{major,minor,micro}_version()`.
`:1181`'s threshold (4.10) sits *below* the AD-3 floor, so the guard is true on
every supported runtime and could in principle be dropped — do NOT drop it: AD-3's
deletion set is enumerated (the five `adw` `supports_*` helpers) and `:1181` is
not in it. Porting it live is oracle-exact and a no-op on any supported host.

**AD-3 scope note — `window.zig:1799` [added 2026-07-17, L5.0 T1 re-review].**
`adw_version.supportsDialogs()` (= `atLeast(1, 5, 0)`, `adw_version.zig:104-106`)
at `window.zig:1799` is a plain runtime `if` around two `showAboutDialog` arms; it
is neither ceiling-blocked (`adw::AboutDialog` is `#[cfg(feature = "v1_5")]`,
libadwaita-0.9.2 `src/auto/mod.rs:6-11` — available exactly AT the floor; probe-
verified) nor a compile-time artifact, so on the rule alone it would stay live.
AD-3 deletes it
anyway: plan:101 names `supports_dialogs()` among the five deleted helpers and
directs inlining the modern arm at each of their **6** call sites, and this is one
of them (it is also the only `adw_version.` call site in `window.zig`; plan:101's
"tab-overview/toolbar-view" label is a mislabel — `supportsTabOverview`/
`supportsToolbarView` have zero call sites repo-wide, and plan:101's list further
cites `clipboard_confirmation_dialog.zig:302-312`, which is an `atLeast(1, 4, 0)`
site rather than a `supports_*` one, while omitting the real sixth site
`surface_child_exited.zig:15`). **T2/T20 port it as ratified** — the `adw` arm
unconditionally, dropping the `gtk.showAboutDialog` legacy arm
(`window.zig:1817-1830`). (Shape note for T20: both `showAboutDialog`s are C
varargs with no gtk-rs free-fn binding; the modern arm ports as an
`adw::AboutDialog` built with the same properties and `present()`ed.) This is
behavior-preserving inside the supported
envelope (`runtime_at_least(1, 5, 0)` is true on every runtime AD-3 admits) and
observable only on an adw < 1.5 runtime, which AD-3 declares unsupported. It is
recorded here as an explicit, flagged deviation from ghostty-on-this-host rather
than left as a doc comment asserting compliance. **Open for the plan owner:** if
parity on sub-floor runtimes is wanted at this site, amend AD-3 to exclude
`window.zig:1799` from the deletion set; T1 did not take that authority.

**Open for the plan owner — plan:96's `at_least` shape.** plan:96 specs
`at_least(..)` as a "const fn over `BASELINE`". rustty has no header fold
(`gtk4::minor_version()` always reads the real shared library; the Cargo features
are an API-surface ceiling, not a version oracle), so a `const fn` over
`BASELINE = (4, 14, 0)` would manufacture one and const-**false**
`window.zig:695` — the very site this row ratifies as live. Under the rule,
`at_least` degenerates to `runtime_at_least` (the oracle distinguishes them only
*because* it has the header fold). T2 must implement it as a live delegate or get
plan:96 amended; T1 flags, does not decide.

**RESOLVED by L5.0 T2 [2026-07-17] — implemented as a live delegate; plan:96 is
now stale and wants amending.** T2 took the first of the two options T1 left,
because it is the only one inside a task's authority: amending plan:96 is the
plan owner's call. `gtk_version::at_least` / `adw_version::at_least` are
`#[inline]` delegates to their `runtime_at_least` twin, keeping the oracle's
`atLeast` call sites portable 1:1 while every threshold stays live. Two
reinforcing findings from the implementation:

1. A const-`BASELINE` `at_least` would have **no legitimate call site**. The two
   sites where the `v4_14` ceiling genuinely forces the legacy arm
   (`application.zig:1261`, `:1565`) cannot name their modern arm's symbols at
   all, so they port as the legacy arm *unconditionally, with no predicate* —
   they never call `at_least`. The remaining sites (`window.zig:695`, `:1181`)
   are exactly the ones the const fold would silently break. So the const shape
   is not merely riskier; it is unused-and-wrong rather than a live trade-off.
2. `BASELINE` survives as a real constant, but as *documentation + logging +
   tripwire*, not as a predicate input: it is the honest analogue of the
   oracle's `comptime_version` (the API surface the `v4_14`/`v1_5` Cargo
   features select) and feeds `log_version`'s `build=` field — precisely the
   oracle's own use of `comptime_version` at `gtk_version.zig:29-32`. plan:96's
   `const _: () = assert!(BASELINE >= (4, 14, 0))` tripwire IS implemented (a
   `BASELINE`-vs-`AD3_FLOOR` const assert in each module; verified to fail the
   build on a downgrade).

**Action for the plan owner:** amend plan:96's "`at_least(..) -> bool (const fn
over BASELINE)`" to "live delegate to `runtime_at_least`", or reverse T2. Left
as a flagged deviation rather than a silent one.

## ~~GlobalShortcuts XDG-portal round-trip (L6 portals)~~ [added 2026-07-18, T26 GlobalShortcuts]

**RESOLVED (WS15, 2026-07-29).** The XDG portal round-trip is PORTED and
verified live. `crates/rustty-apprt/src/gtk/class/global_shortcuts.rs` now
implements the reference's full body (`global_shortcuts.zig:109-460,541-565`):
`refresh` builds the trigger→action map from `config.keybind.set` (`global:`
leaves with exactly one action, keyed by `key::xdg_shortcut_from_trigger`) and
drives `CreateSession` → `BindShortcuts`; `request` pre-computes the `Request`
object path from the connection's unique name + `handle_token` and subscribes to
`Response` BEFORE issuing the call; `on_response` stores the session handle,
subscribes to `Activated` (arg0-path-matched on the session handle) and chains
`BindShortcuts`; `close` unsubscribes both signals, calls `Session.Close` and
clears the map. The D-Bus stack is `gio::DBusConnection` (the connection the
`Application` already hands the class) — no new D-Bus dependency. The one new
crate edge is `rustty-apprt -> rustty-cfg-fmt`, for the `description` field
(`check_acyclic.sh` GREEN).

`Application::global_shortcut_trigger` is no longer a placeholder-leaf either: a
`perform_all_action_thunk` (installed by `bind_core_app`, same shape as
`tick_thunk`/`update_config_thunk`) carries `core_app.performAllAction(rt_app,
action)` across the type-erasure seam, so an activated global shortcut performs
its action. `global:` keybinds are FUNCTIONAL on the GTK apprt.

**Verification:** 14 headless unit tests (map build, token shape, request path,
both payload shapes, response parsing, activation → `trigger` emission) plus one
display test (`application_global_shortcut_trigger_performs_action`), plus the
live gate `.port/tools/gates/ws15_globalshortcuts_portal_gate.sh` run against
`xdg-desktop-portal-gnome` 50.0 on the VM's GNOME Shell 50 Wayland session
(`org.freedesktop.portal.GlobalShortcuts` version 2): CreateSession/BindShortcuts
both answered `Response 0`, the `BindShortcuts` results named the bound
accelerator, and an injected `Activated` signal was OBSERVED to resolve in the
shortcut map, reach `performAllAction` with an `Ok` return, and produce a second
toplevel window (counted over AT-SPI) — the bound action being `new_window`. The emitted traffic was diffed against a `dbus-monitor` capture of the
installed `ghostty 1.3.1-2.fc44` doing the same handshake: same methods, same
order, same payload shapes, differing only in the R2 token prefix
(`rustty_<7hex>` vs `ghostty_<7hex>`).

**Two environment facts the gate encodes** (established empirically in WS15 T1
against the oracle, and reproduced by the port):
1. `xdg-desktop-portal` answers `CreateSession` with `NotAllowed: "An app id is
   required"` unless it can derive the caller's app id — for a host process,
   from its `app.slice` systemd unit plus a matching `.desktop` file. The oracle
   fails identically without it.
2. GNOME 50 gates `BindShortcuts` on a one-time "Add Keyboard Shortcuts"
   confirmation dialog; the grant is then remembered per app id in
   `org.gnome.settings-daemon.global-shortcuts applications`. The gate answers it
   via the dialog button's AT-SPI `click` action.

**Not automatable here (honest boundary):** a PHYSICAL key press. mutter ignores
`wtype` and AT-SPI key synthesis on Wayland, and `ydotool`/uinput needs root.
What is proven instead: gnome-settings-daemon's own store holds the accepted
binding (`io.airely.rustty-debug -> [('SHIFT+CTRL+ALT+F12', {shortcuts:
['<Shift><Control><Alt>F12'], description: 'new_window'})]`), i.e. the
compositor grabbed the accelerator; and an injected `Activated` is followed all
the way to its side effect — the gate asserts the map lookup HIT, that
`performAllAction` returned `Ok`, and that a new toplevel window appeared. Only
mutter's key-event → `Activated` step is unexercised.

**Sibling deferral, split out:** the portal `OpenURI` call site is NOT covered by
WS15 and now has its own top-level entry below ("GTK portal OpenURI (L6)"), so it
stays discoverable by a ledger grep instead of being buried under this resolved
heading.


## GTK portal OpenURI (L6) — call site INHABITED, body unported [split out of the WS15 GlobalShortcuts entry, 2026-07-29]

**OPEN.** The sibling XDG portal call site to GlobalShortcuts: the reference's
`portal.zig` / `portal/OpenURI.zig`, reached from `application.zig`'s `startup`.
In the port, `crates/rustty-apprt/src/gtk/class/application.rs`'s `startup` is an
INHABITED placeholder for this: the `gio::DBusConnection` is fetched (and handed
to `GlobalShortcuts`), but there is no `OpenURI` consumer — nothing calls the
portal's `OpenURI`/`OpenFile` methods, and the reference's `portal` module has no
Rust counterpart yet.

**Impact:** opening a URL/file through the desktop portal (the sandbox-correct
path) is unavailable on GTK; only the non-portal fallback path exists.

**Un-defer trigger:** port `portal.zig` + `portal/OpenURI.zig` onto the same
`gio::DBusConnection` + `Request`-object-path pattern that
`global_shortcuts.rs` already establishes (pre-subscribe to `Response` on a
self-computed request path, then issue the call). WS15's live-gate harness
(`.port/tools/gates/ws15_globalshortcuts_portal_gate.sh`) is directly reusable
for the app-id/`.desktop` environment setup that any portal call needs.


## App-core over-claim CORRECTION — `main_gen.zig` / `main_wasm.zig` / `main_build_data.zig` are DEFERRED (already tracked at C6)

The gap-analysis app-core inventory marked `src/main_gen.zig`, `src/main_wasm.zig`,
and `src/main_build_data.zig` as done. **They are NOT ported — they are DEFERRED.**
This is already correctly tracked in the **C6 ledger entry** (see "`main_gen.zig` /
`main_wasm.zig` / `main_build_data.zig` (0t each) DEFERRED", above): they are
standalone build-tool / WASM-module entrypoints, NOT on the `rustty` exe's
`exe_entrypoint` switch (`main.zig` selects only `.ghostty` → `main_ghostty`), so
they carry no test-count contract and have no MAP entry. This subsection exists
solely to CONFIRM the C6 deferral is authoritative and to nullify the app-core
over-claim; no duplicate deferral is created. **Target (unchanged from C6):** port
with the gen/wasm-artifact tooling milestone (the same `help_strings`-codegen
follow-up), i.e. sub-project #3 (unused-subset) for the gen tooling and a separate
WASM-artifact milestone for `main_wasm.zig`.

## rustty-vaxis Chunk 1b (`gwidth.zig` + `unicode.zig`) — uucode-0.1.0 width-table blocker

The vaxis port (`crates/rustty-vaxis`, vendored-dep-port of libvaxis 0.5.1) lands its
CORE VALUE TYPES in **Chunk 1a** (`Cell`/`Key`/`Mouse`/`GraphemeCache`/`Winsize` +
`Image` struct-shape — READY, dep-free; see `VAXIS_C1_DESIGN.md`). The two
grapheme-width / grapheme-segmentation files are **DEFERRED to Chunk 1b** on a
unicode-data blocker that needs a **human decision** (Option A vs B below):

- `gwidth.zig` (216 LOC, **11 test{}**) → `gwidth.rs`
- `unicode.zig` (64 LOC, 0 test) → `unicode.rs`

**The blocker — vaxis reads raw uucode fields rustty-unicode does not expose.**
`gwidth.zig` imports `uucode` **0.1.0-vanilla** (vaxis pins
`git+https://github.com/jacobsandlund/uucode#5f05f8f83a75caea201f12cc8ea32a2d82ea9732`)
and reads FOUR distinct uucode surfaces:

| uucode surface | where | purpose |
|---|---|---|
| `uucode.get(.east_asian_width, cp)` → `EastAsianWidth` | `eawToWidth` | fullwidth/wide → 2 |
| `uucode.get(.general_category, cp)` | `eawToWidth` | `mark_nonspacing`/`mark_enclosing` → 0 |
| `uucode.get(.is_emoji_presentation, cp)` | grapheme loop | force emoji width 2 |
| `uucode.grapheme.Iterator` + `uucode.utf8.Iterator` | `gwidth` + `unicode.zig` | grapheme-cluster segmentation |

`rustty-unicode` (Ghostty's `src/unicode/` port) embeds tables generated from **uucode
0.2.0** run through **Ghostty's stripped `uucode_config.zig`**, exposing only a
4-field `Properties { width, width_zero_in_grapheme, grapheme_break, emoji_vs_base }`
plus `grapheme::grapheme_break(...)`. Grep over `crates/rustty-unicode/src/` confirms
`east_asian_width` / `general_category` / `is_emoji_presentation` appear ONLY in
doc-comments describing how Ghostty *derives* `width` — they are **absent from the
embedded data**, not merely un-exported. Ghostty's derived `width` is a *different*
algorithm than vaxis's `eawToWidth` (control-char `-1` sentinels; mark→0; a hardcoded
zero-width list; EAW fullwidth/wide→2); substituting it would be **faking widths**
(violates R1 + the "do not fake widths" instruction).

Second, independent skew: even the grapheme-segmentation rustty-unicode *can* supply is
a **different uucode version** (0.2.0, Ghostty-folded `GraphemeBreakNoControl` break)
than vaxis's 0.1.0-vanilla algorithm. The 11 gwidth tests + the unicode grapheme tests
pin uucode-0.1.0-vanilla results; byte-exact reproduction on 0.2.0+Ghostty-config is
**not guaranteed** and must be validated (or the right data sourced) before claiming parity.

**Remediation options (needs human decision — do not pick unilaterally):**
- **Option A — extend rustty-unicode** with `east_asian_width` / `general_category` /
  `is_emoji_presentation` accessors (regenerate its embedded tables from a uucode config
  that also emits these fields; extend the differential harness). Single shared unicode
  crate; cost: expands a crate marked "done", and the 0.1.0-vs-0.2.0 skew still must be
  reconciled/validated.
- **Option B — rustty-vaxis ships its OWN uucode-0.1.0-derived tables** (EAW / GC /
  emoji_presentation + the 0.1.0 grapheme break if it diverges), generated bit-for-bit
  from the exact commit vaxis pins. Most faithful to what vaxis actually compiles
  against; isolates version skew inside rustty-vaxis; cost: duplicates table-gen machinery.
- **Option C — map eawToWidth onto Ghostty's `width` + accept rustty-unicode's break.**
  REJECTED: not byte-exact, fakes widths, violates R1.

**Recommendation to carry into the decision:** Option B is the cleanest fidelity story
(rustty-vaxis mirrors vaxis's actual uucode-0.1.0 dependency exactly, no cross-version
guessing) at the cost of some table-gen duplication; Option A is preferable only if we
first confirm 0.2.0+Ghostty-config reproduces uucode-0.1.0's EAW/GC/emoji/grapheme
results for the 11 pinned tests.

**Un-defer trigger:** the above decision is made AND the chosen data source reproduces
the 11 `gwidth` tests + `unicode.zig` grapheme results byte-exact. Chunk 1b then adds
`gwidth.rs` (`Method` enum unicode/wcwidth/no_zwj; `eawToWidth`; `gwidth`; 11 tests) +
`unicode.rs` (`Grapheme`/`GraphemeIterator`/`graphemeIterator`) and the
`rustty-vaxis → rustty-unicode` dep edge (clean DAG; leaf). Test-count contract for 1b:
**+11** (`check_test_counts.py`). Full analysis: `VAXIS_C1_DESIGN.md`.

## rustty-vaxis Chunk 3b (tty backend + concurrency) — R5 seams / placeholder leaves

From **vaxis Chunk 3b** (`crates/rustty-vaxis/src/{tty,loop_,vaxis}.rs`) — these are
`#[cfg(windows)]` seams / placeholder-leaf `unimplemented!()` bodies, scheduled for the
named later chunk. Nothing is dropped (R1); the reference control-flow/comments are
preserved and the field/method surface is fixed so the later chunk FILLS them.

1. **`tty.rs` — `WindowsTty` (`#[cfg(windows)] mod windows_tty`).** The console-mode +
   `ReadConsoleInputW` + `INPUT_RECORD` key/mouse/resize/focus decode (~450 LOC) lands
   when a Windows target is in scope. Ported as the struct SHAPE (`stdin`/`stdout`/
   `initial_*`/`buf`/`last_mouse_button_press`) + `unimplemented!()` method bodies; the
   non-windows (macOS/Linux) build never names it (which also sidesteps its latent Zig
   `self.fd` bug — its `read` references a field it lacks). **Target: a Windows-target
   chunk.**

2. ~~**`vaxis.rs` — the `Vaxis` INHABITED placeholder leaves.**~~ **RESOLVED by
   Chunk 4a** (commit 9500493): the real `Vaxis<'a>` replaced the placeholder;
   `translate_mouse` (vaxis.rs:890), `device_status_report` (vaxis.rs:1143), and
   `wake_query_futex` (vaxis.rs:1153) are all LIVE. The 4a residuals (the two
   `zigimg` seams) are the **Chunk 4a/4b section below**.

3. **`tty.rs` — `TestTty::next_event`.** A Windows-API-compat stub (`unimplemented!()`) —
   Zig returns `error.SkipZigTest`; never reached on non-windows targets. Filled with the
   `WindowsTty` decode (item 1). **Target: a Windows-target chunk.**

4. ~~**`loop_.rs` — the `test Loop` lifecycle test (`#[ignore]`).**~~ **RESOLVED by
   Chunk 4a**: `loop_lifecycle` (loop_.rs:558) is UN-IGNORED — the filled
   `device_status_report` lets the init/start/stop lifecycle run to completion
   against `TestTty` (see the comment at loop_.rs:525).

NOTE (T14 reconciliation, 2026-07-10): item 1's Windows seam family also covers
`loop_.rs:341` (`#[cfg(windows)]` `tty_run` arm) — same `#[cfg(windows)]` class,
same Windows-target-chunk target.

## rustty-vaxis Chunks 4a/4b (the real `Vaxis<'a>` + widgets) — residual R5 seams [added 2026-07-10, T14 reconciliation — VALIDATION_REPORT M5]

Chunk 4a (commit 9500493) replaced the 3b placeholder with the real `Vaxis<'a>`
(render diff-and-emit engine, capability queries, kitty-graphics transmit
paths); Chunk 4b (commit 17bcf2d) added the `widgets`/TextInput tier. The chunk
sequence for the libvaxis port is FINISHED — these are the crate's remaining
live seams (verbatim reference bodies preserved at each; this section replaces
the formerly-dangling "(DEFERRALS.md)" citations in vaxis.rs, which pointed at
no entry):

1. **`vaxis.rs:997` `Vaxis::transmit_image` + `vaxis.rs:1018` `Vaxis::load_image`
   — the `zigimg` seams.** Both decode/convert image payloads via the unported
   Zig-source dep `zigimg` (`img.convertToPixelFormat`, PNG decode) before the
   (LIVE) kitty transmit paths. Same dep class as libxev/z2d/vaxis itself: a
   vendored-dep port. **Un-defer trigger: a `rustty-zigimg` port (or an R3
   pure-Rust re-home of the two conversions), needed only when a TUI consumer
   transmits images.**
2. **`image.rs:223` `Image::draw` + `image.rs:248` `Image::cell_size`.** The
   "(later chunk)" wording in-source is STALE (the chunk sequence is finished);
   the honest state: both consume a live `Window`/`Screen` but are unreachable
   until item 1 lands (a vaxis `Image` can only be constructed through
   `transmit_image`/`load_image`), so they defer WITH the zigimg tier.
   **Un-defer trigger: item 1.**

No other live seams exist in the crate outside the `#[cfg(windows)]` family +
`TestTty::next_event` (Chunk 3b items 1/3 above).

## rustty-font Atlas wasm surface — per-test deferral ledger (VALIDATION_REPORT §3e)

The reference `font/Atlas.zig` `pub const Wasm` block (the C-ABI/JS atlas surface
for the wasm build) is feature-gated OFF in `crates/rustty-font/src/atlas.rs`
(off-by-default `wasm` cargo feature; the whole block is preserved verbatim as a
comment at atlas.rs — R1, no reference line dropped). Its ONE test defers with it
and is tracked here PER-TEST so `check_test_counts`/G2 auditors can resolve it:

- **`Atlas.Wasm` `test "happy path"`** (Atlas.zig:564; commented verbatim at
  atlas.rs "test \"happy path\"") — **4 assertions deferred with the surface**:
  1. `try testing.expect(wasm.isHostOwned(reg));`
  2. `try testing.expect(reg.width > 0);`
  3. `try testing.expect(!atlas.modified);`
  4. `try testing.expect(atlas.modified);` (after `atlas_set`)
  It exercises `atlas_new`/`atlas_reserve`/`atlas_set`/`atlas_free` + the
  `os/wasm.zig` host-ownership helpers — none of which exist outside the wasm
  artifact. **Target: the rustty wasm app-runtime** (the same milestone that
  un-defers the config/vt wasm exports); porting the Wasm block MUST port this
  test with all 4 asserts live.

## W-ATTACH — windowed host-attach unwired in core `renderer_seam::Renderer::init` (M4 triage, 2026-07-10)

**RESOLVED — P0.1, 2026-07-12.** W-ATTACH (commit 96b0116) wired the on-screen
host-attach (`Renderer::finalizeSurfaceInit` → `Metal::attach_view` sends
`setLayer:`), so the KAT's first assertion passes. The residual red was a
HEADLESS-HARNESS gap, NOT product: (1) the inert `MockHostView` never lays its
hosted layer out, so the renderer's `surfaceSize()` (= `layer.bounds *
contentsScale`, Metal.zig:217-218) read 0×0 and `drawFrame` early-returned at the
zero-dimension guard, never presenting; and (2) the render thread's async
`present(sync=false)` hops `set_surface` onto the MAIN dispatch queue, which no
headless runloop drains. Fix is test-only: `pump_bounded` now sizes the hosted
layer (AppKit-layout stand-in) and drives the layer's `-display` (CA-commit
stand-in) — the reference on-screen SYNC present path (`display` → installed
`displayCallback` → `drawFrame(true)` → `present(true)` → `setSurfaceSync`,
Metal.zig:164-176/253-259; the renderer `golden_presents_frame_on_screen`
precedent). NO product code changed; the real app renders unchanged.
`hosts_view_layer_and_presents_live_frame` is now GREEN and UN-IGNORED. The
historical triage below is retained for provenance.

**The suppressed-then-rerun M4 KAT** (`crates/rustty-apprt-c/tests/macos_window_host.rs`
`hosts_view_layer_and_presents_live_frame`) FAILS deterministically on a Metal host.
Failure verbatim (run 2026-07-10, this machine):

```
thread 'hosts_view_layer_and_presents_live_frame' panicked at
crates/rustty-apprt-c/tests/macos_window_host.rs:365:5:
host view is not layer-hosting
```

**Root cause (verified in-tree):** the on-screen host-attach step never runs —
`rustty_surface_new` builds its renderer through core
`crates/rustty-core/src/renderer_seam.rs` `Renderer::init`, which DROPS the
host surface (`renderer_seam.rs:240`:
`let _ = (surface_mailbox, rt_surface, thread); // held-in-core seam bits`) and
forwards to `rustty_renderer::Renderer::init`, whose Metal is constructed by the
offscreen-portable ctor (`crates/rustty-renderer/src/generic.rs:1840`
`Metal::init_offscreen(...)`, the W1-7 apprt-free divergence). The on-screen
attach ctor `Metal::init(view)` (`crates/rustty-renderer/src/metal/metal_impl.rs:379`,
W1-2 — sends `setLayer:` / `setWantsLayer:` to the host view) exists but has NO
caller on the surface-creation path: the renderer_seam wiring that was to supply
it is the still-open "live-windowing `Renderer::init`" seam. NOT the old ignore
reason (`Metal::loop_enter` `unimplemented!()` — that leaf landed in W5,
`metal_impl.rs:493` is live; the stale reason is what VALIDATION_REPORT M4
flagged).

**Interim state (honest, R5):** the KAT stays `#[ignore]` with a TRUE reason
string pointing here (an un-ignored always-failing KAT would poison the suite).
Its sibling `set_display_id_pushes_to_render_thread_panic_free` PASSES on a
Metal host (4/4 consecutive runs, 2026-07-10) and is UN-IGNORED — live. The TIS
serialization fix (`app_new_serialized`: parallel `rustty_app_new` SIGABRTs in
HIToolbox `TISCopyCurrentKeyboardLayoutInputSource`; test-harness glue restoring
the one-main-thread host contract) stays regardless.

**Resolution = the windowed host-attach porting chunk** (plan
`docs/superpowers/plans/2026-07-10-port-realignment-and-validation-hardening.md`,
Task 23 — "New findings (2026-07-10 execution)" addendum): wire the host view
from `renderer_seam::Options<A>.rt_surface` into the Metal on-screen ctor
(`Metal::init(view)`) at surface init, mirroring ghostty's reference attach:
`src/renderer/Metal.zig:64` `init` reads
`opts.rt_surface.platform.macos.nsview` and makes the view layer-hosting by
setting `layer` then `wantsLayer` (Metal.zig:110-126).
**Acceptance test IS un-ignoring this KAT**: both windowed-present KATs
un-ignored and green on a Metal host.

## rustty-wuffs swizzle — 6 pixel-format seams ✅ RESOLVED (completeness-fill, 2026-07-15) [added 2026-07-10, T14 reconciliation — VALIDATION_REPORT §3b]

**RESOLVED (GAP_ANALYSIS §7 open decision 4 — fill-for-completeness).** All 6
seams in `crates/rustty-wuffs/src/swizzle.rs` (the 5 public `*ToRgba` wrappers +
the private `swizzle` helper) are now ported VERBATIM from
`pkg/wuffs/src/swizzle.zig`, binding
`wuffs_base__pixel_swizzler__{prepare,swizzle_interleaved_from_slice}` through
the wuffs-sys FFI (3 new `rustty_wuffs_pixel_format__*` shims added). 6 new
byte-exact swizzle KATs + ASan-clean (FFI → Miri can't; the ASan gate was
self-verified to trip on a deliberate under-alloc first). **Faithful finding:**
`gaToRgba` is unconditionally `Err(WuffsError)` for every input because the
vendored wuffs C's `pixel_swizzler__prepare` has a `YA_NONPREMUL` case but no
`YA_PREMUL` case — ported AS-IS per R1 (an upstream dead-code path), documented,
NOT silently "fixed". Original deferral rationale (for the record):

`crates/rustty-wuffs/src/swizzle.rs` ports `pkg/wuffs/src/swizzle.zig` as five
R5 `unimplemented!()` seams with the verbatim reference bodies preserved (the
crate previously had NO entry under its own name — only stale pre-crate
renderer lines):

- **`swizzle.rs:26` `gToRgba`**, **`:39` `gaToRgba`**, **`:51` `rgbToRgba`**,
  **`:63` `bgrToRgba`**, **`:75` `bgraToRgba`** — each is one call into the
  vendored wuffs `swizzle(alloc, src, SRC_FMT, RGBA_PREMUL)` core.

**Why deferred (scope-trim, not an external blocker):** NO rustty caller
reaches them — the background-image decode path converts internally
(`decode_frame` + `set_from_slice` with `RGBA_NONPREMUL`,
RENDERER_BG_IMAGE_DESIGN §2.2), and the renderer Kitty path uses the LIVE
re-homed `image::convert` pure-Rust loop (see the R-d P1 reconciliation above).
Fidelity-preserving dead code within the vendored wuffs subset.
**Un-defer trigger:** a future non-RGBA image ingest that consumes the wuffs
swizzle surface directly (e.g. the GTK/OpenGL renderer tier), or a completeness
pass on the wuffs vendored subset (GAP_ANALYSIS §7 open decision 4).

## rustty-z2d subset guards — 9 `unimplemented!()` arms ✅ RESOLVED (completeness-fill, 2026-07-15) [added 2026-07-10, T14 reconciliation — VALIDATION_REPORT §3b]

**RESOLVED (GAP_ANALYSIS §7 open decision 4 — fill-for-completeness).** All 9
z2d rasterizer seams are now filled VERBATIM from the vendored z2d-0.10.0 oracle:
anti-aliased **supersample** fill+stroke (`painter.rs` + new
`internal/raster/supersample.rs`), **dashed** stroke/polyline tessellation (new
`internal/tess/dashed_plotter.rs` + `Dasher`), and sub-8-bit **packed-alpha**
surfaces (`Alpha4/2/1` → new `PackedImageSurface`, with the Surface-enum ripple).
+16 new tests (no count regression), Miri-clean on all three new paths — and the
naive raw-`*mut Polygon` design for the dashed self-referential result was
*rejected as UB* on an isolated harness (`mirihar-z2d-dashed`), confirming the
shipped disjoint-borrow design removes the hazard. Original enumeration (for the
record):

`rustty-z2d` is `PORT STATUS: done` for the ghostty-exercised z2d 0.10.0
subset; the arms OUTSIDE the subset are guarded `unimplemented!()` by design
(lib.rs header documents this). Enumerated here so every live seam has a ledger
entry:

- **`painter.rs:128`** — anti-aliased fill via **supersample** (the sole
  consumer uses `.default`/`.multisample_4x`, both ported).
- **`painter.rs:322`** — supersample stroke (same class).
- **`internal/tess/stroke_plotter.rs:46`** + **`internal/tess/polyline_plotter.rs:47`**
  — **dashed** stroke/polyline tessellation (the sprite/glyf canvas draws no
  dashes).
- **`surface.rs:355` + `surface.rs:392`** — sub-8-bit **packed-alpha**
  surfaces (`Alpha4`/`Alpha2`/`Alpha1` → `PackedImageSurface`; the consumers
  use `Alpha8`/RGB(A) surfaces).

Each arm is reachable only through an `AntiAliasMode`/dash-array/`Pixel`
variant the sole consumers (rustty-font sprite/glyf canvas, the renderer
overlay) never construct — latent, not parity-blocking (GAP_ANALYSIS §6).
**Un-defer trigger:** a consumer constructing those variants (candidate: the
future GTK/OpenGL renderer tier), or GAP_ANALYSIS §7 open decision 4 choosing
fill-for-completeness.

## WASM/web target — ✅ DONE (VT C-ABI wasm), 2026-07-15 (superseded the 2 "limits" below)

**A real, running `.wasm` is produced and merged (main @ ea92fb4).** CRITICAL
REFRAMING from the first pass: the modern ghostty wasm artifact is
**`ghostty-vt.wasm`** — `build.zig:118` → `GhosttyLibVt.initWasm` compiles the
`vt_c` module (root `src/lib_vt.zig` = **libghostty-vt, the terminal VT C-ABI
ONLY**), NOT the font shaper. `main_wasm.zig` is referenced NOWHERE in the build
system; it + `font/shaper/web_canvas.zig` are **abandoned legacy** (the old
in-browser playground), broken since the 2024-04-06 Group→SharedGrid refactor
(`06df9b786` deleted `faceFromIndex`/`GroupCache`), their build target removed
2025-01-07 (`8bf5c4ed7`), replaced 2025-09 by libghostty-vt (Mitchell's
"Libghostty Is Coming" — a zero-dep embeddable VT engine; wasm consumers render
themselves). So the earlier "port web_canvas" framing targeted DEAD code.
**UPDATE (2026-07-15):** the user nonetheless requested `web_canvas` be resurrected
for completeness — DONE and browser-verified; see RESIDUAL #1 below. It is ported
against the MODERN font stack (not the deleted `Group`), so it is a live, working
backend rather than a resurrection of the deleted architecture.

**The correct target is `rustty-vt-c`** (the port of `terminal/c/` = libghostty-vt),
already a cdylib. It now cross-compiles to **`target/wasm32-wasip1/release/
rustty_vt_c.wasm` (~1.02 MB, valid WASM MVP, 157 exports)** — the rustty analog of
`ghostty-vt.wasm`. **Runtime-verified:** instantiated in node WASI, called
`rustty_type_json()` → correct 7262-byte type-layout JSON. Repro:
`.port/tools/build-wasm.sh --release`.
- **Toolchain:** `.port/tools/zig-{cc,cxx,ar}-wasm.sh` route the cc-crate to
  `zig cc/c++/ar -target wasm32-wasi` (zig 0.15.2 bundles the wasi sysroot +
  libc++ headers) via target-scoped `CC_/CXX_/AR_wasm32_wasip1` env — NATIVE
  builds untouched (`cargo build --workspace` green). Disproves the old "needs
  emscripten, env-blocked like GTK" claim: zig cross-compiles the C/C++ `-sys`
  deps (zlib C; simdutf/highway C++ with `NO_LIBCXX`) fine.
- **Faithful wasm-cfg-scoped fixes:** `rustty-xev` wasi backend arm (no-op Loop —
  the C-ABI is synchronous, no event loop, matching ghostty's wasm); `rustty-os`
  page_allocator + `rustty-vt` page.rs mmap→`alloc_zeroed` on wasm (same
  page-aligned/zeroed contract); kitty shared-mem → `UnsupportedMedium`
  (faithful to ghostty's `!link_libc` gate); the C++ `-sys` `cpp_link_stdlib(None)`.
- `xtask wasm-patch-growable-table` — verbatim port of
  `src/build/wasm_patch_growable_table.zig` + its 7 reference tests (mirrors the
  table-patch step of `GhosttyLibVt.initWasm`).

**RESIDUAL (small, documented):**
1. ~~**`web_canvas.zig` NOT ported (dead upstream)**~~ **RESOLVED — PORTED &
   BROWSER-VERIFIED (2026-07-15, user-requested completion).** `web_canvas.zig`
   (face 561L + shaper 305L) is now a live port in
   `crates/rustty-font-shaper/src/{face,shaper}/web_canvas.rs` as the
   `Backend::WebCanvas` wasm backend. The deleted `run.group.group.faceFromIndex`
   is faithfully modernized to `(*run.grid).resolver.collection.get_face(...)`
   (the modern `Collection::get_face` IS the deleted `Group.faceFromIndex` logic —
   confirmed vs `git show 06df9b786^:src/font/Group.zig`). `zig-js` → a 14-op
   `#[link(wasm_import_module="env")]` Canvas FFI + JS glue
   (`.port/tools/web-canvas-glue.mjs`). **Verified in real Chrome**
   (`example/wasm-web-canvas/index.html`): `web_canvas` Face measures + rasterizes
   real glyphs via the browser Canvas (distinct metrics R 21×25 / u 18×19 / s
   18×20 / t 18×24 / y 20×26 / ! 14×25), populates the atlas, the Shaper produces
   cells, and the VT terminal roundtrip runs — all with 0 console errors.
   (`main_wasm.zig` stays un-ported: it is the dead monolithic playground entry;
   the modern equivalent is the per-C-ABI wasm module, which is what we build.)
2. ~~**Target is wasi, not browser-freestanding `wasm32-unknown-unknown`**~~
   **RESOLVED (2026-07-15).** Built a TRUE freestanding
   `target/wasm32-unknown-unknown/release/rustty_web_canvas.wasm` (**936 KB, 184
   exports, `WebAssembly.Module.imports` modules = `["env"]` only — ZERO wasi
   imports**), loadable directly in a browser with no wasi shim. Achieved by
   making the wasm build PURE RUST: `rustty-simd` scalar-Rust fallbacks under
   cfg(wasm) for every simdutf/highway kernel (base64, codepoint_width, index_of,
   utf8-vt — extending the existing `cfg!(miri)` scalar precedent; C++ `-sys`
   gated off), and `rustty-vt` kitty zlib path → `Unsupported` stub on wasm
   (drops `zlib-sys`). New `crates/rustty-web-canvas` cdylib pulls `rustty-vt-c` +
   `rustty-font-shaper` + web_canvas. The wasi `rustty-vt-c.wasm` (residual 3)
   remains as the server/node variant; native + wasip1 builds stay green.
3. ~~**Runtime proof is instantiate + `rustty_type_json()`**~~ **RESOLVED
   (2026-07-15).** The full terminal state machine now runs end-to-end in
   WebAssembly: `.port/tools/wasm-vt-roundtrip.mjs` (node WASI, mirrors ghostty's
   `example/wasm-vt`) does `rustty_terminal_new` (80×24 — exercises the wasm page
   allocator) → `rustty_terminal_vt_write` (parses VT incl. SGR colours into the
   page tree) → `rustty_formatter_format_alloc` → reads back the correct plain
   text. Enabled by porting the **`lib_vt.zig` wasm-export block** (12
   `rustty_wasm_*` allocator convenience helpers over `rustty_lib::allocator::
   convenience` + `sgr::wasm_{alloc,free}_attribute`, wasm-cfg-gated in
   `rustty-vt-c/src/lib.rs`/`sgr.rs`) — which also un-defers the stale sgr
   `wasm_alloc_attribute` PORT NOTE (its "rustty does not yet build a WASM target"
   justification was obsolete). Module now exports 169 symbols.
4. The earlier `rustty-wasm` crate (config/core/font, the `main_wasm.zig` analog)
   is the legacy path — retained (builds natively) but NOT the wasm deliverable.

## rustty-font-shaper — Linux fontconfig backend + web_canvas [added 2026-07-10, T14 reconciliation — VALIDATION_REPORT §3b]

The D1 shaper tier is fully landed (backend selector, CoreText + FreeType
faces, HarfBuzz shaping, discovery, Collection/CodepointResolver,
SharedGrid/SharedGridSet — 110 tests). Residual, per the macOS-first locked
fork:

- ~~**`deferred_face.rs:479/482/489` — the 3 Fontconfig `DeferredFace` arms**
  (`unimplemented!("Fontconfig DeferredFace is DEFERRED (Linux backend)")`).
  The fontconfig discovery half is the same deferral class as the empty
  `fontconfig-sys` stub (VALIDATION_REPORT §3f). **Un-defer trigger: the Linux
  font-discovery milestone** (platform-linux workstream, GAP_ANALYSIS §3.6).~~
  **RESOLVED.** The Fontconfig `Discover`/`DiscoverIterator` (`discovery.rs`) and
  the `DeferredFace` `fc` arms — `familyName`/`name`/`load` (`loadFontconfig`) —
  are real ports against the `fc` binding + `fontconfig-sys` (no `unimplemented!`
  remains). The CLI wiring that consumes them (`+list-fonts`/`+show-face` on
  Linux) closed 2026-07-19 — see the "rustty-cli C1/C2" entry above.
- ~~**`face/web_canvas.zig`** — the wasm browser face: NOT ported~~ **RESOLVED
  (2026-07-15).** Ported to `face/web_canvas.rs` + `shaper/web_canvas.rs` as the
  `Backend::WebCanvas` wasm backend (Canvas FFI replaces `zig-js`; faceFromIndex →
  modern `Collection::get_face`). Browser-verified in real Chrome. See the WASM/web
  section RESIDUAL #1/#2 above for the full record.

The cli C2 entry's "Non-macOS Fontconfig discovery is still DEFERRED in the
shaper" note points here.

## LIFETIMES-AUDIT findings (2026-07-10) [added 2026-07-10, T16 audit close]

The retroactive pointer-field ownership audit (`.port/audits/GHOSTTY_LIFETIMES.tsv`,
ledger `.port/audits/LIFETIMES_AUDIT_2026-07-10.md`) closed with 20 findings:
0 high, 2 medium, 18 low. The 2 mediums are SB-class latent UB whose fix must
reproduce-then-prove on an isolated mirihar harness per doctrine
(shaper-hook-sb-clean-interleave; precedent `.port/phase-2/mirihar-chunk3b`) —
too large for the close pass, so they are FILED here with the fix prescription.
Two low-severity cleanups are filed alongside. All four TSV rows involved carry
`[audited-against-port 2026-07-10: ... FILED in DEFERRALS.md]` cross-references.

### 1. rustty-xev kqueue — `&mut` coercions into the self-referential MPSC queue (MEDIUM, latent SB UB, dead branch) — ✅ RESOLVED 2026-07-12

**RESOLUTION (2026-07-12):** FIXED and PROVEN on the isolated non-FFI Miri
harness `.port/phase-2/mirihar-xev-mpsc` (SB + TB). The harness settled the open
fix-shape questions the close pass could not: the one-line `addr_of_mut!`
prescription below is **insufficient** (candidate `plain_selfptr`: UB under both
SB and TB — the stub self-pointers minted on the init tick are popped by the NEXT
tick's `&mut self` function-entry retag); `UnsafeCell`-class head/tail/stub fields
are **also insufficient** (`cell_selfptr`: UB — the popped lineage roots at the
whole-`Loop` raw coercion, not at the queue field); and caching a raw pointer that
still points INTO the `Loop` allocation is insufficient (`inline_cached`: UB —
proving the SEPARATE allocation, not the cache, is load-bearing). Two shapes are
CLEAN under BOTH SB and TB: the Zig `tick(self: *Loop)` raw receiver
(`plain_rawtick`) and — the **applied fix** — **heap-rooting the queue** so
`Loop`'s per-tick reborrows never touch the queue's allocation (`boxq` +
`xthread_boxq`, single- and cross-thread). Applied to `kqueue.rs`:
`thread_pool_completions: TaskCompletionQueue` → `Box<TaskCompletionQueue>` plus a
cached stable raw handle `thread_pool_completions_ptr` established at first-tick
`init`; the three access sites (init/pop/thread_perform-push) route through the
cache, never a fresh `&mut …thread_pool_completions`. `queue_mpsc.rs` is UNCHANGED
(its methods already take `*mut Self`, faithful to Zig `*Self`; only the storage is
heap-rooted). The raw-receiver route was rejected as it would require converting
the public `tick`/`run` API and every driver (the live driver reborrows
`loop_.run(NoWait)` up to 2000×) — a redesign of live tested code for a dead
branch — whereas `boxq` is confined to the dead branch with zero API/caller/test
change. `cargo build --workspace` + `cargo test -p rustty-xev` (42 tests) +
`cargo +nightly miri test -p rustty-xev -- intrusive` / heap (SB + TB): all green.
The historical finding text is retained below for provenance.

---

Three sites in `crates/rustty-xev/src/kqueue.rs` reach the Vyukov intrusive
MPSC queue (`thread_pool_completions: TaskCompletionQueue`) through a fresh
`&mut` place expression coerced to `*mut Self`, contradicting `queue_mpsc.rs`'s
own PORT NOTE (lines 14-17: "a `&mut self` receiver would invalidate the stored
stub pointer under Stacked Borrows") and the TSV head/tail rows'
`AtomicPtr::from_ptr`-through-stable-`*mut` prescription:

- **`kqueue.rs:630`** — `TaskCompletionQueue::init(&mut self.thread_pool_completions)`
  (tick_inner first-init). The stub self-pointers written into head/tail carry
  this borrow's provenance.
- **`kqueue.rs:740`** — `TaskCompletionQueue::pop(&mut self.thread_pool_completions)`
  (tick_inner drain loop). Each fresh `&mut` retag invalidates the
  provenance of the stub pointers stored by line 630's tag (and by every
  earlier tick's `&mut self`), so pop's deref of the stored tail is SB UB even
  single-threaded.
- **`kqueue.rs:1394`** — `TaskCompletionQueue::push(&mut (*(*c).task_loop).thread_pool_completions, c)`
  in `thread_perform`, which runs ON A POOL THREAD: a unique `&mut` formed
  concurrently with the loop thread's accesses is UB regardless of the
  AtomicPtr operations inside, on top of the same retag-invalidation hazard.

**Why dead today (and why MEDIUM, not HIGH):** every site is behind
`self.thread_pool.is_some()` (kqueue.rs:629, :738) or only reachable from the
threadpool `StartAction` arm; the port's `Loop::init` hardcodes
`Options::default()` (kqueue.rs:321, per the §1 scope: ghostty only ever calls
`Loop.init(.{})`), so `thread_pool` is ALWAYS `None`, and
`ThreadPool::schedule` is itself `unimplemented!()` (thread_pool.rs:99-103).
No execution path reaches the UB. The Zig reference has no such hazard (plain
`*Self` method calls, no aliasing model) — this is a port-side latency, not a
fidelity break.

**Fix prescription (for the un-defer pass, NOT applied now):** route all three
sites through stable raw pointers — `ptr::addr_of_mut!((*self_ptr).thread_pool_completions)`
using the existing `self_ptr: *mut Loop` (kqueue.rs:623) for the two tick_inner
sites, and `ptr::addr_of_mut!((*(*c).task_loop).thread_pool_completions)` in
`thread_perform`. CAVEAT the harness must settle: `addr_of_mut!` through a
`self_ptr` re-derived from each tick's fresh `&mut self` gives valid access to
the queue STRUCT, but the stub self-pointers STORED in head/tail still carry
the init-tick's provenance — the harness must decide whether `tick(&mut self)`
itself needs a raw-receiver seam (or the queue's head/tail need
`AtomicPtr<T>`/UnsafeCell fields proper) for the stored pointers to survive
cross-tick retags under SB. Prove on an isolated non-FFI Miri harness
(mirihar) reproducing: init-queue → retag via new `&mut` → pop, expecting an
SB violation before the fix and none after, under both SB and TB.

**Un-defer trigger (NOW SATISFIED — fix applied ahead of the gate):** the
XEV_BLUEPRINT §6 ThreadPool body landing was the trigger (the moment
`thread_pool` can be `Some`, this becomes HIGH). The aliasing fix is now applied
and proven, so when the §6 ThreadPool body lands it need only wire the queue (the
SB/TB soundness of the completion-queue plumbing is already discharged); the
harness `.port/phase-2/mirihar-xev-mpsc` remains the reproduction/regression proof.

**Follow-up (open, low, [2026-07-19, `xev-libxev-completion` branch]):** worker-thread
`&mut Loop`/`wakeup` aliasing — kqueue.rs twin + `task_loop` lineage audit. `epoll.rs`'s
`thread_perform` no longer autorefs `Loop::wakeup(&mut self)` on a pool worker (it now
notifies through a by-value `Copy` eventfd, forming no `&mut Loop`); `kqueue.rs` still
carries the equivalent dead-path `self.wakeup()` idiom. Joint follow-up: apply the same
copied-fd notify to the kqueue twin and audit the full `task_loop`-pointer lineage under
SB/TB (mirihar) once the pool paths go live on both backends.

### 2. rustty-vt `sys.rs` decode_png — non-lib default decoder parity — ✅ RESOLVED (2026-07-23, WS3)

The reference comptime-defaults the module-global to `&decodePngWuffs` for
non-lib artifacts (sys.zig:18-24); the port defaults `None` with
`decodePngWuffs` preserved only as a commented DEFERRED-SEAM block
(rustty-vt/src/sys.rs:73-104) — even though `rustty-wuffs` now exists with
`png::decode`. Type/discipline of the global itself is verified faithful
(`RwLock<Option<DecodePngFn>>`, sys.rs:58; runtime-swapped by vt-c
sys.rs:239-243). **Un-defer trigger:** wiring the built-in PNG decode default
for non-lib builds (kitty-graphics PNG parity pass, or any consumer hitting
`decode_png == None` where ghostty-the-app would have had wuffs).

**RESOLVED (2026-07-23, `ws3-small-parity-fills`):** `decodePngWuffs` is live in
sys.rs behind the `wuffs` cargo feature (optional `rustty-wuffs` dep); the
default is the faithful comptime map — `Artifact::Lib` (or featureless) → None,
else `Some(decode_png_wuffs)`. rustty-core enables the feature for the app tier
(SharedDeps `.ghostty` analog); rustty-vt-c keeps the `.lib` None default.
KATs: sys.rs `default_decoder_*` + stream_terminal.rs
`kitty_graphics_png_transmit_decodes_via_default_hook` (run with
`--features wuffs`); graphics_image.rs `image_load_png_not_compressed_regular_file`
no longer skips under the feature.

### 3. rustty-termio `shell_integration.rs` — lossy path byte source (LOW, cleanup)

`TmpResourcesDir` (test-only helper) builds its owned `Vec<u8>` paths via
`to_string_lossy().into_owned().into_bytes()` (shell_integration.rs:1708, and
the `realpath_bytes` helper :1753-1763), deviating from the
zig-byteslice-to-rust-rawbytes rule (raw `OsStr` bytes, never lossy String).
Behaviorally benign (macOS tmp paths are UTF-8 in practice); ownership/free
discipline is faithful. **Fix:** `std::os::unix::ffi::OsStrExt::as_bytes` in a
later cleanup pass.

## rustty-apprt-c `rustty_cli_try_action` — CLI action from C (non-null path) [added 2026-07-12, Phase-A ghostty_* symbol closure]

**Context:** `ghostty_cli_try_action` (main_c.zig:120) reads the CLI action stashed
in the global C-API state during `ghostty_init` and, if present, runs it and
`posix.exit`s with its code; if absent it returns (the GUI-launch path). Ported to
`rustty_cli_try_action` in `crates/rustty-apprt-c/src/lib.rs` (+ the `ghostty_*`
alias under `identity-oracle`).

**LANDED (WS4, 2026-07-23):** both the null-action GUI-launch path AND the
non-null CLI-action-from-C path are now live. `rustty_core::GlobalState<Action>`
stays bound `Action = ()` (rustty-cli deps rustty-core — the concrete type CANNOT
live there without a cycle; the "bind Action in rustty-core" hypothesis in the
prior write-up of this entry was impossible). The realized split: PRESENCE lives
in `rustty_core::STATE.action: Option<()>` (set by `rustty_apprt::init_with_action`
from the `action_detected` flag `rustty_init` passes it), and the concrete VALUE
lives in a top-sink `CLI_ACTION: OnceLock<Option<rustty_cli::rustty::Action>>`
stash local to `crates/rustty-apprt-c/src/lib.rs` (set alongside `ARGV`/detection
in `rustty_init`) — together they realize the reference's one `state.action`
field. `rustty_cli_try_action`'s `Some` arm now runs
`std::process::exit(match action.run() { Ok(code) => i32::from(code), Err(err) => {
log(...); std::process::exit(1) } })`, faithful to the reference.

## ✅ RESOLVED 2026-07-28 (WS9) — gettext-sys self-contained libintl (`dgettext`/`bindtextdomain`/`textdomain`)

**Context (P3 self-contained-lib pass):** the macOS Swift-app link
(`macos/build-rustty.sh` `OTHER_LDFLAGS`) still passes
`-L/opt/homebrew/opt/gettext/lib -lintl` — a **Homebrew** dependency, violating
the self-contained-archive goal (R3 wants rustty's OWN vendored C, folded into
`libGhosttyKit.a` by xtask). **zlib was made self-contained in this same pass**
(vendored `deflate.c`/`trees.c`/`compress.c` → full zlib, `-lz` dropped); libintl
is NOT, and is deferred here.

**Why deferred (large vendor change, not a small build.rs tweak):**
`crates-sys/gettext-sys` currently vendors ONLY the gnulib-lib `localename`
family — the six `.c` in `build.rs` `SOURCES` needed to define
`gl_locale_name_canonicalize` (`_libintl_locale_name_canonicalize`). It provides
**no** message-catalog runtime: `dgettext`/`dcgettext`/`dcigettext`/`ngettext`/
`bindtextdomain`/`textdomain`/… are absent, so the app resolves them from
Homebrew's `libintl`. Providing them means vendoring the **entire gettext-runtime
`intl/` core** — per `ghostty/pkg/libintl/build.zig` `srcs`, ~23 additional
translation units: `bindtextdom.c dcgettext.c dcigettext.c dcngettext.c
dgettext.c dngettext.c explodename.c finddomain.c gettext.c hash-string.c
intl-compat.c l10nflist.c langprefs.c loadmsgcat.c localealias.c log.c
ngettext.c plural-exp.c plural.c setlocale.c textdomain.c version.c compat.c`
plus their private headers (`gettextP.h`, `gmo.h`, `loadinfo.h`, `plural-exp.h`,
`hash-string.h`, `gettext.h`, …) and the generated `config.h` knobs — notably
`plural.c` is a **Bison/yacc-generated** parser (checked-in generated source).
This is a substantial vendor+build.zig-flags port, not the "small build.rs
change" bar P3 set for auto-doing, so it is scheduled rather than rushed.

**What's needed to un-defer:** vendor the `intl/` core `.c`/`.h` above into
`crates-sys/gettext-sys/vendor/` (verbatim upstream gettext, byte-faithful R3),
extend `build.rs` `SOURCES` with them, forward `pkg/libintl/build.zig`'s compile
flags (`-DHAVE_CONFIG_H -DIN_LIBINTL -DLOCALEDIR="" -DLOCALE_ALIAS_PATH=""` etc.)
and any `config.h` additions, expand the `gettext-sys` FFI surface
(`bindgen`/hand-written) to the catalog API, then drop
`-L/opt/homebrew/opt/gettext/lib -lintl` from `macos/build-rustty.sh` (the folded
`libRusttyKit.a` will then resolve those symbols). **RESOLVED by WS9**: all 30
translation units (the reference's full `srcs`) are vendored under
`crates-sys/gettext-sys/vendor/intl/`, `build.rs` compiles them with the
reference's exact flags into `librustty_intl.a`, and BOTH `-lintl` link sites
are gone (`macos/build-rustty.sh` and `crates/rustty-os/build.rs`). `plural.c`
needed no Bison — it ships pre-generated in the gettext 0.24 tarball. Proven by
`.port/tools/gates/no_homebrew_gettext_gate.sh`.

## P2 crash-guard: on-feature quicklook/inspector leaves — RESOLVED [closed 2026-07-28 by WS6]

All three guarded leaves are gone; there is no crash-guard sentinel left anywhere
in the tree (`grep -rn "not yet ported\|TODO(port)" crates/rustty-apprt/src/embedded.rs
crates/rustty-apprt-c/src/lib.rs` -> no hits).

- `rustty_surface_quicklook_word` — un-deferred + fully ported in P2.1 (objc-free
  core/vt).
- `rustty_surface_quicklook_font` — un-deferred + fully ported in P2.2 (CoreText
  face read via objc2-core-text, returning the +1 CTFontRef the host releases, I16).
- `embedded::Inspector::init` — un-deferred by the 2026-07-14 inspector series
  (`027f695`): it creates the per-Surface Dear ImGui context, runs
  `CoreInspector::setup()`, and activates the core inspector. The `eprintln!`
  no-op sentinel and its `Err(ApprtError)` early return were removed with it; the
  export's `null` return is once again the reference's own init-failure path, not
  a placeholder. The downstream chain (`deinit`/`initMetal`/`renderMetal`,
  `Backend::deinit`, the `rustty_inspector_metal_*` exports, core
  `activate`/`deactivateInspector`, `queueInspectorRender`) is likewise live, so
  the old "reachability analysis" that justified guarding only `init` no longer
  applies — nothing downstream is unconstructible any more.

**Runtime sign-off (WS6, 2026-07-28):** cmd+opt+i opens a rendering inspector in
Rustty.app, docked under the terminal with the live
Terminal / Surface / Keyboard / Terminal IO / Renderer tab bar; the Terminal tab
shows a populated Screen Status table (primary `active`, alternate
`(not initialized)`) plus Misc/Layout/Mouse/Color/Modes sections, and the Screen
sub-window shows live Info/Cell/PageList with a real cursor position. Typed input
populates the Keyboard panel (per-key press/release rows with UTF-8 + PTY
encoding) and the Terminal IO panel (live VT events). The ImGui style really
rescales with display density — verified by an A/B rebuild against a tree with
WS6 T3 reverted: without T3 the chrome is frozen at the unscaled 1.0 defaults
(inspector row pitch ~21 px), with T3 it is the default style run through
`ScaleAllSizes` at the Retina backing scale (row pitch ~27 px, i.e.
`FramePadding.y` 3 -> 6 exactly as `ImTrunc(v * 2.0)` predicts).
Screenshots: `/tmp/ws6/inspector-open.png`, `/tmp/ws6/inspector-keyboard.png`,
`/tmp/ws6/inspector-after-input.png`, `/tmp/ws6/inspector-screen.png`,
`/tmp/ws6/v2-inspector-open.png` (final HEAD build), and the HiDPI A/B pair
`/tmp/ws6/ab-NO-T3.png` vs `/tmp/ws6/v2-inspector-open.png`.
Backend rasterization is additionally gated by the offscreen KAT
`dcimgui-sys::metal_offscreen_render_writes_pixels`.

---

## rustty-termio ReadThread — PRESERVED REFERENCE QUIRK: trailing pty output dropped on Linux POLLHUP (L1 LT9)

**Status: preserved verbatim, NOT fixed. This is reference behavior, not a port bug.**

`ReadThread.threadMainPosix` checks HUP *before* it re-drains the pty:

```zig
// ghostty src/termio/Exec.zig:1355-1358
if (pollfds[0].revents & posix.POLL.HUP != 0) {
    log.info("pty fd closed, read thread exiting", .{});
    return;
}
```

The read loop's shape is: tight `read()` until `EAGAIN` -> blocking `poll(-1)` ->
quit check -> **HUP check -> return**. There is no final drain after the HUP check.

**Why this is invisible on macOS but bites on Linux** (a kernel pty-semantics
difference, verified empirically in L1 LT9, not inferred):

* **Linux** raises `POLLHUP` on the pty *master* as soon as the last *slave* fd
  closes — **even while unread bytes are still buffered** — so a child that writes
  and immediately exits produces `POLLIN|POLLHUP` in the SAME `revents` word
  (measured: `0x11`). The reference returns on the HUP check and the buffered bytes
  are never read. Measured directly: at that HUP return the pty still held the
  unread `"RUSTTY_OK\r\n"`.
* **macOS** does not surface the dying child as HUP here; it surfaces as
  `read() == 0`, which the loop already handles (`if (n == 0) break;` — and the
  reference's own comment there says *"This happens on macOS instead of WouldBlock
  when the child process dies"*). So macOS drains first and loses nothing.

**Consequence:** on Linux, output written by a child in the same instant it exits
can be dropped by the read thread. For a real interactive shell this is a
shutdown-time edge (the shell is long-lived; the HUP only arrives at exit), which
is presumably why it is unfixed upstream.

**Why we do NOT fix it:** R1 — port faithfully, preserve reference quirks. Adding a
post-HUP drain would be a real behavioral divergence from ghostty on the hot read
path. If upstream fixes it, port the fix.

**Impact on the KAT:** `crates/rustty-termio/tests/live_shell.rs` therefore spells
its command `/bin/sh -c 'echo RUSTTY_OK; /bin/sleep 1'` rather than a bare `echo`:
the trailing sleep keeps a slave fd open for a beat after the write, decoupling
`POLLIN` from `POLLHUP` so the read thread drains on a HUP-free wakeup exactly as
it does on macOS. This removes only the exit RACE — the KAT still spawns a real
`/bin/sh` in a real pty and asserts the echo round-trips into the terminal
`Screen`. (`/bin/sleep` is absolute because that harness's `Config.env` is an empty
`EnvMap` — there is no PATH to search.)

**Un-defer trigger:** an upstream ghostty fix to the `threadMainPosix` HUP path.

---

## GTK winproto (wayland/x11 arms) + dcimgui inspector — Linux/GTK tail [2026-07-18, final parity audit]

Recorded after the **final parity audit** (25 adversarially-verified agents over all Linux/GTK
features vs ghostty) confirmed these are the ONLY remaining Linux/GTK divergences and that they
are **not daily-use parity gaps on the common (GNOME/mutter) target**.

> **CORRECTION (2026-07-29, WS15 review):** "ONLY remaining" is no longer accurate — the portal
> `OpenURI` call site is also open; see "GTK portal OpenURI (L6)" above. Read this entry as
> covering the winproto-x11 and dcimgui divergences only.

- **`gtk/winproto/x11.rs` — the x11 window-protocol arm (L6).** The reference
  `winproto/x11.zig` is unported; the x11 arm of the `App`/`Window` unions stays `#[cfg]`-gated
  off. Non-gap on Wayland sessions. Un-defers with `gdk4-x11` + an X11 session to verify.
- **`gtk/winproto/wayland.rs` — the wayland arm LANDED [2026-07-19], with two sub-deferrals.**
  `winproto/{wayland.rs, wayland/globals.rs, blur_region.rs}` are now ported behind the `wayland`
  cargo feature (`gdk4-wayland` + `wayland-client`/`wayland-backend`/`wayland-protocols`/
  `wayland-protocols-plasma`). `App::init` bridges GDK's foreign `wl_display` -> wayland-backend
  guest backend -> wayland-client `Connection` -> `wl_registry`, then two-phase-roundtrips to bind
  globals. Bound when advertised: `wl_compositor` + `xdg_activation_v1` (present on GNOME/mutter);
  graceful-`None` when absent (all KDE globals + `ext_background_effect` on GNOME):
  `org_kde_kwin_server_decoration_manager`, `org_kde_kwin_slide_manager`, `kde_output_order_v1`,
  `ext_background_effect_manager_v1`. **`ext_background_effect` IS bindable** (wayland-protocols
  0.32 staging), so the blur arm is a full faithful port — not skipped — it just early-returns on
  mutter (no blur cap). `Window` drives CSD (`org_kde_kwin_server_decoration.request_mode` per the
  window-decoration config), `xdg_activation` urgency (`set_urgent`), and blur (`BlurRegion` +
  `ext_background_effect_surface_v1.set_blur_region`). Two sub-deferrals:
  - ~~**Quick terminal / `wlr-layer-shell`.**~~ **RESOLVED [2026-07-19].** Implemented via the
    `gtk4-layer-shell` 0.8 crate (behind the `wayland` feature). `App::supports_quick_terminal()`
    now returns `gtk4_layer_shell::is_supported()` (graceful-`false` on mutter, which has no
    `wlr-layer-shell`); `init_quick_terminal()` + `sync_quick_terminal` (layer/namespace/keyboard/
    anchor/margin + the KDE `org_kde_kwin_slide`) + `entered_monitor` sizing are ported faithfully
    (`wayland.zig:70-88,310-389,392-421`). **Verified working on a real sway 1.11 session** (Super+`
    → full-width top drop-down, functional), matching ghostty. Plan:
    `docs/superpowers/plans/2026-07-19-gtk-quick-terminal-layer-shell.md`. NOTE: the toggle GAction
    handler (`action_toggle_quick_terminal`) had been a placeholder-leaf no-op — a whole class of bug
    (also `action_quit`/`action_open_config`) fixed in `8e024f3`; that wiring is what makes the
    keybind actually work. ~~Residual: `estimateInitialSize`~~ **RESOLVED**: `Surface::estimate_initial_size`
    is now ported (`surface.zig:2095-2138`) and called from `init_and_show_window` — window-width/height
    are honored (verified on real sway: `window-width=120 window-height=40` → 1204×869 px vs 804×629
    natural; early-returns when unset, matching ghostty's `<=0` guard). The D-Bus `+toggle-quick-terminal`
    IPC path is **confirmed working** on real sway (renders the drop-down, human-observed) — the earlier
    "IPC doesn't render" was a test-harness artifact (busy-loop "waits" that completed in µs → every grim
    fired before the async enter-monitor render). No residual: keybind AND IPC both render; the toggle/IPC
    code is byte-faithful to ghostty (6-point audit).
  - **Post-init async event delivery — RESOLVED [2026-07-19] via a GSource, RUNTIME-VERIFIED
    on a live wlroots compositor (headless sway) [2026-07-19].** The registry `Connection` gets its OWN libwayland event
    queue (`Backend::from_foreign_display` -> `wl_display_create_queue`), which GDK's main loop
    does not dispatch. The init two-phase roundtrip captures all bind-time state (default
    decoration mode, blur caps, output order); events arriving LATER (a compositor
    decoration-mode change, an `xdg_activation` token `done`) dispatch only if the queue is
    cranked. `winproto/wayland/queue_source.rs` (`WlQueueSource`/`Pump`) integrates the private
    queue into the GLib main loop as a custom `GSource` whose prepare/check report ready exactly
    when the queue has pending events (`EventQueue::prepare_read()` returning `None`; a returned
    guard is dropped, cancelling the read intention — GDK alone reads the socket) and whose
    dispatch runs `dispatch_pending` + flush — the wayland-rs analogue of the reference relying
    on GDK's default-queue dispatch (`Globals.zig:82`). Requests we SEND go out on the shared
    display fd via `Connection::flush`. VERIFIED end-to-end by the live-compositor KAT (isolated headless sway)
    `wayland_set_urgent_dispatches_activation_token` (`tests/gtk_wayland_winproto.rs`):
    `set_urgent(true)` -> the compositor's real token `done` -> `Globals` handler `activate` (observed via
    the test-only `App::activation_done_count`). NOTE the FIRST cut of this GSource (fd watch via
    `g_source_add_unix_fd`, `check` keyed on `G_IO_IN` revents) NEVER fired — GLib dispatches only
    the highest-priority READY band per iteration, and GDK's higher-priority source (same fd) wins
    the band and drains the socket, so the revents-keyed check was permanently starved; the KAT
    caught the token `done` demarshalled-but-undispatched, plus a latent trampoline cast bug
    (callback data is `*mut Rc<RefCell<Pump>>`, was deref'd as `*const RefCell<Pump>` — every
    `try_borrow_mut` failed). Both fixed by the queue-pending prepare/check rewrite. Still-open
    coupled sub-item: `Window` snapshots the decoration mode at init, so a LIVE compositor
    decoration-mode change (now cranked) is not re-read — a separate tracked deferral.
- **`gtk/class/inspector_*` + `imgui_widget` — the dcimgui terminal inspector (L6)
  — RESOLVED [2026-07-19].** The GTK/OpenGL3 inspector landed (commits `657f223`,
  `f780650`): the `.inspector` performAction arm and `Surface::control_inspector`
  are live, the `InspectorWindow`/`imgui_widget` GTypes are ported, and the
  OpenGL3 backend arm of `dcimgui-sys` (`imgui_impl_opengl3.cpp`, the
  `#[cfg(not(target_os = "macos"))]` FFI block) is gated by the
  `opengl3_symbols_link` KAT plus the `gtk_inspector_dump` framebuffer KAT. It
  shares the same `crates/rustty-inspector` widget render as the macOS/Metal
  inspector, so WS6's G3b zero-drop proof covers both. The 2026-07-14
  inspector-port plan is superseded; WS6 closed the macOS half.
  Remaining Linux debt is the frame-time-plot row in VERIFICATION_DEBT.md.
- **`+new-window -e <cmd>` argv override — DONE [2026-07-19].** `action_new_window`
  (`class/application.rs`) now parses the received `new-window-command` `as` parameter
  (`-e`/`--command=`/`--working-directory=`/`--title=`) into a `TabOverrides` and opens the window
  directly via `action::new_window(app, Target::App, overrides)` — mirroring `actionNewWindow`'s
  direct `Application.newWindow(self, null, overrides)` call (`application.zig:1716-1808`), rather
  than queuing a `.new_window` on the core mailbox (the `activate`/`.none` path). Runtime-verified:
  `rustty +new-window -e /bin/sleep 987` against a single-instance primary spawned `/bin/sleep 987`
  as a direct child of the primary (not the default `/bin/bash` shell).

### Wayland winproto — set_urgent async dispatch + duplicate-global nit [2026-07-19, grounding review]
The wayland arm (feature `wayland`) is a FAITHFUL, runtime-verified port (App::init selects
Protocol::Wayland on a live GNOME/Wayland session; blur/CSD/slide/ownership verified vs the
protocol XML — the port even destroys a `wl_region` + the `activation_token` that ghostty leaks).
Two known residuals (grounding-review):
- **RESOLVED + RUNTIME-VERIFIED [2026-07-19]: `set_urgent` urgency now dispatches on a live
  Wayland compositor (KAT run isolated on headless sway/wlroots; mutter implements the same
  `xdg_activation_v1` protocol).** The registry/globals live on a private libwayland event queue
  (`Connection::new_event_queue`) that GDK's main loop does not dispatch; the port previously only
  `flush`ed (outgoing), so `xdg_activation_token_v1.done` (the inbound reply carrying the token)
  never dispatched → `xdg_activation_v1.activate(token, surface)` never fired → a backgrounded
  terminal's bell did NOT flash the taskbar (works on ghostty, which uses GDK's default
  `wl_display` queue, `Globals.zig:82`). Fixed by `winproto/wayland/queue_source.rs`
  (`WlQueueSource`): a custom GLib `GSource` whose prepare/check are ready exactly when the private
  queue has pending events (`prepare_read()` == None; GDK alone reads the socket) and whose
  dispatch `dispatch_pending`s + flushes, at `G_PRIORITY_DEFAULT_IDLE`. (The first cut keyed
  `check` on fd revents and was permanently starved by GDK's same-fd higher-priority source —
  GLib dispatches one priority band per iteration; it also had a trampoline cast bug that made
  every `try_borrow_mut` fail. Both caught by the KAT below and fixed.) Runtime evidence: the
  live-session KAT `wayland_set_urgent_dispatches_activation_token`
  (`crates/rustty-apprt/tests/gtk_wayland_winproto.rs`) drives `set_urgent(true)` on a realized
  window and asserts mutter's real token `done` dispatched through the GSource (via the test-only
  `App::activation_done_count` counter) — passes in <1s. CSD + blur were never affected (their
  inputs are captured at the init two-phase roundtrip; their requests are outgoing). Remaining
  coupled follow-up: `Window` still snapshots the decoration mode at init rather than re-reading
  `Globals` live (a compositor decoration-mode change now cranks but is not re-read).
- **LOW (pathological): duplicate global re-advertisement.** If a compositor re-advertises a singleton
  interface with a new `name`, `Globals::bind_global` overwrites the typed proxy but leaves the stale
  `name→tag` map entry, so a later `global_remove(old_name)` would unbind the live proxy. Compositors
  don't re-advertise these singletons in practice. (globals.rs bind_global/remove_global.)

### xev ThreadPool `Event` shutdown wake — DELIBERATE DEVIATION (controller-approved) [2026-07-19, Task 1]
`crates/rustty-xev/src/thread_pool.rs` `Event::wake` deviates from the oracle
(`ThreadPool.zig:508-518`, kprotty's Zap pool) in two ways, both confined to `wake`:
**(1)** the terminal SHUTDOWN transition's `futex_wake` is UNCONDITIONAL (the oracle skips it when
the pre-swap state is not WAITING, which strands remaining parked sleepers when a trailing run-loop
`notify()` left the event NOTIFIED — `join()` then hangs); **(2)** SHUTDOWN is STICKY (the NOTIFIED
transition is a CAS loop that no-ops on a shutdown event, because the oracle's unconditional swap
lets a late `notifySlow` event-write resurrect the dead event and re-park a woken sleeper forever).
Rationale: SHUTDOWN is terminal/unrepeatable, so the extra wake is sound (sleepers re-check state;
late arrivals fail the futex value-check) and notifying a shutdown event is already a no-op by
`Event::wait`'s own contract; cost = one extra syscall at pool shutdown. Both oracle holes are
ANALYTIC windows (the originally-observed 100% teardown deadlock was root-caused to a rustty port
bug — the `futex_wake` shim passed `u32::MAX` unclamped through FUTEX_WAKE's C-int slot = `-1` =
wake at most ONE; fixed with the oracle's own `min(i32::MAX)` clamp semantics, after which the
VERBATIM oracle Event passed 400/400 multi-worker lifecycles). ghostty never instantiates the pool
(all 4 xev loops are `Loop.init(.{})`), and upstream libxev `1c80766` (Dec 2024) fixed a DIFFERENT
join race already in our pin — upstream HEAD still carries these two windows. rustty's Task-5 epoll
File watcher WILL multi-worker-join, hence the defensive close. Full interleavings + soundness
argument: the DELIBERATE DEVIATION note on `Event::wake`; RED/GREEN evidence + regression gate
(`thread_pool::tests::multi_worker_quiesce_then_join_50x`, 50 lifecycles under a 60s watchdog):
`.superpowers/sdd/task-1-report.md` addendum 2.

### `share/{APP}` resource root — shell-integration + themes SHIPPED ✅ RESOLVED [2026-07-23, WS1]
The R5 placeholder (`share/{APP}/DEFERRALS.md`, written by
`xtask/src/res_terminfo.rs::gen_app_root`) that stood in for the
shell-integration and themes subtrees is DELETED — both trees ship real
vendored content: `crates/rustty-extra/shell-integration/` (7 scripts,
R2-rebranded to `RUSTTY_*`/`rustty-*`, matching `rustty-termio`'s
`env_name()`/`APP_NAME` runtime lookups; oracle-parity KAT
`xtask res_shell_integration::tests::oracle_shell_integration_parity`,
dev-only `#[ignore]`, rebrand-mapped byte-compare with `bash-preexec.sh`
verbatim) and `crates/rustty-extra/themes/` (534 files, verbatim from ghostty's
pinned `iterm2_themes` tarball `ghostty-themes-release-20260608-160426-8c84dd1.tgz`;
MIT attribution vendored at `themes/LICENSE.md`, `.md`-excluded from emission
so the shipped count stays exactly 534). `gen_app_root` now scrubs stale
placeholder notes from old output dirs. Verified: clean-regen byte-compare,
rebuilt `Rustty.app` bundle carries 534 themes + all 7 scripts + zero
`DEFERRALS.md`; headless zsh injection smoke green
(`ZDOTDIR=<share>/shell-integration/zsh zsh -i` registers
`_rustty_deferred_init` in `precmd_functions`); `identity-oracle` profile
emits the `share/ghostty/` parity dir. Residual: live `+list-themes` CLI
count + in-window OSC7 observation re-queued to WS4 (blocked on
`rustty_cli_try_action`'s non-null arm). Ledger: INTEGRATION_PARITY.md
§2.1/§2.2 struck RESOLVED same-commit.

## Deferred to WS7 D6 (inspector compression memory view)

From **WS7 Phase B review (I1)** — these need the final
`oracle@main:src/inspector/widgets/pagelist.zig` (`70e788f06` + `6d5dda40d`):

1. **`crates/rustty-inspector/src/widgets/pagelist.rs` cell-detail path still
   restores.** The page-TREE loop was fixed in the Phase B review (it now uses
   `Node::rows`/`page_if_resident`/`storage`/`page_preserving_state`, guarded by
   `ws7_tests::pagelist_widget_page_tree_never_restores`). The CELL-detail path
   (`Node::page(cell.node)` at the style / hyperlink / grapheme tables) still
   restores the ONE page holding the inspected cell. Fixing it faithfully is the
   D6 restructure, not a local edit: the oracle replaced `pages.getCell(pt)` with
   `pages.pin(pt)` + `pin.node.pagePreservingState(...)` + an `InspectedCell`
   struct, and threads the resulting `page: *const Page` through
   `cell_info.draw`, `hyperlinkTable` and `graphemeTable`. D6 must port that end
   state and then tighten the guard test to the plan's stricter form
   (`!src.contains("Node::page(")` over the WHOLE file).

2. **`PageList::Limits` / `Limits::enforce` has no rustty counterpart.** The
   oracle's 11th `markActivity` call site (`oracle@main:PageList.zig:6312`, inside
   `Limits.enforce`) therefore has nothing to port onto. rustty's PageList carries
   its byte limit as a plain field with no `Limits` struct, so there are 10
   `mark_activity` sites, not 11. See the PORT NOTE next to `mark_activity` in
   `crates/rustty-vt/src/pagelist/compress.rs`. If a `Limits` struct is ever
   introduced, `enforce` must `mark_activity` on entry.

## Deferred to WS7 Phase E (Miri / gates)

From **WS7 Phase B review (I2 harness run)**:

1. **`compress_page` feeds uninitialized bytes to LZ4 (Miri SB, OPEN).**
   `.port/phase-2/mirihar-pagelist-compress`'s `roundtrip`/`leak` scenarios abort with
   "constructing invalid value of type `[u8; 8]`: encountered uninitialized memory" inside
   `lz4::compress` reading the page mapping (`compress/page.rs:137` ->
   `pagelist/compress.rs:406`). **Not pre-existing — introduced by B7; the Phase A codec
   is not implicated and `main` is unaffected** (nothing merged today feeds a page mapping
   to LZ4). `rustty-os`'s `cfg(miri)` page allocator hands out zeroed memory, so the uninit
   bytes are real de-initialization from typed writes of padded structs into the mapping,
   not an FFI/Miri artifact; impact is bounded by blast radius (padding/inter-table gaps
   only — worst case a spurious verify failure or differing padding after restore, never
   cell corruption). Full write-up, backtrace and candidate resolutions in
   `.port/phase-2/MIRI_EXCEPTIONS.md` ("WS7 Phase B"), including a cheaper fourth
   resolution (zero the sub-table gaps / write through a pre-zeroed `MaybeUninit` at
   construction, removing the uninit at the source instead of the read). Phase E must
   either fix it or record a formal 4-criterion exception; it must NOT be left silently
   red.
2. **I3 leak check is a manual proof artifact, not a gate.** Nothing in `.port/tools/` or
   `ci.sh` runs any `mirihar-*` crate, so `.port/phase-2/mirihar-pagelist-compress`'s
   `leakcheck` scenario (native, milliseconds, no Miri needed) demonstrates leak-freedom
   but does not automatically catch a future regression. This matches existing repo
   precedent (`mirihar-compress-lz4`, `mirihar-chunk3b`), so it is not being wired now —
   but at minimum the native `leakcheck` scenario should be wired into a gate in Phase E.
