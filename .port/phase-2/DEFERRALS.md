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

### rustty-renderer R-d — offscreen Metal draw path (Chunks 0-4) DONE; on-screen + config + Kitty + overlay DEFERRED

**DONE.** The monomorphic `Renderer<Metal>` RENDERS a terminal state offscreen
(macOS/Metal). The whole `metal/*` backend (device/queue/buffer/texture/pipeline/
render-pass/target/frame), `SwapChain`/`FrameState`, `update_frame`, the
`rebuild_cells` CPU cell-fill, the uniforms math, and `draw_frame` (Metal
encode+submit → IOSurface readback) are ported and gated by the golden test
`generic::macos_impl::tests::golden_renders_cursor_fg_bg` ("Ab" + cursor, real pixel
asserts). The `size_of::<CellText>()==32` un-defer landed. The `draw_frame` raw-ptr
aliasing discipline is proven clean under **both** Stacked and Tree Borrows by
`.port/phase-2/mirihar-chunk3b`.

**DEFERRED (five named Goal-2 tiers — 18 live `unimplemented!()` call sites, each
with its verbatim Zig body preserved, R5; the full seam audit is
`.port/phase-2/GPU_RD_CHUNK5_DESIGN.md` §1).** Zero of the 18 is reachable from the
tested offscreen path (proven by walking `draw_frame` against the golden's static
pins: `overlay/inspector/images/bg_image = None`, empty images, `sync = true`,
`size_changed = false`, direct struct construction with no `Drop`):
- **windowing-host / apprt-surface:** `Renderer::{init,deinit}` (live NSView),
  `Metal::{init,loop_enter}`, the async `Frame::complete` present half, the real
  CAMetalLayer/IOSurfaceLayer + display-link windowed present, and the
  resize/config-swap (`changeConfig`/`setScreenSize`/`setFontGrid`).
- **renderer↔config (G7 cycle):** `Renderer.DerivedConfig::init`,
  `Message::initChangeConfig`, `Thread.DerivedConfig::init`,
  `shadertoy::load_from_files`/`load_from_file` (commented verbatim, no live fn), and
  `prep_background_image` file I/O (subsumed by the `Renderer::init` seam).
- **Kitty-pixel / image-upload:** `image` `kitty_update`/`prep_kitty_*`/`convert`
  (wuffs swizzle).
- **overlay-render:** the z2d `Overlay` `highlight_*` rect draws.
- **rustty-inspector:** `overlayFeatures` (uninhabited inspector placeholder).
- the **OpenGL/WebGL** backends (Linux/wasm) and **GTK** — out of the monomorphic
  Metal scope.

**G3b.** The crate remains `PORT STATUS: in progress` ⇒ G3b comment-coverage keeps
SKIPPING it (honest — the crate is not wholly ported). R1 for the ported slice is
backstopped by the adversary + verify panel + the golden gate (heavy-tier doctrine);
a scoped `check_comment_coverage.py --port-glob` run is available on the done slice
as an optional on-demand check.

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
- `surface.rs` — **2** (was 3; the `performBindingAction .search` arm now lands
  LIVE — see "RESOLVED (`.search` arm)" above): `activateInspector` /
  `deactivateInspector` (Phase-3 inspector, §4).
- `app.rs` — **8**, ALL inside `#[cfg(test)] mod test_runtime` (the §7
  TestSurface fake methods the C4 seam tests do not drive — honest test glue,
  NOT production leaves and NOT deferrals).
- `renderer_seam.rs` — **15**, the C3 dependency-placeholder bodies (below).

**OPEN core deferrals (roll-up — targets: Phase-3 inspector tier / Phase-3 R-d
GPU renderer / D6 libxev port):**

1. **Inspector activate/deactivate (verbatim-comment deferral, §4).**
   `Surface.activateInspector` (Surface.zig:890) + `deactivateInspector` (:915)
   `alloc.create`/`deinit` the UNINHABITED Phase-3 `inspectorpkg::Inspector`
   placeholder; bodies verbatim above `unimplemented!()`; `deinit`'s inspector
   teardown arm is `unreachable!()`-by-construction. **Target: Phase-3 inspector
   tier** (fills `rustty_inspector::Inspector` from the uninhabited enum).
   (The `keyCallback` inspector defer-block ports REAL — None-arm live, Some-arm
   unreachable until Phase 3, per §4.)

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

1. **The full `state.init()` port.** `rustty_apprt::init()` (the `rustty_init`
   C-export's body) is the E0 MINIMAL precursor: it sets `rustty_core::STATE` with
   the `()` allocator seam + `resourcesDir()` only, so the CAPI `global.alloc`
   readers (`ghostty_surface_free_text`/`_tty_name`/`Text.deinit`) have a live
   state. The FULL reference `global.zig` `init` body — GPA/allocator selection,
   CLI action detection (`cli.action.detectArgs`), logging config (`GHOSTTY_LOG`),
   `crash.init`, `internal_os.ensureLocale`, `i18n.init`, `initSignals` (SIGPIPE) —
   is the BREAK #1 deferred seam (`rustty_global::GlobalState::init` is commented,
   pending rustty-core wiring). **Target: the rustty-core/rustty-global `state.init`
   wiring milestone.**

2. **`std.os.argv` storage in `rustty_init`.** The reference `ghostty_init` does
   `std.os.argv = argv[0..argc]`; the stored argv is consumed only by the deferred
   CLI-action detection (item 1). `rustty_init` accepts `argv` (fixing the C-ABI
   signature now) but does not store it yet. **Target: with item 1.**

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
  `content_scale`, the read `cursorPosCallback` needs — the dcimgui style-scaling is
  elided as a documented leaf, method does NOT panic); `updateSize` (accepts the
  size; the imgui IO `DisplaySize` write is a dcimgui leaf, method does NOT panic —
  the Inspector has no Rust-side size field, `DisplaySize` lives in imgui's IO);
  `queueRender` (forwards LIVE to `Surface::queueInspectorRender`, the draw/refresh
  delegation precedent); `Surface::initInspector` (real heap-`Box` pin + LIVE call to
  the leaf `Inspector::init`).

Deferred out of E4 (all Phase-3 leaves; NO new capability dropped — R1; every body
preserved verbatim as comments, never fabricated):

1. **`Inspector::init` / `deinit` — dcimgui context + core inspector Phase-3 leaf.**
   `init` = `ImGui_CreateContext` + `CoreInspector.setup()` + `activateInspector`;
   `deinit` = `deactivateInspector` + `ImGui_DestroyContext`. `unimplemented!()`
   leaves. Because `init` is a leaf, **no live `Inspector` can be constructed this
   milestone** — `Surface.inspector` stays `None` in practice, so `freeInspector` /
   `Inspector::deinit` never actually run. **Target: Phase-3 R-d (dcimgui render path).**

2. **`Inspector::initMetal` / `renderMetal` + `Backend::deinit` — objc/Metal Phase-3
   leaf.** The two objc `objc.Object` handles → opaque `*mut c_void` (no objc2/coretext
   dep now — blueprint §5/§8.1); `ImGui_ImplMetal_Init`/`NewFrame`/`RenderDrawData` +
   the `MTLRenderCommandEncoder` msgSends + `ImGui_ImplMetal_Shutdown` are
   `unimplemented!()` leaves. **Targets of the E10 Darwin CAPI**
   `ghostty_inspector_metal_init`/`_metal_render`/`_metal_shutdown`. **Phase-3 R-d.**

3. **`Inspector::{mouseButtonCallback,scrollCallback,cursorPosCallback,focusCallback,
   textCallback,keyCallback}` + the private `newFrame` — dcimgui event/IO Phase-3
   leaves.** Each drives `cimgui.c.ImGuiIO_Add*Event` (+ `key.imguiKey()`, itself the
   already-deferred `Key::imgui_key` dcimgui binding in rustty-vt). `unimplemented!()`
   leaves; `textCallback`'s `[:0]const u8` → `&CStr` (NUL sentinel handed to imgui).
   `newFrame` is a private helper called ONLY by the deferred `renderMetal`, so it
   carries `#[allow(dead_code)]` until that render path goes live. **Targets of the E9
   CAPI** `ghostty_inspector_{mouse_button,mouse_scroll,mouse_pos,set_focus,text,key}`.
   **Phase-3 R-d.**

4. **`Surface::initInspector` reaches the `Inspector::init` leaf at RUNTIME.** Ported
   LIVE (real `Box::into_raw(Box::new(...))` heap-pin, mirroring the reference
   `alloc.create` + placement `.init`); the `?` on `Inspector::init` panics once a
   live inspector is requested (the E9 CAPI `ghostty_surface_inspector` seam). No new
   leaf — LIVE delegation to leaf item 1. The `Inspector` back-pointer to `Surface`
   is sound: the embedded `Surface` is itself heap-pinned (ADV-FIX 8).

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
  STUBBED.** `version.rs` emits documented STUB strings for the `- renderer` line
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
  fields convert exactly. **Non-macOS Fontconfig discovery** is still DEFERRED in
  the shaper (`discovery.rs`); on those targets both actions map the reference's
  `font.Discover == void` arm (print "no discovery mechanism", exit 1) — the macOS
  CoreText path is the live port (gates run on macOS).

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
self-consistent under both the default and `identity-ghostty` profiles. No new
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

## rustty-cli C5 (Phase-2 CLI milestone) — vaxis/zf-touching actions

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

- **`vaxis` + `zf` are unported Zig-source dependencies** (TTY UI + fuzzy match) —
  the same class as libxev (D6) and z2d, NOT ghostty src and NOT C libs. Every
  code path that needs them is 0-test and sits behind the `tui.can_pretty_print &&
  !plain && isatty` gate, so the plain/enumeration paths — the ONLY paths the KAT
  and any redirected pipe hit — land LIVE now and the render paths defer:
  - **`list_colors` `--plain`/non-tty path — LIVE.** Iterates `x11_color.map.keys()`
    (sorted `orderIgnoreCase`) and prints `name = #RRGGBB`. The `prettyPrint`
    (vaxis) branch is a deferred leaf (`unimplemented!()` + verbatim body/comments
    preserved). **Enabling change in `rustty-vt` (x11_color.rs):** the ported
    `x11_color` map stored keys *lowercased* (for case-insensitive `get`), which
    would corrupt `+list-colors`' displayed names (e.g. `GhostWhite`→`ghostwhite`).
    Added an additive `Map::keys()` returning the ORIGINAL-case names in rgb.txt
    order (a second lazy parse; `get`/`build_color_map` and the `x11 color map`
    test are untouched — rustty-vt stays 2019, x11_color.zig G3b stays missing=0).
  - **`list_keybinds` `--plain`/non-tty path — LIVE.** Streams
    `config.keybind.formatEntryDocs(entryFormatter("keybind", writer), opts.docs)`
    over the ported `rustty_cfg_fmt::EntryFormatter` (`--docs` rides the same
    `help_strings`/`NoHelpStrings` deferred seam already tracked for C1/C2 — no new
    deferral). The `prettyPrint` (vaxis) + its `ChordBinding`/`TriggerNode`/
    `iterateBindings` helpers are a deferred leaf (verbatim body/comments preserved).
  - **`list_themes` theme ENUMERATION — LIVE.** Built on the ported
    `config/theme.zig` `LocationIterator` + `Config.loadFile` + `shouldIncludeTheme`
    (luminance filter). The interactive vaxis+zf fuzzy-finder picker (`Preview` and
    all its methods, `preview`, `resolveAutoThemePath`/`writeAutoThemeFile`,
    `color`, `ThemeListElement.toUri`) is a deferred leaf — the reference body +
    comments (1552 lines) are preserved VERBATIM as a commented block; `preview` is
    an `unimplemented!()` marker. `lorem_ipsum.txt` (`@embedFile`, used only by the
    deferred `drawPreview`) is copied into the rustty tree (R3) only when the picker
    lands — NOT needed for the live enumeration.
  - **`boo` — full vaxis-deferred leaf.** A 100% vaxis/vxfw ghost animation (needs
    the `framedata` compressed asset too); `run` is an `unimplemented!()` marker and
    the whole reference (`Boo` widget, span-parser, flate decompression) is preserved
    verbatim as comments (comment-fidelity, scoped-G3b missing=0). Un-deferred with
    the `vaxis` port.

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
  glslang.init, oni.init, i18n.init) stay BREAK #1 deferred seams (preserved verbatim
  in `rustty_global::GlobalState::init`), wired as rustty-core / apprt land (Phase-3).

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

DEFERRED (R5 `unimplemented!()` seams — verbatim Zig body preserved as a leading
block comment at each; golden never runs any of them):

- **`Image::convert`** (image.rs) — the `wuffs.swizzle.{g,ga,rgb,bgr,bgra}ToRgba`
  pixel path. `wuffs` has **no Rust home** (grep: only a `rustty-font` comment
  references it) — the one HARD dep gap. NOTE (adversary fix): the `.rgba => return`
  early-out lives INSIDE `convert`'s body, so this whole-method seam swallows it —
  `Image::upload`/`prep_for_upload` are FULL wiring but ALWAYS reach this panic if
  ever run, even for already-RGBA data (e.g. the overlay). Golden-SAFE (empty map +
  `None` overlay + no `background-image` never reach `upload`). Un-seaming needs a
  pure-Rust g/ga/rgb/bgr/bgra→rgba swizzle (also unblocks the pre-Kitty `bg_image`
  path — `prepBackgroundImage` builds `Pending{.rgba}` and `uploadBackgroundImage`
  → `prep_for_upload` → `convert`; §7 OPEN RISK #1). Offscreen golden config MUST
  NOT set `background-image` (solid bg → `bg_color`, not a texture).
- **`State::{kitty_update, prep_kitty_placement, prep_kitty_virtual_placement,
  prep_kitty_image}`** (image.rs) — the Kitty terminal-walk (viewport pins, per-
  placement `rect`/`pixelSize`/`pointFromPin`, `unicode.placementIterator`/
  `renderPlacement`, z-sort, bg/text split). Never run by the no-image golden;
  feeds only each other + `kitty_placements`.
- **`Overlay::{highlight_hyperlinks, highlight_semantic_prompts, highlight_grid_rect,
  highlight_pixel_rect}`** (overlay.rs) — the four z2d CPU render leaves. Doubly
  forced: (1) the ported z2d `Context` subset lacks `set_anti_aliasing_mode`/
  `set_hairline`; (2) the `RenderState` row/cell `.hyperlink`/`.semantic_*` metadata
  is not yet exposed; (3) no overlay feature is enabled by the golden.

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

**Deferred leaf (INHABITED-placeholder doctrine, → W3):** `Renderer::loop_enter`
is filled and faithful, but its transitive `self.api.loop_enter()` (metal_impl.rs) stays
`unimplemented!("windowing-host: Metal::loop_enter installs the IOSurfaceLayer display
callback")` — it needs a live NSView layer (W3). The method compiles and is faithful;
it PANICS if called until W3 fills the Metal hook. The KAT does NOT call
`Renderer::loop_enter` — it drives the `DisplayLink` primitive + the callback→`Async`
seam directly, so the deferred Metal leaf never gates the vsync proof. `loop_exit` has
no Metal hook (`@hasDecl(Metal,"loopExit")` false) and is fully runnable.

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

No other W3 E2 deferrals: `cursorBlinkInterval`'s valgrind `* 5` branch is kept
verbatim-inert (no `std.valgrind` shim, in-place comment + PORT NOTE, not a
DEFERRALS entry); the `MUST_DRAW_FROM_APP_THREAD` app-mailbox `drawFrame` arm is
dead-but-live (`false` this milestone, flips when GTK lands) — both arms compile,
no `unimplemented!`.

---

# Ledger-hygiene reconciliation (2026-07 — GAP_ANALYSIS drift backlog §6)

The gap-analysis completeness critics found several real deferrals with NO ledger
entry, plus one app-core over-claim. These entries reconcile that drift. Each
maps to a GAP_ANALYSIS §3 workstream ("sub-project"); the numbering below is the
sub-project id the critics used (macos-swift-app = #2, unused-subset = #3, gtk = #4).

## Sub-project #3 (unused-subset): `src/synthetic/**` — differential-fuzzing generators

**Ghostty source:** `src/synthetic/**` — 9 `.zig` files, ~1157 LOC
(`Generator.zig`, `Bytes.zig`, `Osc.zig`, `Utf8.zig`, `cli.zig`, `main.zig`,
`cli/**`). **Rustty counterpart:** only a 9-line scaffold in
`crates/rustty-synthetic/src/lib.rs` (crate exists, body unported).
**Why deferred:** these are the differential-fuzzing input generators — a
developer/CI tool, NOT reachable from the terminal product; ghostty itself does
not link them into the app. Headless-feasible (no GPU/GTK/windowing dependency).
**What unblocks it:** slated for sub-project #3 (unused-subset, GAP_ANALYSIS §3.8).
Port the generators into `rustty-synthetic` when the unused-by-ghostty subset
milestone runs. 0-test impact on the product crates.

## Sub-project #3 (unused-subset): `src/benchmark/**` + `main_bench.zig` + `ghostty_benchmark_cli`

**Ghostty source:** `src/benchmark/**` (12 files: `Benchmark.zig`, `CApi.zig`,
`CodepointWidth.zig`, `GraphemeBreak.zig`, `IsSymbol.zig`, `OscParser.zig`,
`ScreenClone.zig`, `TerminalParser.zig`, `TerminalStream.zig`, `cli.zig`,
`main.zig`, `options.zig`) + `src/main_bench.zig`, together ~1441 LOC; plus the
`export fn ghostty_benchmark_cli` C-ABI entrypoint (`src/benchmark/CApi.zig:8`).
**Rustty counterpart:** `crates/rustty-bench` scaffold only (Cargo.toml + stub
`lib.rs`). **Why deferred:** the micro-benchmark harness is a developer tool, not
part of the shipping terminal; the `ghostty_benchmark_cli` export is not on the
71-symbol libghostty product contract. Headless-feasible. **What unblocks it:**
sub-project #3 (unused-subset, GAP_ANALYSIS §3.8) — port `rustty-bench` and its
`rustty_benchmark_cli` export alongside the synthetic generators. 0-test impact.

## Sub-project #3 (unused-subset): `src/extra/{bash,fish,zsh,vim,sublime}.zig` — shell-completion / editor-syntax generators

**Ghostty source:** `src/extra/bash.zig`, `fish.zig`, `zsh.zig`, `vim.zig`,
`sublime.zig` — ~941 LOC of shell-completion + editor-syntax string constants,
emitted by `main_build_data.zig`'s `+gen-*` actions (the `bash`/`fish`/`zsh`/
`sublime`/`@"vim-syntax"`/`@"vim-ftdetect"`/`@"vim-ftplugin"`/`@"vim-compiler"`
gen switch). **Rustty counterpart:** NONE (no crate, no scaffold). **Why
deferred:** build-time codegen data for packaging (shell completions, editor
syntax files) — not reachable from the running terminal; driven only by the
`main_build_data` gen tooling (itself deferred, see C6). **What unblocks it:**
sub-project #3 (unused-subset, GAP_ANALYSIS §3.8), together with the
`main_build_data` gen-action tooling that consumes them. 0-test impact.

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
