# rustty-vt — Tracked Deferrals (must be completed by the named sub-batch)

Hard tracker for code/tests/comments deferred from an earlier sub-batch because a
dependency wasn't ported yet. Each MUST be completed by its target sub-batch — the
target workflow consumes this file. Nothing here is "dropped" (R1); it is scheduled.

> **READ THIS BEFORE GREPPING FOR "Phase-3" [WS19 T2, 2026-07-29 — audit row W10].**
> The hyphenated token **`Phase-3`** in this file is the **pre-workstream CODENAME**
> for the GPU-renderer / xev / inspector tiers — **all of which have since LANDED**
> (windowing `W0`–`W5`, xev Milestones `A`/`B`, `WS6` for the inspector/dcimgui).
> It is **NOT** `.port/phase-3/` (the upstream-resync phase), which is what "Phase 3"
> means today. Grepping this file's `Phase-3` labels to build a Phase-3 worklist
> yields ~40 **false positives**. WS19 T2 retargeted every tier-qualified label to the
> owning tier with its landing evidence (`[LANDED: …]`); the bare narrative mentions
> that remain are history, not open work. The authoritative index of what is actually
> open, and who owns it, is the `# PHASE 2 CLOSURE AUDIT (WS19)` section at the end of
> this file, and the standing invariant is gate **G-P2**
> (`.port/tools/gates/check_phase2_closure.py`).

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

   > **OWNER [WS19 T19, 2026-07-29]: Phase 3** (upstream resync) — added because gate
   > **G-P2** flagged this row as an open claim with a TRIGGER but NO OWNER, which is
   > exactly the shape that produces orphans. It is **upstream-conditional, not
   > deferred**: the reference itself carries "TODO: Before shipping an ABI-compatible
   > libghostty, verify this" and "This was just arbitrarily chosen for now", so the
   > `[16]u64`=128 verification cannot be written against a layout upstream has not
   > committed to. **Phase 3 assertion:** if the resync range contains a commit that
   > fixes `stream.Action`'s `c_union` layout, port the const-assert with it; if it does
   > not, this row stays as-is and remains correct. No rustty-side work is possible
   > before that.


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

**RESIDUAL — NONE. The last bullet is resolved:**
- ~~the **WebGL** backend (wasm) — out of the monomorphic Metal scope; a
  whole-backend follow-up port, no in-crate `unimplemented!()`. (Metal = macOS and
  OpenGL3 = GTK/Linux are both done.)~~ **RESOLVED — WS14, 2026-07-30. The premise
  was wrong: there is no WebGL backend upstream.** The reference
  `src/renderer/WebGL.zig` is a **2-line placeholder** (a doc comment plus
  `pub const WebGL = @This();`) — no fields, no methods, no shaders, no draw path,
  no tests — and it is byte-identical on the oracle's `origin/main`, so this is not
  a pin artifact. Every previous re-derivation of this row measured the PORT
  (`grep -rni 'webgl'` over `crates/`) and never the ORACLE, which is how "no
  backend implementation in the port" was read as "a backend is owed" for months:
  the port had exactly as much WebGL as upstream does. It is now ported verbatim at
  `crates/rustty-renderer/src/webgl.rs` (a unit struct + the file's doc comment,
  with a `size_of == 0` reference test), and `renderer.zig`'s comptime switch has
  all three arms — `#[cfg(target_arch = "wasm32")] pub use webgl::WebGL as Renderer`.
  Inventing a WebGL draw path here would be an **R1 violation** (no reference to be
  faithful to). Actual browser drawing lives where the reference's own wasm model
  puts it — in the consumer: `example/wasm-webgl/` uploads the `Atlas.Wasm` texture
  as an `R8` WebGL2 texture and draws one quad per glyph, gated by
  `.port/tools/gates/ws14_webgl_browser_gate.sh` (per-cell ink counts vs a CPU
  expectation computed from the module's own atlas bytes; two negative controls
  that MUST fail every run; opt-in CI lane `ci.sh --webgl`).
  See `docs/superpowers/plans/2026-07-23-ws14-wasm-webgl.md`.

  > **OWNER [WS19 T14, 2026-07-29 — audit row O16]: WS14 (wasm/WebGL).** ~~This row previously named "out of the monomorphic Metal scope" with no owner, which is not a workstream, so it was ORPHANED.~~ Gate **G-P2** (`.port/tools/gates/check_phase2_closure.py`) now FAILS on any open row without an owner in an OPEN workstream, so this attribution is enforced rather than aspirational. **CLOSED by WS14 (2026-07-30)** — the "L" size estimate the WS19 audit carried for O16 was itself a product of the oracle never being read; the real task was ~15 lines.


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
~~the DCS↔tmux `.tmux` enter/exit arm in `stream_handler::dcsCommand`~~ **[STRUCK
2026-07-31, `fix-freetype-tmux`: the vt-side `TmuxCommand = ()` stub is long gone;
the arm is now ported and KAT'd — see the `dcsCommand` tmux arm row below]**; and the
Windows stubs (`Subprocess::start` windows spawn, `ReadThread::
threadMainWindows`, `Backend::childExitedAbnormally` — the last uninstantiated in
the reference via Zig lazy compilation), all `#[cfg]`/const-false and unrelated to
the macOS live-shell milestone. **[WS20] `killCommandFlatpak` STRUCK from this
list — it is now the real `command.signal(SIGHUP, true)` body behind the
`rustty-termio` `flatpak` feature; see the Flatpak row below.** The remaining
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

- **GTK growth point (`RtPreExecInfo`/`RtPostForkInfo`) — FILLED (V13,
  branch `ws20-v13-preexec-wiring`).** The reference's comptime
  `@hasDecl(apprt.runtime, "pre_exec"/"post_fork")` branches are realized ONLY
  by the GTK apprt. Both types now carry the reference's REAL GTK fields
  (`pre_exec.zig:11-23`, `post_fork.zig:15-31`) and their `init(&Config)` copies
  them; `rustty_apprt::gtk::{pre_exec::PreExecInfo, post_fork::PostForkInfo}`
  are re-exports OF these types, which is the reference's comptime identity
  established from the lower crate (R4 forbids rustty-termio ->
  rustty-apprt). The hook FUNCTIONS are installed from above by
  `gtk::App::init` via `command::install_rt_hooks`, read back by
  `exec.rs` at the `Command` construction site — the one divergence from the
  comptime branch, documented in full in the seam PORT NOTE above
  `command::RtPreExecInfo`. Still NOT in rustty-apprt-api (blueprint §1).
  0 reference tests affected (3 rustty-margin KATs added).
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
- ~~**Flatpak comptime-false branches (exec.rs).** `build_config.flatpak` is~~
  ~~const `false` ... they re-bind to rustty-os's FlatpakHostCommand with the~~
  ~~Linux/flatpak tier.~~ **STRUCK — LANDED (WS20).** `build_config.flatpak` is the
  REAL `-Dflatpak` ghostty build option, so treating it as a `const false` dead
  branch was wrong. It is now the `rustty-termio` **`flatpak` cargo feature**
  (`flatpak = ["rustty-os/flatpak"]`, OFF by default exactly as `-Dflatpak`
  defaults to false), and with it ON:
  - `FlatpakHostCommand` / `.Completion` / `.WaitError` bind the REAL
    `rustty_os::flatpak` types instead of the zero-size fallback (the `if
    (!build_config.flatpak) struct{...} else internal_os.FlatpakHostCommand`
    selection, both arms live);
  - `killCommandFlatpak` is the reference body `command.signal(SIGHUP, true)` —
    it was `unimplemented!()`;
  - the other three flatpak arms are filled: `Subprocess.start`'s cwd
    test-spawn, `Subprocess.start`'s HostCommand spawn (+ its
    `errdefer killCommandFlatpak`), `Subprocess.stop`'s `.flatpak` kill+wait, and
    `threadEnter`'s `else if (comptime build_config.flatpak)` arm wiring
    `cmd.waitXev(...)` to the new live `flatpakExit` callback.
  - **`rustty-os`'s own `flatpak` feature had never compiled** and was NOT
    "fully ported with zero stubs": it bound `rustty_global::xev` (the `()`
    anchor) instead of `rustty-xev`; `spawn` never spawned the thread
    (`let _ = threadMain;`); `wait` never waited (a `break` + `unreachable!()`);
    `onExit` dropped the reference's `if (state.completion)` xev-completion wake;
    and the `@fieldParentPtr("c_xev", ...)` recovery called a nonexistent
    `xev::completion_parent`. All five are fixed, plus `state` moved inside the
    `Mutex` that already documented it as guarded (Zig's mutex-by-convention over
    a plain field is a data race in Rust). `rustty-os` gains an OPTIONAL
    `rustty-xev` dep behind the feature — acyclic (rustty-xev deps are
    `{rustty-global, libc}` only). Verified: `cargo check`/`clippy -D warnings`
    GREEN for `-p rustty-os --features flatpak` and `-p rustty-termio --features
    flatpak --all-targets`.
  - **Aliasing:** the one new cross-thread raw-pointer handoff (`spawn` handing
    `self` to the detached `threadMain`, plus the GIO `onExit` callback recovering
    it from an opaque `gpointer`) is proven Stacked- AND Tree-Borrows clean on the
    isolated harness `.port/phase-2/mirihar-ws20-flatpak/` — with a NEGATIVE
    CONTROL (`-- --unsound`, the first draft's `&mut self` + `&mut *ptr` shape)
    that Miri rejects under both models, so the green arm is non-vacuous. The cure
    is the same discipline the xev lane established: no `&mut` to the shared
    object anywhere (every method takes `&self`; all mutation goes through
    `state_mutex`), so there is no exclusive tag a foreign thread can invalidate.
  - **Remaining (NOT a scope exclusion):** the feature cannot be EXECUTED on this
    host — it links GIO/GLib and needs a live flatpak session bus. That is
    verification debt with a runnable recipe (VERIFICATION_DEBT.md V10), owned by
    WS20.
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
- **~~`dcsCommand` tmux arm (stream_handler.rs).~~ RESOLVED [2026-07-31,
  `fix-freetype-tmux`].** The stated blocker — rustty-vt dcs.rs's
  `TmuxCommand = ()` stub — is gone: `dcs::Command::Tmux` carries a real
  `tmux::ControlNotification` (`dcs.rs:333`) and `tmux::Viewer::next` is ported
  (`tmux/viewer.rs:441`). Since `TMUX_ENABLED` is TRUE in the shipped binary
  (rustty-core enables `rustty-vt/oniguruma`), the empty arm was live-reachable
  dead behaviour: entering tmux control mode built no viewer, sent tmux no
  commands, and never tore the viewer down. All three reference prongs
  (`.enter` / fallthrough→`viewer.next` + `messageWriter` / `.exit`) are now
  ported 1:1 with three Rust-only seams recorded at the site (labeled block for
  `tmux: {}`, `mem::replace` move-out of the owned notification, and take/restore
  of the `Box<Viewer>` across the `&mut self` mailbox writes). KAT:
  `stream_handler_dcs_tmux_arm_drives_the_viewer`, which asserts BOTH build
  configurations (option off → the arm is inert; option on → all three prongs).
  A new default-off `rustty-termio/oniguruma` feature forwards the vt option so
  the live half is runnable: `cargo test -p rustty-termio --features oniguruma`.
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
R1-safe; owner: the the inspector tier [LANDED: WS6] port):

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
Owner of the fill: the the R-d GPU/renderer tier [LANDED: windowing W0–W5] GPU renderer tier + the D6 libxev port.

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
  the R-d GPU/renderer tier [LANDED: windowing W0–W5] + D6 work that lands WITH the real port (renderer/Thread.zig
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

- **~~DEFERRED~~ RESOLVED [2026-07-31, `fix-freetype-tmux`]: freetype
  load flags through the break-#9 DI bridge.** The `.font = try
  font.SharedGridSet.DerivedConfig.init(alloc, config)` line (Surface.zig:
  368) is realized as core-local glue (`font_shared_grid_set_derived_config`)
  because the reference init is duck-typed over the GLOBAL config while
  rustty-font-shaper's `DerivedConfig::init` takes its break-#9 subset type;
  core is the first crate that sees both. All fields convert exactly (both
  sides model the SAME reference config types; `Modifier`/`CodepointMap` are
  literally shared rustty-font types).
  **The `freetype-load-flags` field is no longer excepted.** The premise of the
  deferral was false: `face::FreetypeLoadFlags` on the freetype-active backends
  is a `pub use` of `face::freetype::FreetypeLoadFlags`, the REAL 5-field
  stand-in for `config.FreetypeLoadFlags` — not a `_private` placeholder — so
  nothing was blocking. `surface.rs` now assigns
  `derived_freetype_load_flags(&config.freetype_load_flags)`, the exact
  `if (font.face.FreetypeLoadFlags != void) config.@"freetype-load-flags"
  else {}` selector (SharedGridSet.zig:528), with the field-for-field DI bridge
  `freetype_load_flags_from_config`. Two consequences of the old hard-coded
  default: (1) the user's setting did nothing on every freetype build, and
  (2) `SharedGridSet::Key::hash` skipped the field entirely, so two configs
  differing only in `freetype-load-flags` collided on the same cached grid —
  both fixed (`shared_grid_set.rs` now hashes it; `FreetypeLoadFlags` derives
  `Hash`; the `()` arm hashes nothing, matching `autoHash` of a `void` field).
  4 tests (`surface.rs` `freetype_load_flags_tests`) + a macOS compile-time
  zero-size assertion on the `void` arm. **Residual:** the KAT at the exact bug
  site (`derived_config_carries_the_configured_flags`) is
  `cfg(not(any(macos, wasm32)))` — on macOS the reference itself takes the
  `void` arm, so there is nothing to thread and nothing to assert; it has never
  been compiled or run on a host (no Linux target installed). The
  host-runnable tests cover the DI bridge and the hash discrimination.
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
  precedent). Resolved by the the inspector tier [LANDED: WS6] fill.
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
- ~~`renderer_seam.rs` — **15**, the C3 dependency-placeholder bodies (below).~~
  **RESOLVED / STALE [WS19 T2 audit, 2026-07-29 — audit row W1]** All 15 sit inside
  `#[cfg(test)] mod compression_tests` (`renderer_seam.rs:2013`), each reading
  `unimplemented!("test runtime: not driven by the compression tests")` — test-runtime
  fakes, the same honest test glue as `app.rs`'s 8. The production bodies are LIVE
  (`Renderer::{surfaceInit,init,finalizeSurfaceInit,attachHostView,threadEnter,drawFrame,displayRealized,displayUnrealized,deinit}`
  at `:242,:269,:321,:337,:361,:390,:415,:439,:454`; `Thread::{init,threadMain,deinit}`
  at `:669,:875,:939`). **Zero production leaves.**

**OPEN core deferrals (roll-up — targets: the inspector tier [LANDED: WS6] tier / the R-d GPU/renderer tier [LANDED: windowing W0–W5]
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

3. ~~**`renderer_seam.rs` dependency placeholders (C3, §5).** Inhabited structs
   with `unimplemented!()` bodies (termio `xev_seam` precedent; no reference
   logic stubbed):~~
   ~~- `Renderer` (generic.zig GenericRenderer) — `surfaceInit`/`init`/
     `finalizeSurfaceInit`/`threadEnter`/`drawFrame`/`deinit` + the
     `Renderer.DerivedConfig` placeholder. **Target: the R-d GPU/renderer tier [LANDED: windowing W0–W5].**~~
   ~~- `Thread<A>` run loop — faithful renderer/Thread.zig field shape (rustty-xev
     handles + REAL state/mailbox); `init`/`threadMain`/`deinit` unimplemented.
     **Target: the R-d GPU/renderer tier [LANDED: windowing W0–W5] + D6.**~~
   (`renderer_seam::Options<A>` is LIVE, not deferred.)

   **RESOLVED / STALE [WS19 T2 audit, 2026-07-29 — audit row W1].** The GPU tier
   (W0–W5) and the xev port (Milestones A/B) both landed; the production
   `Renderer` and `Thread<A>` bodies listed above are LIVE at the line numbers
   given in the struck bullet at `renderer_seam.rs`'s leaf roll-up above. The only
   remaining `unimplemented!()` in the file are the 15 `#[cfg(test)] mod
   compression_tests` fakes. NOTE the "Phase-3" in the struck **Target:** lines is
   the pre-WS codename for the GPU/xev tiers, not `.port/phase-3/` (upstream
   resync) — see the "Phase-3" note at the top of this file.

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

   > **RESOLVED / STALE [WS19 T19, 2026-07-29]** — flagged by gate G-P2 as an open claim
   > with no owner. **D6 LANDED**: the libxev port completed as xev Milestones A/B
   > (`crates/rustty-xev`; `thread_pool.rs` alone is 1630 lines with 0
   > `unimplemented!()`, audit row W5), and `rustty-xev` is a real crate, not a
   > placeholder. The struck **Target:** names "the D6 libxev port", which is the
   > pre-WS codename for work that is complete — see the "READ THIS BEFORE GREPPING"
   > note at the top of this file.


5. ~~**App.zig Wasm block (C4, in-file).** `App.zig:579 pub const Wasm` is
   wasm-target-gated and commented out IN the reference; preserved verbatim at
   the bottom of app.rs. **Target: the rustty wasm app-runtime**~~ — **OWNER [WS19 T14, 2026-07-29]: WS14 (wasm/WebGL)**; "the rustty wasm app-runtime" is a milestone name, not a workstream, so these rows were ORPHANED. Gate **G-P2** now FAILS on any open row whose owner is not an OPEN workstream (off-by-default
   `wasm` feature family).

   > **RESOLVED — WS14, 2026-07-30 (audit row O15).** Ported LIVE, and it is
   > *complete precisely by being empty*. `App.zig:577-604` is
   > `pub const Wasm = if (!builtin.target.isWasm()) struct {} else struct { … }`,
   > and on the wasm arm the struct's ONLY live members are two private aliases
   > (`wasm`, `alloc`) — all three exports (`app_new`, `app_new_`, `app_free`) are
   > **commented out IN THE REFERENCE**. The port is
   > `crates/rustty-core/src/app.rs`'s `#[cfg(target_arch = "wasm32")] pub mod wasm`,
   > carrying the two aliases and the commented-out bodies verbatim, so a future
   > un-comment upstream is a diff rather than archaeology.
   >
   > **Consequence, and it is the load-bearing one:** because retaining this block
   > emits **zero symbols**, `crates/rustty-wasm` needs **no `rustty-core`
   > dependency edge** to be a faithful `main_wasm.zig`. The edge the port used to
   > carry was not merely heavy but UNFAITHFUL — it over-retained an entire
   > application tier and dragged `rustty-inspector → dcimgui-sys → freetype-sys →
   > zlib-sys` into a freestanding wasm link. Dropping it (WS14 T7) is the *more*
   > faithful choice, not a concession.

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
   inert stubs and `rustty_init` is runtime-live). **Targets: the R-d GPU/renderer tier [LANDED: windowing W0–W5] (GPU) +
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
   (`queueInspectorRender` = Inspector, the dcimgui tier [LANDED: WS6]/Metal). `App::newSurface`
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

1. ~~**`Inspector` E2→E4 SEQUENCING STUB (the dcimgui tier [LANDED: WS6]/Metal leaf).**~~
   **RESOLVED — verified 2026-07-30 (WS20 ledger pass).** The row was FALSELY OPEN.
   `crates/rustty-apprt/src/embedded.rs` contains exactly ONE real `unimplemented!()`
   call site — `:2275` `renderMetal`, which is Darwin-only and unreachable off macOS
   (a faithful platform arm, not a seam). The `Inspector` `deinit` leaf this row
   describes no longer exists; the dcimgui tier landed in WS6 as the row's own
   `[LANDED: WS6]` annotation already noted. Evidence:
   `grep -n 'unimplemented!(' crates/rustty-apprt/src/embedded.rs` → one code hit
   (`:2275`); every other hit is a doc comment.
   Original text follows for provenance:
   ~~`Surface`
   carries `inspector: Option<*mut Inspector>` and `deinit` calls `freeInspector`,
   so the `Inspector` type must exist NOW. E2 defines a MINIMAL opaque `Inspector`
   (`#[allow(dead_code)]`) whose `deinit` is an `unimplemented!()` leaf; `deinit`/
   `freeInspector` are ported faithfully but the `if let Some(v) = self.inspector`
   arm is a no-op while `inspector` is always `None` (nothing constructs an
   `Inspector` until **E4**'s `initInspector`, which needs the dcimgui/Metal
   context). **Target: E4, EMBEDDED_BLUEPRINT §7** (`Inspector`, embedded.zig
   992-1243). Chunk-sequencing + Phase-3 leaf, not a permanent gap.

2. ~~**`Surface::queueInspectorRender` — E3/E4 `unimplemented!()` leaf (carried from
   E1).**~~ **RESOLVED — verified 2026-07-30 (WS20 ledger pass).** Also FALSELY OPEN:
   `queueInspectorRender` is IMPLEMENTED at `embedded.rs:1687` and dispatches the
   `render_inspector` action through the same `performAction` host-callback seam every
   other apprt action routes through; its live call site is `embedded.rs:532`. There is
   no `unimplemented!()` anywhere in that region. Evidence: `sed -n '1685,1705p'` shows
   zero `unimplemented!` occurrences.

3. **GPU/xev runtime leaves reachable THROUGH E2's `init`/`deinit`.**
   `Surface::init` calls core `Surface::init` (surface.rs) which constructs the
   `renderer_seam::{Renderer,Thread}` Phase-3 placeholder leaves (blueprint §1/§3),
   and `deinit` calls the core `Surface::deinit` renderer/io-thread teardown. These
   **compile + link now** but PANIC at the renderer leaf the moment a LIVE surface
   is created — which requires `ghostty_surface_new` (**E7**) AND the renderer land
   (**the R-d GPU/renderer tier [LANDED: windowing W0–W5] + D6**). No E2 gate exercises a live surface (the smoke KAT, E11,
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

1. **`draw`/`refresh` reach the the GPU tier [LANDED: W0–W5] renderer leaf at RUNTIME.**
   `Surface::draw`/`refresh` delegate to core `Surface::draw`/`refreshCallback`,
   which drive the `renderer_seam` Phase-3 `unimplemented!()` leaves (blueprint §1).
   The delegation is ported LIVE (byte-faithful) — the panic is in the leaf, not
   here; it fires only once a LIVE surface exists (`ghostty_surface_new`, **E7**) AND
   the renderer lands (**the R-d GPU/renderer tier [LANDED: windowing W0–W5] + D6**). Same honest line as E2 item 3 — E3
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
`renderer_seam::{Renderer,Thread}` (surface.rs:324/333 — the the R-d GPU/renderer tier [LANDED: windowing W0–W5] GPU + D6 xev
leaves, already in the core C3 deferral set above). So the whole E7 surface set
**COMPILES + LINKS + header-generates NOW and PANICS when driven against a real
surface**. E7 adds **NO new `unimplemented!()` leaf in apprt-c** — it is a pure FFI
projection that REACHES the existing renderer/xev leaves at runtime:

1. **`rustty_surface_new`** → `App::newSurface` → `Surface::init` → core
   `Surface::init` constructs `renderer_seam::{Renderer,Thread}` (the R-d GPU/renderer tier [LANDED: windowing W0–W5]/D6
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
(created via `rustty_surface_new`, the GPU-tier-leaf [LANDED: W0–W5] boundary — core `Surface::init`
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
(created via `rustty_surface_new`, the GPU-tier-leaf [LANDED: W0–W5] boundary — the R-d/D6 leaves in
the core C3 deferral set). The surface-action + clipboard exports are pure FFI
marshaling once a surface exists (the split exports reach the host `action` callback;
`_binding_action` runs real core logic; `_complete_clipboard_request` runs the real E3
backend, reaching the host `confirm_read_clipboard` callback on the unsafe/unauthorized-
paste arm). The **inspector per-callback exports additionally reach the E4 `Inspector`
`unimplemented!()` leaves** (dcimgui `ImGuiIO_Add*Event`, `Inspector::init`): a LIVE
inspector cannot yet exist because `_surface_inspector` → `Surface::initInspector` →
`Inspector::init` is a the dcimgui tier [LANDED: WS6] leaf, so `_inspector_{mouse_button,mouse_pos,
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
   (they land as live `extern "C"` fns WITH the leaf). NOT in the runtime-safe
   list; the E11 smoke KAT MUST NOT call it. ~~**Target: Phase-3 (macOS objc/CGS glue).**~~
2. **`rustty_surface_set_display_id` — renderer/xev leaf.** Pushes a `.macos_display_id`
   message to `renderer_thread.mailbox` + `wakeup.notify()`; both the renderer thread
   and its xev `wakeup` are placeholders. `unimplemented!()` leaf. **Target: the R-d GPU/renderer tier [LANDED: windowing W0–W5]
   (renderer) + D6 (xev).**
3. **`rustty_surface_quicklook_font` — objc/coretext + renderer leaf.** Reaches the
   renderer `font_grid` primary CoreText face + `copyAttribute`/`copyWithAttributes`
   (coretext). `unimplemented!()` leaf. **Target: the R-d GPU/renderer tier [LANDED: windowing W0–W5] (renderer) + coretext.**
4. **`rustty_surface_quicklook_word` — renderer_state live-surface leaf.** Real terminal
   logic (`getCursorPos`/`posToViewport`/`pages.pin`/`selectWord` + the E7 `readTextLocked`
   helper) under `renderer_state.mutex` — grouped with the Darwin block per the blueprint,
   ported as an `unimplemented!()` leaf (reaches the live-surface renderer_state seam).
   **Target: the R-d GPU/renderer tier [LANDED: windowing W0–W5] (renderer_state).**
5. **`rustty_inspector_metal_init` / `_metal_render` / `_metal_shutdown` — delegate to
   the E4 backend leaves (NO new apprt-c leaf).** The reference bodies are pure
   delegations, so the wrappers forward to the already-landed E4 backend methods
   `Inspector::{initMetal, renderMetal}` + `Backend::deinit` (DEFERRALS.md E4 item 2,
   which named these three as "Targets of the E10 Darwin CAPI"). `_metal_render` catches
   + logs the error exactly as the reference `catch |err| { log.err(...); return; }`;
   `_metal_shutdown` is a faithful `if let Some(v) = ptr.backend { v.deinit(); ptr.backend
   = None }` (a no-op this milestone, since `initMetal` is a leaf so `backend == None`).
   The panic is in the E4 backend leaf. **Target: the R-d GPU/renderer tier [LANDED: windowing W0–W5] (objc/Metal + dcimgui).**

**ALL FIVE ITEMS ABOVE ARE RESOLVED / STALE [WS19 T2 audit, 2026-07-29 — audit row
W2].** This is the second copy of the "apprt-c has exactly 4 `unimplemented!()`
code-leaves" claim (the first is the struck E11 leaf-audit paragraph earlier in this
file). Verified against code: `grep -rn 'unimplemented!' crates/rustty-apprt-c/src/`
→ **6 hits, every one inside a comment**; zero live leaves. All five exports are live
in `crates/rustty-apprt-c/src/lib.rs`: `rustty_set_window_background_blur` `:2515`
(real `objc2` `msg_send!` + the two private CGS `extern "C"` decls, no longer
comments), `rustty_surface_set_display_id` `:2629`, `rustty_surface_quicklook_font`
`:2674`, `rustty_surface_quicklook_word` `:2811`, `rustty_inspector_metal_init`
`:2929`. Closed by P2.1 / P2.2 / WS2 / WS6 plus the W0–W5 GPU tier. The struck
**Target:** lines say "Phase-3", which in this file is the pre-WS codename for those
tiers — see the note at the top of this file.

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

   > **RECLASSIFIED as a bounded TEST-SHIM accommodation, not open work
   > [WS19 T19, 2026-07-29].** Flagged by gate G-P2: "config C-ABI header (own
   > milestone)" is a milestone name, not an owner. What is actually being described is
   > that `smoke.c` — a TEST fixture — forward-declares 3 config functions instead of
   > including a generated `rustty-config.h`. The SHIPPED artifacts are unaffected:
   > `rustty-config` already exports the `rustty_config_*` symbols, and every published
   > header (`rustty.h`, `rustty-apprt.h`, `rustty-embed.h`) now compiles standalone as
   > C11 and C++17 under gate A5 (WS19 T3). A forward declaration in a test that links
   > the real library is a normal C practice, and the A4/A5 ABI gates bind the symbols
   > regardless of which header declares them. **If a config header is ever wanted for
   > its own sake, the trigger is a CONSUMER asking for one** — at which point it is a
   > cbindgen invocation in `xtask` beside the existing two. No owner is needed for a
   > test-fixture convenience.

4. **Cross-crate ABI-pin const-asserts** (`capi.rs`, the E11 audit): `Options`==64,
   `SurfaceOptions`==88, `EnvVar`==16, `PlatformC`==8, `TargetC`==16, `ActionC`==32 +
   the enum-width pins — the drift gate that keeps the hand-pinned header honest.

~~**Leaf audit (E11 item 5) — apprt-c has exactly 4 `unimplemented!()` code-leaves, ALL
E10-created, ALL documented above:** `rustty_set_window_background_blur` (macOS
objc/CGS), `rustty_surface_set_display_id` (renderer/xev), `rustty_surface_quicklook_font`
(objc/coretext + renderer), `rustty_surface_quicklook_word` (renderer_state). The
inspector `_metal_*` exports delegate to the E4 backend leaves (no apprt-c leaf). E11
adds none.~~

**RESOLVED / STALE [WS19 T2 audit, 2026-07-29 — audit row W2].** `grep -rn
'unimplemented!' crates/rustty-apprt-c/src/` → **6 hits, every one inside a comment**;
zero live leaves. `rustty_set_window_background_blur` is LIVE at
`crates/rustty-apprt-c/src/lib.rs:2515` (real `objc2` `msg_send!` + CGS). The other
three were closed by P2.1 / P2.2 / WS2 / WS6. The inspector `_metal_*` note still holds.

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
  `version_plus` KAT now ASSERTS their content. The OSC-8 commit-hash hyperlink
  is ✅ RESOLVED too (2026-07-29, WS19 T12, audit row O12) — see the RESOLUTION
  note at the end of this entry. NOTHING in this entry is open.**
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

  **RESOLUTION of the OSC-8 hyperlink half — IMPLEMENTED, not diverged (2026-07-29,
  WS19 T12, audit row O12).** Weighed implement-vs-divergence honestly and
  implemented: the repo URL already EXISTS as a first-class product-identity value
  (the workspace `Cargo.toml` `repository = "https://github.com/AirelyDev/rustty"`),
  and the hyperlink is user-observable R1 behavior, so there was no honest
  4-criterion basis for making it permanent. Three pieces:
  1. `rustty_identity::COMMIT_URL_PREFIX` — `https://github.com/AirelyDev/rustty/commit/`,
     with the oracle spelling (`github.com/ghostty-org/ghostty/commit/`) under
     `identity-oracle`, both arms asserted by the existing profile tests (R2).
  2. `crates/rustty-cli/build.rs` gains a third generator baking
     `RUSTTY_COMMIT_HASH` from `git rev-parse --short HEAD` — the `+BUILD`-metadata
     channel Rust's `SemanticVersion` view lacks. **R3-clean:** the oracle tree is
     not consulted; it reads *this* repo's git. **Reproducibility-clean:** the value
     is a function of the source revision (same commit → same bytes), the
     `RUSTTY_COMMIT_HASH` env var overrides it so a packager can pin or blank it
     without git, and a non-checkout build (tarball / vendored crate / docs.rs)
     degrades to EMPTY → `commit_hash() == None` → no hyperlink, which is exactly
     the reference's own `build_config.version.build == null` behavior rather than a
     build failure. `rerun-if-changed` is registered on the resolved HEAD + ref
     files (via `git rev-parse --git-path`, so it is correct inside a git WORKTREE)
     and ONLY when those paths exist, so a vanished worktree cannot force perpetual
     rebuilds through the shared `target/`.
  3. `version.rs`'s hyperlink branch is live, and `commit_url()` composes
     prefix+hash. NOTE the old branch was ALSO WRONG, not merely dead: it wrote
     `\x1b]8;;{hash}` — the bare hash as the URI, with no URL prefix at all.

  **Gate.** `+version`'s hyperlink is tty-only, and `cli_smoke`'s stdout is a pipe,
  so the binary-level KAT structurally cannot reach it. `run()` was therefore split
  into `run()` (acquires stdout + `isTty`) and `write_version(sink, tty)` — a seam,
  not a divergence: the reference already threads `stdout` and `tty` through this
  code. Four `rustty-cli` unit tests: the commit hash is captured whenever the
  source IS a git checkout (the assertion that was dead before), the URL is the
  product repo's, the tty path emits `OSC 8 ;; <commit-url> ST <header> \n\n OSC 8
  ;; ST`, and the non-tty path emits no OSC-8 at all. Mutation-verified (3
  experiments, all RED then reverted): `commit_hash() -> None` (the pre-fix state),
  a wrong URL prefix, and a suppressed hyperlink write.

  Incidentally documented by the tty test: the CLOSING `OSC 8 ;; ST` is emitted on
  a tty even when no link was opened — the reference guards it on `tty` alone, not
  on the hash. Preserved verbatim (R1).

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

## rustty-cli C5 (Phase-2 CLI milestone) — vaxis/zf-touching actions [RECONCILED 2026-07-10, T14: TUI wiring landed; ~~`boo.rs:22` is the crate's ONLY live seam~~ **boo RESOLVED — WS19 T2 audit row W3, 2026-07-29**]

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
below, only **`boo`** remains deferred: ~~`boo.rs:22` `unimplemented!()` is the
crate's ONLY live seam~~ **(boo has since RESOLVED too — WS19 T2 audit row W3; see
the struck bullet below)** (it needs the vaxis `vxfw` widget framework tier — NOT
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
  - ~~**`boo` — STILL DEFERRED (the crate's ONLY live seam, `boo.rs:22`).** A 100%
    vaxis/**vxfw** ghost animation (needs the `framedata` compressed asset too);
    `run` is an `unimplemented!()` marker and the whole reference (`Boo` widget,
    span-parser, flate decompression) is preserved verbatim as comments
    (comment-fidelity, scoped-G3b missing=0). NOT unblocked by the vaxis port:
    chunks 1a–4b stop at the `widgets`/TextInput tier and do NOT port the `vxfw`
    widget framework `Boo` is built on. Un-defer trigger: a vaxis vxfw follow-up
    chunk + the `framedata` asset (R3 copy).~~
    **RESOLVED (`e7e5a8c`) — see the `boo` ✅ RESOLVED row ~240 lines above in this same
    file, which this block contradicted for weeks. `crates/rustty-cli/src/boo.rs` is a
    525-line live port with a real `run()`; both the vxfw widget-framework subset and the
    `framedata` asset landed. VERIFIED END-TO-END [WS12 T5] by
    `crates/rustty-cli/tests/boo_pty_e2e.rs` + `.port/tools/boo_pty_kat.py`: driving the
    real binary through a pty gave `EXIT_STATUS=0` 0.04 s after ESC with 69 frames /
    2,009 colour runs / max column 142 / 145,598 bytes, and the `--no-dsr` negative
    control was still alive 4.37 s after ESC. Two traps are encoded in that KAT: a pty with
    no winsize renders zero cell content, and a bare pty MUST answer DSR (`ESC[5n` →
    `ESC[0n`) or shutdown blocks in `Loop::stop` — upstream-faithful libvaxis behaviour,
    not a rustty defect.**

    **RESOLVED / STALE [WS19 T2 audit, 2026-07-29 — audit row W3].** This row was
    already self-contradicted 240 lines earlier by the `boo` entry at the
    "cli C5 finale" heading above, which records it ✅ RESOLVED.
    `crates/rustty-cli/src/boo.rs` is a **525-line LIVE port** with a real
    `pub fn run() -> Result<u8, Error>` at `:406`; `boo.rs:22` is a doc-comment
    line, not an `unimplemented!()`. The vxfw tier and the `framedata` asset both
    landed. Nothing here is open.

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

- ~~**Phase-3 GUI launch leaf (`main_ghostty.rs`).**~~ **[WS19 T2 audit, 2026-07-29 —
  audit row W9: MOSTLY RESOLVED; the file is now `main_rustty.rs` (renamed 410c387),
  and the GTK arm is LIVE.]** The reference `main()`'s GUI
  path (`App.create(alloc)` + `apprt.App.init/run` the event loop, `startQuitTimer`)
  reaches the concrete embedded/GTK apprt backend + the xev event loop (D6). ~~It is
  ported as a documented `unimplemented!()` leaf with the reference body preserved
  verbatim as comments (the GPU/GTK/xev "deferred placeholder leaf" precedent).~~ The
  reference reaches it only for a GUI build (`app_runtime != .none`); the headless
  `.none` bin (the current darwin/Windows default) never does — it runs the action
  or prints the usage banner. ~~**Target:** un-defer when the embedded/GTK apprt
  backend + xev (D6) land (Phase-3).~~ The no-args KAT is scoped to `.none`
  (`app_runtime()` guard) so it stays honest on a future `.gtk` build.

  **CURRENT STATE.** `crates/rustty/src/main_rustty.rs` has exactly ONE
  `unimplemented!()`, at `:299`, and it is NOT the GUI-launch port: the GTK arm
  `run_gtk(&state.logging)` at `:292` is LIVE and diverges (`-> !`, every arm ends in
  `std::process::exit` after the reference's `terminate`/`destroy` teardown), so
  control never reaches `:299` on a GTK build. `:299` is reachable only on a
  configuration with **no compiled GUI backend** (e.g. a Linux
  `--no-default-features` build with GTK off) — unreachable on both supported hosts
  (macOS `app_runtime()` is `.none` and takes the banner path). It is therefore a
  configuration-guard leaf, not a deferred port; no owner and no trigger are needed.

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

  > **SPLIT AND OWNED [WS19 T14, 2026-07-29 — audit rows O13a / O13b].** This row bundled
  > three files with one non-owner target; they have three different dispositions.
  > * **`main_gen.zig` — STALE, it is PORTED** at `crates/rustty-synthetic/src/main_gen.rs`
  >   (and registered as the `rustty-synthetic` bin, `Cargo.toml:26-28`). Strike it.
  > * **`main_wasm.zig` → OWNER: WS14** (wasm/WebGL). ~~Re-derived repo-wide:
  >   `grep -rn 'main_wasm' crates/ --include='*.rs'` → 0 hits, so genuinely OPEN.~~
  >   **RESOLVED — WS14, 2026-07-30 (audit row O13a).** That re-derivation was a
  >   FALSE NEGATIVE: it grepped for the *reference filename*, which a Rust port
  >   naturally never spells. `main_wasm.zig` was already ported at
  >   `crates/rustty-wasm/src/lib.rs` when the row was written — the genuine residual
  >   was never the source but the **artifact**: the crate was an `rlib`, could not
  >   build for `wasm32`, and retained only 3 of the reference's 5 modules. WS14
  >   closes exactly that: `crates/rustty-wasm` is now a `cdylib` producing a real
  >   freestanding `target/wasm32-unknown-unknown/release/rustty_wasm.wasm`
  >   (built by `.port/tools/build-main-wasm.sh`, shape-asserted by
  >   `crates/rustty-wasm/tests/artifact_shape.rs`), with the retention set
  >   corrected — `font/Atlas.Wasm` added, and the `rustty-core` edge DROPPED as
  >   unfaithful (see item 5 of the C4 list above: the reference's `App.Wasm` emits
  >   zero symbols, so an edge to it retains nothing and costs a
  >   `rustty-inspector → dcimgui-sys → freetype-sys → zlib-sys` chain in a
  >   freestanding link). **Zero wasi imports; every import in `env`.**
  > * ~~**`main_build_data.zig` + the `+gen-*` file-emit wiring → OWNER: WS13** (i18n &
  >   docs), which already owns the generated-artifact tooling. Also 0 hits, genuinely OPEN.~~
  >   **PORTED (WS13, 2026-07-30)** at `xtask/src/main_build_data.rs`, exposed as
  >   `xtask gen +<action>`; see the sub-project #3 entry for the full disposition
  >   and the anti-drift test that pins it to the installed bytes.
  >
  > Gate **G-P2** FAILS on any open row without an owner in an OPEN workstream.


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

- ~~**`version.zig` renderer/libxev/toolchain STUB lines (continued from C1, ADV-FIX
  1).** The `+version` KAT asserts ONLY exit 0 + stdout starts `rustty ` — it does
  NOT assert the `- renderer` (`RENDERER_STUB`), `- libxev` (`LIBXEV_STUB`), or
  `- Rust version` (`TOOLCHAIN_VERSION_STUB`) line CONTENT, which stay documented
  stubs until R-d (renderer) / D6 (libxev) land. Unchanged by C6.~~

  **RESOLVED / STALE [WS19 T2 audit, 2026-07-29 — audit row W8].** `grep -rn
  'RENDERER_STUB\|LIBXEV_STUB\|TOOLCHAIN_VERSION_STUB' crates/` → **zero hits**.
  WS3-B replaced all three with real values and `crates/rustty/tests/cli_smoke.rs`'s
  `version_plus` KAT now asserts their CONTENT. The `D:2639` copy of this row already
  recorded that; this C6 copy was never struck. The last `+version` residual — the
  OSC-8 commit hyperlink (audit row O12) — is ✅ RESOLVED by **WS19 T12** (2026-07-29;
  implemented, not diverged: see the RESOLUTION note on the `version.zig is PARTIAL`
  entry). `+version` now has NO residual.

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

- ~~**`ThreadPool.zig` work-stealing BODY** (§1/§6). Only the TYPE SURFACE
  (`Task`/`Batch`/`schedule`/`init`/`deinit`/`shutdown` sigs) is ported in
  `thread_pool.rs` so `kqueue.rs` compiles~~; ghostty passes NO `thread_pool`
  (`Loop.init(.{})` with no pool), so the pool is unreachable on macOS. Consequence:
  the `kqueue: file IO on thread pool` test is a **SkipZigTest-style skip-stub
  `#[test]`** (`kqueue_file_io_on_thread_pool` — the count stays 12, honest R5-safe:
  it defers genuinely-unused functionality, not a live path). ~~G3b on `ThreadPool.zig`
  is intentionally NOT missing=0 (only the type surface is ported; the deferred body's
  ~700 comments are the documented gap, this bullet).~~

  **BODY RESOLVED / STALE [WS19 T2 audit, 2026-07-29 — audit row W5].**
  `crates/rustty-xev/src/thread_pool.rs` is **1630 lines with 0 `unimplemented!()`** —
  the full Zap work-stealing pool, shared by both targets, SB/TB-proven on
  `.port/phase-2/mirihar-threadpool` (commits 89dde0f / e6885c0). Only the NARROW
  residual survives: the macOS `kqueue: file IO on thread pool` skip-stub above, kept
  as-is and **reclassified as a permanent Justified R1 Divergence by WS19 T11**
  (audit row O10) — see the `# Justified R1 Divergences` section. Nothing in this
  bullet is a Phase-2 deferral any more.
- **`dynamic.zig`** (525 loc, 5 tests) — comptime-elided on single-candidate macOS
  (§0: `Backend::candidates()` is `&[Kqueue]`, so `Xev(bes)` collapses to
  `bes[0].Api()`); N/A, not weakened. The `Dynamic`/`DynamicXev`/`Sys`/`dynamic`/
  `backend`/`available` main.zig decls are noted (reference doc preserved, R3) but not
  re-exported.
- **Other backends** (`backend/epoll` 12t, `io_uring` 13t — **both since PORTED**;
  ~~`iocp` 14t, `wasi_poll` 9t — out-of-scope-by-platform per `ROADMAP:214`, not
  deferrals; formally ledgered with a 4-criterion basis + the G-XEVSCOPE gate by
  WS19 T7, 2026-07-30~~ **[STRUCK — WS20, 2026-07-30: that classification was
  WRONG and is retracted; `iocp` and `wasi_poll` are BOTH PORTED (`iocp.rs` +
  `windows.rs`, `wasi_poll.rs` + `wasi.rs`) and gate-counted by
  `check_test_counts.py` rows `xev_iocp`/`xev_wasi_poll`. See the RETRACTED entry
  in `# Justified R1 Divergences`.]**), ~~**`watcher/file`** (8t, `xev.File` — zero ghostty uses),
  **`watcher/tcp`** (5t), **`watcher/udp`** (6t), **`c_api.zig`** (1t, libxev's C ABI —
  rustty consumers use the Rust API) — whole follow-up ports (§1). Non-macOS handle
  types stay `lib.rs` placeholders.~~

  **RECLASSIFIED [WS19 T11, 2026-07-29 — audit rows O10, O11].** `File`/`TCP`/`UDP` are
  **PORTED on Linux** (per-backend in `epoll.rs`/`io_uring.rs`, wrapped as the Dynamic
  enums). What remains is (a) the macOS export surface for those three plus the
  `kqueue: file IO on thread pool` skip-stub, and (b) `c_api.zig`. Both are now
  **permanent Justified R1 Divergences** with 4-criterion bases — see
  "macOS `xev.File`/`TCP`/`UDP` watcher surface …" and "libxev `c_api.zig` …" in the
  `# Justified R1 Divergences` section. Neither is open work; neither had an owner while
  it sat in this list.
- **[ADV-FIX F6] `os/flatpak.rs` + vt-search xev refs** — NOT resolved by making
  `rustty_global::xev` re-export `rustty-xev` (that would cycle
  `global → xev → global`). They resolve LATER via DIRECT edges: `os → xev` (flatpak)
  and `vt → xev` (search), both acyclic (rustty-xev deps neither os nor vt).
  `rustty_global::xev` stays the `()` package anchor exactly as `global.zig`'s
  `pub const xev` is.
  **[WS20] the `os → xev` half is now REAL:** `rustty-os` declares an optional
  `rustty-xev` dep enabled by its `flatpak` feature, and `os/flatpak.rs` binds
  `use rustty_xev as xev;`. `check_acyclic.sh` GREEN.
- **`net.rs`** (the `std.net.Address` subset for connect/sendto/recvfrom) is a std
  port, not a libxev source file, so it is intentionally UNMAPPED in
  `check_test_counts.py` (no `.zig` reference oracle).
- **Milestone B (NEXT — not this chunk):** un-defer TERMIO ONLY ([ADV-FIX F8]) —
  `Thread.run` loop + `Termio.init/threadEnter/processOutput` + `Exec`
  `Subprocess.start`/`ReadThread`/`queueWrite` + the core `Async::notify()` leaves —
  capped with a live-shell KAT. `renderer.Thread` stays the GPU tier [LANDED: W0–W5] tier (GPU-and-xev
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
(notably the wasm32 build, which has no event loop at all). ~~**[WS19 T7,
2026-07-30: renamed again to `unsupported_target_seam` and re-worded — these are a
permanent SCOPE BOUNDARY, not a deferral.]**~~ **[WS20, 2026-07-30: that wording is
RETRACTED. `iocp` and `wasi_poll` are ported; the seam is now split into
`watcher_seam` (the not-yet-instantiated watcher layer, owner `Phase 3`) and
`no_backend_seam` (freestanding wasm — a target the REFERENCE has no backend for,
so there is nothing to port).]**

**[ADV-FIX F7] gate wiring:** `check_test_counts.py` gains four rows against the
SAME pinned `GHXEV` libxev package the macOS rows use (the Linux read-only oracle
checkout is the same commit, verified by reproducing every existing xev row's
authored zig count exactly): `xev_epoll` (zig=12, rust=15), `xev_io_uring` (zig=13,
rust=16), `xev_dynamic` (zig=4 gate-counted of 5 `test{}` — the 5th is the anon
`refAllDecls`, rust=14), `xev_timerfd` (zig=1, rust=1). All 19 xev rows are rust ≥
zig. This moves `backend/epoll` / `io_uring` / `dynamic.zig` OUT of the C7 finale's
"Other backends ... whole follow-up ports (§1)" DEFERRED bullet above — that bullet
is now only `iocp` (14t) and `wasi_poll` (9t). **[WS20, 2026-07-30: and now it is
EMPTY — both are ported and gate-counted.]**

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
- ~~**`Timer::cancel` on Linux** (timer.zig:369) — carried over from LT7 (see
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
  until this is un-deferred.~~

  **RESOLVED / STALE [WS19 T2 audit, 2026-07-29 — audit row W4].**
  `crates/rustty-xev/src/dynamic.rs:1532` `Timer::cancel` is fully ported with BOTH
  arms — `io_uring::Operation::TimerRemove` (`:1565`) and `epoll::Operation::Cancel`
  (`:1578`) — through the `Completion::new_op` seam the struck text said did not
  exist; its own PORT NOTE says "This un-defers the C7 gap". The "core will not build
  on Linux" claim is therefore **false**: Linux `cargo build --workspace` is green
  (ROADMAP flagged this; only the ledger row was never struck).
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

~~**Residual (4 live `unimplemented!()` — the BINARY-ENTRY seams,
`crates/rustty-synthetic/src/cli.rs`, previously tracked only in
`SYNTHETIC_DESIGN.md`; each keeps its verbatim Zig body as comments):**~~

1. ~~**`cli.rs:56` `main()`** — needs `cli.action.detectArgs` (generic `+action`
   detection), which lives ABOVE `synthetic` in the acyclic Rust split (a
   synthetic→cli edge would cycle). No ported `test{}` exercises it.~~
2. ~~**`cli.rs:85/88/91` `main_action_impl_{ascii,osc,utf8}`** — the
   option-parsing (`cli.args.parse` over argv/string iterators) + stdout wiring
   of `mainActionImpl`. The substantive dispatch (`main_action`) and every
   generator body are LIVE; only the binary-entry glue defers.~~

~~**Un-defer trigger:** the `rustty +synthetic`-style binary entry (the same
milestone that wires generator binaries into the differential-fuzz harness, G5).
Placeholder-leaf doctrine: shapes fixed, bodies panic if called, no test reaches
them.~~

**RESOLVED / STALE [WS19 T13, 2026-07-29 — audit row O18].** These four seams **do not
exist**. Verified repo-wide, not with the row's own scope (see §G for why that matters):
`grep -rn 'unimplemented!\|todo!' crates/rustty-synthetic/` → **0 hits**. `main()` is
LIVE at `cli.rs:97` and `main_action_impl_{ascii,osc,utf8}` at `:245`/`:274`/`:297`.
`crates/rustty-synthetic/Cargo.toml:26-28` registers the `rustty-synthetic` bin at
`src/main_gen.rs`, whose `fn main` calls `rustty_synthetic::cli::main()` — so the
"un-defer trigger" (a binary entry) has ALSO already happened. The plan's T13 was
therefore going to implement something that already shipped.

**What actually remains** is the LOCAL replication of `detectArgs`, which is a
**permanent Justified R1 Divergence** forced by the acyclic split (R4): the shared helper
lives in the heavy `rustty-cli`, above this crate. See
"`rustty-synthetic` / `rustty-bench` locally-replicated `detectArgs`" in the
`# Justified R1 Divergences` section for the 4-criterion basis. **This is a decision, not
a deferral** — no owner or trigger is needed beyond "if `detectArgs` changes".

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

**ALL FOUR RESIDUALS DISPOSED — WS19 T13/T14, 2026-07-29 (audit rows O19, O19a).** None
is open work:
* **(1) `os_signpost`** → **permanent Justified R1 Divergence** (pure Instruments
  instrumentation, verbatim body preserved, unobserved by any test, and the binding tier
  it needs is deferred tree-wide). 4-criterion basis in the divergence section.
* **(2) the `ghostty_benchmark_cli` C-name alias** → **MOOT.** Upstream **deleted** the
  `ghostty_*` alias layer at `e4cc852` (audit row W7), so there is no legacy name to be
  compatible with. `rustty_benchmark_cli` is the whole surface. Do not re-add aliases.
* **(3) `main_bench.zig` binary entry** → **SHIPPED.** `Cargo.toml:21-23` registers the
  `rustty-bench` bin at `src/main_bench.rs`, whose `fn main() -> ExitCode` calls
  `rustty_bench::cli::main()`. `grep -rn 'unimplemented!\|todo!' crates/rustty-bench/`
  → 0 hits. The row was stale; the parenthetical "only reached by the deferred
  `main_bench.zig` bin" in (4) is stale for the same reason.
* **(4) local `detectArgs`** → **permanent Justified R1 Divergence**, forced by the
  acyclic split (R4). 4-criterion basis in the divergence section, shared with
  `rustty-synthetic`.

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
`Action::option_specs()` reifying the commented-out `options()`). 2 `test{}` ported.
~~**Residual deferral:** the `+gen-*` CLI/build.rs file-emit wiring (writes the
scripts to disk at packaging time) stays deferred with `main_build_data` (C6).~~
**RESOLVED (WS13, 2026-07-30).**

> **RESOLVED [WS13, 2026-07-30 — audit rows O13a/O13b].** Both halves now exist and
> are pinned to each other:
> * **The file-emit half** was in fact already shipping — `xtask resources`
>   (`res_shell::gen_shell`, `res_editor::gen_editor`, `res_terminfo::gen_terminfo`)
>   writes every one of these artifacts into `zig-out/share`, which is what
>   `build.zig` does with `main_build_data`'s stdout.
> * **The `+gen-*` CLI half** was genuinely missing (`grep -rn main_build_data
>   crates/ --include='*.rs'` → 0 hits) and is now `xtask/src/main_build_data.rs`,
>   the line-for-line port of `src/main_build_data.zig`'s `Action` enum and `main`,
>   exposed as `cargo run -p xtask -- gen +<action>` for all nine actions
>   (`bash`, `fish`, `zsh`, `sublime`, `vim-syntax`, `vim-ftdetect`,
>   `vim-ftplugin`, `vim-compiler`, `terminfo`). It is an `xtask` subcommand rather
>   than a separate binary because the reference splits it out purely to stop
>   `build.zig` rebuilding when its data dependencies change — a Zig build-graph
>   concern cargo does not have — and because `xtask` is the only crate already
>   depending on BOTH `rustty-extra` and `rustty-terminfo`, so no new edge appears.
> * **Anti-drift:** `main_build_data::tests::gen_actions_match_the_installed_bytes`
>   asserts the stdout bytes of seven actions are byte-identical to the files
>   `xtask resources` installs, so the two surfaces cannot diverge.
>   `every_action_emits_distinct_non_empty_bytes` catches a wrong `switch` arm;
>   `detect_matches_exactly_the_reference_action_set` carries five negative
>   controls. FAIL arm: `xtask gen bogus` exits 1 with `NoAction`.


## ~~`src/build/mdgen/**` + `src/build/webgen/**` — man-page + website doc generators (low priority)~~ **RESOLVED (WS13 T6/T7/T8, 2026-07-30)**

**Ghostty source:** `src/build/mdgen/**` (`mdgen.zig`, `main_ghostty_1.zig`,
`main_ghostty_5.zig`, the `ghostty_{1,5}_{header,footer}.md` fixtures) +
`src/build/webgen/**` (`main_actions.zig`, `main_commands.zig`,
`main_config.zig`) — ~345 LOC of `.zig` (plus the `.md` header/footer fixtures).

**Rustty counterpart (all of it, ported):**

* `crates/rustty-extra/src/mdgen.rs` — `substitute` (`mdgen.zig:8-19`),
  `gen_config` (`:21-48`), `gen_actions` (`:50-83`), `gen_keybind_actions`
  (`:85-117`), `man_1_markdown`/`man_5_markdown` (`main_ghostty_{1,5}.zig`).
  The reference's `inline for` over reflected fields becomes a runtime loop over
  the reified tables (`config_fields()`, `Action::ALL`,
  `binding::Action::FIELD_NAMES`) plus the build-generated help corpora.
* `crates/rustty-extra/src/mdgen/rustty_{1,5}_{header,footer}.md` — the four
  prose fixtures, rebranded per R2 and DE-SNARKED (memory:
  strip-ghostty-gag-prose). Deviations are enumerated in `mdgen.rs`'s module
  header: the "We aim to be the best in this category, and competitive in the
  rest." boast is dropped, "you'll see a lot of weird blank configurations" is
  reworded, the ghostty-users community pointer is dropped, the upstream
  `--Use the system log CLI` typo is fixed, and `# BUGS`/`# AUTHOR` carry
  rustty's maintainer identity instead of upstream GitHub links (the precedent
  the GTK About dialog already set by dropping `website`/`issue-url`).
* `crates/rustty-extra/src/mdgen/render.rs` — a Rust markdown→roff and
  markdown→HTML renderer standing in for the reference's `pandoc --to man` /
  `--to html` (`GhosttyDocs.zig:60-88`).
* `crates/rustty-extra/src/webgen.rs` — `config_mdx` (`main_config.zig:12-125`,
  including the undocumented-field grouping and the `code`/`<Note>`/`<Warning>`
  block detection), `actions_mdx` (`main_actions.zig:5-9`, delegating to the
  already-ported `rustty_input::helpgen_actions::generate` rather than growing a
  second implementation — a test asserts the two are byte-identical), and
  `commands_mdx` (`main_commands.zig:14-52`).
* `xtask/src/res_misc.rs` — `gen_man` (`share/man/man{1,5}/{APP}.{1,5}`),
  `gen_doc` (`share/{APP}/doc/{APP}.{1,5}.{md,html}`), `gen_webdata`
  (`share/{APP}/webdata/{config,actions,commands}.mdx`). The hand-written
  `MAN1_BODY`/`MAN5_BODY` roff transcription and its PARTIAL note are deleted;
  the pages now carry the FULL generated `COMMAND LINE ACTIONS` /
  `CONFIGURATION OPTIONS` / `KEYBIND ACTIONS` catalogs instead of pointing
  readers at `+show-config`.

**The `pandoc` tradeoff, recorded so it is not re-litigated:** `pandoc` is
installed on NEITHER build host (macOS workstation, Fedora VM). Requiring it
would make `xtask resources` — a step every packaging run needs — fail on a
stock machine; that option was rejected on exactly the grounds WS9 used to
reject linking Homebrew's libintl (vendor, do not depend). Emitting roff
directly from `mdgen` was also rejected: it would discard the `.md`/`.html`
artifacts the reference installs and make the generator's output un-diffable
against the reference's markdown. The chosen option generates the reference's
markdown verbatim (modulo R2) and converts it in Rust over the small, CLOSED
subset `mdgen` can produce. Full argument in `render.rs`'s module header.

**Intentional divergences from the reference:** (1) rustty ALWAYS emits real
pages, never `GhosttyDocs.installDummy`'s `share/man/.placeholder`, because the
macOS Copy-Resources phase ships `share/man` unconditionally; (2) the `.TH`
title derives from `crate::APP`, so `--features identity-oracle` yields
`.TH "GHOSTTY" "1"` matching the `ghostty.1` FILENAME — before WS13 the filename
flipped but the title still said `RUSTTY`; (3) the `.TH` date field is populated
from a committed constant rather than left empty as pandoc leaves it, because an
empty date makes `mandoc -Tlint` warn unconditionally.

**Verification actually run (WS13 T7/T8):**

* `mandoc -Tlint` on macOS: **rc=0, zero diagnostics** on both pages — no ERROR,
  no WARNING, no STYLE. FAIL arm demonstrated: injecting an unbalanced `.RE` and
  a raw non-ASCII byte into a copy makes mandoc report
  `ERROR: skipping end of block that is not open: RE` and exit 3.
* `groff -man -Tutf8 -ww` on the Fedora VM (a DIFFERENT formatter — the VM has
  no mandoc, macOS has no groff): rc=0 for both pages. This cross-check found
  two real defects mandoc had accepted: raw UTF-8 in the page (groff reported
  `invalid input character code 134` per continuation byte and DROPPED the
  character — fixed by emitting `\[uXXXX]` escapes) and a `[text](url)` link
  split across source lines that the per-line inline pass left as RAW MARKDOWN
  in the man page (fixed by joining filled runs before the inline pass). FAIL
  arm demonstrated: the same injected raw-UTF-8 byte makes groff report
  `invalid input character code 128`.
* Residual, honestly recorded: 5 `cannot adjust line` notices at groff's
  MAXIMUM warning level (`-ww`), all long URLs inside an indented `.RS` region.
  A `\:` break-opportunity experiment was tried and REVERTED — measured, it
  raised the count 5 → 7, because breaking a URL yields lines with a single long
  chunk and no inter-word space left to stretch, which is the very condition the
  notice reports. No input-side markup can make a token wider than the measure
  justify, and `pandoc --to man` emits the same `\(la…\(ra` construct. Both
  formatters exit 0.
* `man -M <share>/man` renders 4761 (man1) and 5019 (man5) lines; probed option
  names (`font-family`, `background-opacity`, `keybind`, `window-decoration`,
  `confirm-close-surface`) all present with their real doc bodies, `new_split`
  present in man5's KEYBIND ACTIONS, `show-config` in man1's COMMAND LINE
  ACTIONS. Negative controls: 0 occurrences of `ghostty` (case-insensitive) in
  either page, 0 of `not-a-real-option`, 0 of the stripped `competitive in the
  rest` marketing line.
* HTML: both `share/rustty/doc/*.html` parse with a balanced tag stack, 215 and
  279 `<dt>` terms respectively, `&lt;` present (escaping exercised) and no raw
  `<script`.
* webdata: 206 `## ` headings in `config.mdx`, 18 in `commands.mdx`, callout and
  code-fence counts balanced with all three block kinds actually produced, and
  0 occurrences of `ghostty` or `ghostty-org` in any of the three files.
* 36 unit tests in `rustty-extra` cover the generators and the renderer, each
  with at least one negative control.

## ~~`src/stb/stb_image_resize.h` — glyph resize (`stbir_resize_uint8`); rustty-font does NOT yet cover it~~ **PORTED 2026-06-30 at `2e82776`; RECLASSIFIED to VACUOUS TEST COVERAGE → CLOSED by WS11, 2026-07-29** [row corrected WS19 T14 — audit row O17; coverage closed WS11]

**Ghostty source:** `src/stb/stb_image_resize.h` (single-header stb library),
invoked as `stb.stbir_resize_uint8(...)` in the FreeType glyph-resize path
(`src/font/face/freetype.zig:640`, "resizes the glyph"). ~~**Rustty counterpart:**
NOT covered — `grep -rn 'stbir|stb_image_resize' crates/rustty-font/src` finds
only `Vec::resize`/`atlas.resized` (unrelated). The glyph/atlas resize seam has no
Rust analogue yet. **Why deferred:** the live FreeType glyph-rasterization path
(which is itself windowing/font-face-tier and gated on the Phase-3 renderer) is
where this resize is called; the offscreen port did not need it. **What unblocks
it:** when the live glyph-rasterization path lands, wire glyph resize — either a
pure-Rust resize (e.g. an image-resize crate) or a `stb-sys` cc build compiling
`stb_image_resize.h` (a `cc` build.rs, the -sys precedent). Note a **stb-sys cc
build may be needed** if bit-exact parity with stb's resampler is required.~~

**THE PORT SHIPPED — 2026-06-30, commit `2e82776`** [corrected WS19 T14,
2026-07-29 — audit row O17]. Independently verified:
* `crates/rustty-font-shaper/src/stb.rs:18` declares `stbir_resize_uint8`;
* `crates/rustty-font-shaper/build.rs` compiles `vendor/stb_impl.c` via `cc::Build`;
* `crates/rustty-font-shaper/vendor/stb_image_resize.h` is **byte-identical to the
  oracle** — md5 `7381a7070120fd5a36d8fb9e3179dd7c` on both sides
  (`git show origin/main:src/stb/stb_image_resize.h`);
* call site `crates/rustty-font-shaper/src/face/freetype.rs:806`,
  argument-for-argument identical to oracle `freetype.zig:639-652`.

**WHY THIS ROW STAYED "OPEN" FOR A MONTH — a methodology finding, recorded so it
does not recur.** The row's own evidence grep was **scoped to
`crates/rustty-font/src`**, but the code lives in the **sibling** crate
`rustty-font-shaper`, created afterwards by the **D1 three-tier split** (memory
`apc-glyf-font-layering-d1`). *The grep scope WAS the bug.* Re-verifying the row by
re-running the scope the row supplied could not possibly detect that — it is a
self-confirming check. **Rule: never verify a ledger row with a search scope the row
itself provides; use a repo-wide search.** The WS19 audit's own re-verification of
this row fell into exactly that trap. Every other audit row verified with a
row-supplied scope has been re-derived repo-wide (see the audit's §G).

**ALSO CORRECTED: the gating is NOT platform-conditional.** The ROADMAP's "wired on
the non-macOS path" is wrong — `build.rs` gates on
`cfg(not(target_arch = "wasm32"))`, not `not(target_os = "macos")`. The stb build and
the FreeType face are **live on macOS too** (`nm -g` shows `T _stbir_resize_uint8`
in the macOS archive). Any reasoning that treats this seam as Linux-only is wrong.

~~**WHAT IS ACTUALLY OPEN → OWNER: WS11**~~ **— CLOSED by WS11, 2026-07-29.**
The residual item was that the **test coverage of the resize was VACUOUS**: every
existing test that reached the call did so at **scale 1.0** (`8x16→8x16`,
`136x128→136x128`), and `render_and_resize` never enters the bitmap arm at all
(Inconsolata is an outline font). Proof, re-reproduced at the head of WS11:
substituting a nearest-neighbour loop for stb left **all 9 tests green** — nothing in
the suite would have noticed a wrong resampler.

**WS11 closed it with 5 new tests + a new gate, each with a demonstrated RED arm:**

| what | where | discriminates? |
|---|---|---|
| `stbir_resize_uint8_upsamples_a_known_ramp` | `src/stb.rs` | structural: rejects a non-interpolating resampler (falsified by swapping `input_w`/`input_h`) |
| `stbir_resize_uint8_treats_channels_independently` | `src/stb.rs` | the `num_channels = 4` (BGRA) path: a 4-channel resize must equal 4 separate 1-channel resizes of the deinterleaved planes (falsified by a wrong stride) |
| `stb_resize_kat_bitmap_strike_grayscale` | `face/freetype.rs` | Terminus 8×16 → **16×17** and **8×17** via `Constraint{size:Stretch}`; consensus-validated fixtures |
| `stb_resize_kat_bitmap_strike_bgra` | `face/freetype.rs` | NotoColorEmoji 136×128 → **16×15** and **8×7** via `Constraint{size:Cover}` + primary-face grid metrics; the STRONG discriminator (real 8-bit source) |
| `stb_resize_kat_rejects_nearest_and_bilinear` | `face/freetype.rs` | makes the falsification PERMANENT: recomputes nearest + bilinear in-test and asserts the fixtures reject them, and that the tolerated ±1 envelope is disjoint from those rejections |
| **gate D10** `check_stb_resize_parity.sh` | `.port/tools/gates/` | our vendored C vs **ghostty's own build recipe** at all four Zig optimize modes, plus `negative_control.py`; `--self-test` proves both RED arms |

**The non-vacuity is measured, not asserted.** Under nearest-neighbour substitution
both KATs FAIL while all 9 pre-existing tests still pass — simultaneously the proof
that the new coverage discriminates and that the old coverage never did. Delta budget:

```
gray 16x17  vs nearest   11/272 ( 4.0%) maxdelta=255   vs bilinear   2/272 maxdelta=255
gray  8x17  vs nearest    5/136 ( 3.7%) maxdelta=255   vs bilinear   0/136  INDISTINGUISHABLE
BGRA 16x15  vs nearest  597/960 (62.2%) maxdelta=210   vs bilinear 575/960 maxdelta=108
BGRA  8x7   vs nearest  185/224 (82.6%) maxdelta=217   vs bilinear 187/224 maxdelta=121
```

**A WS11 finding the plan did not anticipate:** the grayscale strike is **mono**
(`num_grays = 2`), so stb both reads and writes 0/1 there and `freetype.rs` applies the
`255/(num_grays-1)` factor afterwards. That makes the grayscale fixtures *weak*
discriminators (a wrong resampler's fractional output rounds back onto 0/1) and makes
bilinear entirely inseparable at 8×17. This is why the BGRA KAT is not optional. Fully
documented in `crates/rustty-font-shaper/src/testdata/README.stb-resize.md` (TRAP 2).

Cross-platform: all four fixtures reproduce **byte-for-byte** under Fedora 44 gcc 16.1.1
and clang 22.1.8 at all five optimization levels, and the Linux runtime path (where
FreeType is the *default* backend) runs 12/12 + 2/2 green.

**DO NOT "fix" the stb `build.rs` flags.** Bit-exactness against a single oracle
build is unattainable: ghostty's own Debug and ReleaseFast builds of the identical C
disagree (9/512 bytes, maxdelta 1 — ReleaseFast's loop vectorizer). rustty sits on
the **consensus** side across 5 toolchain/optimization combinations on 2 OSes.
Changing flags would *create* divergence, and since the seam is live on macOS it
would violate R5.

**Confirmed by WS11's `--self-test`, which is the direct evidence for that decision:**
rebuilding our vendored header with `-ffast-math` makes gate D10 go RED against
ghostty's `Debug`/`ReleaseSafe`/`ReleaseSmall` recipes (3/512 bytes, maxdelta 1). Adding
that flag would therefore *create* the divergence it appears to fix. **Locked: no
`build.rs` flag changes.**

## PERMANENT DIVERGENCE — `stbir_resize_uint8` differs from a `ReleaseFast` ghostty build by ≤1 LSB on ≤2 % of bytes [WS11, 2026-07-29]

**Scope:** the FreeType bitmap-strike glyph resampler
(`crates/rustty-font-shaper/src/face/freetype.rs`, the `crate::stb::stbir_resize_uint8`
call). The C is byte-identical to the oracle's
(md5 `7381a7070120fd5a36d8fb9e3179dd7c` at the pin `4749c4e93` **and** at
`origin/main`); only the *codegen* differs.

**Measured** (zig **0.16.0**, oracle @ `4749c4e93`, ghostty's own
`src/build/SharedDeps.zig` recipe — `-I src/stb` + `stb.c`, no extra flags on
non-MSVC — vs rustty's `cc` build of the same header):

```
=== case 16x32 (8x16 grayscale ramp upsample)
  ghostty Debug        vs rustty cc: 0/512 differ, maxdelta=0
  ghostty ReleaseFast  vs rustty cc: 9/512 differ, maxdelta=1     <-- the divergence
  ghostty ReleaseSafe  vs rustty cc: 0/512 differ, maxdelta=0
  ghostty ReleaseSmall vs rustty cc: 0/512 differ, maxdelta=0
=== case 8x17
  all four ghostty modes vs rustty cc: 0/136 differ, maxdelta=0
```

These figures were originally measured under zig 0.15.2 and **re-measured unchanged
under zig 0.16.0** (both consensus MD5s — `5f18d21d6d9d9c6f035bade776299e7c` for
16×32, `0e2fb9f06dc7de62c1c28b5706bd918b` for 8×17 — and the 9/512 maxdelta=1 figure
reproduce exactly).

**Cause identified, not guessed: the loop vectorizer.** Under zig 0.16 (16×32 case,
vs the consensus / vs the ReleaseFast recipe):

```
  zig cc -O0 (no ubsan)              0/512 maxdelta=0   |  9/512 maxdelta=1
  zig cc -O1 -fno-vectorize          0/512 maxdelta=0   |  9/512 maxdelta=1
  zig cc -O2                         9/512 maxdelta=1   |  0/512 maxdelta=0
  zig cc -O3                         9/512 maxdelta=1   |  0/512 maxdelta=0
  zig cc -O2 -fno-vectorize          0/512 maxdelta=0   |  9/512 maxdelta=1
  zig cc -O2 -fno-vectorize -fno-slp 0/512 maxdelta=0   |  9/512 maxdelta=1
  zig cc -O2 -fno-slp-vectorize      9/512 maxdelta=1   |  0/512 maxdelta=0
  zig cc -O2 -ffp-contract=off       9/512 maxdelta=1   |  4/512 maxdelta=1
```

`-fno-vectorize` restores the consensus **exactly**; `-fno-slp-vectorize` alone and
`-ffp-contract=off` do not. (Refinement vs the WS11 plan, which credited only `-O3`:
under 0.16 **both** `zig cc -O2` and `-O3` are byte-identical to the ReleaseFast
recipe, so *any* zig `-O≥2` build lands on the ReleaseFast side.)

**Four-criterion permanent-divergence basis:**

1. **Not closable within the port's constraints.** The only construction that matches
   `ReleaseFast` is compiling the C with `zig cc -O≥2`, i.e. making the Zig toolchain a
   hard build dependency of the Rust port — directly against R3.
2. **The oracle is not self-consistent.** Ghostty's own `Debug` and `ReleaseFast` builds
   of the identical header disagree by the same ±1, so "bit-exact with the oracle" has
   no unique referent. The honest target is "exact with the oracle's non-`ReleaseFast`
   modes and with every non-Zig toolchain, within ±1 LSB of `ReleaseFast`".
3. **Not port-specific.** The same ±1 appears on two further axes: aarch64 vs x86_64
   builds of the same compiler at the same `-O` (9/512 and 3/136, maxdelta 1), and —
   found by WS11's Linux verification — **Fedora gcc 16 `-O0`/`-O1` vs its own
   `-O2`/`-O3`/`-Os`** (9/512 and 3/136, maxdelta 1; gcc `-O0/-O1` produce a third
   variant, md5 `770a168d…`/`4dc5c919…`). It is a property of the resampler's float
   summation order.
4. **Below the observable threshold by upstream's own standard.** Upstream accepted
   sprite-golden regolds at max-delta 48 (`d0e72a3ab`, per
   `.port/phase-3/assessments/F-ui-tiers.md`); ±1 LSB in an anti-aliased glyph bitmap is
   48× smaller and imperceptible.

**What is GUARDED, so this cannot silently widen:**
* **Gate D10** (`check_stb_resize_parity.sh`, registered PROGRESS in `ci.sh`)
  continuously asserts the envelope: ghostty `Debug`/`ReleaseSafe`/`ReleaseSmall` must
  be **byte-exact**, `ReleaseFast` within max delta 1 on ≤3 % of bytes. Drift beyond
  that is RED. `exit 3` BLOCKED (never a silent green) where `zig` or the oracle
  checkout is absent.
* **The in-crate KATs** assert **exact** equality on the shipping toolchains — and
  that exactness is verified at all five optimization levels on macOS Apple clang,
  Fedora gcc 16 and Fedora clang 22.
* **`negative_control.py` + `stb_resize_kat_rejects_nearest_and_bilinear`** prove the
  envelope cannot admit a wrong resampler (a wrong one moves 55–98 % of bytes at max
  delta 14–255; the envelope allows ≤3 % at max delta 1 — an ~18–20× margin in both
  dimensions).

**Note on the fraction bound.** D10 uses **3 %**, not 2 %. The largest rate observed
anywhere is 3/136 = 2.21 % (Fedora gcc `-O0` vs `-O2` on the 8×17 case — a small buffer,
where one extra differing byte moves the rate a lot); 9/512 = 1.76 % is the next. 3 % is
the smallest round bound above measurement. A hard 2.00 % would make D10 permanently and
**falsely** RED on any Linux box whose `cc` is gcc, i.e. CI. The negative control
validates the same 3 % bound the gate enforces, so the check can never certify a
stricter envelope than the one actually in force.

## Sub-project #2 (macos-swift-app): the `ghostty_*`→`rustty_*` C-ABI compatibility ALIAS layer

~~**Context:** the reused macOS Swift app hard-codes ~163 `ghostty_`-prefixed
symbols, but the rustty C-ABI (rustty-apprt-c, the E-series) exports `rustty_`-
prefixed symbols (R2 identity rename). **What's deferred:** a compatibility ALIAS
layer that re-exports each `rustty_*` C export under its original `ghostty_*` name
(so the unmodified Swift app links against `librustty_apprt_c`). No rustty crate
provides this yet.~~ **Locked decisions (per the critics):** identity is
`io.airely.rustty`; **iOS is in scope** (the xcframework must cover the iOS targets
too). ~~**Why deferred:** only needed when the Swift frontend is
actually reused (Phase-3, after R-d windowed present + D6 land). **What unblocks
it:** sub-project #2 (macos-swift-app, GAP_ANALYSIS §3.3) — generate the ~163
`ghostty_*` = `rustty_*` alias exports (comptime/attribute export aliases) as part
of the Swift-app reuse milestone.~~

**SUPERSEDED — THE PREMISE IS MOOT [WS19 T2 audit, 2026-07-29 — audit row W7].**
Upstream **deleted** the `ghostty_*` C-ABI forwarder layer at `e4cc852`, so there is
no ~163-symbol legacy surface to be compatible with. `grep -rn 'export_name'
crates/rustty-apprt-c/src/` → **0 hits**, and `crates/rustty-apprt-c/Cargo.toml:31-33`
states the crate "no longer compiles any `ghostty_*` C-ABI alias layer … exports ONLY
`rustty_*` (R2), always, in every profile"; `identity-oracle` now flips only the
`rustty-identity` STRING identity for the differential harness. **Do not re-add
aliases.** Consequence outside this file: the ROADMAP's global constraint
"`ghostty_*` C-ABI aliases stay" (`ROADMAP:25`) was FALSE and is corrected by the same
WS19 T2 commit.

## Sub-project #2 (macos-swift-app): signed `.xcframework` repackaging xtask (`GhosttyKit.xcframework`)

~~**Context:** the Xcode project consumes a `GhosttyKit.xcframework`; the rustty
build produces `librustty_apprt_c` (the apprt-c staticlib/dylib). **What's
deferred:** an `xtask` (build-orchestration) subcommand that repackages
`librustty_apprt_c` (+ the generated C header + the `ghostty_*` alias layer above)
into a **signed** `GhosttyKit.xcframework` consumable by the Xcode project (macOS +
iOS slices). No such xtask exists yet (the current `xtask` only does
`header-gen-apprt` cbindgen, see E11/E-series). **Why deferred:** only needed for
the Swift-app reuse build. **What unblocks it:** sub-project #2 (macos-swift-app,
GAP_ANALYSIS §3.3) — add the `xtask xcframework` packaging + codesign step
alongside the alias layer.~~

**RESOLVED / STALE [WS19 T2 audit, 2026-07-29 — audit row W6].** The xtask exists:
`xtask/src/main.rs:65` registers `xcframework` ("Package rustty-apprt-c into
`dist/RusttyKit.xcframework` (macOS Swift-app tier)"), dispatched at `:103`, implemented
at `:305` (staticlib under the `xcframework` profile, archive folding, then
`xcodebuild -create-xcframework`). `xtask/src/` also carries `dist.rs`, `archives.rs`,
`res_*`, `translations.rs`, `wasm_patch_growable_table.rs`. The R2 artifact name is
`RusttyKit.xcframework`, not `GhosttyKit.xcframework`. **The `ghostty_*` alias-layer
clause is moot**: upstream deleted that layer at `e4cc852` — see audit row W7 and the
struck alias-layer entry.

## Sub-project #4 (gtk): the `apprt-gtk` subsystem (`src/apprt/gtk/**`)

**Ghostty source:** `src/apprt/gtk/**` — ~52 `.zig` files, ~22k LOC (`App.zig`,
`Surface.zig`, the `class/**` GObject widgets, `ext/**`, `cgroup.zig`,
`adw_version.zig`, …) plus the `.blp` Blueprint UI + `.css` theme assets. **Rustty
counterpart:** ~~NONE (zero counterpart — the single largest unported chunk)~~
**✅ PORTED — 53 oracle files ↔ 53 rustty files** (`crates/rustty-apprt/src/gtk/**`).
STATUS CORRECTED by the deferral re-audit, 2026-07-31: the only 4 oracle files
without a same-named counterpart are RESTRUCTURED, not missing —
`build/blueprint.zig` → `crates/rustty-apprt/build.rs:173-242`, and
`ipc/{DBus,new_window,toggle_quick_terminal}.zig` → `gtk/app.rs:172,250,261`.
Live-code leaves in `src/gtk/`: ZERO (the 2 `inspector_widget.rs:109,120` hits
are GObject property-name `_ =>` fallthroughs, faithful to
`inspector_widget.zig:106-118`; the 8 in `application.rs` are `#[cfg(test)]`
doubles). See `.port/phase-3/DEFERRAL_REAUDIT.md` §D.
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

**UPDATE [2026-07-30, WS20 — ghostty-mirrored opt-in, second and last arm].**
`application.zig:1261` is now PORTED on exactly the terms the `:1565` UPDATE
above established, closing the second of the two ceiling-forced legacy arms:
* the ceiling was RE-VERIFIED, not assumed: `CssSection::bytes()` is still
  `#[cfg(feature = "v4_16")]` in the resolved gtk4-0.11.4
  (`src/auto/css_section.rs:57`; `v4_16` is a real Cargo feature there,
  `Cargo.toml:119`), so the `v4_14` pin genuinely still hides the symbol and the
  default build must still take the legacy arm. Nothing here was re-ledgered on
  a stale premise.
* a new **`gtk-v4_16`** Cargo feature (default OFF) is the comptime analog of
  ghostty building against >= 4.16 headers; `#[cfg(feature = "gtk-v4_16")]` in
  `class/application.rs` `signal_css_parsing_error` lights the byte-dump, and the
  live `gtk_version::at_least(4, 16, 0)` probe INSIDE that cfg is the runtime
  analog (rustty has no `comptime_version`, so the probe is never folded out).
* `gtk-v4_20` now **implies** `gtk-v4_16` — a >= 4.20 build box also has the
  >= 4.16 headers, so ghostty compiled there takes both modern arms.
* the AD-3 floor is UNCHANGED: `gtk4` still pins `v4_14` by default, the minimum
  system gtk is still 4.14, and the stock `--features gtk` build names no `v4_16`
  symbol. The DEFAULT-build loss described above still holds verbatim for anyone
  who does not opt in.
* verified on the Linux VM against GTK 4.22.4: `cargo check -p rustty-apprt
  --all-targets` green under each of `gtk`, `gtk,gtk-v4_16` and `gtk,gtk-v4_20`,
  and a new headed KAT `tests/gtk_css_parse_error.rs` (ONE test, G-GTKINIT)
  drives a real `CssProvider::parsing-error` and asserts the section's bytes are
  byte-for-byte the source that was loaded. Mutation-proved (assert against the
  wrong bytes -> RED, reverted -> green).
* **Bounded verification debt, with a recipe.** The KAT proves the ceiling is
  lifted and `CssSection::bytes()` yields the exact source; it does NOT drive
  `Application::signal_css_parsing_error` itself, which is `pub(super)` and so
  unreachable from an integration test. Recipe: add a `#[cfg(test)]` unit test
  inside `class/application.rs`'s `imp` module that builds an `Application`,
  installs a stderr capture, calls `signal_css_parsing_error` with a
  `CssSection` obtained from a real `parsing-error` emission, and asserts the
  logged line contains the source bytes under `gtk-v4_16` and does not without
  it. This needs the one-headed-test-per-binary rule (G-GTKINIT) to be reconciled
  with the lib-test binary, which already hosts headed tests.

Both `v4_14`-ceiling arms are therefore now ported behind faithful, ghostty-
mirrored opt-in features. Nothing in this row remains "deferred by platform"; it
is a compatibility-floor DECISION with an opt-in escape hatch on both sites.

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

**Sibling deferral, split out:** the portal `OpenURI` call site was NOT covered by
WS15 and got its own top-level entry below ("GTK portal OpenURI (L6)"), so it
stayed discoverable by a ledger grep instead of being buried under this resolved
heading. **That entry is now RESOLVED (WS19 T8, 2026-07-29).**


## ~~GTK portal OpenURI (L6)~~ **RESOLVED — WS19 T8, 2026-07-29 (audit row O6)**

**RESOLVED.** `portal.zig` + `portal/OpenURI.zig` are ported as
`crates/rustty-apprt/src/gtk/portal.rs` + `portal/open_uri.rs`, onto the same
`gio::DBusConnection` + self-computed-request-path pattern `global_shortcuts.rs`
established. The call site is no longer a placeholder: `startup` hands the connection to
the `OpenUri` object (`application.zig:1305-1308`), `open_url` dispatches to
`open_uri().start(value)` with the reference's `catch → openUrlFallback` arm
(`:2358-2363`), `openUri()` is exposed (`:817-820`), and `deinit_private` calls its
`deinit` (`:440`). The prior stand-in (`gio::AppInfo::launch_default_for_uri`) is gone.

Three DELIBERATE, DOCUMENTED divergences, all carried in the module docs: (1) no `app`
backref — the reference holds one only for the allocator and `openUrlFallback`, and the
port's fallback is app-independent; (2) `RefCell` rather than a mutex, with the borrow
released before the outbound fallback call (the reference holds its mutex across it;
every caller here is the GTK main loop); (3) a non-UTF-8 URI is reported as an error —
D-Bus strings must be UTF-8, so the reference's raw `dupeZ` bytes would build an invalid
message, and the error routes the byte string to the OS opener via the caller's existing
`catch` arm.

**Verification** — `.port/tools/gates/ws19_portal_openuri_gate.sh`, GREEN on the VM
(3 layers, both negative controls exercised; host uptime 14:02 unchanged across the run):
1. **Ordering, against a stub portal on a private activation-free bus**
   (`tests/gtk_portal_openuri.rs`): the stub emits `Response` BEFORE returning the method
   reply, so only a client that pre-subscribed can observe it. Also asserts the
   on-the-wire payload field by field against `OpenURI.zig:266-330` (parent window `""`,
   the uri verbatim, `handle_token`/`writable`/`ask`) and that the payload's
   `handle_token` is the token in the path subscribed to.
   **DEMONSTRATED FAIL ARM:** subscribing from the reply callback instead (the literal
   documented flow) gives `responses: 0` while the stub confirms it emitted on the right
   path. Recorded finding: merely moving the subscribe to the line AFTER the send does
   NOT reproduce it — `gio`'s `call` is async and the portal's `Response` is unicast, so
   what matters is "before any main-loop iteration", not "before the send".
   **NEGATIVE CONTROL:** the stub then releases the portal bus name and the OS-opener
   fallback must fire exactly once, with no hang.
2. **Live round-trip against the session's real `xdg-desktop-portal`**
   (`tests/gtk_portal_openuri_live.rs`): `Response` code 0 on the self-computed request
   path, fallback NOT used. **NEGATIVE CONTROL:** with the probe scheme handler removed
   the portal cannot resolve the URI and the same test FAILS — the gate runs that arm.
3. **Wire shape** from a `dbus-monitor` capture, anti-vacuity-checked by grepping for the
   token the app itself logged.

**PLAN CORRECTION (what the WS19 plan got wrong).** Task 8 Step 4 asked for the capture to
be diffed against "the installed `ghostty 1.3.1-2.fc44` doing the same handshake". That is
impossible: `strings /usr/bin/ghostty | grep -c org.freedesktop.portal.OpenURI` → **0**.
That build carries only `org.freedesktop.portal.GlobalShortcuts` + `.Request`; the portal
`OpenURI` call site landed upstream AFTER the 1.3.1 tag, so there is no installed oracle to
diff against. The substitute — layer 1's field-by-field payload assertions against the
oracle SOURCE at the pin — is stronger than an eyeballed transcript comparison.

**A TOOLING HAZARD worth recording for every VM lane:** `rsync -a` preserves mtimes, so
syncing a macOS-edited file onto the VM can leave it OLDER than the existing build
artifacts and `cargo` will NOT rebuild. This produced a fully convincing false result —
a KAT "failing" against correct source because the binary still held a previous
experiment. Always `touch` synced files (or use `rsync --checksum` plus `touch`) before
building, and distrust any VM result whose build log shows no compile step.

The original analysis follows, for the record.

The sibling XDG portal call site to GlobalShortcuts: the reference's
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

> **CORRECTION OF THE CORRECTION [WS14, 2026-07-30 — audit row O13a].** The
> blanket "NOT ported — DEFERRED" above was itself an over-claim, in the opposite
> direction, for two of the three files. It was **already stale for `main_gen.zig`**
> when written (see the WS19 split below), and it was **stale for `main_wasm.zig`**
> as of WS14's plan: that file was ported as an rlib at
> `crates/rustty-wasm/src/lib.rs` (128 L) at plan time and is now a real freestanding
> `cdylib` artifact (`rustty_wasm.wasm`, zero wasi imports) with the retention set
> corrected. **The correction still stands, unaltered, for `main_build_data.zig`**
> (→ WS13). Method note for future re-derivations: grepping `crates/` for a
> *reference filename* proves nothing about whether a file is ported — a faithful
> Rust port never spells `main_wasm.zig`. Grep for the ported *symbols*.

> **SPLIT AND OWNED [WS19 T19, 2026-07-29 — audit rows O13a/O13b].** Flagged by gate
> G-P2 as an open claim with no owner; this is the second copy of the three-file bundle.
> Same disposition as the C6 copy: **`main_gen.zig` is PORTED** (
> `crates/rustty-synthetic/src/main_gen.rs`, registered as a bin at
> `Cargo.toml:26-28`) so that third is STALE; **`main_wasm.zig` → WS14**;
> **`main_build_data.zig` + the `+gen-*` emit wiring → WS13** (added to WS13's plan).

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

## rustty-vaxis Chunk 1b (`gwidth.zig` + `unicode.zig`) — uucode-0.1.0 width-table fork

> ## ✅ RESOLVED — decided **Option B**, shipped `e0abb03` (2026-07-10), **RATIFIED 2026-07-29 (WS12 T2)**, gate proven non-vacuous (WS12 T4)
>
> **This is NOT an open blocker and has not been one since 2026-07-10.** Everything below
> the banner is retained as HISTORICAL RECORD — the Option A/B/C analysis is worth keeping
> so the reasoning survives — but the "needs a **human decision**" framing and the
> "Un-defer trigger" at the end of this section are **stale prose describing a decision
> that has already been made and implemented**. Read them as history, not as work items.
> (This row was still presenting itself as an open human decision as late as the WS12 plan,
> which is exactly the rot this banner exists to stop.)
>
> **What shipped (Option B):** a vendored `uucode` 0.1.0 runtime subset as a module inside
> `rustty-vaxis` at `crates/rustty-vaxis/src/uucode/` (1,035 LOC across
> `{mod,get,grapheme,tables,types,utf8}.rs`), whose stage1/stage2 tables are regenerated at
> build time by `crates/rustty-vaxis/build.rs` (552 LOC) from the 3.7 MB vendored UCD 16.0.0
> corpus at `crates/rustty-vaxis/ucd/`. `gwidth.rs` (311 LOC, 12 `#[test]`) and `unicode.rs`
> (94 LOC) are LIVE. The UCD corpus is a **build-time input only** and is never shipped in
> any artifact.
>
> **Ratification:** the human (project owner) ratified Option B verbatim on 2026-07-29;
> the decision text, the tradeoff reasoning, and the reason Option A was rejected are
> recorded in `.port/phase-2/VAXIS_C1B_DESIGN.md` § "WS12 decisions". Option A would first
> require PROVING that a uucode-0.2.0 + ghostty-stripped-config derivation reproduces
> 0.1.0-vanilla across all 1,114,112 code points and all four fields, to save 3.7 MB of
> build-time text at the cost of permanently coupling `rustty-vt` — the port's most
> load-bearing crate — to a TUI library's dependency pin. Option C (mapping `eawToWidth`
> onto ghostty's derived `width`) stays rejected: it is a different algorithm and would
> fake widths.
>
> **Why the gate is not vacuous (WS12 T4), because ratification by prose alone is not a
> gate:** perturbing a single generated table entry in a scratch tree
> (`eaw[0x4E00] = EAW_NEUTRAL`, forcing CJK IDEOGRAPH-ONE from `Wide` to `Neutral`) turns
> **both** `uucode::tables::tests::spot_check` **and** the behavioural
> `gwidth::tests::gwidth_missing_wide_anchor` RED (`95 passed; 2 failed`), and reverting
> that one line returns `97 passed`. Additional pins added by T4: the UAX #29 corpus loop
> now asserts it drove ≥ 1000 cases (a truncated `include_str!` corpus previously passed
> silently — a 40-line corpus drives 16 cases and is now caught), and
> `crates/rustty-vaxis/tests/ucd_provenance.rs` verifies all six vendored UCD inputs against
> SHA-256 digests recorded in `ucd/PROVENANCE.md`.

### Historical record — the original blocker write-up (2026-07-10)

The vaxis port (`crates/rustty-vaxis`, vendored-dep-port of libvaxis 0.5.1) lands its
CORE VALUE TYPES in **Chunk 1a** (`Cell`/`Key`/`Mouse`/`GraphemeCache`/`Winsize` +
`Image` struct-shape — READY, dep-free; see `VAXIS_C1_DESIGN.md`). The two
grapheme-width / grapheme-segmentation files were **DEFERRED to Chunk 1b** on a
unicode-data blocker that needed a **human decision** (Option A vs B below):

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
> **OWNER [WS19 T19, 2026-07-29]: WS12 (vaxis completion).** Flagged by gate G-P2 as an
> open claim carrying a trigger but NO owner. WS12 is the workstream that completes
> `rustty-vaxis`, so the uucode/width-data decision and its consequences belong to it —
> including making the decision itself, which is the trigger's precondition.

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

1. ~~**`tty.rs` — `WindowsTty` (`#[cfg(windows)] mod windows_tty`).**~~ **PORTED — WS20
   (2026-07-30).** The full `tty.zig:189-691` body is LIVE: `GetStdHandle`/
   `GetConsoleOutputCP`/`GetConsoleMode`/`SetConsoleMode` init + `deinit` restore, the
   `CONSOLE_MODE_INPUT`/`CONSOLE_MODE_OUTPUT` bit layouts, the buffered `WriteFile`
   writer, and the whole `ReadConsoleInputW` + `INPUT_RECORD` key/mouse/resize/focus
   decode (`next_event`/`event_from_record`/`translate_mods` + the UTF-16 surrogate
   pairing). The Win32 ABI is hand-rolled `extern "system"` against `kernel32` (the
   `rustty-os::windows` / `rustty-xev::windows` convention; NO new crate). The two
   upstream compile errors on that path are repaired and documented in the module PORT
   NOTE — `WindowsTty.read` calls `posix.read(self.fd, ...)` against a field the struct
   does not have (ported as `ReadFile` on the stdin handle), and upstream has NO
   `WindowsTty.getWinsize` at all (ported as `GetConsoleScreenBufferInfo`, the same call
   the resize arm already makes). One rustty-only SOUNDNESS divergence, documented at
   the field: upstream's `EventState` (which owns the `ansi_buf` the returned escape-
   sequence `Event` BORROWS) is a `nextEvent` local, so that event points into a dead
   frame — the state is hoisted into the tty and reset on entry to `next_event`, which
   is behaviourally identical and not a UAF. **VERIFICATION DEBT (owner WS20):** cannot
   be cross-compiled on this host (`zlib-sys`), see
   `.port/phase-2/VERIFICATION_DEBT.md`; type-checked + clippy-clean on the host with
   the windows `cfg` arms forced on.

2. ~~**`vaxis.rs` — the `Vaxis` INHABITED placeholder leaves.**~~ **RESOLVED by
   Chunk 4a** (commit 9500493): the real `Vaxis<'a>` replaced the placeholder;
   `translate_mouse` (vaxis.rs:890), `device_status_report` (vaxis.rs:1143), and
   `wake_query_futex` (vaxis.rs:1153) are all LIVE. The 4a residuals (the two
   `zigimg` seams) are the **Chunk 4a/4b section below**.

3. **`tty.rs` — `TestTty::next_event`.** A Windows-API-compat stub — upstream's body is
   literally `return error.SkipZigTest;`, so the `unimplemented!()` IS the port of that
   body, not a seam. WS20 brought its signature to parity with the (now live)
   `WindowsTty::next_event` (raw receiver, `Event`-returning) so the `Tty` alias exposes
   ONE surface. `TestTty` itself is now `cfg(all(test, not(windows)))`: upstream's
   `TestTty` calls `posix.pipe`, so it would not compile on a Windows target either, and
   a Windows test build aliases the real `WindowsTty`.

   > **CORRECTION [WS20, 2026-07-30].** The "OUT-OF-SCOPE-BY-PLATFORM per `ROADMAP:214`"
   > note that stood here (WS19 T19) is STRUCK. "Out of scope by platform" is not a
   > valid disposition: everything ghostty/libvaxis supports is in scope, and inability
   > to build or run it on this host is verification debt with a recipe, not an
   > exclusion. Both items are now ported; the residual is recorded in
   > `.port/phase-2/VERIFICATION_DEBT.md` with owner **WS20**.


4. ~~**`loop_.rs` — the `test Loop` lifecycle test (`#[ignore]`).**~~ **RESOLVED by
   Chunk 4a**: `loop_lifecycle` (loop_.rs:558) is UN-IGNORED — the filled
   `device_status_report` lets the init/start/stop lifecycle run to completion
   against `TestTty` (see the comment at loop_.rs:525).

NOTE (T14 reconciliation, 2026-07-10): item 1's Windows seam family also covers
`loop_.rs:341` (`#[cfg(windows)]` `tty_run` arm) — same `#[cfg(windows)]` class.
**PORTED — WS20 (2026-07-30):** the `.windows` arm of `Loop.ttyRun` (`Loop.zig:116-122`)
is LIVE, and so is the previously-undropped `.windows` arm of `handleEventGeneric`
(`Loop.zig:167-234`), which is NOT a strict subset of the POSIX arm — its `winsize` skips
the in-band-resize latch and `resetSignalHandler`, and `mouse_leave`/`paste*`/the color
reports/every `cap_*` but `cap_da1` fall into its `else => {}`. `Loop.winsizeCallback` is
`cfg`-gated off on Windows exactly where upstream elides it (upstream's `Loop.init` is
`.windows => {}`, so Zig never analyzes the callback's reference to the non-existent
`WindowsTty.fd`).

## rustty-vaxis Chunks 4a/4b (the real `Vaxis<'a>` + widgets) — the image tier [added 2026-07-10, T14 reconciliation — VALIDATION_REPORT M5]

Chunk 4a (commit 9500493) replaced the 3b placeholder with the real `Vaxis<'a>`
(render diff-and-emit engine, capability queries, kitty-graphics transmit
paths); Chunk 4b (commit 17bcf2d) added the `widgets`/TextInput tier.

> ## ✅ RESOLVED — all four functions LIVE, WS12 T8–T11 (2026-07-29)
>
> **The image tier is no longer deferred, and `rustty-vaxis` has zero non-`#[cfg(windows)]`
> `unimplemented!()`.** Verified: `grep -rn 'unimplemented!\|todo!' crates/rustty-vaxis/src/`
> now returns only the `#[cfg(windows)]` family (`tty.rs:419,422,425,439,442,445,539`;
> `loop_.rs:341`) plus two doc-comment mentions. Both former items are discharged:
>
> 1. **`Vaxis::transmit_image` + `Vaxis::load_image` — LIVE** over the new
>    **`crates/rustty-zigimg`** crate (WS12 Decision 2, **Option Z-C**, ratified 2026-07-29;
>    the tradeoff table and the decision text are in
>    `.port/phase-2/VAXIS_C1B_DESIGN.md` § "WS12 decisions"). Note this row's own
>    un-defer trigger read "a `rustty-zigimg` port **(or an R3 pure-Rust re-home of the two
>    conversions)**" — the parenthetical is exactly what was taken. `rustty-zigimg`
>    (≈840 src LOC + 264 test LOC) re-homes the **six** zigimg operations `libvaxis`
>    actually instantiates (`Vaxis.zig:983-1033`) onto **ghostty's own vendored codecs**:
>    decode via `rustty-wuffs` (whose RGBA_NONPREMUL output *is* zigimg's `rgba32`),
>    a faithful `rgba32`↔`rgb24` converter, and a local PNG encoder over `zlib-sys`. It is
>    explicitly **not** a port of zigimg's 15 format modules (18,316 src + ~10,700 test LOC,
>    for a surface no ghostty code path calls) — that was Option Z-A and is rejected.
>    `transmit_image`'s dropped `img` parameter is restored (the 3-arg signature is what had
>    made the seam un-callable).
>
> 2. **`Image::draw` + `Image::cell_size` — LIVE**, and this row's stated rationale turned
>    out to be **wrong in an instructive way**: they were said to "defer WITH the zigimg
>    tier", but they need **zero** zigimg. Both are pure `Window`/`Screen` geometry and
>    could have been filled at any point after Chunk 2; only *reachability* ever depended on
>    item 1. Both had also silently dropped their `win: Window` parameter, which is the
>    actual reason they were uncallable. Parameter restored.
>
> **THE ONE DIVERGENCE, bounded and named — a PORT NOTE, not a seam and not a deferral:**
> the set of input file formats `load_image` accepts is **PNG + JPEG** rather than zigimg's
> 15. That is a *superset* of what ghostty itself decodes anywhere (both kitty graphics and
> background images go through `pkg/wuffs`, i.e. PNG+JPEG), and it is documented at
> `crates/rustty-zigimg/Cargo.toml` and `src/decode.rs`. If a future consumer needs
> BMP/GIF/TGA input, that is a new workstream (Option Z-A), **not** a WS12 residual.
>
> **Also preserved deliberately (an upstream quirk, found while KAT-ing):** `Vaxis.zig:996`
> sizes its PNG buffer at `img.imageByteSize()` — the *raw* pixel byte count — and a PNG has
> ~57 bytes of unavoidable fixed overhead, so **a tiny image cannot be transmitted as PNG at
> all**. A 4×4 rgba32 image (64-byte budget) legitimately overflows, upstream too. Surfaced
> as `ImageError::Decode(NoSpaceLeft)`; not "fixed" by widening the buffer.
>
> **Gates:** 22 KATs in `crates/rustty-vaxis/tests/image_tier.rs` (all four `Scale` arms and
> the `contain` early break asserted to *different* hand-computed values; `cell_size`'s `try`
> and `catch 0` arms asserted separately; `transmit_image` pinned to the COMPLETE expected
> escape byte string; zero-byte-sink negative controls on both `NoGraphicsCapability`
> paths). 28 tests in `rustty-zigimg`, whose PNG round trip is bit-exact through
> `rustty-wuffs` and is negative-controlled by an IDAT byte flip. `check_acyclic.sh` GREEN
> with the new `rustty-vaxis → rustty-zigimg → {rustty-wuffs, zlib-sys}` edges.
>
> Miri: the FFI-touching arms are `#[cfg_attr(miri, ignore)]` with rows in
> `.port/phase-2/MIRI_EXCEPTIONS.md`; every geometry KAT stays Miri-live.
>
> **The `#[cfg(windows)]` family was NOT struck by WS12; it is struck by WS20
> (2026-07-30)** — see "rustty-vaxis Chunk 3b" items 1/3, both now PORTED. What remains
> is verification debt (no Windows host, and no MSVC/mingw cross toolchain here), not
> unported code: `.port/phase-2/VERIFICATION_DEBT.md`, owner WS20.

### Historical record — the original seam write-up (2026-07-10)

The chunk sequence for the libvaxis port is FINISHED — these were the crate's remaining
live seams (verbatim reference bodies preserved at each; this section replaced
the formerly-dangling "(DEFERRALS.md)" citations in vaxis.rs, which pointed at
no entry):

1. ~~**`vaxis.rs:997` `Vaxis::transmit_image` + `vaxis.rs:1018` `Vaxis::load_image`
   — the `zigimg` seams.** Both decode/convert image payloads via the unported
   Zig-source dep `zigimg` (`img.convertToPixelFormat`, PNG decode) before the
   (LIVE) kitty transmit paths. Same dep class as libxev/z2d/vaxis itself: a
   vendored-dep port. **Un-defer trigger: a `rustty-zigimg` port (or an R3
   pure-Rust re-home of the two conversions), needed only when a TUI consumer
   transmits images.**~~ OWNER was **WS12**; **discharged by WS12 T8/T9.**
2. ~~**`image.rs:223` `Image::draw` + `image.rs:248` `Image::cell_size`.** The
   "(later chunk)" wording in-source is STALE (the chunk sequence is finished);
   the honest state: both consume a live `Window`/`Screen` but are unreachable
   until item 1 lands (a vaxis `Image` can only be constructed through
   `transmit_image`/`load_image`), so they defer WITH the zigimg tier.
   **Un-defer trigger: item 1.**~~ **Discharged by WS12 T10/T11** — and, as the banner
   notes, the "defer WITH the zigimg tier" reasoning was wrong: no zigimg was needed.

  > **OWNER [WS19 T19, 2026-07-29]: WS12 (vaxis completion).** Flagged by gate G-P2 as an
  > open claim with NO owner. Same class as `Vaxis::transmit_image`/`load_image` above:
  > both depend on the image-decode tier (`rustty-zigimg` or an R3-acceptable
  > substitute), and WS12 is the workstream that completes `rustty-vaxis`. Naming WS12
  > here is what stops these two rows drifting back into ownerlessness — the gate now
  > fails if they do. **[Closed 2026-07-29 by WS12 T8–T11.]**


No other live seams exist in the crate outside the `#[cfg(windows)]` family +
`TestTty::next_event` (Chunk 3b items 1/3 above).

## rustty-font Atlas wasm surface — per-test deferral ledger (VALIDATION_REPORT §3e)

**RESOLVED — WS14, 2026-07-30 (audit row O14). All 4 assertions are LIVE.**

~~The reference `font/Atlas.zig` `pub const Wasm` block (the C-ABI/JS atlas surface
for the wasm build) is feature-gated OFF in `crates/rustty-font/src/atlas.rs`
(off-by-default `wasm` cargo feature; the whole block is preserved verbatim as a
comment at atlas.rs — R1, no reference line dropped). Its ONE test defers with it
and is tracked here PER-TEST so `check_test_counts`/G2 auditors can resolve it:~~

**The archaeology matters, because this is the single most confusing item in the
ledger.** "Port the test with all 4 asserts live" was not merely deferred work — as
written it was **impossible**, and would have stayed impossible however long it sat
here. Assert 1 is `try testing.expect(wasm.isHostOwned(reg));`, and
`wasm.toHostOwned` / `isHostOwned` / `toModuleOwned` were **DELETED upstream by
`9dc2e5978` (#9301, 2025-10-21)**. `Atlas.Wasm` still calls `wasm.toHostOwned` at
the pin, so the reference block **dangles and does not compile upstream either**.

Closing it therefore required **resurrecting the deleted host-ownership tracker**
(`crates/rustty-os/src/wasm.rs` + `wasm/host_owned.rs`, feature `wasm-host-owned`,
ported from `git show 9dc2e5978^:src/os/wasm.zig` with `9dc2e5978` cited in-source,
carrying its own **2** reference tests). That is the same accepted precedent as the
`web_canvas` face/shaper resurrection — port the deleted-upstream code the pinned
reference still depends on, rather than silently dropping the assertion.

Disposition:
- the `Wasm` block is ported LIVE under `#[cfg(feature = "wasm")]` in
  `crates/rustty-font/src/atlas.rs` (7 C-ABI exports, unprefixed by design — the JS
  host binds them by bare name — plus `atlas_debug_canvas`);
- `test "happy path"` runs natively with **all 4 asserts live** under
  `cargo test -p rustty-font --features wasm`;
- `check_test_counts.py`'s `font_atlas` `apc_deferred` goes **1 → 0** (13/13);
- `atlas_debug_canvas` is additionally asserted in the browser: gate arm **P5**
  requires its canvas readback to agree with the WebGL path
  (`.port/tools/gates/ws14_webgl_browser_gate.sh`), proven falsifiable against
  product code.

The original per-test row, for the record:

- ~~**`Atlas.Wasm` `test "happy path"`** (Atlas.zig:564; commented verbatim at
  atlas.rs "test \"happy path\"") — **4 assertions deferred with the surface**~~ — **ALL 4 NOW LIVE**:
  1. `try testing.expect(wasm.isHostOwned(reg));`
  2. `try testing.expect(reg.width > 0);`
  3. `try testing.expect(!atlas.modified);`
  4. `try testing.expect(atlas.modified);` (after `atlas_set`)
  It exercises `atlas_new`/`atlas_reserve`/`atlas_set`/`atlas_free` + the
  `os/wasm.zig` host-ownership helpers — none of which exist outside the wasm
  artifact. **Target: the rustty wasm app-runtime** — **OWNER [WS19 T14, 2026-07-29]: WS14 (wasm/WebGL)**; "the rustty wasm app-runtime" is a milestone name, not a workstream, so these rows were ORPHANED. Gate **G-P2** now FAILS on any open row whose owner is not an OPEN workstream (the same milestone that
  un-defers the config/vt wasm exports); porting the Wasm block MUST port this
  test with all 4 asserts live.

  > **OWNER [WS19 T14, 2026-07-29 — audit row O14]: WS14 (wasm/WebGL).** ~~The row targeted "the wasm-artifact milestone", which is not a workstream — it was ORPHANED. Re-derived repo-wide: `crates/rustty-font/src/atlas.rs:740` carries the block as `PORT NOTE (wasm — feature-gated OFF)` with the reference preserved as comments, so the row is genuinely OPEN and lands with the wasm atlas surface.~~ **CLOSED by WS14, 2026-07-30** — see the RESOLVED note at the head of this subsection; the "S" size estimate was wrong because the audit did not know assert 1's dependency had been deleted upstream.
>
> Gate **G-P2** (`.port/tools/gates/check_phase2_closure.py`) FAILS on any open row without an owner in an OPEN workstream, so this attribution is enforced, not aspirational.


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

## WASM/web target — VT C-ABI wasm + `web_canvas` DONE (2026-07-15); WebGL / `main_wasm` / `Atlas.Wasm` closed by WS14 (2026-07-30)

> **SCOPE NOTE [WS14, 2026-07-30]. Read this before concluding "wasm is finished"
> from the heading.** The heading used to read "WASM/web target — ✅ DONE", which a
> grep could — and did — take as covering the whole wasm/web surface. It did not.
>
> **What the 2026-07-15 pass actually covered:** the wasip1 `rustty_vt_c.wasm`
> (the libghostty-vt C-ABI, node-WASI runtime-verified), the freestanding
> `rustty_web_canvas.wasm`, and the `web_canvas` face + shaper, all browser-verified.
>
> **What it did NOT cover:** *any* GPU or WebGL path — **every pixel in that harness
> is drawn by the 2D Canvas** — plus the four ledger items O13a (`main_wasm.zig`
> artifact), O14 (`Atlas.Wasm` + its 4-assert test), O15 (`App.zig`'s `Wasm` block)
> and O16 (`renderer/WebGL.zig`). Those are closed by **WS14**
> (`docs/superpowers/plans/2026-07-23-ws14-wasm-webgl.md`), which adds a THIRD wasm
> artifact — `rustty_wasm.wasm` (`main_wasm.zig`'s port; 18 exports, 4 imports, all
> `env`, zero wasi, 586 KB) — and the first GPU-path proof in the tree: WebGL2
> rendering of the `Atlas.Wasm` texture in a real Chrome, gated by
> `.port/tools/gates/ws14_webgl_browser_gate.sh` / `ci.sh --webgl`.
>
> Note also that `rustty_vt_c.wasm`'s wasip1 lane needs
> `rustup target add wasm32-wasip1` on a fresh machine (only
> `wasm32-unknown-unknown` is installed by default here), so a missing artifact
> there is a toolchain gap, not a regression.


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
   ~~(`main_wasm.zig` stays un-ported: it is the dead monolithic playground entry;
   the modern equivalent is the per-C-ABI wasm module, which is what we build.)~~
   **STALE [WS14, 2026-07-30]:** `main_wasm.zig` IS ported
   (`crates/rustty-wasm/src/lib.rs`) and now ships as a real freestanding
   `rustty_wasm.wasm` cdylib. "Dead upstream" is a reason to record the divergence,
   not a licence to skip the port — the same judgement that resurrected
   `web_canvas` in this very item.
2. ~~**Target is wasi, not browser-freestanding `wasm32-unknown-unknown`**~~
   **RESOLVED (2026-07-15).** Built a TRUE freestanding
   `target/wasm32-unknown-unknown/release/rustty_web_canvas.wasm` (**1.06 MB, 199
   exports, 14 imports, `WebAssembly.Module.imports` modules = `["env"]` only — ZERO
   wasi imports**; the "936 KB / 184 exports" first recorded here was stale-low and
   is corrected to the 2026-07-30 measurement so the row stays checkable. **Now 1.12
   MB / 205 exports / 14 imports** after the WS19 T7 `DeferredFace` web_canvas port
   added the six `rustty_wc_deferred_*` harness exports — see the
   `rustty-font-shaper` section below), loadable
   directly in a browser with no wasi shim. Achieved by
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
4. ~~The earlier `rustty-wasm` crate (config/core/font, the `main_wasm.zig` analog)
   is the legacy path — retained (builds natively) but NOT the wasm deliverable.~~
   **SUPERSEDED — WS14, 2026-07-30.** `crates/rustty-wasm` is no longer an
   rlib-only "legacy path": it is a `cdylib` that builds for
   `wasm32-unknown-unknown` and IS a wasm deliverable alongside the two above —
   `rustty_wasm.wasm`, the faithful `main_wasm.zig` artifact (`atlas_*` from
   `font/Atlas.Wasm`, `config_*` from `config.Wasm`, plus `malloc`/`free`;
   4 imports, all `env`, zero wasi). Its retention set is now the reference's,
   corrected: `font` ADDED, `core` DROPPED — the reference's `App.Wasm` emits zero
   symbols, so the `rustty-core` edge retained nothing while dragging
   `rustty-inspector → dcimgui-sys → freetype-sys → zlib-sys` into a freestanding
   link. Built by `.port/tools/build-main-wasm.sh`; shape-asserted by
   `crates/rustty-wasm/tests/artifact_shape.rs`; rendered and pixel-gated by
   `example/wasm-webgl/`.

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
- ~~**`deferred_face.rs:735/738/745` — the 3 WebCanvas `DeferredFace` arms**
  (`unimplemented!("WebCanvas DeferredFace is DEFERRED (wasm backend)")` on
  `has_codepoint`/`name`/`load`)~~ **RESOLVED (2026-07-30, WS19 T7).** The premise
  had gone STALE the moment WS14 resurrected the `web_canvas` face: the oracle
  (`src/font/DeferredFace.zig`) has real `.web_canvas` bodies for `familyName`,
  `name`, `hasCodepoint` and `loadWebCanvas`, and every primitive they need
  (`Face.initNamed`, `Face.glyphIndex`) was already ported in
  `face/web_canvas.rs`. All four are now real ports in a new
  `deferred_face::web_canvas_payload` module, alongside the reference
  `DeferredFace.WebCanvas` payload struct (`font_str: Vec<u8>` — byte-slice
  fidelity, never a lossy `String` — plus `presentation`); the wasm `_deferred: ()`
  placeholder field is replaced by the real `wc: Option<WebCanvasData>`, mirroring
  the macOS `ct` / Linux `fc` shape. **No `unimplemented!` remains in this file.**
  **Falsifiable gate** (this port is wasm-only, so a host `#[test]` cannot reach
  it): six LOCAL `rustty_wc_deferred_*` harness exports were added to the
  `rustty-web-canvas` cdylib root — the same "local helper export" class as
  `rustty_wc_atlas_*`, NOT a port of anything (the reference `DeferredFace.zig`
  carries no `Wasm` C-ABI) — and `.port/tools/web-canvas-instantiate.mjs` gained a
  `(c4)` block that ASSERTS values, not merely "did not trap":
  `name`/`family_name` must equal the payload's font string exactly, and
  `has_codepoint` must be `true` for a null presentation, `true` for a matching
  one, and `false` for a mismatched one (the reference's no-face-init fast path).
  Recipe: `sh .port/tools/build-web-canvas-wasm.sh --release && node
  .port/tools/web-canvas-instantiate.mjs`. **Mutation-proven RED twice:** neutering
  the presentation fast-path (`if false && …`) →
  `FAIL: deferred has_codepoint('A', emoji) should be false`; truncating the `name`
  copy (`&wc.font_str[..1]`) → `FAIL: deferred name = "b"`. Both reverted, green
  re-confirmed.

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

**Follow-up — ✅ RESOLVED (2026-07-29, WS19 T10; audit row O9).** ~~worker-thread
`&mut Loop`/`wakeup` aliasing — kqueue.rs twin + `task_loop` lineage audit.~~ Filed
2026-07-19 as "open, low" INSIDE this RESOLVED entry, which is why it stayed invisible.

The original prescription — "apply epoll's copied-fd notify to the kqueue twin" — was
**disproven by the harness**, which is exactly why the doctrine forbids hand-reasoning
SB. Five new `.port/phase-2/mirihar-xev-mpsc` candidates settle it (full 14-row matrix in
that README; `bash run-all.sh` reproduces):

| candidate | SB | TB |
|---|---|---|
| `wakeup_naive` — shipped kqueue `(*(*c).task_loop).wakeup()` | **UB** | **UB** |
| `wakeup_copied` — shipped epoll "fix": by-value `Copy` handle read through `task_loop` | **UB** | CLEAN |
| `wakeup_completion_cached` — **APPLIED FIX** | CLEAN | CLEAN |
| `taskloop_lineage_naive` — shipped `(*c).task_loop = self`, deref'd after a later tick | **UB** | CLEAN |
| `taskloop_lineage_rooted` | CLEAN | CLEAN |

**Why "form no reference" is not enough.** The loop thread sits inside
`tick(&mut self)`/`run(&mut self)` for the entire time a pool worker runs, so it holds a
*function-entry, PROTECTED* `&mut Loop`. A by-value read of a `Copy` field is still a
**foreign access** on the `Loop` allocation, and protected tags may not be invalidated at
all — hence `wakeup_copied` is SB-UB. **This also corrects the M1 verdict above:** the
already-applied `(*(*c).task_loop).thread_pool_completions_ptr` read had the same defect;
`xthread_boxq` read CLEAN only because that candidate sequenced the two threads with a
channel handshake, so the loop thread was NOT holding its `&mut Loop`. The new candidates
deliberately remove that concession.

**Applied fix (both backends).** The worker must not touch the `Loop` allocation at all,
so `Loop::start`/`thread_schedule` copies the two handles it needs into the **caller-owned
`Completion`** (never inside `Loop`, hence immune to `Loop`'s per-tick reborrows) before
`schedule`, and `thread_perform` reads only from `(*c)`:
`task_completions: *mut TaskCompletionQueue` — **the reference's OWN field, restored**
after the Task-2 elision, so this half is a fidelity GAIN — and `task_wakeup`
(`Async`/`Eventfd`, `Copy`) — port-added, ledgered as a `[R1-DIVERGENCE]` below. Because
`task_loop` is no longer dereferenced on the worker, the lineage audit is discharged too:
its provenance (`taskloop_lineage_naive`: SB-UB) stops mattering. `COMPLETION_SIZE`
tripwires re-pinned (kqueue 248→264, epoll 208→224 — the `task_completions` half moves
the Rust size *toward* Zig's 256). Verified: `cargo test -p rustty-xev` 47+2 green on
macOS and on the Linux VM (epoll arm), `cargo +nightly miri test -p rustty-xev` no new
findings.

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

### 3. ~~rustty-termio `shell_integration.rs` — lossy path byte source (LOW, cleanup)~~ **RESOLVED — WS19 T15, 2026-07-29 (audit row O21)**

~~`TmpResourcesDir` (test-only helper) builds its owned `Vec<u8>` paths via
`to_string_lossy().into_owned().into_bytes()` (shell_integration.rs:1708, and
the `realpath_bytes` helper :1753-1763), deviating from the
zig-byteslice-to-rust-rawbytes rule (raw `OsStr` bytes, never lossy String).
Behaviorally benign (macOS tmp paths are UTF-8 in practice); ownership/free
discipline is faithful. **Fix:** `std::os::unix::ffi::OsStrExt::as_bytes` in a
later cleanup pass.~~

**RESOLVED.** Both sites now use `std::os::unix::ffi::OsStrExt::as_bytes` — the raw
platform bytes, which is the exact analogue of the reference's `realpathAlloc`
returning `[]u8` (POSIX paths are byte strings, not UTF-8). Owner was "a later cleanup
pass", i.e. nobody.

Landed with a **RED/GREEN discriminator, not a smoke test**:
`non_utf8_path_bytes_survive_the_conversion_but_not_the_lossy_one` asserts BOTH that
`OsStrExt::as_bytes` round-trips a `0xFF` byte exactly AND that
`to_string_lossy().into_owned().into_bytes()` mangles it into U+FFFD (`EF BF BD`) — so
the test fails if the fix is reverted, and also fails if the fixture ever stops being
non-UTF-8 (which would make it vacuous).

**Platform finding worth recording:** a filesystem-based fixture is IMPOSSIBLE on the
primary host — APFS rejects non-UTF-8 filenames outright (`create_dir_all` on
`b"ws19_\xffdir"` → `Os { code: 92, … "Illegal byte sequence" }`, observed). The
property under test is the CONVERSION, which is host-independent, so the main test does
not touch the filesystem; a sibling `#[cfg(target_os = "linux")]` test
(`realpath_bytes_round_trips_a_non_utf8_directory_name`) exercises the real
`realpath` round-trip on ext4/tmpfs, where such names are legal.

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

---

## GTK winproto (wayland/x11 arms) + dcimgui inspector — Linux/GTK tail [2026-07-18, final parity audit]

Recorded after the **final parity audit** (25 adversarially-verified agents over all Linux/GTK
features vs ghostty) confirmed these are the ONLY remaining Linux/GTK divergences and that they
are **not daily-use parity gaps on the common (GNOME/mutter) target**.

> **CORRECTION (2026-07-29, WS15 review):** "ONLY remaining" was not accurate — the portal
> `OpenURI` call site was also open; see "GTK portal OpenURI (L6)" above. Read this entry as
> covering the winproto-x11 and dcimgui divergences only.
> **UPDATE (2026-07-29, WS19 T8):** that portal gap is now CLOSED, so the correction is
> historical; the two divergences named below are once again the full set.

- ~~**`gtk/winproto/x11.rs` — the x11 window-protocol arm (L6).**~~ **RESOLVED.** The
  arm landed 2026-07-19 (`aae9536`) behind the `x11` cargo feature (`gdk4-x11` with
  the `xlib` feature, which re-exports `x11-dl`) — this bullet's "unported" wording
  was written one day earlier and was stale from 2026-07-19 onward. **WS16
  [2026-07-29] closed the three residual sub-deferrals the arm carried** and verified
  it live: XKB extension init in `App::init` (`x11.zig:71-101`), the `eventMods` XKB
  modifier peek (`x11.zig:114-159`), and `addSubprocessEnv`/`WINDOWID`
  (`x11.zig:320-329`) — the last of which also added the missing
  `winproto::Window::add_subprocess_env` dispatch (no-op on the `noop`/`wayland`
  arms, exactly as the reference), a `class::window::Window::winproto_mut()`
  accessor (`window.zig:386-388`), and the `class/surface.rs` `default_termio_env`
  call site (`surface.zig:1629-1638`, including the `<ENV_PREFIX>_QUICK_TERMINAL`
  var). The whole arm now matches `x11.zig` at the pin with ONE intentional
  non-gap: `supportsQuickTerminal` is `false`, which is what the reference itself
  returns (the quick terminal is a wlr-layer-shell/Wayland feature, `x11.zig:161`).
  `crates/rustty/Cargo.toml` gained `wayland`/`x11` pass-through features so the
  shipped binary can actually select an arm — without them the live app could only
  ever be the noop arm. VERIFIED live on the VM under `GDK_BACKEND=x11` (rootless
  XWayland, GNOME/mutter session): four KATs (`tests/gtk_x11_{winproto,xkb_init,
  event_mods,windowid}.rs`) plus the running binary logging `windowing protocol=X11`
  and `xprop` reads of `WM_CLASS`, `_MOTIF_WM_HINTS` (decorations word `0x0` with
  CSD vs `0x1` with `--window-decoration=server`), `_KDE_NET_WM_BLUR_BEHIND_REGION`
  (a well-formed CARDINAL rect list with `--background-blur=true`, absent without
  it), `WINDOWID` round-tripped through a real child process, and the `WM_HINTS`
  urgency bit appearing only after a bell on an unfocused window. Residual
  verification scope — NOT an implementation deferral — is ledgered in
  `VERIFICATION_DEBT.md` V5.
  Plan: `docs/superpowers/plans/2026-07-29-ws16-x11-winproto.md`.
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
    coupled sub-item — **RESOLVED, WS19 T9, 2026-07-29 (audit row O7)**: `Window` no longer
    snapshots the decoration mode. It shares the app's `Rc<RefCell<Pump>>` and re-reads
    `Globals` at each `decoration_mode`/`client_side_decoration_enabled`/`sync_blur` call,
    which is the reference's live `globals: *Globals` dereference
    (`wayland.zig:222`/`:252`/`:303`).
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
  coupled follow-up — **RESOLVED, WS19 T9, 2026-07-29 (audit row O7).** `Window` re-reads
  `Globals` live (the reference's `globals: *Globals` dereference at
  `wayland.zig:222`/`:252`/`:303`) instead of snapshotting at init; the same change made the
  KDE-decoration-manager presence and the blur capability live too, since the reference reads
  all three at the same points. Gated by
  `.port/tools/gates/ws19_wayland_deco_live_gate.sh` (nested KWin 6.7.3, GREEN, with the
  pre-fix snapshot FAIL arm demonstrated). Residual — the compositor-originated *transport*
  of a post-bind `default_mode` change — is recorded in `VERIFICATION_DEBT.md` (V6) with the
  measurement that established it.
- ~~**LOW (pathological): duplicate global re-advertisement.**~~ **RECLASSIFIED — WS19 T9,
  2026-07-29 (audit row O8): moved to `# Justified R1 Divergences` with a 4-criterion basis
  (input no compositor produces; the oracle has the same shape; the failure mode degrades to
  the graceful-`None` arm; the fix costs state the reference does not have). Not open work.**

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

1. ~~**`crates/rustty-inspector/src/widgets/pagelist.rs` cell-detail path still
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
   (`!src.contains("Node::page(")` over the WHOLE file).~~

   **RESOLVED — WS19 T7, 2026-07-29 (audit row O4).**

   **This item was ORPHANED: WS7 D6 MERGED at `b8c40f9` without closing it**, and was
   reported closed. It was not. Six restoring call sites were still live at
   `pagelist.rs:1701, :1703, :1759, :1889, :2163, :2379`, so inspecting a single cell
   in a compressed region silently decompressed that page — and **the guard test was
   VACUOUS-IN-SCOPE**: `ws7_tests::pagelist_widget_page_tree_never_restores` asserts
   over `loop_src`, the page-tree loop's substring only, so it stayed green while those
   six calls sat elsewhere in the same file. That is precisely the weaker form this
   deferral predicted, which is why the row said "tighten the guard to the WHOLE file".

   **What landed.** The oracle's end state, ported in structure:
   `pages.get_cell(pt)` → `pages.pin(pt)` + `Node::page_preserving_state(pin.node)`,
   a node-free `InspectedCell { row, cell, row_idx, col_idx }`
   (`oracle@main:pagelist.zig:14`), and the resulting `page: *const Page` **threaded**
   through `CellInfo::draw`, `hyperlinkTable` and `graphemeTable` instead of
   re-derived. Omitting the node from the aggregate makes the property **structural**:
   there is no longer anything to call the restoring accessor on. `preserved.deinit()`
   runs last, after every use of `page`, because for a compressed node `page` borrows
   `preserved`'s owned buffer.

   **The whole-file guard lives in a SIBLING file** —
   `crates/rustty-inspector/tests/pagelist_never_restores.rs` — because a whole-file
   assertion cannot live in the file it checks: its own string literal would appear in
   that file and satisfy it. It builds the forbidden token at runtime, skips
   comment-only lines (the PORT NOTES deliberately name the accessor to explain why it
   is banned), and carries a second test that would FAIL if `include_str!` ever
   resolved to the wrong file — so the guard cannot go vacuous either. The old in-file
   loop-scoped guard is kept, with its literal de-literalised for the same reason.

   **FAIL ARM PROVEN BY MUTATION, not asserted:** re-introducing one
   `Node::page(pin.node)` call → `test result: FAILED`, naming
   `src/widgets/pagelist.rs:1763`; reverted → `2 passed`.

2. ~~**`PageList::Limits` / `Limits::enforce` has no rustty counterpart.**~~
   **CLOSED — `P3-W3` T3, 2026-07-30 (re-vet row `P3-RV1`).** The resync DID
   introduce `PageList.Limits` (`5b2d3b7df` + `f4c68d65e`), so the pin-conditional
   assertion fired. `crates/rustty-vt/src/pagelist/mod.rs` now carries `Limits` /
   `Limit` / `Key`, and the oracle's 11th `markActivity` site is ported as the
   `page_compression.mark_activity()` call inside `PageList::enforce_limit` (the port
   of `Limits.enforce`). rustty's `mark_activity` site count is now **11**, matching
   the oracle.

   **One correction to the assertion as written.** The re-vet row said `enforce` must
   `mark_activity` **on entry**. That was true of the intermediate commit
   (`5b2d3b7df`'s `enforceMaxLines`) but NOT of the final oracle shape: `f4c68d65e`
   moved the bump to `if (removed == 0) pagelist.page_compression.markActivity();`
   *inside* the prune loop, after the active-boundary `break`. An `enforce` call that
   finds the limit exceeded but removes nothing (the oldest page still overlaps the
   active area) therefore does NOT invalidate an in-flight incremental compression
   traversal. The port follows the final oracle, not the assertion's wording.

   Guarded by `pagelist_replacements_preserve_compression_continuation_and_mark_activity`'s
   sibling, `pagelist_enforce_limit_marks_activity_and_preserves_continuation`
   (`crates/rustty-vt/src/pagelist/tests.rs`), whose fail arm was proven by removing
   the `mark_activity()` call.

## ~~Deferred to WS8 upstream sweep~~ ~~→ OWNER: WS10 (BL-4 perf series)~~ ~~**→ OWNER: Phase 3 — BEYOND-PIN**~~ — WS7 Phase C divergence #11 (`Screen::cursor_scroll_region_up`) — **✅ RESOLVED — PORTED (deferral re-audit, 2026-07-31)**

> **STATUS CORRECTED.** This row asserted the symbol was unported and parked it
> behind the pin. Both halves are now false. The oracle pin advanced
> `4749c4e93` → `6ad1fe7d8` on 2026-07-31 (`.port/ORACLE_SYNC.md:36`, 365
> commits), so `77190bd02` is now an ANCESTOR of the pin — and the port had
> already landed regardless: `crates/rustty-vt/src/screen.rs:1885`
> `cursor_scroll_region_up` (`_inner` `:1903`, `_slow` `:2054`), all 6 oracle
> tests (`screen.rs:12828`–`13097`) including
> `screen_cursor_scroll_region_up_renews_page_generation` (`:12857`), PLUS a
> fast/slow differential test the oracle does not have (`:13097`). Landed by
> `a8855af` (P3-W5 T3). The `invalidate_node_layout` site count now matches the
> oracle 5-for-5. See `.port/phase-3/DEFERRAL_REAUDIT.md` §B.
>
> The narrative below is HISTORY, retained for the record.

From **WS7 Phase C review (I2)**. The Phase C plan
(`docs/superpowers/plans/2026-07-23-ws7-scrollback-compression.md:3790,3830`)
prescribes porting `renew_generation`/`invalidate_node_layout` to
`Screen::cursor_scroll_region_up` "and its siblings" and mandates the test
`screen_cursor_scroll_region_up_renews_page_generation` (from oracle
`0bad91adb`). **Neither exists in rustty** — zero hits repo-wide for
`cursor_scroll_region_up` or that test name.

**Root cause:** `cursor_scroll_region_up` is defined by unported upstream
commit `77190bd02` ("stream/terminal/termio: a handful of scroll region
optimizations"), which is `MAIN_PARITY.md` §3's row for `77190bd02` — filed
under **BL-4** ("stream/terminal/termio performance series
… scroll-region opts", §2), not yet ported. `0bad91adb`'s 4th
`invalidateNodeLayout` call site lives inside that function; you cannot
invalidate a node-layout call site in a function the port doesn't have. The
omission is therefore **legitimate and unavoidable, not silent** — recorded
here as divergence #11 in the same class as the earlier recorded WS7
divergences.

**What IS ported, verbatim, from `0bad91adb`:** the other 3 Screen call sites
of `invalidate_node_layout` (`crates/rustty-vt/src/screen.rs:1536` inside the
fast-path rotate of `cursor_scroll_above`'s single-page branch, `:1618` inside
`cursor_scroll_above_rotate`'s per-page rotation loop, `:1651` on the final
cursor-page rotation) and all 3 Screen tests the oracle commit added (grep
`0bad91adb` in `crates/rustty-vt/src/screen.rs`, at the `test_write_string` /
`cursor_scroll_above` / `cursor_scroll_above_rotate` regression tests around
lines 11634, 11731, 11946).

**Disposition [REWRITTEN — WS19 T2, 2026-07-29, audit row O3].** ~~belongs to the
**WS8 upstream-sweep backlog** (BL-4)~~ — that was wrong **twice over**: WS8 is
`[x]` AUTHORED + EXECUTED + **MERGED** (a closed workstream, so the row was
ORPHANED), **and** BL-4 is the `stream`/`terminal`/`termio` **performance series**,
which has never been WS8's scope — it is **WS10**'s.

~~**OWNER: WS10 (BL-4 perf series).**~~ **OWNER: Phase 3 — `BEYOND-PIN`.**
[re-owned WS10 T18, 2026-07-30]

**Why this moved off WS10.** `77190bd02` is **beyond the pin** (`../ghostty` working
tree is pinned at `4749c4e93`); its source is the **upstream resync range**, which is
Phase 3's scope by definition. WS10 ported the rest of BL-4 from beyond-pin commits
the way WS5/WS7/WS8 did, but this row's substance is not a perf change — it is a
*missing function* whose absence is only visible against post-pin upstream, and
hand-transcribing it into the pinned tree means writing code Phase 3's upstream
re-vet will revisit anyway.

**This is a re-owning, NOT a deferral.** The owner is named (Phase 3), the trigger is
named (the pin advancing past `4749c4e93`), and the row carries the `BEYOND-PIN`
marker that `check_phase2_closure.py` requires before it will accept `Phase 3` as an
owner at all (`row_is_beyond_pin()`, gate lines 202-208 / 286-290). Phase 3 is not a
blanket escape hatch in that gate: it is accepted for exactly this case, work whose
source lies in the resync range.

**What lands together when Phase 3 ports it.** `Screen::cursor_scroll_region_up` +
`cursor_scroll_region_up_slow` + their 5 tests, `0bad91adb`'s 4th
`invalidate_node_layout` site, and the test
`screen_cursor_scroll_region_up_renews_page_generation` — **alongside** the function,
not retrofitted into WS7. Note the other three Screen `invalidate_node_layout` sites
from `0bad91adb` ARE already ported (`crates/rustty-vt/src/screen.rs:1536,1618,1651`),
as is the Terminal one (`terminal.rs:3004`); the single missing site is the one inside
this unported function.

**Miri note for whoever ports it:** the row shifts are `&mut Page`/`&mut [Cell]`
arguments re-read across `assert_integrity`, which is Stacked-Borrows UB in Rust
(memory: `rust-mut-args-strongly-protected`). Use raw-pointer args and prove it on an
isolated non-FFI Miri harness, per the `mirihar-chunk3b` precedent.

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
   **RESOLVED — WS7 Phase E (2026-07-29), commit `76bd5a9`.** Root-caused to a WRITE, not
   the read: `RefCountedSet::init`'s `fill(Item::default())` (the port of
   `@memset(items, .{})`), which every page runs twice over item arrays that live inside
   the page mapping. `Item<Style, ..>` holds three `style::Color`s, and a typed Rust write
   of an enum leaves the INACTIVE payload bytes uninitialized (verified with an isolated
   probe). Fixed by zeroing the item bytes and writing `meta` field by field — faithful,
   because the reference's `Item.value` has no default (its own comment says `undefined`),
   so writing zeroes is strictly MORE defined than the reference.
   **Two claims in the write-up above were WRONG and are corrected in `MIRI_EXCEPTIONS.md`:**
   the inter-table alignment gaps are NOT a source (nothing writes them and the mapping
   arrives zeroed), and the defect was NOT "entirely at the B7 call site" —
   `CompressedPage::init`'s whole-mapping read is exactly what the oracle's
   `compress/Page.zig` does with `page.memory`, so changing it would have been the fidelity
   break. Measured with a new seconds-long harness probe `mapinit` (the multi-ten-minute
   `roundtrip`/`leak` are no longer the primary probe): before, uninitialized at mapping
   offset 0x160 = the first style-set `Item` slot; after, "mapping of 16384 bytes fully
   initialized". **And on a REAL production test**, which is stronger evidence than the
   harness scenarios: `pagelist::tests::pagelist_compress_page_round_trips_a_cold_history_page`
   under Miri aborts with the exact `[u8; 8]` uninit error before the fix and reports
   `1 passed` (194.01s) after it; `pagelist_compress_drain_finishes_in_one_call`, which
   compresses SIX std-capacity mappings, also runs to completion post-fix (1631.36s, zero
   UB, zero leaks). The harness's own `roundtrip`/`leak` could NOT serve as the acceptance
   run: past the abort they interpret several std pages to completion, and two attempts
   were killed by the host at ~45 and ~65 minutes without finishing. A narrower RESIDUAL
   remains, now MEASURED rather than implicit
   (`mapinit_styled`, aborts at [0x17c]): storing a non-default `style::Style` re-poisons
   its own colour slots, because `RefCountedSet::add` must place the caller's `T` with a
   typed write. The only fix is making `style::Color` padding-free — 163 sites across 17
   files, a cross-crate type change outside this lane's fence — so it is carried with a
   full 4-criterion basis in `MIRI_EXCEPTIONS.md` and listed for WS8 below.
2. **I3 leak check is a manual proof artifact, not a gate.** Nothing in `.port/tools/` or
   `ci.sh` runs any `mirihar-*` crate, so `.port/phase-2/mirihar-pagelist-compress`'s
   `leakcheck` scenario (native, milliseconds, no Miri needed) demonstrates leak-freedom
   but does not automatically catch a future regression. This matches existing repo
   precedent (`mirihar-compress-lz4`, `mirihar-chunk3b`), so it is not being wired now —
   but at minimum the native `leakcheck` scenario should be wired into a gate in Phase E.
   **RESOLVED — WS7 Phase E (2026-07-29), commit `f79a762`.** Wired as
   `.port/tools/gates/check_pagelist_compress_leaks.sh`, registered in `ci.sh` as **G6b**
   (HARD). Its FAIL arm is PROVEN reachable, not assumed: `--self-test` runs a new
   `leakcheck_negative` scenario that reproduces the real leak through the real path
   (`mem::forget` of a compressed list, so no discard-restore runs) under the SAME
   assertion as `leakcheck`, and the gate is required to exit non-zero and report the
   leaked bytes — measured 27402 bytes, gate exit 101, self-test RC=0.

~~**Still open after Phase E (new, narrower, for WS8):** make `style::Color` padding-free
(byte-complete `#[repr(C)]` value with accessors instead of a Rust enum whose `None`/
`Palette` variants do not cover their whole slot) so that storing a non-default `Style`
into a page's style set no longer leaves uninitialized bytes in the mapping that
`CompressedPage::init` compresses. 163 sites across 17 files; reproduction is the
committed `mapinit_styled` harness scenario (seconds); 4-criterion acceptance basis is in
`MIRI_EXCEPTIONS.md`'s WS7 Phase E section.~~

**RESOLVED — WS19 T5 (2026-07-29).** `style::Color` is now a byte-complete
`#[repr(C)] { tag: u8, data: [u8; 3] }` with `Color::None` / `Color::Palette(idx)` /
`Color::Rgb(rgb)` kept as an associated const + two associated constructors (so the
construction sites are untouched) and reads routed through `tag()` + tag-checked
`palette()` / `rgb()` accessors. `const _: () = assert!(size_of::<Color>() == 4 &&
align_of::<Color>() == 1)` pins the no-padding property. Fidelity is *improved*: the four
bytes are exactly the reference's own `Color.Tag` + 24-bit `PackedStyle.Data` per-tag
content (`none => 0`, `palette => idx` with `_padding: u16 = 0`, `rgb => r|g<<8|b<<16`),
so `data_from_color` is now a plain little-endian widening and `color_key` a plain
4-byte load — see the PORT NOTE at the type. Re-measured blast radius: **160**
`Color::{None,Palette,Rgb}` occurrences across **18** files (the 163/17 above was stale),
of which only the ~22 *pattern-match* sites needed edits — in `rustty-vt`
(`style.rs`, `terminal.rs`, `screen.rs`, `kitty/graphics_unicode.rs`), `rustty-vt-c`
(`style.rs`, `render.rs`) and `rustty-inspector` (`widgets/style.rs`). All the
`rustty-cli` / `rustty-vaxis` / `rustty-font-shaper` hits were either a *different*
`Color` type (vaxis's own `cell::Color`) or expression-only, so they needed no change.

**The same run found and fixed one more instance of the class, not previously known:**
`RefCountedSet::insert` performed *whole-`Item` typed writes* (`*item = Item::default()`
when reaping a dead item, and `*items.add(chosen_id) = new_item` when placing the new
one). `Item<Style, u16, u16>` is 28 bytes with `meta` at 0..6 and `value` at 6..27, i.e.
it has **1 trailing padding byte** (`meta` is align-2, `Style` is align-1), and a typed
whole-struct write de-initializes that byte even though `init` had zeroed it. Both writes
are now FIELD-WISE (`value` then `meta`), the same accommodation `init` already documents.
Measured before/after on the `mapinit_styled` probe: uninit at `[0x17c]` → after the
`Color` reshape, uninit at `[0x187]` (= that padding byte) → after the field-wise writes,
`styled mapping of 16384 bytes fully initialized`. **This closes the whole
uninit-in-page-mapping class**; `mapinit` and `mapinit_styled` are both GREEN.


**[WS19 T2, 2026-07-29 — audit §E.1] Items 3 and 4 below were physically MIS-FILED:**
they had been appended AFTER the `# Justified R1 Divergences` heading and had come to
sit *inside* the `killPid` divergence entry, which made item 4's open work (the
`-C debug-assertions=off` vt Miri lane, audit row O2) invisible to anyone grepping for
open items — it read as part of a permanent divergence. Moved back here under their own
heading, restoring the 1–2–3–4 numbering. Not one word of their text is changed.

3. **`pagelist_decommit_pool_item_zeroes_the_dirty_region` has a real Stacked-Borrows
   violation under Miri (OPEN, discovered while proving WS7 Phase C's I1).** Confirmed
   deterministic in isolation — full backtrace and root cause in
   `.port/phase-2/MIRI_EXCEPTIONS.md`'s Category 2 correction note. **Pre-existing (WS7
   B1, commit `39cd6b8`; last touched at `0413d2b`), NOT introduced by Phase C** — no
   Phase C commit touches `mem.rs` or this test. It is a test-code bug (holds a `&mut
   [u8]` across a call that mutates the same bytes through a separate raw pointer, then
   reads through the stale reference), not a production-code bug; native `cargo test`
   and ASan both stay green. The claim in `MIRI_EXCEPTIONS.md`'s Category 2 table that
   this test "IS Miri-provable" was asserted without having actually been run under Miri
   — now corrected in-place. Phase E must either fix the test (re-derive the slice fresh
   after each `decommit_pool_item` call) or record a formal exception; it must NOT be
   left silently claimed-clean.
   **RESOLVED — WS7 Phase E (2026-07-29).** Fixed exactly as sketched: every slice is now
   derived fresh from `ptr` and none is live across `decommit_pool_item`. Assertions
   unchanged (both the fully-dirty and the partially-dirty item must still read back
   all-zero across the whole buffer). Measured under Miri: was the retag UB above, now
   `1 passed` in 13.28s. Before/after output recorded in `MIRI_EXCEPTIONS.md`'s Category 2
   correction note, which no longer claims anything unverified.
4. **Several WS7 Phase B compression tests are ALSO Miri-intractable and undocumented
   (OPEN, discovered the same pass).** Attempting a bounded full-module pagelist Miri
   sweep (beyond I1's specific ask) found `pagelist_compress_incremental_converges_to_complete`,
   `pagelist_compress_full_leaves_viewport_and_active_resident`,
   `pagelist_compress_drain_finishes_in_one_call`, and
   `pagelist_compress_full_mode_leaves_no_continuation` all TIMEOUT at a 2–3 min bound —
   confirmed in single-worker isolation (not a parallel-contention artifact), and
   `…_leaves_no_continuation` reproduces identically on **pre-Phase-C** commit `098fc63`,
   proving this is a **pre-existing WS7 Phase B gap** (these tests were apparently never
   run under a bounded Miri harness before), not a Phase C regression. `MIRI_EXCEPTIONS.md`
   had cited two of these as "lighter, passing" coverage for a Phase C exception row; that
   citation is now corrected (see the row's inline correction). Out of scope for the Phase
   C review response — Phase E should measure the rest of the `compress`/`pagelist_compress`
   family the same way (`bash .port/tools/miri-run.sh rustty-vt pagelist_compress 8 3 5`)
   and either annotate the genuinely-intractable ones or investigate why compression state
   machines are this expensive under Miri interpretation.
   **RESOLVED — WS7 Phase E (2026-07-29).** Whole family measured, per test, with every
   candidate re-verified in single-worker ISOLATION before being excepted (a parallel
   TIMEOUT is a candidate, never a verdict). Full table in `MIRI_EXCEPTIONS.md`
   ("WS7 Phase E — the page-compression test family"). Of **12** measured:
   * **10 annotated** — the 4 named above plus the 4 other `pagelist_compress_*` tests,
     plus **2 that this pass discovered and the earlier one had not named**
     (`pagelist_mutations_mark_compression_activity`,
     `pagelist_memory_stats_accounts_compressed_and_resident_bytes`). All 10 TIMEOUT at a
     6-min bound in isolation.
   * **1 TRACTABLE and deliberately left un-annotated** —
     `pagelist_page_preserving_state_leaves_a_compressed_node_compressed`, ok in **77s**.
     Blanket-annotating the family would have hidden a working check, exactly as the
     Phase C pass warned.
   * **1 already excepted** —
     `search::pagelist::…::compressed_history_match_spanning_page_boundary_remains_compressed`.
     `miri-run.sh` scored it `res=FAIL` / `VERDICT: PROBLEM`; the per-test output file
     reads `ignored, MIRI EXCEPTION …`. Worked example of the harness's known
     ignored-test-as-FAIL misclassification.

   **Root cause, measured (this was the "investigate why" half of the ask).** Not hangs and
   not a logic difference from the tractable siblings — pure volume, on two independent
   measurements. (a) Natively these are the slowest PageList tests by two to three orders
   of magnitude (0.50–4.48s debug vs ~0.00s typical): `fill_compressible_history` calls
   `grow()` once per row until N std-capacity 215x215 pages exist, then `compress()`
   LZ4-compresses AND verify-decompresses every byte of each mapping. (b) Two of them were
   run to COMPLETION under Miri — `pagelist_compress_drain_finishes_in_one_call` **ok in
   1631s**, `pagelist_compress_page_round_trips_a_cold_history_page` **ok in 194s**, both
   with ZERO UB and ZERO leaks. That is direct positive criterion-3 evidence, not an
   argument from a lighter sibling.

   **One lever found, and a follow-up for WS8 rather than a change here.** Those completion
   runs used `-C debug-assertions=off`, which disables `Page::assert_integrity` (O(page),
   runs on nearly every PageList mutation; the `mirihar-pagelist-compress` harness already
   uses this accommodation on the documented grounds that it adds no SB/TB or leak
   coverage). With it, `pagelist_compress_page_round_trips_a_cold_history_page` goes from
   TIMEOUT(360s) to **ok in 194s** — so the page-level tests are dominated by
   `assert_integrity` and would be tractable in a `RUSTFLAGS=-Cdebug-assertions=off` Miri
   lane, while the multi-page ones are dominated by LZ4 volume and stay out of budget
   (1631s). The annotations are therefore correct for the lane as it runs today
   (`check_leaks.sh`: `cargo +nightly miri test -p rustty-vt --lib`, debug assertions ON —
   without them the rustty-vt G6 lane cannot complete at all). **NEW WS8 item:** add a
   `-C debug-assertions=off` variant of the vt Miri lane and re-measure the whole
   `cfg_attr(miri, ignore)` inventory against it; changing G6's flags is a cross-crate
   change to a shared gate and would re-classify far more than the compression family.
   **[WS19 AUDIT 2026-07-29] WS8 IS CLOSED — this item was ORPHANED. Re-owned as O2 of
   the Phase 2 Closure Audit below (owner: WS19 T6).**
   ~~**NEW WS8 item:** add a `-C debug-assertions=off` variant of the vt Miri lane and
   re-measure the whole `cfg_attr(miri, ignore)` inventory against it.~~
   **RESOLVED — WS19 T6 (2026-07-30).** The lane is **G6c**
   (`check_leaks.sh --debug-assertions-off`, `ci.sh` PROGRESS), additive to G6 (R5), with
   both FAIL arms proven by `--debug-assertions-off --self-test`. The inventory was
   re-measured per test in single-worker isolation: **82 of 97 annotations were rescued**
   and now RUN under Miri; 15 remain, each with a measured two-lane basis. The prediction
   this row made was right in direction and wrong in extent — only 2 of the 10
   page-compression tests are rescued (`…round_trips_a_cold_history_page` ok **195s**,
   reproducing the 194.01s figure post-T5; `pagelist_mutations_mark_compression_activity`
   ok **5s**), while the far larger win was the *multi-page* class, whose ">20min" reason
   text had never been measured per test at all. Rescuing them also exposed **10 real
   Stacked-Borrows violations** in test bodies that the blanket annotation had kept out of
   Miri entirely; all 10 are fixed in the same commit and re-verified clean. Evidence:
   `.port/phase-2/MIRI_EXCEPTIONS.md`, "WS19 T6 — the SECOND vt Miri lane".
---

## ~~`threadEnter` error arms should drain the pty master BEFORE `stop()`~~ — ✅ RESOLVED (2026-07-29, WS19 T17a; audit row N9) [added 2026-07-29, WS-hang review I3]

**RESOLVED — the fix is applied, and it is RECLASSIFIED as a `[R1-DIVERGENCE]`**
(entry: "`threadEnter` error arms drain the pty master before `stop()`", in the
Justified R1 Divergences section, next to the bounded `killPid` it shares its
reasoning with). `crates/rustty-termio/src/exec.rs` gains
`drain_pty_master_once(fd)` — a bounded, synchronous, discard-the-bytes drain (a
`DRAIN_BUDGET` = 50 ms deadline re-checked every iteration, and a **zero-timeout
`poll` gate before every `read`** so it can never park on the blocking master) —
and all **six** live `self.subprocess.stop()` call sites in `threadEnter` call it
first: `ProcessNotStarted`, `ProcessNoPid`, `xev::Process::init`, `pipe()`,
`xev::Timer::init`, read-thread `spawn`. (The turnaround the deferral predicted —
"a restructuring, not a local edit" — proved unnecessary: the drain does not need
the read thread's machinery, only the master fd, which every arm already has as
`pty_fds.read`.) `threadExit` is untouched: it already gets the invariant for
free.

Re-verified against upstream head before implementing
(`.port/phase-3/assessments/E-os-termio-platform.md` §5.3): those `errdefer`s are
**unchanged context lines** upstream, so the ordering defect really is shared and
this really is a deliberate divergence we intend to upstream. Area E's §5.3
recommended folding this into the E1 read-pipeline port instead; that advice is
recorded and NOT followed, because (a) N9 blocks Phase-2 closure now, and (b) the
implementation turned out to be ~40 lines with no dependency on E1's gather stage.
**E1 still owes the two NEW no-reader windows** §5.1/§5.3 identify (`setNonblock`
failure and `io-gather` spawn failure abort the reader from INSIDE the read thread,
after `threadEnter` returned success) — those are not `threadEnter` arms and are
NOT covered here; see the divergence entry's own note.

**Guard:** `exec::tests::drain_pty_master_once_consumes_pending_and_returns_on_a_quiet_master`
— a real `openpty`, asserting BOTH that the drain consumes what was already
written AND that it returns promptly on a quiet master. Mutation-verified twice
(each RED, then reverted): making the `poll` timeout blocking (5 s) fails the
promptness assertion; making the `read` a no-op fails the consumption assertion.
Deliberately NOT a reproduction of the wedge — see the `killPid` entry for why the
previous such test was deleted. Out-of-suite evidence unchanged:
`.port/tools/repro_subprocess_hang.sh` (6/6 runs, 0 hangs, no residual `?Es`
child).

The historical finding text is retained below for provenance.

---

**What is outstanding.** `Exec::threadEnter`
(`crates/rustty-termio/src/exec.rs`) has several failure arms — `pipe()`,
`xev::Timer::init`, `xev::Process::init`, and the read-thread `spawn` failure
(i.e. fd- or thread-exhaustion) — and every one of them calls
`self.subprocess.stop()` on the way out. Those arms run **before the io-reader
thread exists**, so at that moment the child is a freshly-forked `login(1)` with
output already queued on the pty slave and **nobody reading the master**. That is
exactly the condition under which a SIGHUP'd session leader wedges
uninterruptibly in the kernel exit path (macOS state `?Es`) and becomes
permanently unreapable — see the `killPid bounded wait` divergence entry for the
mechanism.

**The cure.** Reorder those arms so the pty master is DRAINED (or at minimum a
drain is in flight) before `stop()` is called — the same invariant the normal
close path gets for free from `threadExit`, which calls `stop()` while the
io-reader thread is still draining. Concretely this means the error arms need
access to a drain step that today only exists once the read thread is up, so the
fix is a restructuring of `threadEnter`'s setup/teardown ordering (and probably a
small "drain the master once, synchronously" helper shared with the read thread),
not a local edit.

**Why deferred.** (a) It is a larger restructuring of `threadEnter`'s error
handling than the hang fix needed, on arms that only fire under fd/thread
exhaustion. (b) The bounded `killPid` already converts the symptom from "rustty
hangs forever and can never be closed" into "rustty closes in ≤ `KILL_ABANDON`
(10s), leaving the wedged child to launchd" — the liveness defect is gone; what
remains is a stranded child and a 10s stall on a rare path. (c) **Upstream
Ghostty has the same ordering** in `src/termio/Exec.zig` and therefore the same
latent defect, so fixing it here is a divergence to design deliberately (and
ideally to upstream) rather than to bolt on during a hang fix.

**Where the evidence is.** `.port/tools/repro_subprocess_hang.sh` reproduces the
no-drainer wedge deterministically (pre-fix: 12/12 hangs), and
`.port/tools/capture_subprocess_hang.sh` captures the `sample`/`ps` state showing
the child in `?Es` and the parent spinning inside `killPid`.

---

## ~~`posix_waitpid` aborts on `ECHILD` where a concurrent reaper can win the race~~ — ✅ RESOLVED (2026-07-29, WS19 T17b; audit row N9) [added 2026-07-29, WS-hang review M1]

**RESOLVED, and NOT as a divergence — the premise below went STALE.** The entry
(and the WS19 plan) both said to record the fix as an `[R1-DIVERGENCE]` alongside
the bounded `killPid`, on the grounds that the verbatim
`unreachable!("waitpid: unexpected errno")` was FAITHFUL to Zig std's
`.CHILD => unreachable`. That was true at the pin and is **no longer true at
upstream head**: by dropping the std wrapper for the raw syscall (`a77c706a1` /
`e8525c0fd`), upstream's `killPid` now breaks on `rc == -1`, so an `ECHILD` ends
its loop **gracefully** — exactly the behavior this item proposed. Implementing it
therefore **REMOVES a divergence instead of adding one**, and no divergence entry
is warranted. (Established by `.port/phase-3/assessments/E-os-termio-platform.md`
§5.2, which rates it the best cost/benefit item in area E, cost S, risk LOW.)

**Applied.** `command.rs` gains `posix_waitpid_checked(pid, flags) ->
Option<WaitPidResult>` — `EINTR` still retries, every other errno returns `None`
(upstream's `rc == -1` makes no finer distinction either). `killPid` is its ONLY
caller and treats `None` as "the child is gone" → `break`, which is what the loop
was waiting for. The verbatim `posix_waitpid` is left untouched on the
`Command::wait` path, so the Zig-std shim contract this port reproduces elsewhere
is preserved.

**Guard:** `exec::tests::posix_waitpid_checked_reports_echild_instead_of_aborting`
— waits on pid 1 (never our child ⇒ deterministic `ECHILD`, no fork, nothing left
behind). Mutation-verified: restoring the `unreachable!()` arm makes it RED with
`internal error: entered unreachable code: waitpid: unexpected errno`, which is
also the direct proof that the race really was a reachable panic on the teardown
path.

The historical finding text is retained below for provenance.

---

**What is outstanding.** `crates/rustty-termio/src/command.rs`'s
`posix_waitpid` treats every non-`EINTR` errno as `unreachable!("waitpid:
unexpected errno")`, which in Rust is an unconditional panic. `ECHILD` is
reachable in rustty because there is a **second reaper**:
`crates/rustty-xev/src/process.rs` calls `waitpid(pid, &mut status, 0)` in the
`.proc` completion trampoline. If that completion fires while `killPid` is in its
SIGHUP/`waitpid(WNOHANG)` loop, the loop's next `waitpid` returns `ECHILD` and
rustty **aborts** instead of simply ending the loop (the child is, after all,
gone — which is what the loop was waiting for).

**Correcting the review's premise.** The review suggested the oracle's `killPid`
"breaks on any non-`EINTR` error". It does not: ghostty's `killPid` calls Zig
std's `posix.waitpid`, which itself has `.CHILD => unreachable, // The process
specified does not exist. It would be a race condition to handle this error.`
**So the port is FAITHFUL here** — the crash exposure is inherited, not
introduced. The difference is only in how the two languages cash out
`unreachable`: Zig's is a no-op/UB in `ReleaseFast`, Rust's always panics.

**Why deferred rather than fixed now.** `posix_waitpid` is a shared Zig-std shim
(`Command::wait` uses it too) whose return type `WaitPidResult` has no error
channel, so making the `ECHILD` race end the loop gracefully means either
changing that shim's signature and every caller, or special-casing `ECHILD`
inside `killPid` — both of which are divergences from the Zig-std shim contract
that this port reproduces verbatim elsewhere. That is a deliberate design
decision to make on its own, not a drive-by in a hang fix.

**Suggested resolution when picked up.** Give `posix_waitpid` an `ECHILD`-aware
variant (or return `Option<WaitPidResult>`) used only by `killPid`, and treat
`ECHILD` there as "reaped by someone else" → break the loop, matching the loop's
actual intent. Record it as a `[R1-DIVERGENCE]` alongside the bounded wait, since
it deviates from the Zig-std shim.

---

# Justified R1 Divergences (NOT deferrals — deliberate, permanent)

Divergences from the oracle that are intentional and will NOT be "completed"
later. Each records what diverges, why fidelity was overruled, and how the
divergence is bounded. Anything here must be a conscious decision with a
rationale — never an accident.

**[P3-W7 T8, 2026-07-30] Phase 3 area E's deliberate NON-PORTS live in
[`.port/phase-3/NON_PORTS.md`](../phase-3/NON_PORTS.md), each with the same
4-criterion basis this section requires.** They are recorded there rather than
inline because they are decisions about the *upstream resync range*, not about
the pinned baseline this file otherwise covers, and because keeping them
together is what stops a future mechanical sweep of the range diff from
"discovering" them and porting them by reflex. Nothing in that file is open
work; it needs no owner. Covered: `quirks_memset` (`E34`, with the
perf-baseline measurement hazard), `os/hostname.zig`'s `isValid` deletion
(`E17` — **keep rustty's**, Rust std has no RFC 1123 validator),
`os/windows.zig`'s rewrite (`E25` — already at the destination shape),
`os/mach.zig`'s `next_mmap_addr_hint` (`E24`), `os/flatpak.zig`'s
`@cImport`→`gio_c` (`E26`), `main_ghostty.zig`'s iOS `allow_stack_tracing`
(`E36`), `main.zig`'s `std.testing.log_level` (`E35`), and `tripwire.zig`
(`E38` — doc comment only; the `cfg!(debug_assertions)` equivalence needs no
rebase). Plus two upstream BUGS that must not be reproduced: the `os/main.zig`
doc typo (`E28`) and `pty.zig`'s `@tagName(posix.errno(-1))` (`E32`).

## `include/rustty/vt.h` — three doc-only lines rustty keeps that upstream does not have [P3-W6 T1, 2026-07-30]

**What diverges.** `include/rustty/vt.h`'s Doxygen preamble carries three things
the oracle's `include/ghostty/vt.h` does not:

1. the `@ref color_scheme "Color Scheme Report Encoding"` module bullet (`:39`);
2. the `@example c-vt-color-scheme/src/main.c` block (`:125-128`);
3. the identity URL, `[Rustty](https://rustty.sh)` where the oracle has
   `[Ghostty](https://ghostty.org)` (`:18`).

**Why fidelity is overruled.** (1) and (2) are *strictly better Doxygen* against
the same C surface: rustty ships `c-vt-color-scheme` as a real example under the
`build.sh` convention, and upstream simply never added the `@ref`/`@example`
lines when `f00e90694` landed `color_scheme.h`. Deleting them to match would
delete accurate documentation of code that exists on both sides. (3) is the R2
rebrand, which is program-wide policy, not a decision taken here.

**How it is bounded.** The `#include` ORDER is now identical to upstream —
`color_scheme.h` before `device.h`, matching `f00e90694` (rustty previously had
the two reversed; that WAS an accident and is fixed in this task). So the only
remaining delta in this file is the three doc lines above. The identity URL is
now DATA, not a literal: `PORT_URL` / `ORACLE_URL` in
[`.port/tools/parity_names.env`](../tools/parity_names.env), so a future
mechanical header-diff gate sources the pair instead of hard-coding either.

**Why record it at all.** A mechanical `vt.h` header-diff is a plausible future
gate. Without this entry it would report three lines every run, and the standard
response to a permanently-noisy gate is to weaken it (R5 violation). Recording
them here means such a gate can subtract exactly these three and stay strict
about everything else.

## ~~libxev `iocp` (Windows) + `wasi_poll` (WASI/wasm) backends — OUT OF SCOPE BY PLATFORM~~ — **RETRACTED. THE BACKENDS ARE PORTED.**

**[RETRACTED AND REPLACED — WS20, 2026-07-30.]** The entry that stood here was
**wrong** and is struck in full. It claimed `backend/iocp.zig` (2,376 lines) and
`backend/wasi_poll.zig` (1,648 lines) were "out of scope by platform,
permanently", on a 4-criterion basis whose real content was *"no artifact rustty
builds today links these bodies"*.

**Why it was wrong.** The binding rule for this port is: **everything the
reference supports gets PORTED.** An inability to BUILD or RUN a target on the
porting host is **verification debt**, never a scope exclusion. "No shipped
artifact reaches it" is an argument about *reachability*, and reachability is not
scope — by that standard most of `rustty-vt`'s conformance surface would also be
out of scope. Criterion 1 in particular ("there is no owner to name because there
is no plan for the target") inverted the rule: the absence of a plan is a reason
to WRITE one, not a reason to declare the reference's own source out of scope.
The word "deferred" in the code was replaced by a WORSE word, and a gate was
built to defend the wrong conclusion.

**What is true now.** Both backends are ported, in full, as first-class
self-contained backends alongside `kqueue`/`epoll`/`io_uring`:

| reference | port | notes |
| --- | --- | --- |
| `backend/wasi_poll.zig` | `crates/rustty-xev/src/wasi_poll.rs` | Loop (submissions/asyncs/Batch/timer heap/`poll_oneoff` tick), Completion (`subscription`/`perform`/state), every Operation, the 1024-slot `Batch` with its swap-on-put invariant, all error sets, `COMPLETION_SIZE = 80` (wasm32, measured) |
| `std.os.wasi` + `loop.zig` ABI | `crates/rustty-xev/src/wasi.rs` | the `wasi_snapshot_preview1` imports + subscription/event ABI structs + the `Options`/`RunMode`/`CallbackAction`/`CompletionState` twins (the `linux.rs` pattern) |
| `backend/iocp.zig` | `crates/rustty-xev/src/iocp.rs` | Loop (IOCP handle, submissions/cancellations/completions/asyncs, timer heap, `submit`/`tick`/`start_completion`/`stop_completion`/`associate_fd`/`async_notify`), Completion with the `OVERLAPPED` field recovered by `offset_of!` container_of (faithful `@fieldParentPtr`), every Operation incl. `job_object`, `COMPLETION_SIZE = 248` (x86_64, measured) |
| libxev's own `src/windows.zig` | `crates/rustty-xev/src/windows.rs` | the `std.os.windows` forwards + the whole `exp` namespace (job objects, `CreateFile`/`ReadFile`/`WriteFile`/`DeleteFile`), the `loop.zig` twins, and the `std.net.Address` subset on the WINDOWS `sockaddr` layout. Hand-rolled `extern "system"` — no crate dependency added (R4) |

`lib.rs` re-exports each under the SAME public names the macOS block uses; both
are single-candidate targets (`backend.zig`'s `.windows => &.{.iocp}` /
`.wasi => &.{.wasi_poll}`), so the Dynamic layer comptime-elides exactly as it
does for macOS/kqueue (§0). Test parity is now GATED, not asserted:
`check_test_counts.py` gains `xev_wasi_poll` (zig=8, rust=9) and `xev_iocp`
(zig=14, rust=14).

**What actually remains** (REAL PORT WORK with an owner — read the rows below, and
note that none of it is recorded as out of scope):

- ~~**[OPEN — owner `Phase 3`] the libxev WATCHER layer instantiated against~~
  ~~`iocp`/`wasi_poll`.**~~ **STRUCK — LANDED (WS20).** `iocp_watcher.rs` and
  `wasi_watcher.rs` are the `watcher/{async,timer,process,stream}.zig`
  monomorphizations for those two backends, and `lib.rs`'s `watcher_seam`
  placeholder module (8 `unimplemented!()` sites) is DELETED — `lib.rs` now
  re-exports the real types. What landed per backend:
  - **iocp**: `AsyncIOCP` (async.zig:530 — guard/wakeup/waiter, `wait`+`notify`),
    `TimerLoop` (`run`/`reset`/`cancel`), `ProcessIocp` (process.zig:228 — the
    job-object watcher: `DuplicateHandle` + `CreateJobObject` +
    `AssignProcessToJobObject`, and the `.associated`/`.message` result decode
    with the `STILL_ACTIVE` re-arm), `Shared` (`WriteQueue`/`WriteRequest`/
    `PollError = error{}`/`PollEvent`) and `GenericStream`
    (`read`/`write`/`writeInit`/`queueWrite` incl. the partial-write remainder
    engine/`close`).
  - **wasi_poll**: `AsyncLoopState` (async.zig:446 — the non-`threaded` arm; the
    `threaded` arm is the reference's own `return struct {}` and is mapped onto
    `#[cfg(target_feature = "atomics")]`), `TimerLoop`, `Shared`, `GenericStream`.
  - Two REFERENCE-DRIVEN shape facts, not omissions: `Stream` has NO `poll`
    method on either backend (`Pollable` is `return struct {}` for
    `.iocp`/`.wasi_poll`, stream.zig:221) and WASI has NO `Process` watcher
    (`Process(xev)`'s `.wasi_poll` arm is `struct {}`, process.zig:23 "//
    Unsupported"). Both are quoted verbatim at their sites.
  - Enabling seams: `iocp::Completion::new_op` + `wasi_poll::Completion::new_op`
    (the `pub(crate)` full-reset helper the kqueue port already had), the
    `windows.rs` `kernel32` additions `GetCurrentProcess`/`DuplicateHandle`/
    `GetExitCodeProcess`, and `mod common` un-gated from macOS-only.
  - Verified: `cargo check -p rustty-xev --target x86_64-pc-windows-msvc
    --all-targets` and `--target wasm32-wasip1 --all-targets` both GREEN, and
    `cargo clippy` `-D warnings` GREEN on both targets. The reference `test {}`
    blocks are ported as documented skip-stubs (14 iocp + 12 wasi) — they need a
    live Win32/WASI host, which is the V8 VERIFICATION DEBT row, not a scope
    exclusion.
- ~~**[OPEN — owner `Phase 3`] `ThreadPool`'s Windows futex arm.**~~
  **STRUCK — LANDED (WS20).** `thread_pool.rs` now carries BOTH remaining
  `std/Thread/Futex.zig` `*Impl` arms: `WindowsImpl` (ntdll
  `RtlWaitOnAddress`/`RtlWakeAddressSingle`/`RtlWakeAddressAll` — the reference
  uses NtDll rather than API-MS-Win-Core-Synch, comment preserved; hand-rolled
  `extern "system"`, no new dep) and `SingleThreadedImpl` (WASI + freestanding
  wasm, which are `builtin.single_threaded` — the FIRST arm of the reference's
  dispatch chain). `mod thread_pool` is un-gated to every target, matching
  `main.zig`'s unconditional `pub const ThreadPool`, and `pub use
  thread_pool::ThreadPool` is added for the non-macOS/Linux targets. Verified by
  both cross-compiles above.

**Verification status — the honest version.** This host builds macOS and Linux
only. What was actually run:

- `cargo check -p rustty-xev --target wasm32-wasip1 --all-targets` — GREEN.
- `cargo test -p rustty-xev --target wasm32-wasip1 --no-run` — GREEN (both test
  binaries link).
- `cargo check -p rustty-xev --target x86_64-pc-windows-msvc --all-targets` — GREEN.
- `cargo check -p rustty-xev --target wasm32-unknown-unknown` — GREEN.

Nothing in either backend has been EXECUTED. That is recorded in
`.port/phase-2/VERIFICATION_DEBT.md` with runnable recipes (a wasm runtime for
wasi_poll; a Windows host for iocp), and it is **verification debt, not a scope
exclusion**.

**What happened to the gate.** `.port/tools/gates/xev_scope_boundary_gate.sh` was
built to defend the conclusion this entry has just retracted, so its conclusion is
void. Its *factual* assertion is not: it is retained and REPURPOSED as
`G-XEVREACH`, a dependency-graph invariant (`rustty-xev` stays out of every wasm
artifact's tree; the one sub-app-tier xev edge stays macOS-gated) that no longer
draws any scope conclusion from that fact. See the header of the script.

## rustty-termio Windows paths (`Exec.Subprocess.start` spawn half, `ReadThread.threadMainWindows`) — **PORTED, was mis-scoped**

**[RETRACTION + PORT — WS20, 2026-07-30.]** `crates/rustty-termio/src/exec.rs`
carried two `unimplemented!("... OUT OF SCOPE BY PLATFORM ...")` bodies for code
the oracle has in full (`src/termio/Exec.zig` has 45 `windows` references with
real bodies). Same error as the xev entry above, same retraction. Both are now
ported:

- `ReadThread::threadMainWindows` — the real `ReadFile`/`PeekNamedPipe` loop
  (quit handle closed on exit; `processOutput` + `yieldToDemand` per batch;
  `OPERATION_ABORTED` = quit signal).
- `Subprocess::start`'s Windows arm — pty open, `Command` with `pseudo_console` +
  null std handles + null `os_pre_exec`, returning `{ read: pty.out_pipe, write:
  pty.in_pipe }`. `PtyFds` now carries the reference's per-target `Pty.Fd` type.
- Support: `rustty-os::windows` gains the three `kernel32` forwards termio names
  but that hand-rolled forwarding set was missing (`ReadFile`, `CancelIoEx`,
  `TerminateProcess` — `CancelIoEx`/`TerminateProcess` were already CALLED by
  `Exec.threadExit`/`killCommand`, so the Windows build could not have compiled
  before this), and `WindowsPty`'s pipe/console fields become `pub`, matching
  Zig's default-public struct fields.

**Verification status.** `rustty-termio` cannot even be cross-CHECKED for Windows
on this host: its transitive `highway-sys` C++ build script cannot cross-compile
to MSVC from macOS —

```
cargo check -p rustty-termio --target x86_64-pc-windows-msvc
  ...
  cargo:warning=.../clang/21/include/mm_malloc.h:13:10: fatal error: 'stdlib.h' file not found
  error occurred in cc-rs: command did not execute successfully (status code exit status: 1)
```

so this code is UNCOMPILED, not merely unrun. Row + recipe in
`.port/phase-2/VERIFICATION_DEBT.md`. **Verification debt, not a scope
exclusion.**

## wayland `Globals::bind_global` — duplicate re-advertisement of a SINGLETON global leaves a stale `name→tag` entry

> **⚠ BASIS EXPIRED — REOPENED by the deferral re-audit, 2026-07-31.**
> The 4-criterion basis's criterion 2 is FACTUALLY FALSE about the oracle.
> rustty keys `names: HashMap<u32, Tag>` name→tag
> (`crates/rustty-apprt/src/gtk/winproto/wayland/globals.rs:149`). The oracle's
> map is **Tag-keyed with the name in the value** (`Globals.zig:165-175`), and on
> re-advertisement it calls `onGlobalRemoved(tag)` + `old.proxy.destroy()`.
> rustty does NEITHER — so besides the stale `name→tag` entry it also LEAKS the
> superseded proxy. Fix: invert the map to `Tag -> {name, proxy}`.
> **STILL OPEN, size S.**
> See `.port/phase-3/DEFERRAL_REAUDIT.md` §C/§D.


**[RECLASSIFIED — WS19 T9, 2026-07-29, audit row O8.]** Previously filed as an open
LOW deferral with no owner (`D:4905`). It is a permanent Justified R1 Divergence.

**What diverges.** If a compositor re-advertises a singleton interface under a NEW
`name`, `Globals::bind_global` overwrites the typed proxy but leaves the old
`name→tag` map entry in place, so a later `global_remove(old_name)` would unbind the
live proxy (`winproto/wayland/globals.rs` `bind_global`/`remove_global`).

**4-criterion basis:**
1. **Bounded input that no implementation produces.** It requires a compositor to
   re-advertise a *singleton* interface under a second `name` while the first is still
   advertised. Nothing does this; the singletons involved
   (`wl_compositor`, `xdg_activation_v1`, the KDE managers, `ext_background_effect`) are
   advertised exactly once per registry.
2. **The oracle has the same shape.** `Globals.zig`'s bind path likewise overwrites the
   typed proxy and keeps no bidirectional `tag↔name` bookkeeping, so "fixing" it here
   would be a divergence FROM the reference, which is the opposite of R1.
3. **The failure mode is safe.** A `global_remove(old_name)` would clear a live proxy,
   degrading to the graceful-`None` arm the code already handles for absent globals
   (blur/CSD/slide/urgency each early-return when their global is `None`). No UB, no
   panic, no hang.
4. **Cost exceeds benefit.** Tracking `tag→name` bidirectionally to defend against an
   input no implementation produces adds state the reference does not have, on the
   registry path, for no observable gain.

## rustty-termio ReadThread — PRESERVED REFERENCE QUIRK: trailing pty output dropped on Linux POLLHUP (L1 LT9)

> **DEFERRAL RE-AUDIT (2026-07-31): CONFIRMED NON-PORT, re-verified against the ORACLE.** Upstream still checks HUP after `poll` with no final drain, now inside the two-stage pipeline's gather stage (`ghostty/src/termio/Exec.zig:1720-1722`); rustty is byte-faithful at `crates/rustty-termio/src/exec.rs:4626-4628`. The line numbers quoted below are stale; the behaviour is unchanged.


**[RECLASSIFIED — WS19 T2, 2026-07-29, audit §E.2.]** This entry was filed among the
DEFERRALS even though its own first line says "preserved verbatim, NOT fixed … reference
behavior, not a port bug". It is a permanent Justified R1 Divergence and is moved here so
no closure sweep reads it as open work. **4-criterion basis:**
1. **It is FIDELITY, not a gap.** Rustty reproduces `threadMainPosix`'s HUP-before-drain
   ordering byte-for-byte; "fixing" it would be a *divergence from* the oracle, which is
   the opposite of R1.
2. **Bounded.** It costs at most the trailing output of a process that exits in the same
   `revents` word that raised `POLLHUP`, on Linux only (macOS's kqueue does not coalesce
   this way). No state corruption, no hang.
3. **Gate/KAT-pinned.** The KAT spawns a real `/bin/sh` in a real pty and asserts the echo
   round-trips into the terminal `Screen`, with the exit RACE (not the quirk) removed, so
   the behavior cannot drift silently.
4. **The only correct trigger is upstream.** Un-diverging means upstream changes
   `threadMainPosix`; Phase 3's resync is where that would arrive. Keeping a rustty-local
   fix would put the port permanently ahead of its reference on a path with a live KAT.


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

### xev ThreadPool `Event` shutdown wake — DELIBERATE DEVIATION (controller-approved) [2026-07-19, Task 1]

**[RECLASSIFIED — WS19 T2, 2026-07-29, audit §E.2.]** Filed among the deferrals despite
being labelled a controller-approved DELIBERATE DEVIATION in its own heading. Moved here.
**4-criterion basis** (the detail below is the original rationale, unchanged):
1. **Conscious and approved** — the deviation was decided, reviewed and recorded, not
   discovered; it is not an unported remainder.
2. **Bounded to `wake`'s shutdown transition** — both changes are confined to the terminal
   SHUTDOWN edge; the steady-state notify path is byte-faithful to `ThreadPool.zig:508-518`.
3. **The failure mode it removes is a lost-wakeup HANG**, i.e. liveness beats byte-fidelity
   here — the same trade the bounded `killPid` wait already made and recorded.
4. **Regression-gated** — RED/GREEN evidence plus a regression gate accompany it (see the
   argument recorded in the entry), so it cannot silently widen.

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


---

## macOS `xev.File`/`TCP`/`UDP` watcher surface + the `kqueue: file IO on thread pool` skip-stub [RECLASSIFIED — WS19 T11, 2026-07-29, audit row O10]

> **⚠ BASIS EXPIRED — REOPENED by the deferral re-audit, 2026-07-31.**
> The permanence basis is a REACHABILITY argument ("zero consumers") — the exact
> argument this same ledger already RETRACTED for the `iocp`/`wasi_poll` backends
> (see the retracted row above). Under the standing scope rule ("everything is in
> scope if ghostty supports it") that is not a valid basis. The oracle HAS the
> kqueue arms: libxev `watcher/file.zig:134,305`, `tcp.zig`, `udp.zig`, plus the
> real test `backend/kqueue.zig:2486`. rustty ports them on Linux only
> (`crates/rustty-xev/tests/surface_parity.rs:59-62,169-172`;
> `crates/rustty-xev/src/lib.rs:379,426-430`). **STILL OPEN, size M.**
>
> NOTE: the in-tree skip-stub marker was renamed `THREAD_POOL_BODY_PORTED` ->
> `MACOS_FILE_WATCHER_PORTED` (`crates/rustty-xev/src/kqueue.rs`) because the Zap
> pool body HAS shipped; this row is now the marker's ledger anchor.
> See `.port/phase-3/DEFERRAL_REAUDIT.md` §C/§D.


**Was** filed among the xev DEFERRALS, which made a closure sweep read it as open work.
It is neither open nor a deferral of choice: it is a deliberate, gate-pinned
cross-target surface difference. Moved here.

**What diverges.** On macOS, `xev.File` / `xev.TCP` / `xev.UDP` are not instantiated
(the kqueue backend exports no `File`/`TCP`/`UDP` handle types), and the reference's
`kqueue: file IO on thread pool` test is a **SkipZigTest-style skip-stub `#[test]`**
(`kqueue_file_io_on_thread_pool`), keeping the mapped count at 12. On Linux they ARE
fully ported per-backend (`epoll.rs` + `io_uring.rs`) and wrapped as the Dynamic
`File`/`TCP`/`UDP` enums.

**4-criterion basis.**
1. **Zero consumers.** ghostty makes no use of `xev.File`/`TCP`/`UDP` on macOS
   (`watcher/file` is annotated "zero ghostty uses" in the reference-surface notes
   above), and rustty's only thread-pool consumer is the Linux epoll `File` watcher. No
   caller — in the reference or the port — can observe the difference.
2. **Gate-pinned, not hidden.** `crates/rustty-xev/tests/surface_parity.rs` asserts each
   target's ACTUAL exported surface, so neither side can drift silently: adding or
   removing an export fails that test.
3. **The capability EXISTS in the tree.** The Linux side is ported, and the work-stealing
   `ThreadPool` body is 1630 lines with zero `unimplemented!()` (audit row W5). The macOS
   gap is an EXPORT decision on an unexercised surface, not missing logic — so there is
   no unported reference behavior hiding behind it.
4. **The skip-stub is honest.** `kqueue_file_io_on_thread_pool` is a real `#[test]` that
   declares what it skips and why, keeping the R1 test count intact rather than quietly
   dropping a reference test. It defers genuinely-unused functionality, never a live path.

**If a consumer ever appears** (a macOS caller that needs pool-routed file IO), the arms
to fill are named in the xev surface notes above; the trigger is that consumer, and the
`surface_parity.rs` expectations are what must change first.

---

## libxev `c_api.zig` — libxev's own C ABI [RECLASSIFIED — WS19 T11, 2026-07-29, audit row O11]

> **⚠ BASIS EXPIRED — REOPENED by the deferral re-audit, 2026-07-31.**
> Same defect as the row above: "outside the port's consumption surface" is a
> REACHABILITY argument, already retracted once in this ledger. Criterion 3
> ("ungateable") is answered by its own criterion 4 — cbindgen + a smoke KAT,
> exactly the pattern `rustty-vt-c` already uses. Zero `c_api` references exist
> under `crates/rustty-xev/src/`; the oracle file is ~360 lines.
> **STILL OPEN, size M.**
> See `.port/phase-3/DEFERRAL_REAUDIT.md` §C/§D.


**What diverges.** libxev's `c_api.zig` (1 test) — the C ABI libxev exposes for C
consumers — has no rustty counterpart.

**4-criterion basis.**
1. **Outside the port's consumption surface.** Every rustty consumer of `rustty-xev`
   uses the RUST API. The C ABI is a libxev-to-C boundary, not a ghostty-to-libxev one.
2. **ghostty does not consume it either.** ghostty links libxev as a Zig package and
   calls its Zig API, so there is no reference behavior any caller observes through
   `c_api.zig`. R1 is a fidelity obligation about observable behavior; nothing observes
   this.
3. **Porting it would create an UNGATEABLE surface.** A C ABI with no consumer cannot be
   bound by any existing gate — not the differential harness (no driver), not
   `surface_parity.rs` (which asserts the RUST surface), not the header gates (which
   cover `rustty-vt-c`/`rustty-apprt-c`). It would be new unexercised `unsafe` whose
   correctness no test could assert, which is strictly worse than its absence.
4. **The trigger is named.** If a C consumer of `rustty-xev` ever appears, the port is
   `c_api.zig` plus a cbindgen header and a smoke KAT modelled on
   `rustty-vt-c`'s — at which point it becomes gateable, which is exactly the condition
   that makes it worth doing.

---

## xev `Completion.task_wakeup` — the pool worker wakes the loop through a per-completion `Copy` handle, not `c.task_loop.wakeup()` [WS19 T10, 2026-07-29, audit row O9]

**Oracle:** both `backend/kqueue.zig` and `backend/epoll.zig` end `thread_perform` with
`c.task_loop.wakeup()` — the pool worker reaches back into the `Loop` to notify it.

**Divergence:** `Completion` gains ONE port-added field, `task_wakeup` (`Async` on
kqueue — a `Copy` `port: u32`; `Eventfd` on epoll — a `Copy` `fd: i32`), written on the
loop thread in `Loop::start`/`thread_schedule` immediately after `(*c).task_loop = self`,
and `thread_perform` notifies THAT copy. Nothing else changes: same handle, same
`mach_msg`/`write(1u64)`, same ordering relative to the queue push. Note the companion
field `task_completions` is NOT a divergence — it is the reference's own field, restored
after a Task-2 elision, and it makes `Completion` a closer match to Zig's 256 bytes
(kqueue 248→264).

**Why fidelity is overruled (soundness > R1):**

1. **The oracle's shape is UB in Rust, proven not argued.** The loop thread is inside
   `tick(&mut self)` / `run(&mut self)` — a *function-entry, PROTECTED* `&mut Loop` — for
   the entire time a pool worker runs. `.port/phase-2/mirihar-xev-mpsc` (`bash
   run-all.sh`): `wakeup_naive`, the literal `(*(*c).task_loop).wakeup()` autoref, is UB
   under **both** Stacked and Tree Borrows. Zig has no aliasing model, so the reference
   incurs no such cost; this is a language-level obligation, not a behavior choice.
2. **The obvious cheaper fix is insufficient — also proven.** `wakeup_copied` (read the
   `Copy` handle out of the `Loop` by value, forming no reference — the shape `epoll.rs`
   had already shipped) is **still SB-UB**: a by-value read is a foreign *access* on the
   `Loop` allocation, and a protected tag may not be invalidated at all. Only
   `wakeup_completion_cached` — the worker touching ONLY the caller-owned `Completion`
   allocation — is CLEAN under both models. There is no faithful shape that is sound.
3. **The divergence is invisible to every observer.** The handle notified is bit-identical
   to `Loop`'s own, so the kernel-visible effect (`mach_msg` to the same port / `write` to
   the same eventfd) is byte-identical; no caller, test or syscall trace can tell the two
   apart. The only observable is `size_of::<Completion>()`, which is *already* a
   documented Rust-vs-Zig divergence pinned by its own tripwire.
4. **It is bounded and gate-pinned.** Two fields, two write sites (both on the loop
   thread, both adjacent to the existing `task_loop` write), one read site per backend.
   The `COMPLETION_SIZE` `const` assert trips on any drift, `tests/surface_parity.rs`
   pins `Completion`'s public surface (`Sized`/`Default`/`Send`), and the mirihar matrix
   is the standing regression proof. As a bonus the change removes ALL worker derefs of
   `task_loop`, which discharges the separate `task_loop`-lineage hazard
   (`taskloop_lineage_naive`: SB-UB) rather than merely relocating it.

**Guard:** `.port/phase-2/mirihar-xev-mpsc` (`wakeup_naive` / `wakeup_copied` /
`wakeup_completion_cached` / `taskloop_lineage_*`, SB + TB) — out-of-suite by necessity:
the pool path is dead in-tree (`Loop::init` hardcodes `Options::default()`, so
`thread_pool` is always `None`), so no in-suite test can reach it, and Miri only checks
executed code.

---

## `rustty-bench` macOS `os_signpost` Instruments interval [RECLASSIFIED — WS19 T13, 2026-07-29, audit row O19a]

> **⚠ BASIS EXPIRED — REOPENED by the deferral re-audit, 2026-07-31.**
> This entry's permanence rested on "the log-facade tier is deferred TREE-WIDE".
> That is no longer true: `macos.os.Log` IS ported
> (`crates/rustty-os/src/macos.rs:220-262` + `cbits/log_shim.c`), so the sibling
> tier this depended on has LANDED and `signpost` is the lone hold-out. The
> macOS body is still a no-op at `crates/rustty-bench/src/benchmark.rs:115-118`
> with the reference body preserved as comments `:93-114`. Oracle:
> `src/benchmark/Benchmark.zig:45-66` + `pkg/macos/os/signpost.zig`.
> **STILL OPEN, size S.**
> See `.port/phase-3/DEFERRAL_REAUDIT.md` §C/§D.


**What diverges.** `Benchmark.zig:44-65` opens an `os_signpost` interval so a benchmark
run shows up as a named region in Instruments. `crates/rustty-bench/src/benchmark.rs:81`
is a `#[cfg(target_os = "macos")]` **no-op**, with the reference body preserved verbatim
as comments.

**4-criterion basis.**
1. **Pure instrumentation.** An Instruments trace interval has zero effect on measured
   behavior, on benchmark timings as reported by the port's own harness, or on any
   output. It changes what a profiler DISPLAYS, nothing the program computes.
2. **R1 no-loss.** The verbatim Zig body is preserved at the site, so nothing is dropped;
   the reference logic is readable in place and comment-fidelity gates count it.
3. **Unobserved.** No test in the reference or the port asserts anything about the
   signpost.
4. **Filling it here would be an isolated one-off.** It needs the `macos.os.Log` /
   `macos.os.signpost` binding tier, which the tree defers TREE-WIDE (the log-facade
   seam). Binding that family for one profiler annotation — while every other consumer
   of the same tier stays deferred — buys nothing and creates a lone unexercised FFI
   surface. The trigger is the log-facade tier landing, not this call site.

---

## `rustty-synthetic` / `rustty-bench` locally-replicated `detectArgs` [JUSTIFIED — WS19 T13, 2026-07-29, audit row O18/O19]

> **DEFERRAL RE-AUDIT (2026-07-31): CONFIRMED NON-PORT.** The replicas are real (`rustty-synthetic/src/cli.rs:110`, `rustty-bench/src/cli.rs:199`) and G7 acyclicity is a binding constraint; the logic is duplicated, not dropped.


**Context, and a correction.** The ledger claimed these two crates had unfilled
BINARY-ENTRY seams. **They do not** — see the strike on those rows: `cli.rs:97` `main()`,
`:245`/`:274`/`:297` `main_action_impl_{ascii,osc,utf8}` are all LIVE, both crates
register real `[[bin]]` targets (`src/main_gen.rs`, `src/main_bench.rs`) whose `fn main`
calls `cli::main()`, and `grep -rn 'unimplemented!\|todo!'` over both crates returns
**zero hits**. What remains is one real, permanent structural divergence.

**What diverges.** Each crate replicates the `+action` detection logic locally
(`detect_args`, mirroring `cli.action.detectArgs(Action, alloc)`) instead of calling the
shared generic helper.

**4-criterion basis.**
1. **The shared helper is ABOVE these crates in the acyclic graph (R4).**
   `cli.action.detectArgs` lives in `rustty-cli`, which is a heavy crate depending on
   config/vaxis/zf; a `rustty-synthetic → rustty-cli` or `rustty-bench → rustty-cli` edge
   would invert the graph. `check_acyclic.sh` (G7) is the binding constraint, and it is
   invariant (R5).
2. **The replication is FAITHFUL and small.** Each local `detect_args` reproduces the
   reference's detect logic — find the single `+name` argument, `stringToEnum` — over
   `std::env::args_os`. The synthetic `Action` declares no `detectSpecialCase`, so the
   special-case arm of `detectIter` is correctly elided rather than stubbed; that
   elision is documented at the site.
3. **Behavior is identical where it is observable.** Both binaries dispatch the same
   actions from the same argv shapes as the reference; no caller can distinguish a local
   detector from the shared one.
4. **The alternative is worse than the divergence.** Hoisting `detectArgs` into a new
   leaf crate purely to de-duplicate ~30 lines adds a crate to the graph and a new
   published surface for no behavioral gain; hosting the entries in the `rustty` bin
   (the plan's suggested alternative) is moot now that both crates have working bins of
   their own. If `detectArgs` ever needs to change, the trigger is that change, and the
   two replicas are named here.

---

## terminfo SOURCE + termcap files never emitted (Debug builds only) [RECLASSIFIED — WS19 T14, 2026-07-29, audit row O20]

> **DEFERRAL RE-AUDIT (2026-07-31): OBSOLETE — both files ARE emitted.** `xtask/src/res_terminfo.rs:123` calls `emit_source_and_termcap` (defined `:165`), writing `{APP}.terminfo` and `{APP}.termcap` via `/usr/bin/infotocap`, asserted at `:260`. This divergence entry no longer describes the code and should be deleted.


**Was** `INTEGRATION_PARITY.md §2.8`, with fix-list item 8 marked "(Optional)".
"Optional" is not a disposition, and the row had no owner. Moved here as permanent.

**What diverges.** `xtask` does not emit the terminfo **source** file or the **termcap**
file that `GhosttyResources.zig` emits.

**4-criterion basis.**
1. **In the SHIPPED configuration, rustty matches the oracle EXACTLY.** The reference's
   `emit_terminfo_source` / `emit_termcap` options default **FALSE for Release**
   (`Config.zig:410-429`), and the shipping app IS a Release build. There is no
   divergence at all in what users get — only in a Debug-build convenience.
2. **Confined to informational files; the functional artifact IS produced.** The
   COMPILED terminfo database — which is what TERM resolution actually reads — is
   generated (`xtask/src/res_terminfo.rs:50-90`). The source and termcap files are
   human-readable/legacy conveniences, not inputs to any runtime path.
3. **Unobserved.** No test in the reference or the port asserts these two files exist.
4. **Named trigger, bounded cost.** If a Debug-parity need appears, the work is
   mirroring the two `GhosttyResources.zig` hunks into `res_terminfo.rs`; the trigger is
   that need, and the hunks are named. Deleting the "(Optional)" fix-list row removes the
   pretence that this is scheduled work.

---

## `killPid` bounded wait (`crates/rustty-termio/src/exec.rs`) — WS-hang

**Oracle:** `ghostty/src/termio/Exec.zig`'s `killPid` loops FOREVER — `killpg(pgid,
SIGHUP)`, then `waitpid(pid, WNOHANG)`, sleep 10ms, repeat — until the child is
reaped. There is no deadline and no escalation.

**Divergence:** rustty's `killPid` adds exactly ONE deadline and changes nothing
else:
- `0 .. KILL_ABANDON` (10s) — byte-for-byte the reference behavior: `killpg(pgid,
  SIGHUP)` + `waitpid(pid, WNOHANG)` + 10ms sleep, repeated. Not one signal,
  argument or ordering differs.
- at `KILL_ABANDON` — stop waiting and return `KillError::Abandoned` instead of
  looping forever. The child is left alive (or wedged) and reparented to
  launchd; the caller (`stop`) proceeds with teardown.

**Why fidelity is overruled (liveness > R1):** on macOS a child can become
permanently unreapable. A `login(1)` — a session leader — that is SIGHUP'd while
it still has output queued on its pty *slave* wedges UNINTERRUPTIBLY in the
kernel exit path (state `?Es`). It is immune to `SIGKILL` and `waitpid` will
never return it, so the reference loop spins forever. Because this loop runs on
the teardown path, the consequence is that the terminal can never finish
closing.

**Production reachability (this is not merely a test artifact):** the *normal*
surface-close path is safe — `Exec::threadExit` calls `stop()` while the
io-reader thread is still draining the pty master, and upstream orders it that
way deliberately ("Quit our read thread after exiting the subprocess so that we
don't get stuck waiting for data to stop flowing"). Draining the master is
exactly what un-wedges the child. **But `threadEnter`'s error arms also call
`stop()`, and they run before the io-reader thread exists** — the `pipe()`,
`xev::Timer::init`, `xev::Process::init` and read-thread `spawn` failure arms
(fd/thread exhaustion). On those arms the child is a freshly-forked `login` with
pending output and no reader: precisely the wedging condition. That is a
genuine, if narrow, unrecoverable-hang bug. Upstream Ghostty has the same
ordering and therefore the same latent defect. (The residual cure for those arms
is tracked as an open deferral — see "`threadEnter` error arms should drain the
pty master before `stop()`".)

**[P3-W7 T7, 2026-07-30] The two-stage read pipeline WIDENS this paragraph — the
divergence's justification gets STRONGER, not weaker.** Upstream `2f0e6659d` +
`bb0ac4c72` (ported by WS10 T20, `a788346`) make the read thread return early in
places that did not exist at the pin. Re-checked against the landed code, there
are **two** such arms and they are NOT symmetric:

1. **`setNonblock(fd)` failure — a REAL new no-reader window.**
   `crates/rustty-termio/src/exec.rs` `ReadThread::threadMainPosix` does
   `if !Self::setNonblock(fd) { return; }` **before** the gather thread is
   spawned and before any byte is read. `threadEnter` has ALREADY returned
   success at that point, so this is not a `threadEnter` error arm at all — it
   is a *new class* of the same bug: a forked child with pending output on its
   pty slave and no reader on the master, with `threadExit`'s `stop()` still to
   come. That is exactly the `?Es` wedging condition, and the bounded `killPid`
   is what stands between it and an unclosable window. Narrow (it needs
   `fcntl(F_SETFL)` to fail on a valid pty master) but real, and it did not
   exist before the pipeline.
2. **`io-gather` spawn failure — NOT a new window.** Upstream treats a spawn
   failure as fatal, and `std::thread::scope` has no fallible spawn, so rustty's
   `.expect("spawn io-gather")` **aborts the process** rather than returning
   from the read thread. A dead process cannot have an unclosable window, so
   this arm contributes no reachability. Recorded because area E §5.1 predicted
   two arms and only one of them survives contact with the ported code — a
   ledger that transcribed the prediction instead of checking would be wrong
   here.

**Do NOT let a mechanical rebase drop the deadline.** Upstream's loop is *still*
`while (true)` at `6ad1fe7d8` (re-read at upstream head), so the divergence is
unchanged in kind; `KILL_ABANDON` and the `elapsed >= KILL_ABANDON` check stay
exactly where they are. Upstream now passes `null` for the status pointer in
`killPid` (it never read the status) — cosmetic, no action.

**PRECISE GUARANTEE — scope of the behavior change.** An earlier revision of
this entry claimed "every healthy teardown is observationally identical to the
oracle". That over-claimed. The guarantee that actually holds is:

> rustty is observationally identical to the oracle for any teardown that
> **completes within `KILL_ABANDON`**. For teardowns that take longer it is
> **not** identical: the oracle keeps SIGHUPing forever, rustty stops waiting.

Two distinguishable cases live past the deadline, and only the first is
pathological:

1. **Unreapable (`?Es`).** Waiting longer provably cannot succeed, so
   abandoning loses nothing. This is the case the divergence exists for.
2. **Alive, reapable, but SLOW** — a shell trapping/ignoring SIGHUP, a
   `zshexit`/history flush over a stalled network filesystem, an ssh
   multiplexer or a DB client flushing state. **ACCEPTED CONSEQUENCE:** we stop
   waiting for such a child and no longer collect its exit status; it is
   reparented to launchd. Note what we deliberately do *not* do: we do not kill
   it. Its cleanup runs to completion, so there is **no data loss**, and the
   signals it received are exactly the ones the oracle would have sent. The
   entire observable difference is "rustty finished closing while the child was
   still tidying up".

**Rejected design: the SIGKILL escalation (removed in review follow-up I1).**
An intermediate revision added `KILL_ESCALATE` (5s), past which the
process-group signal became `SIGKILL`. It was **removed**, because it is useless
where it was needed and harmful where it was not:

- **For the `?Es` case it does nothing.** An `E`-state process has already
  entered the kernel exit path and is SIGKILL-immune — that immunity is the very
  property that makes the wedge unreapable. So the escalation contributed
  *zero* liveness for the only case that motivated the divergence; all of the
  liveness comes from `KILL_ABANDON`.
- **For case 2 it caused real harm.** It converted "slow cleanup that eventually
  finishes" into "killed mid-cleanup": a truncated history file, a half-flushed
  client. That is user data loss, in a case that is not pathological and where
  the oracle would have waited. And `killPid` cannot distinguish case 2 from a
  SIGHUP-ignoring child — they look identical from the inside.
- **Its only genuine beneficiary** was a child that ignores SIGHUP but *is*
  killable, which the escalation would kill instead of abandoning at 10s. But
  the oracle never kills such a child either, so abandoning it is *closer* to
  the reference; inventing a SIGKILL that the user's shell never asked for is
  not worth the case-2 exposure.
- Its stated secondary justification ("closer to the reference's own intent —
  it also kills descendants that survived the setsid/execve race and ignore
  SIGHUP") was **vacuous**: the loop's exit condition is `waitpid(pid)` on the
  *direct child* only. Surviving descendants never gate the loop, so escalation
  could only ever affect the group during a window that a healthy teardown never
  reaches.

The surviving design is therefore the **minimal** divergence that achieves
liveness: a single wall-clock deadline bolted onto an otherwise byte-for-byte
faithful loop.

**Guard:** none in-suite, deliberately. The former
`exec::tests::kill_pid_bounded_when_child_is_unreapable` was **deleted** in
review follow-up I2: it reproduced the wedging condition inside the default
`cargo test`, asserted nothing that could fail (it reaped in milliseconds on
ordinary runs, and its `KILL_ABANDON + 20s` threshold would have passed even if
`KILL_ABANDON` were tripled), its only failure mode was *hanging* — the exact CI
failure this work removes — and it stranded an abandoned `?Es` child on the
developer's machine on every run. A deterministic replacement is not available
either: with the escalation gone the sole divergence is a wall-clock deadline,
which cannot be asserted without spending `KILL_ABANDON` seconds and stranding a
child. The deterministic evidence lives out-of-suite in
`.port/tools/repro_subprocess_hang.sh` (pre-fix: 12/12 hangs; post-fix: 0).

**Related, NOT diverged:** `getpgid`'s retry loop is also unbounded in both the
oracle and the port. It is left verbatim — it terminates as soon as the child
`setsid`s or dies, and a wedged `?Es` child still reports a valid pgid, so it is
not a hang site (confirmed by `sample`: the hang is inside `killPid`).

**Related, NOT part of this divergence:** the `ECHILD` handling in this same loop
is **NOT** a divergence — `killPid` now uses `posix_waitpid_checked` and breaks on
`None`, which is what upstream head's raw-syscall `killPid` does. See the resolved
`posix_waitpid`/`ECHILD` entry above (WS19 T17b). Do not fold it in here.

---

## `threadEnter` error arms drain the pty master before `stop()` (`crates/rustty-termio/src/exec.rs`) [WS19 T17a, 2026-07-29, audit row N9]

**Oracle:** `ghostty/src/termio/Exec.zig`'s `threadEnter` registers
`errdefer self.subprocess.stop();` right after `Subprocess.start`, and every later
fallible step (`internal_os.pipe`, `xev.Timer.init`, `xev.Process.init`, the
`std.Thread.spawn` of the read thread) unwinds through it. None of them drains the
pty master first, because on those arms the io-reader thread does not exist yet.

**Divergence:** each of the six corresponding rustty arms calls
`drain_pty_master_once(pty_fds.read)` immediately before `self.subprocess.stop()`
— a bounded, synchronous read-and-discard of whatever the child has already
written. Nothing else changes: same signals, same cleanup order, same return
values, and **no effect whatsoever on any path that succeeds**.

**Why fidelity is overruled (liveness > R1) — the same trade the bounded `killPid`
made, and a strictly weaker one:**

1. **It cures a real, if narrow, unrecoverable hang.** On these arms the child is a
   freshly-forked `login(1)` — a session leader — with output queued on the pty
   slave and **nobody reading the master**. SIGHUP'ing it in that state wedges it
   uninterruptibly in the macOS kernel exit path (state `?Es`): SIGKILL-immune,
   never reapable. That is precisely the mechanism the bounded `killPid` entry
   documents; draining the master is what un-wedges it, and it is exactly what the
   *normal* close path gets for free (`threadExit` calls `stop()` while the reader
   is still draining — upstream orders it that way deliberately).
2. **Upstream has the same defect, verified twice.** The deferral said so at the
   pin, and the Phase-3 area-E rebase check re-confirmed it at upstream head:
   `.port/phase-3/assessments/E-os-termio-platform.md` §5.3 — those `errdefer`s are
   **unchanged context lines**; the only nearby edits are cosmetic
   (`posix.exit` → `std.process.exit`, `posix.close` → `posix.system.close`). So
   this is a bug to fix and upstream, not a fidelity gap to close.
3. **The behavior change is unobservable outside the failing arm.** The bytes
   consumed were about to be discarded anyway (the arm is tearing the subprocess
   down and returning an error), so nothing that could have read them still exists.
   Contrast the bounded `killPid`, which DOES have an observable consequence (a slow
   child is abandoned): this one has none.
4. **It is bounded, so it cannot become the hang it prevents.** A `DRAIN_BUDGET`
   (50 ms) deadline is re-checked every iteration, and every `read` is gated by a
   **zero-timeout `poll`**, so a quiet master returns immediately instead of parking
   on the blocking fd. The promptness property is the second half of the guard KAT,
   and mutation-verified (a blocking poll timeout makes it RED).

**Guard:** `exec::tests::drain_pty_master_once_consumes_pending_and_returns_on_a_quiet_master`
(real `openpty`; both properties; 2 mutation experiments) plus the out-of-suite
`.port/tools/repro_subprocess_hang.sh` (6/6 runs, 0 hangs, no residual `?Es`
child). Deliberately NOT an in-suite reproduction of the wedge — see the `killPid`
entry above for why the previous such test was deleted.

**Known residual — RE-VERIFIED against the LANDED pipeline [P3-W7 T7,
2026-07-30]; still OPEN, narrowed from two arms to one.** The prediction was
that upstream's read-pipeline commits (`2f0e6659d`, `bb0ac4c72`) add TWO new
no-reader windows that are *not* `threadEnter` arms. The pipeline has since
landed (WS10 T20, `a788346`), so the prediction is now checkable against code
rather than against a diff, and it is **half right**:

* **`setNonblock(fd)` failure — CONFIRMED, and OPEN.**
  `crates/rustty-termio/src/exec.rs` `ReadThread::threadMainPosix` does
  `if !Self::setNonblock(fd) { return; }` before the gather thread exists and
  before any byte is read, with `threadEnter` already returned success. It does
  **not** call `drain_pty_master_once`. This is a *new class* of the same
  defect this entry describes — same wedging condition, different arm — and it
  needs the same drain. **Owner: whichever workstream next touches
  `ReadThread::threadMainPosix`;** P3-W7 records it rather than fixing it
  because the fix belongs with the pipeline's owner and is a behaviour change
  on a path this workstream has no test for.
* **`io-gather` spawn failure — REFUTED.** `std::thread::scope` has no fallible
  spawn, so rustty's `.expect("spawn io-gather")` aborts the process instead of
  returning from the read thread. A dead process has no unclosable window, so
  this arm adds no reachability at all. Recorded explicitly: a ledger that
  transcribed the prediction instead of re-checking it would have carried a
  phantom arm forever.

The bounded `killPid` entry's "Production reachability" paragraph has been
updated with both findings. See area E §5.1/§5.3 for the original prediction.

---

# PHASE 2 CLOSURE AUDIT (WS19) — every open item, verified against code [2026-07-29]

**Why this section exists.** Items in this file had been filed to **WS8**, which is
`[x]` AUTHORED + EXECUTED + MERGED (`docs/superpowers/plans/2026-07-23-rustty-completion-ROADMAP.md:166`).
An item filed into a closed workstream is **orphaned** — nobody will ever pick it up.
This audit walks all of `DEFERRALS.md`, `VERIFICATION_DEBT.md`, `MIRI_EXCEPTIONS.md`
and `INTEGRATION_PARITY.md`, **verifies each claim against the code rather than
transcribing it**, and gives every genuinely-open item an owner in an OPEN workstream.
Phase 2 is a COMPLETION roadmap: "deferred to a later phase" is not an acceptable
disposition. Every row below ends as one of:

* **fix in WS-n** — owned by an open workstream with a plan;
* **permanent divergence** — reclassified into "Justified R1 Divergences" with a
  4-criterion basis;
* **verification debt** — reclassified into `VERIFICATION_DEBT.md` with a runnable recipe.

Owner of this audit and of every ORPHANED row below: **WS19**
(`docs/superpowers/plans/2026-07-29-ws19-phase2-closeout.md`).

## A. Counts

| | |
|---|---|
| Trackable items enumerated across the 4 ledgers | **96** |
| Verified RESOLVED / permanent / out-of-scope-by-platform (no action) | **58** |
| **Genuinely OPEN** | **38** |
| of those, **ORPHANED** (owner is a closed WS, or no owner at all) | **21** |
| Rows whose **claimed status was WRONG** when checked against code | **11** |
| New items this audit adds (not previously in any ledger) | **10** |

## B. ORPHANED — filed to a closed workstream, or never owned

| # | Ledger row | Verified state (evidence) | Disposition | Cost |
|---|---|---|---|---|
| O1 | `D:5080` make `style::Color` padding-free, "listed for WS8" | **OPEN, real.** `crates/rustty-vt/src/style.rs:143` is a plain Rust `enum Color { None, Palette(u8), Rgb(RGB) }` — no `#[repr]`, payload-sized, so a typed write leaves the inactive payload uninit. Site count re-measured: **160** `Color::{None,Palette,Rgb}` occurrences across **18** files (ledger said 163/17 — mildly stale). Reproduction is the committed `mapinit_styled` harness scenario (seconds). | **fix in WS19 T5** — no other open WS touches vt style | **L** | **WS19 EXECUTION STATUS:** **RESOLVED — WS19 T5 (2026-07-29).** `style::Color` is a byte-complete `#[repr(C)] { tag: u8, data: [u8; 3] }` (size/align pinned by a `const _` assert); the ~160 construction sites are unchanged (associated const + associated constructors), only the ~22 pattern sites moved to `tag()` + `palette()`/`rgb()`. The same pass also fixed a NEWLY-FOUND second instance of the class: `RefCountedSet::insert`'s two whole-`Item` typed writes de-initialized `Item`'s 1 trailing padding byte, and are now field-wise. `mapinit_styled` before `[0x17c]` uninit → intermediate `[0x187]` uninit → after: "styled mapping of 16384 bytes fully initialized". vt suite unchanged at 2159/0/2. |
| O2 | `D:5357` add a `-C debug-assertions=off` vt Miri lane, "NEW WS8 item" | **OPEN, real.** `check_leaks.sh` runs `cargo +nightly miri test -p rustty-vt --lib` with debug assertions ON; no second lane exists (`grep -rn 'debug-assertions' .port/tools/` → only `check_release_lane.sh` prose and this ledger). The 10 `cfg_attr(miri, ignore)` annotations are correct only *for the lane as it runs today*. | **fix in WS19 T6** | **M** | **WS19 EXECUTION STATUS:** **RESOLVED — WS19 T6 (2026-07-30).** The lane exists: `check_leaks.sh --debug-assertions-off` (**G6c**), registered in `ci.sh` as PROGRESS, ADDITIVE — G6 keeps debug assertions ON, unchanged (R5). The partition is carried by the annotation itself (`cfg_attr(all(miri, debug_assertions), ignore)` = ignored in G6, RUN in G6c) and G6c derives its test set by grepping those annotations, so the lanes cannot drift. Both FAIL arms are proven by `--debug-assertions-off --self-test`: flag-loss (the real crate goes `0 passed; 82 ignored` -> RED on the vacuity floor) and injected UB (RED, names it). **The whole 97-test `cfg_attr(miri, ignore)` inventory was re-measured per test, in isolation, against the new lane, and 82 of the 97 were NOT intractable at all** — the "multi-page PageList ... >20min" reason text was a class-level assumption, and most of those tests finish in 6-30s. Making them runnable also surfaced **10 real Stacked-Borrows violations** (cached raw `*PageList` read across `Terminal::active_mut()`), all test-code, all fixed here and re-verified UB-clean. 15 annotations remain, each with a measured per-test basis (11 TIMEOUT in BOTH lanes; 4 `debug_assertions`-only, which G6c makes inert rather than fast). Full evidence: `.port/phase-2/MIRI_EXCEPTIONS.md`, "WS19 T6 — the SECOND vt Miri lane". |
| O3 | `D:4981`/`:5011` WS7 Phase C divergence #11 — `Screen::cursor_scroll_region_up` unported; "belongs to the WS8 upstream-sweep backlog (BL-4)" | **OPEN, real, and DOUBLY mis-filed.** `grep -rn 'cursor_scroll_region_up\|cursorScrollRegionUp' crates/` → **zero hits**. But BL-4 is the **perf series**, which is **WS10**'s scope, not WS8's — so the row names a closed WS *and* the wrong backlog. The blocking commit `77190bd02` is beyond the pin. | **route to WS10** (BL-4). Because `77190bd02` is beyond-pin, the port is **cheaper AFTER Phase 3 advances the pin** — WS19 T2 rewrites the row to say exactly that instead of leaving it in a closed WS. | **S** (row) / **M** (port, in WS10) | **WS19 EXECUTION STATUS:** **DONE (row rewritten) — WS19 T2.** **WS10 T18 STATUS (2026-07-30): re-owned to Phase 3 with the `BEYOND-PIN` marker.** `77190bd02`'s source is the upstream resync range, so it is Phase 3's by definition; WS10 did not port it. The marker is what lets `check_phase2_closure.py` accept `Phase 3` as an owner (`row_is_beyond_pin()`). | **DEFERRAL RE-AUDIT STATUS (2026-07-31):** **WRONG — DISCHARGED.** The symbol IS ported: `crates/rustty-vt/src/screen.rs:1885` + all 6 oracle tests + a fast/slow differential, landed by `a8855af` (P3-W5 T3). The BEYOND-PIN parking is also moot — the pin advanced to `6ad1fe7d8` on 2026-07-31, making `77190bd02` an ancestor. G-P2 stayed green throughout because it checks OWNERSHIP, not completion.
| O4 | `D:4960` inspector `pagelist.rs` CELL-detail path still restores compressed pages; "D6 must port that end state" | **OPEN — and WS7 D6 MERGED (`b8c40f9`) without closing it.** `crates/rustty-inspector/src/widgets/pagelist.rs` still calls `Node::page(cell.node)` at **6** sites (`:1701,:1703,:1759,:1889,:2163,:2379`). The guard at `:2414` asserts `!loop_src.contains("Node::page(")` over the **page-tree loop source only** — exactly the weaker form the deferral predicted — so the file-wide property is still unproven. | **fix in WS19 T7** (port the oracle `pages.pin` + `InspectedCell` restructure; tighten the guard to whole-file) | **M** | **WS19 EXECUTION STATUS:** **DONE — WS19 T7.** Ported the oracle `pin` + `InspectedCell` restructure; whole-file guard in a SIBLING file; FAIL arm mutation-proven. |
| O5 | `D:4973` `PageList::Limits`/`Limits::enforce` has no rustty counterpart, so the oracle's 11th `mark_activity` site has nothing to port onto | **OPEN, correctly described.** Depends on a struct beyond the pin. | **route to Phase 3 as a re-vet row**, not a Phase-2 deferral: WS19 T2 restates it as pin-conditional with the exact assertion Phase 3 must make (`Limits::enforce` must `mark_activity` on entry) | **S** | **WS19 EXECUTION STATUS:** **DONE (row rewritten) — WS19 T2.** Phase 3 re-vet assertion **P3-RV1**. | **DEFERRAL RE-AUDIT STATUS (2026-07-31):** **WRONG — DISCHARGED.** `crates/rustty-vt/src/pagelist/mod.rs:1295` `Limits`, `:1370` `exceeded`, `:2388` `enforce_limit`, landed by `abc2b3d` (P3-W3 T3) — whose commit message already states "mark_activity on enforce", i.e. re-vet assertion **P3-RV1 was satisfied when it was written**. Phase 3 owes nothing here.
| O6 | `D:4046` GTK portal `OpenURI` — call site INHABITED, body unported; marked **OPEN**, no owner | **OPEN, real.** No `portal` module under `crates/rustty-apprt/src/gtk/`; `startup` fetches the `gio::DBusConnection` for GlobalShortcuts only. WS15 explicitly split this out and did not take it. | **fix in WS19 T8** (reuses the WS15 request-path pattern + its live gate harness) | **M** | **WS19 EXECUTION STATUS:** **DONE — WS19 T8, 2026-07-29.** Ported (`gtk/portal.rs` + `gtk/portal/open_uri.rs`), call site live in `application.rs`, and verified on the VM by `.port/tools/gates/ws19_portal_openuri_gate.sh` (stub-portal ordering with a demonstrated FAIL arm, live round-trip against the real `xdg-desktop-portal`, `dbus-monitor` wire shape; both negative controls exercised). The plan's "diff against installed ghostty 1.3.1" step was impossible — that build has no OpenURI portal path (`strings … | grep -c` → 0); see the resolved entry. |
| O7 | `D:4858`/`:4904` wayland `Window` snapshots the decoration mode at init; a live decoration-mode change is now *cranked* but never *re-read* | **OPEN, real, no owner** — recorded twice, both as a "coupled follow-up" with no WS. | **fix in WS19 T9** | **S** | **WS19 EXECUTION STATUS:** **DONE — WS19 T9, 2026-07-29.** `Window` shares the app's `Rc<RefCell<Pump>>` and re-reads `Globals` at each `decoration_mode`/`client_side_decoration_enabled`/`sync_blur` call (the reference's `globals: *Globals` dereference at `wayland.zig:222`/`:252`/`:303`); the three snapshot fields are now caches refreshed by every live read. Gated by `.port/tools/gates/ws19_wayland_deco_live_gate.sh` — nested KWin 6.7.3 on its own socket, GREEN, with the pre-fix snapshot FAIL arm demonstrated (`csd` stays `false`) and a latch check (mode back to Server → `csd` back to `false`). Residual, recorded as `VERIFICATION_DEBT.md` V6: no compositor on this host re-advertises `default_mode` post-bind — MEASURED (`WAYLAND_DEBUG=1`: KWin 6.7.3 emits exactly two `default_mode(2)`, both at bind, none after `org.kde.KWin.reconfigure`; sway sets it once at startup; mutter has no KDE decoration manager), so the wire *transport* of a post-bind change is unexercised while the re-read itself is proven. |
| O8 | `D:4905` wayland duplicate-global re-advertisement leaves a stale `name→tag` entry | **OPEN, LOW, no owner.** | **permanent divergence** — pathological input no compositor produces; 4-criterion basis written in WS19 T9 | **S** | **WS19 EXECUTION STATUS:** **DONE — WS19 T9 (ledger half).** Duplicate-global nit is a **Justified R1 Divergence** with a 4-criterion basis. |
| O9 | `D:4565` kqueue `thread_perform` still forms `&mut Loop` on a pool worker (the `epoll.rs` twin is already fixed); `task_loop` lineage unaudited | **OPEN, real, no owner.** Filed as "Follow-up (open, low)" *inside a RESOLVED entry* — invisible to any grep for open items. Now reachable: the ThreadPool body has landed (see W5). | **fix in WS19 T10** (mirror epoll's copied-fd notify; audit the lineage on `mirihar-xev-mpsc`) | **M** | **WS19 EXECUTION STATUS:** **DONE — WS19 T10.** And the prescription it carried was WRONG: the harness proved "mirror epoll's copied-fd notify" INSUFFICIENT (`wakeup_copied`: still SB-UB — a by-value read is a foreign *access* against the loop thread's *protected* `&mut Loop`, and forming no reference is not enough). Cure applied to BOTH backends: the queue handle + the wakeup handle are copied into the caller-owned `Completion` at schedule time (`task_completions`, the reference's OWN field restored, + port-added `task_wakeup`), so `thread_perform` never accesses the `Loop` allocation and never derefs `task_loop` — which also discharges the lineage audit. 14-candidate SB/TB matrix in `.port/phase-2/mirihar-xev-mpsc/README.md`. |
| O10 | `D:3273` macOS `xev.File`/`TCP`/`UDP` watcher instantiations still deferred (Linux ported them, Tasks 3–5); plus the macOS `kqueue: file IO on thread pool` skip-stub | **OPEN, real, no owner.** The two targets deliberately diverge, gate-pinned by `crates/rustty-xev/tests/surface_parity.rs`. | **permanent divergence** — already deliberate and gate-pinned; WS19 T11 moves it into the divergence section with its 4-criterion basis rather than leaving it in a "DEFERRED" list | **S** | **WS19 EXECUTION STATUS:** **DONE — WS19 T11.** Permanent **Justified R1 Divergence**, 4-criterion basis. | **DEFERRAL RE-AUDIT STATUS (2026-07-31):** **REOPENED.** The permanence basis is a reachability argument, the same one this ledger retracted for `iocp`/`wasi_poll`. Oracle has the kqueue arms. STILL OPEN, size M.
| O11 | `D:3274` xev `c_api.zig` (libxev's own C ABI) unported | **OPEN, no owner.** rustty consumers use the Rust API; nothing in ghostty consumes libxev's C ABI. | **permanent divergence** (outside the port's consumption surface; basis in WS19 T11) | **S** | **WS19 EXECUTION STATUS:** **DONE — WS19 T11.** Permanent **Justified R1 Divergence**, 4-criterion basis. | **DEFERRAL RE-AUDIT STATUS (2026-07-31):** **REOPENED.** Same reachability defect as O10; criterion 3 is answered by criterion 4. STILL OPEN, size M.
| O12 | `D:2643` `+version`'s OSC-8 commit-hash hyperlink never emits (no `+BUILD` metadata, no repo-URL identity constant) | **OPEN, real, no owner.** The three STUB lines it is bundled with are GONE (see W8); only the hyperlink branch is still dead. | **fix in WS19 T12** (repo-URL constant in `rustty-identity` + a build-time commit hash) | **S** | **WS19 EXECUTION STATUS:** **DONE — WS19 T12. IMPLEMENTED, not diverged** — there was no honest 4-criterion basis for permanence (the repo URL already exists as workspace `Cargo.toml` `repository`, and the hyperlink is user-observable R1 behavior). `rustty_identity::COMMIT_URL_PREFIX` (both profile arms) + a `rustty-cli/build.rs` `RUSTTY_COMMIT_HASH` capture (env-overridable, degrades to `None` off a git checkout — R3-clean, reproducibility-clean) + a live `commit_url()`. Also fixed a latent BUG in the dead branch: it wrote the bare hash as the URI, with no URL prefix. Gated by 4 `rustty-cli` unit tests over a new `write_version(sink, tty)` seam (the tty-only branch is unreachable from `cli_smoke`, whose stdout is a pipe); 3 mutation experiments. |
| O13 | `D:3065`/`:4067` `main_wasm.zig` + `main_build_data.zig` unported, "Target: the gen/wasm-artifact tooling milestone" | **OPEN.** `grep -rn 'main_wasm\|main_build_data' crates/ --include='*.rs'` → **zero hits**. (`main_gen.zig` **IS** ported at `crates/rustty-synthetic/src/main_gen.rs`, so that third of the row is stale.) | `main_wasm` → **route to WS14**; `main_build_data` + the `+gen-*` file-emit wiring (`D:3725`) → **route to WS13** | **M** each | **WS19 EXECUTION STATUS:** **DONE (split + routed) — WS19 T14/T19.** `main_gen` PORTED; `main_wasm` → WS14; `main_build_data` + `+gen-*` → WS13 (in WS13's plan). **WS14 EXECUTION STATUS (O13a), 2026-07-30: CLOSED.** The "zero hits" derivation was a false negative — it grepped the reference FILENAME, which a Rust port never spells. `main_wasm.zig` was already ported (`crates/rustty-wasm/src/lib.rs`); WS14 shipped the missing ARTIFACT (freestanding `rustty_wasm.wasm` cdylib) + the corrected retention set. O13b (`main_build_data`) remains with WS13. |
| O14 | `D:4209` `Atlas.Wasm` `test "happy path"` (4 named assertions) deferred with the wasm atlas surface | **OPEN, correctly described**; target is an unscheduled milestone. | **route to WS14** | ~~**S**~~ | **WS19 EXECUTION STATUS:** **DONE (routed) — WS19 T14.** → **WS14**. **WS14 EXECUTION STATUS, 2026-07-30: CLOSED — all 4 asserts live** (`cargo test -p rustty-font --features wasm`), `check_test_counts.py` `font_atlas` apc_deferred 1 → 0. The **S** estimate was wrong: assert 1's `wasm.isHostOwned` was DELETED upstream by `9dc2e5978`, so the block dangles upstream and closing the row required resurrecting the tracker (`crates/rustty-os/src/wasm/host_owned.rs` + 2 reference tests). |
| O15 | `D:1894` `App.zig` `Wasm` block preserved verbatim, "Target: the rustty wasm app-runtime" | **OPEN, correctly described** (the block is commented out in the *reference* too). | **route to WS14** | **S** | **WS19 EXECUTION STATUS:** **DONE (routed) — WS19 T14.** → **WS14**. **WS14 EXECUTION STATUS, 2026-07-30: CLOSED** — ported live at `crates/rustty-core/src/app.rs` (`#[cfg(target_arch = "wasm32")] pub mod wasm`), complete BY BEING EMPTY, which is what let WS14 drop `rustty-wasm`'s unfaithful `rustty-core` edge. |
| O16 | `D:571` the **WebGL** renderer backend | ~~**OPEN, correctly described.**~~ **MIS-DESCRIBED** — see below. | **route to WS14** | ~~**L**~~ → **XS** | **WS19 EXECUTION STATUS:** **DONE (routed) — WS19 T14.** → **WS14**. **WS14 EXECUTION STATUS, 2026-07-30: CLOSED.** There is no WebGL backend upstream to port: `src/renderer/WebGL.zig` is **2 lines**, identical at the pin and on `origin/main`. The **L** estimate came from re-deriving the row against the PORT and never the ORACLE. Ported verbatim (~15 lines incl. PORT NOTES) at `crates/rustty-renderer/src/webgl.rs`; browser drawing is proven in the consumer under a falsifiable pixel gate. |
| O17 | `D:3741` `stb_image_resize.h` glyph resize has no rustty counterpart | ~~**OPEN, correctly described.**~~ **THIS AUDIT VERDICT WAS WRONG — corrected 2026-07-29 by the WS11 plan-authoring lane.** The port **SHIPPED at `2e82776` (2026-06-30)**: `rustty-font-shaper/src/stb.rs:18` declares `stbir_resize_uint8`, `build.rs` compiles `vendor/stb_impl.c` via `cc::Build`, `vendor/stb_image_resize.h` is byte-identical to the oracle (md5 `7381a7070120fd5a36d8fb9e3179dd7c`), call site `face/freetype.rs:806` is argument-for-argument identical to `freetype.zig:639-652`. **Root cause of the miss:** the row's evidence grep was scoped to `crates/rustty-font/src`, but the code lives in the sibling crate `rustty-font-shaper` created by the D1 three-tier split — *the grep scope WAS the bug*, so re-running it could not detect the error (see §G). | **RECLASSIFIED, not closed:** the port is done; what is open is that its test coverage is **VACUOUS** (all reaching tests run at scale 1.0; `render_and_resize` never enters the bitmap arm; swapping stb for nearest-neighbour leaves all 9 tests green). **Route to WS11**, which owns it with a 10-task plan. | **S** (row) / **M** (coverage, in WS11) | **WS19 EXECUTION STATUS:** **DONE (row CORRECTED) — WS19 T14.** The audit verdict was WRONG: the port SHIPPED at `2e82776`. Reclassified to vacuous test coverage → **WS11**. **WS11 EXECUTION STATUS: DONE — coverage CLOSED 2026-07-29.** 5 new tests (2 direct-FFI in `src/stb.rs`, 3 KATs in `face/freetype.rs` incl. a permanent machine-checked negative control) + new gate **D10** vs ghostty's own build recipe at 4 zig optimize modes, each with a demonstrated RED arm; nearest-neighbour substitution now FAILS both KATs while all 9 pre-existing tests still pass. The `ReleaseFast` ±1 LSB is ledgered as a permanent divergence with a four-criterion basis. **Nothing deferred.** |
| O18 | `D:3668` `rustty-synthetic` `cli.rs` 4 binary-entry seams (`main`, `main_action_impl_{ascii,osc,utf8}`) | **OPEN**; un-defer trigger is "the `rustty +synthetic` binary entry", which no WS owns. | **fix in WS19 T13** — wire the entries through the existing `rustty-cli` detection, **or** record as permanent if the acyclic split forbids it. Decide in-plan; do not defer. | **M** | **WS19 EXECUTION STATUS:** **DONE (row was STALE) — WS19 T13.** The 4 seams do not exist; only the local `detectArgs` remains, as a **Justified R1 Divergence** (R4). |
| O19 | `D:3697` `rustty-bench` residuals: macOS `os_signpost` interval; `main_bench.zig` binary entry; locally-replicated `detectArgs` | **OPEN, no owner.** | `os_signpost` → **permanent divergence** (Instruments-only instrumentation, zero behavior); the binary entry + local `detectArgs` → **fix in WS19 T13** with O18 (same class) | **S** | **WS19 EXECUTION STATUS:** **DONE — WS19 T13/T14.** All four residuals disposed: `os_signpost` + local `detectArgs` are divergences; the bin entry SHIPPED; the alias clause is MOOT. |
| O20 | `INTEGRATION_PARITY §2.8` terminfo SOURCE + termcap never emitted (diverges only in Debug builds); fix-list item 8 is marked "(Optional)" | **OPEN, no owner — and "optional" is not a disposition.** | **permanent divergence** — the shipping app is Release, where rustty MATCHES the oracle exactly; WS19 T14 writes the basis and deletes the fix-list row | **S** | **WS19 EXECUTION STATUS:** **DONE — WS19 T14.** Permanent **Justified R1 Divergence**; `INTEGRATION_PARITY` fix-list item 8 DELETED. | **DEFERRAL RE-AUDIT STATUS (2026-07-31):** **WRONG — DISCHARGED.** Both files ARE emitted unconditionally: `xtask/src/res_terminfo.rs:123` calls `emit_source_and_termcap` (`:165`), asserted at `:260`. The divergence entry is obsolete.
| O21 | `D:4595` `rustty-termio` `shell_integration.rs` `TmpResourcesDir` builds paths via `to_string_lossy()` (violates zig-byteslice-to-rust-rawbytes) | **OPEN, LOW, no owner.** Test-only helper. | **fix in WS19 T15** (`OsStrExt::as_bytes`) | **S** | **WS19 EXECUTION STATUS:** **DONE — WS19 T15.** `OsStrExt::as_bytes` at both sites, with a RED/GREEN discriminator (not a smoke test). |

## C. Rows whose CLAIMED STATUS WAS WRONG (headline deliverable)

Verified against code, not transcribed. **Both directions appear** — claimed-open
things that shipped, and claimed-done things that are vacuous or incomplete.

| # | Row | Claim | Verified reality | Direction |
|---|---|---|---|---|
| W1 | `D:1839`, `D:1869-1878`, `renderer_seam.rs:35` | "`renderer_seam.rs` — **15** `unimplemented!()`, the C3 dependency-placeholder bodies … **Target: Phase-3 R-d GPU tier / D6**"; module header "PORT STATUS: placeholder" | **WRONG.** All 15 sit inside `#[cfg(test)] mod compression_tests` (`renderer_seam.rs:2013`), each reading `unimplemented!("test runtime: not driven by the compression tests")` — *test-runtime fakes*. Production `Renderer::{surfaceInit,init,finalizeSurfaceInit,attachHostView,threadEnter,drawFrame,displayRealized,displayUnrealized,deinit}` and `Thread::{init,threadMain,deinit}` are LIVE (`:242,:269,:321,:337,:361,:390,:415,:439,:454,:669,:875,:939`). **Zero production leaves.** | claimed-open, actually DONE |
| W2 | `D:2609-2614` (E11 leaf audit) | "apprt-c has exactly **4** `unimplemented!()` code-leaves: `rustty_set_window_background_blur`, `_surface_set_display_id`, `_surface_quicklook_font`, `_surface_quicklook_word`" | **WRONG.** `grep -rn 'unimplemented!' crates/rustty-apprt-c/src/` → **6 hits, all inside comments**. `rustty_set_window_background_blur` is LIVE at `lib.rs:2515`+ (real objc2 `msg_send!` + CGS). The other three were closed by P2.1/P2.2/WS2/WS6. **Zero leaves.** | claimed-open, actually DONE |
| W3 | `D:2954`, `D:2989-2996` | "`boo` — **STILL DEFERRED** (the crate's ONLY live seam, `boo.rs:22`)" | **WRONG, and self-contradicted 240 lines earlier** at `D:2747` ("`boo` … ✅ RESOLVED"). `crates/rustty-cli/src/boo.rs` is a 525-line LIVE port with a real `run()`; `boo.rs:22` is a doc-comment line. `crates/rustty-cli/src/lib.rs:12` repeats the false claim. | claimed-open, actually DONE |
| W4 | `D:3409-3421` | "`Timer::cancel` on Linux … To un-defer: make `Completion::new_op` `pub(crate)` … **core will not build on Linux until this is un-deferred**" | **WRONG.** `crates/rustty-xev/src/dynamic.rs:1532` `Timer::cancel` is fully ported with BOTH arms (`io_uring::Operation::TimerRemove`, `epoll::Operation::Cancel`) through `Completion::new_op`; its own PORT NOTE says "This un-defers the C7 gap". Linux `cargo build --workspace` is green. The ROADMAP flagged this; the ledger row was never struck. | claimed-open, actually DONE |
| W5 | `D:3258-3266` | "`ThreadPool.zig` work-stealing **BODY** deferred; only the TYPE SURFACE is ported" | **WRONG.** `crates/rustty-xev/src/thread_pool.rs` is **1630 lines with 0 `unimplemented!()`** — the full Zap pool, shared by both targets, SB/TB-proven on `mirihar-threadpool`. (The row's narrow residual, the macOS `kqueue: file IO on thread pool` skip-stub, is real → O10.) | claimed-open, actually DONE |
| W6 | `D:3771` | "signed `.xcframework` repackaging xtask … **No such xtask exists yet** (the current `xtask` only does `header-gen-apprt`)" | **WRONG.** `xtask/src/main.rs:65,103` registers `xcframework` ("Package rustty-apprt-c into dist/RusttyKit.xcframework"); `xtask/src/` also has `dist.rs`, `archives.rs`, `res_*`, `translations.rs`, `wasm_patch_growable_table.rs`. | claimed-open, actually DONE |
| W7 | `D:3756` | "the `ghostty_*`→`rustty_*` C-ABI compatibility ALIAS layer … No rustty crate provides this yet" | **WRONG in premise, and the premise is MOOT.** `grep -rn 'export_name' crates/rustty-apprt-c/src/` → **0 hits**; `crates/rustty-apprt-c/Cargo.toml:31-33` states the crate "no longer compiles any `ghostty_*` C-ABI alias layer … exports ONLY `rustty_*` (R2), always, in every profile" — upstream **deleted** the alias layer at `e4cc852` (recorded in the WS5 note). **Consequence: the ROADMAP's global constraint "`ghostty_*` C-ABI aliases stay" (`ROADMAP:25`) is FALSE** — see §E.3. | claimed-open, actually SUPERSEDED |
| W8 | `D:3089-3093` (the C6 copy) | "`version.zig` renderer/libxev/toolchain **STUB lines** (continued from C1)" | **WRONG.** `grep -rn 'RENDERER_STUB\|LIBXEV_STUB\|TOOLCHAIN_VERSION_STUB' crates/` → **zero hits**; WS3-B replaced all three with real values and the `cli_smoke` `version_plus` KAT asserts their content. The `D:2639` copy records this; the `D:3089` copy still asserts the stubs. Only the OSC-8 hyperlink survives (O12). | claimed-open, actually DONE |
| W9 | `D:3038-3047` | "**Phase-3 GUI launch leaf** (`main_ghostty.rs`) … un-defer when the embedded/GTK apprt backend + xev (D6) land" | **PARTIALLY WRONG.** `crates/rustty/src/main_rustty.rs` has ONE `unimplemented!()` (`:299`) whose message is "GUI launch has no compiled backend in **this build**"; the GTK arm (`run_gtk`, `:292`, diverging `-> !`) is LIVE and returns before it. The leaf is reachable only on a configuration with no compiled backend — unreachable on both supported hosts. | claimed-open, mostly DONE |
| W10 | ~40 sibling "**Target: Phase-3**" labels (`D:954`, `:998`, `:1148`, `:2027`, `:2129`, `:2302`, `:2528`-`:2549`, …) | Every core/embedded chunk row targets its residual at "**Phase-3** R-d GPU / D6 xev / dcimgui" | **SYSTEMATICALLY MIS-LABELLED.** "Phase 3" in this file is the *pre-WS codename* for the GPU/xev/inspector tiers, **all of which have since landed** (W0–W5, xev Milestones A/B, WS6). Phase 3 today means **upstream resync** (`.port/phase-3/README.md`). Anyone grepping "Phase-3" here to build a Phase-3 worklist gets ~40 false positives. | mis-labelled |
| W11 | `VERIFICATION_DEBT.md:15` V1 | "**V1.** macOS build + test of `rustty-xev` (blocked: no macOS toolchain on this VM)" — listed as OPEN | **WRONG.** `ROADMAP:41` records "VD:V1 … **DISCHARGED 2026-07-23**", and every WS since gates on `cargo build --workspace` + `cargo test` on macOS. The row was never deleted, though that file's own header says "When an item is verified … delete it here". | claimed-open, actually DONE |

## D. New items this audit records (not previously in any ledger)

**N1–N5 are vacuous-or-broken gates and are the highest-priority rows in this audit:
a vacuous gate means every "green" claim it ever made is unproven.**

| # | Item | Verified evidence | Owner | Cost |
|---|---|---|---|---|
| **N1** | **G1b (`check_test_counts.py`) hard-codes the RUST-side absolute path, so run from a git worktree it gates `main`, not the working tree — G1b is VACUOUS for every worktree change** | `.port/tools/gates/check_test_counts.py:25,29,39,43,46,52,56,64,67,76,91,92` all read `/Users/gael/projects/rustty/crates/...`. Identical bug to the one `miri-run.sh` already fixed via `git rev-parse --show-toplevel`. **The ORACLE-side hardcodes (`:24` etc.) are CORRECT and must stay** — R3 makes the oracle external. Today the bug is masked by N4 (the gate exits 3 BLOCKED before counting), so **fixing N4 without N1 would turn a blocked gate into a silently WRONG one**. | **WS19 T1** | **S** | **WS19 EXECUTION STATUS:** **DONE — WS19 T1.** `RS_ROOT` from `git rev-parse --show-toplevel`; 26 hardcodes rewritten (the audit said ~12); a RUST-side stale-mapping guard added, FAIL arm mutation-proven. |
| **N2** | **`include/rustty.h` does not compile standalone as C** — `unknown type name 'Opaque'`; gate A5 only compiles the *curated* `rustty/vt.h`, which is why it was never caught | `clang -fsyntax-only -I include` over a TU including `rustty.h` → 2 errors at `include/rustty.h:2455` (`Opaque *rustty_wasm_alloc_opaque(void);`) and `:2457`. `grep -n 'Opaque' include/rustty.h`: the only `typedef`s are `Node` and `Command` — `Opaque` is never declared (cbindgen `WARN: Can't find Opaque`). `.port/tools/gates/check_rustty_vt_headers.sh:13` probes `#include <rustty/vt.h>` only. **Pre-existing** (confirmed by stashing), not introduced by any lane. | **WS19 T3** | **S** | **WS19 EXECUTION STATUS:** **DONE — WS19 T3.** `typedef void *Opaque` pinned in `cbindgen.toml`; A5 now compiles ALL published headers as C11 + C++17, and gained an ORACLE-driven bidirectional export arm which immediately found 2 more missing exports (→ Phase 3 **P3-RV5**). |
| **N3** | **`crates/rustty-apprt/tests/gtk_wayland_winproto.rs` has TWO `gtk4::init()` tests in one binary, which cannot work** | `:26` and `:81` both call `gtk4::init()`. `gtk4::init()` pins GTK to the calling thread; libtest gives each `#[test]` its own thread even at `--test-threads=1`, so the second panics *"Attempted to initialize GTK from two different threads"*. WS16 T2 already established the fix (`a7fa3b7` "split the x11 KATs one-per-binary (GTK is thread-pinned)"); the wayland file was never split. Latent in already-merged code. | **WS19 T4** | **S** | **WS19 EXECUTION STATUS:** **DONE — WS19 T4.** Wayland KATs split one-per-binary; new HARD gate **G-GTKINIT**, both FAIL arms mutation-proven. |
| **N4** | **G1b is BLOCKED, not green** — `~/.cache/zig/p/` was evicted, so the vaxis/zf/libxev reference corpus is gone | `ls ~/.cache/zig/p/` → *No such file or directory*. `python3 .port/tools/gates/check_test_counts.py` → **exit 3**, "BLOCKED — missing zig reference file(s)", listing 42 paths under `zf-0.10.3-…`, `vaxis-0.5.1-…`, `libxev-0.0.0-…` (pinned at `:162,:183,:200`). `.port/tools/restore_zig_deps.sh` **does not exist** — only *proposed* in the WS12 plan. Correctly surfaced as a WARNING (never a silent green), but the per-module gate does not bind at all today. | **WS19 T1** (write the hash-pinned restore script; WS12 consumes it) | **S** | **WS19 EXECUTION STATUS:** **DONE — WS19 T1.** `.port/tools/restore_zig_deps.sh` (pin = zig's own content hash, verified via `zig fetch`); G1b went 3 BLOCKED → 0 GREEN. |
| **N5** | **`cargo test --workspace` (ci.sh step 4, the DEBUG lane) lacks `--no-fail-fast`, so a failure in one crate MASKS every later crate** | `.port/tools/ci.sh:289` is bare `cargo test --workspace`. This is what hid the `rustty-extra` golden failures behind `rustty-apprt-c --test smoke`. **Half the reported symptom is already fixed:** `ci.sh:288` runs `cargo build --workspace` first, so the cold-`target/` uplift problem (smoke needs `librustty_apprt_c.dylib` in `target/<profile>/`) does not bite *inside* ci.sh — only outside it. `check_release_lane.sh:148,155` already does both correctly; the debug lane must match. | **WS19 T1** | **S** | **WS19 EXECUTION STATUS:** **DONE — WS19 T1.** `--no-fail-fast` on ci.sh's debug lane; the masking was reproduced live (a `rustty-apprt-c --test smoke` failure at output line 485 of 5039). |
| **N6** | **G3b comment-coverage counts RISE as an ARTIFACT of porting beyond-pin commits** — the gate diffs against the pin, so a sentence upstream DELETED beyond the pin still exists at the pin and is reported "missing" | `.port/tools/gates/check_comment_coverage.py:34` reads `GHOSTTY_SRC`, defaulting to `/Users/gael/projects/ghostty/src` — the oracle **working tree, pinned at `4749c4e93`**. WS7 D2 saw the config lane go 5→9 for exactly this reason. **VERDICT (the question this audit was asked to settle): advancing the pin in Phase 3 resolves it WHOLESALE, and is the only correct fix.** Once `GHOSTTY_SRC` is the resynced tree, the deleted sentences are absent from the reference side and the counts drop by construction — no per-file exception list, no gate change, no R5 exposure. **Do NOT paper over it with a per-file `GHOSTTY_SRC` override**: that is precisely the spoof hole `ci.sh:114-129` killed for G1b (a probe redirected away from what the script actually reads yields a vacuous green). Until the pin advances, beyond-pin G3b deltas are EXPECTED and must be recorded per-lane. | **no code change; WS19 T2 records the verdict**, Phase 3 realizes it | **S** | **WS19 EXECUTION STATUS:** **DONE (verdict recorded) — WS19 T2.** G3b subsection in the gates README + Phase 3 **P3-RV2**; per-file `GHOSTTY_SRC` override explicitly FORBIDDEN. |
| **N7** | **`example/*/build.sh` (all 23) link `target/release/librustty_vt_c.dylib` but build it only IF ABSENT, so a stale release dylib silently fails or mis-links** | `example/c-vt-colors/build.sh:11`: `[ -f "$lib" ] \|\| ( cd "$root" && cargo build … )`. Identical in all 23 (`ls example/*/build.sh \| wc -l` → 23). | **WS19 T16** — ~~open~~ **RESOLVED 2026-07-29**: guard replaced with an unconditional `cargo build` in all 23 (cargo is already incremental, so the avoid-rebuild motive was void). Hazard demonstrated concretely, not argued: with the old guard and a present-but-bogus dylib, `ld: unknown file type in .../librustty_vt_c.dylib`; with the new one the rebuild replaces it and the example runs. All 23 build+run green (`ok=23 fail=0`), including after `touch -t 200001010000` on the dylib. Repo-wide sweep for the same idiom: no other occurrence. | **S** | **WS19 EXECUTION STATUS:** **DONE — WS19 T16.** All 23 `example/*/build.sh` rebuild unconditionally; hazard demonstrated (`ld: unknown file type`), all 23 build+run. |
| **N8** | **3 `.blp` msgids still spell "Ghostty" and ZERO code-side i18n call sites are wrapped**, so `xgettext` extracts no msgid from any `.rs` file | `assets/ui/1.3/debug-warning.blp:6`, `assets/ui/1.5/inspector-window.blp:5`, `assets/ui/1.5/window.blp:307`. `grep -rn 'gettext_' crates/rustty-apprt/src crates/rustty-core/src` finds only the `i18n::init`/`initGlobalDomain` wiring, never a wrapped literal. **LEDGERED?** Yes — the WS13 plan documents both as findings F3/F4 with exact file:line tables and owns them in its T2/T3. This row is the `DEFERRALS.md` cross-reference so a ledger grep finds them too. | **WS13** (already planned) | **M** | **WS19 EXECUTION STATUS:** **DONE (routed) — already WS13's**; cross-referenced. |
| **N9** | **`threadEnter` error arms should drain the pty master BEFORE `stop()`** (`D:5090`) and **`posix_waitpid` aborts on `ECHILD` where a concurrent reaper can win the race** (`D:5130`) — both filed 2026-07-29 with **NO owner** | Both verified as described. `posix_waitpid`'s `unreachable!("waitpid: unexpected errno")` is FAITHFUL (Zig std does the same), so the exposure is inherited — but Rust's `unreachable!` always panics where Zig's is a no-op in `ReleaseFast`, so the *consequence* diverges. The second reaper is real: `crates/rustty-xev/src/process.rs` calls `waitpid` in the `.proc` completion trampoline. | **WS19 T17** (one teardown-ordering fix + one shim-signature change; BOTH must be recorded as `[R1-DIVERGENCE]` alongside the bounded `killPid`) | **M** | **WS19 EXECUTION STATUS:** **DONE — WS19 T17.** BOTH fixed, but the disposition is **ONE** `[R1-DIVERGENCE]`, not two — this row's own "`posix_waitpid` is FAITHFUL" premise went STALE. Upstream head dropped the Zig-std wrapper for the raw syscall (`a77c706a1`/`e8525c0fd`), so its `killPid` now breaks on `rc == -1`, i.e. an `ECHILD` ends the loop gracefully upstream too; the fix therefore REMOVES a divergence and warrants no divergence entry (`.port/phase-3/assessments/E-os-termio-platform.md` §5.2). (a) `drain_pty_master_once` + all **6** live `stop()` arms in `threadEnter` → recorded as a `[R1-DIVERGENCE]` next to the bounded `killPid`, with the E-§5.3 upstream-head re-verification. (b) `posix_waitpid_checked` used ONLY by `killPid`; the verbatim shim untouched on the `Command::wait` path. 2 new KATs, 3 mutation experiments; `repro_subprocess_hang.sh` 6/6 runs 0 hangs. The bounded `killPid` divergence itself is UNCHANGED (area E: KEEP). |
| **N11** | **`ci.sh` STEPS 1 AND 2 — `cargo fmt --check` and `cargo clippy --workspace --all-targets -- -D warnings` — WERE BOTH RED ON `main`.** Not vacuous: genuinely FAILING, and evidently never run. Every lane that reported "gates green" reported it without these two. | Reproduced on the pristine `/Users/gael/projects/rustty` checkout at the pinned `rustc 1.93.1`: `cargo fmt --check` → **317 diffs across 76 files**; `cargo clippy --workspace --all-targets -- -D warnings` → **RED in 8 crates** (`rustty-z2d` 5, `gettext-sys` 1, `rustty-vt` 6, `rustty-vaxis` 1, `rustty-renderer` 4, `rustty-web-canvas` 6, `rustty-termio` 1, `xtask` 3). Found while running WS19 T7's per-crate clippy, not by looking for it. **Two root causes worth naming: (1) five of the "doc list item" failures share ONE bug — a doc-comment line beginning with `+ `, which markdown reads as a list bullet, making the lines after it lazy continuations (`rustty-z2d/src/lib.rs`, `rustty-renderer/src/generic.rs`, `rustty-termio/tests/subprocess_drop_repro.rs`); (2) the `not_unsafe_ptr_arg_deref` cluster in `rustty-web-canvas` is a REAL C-ABI signature gap — four `extern "C"` exports dereference caller-supplied pointers without being `unsafe fn`, unlike the `rustty-vt-c`/`rustty-apprt-c` convention. The rest are fidelity-preserving constructs (`(n + 7) / 8` ceil-div, reference-shaped large enum variants, ordering-marker `drop()` of a non-`Drop` value) now allowed AT THE SITE with a written reason, never crate-wide, and one genuinely useless assertion (`gettext-sys`'s `assert!(!(f as *const ()).is_null())` on a function pointer, replaced by `black_box` since only the linker reference was ever load-bearing). | **WS19 T7** (fixed in-line; both steps now green) | **S** | **WS19 EXECUTION STATUS:** **DONE — WS19 T7 (in-line).** `cargo fmt --check` 317 diffs → 0; `clippy --workspace --all-targets -D warnings` RED in 8 crates → clean. |
| **N10** | **DOCUMENTATION finding (not a defect): a fresh `Terminal::compress(.incremental)` right after a COMPLETED pass returns `Pending`, not `Complete`**, because the incremental traversal restarts — this invalidates the obvious "work is done" probe | `crates/rustty-vt/src/terminal.rs:2861-2894` forwards straight to `PageList::compress(Incremental)` and maps the result 1:1; the doc comment (`:2847-2860`) says only "the return value tells callers whether they should continue calling it" and never states the restart. `renderer_seam.rs:1990-1997` already depends on the distinction (it schedules another tick on `Pending`, stops on `Complete`/`Unsupported`). Nothing anywhere records the gotcha, so the next test author will read `Pending` as "not finished". | **WS19 T18** — ~~open~~ **RESOLVED 2026-07-29.** Behavior CONFIRMED by scratch probe before documenting anything: driving `compress(Incremental)` to `Complete` and calling once more printed `WS19-T18-PROBE: after Complete, next incremental = Pending`. Landed as (a) a `PORT NOTE` on `Terminal::compress` stating that `Pending` means "call me again", never "work is outstanding", and that `Complete` is not a durable probe; (b) `terminal_compress_incremental_restarts_after_complete`, which also asserts the pass took >1 step so a one-call convergence cannot make it vacuous; (c) a cross-reference in `MIRI_EXCEPTIONS.md`'s compression-coverage argument, where the same misreading would corrupt a coverage claim. | **S** | **WS19 EXECUTION STATUS:** **DONE — WS19 T18.** Restart CONFIRMED by probe, then documented in 3 places and pinned by a test that cannot go vacuous. |

## E. Structural ledger defects found (fix in WS19 T2)

1. **Two numbered items are physically MIS-FILED.** `D:5282` (item 3, the
   `decommit_pool_item` SB violation) and `D:5301` (item 4, the Miri-intractable
   compression family) are the continuation of the numbered list under
   "**Deferred to WS7 Phase E**" (`D:5016`, items 1–2 at `:5020`/`:5066`) — but they
   were appended AFTER the `# Justified R1 Divergences` heading (`D:5166`) and now sit
   *inside* the `killPid` divergence entry. **Consequence:** the O2 work item at
   `D:5357` is buried inside a permanent-divergence entry, where nobody looking for
   open work will find it. Both items move back under their own heading.
2. **Two genuinely-permanent items live OUTSIDE the divergence section.** The
   `rustty-termio` ReadThread POLLHUP quirk (`D:4713` — "preserved verbatim, NOT
   fixed … reference behavior, not a port bug") and the xev ThreadPool `Event`
   shutdown-wake deviation (`D:4910` — "DELIBERATE DEVIATION (controller-approved)")
   are permanent by their own text but are filed among deferrals. Both move into
   "Justified R1 Divergences", carrying their existing rationale as the basis.
3. **The `ghostty_*` alias-layer ROADMAP constraint is false** (see W7). `ROADMAP:25`
   must read: "`ghostty_*` C-ABI aliases were deleted upstream at `e4cc852`; rustty
   exports `rustty_*` only, in every profile."
4. **~40 "Target: Phase-3" labels mean the wrong thing** (see W10). Each is retargeted
   to the tier that actually owns it (all landed) or deleted.

## F. Verified-no-action rows (sampled evidence, for auditability)

The 58 no-action rows are the `~~struck~~` / `RESOLVED` / `✅` entries throughout this
file. Spot-verified during this audit and confirmed accurate: `D:4293` wuffs swizzle
(6 seams filled), `D:4327` z2d subset guards (9 arms filled), `D:4363` wasm VT C-ABI
(real `.wasm`; 3 of 4 residuals struck), `D:4446` shaper fontconfig (`unimplemented!`
gone), `D:4229` W-ATTACH (`attachHostView` live at `renderer_seam.rs:337`), `D:3982`
WS15 GlobalShortcuts (merged `e8a9f59`), `D:4776` x11 winproto (merged `07b64db`),
`D:4627` gettext-sys libintl (30 TUs vendored), `D:4670` P2 crash-guard leaves,
`D:1148`/`D:2168` inspector (WS6), `D:2661`/`D:2815` help_strings + `explain_config`,
`D:2854` `list_fonts`/`show_face`.
**Out-of-scope-by-platform (NOT deferrals, per `ROADMAP:214`):** every
`#[cfg(windows)]` seam (`D:3155`, `D:4155`, `D:4169`), the `iocp`/`wasi_poll` xev
backends, and the iOS device runtime.

## G. Methodology correction — the self-confirming-scope failure mode [added 2026-07-29]

**How this audit got a row wrong.** Row **O17** was recorded as "OPEN, correctly
described". It was **not open** — `stb_image_resize` shipped on 2026-06-30 at
`2e82776`. The audit verified the row by re-running the row's own evidence grep,
which was scoped to `crates/rustty-font/src`. The code lives in the **sibling**
crate `crates/rustty-font-shaper/`, created afterwards by the **D1 three-tier
split**. *The grep scope WAS the bug*, so re-running it was a self-confirming check
that could not detect the error.

**RULE, binding on every future ledger audit: never verify a row with a search
scope the row itself supplies. Use a repo-wide search, then narrow.** A row that
names its own scope is exactly the row whose scope may be stale — crate splits,
renames (`ghostty.rs`→`rustty.rs` at `410c387`, which also broke a G1b mapping) and
re-homings all invalidate scopes silently while the row's prose stays plausible.

**Re-derivation of every affected verdict [WS19 T14, 2026-07-29].** Each row below
was re-checked with a **repo-wide** grep instead of the row's own scope. Result: O17
was the only wrong one, but two further rows were found to be **stale in the
opposite direction** — claiming open work that has shipped.

| Row | Repo-wide re-derivation | Verdict |
|---|---|---|
| **O17** stb glyph resize | `grep -rn 'stbir_resize_uint8' crates/` → `rustty-font-shaper/src/stb.rs:18`, `build.rs`, `face/freetype.rs:806` | **WAS WRONG** — ported at `2e82776`; reclassified to vacuous coverage → **WS11** |
| **O18** synthetic `cli.rs` "4 live `unimplemented!()` binary-entry seams" | `grep -rn 'unimplemented\|todo!' crates/rustty-synthetic/` → **0 hits**. `cli.rs:97` `main()`, `:245/:274/:297` `main_action_impl_{ascii,osc,utf8}` are all LIVE; `Cargo.toml:26-28` registers the `rustty-synthetic` bin at `src/main_gen.rs`, whose `fn main` calls `cli::main()` | **WAS WRONG (stale)** — all 4 seams filled; only the LOCAL `detectArgs` replica remains, and that is a permanent acyclic-split divergence (see the divergence section) |
| **O19** bench `main_bench.zig` binary entry | `crates/rustty-bench/Cargo.toml:21-23` registers the `rustty-bench` bin at `src/main_bench.rs`; `main_bench.rs:24` `fn main() -> ExitCode` calls `rustty_bench::cli::main()` | **WAS WRONG (stale)** — shipped. `os_signpost` (`benchmark.rs:81-101`, no-op + verbatim body) and the local `detectArgs` are the real residuals, both permanent divergences |
| **O14** `Atlas.Wasm` test | `grep -rn 'wasm' crates/rustty-font/src/atlas.rs` → `:740` "PORT NOTE (wasm — feature-gated OFF)", block preserved as comments | ~~confirmed **OPEN** → WS14~~ **CLOSED by WS14 2026-07-30** (block live under `feature = "wasm"`; 4/4 asserts) |
| **O15** `App.zig` Wasm block | `grep -rn 'Wasm' crates/rustty-core/src/app.rs` → `:998`–`:1007`, preserved as comments | ~~confirmed **OPEN** → WS14~~ **CLOSED by WS14 2026-07-30** (ported live; empty by construction, per the reference) |
| **O16** WebGL backend | repo-wide `grep -rni 'webgl'` → only `build_config::renderer::Backend::Webgl` (an enum variant) + prose "WebGL wasm-deferred"; no backend implementation | ~~confirmed **OPEN** → WS14~~ **CLOSED by WS14 2026-07-30.** METHOD DEFECT worth remembering: this probe searched the PORT only. The ORACLE has no backend either (`src/renderer/WebGL.zig` = 2 lines), so "no backend implementation" was PARITY, not a gap. Re-derive open rows against the oracle, not just the port. |
| **O5** `PageList::Limits` | repo-wide `grep -rn 'struct Limits'` → only `kitty/graphics_image.rs:126` (an unrelated struct); no `PageList::Limits` | confirmed **OPEN** → Phase 3 re-vet P3-RV1 |
| **O6** GTK portal `OpenURI` | repo-wide `grep -rni 'openuri\|open_uri'` across `crates/` → **0 hits** at audit time; the only portal code was `gtk/class/global_shortcuts.rs` (GlobalShortcuts iface) | was **OPEN** → **CLOSED by WS19 T8, 2026-07-29** (`gtk/portal.rs` + `gtk/portal/open_uri.rs`, live-gated against a real `xdg-desktop-portal` `Response` code 0) |
| **O9** kqueue `thread_perform` | `crates/rustty-xev/src/kqueue.rs:1446` `thread_perform` ended in `(*(*c).task_loop).wakeup()`, and `wakeup(&mut self)` at `:1475` — the autoref forms `&mut Loop` on a pool worker | **CLOSED** by WS19 T10 (both backends; `thread_perform` no longer derefs `task_loop` at all). NOTE: the Miri harness proved the *prescribed* epoll-style fix (copy the field, form no reference) is ITSELF UB under Stacked Borrows — a by-value read of a `Copy` field is still a foreign ACCESS on the `Loop` allocation. This also retroactively invalidates the previously-"RESOLVED" M1 verdict. |
| **O12** `+version` OSC-8 hyperlink | `crates/rustty-cli/src/version.rs:66` `commit_hash()` returned `None` unconditionally; `:103` was the unreachable emit (and wrote the bare hash as the URI, with no URL prefix at all) | **CLOSED** by WS19 T12 (implemented, not diverged — the repo URL already exists as the workspace `Cargo.toml` `repository` key, so there was no honest 4-criterion basis for permanence) |
| **O21** `shell_integration.rs` lossy path | `crates/rustty-termio/src/shell_integration.rs:1708` and `:1760` both `to_string_lossy()` | confirmed **OPEN** → WS19 T15 |
| **O1** `style::Color` | `crates/rustty-vt/src/style.rs:143` is a plain `enum Color { None, Palette(u8), Rgb(RGB) }`, no `#[repr]` | confirmed **OPEN** → WS19 T5 |
| **O3** `cursor_scroll_region_up` | already repo-wide in the original audit (`grep -rn … crates/` → 0 hits) | confirmed **OPEN** → WS10, pin-sequenced |

**Related gate consequence.** The same class of staleness broke a **gate**, not just a
ledger row: G1b's `MAP` still pointed at `crates/rustty-cli/src/ghostty.rs` and
`crates/rustty/src/main_ghostty.rs` after `410c387` renamed both. WS19 T1 added a
RUST-side existence guard so a mapped-but-absent path is a loud RED instead of a
silent `rust=0`. Scope staleness is a recurring defect in this program, not a
one-off; gates that name paths need existence guards.

---

# DEFERRAL RE-AUDIT — independent re-verification of this whole file [2026-07-31]

Full report with per-row evidence: **`.port/phase-3/DEFERRAL_REAUDIT.md`**.

This pass re-derived every deferral in this file, and in the sibling ledgers,
against today's code — deliberately NOT transcribing the WS19 audit above. Both
binding method rules were applied: never verify a row with the scope the row
supplies, and re-derive "not ported" against the ORACLE as well as the port.

## Counts

| | |
|---|---|
| Rows examined (this file + all sibling ledgers) | **231** |
| DISCHARGED (verified done) | **196** |
| STILL OPEN (verified unimplemented) | **19** |
| NON-PORT (argued, survives the scope rule) | **16** |
| Newly implemented in this pass | **5** |
| **Rows whose claimed status was WRONG** | **14** (11 claimed-open-but-done, 3 claimed-permanent-but-open) |

## The error direction has INVERTED since WS19

WS19's failure mode was *orphaned open rows*. Today's is **stale-open rows**: work
that P3-W3, P3-W5, P3-W10, WS13, WS14 and WS20 landed without anyone ticking the
ledger. Eleven rows in this file assert missing code that exists.

## G-P2 is an OWNERSHIP gate, not a COMPLETION gate

`.port/tools/gates/check_phase2_closure.py` is GREEN (93 entries, 0 violations) and
was green through every one of the 11 wrong rows. Re-read its predicate: each row
must be *"struck, reclassified, or **owned by an OPEN workstream**"*. A row reading
"OPEN, owner: Phase 3" satisfies that forever — whether the work is untouched or
shipped months ago. The gate is well-built for what it does (real `--self-test`,
real FAIL arms) but **structurally cannot detect an item that is done, nor one that
is undone**. Recommendation: rows asserting "X is unported" should carry the grep
that made the claim, re-run by the gate, so staleness in the DONE direction goes RED.

## Pin advance invalidates "BEYOND-PIN" dispositions

The oracle pin moved `4749c4e93` → `6ad1fe7d8` on 2026-07-31 (365 commits,
`.port/ORACLE_SYNC.md:36`). Both rows parked behind it (**O3**
`cursor_scroll_region_up`, **O5** `PageList::Limits`) were already ported. **A pin
advance must trigger a ledger sweep, not just a resync.**

## "Reachability" is retired as a permanence basis

It has now produced four wrong dispositions: `iocp`/`wasi_poll` (already retracted
here), macOS `xev.File`/`TCP`/`UDP`, libxev `c_api.zig`, and the 13 vxfw widgets.
Under the standing scope rule, "nothing consumes it" is not a reason not to port it.
The three still-live instances are reopened above.

## Still open — summary (full detail in the re-audit report §C)

* **S/XS:** ~~`freetype-load-flags` silently ignored~~ and ~~termio `dcsCommand`
  tmux arm is a no-op~~ — **both CLOSED 2026-07-31 (`fix-freetype-tmux`)**, see
  their rows above;
  wayland `bind_global` map shape + leaked proxy; `os_signpost` no-op;
  `setNonblock` drain arm; iOS Darwin export gating; `renderer/Thread.zig` G3b
  comment drift (11); `MIRI_EXCEPTIONS.md` missing 15 annotations.
* **M:** `stream.zig` `Action` C-ABI union; `vt -> xev` fake module
  (`rustty-vt/src/search/thread.rs:1740`); macOS `xev.File`/`TCP`/`UDP`; libxev
  `c_api.zig`; uucode bump (G1); Windows termio spawn arms.
* **L:** vaxis 0.6.0 / zf 0.11.0 upstream delta (G2); 13 unported vxfw widgets.
* **Verification-only** (code present, host absent): V2–V7, V15–V19, V21(b).

## Implemented in this pass

1. `sgr::SepList` de-duplicated onto `parser::SepList` (oracle has ONE type);
   mutation-proved — 12 sgr tests RED under an injected fault, 2302/0 after revert.
2. **G3b `--ref-files` vacuity hole closed** — the scope override silently reported
   `distinctive ref comments=0 ... GREEN` for unresolvable or nonexistent paths,
   reintroducing the very "coverage against nothing" spoof hole that module's
   `GHOSTTY_SRC` note refuses. Now BLOCKED (2). This is what exposed the
   `renderer/Thread.zig` drift.
3. `THREAD_POOL_BODY_PORTED` -> `MACOS_FILE_WATCHER_PORTED` — the marker named a
   blocker that shipped; mutation-proved both arms.
4. zf `SEP` made target-native (upstream uses `std.fs.path.sep`; the hardcoded `/`
   mis-scored path segments on Windows).
5. Four stale "still `todo!()`" claims corrected (`Colors::default()` is implemented;
   two were justifying duplicated test helpers).
