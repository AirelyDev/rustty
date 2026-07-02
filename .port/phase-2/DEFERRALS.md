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

## Deferred: DCS <-> tmux enter/exit wiring — test `dcs::tests::tmux_enter_and_implicit_exit`

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
(2019) and `--features oniguruma` (2030) builds; the faithful body is preserved
verbatim (R1, not weakened). This is NOT one of the StringMap gaps above — it is a
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
- **DEFERRED (untested-only, 0 tests — R1-safe) — PARTIALLY RESOLVED (core
  C8):** the concrete `ThreadStateSurface` impl. **rustty-core's `Surface` now
  implements the trait** (core C8, ahead of its C10 row: `impl
  ThreadStateSurface for Surface<A>` + `Surface.crashThreadState()`
  Surface.zig:6014 + the first two `crash.sentry.thread_state = ...` sites —
  sizeCallback :2469 / preeditCallback :2530, via
  `sentry::setThreadState`/`clearThreadState` and the [ADV 6.1] Drop guard
  `ThreadStateGuard` mapping the paired `defer ... = null`). The remaining
  Surface.zig sites port with their owning chunks (C10-C13); the renderer
  Thread.zig / termio Thread.zig/Exec.zig sites stay with their owning files
  (GPU/D6 deferrals).
- Note: tests never call the real `sentry_init` (the reference gates init on
  build options and threading; the vendored build is `transport = .none`) —
  the sentry tests exercise `beforeSend` + the seam through the PURE value API
  only, and `sentry_pkg`'s reify test mirrors the sentry-sys KAT idiom.

## rustty-termio stage 2 (integration tier) — dep wiring + struct shapes; deferred live-thread bodies + D6 xev seam

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
  - `Thread.zig`: the run loop (`threadMain`/`threadMain_`/`CallbackData`/
    `drainMailbox`/`startSynchronizedOutput`/`handleResize`/`startScrollTimer`/
    `stopScrollTimer` + the five xev callbacks) — D6. Ported REAL: the struct
    shape, `Coalesce` (+ `MIN_MS`), `SYNC_RESET_MS`/`SELECTION_SCROLL_MS`,
    `Flags`, `init`, `deinit`.
  - `mailbox.zig`: `send` — its slow path unlocks/relocks a CALLER-held
    `std.Thread.Mutex` (manual lock/unlock), which guard-based
    `std::sync::Mutex` cannot express from the callee frame; the renderer
    `State.mutex` modeling is decided ONCE when the threads go live (D6 /
    core tier), not fabricated now. Ported REAL: `initSPSC`/`deinit`/`notify`.
    [UPDATED core C0: the mutex MODEL is now decided — renderer `State.mutex`
    retyped to `*mut rustty_zigstd::thread::Mutex` (guard-less Zig-shaped
    lock/try_lock/unlock, CORE_BLUEPRINT §3) and `send`'s parameter carries
    it; the body itself stays deferred until the core-tier termio pass (C-T).]
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
  the core-tier termio pass (C-T), per CORE_BLUEPRINT §3.] Every OTHER handler
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
  (App.performAction/keyEvent). **Still deferred: `Scoped(.surface)`** — it
  lands with ITS consumer, core C14 `Surface.performBindingAction`
  (Surface.zig:4775). 0 tests either way.
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

- **DEFERRED (verbatim, CORE_BLUEPRINT §4 — the ONLY verbatim-comment
  deferrals in core): `activateInspector`/`deactivateInspector`**
  (Surface.zig:890/915, ~45 loc) — they `alloc.create`/`deinit` the
  UNINHABITED Phase-3 `inspectorpkg::Inspector` placeholder (unconstructible
  until the ImGui inspector tier lands). Bodies preserved verbatim above
  `unimplemented!()`; `deinit`'s inspector teardown arm is
  unreachable-by-construction (`unreachable!()` + verbatim body) for the same
  reason. 0 tests, R1-safe; resolved by the Phase-3 inspector fill.
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
