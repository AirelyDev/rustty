#!/usr/bin/env python3
"""G-gate: per-module Zig test{} count vs Rust #[test] count.

R1 makes the test count a CONTRACT: every Zig `test{}` block must have a Rust
`#[test]` counterpart. A module where rust_count < zig_count has DROPPED tests
(the vt-12 Terminal -178 / Screen -12 regressions). This gate is the backstop the
per-batch scoped-G3b gate could not provide (G3b counts distinctive *comments*; a
dropped test with non-distinctive comments slips through — only the COUNT catches it).

rust >= zig is allowed (Rust adds rustty-only helper tests: tracking-allocator,
benches, zeroed-buffer, etc.). rust < zig is RED.

Usage: check_test_counts.py            # audit all mapped rustty-vt modules
       check_test_counts.py <module>   # one module
Exit 1 if any mapped module has rust < zig (minus documented apc-deferred).
"""
import re, sys, glob, os

GH = "/Users/gael/projects/ghostty/src/terminal"
RS = "/Users/gael/projects/rustty/crates/rustty-vt/src"

# rustty-font: GH base differs (src/font, not src/terminal).
GHF = "/Users/gael/projects/ghostty/src/font"
RSF = "/Users/gael/projects/rustty/crates/rustty-font/src"

# rustty-font-shaper: the D1 3-tier shaper crate (ABOVE vt+font). Same GH base
# (src/font) as rustty-font, but the impure C-lib/CoreText face+discovery+
# collection tier (freetype/coretext faces, Collection, DeferredFace,
# CodepointResolver, discovery) — the FFI surface rustty-font (pure) must never
# pull in. Face backends are cfg/feature matrixed (macOS default=coretext,
# Linux default=fontconfig_freetype); the fontconfig discoverer + web_canvas
# face are DEFERRED (documented in DEFERRALS.md), ported as SkipZigTest-style
# skip-stub #[test]s so the count still holds.
RSFS = "/Users/gael/projects/rustty/crates/rustty-font-shaper/src"

# rustty-args: GH base differs (src/cli, not src/terminal). Stage C.
GHC = "/Users/gael/projects/ghostty/src/cli"
RSA = "/Users/gael/projects/rustty/crates/rustty-args/src"

# rustty-terminfo: GH base differs (src/terminfo, not src/terminal). Stage C.
GHT = "/Users/gael/projects/ghostty/src/terminfo"
RST = "/Users/gael/projects/rustty/crates/rustty-terminfo/src"

# rustty-cfg-fmt: GH base = src/config (config/formatter.zig). Near-leaf below
# config<->input (formatter.zig's key.zig import is DEAD code; Key lives with
# Config later). Distinct from the EXISTING `formatter` module above, which maps
# the much larger src/terminal/formatter.zig (the Page/Screen/Terminal debug
# formatters in rustty-vt).
GHCFG = "/Users/gael/projects/ghostty/src/config"
RSCF = "/Users/gael/projects/rustty/crates/rustty-cfg-fmt/src"

# rustty-config: C1 leaf value-types (same GH base = src/config as GHCFG above).
RSCG = "/Users/gael/projects/rustty/crates/rustty-config/src"

# rustty-input: GH base = src/input. The vt input slice (mouse/paste/key/
# key_encode/mouse_encode/function_keys + the VT-Mods half of key_mods) already
# lives in rustty-vt (mapped above as input_*). rustty-input is the REST of
# src/input/: Binding/command/config/keycodes/KeymapDarwin/KeymapNoop/Link/
# keyboard/helpgen_actions (+ the config-dependent RemapSet/translation half of
# key_mods, counted by the existing input_key_mods entry above).
GHI = "/Users/gael/projects/ghostty/src/input"
RSI = "/Users/gael/projects/rustty/crates/rustty-input/src"

# rustty-renderer: the R-a headless renderer slice (heavy-tier, R-a of memory
# heavy-tier-strategy). GH base = src/renderer. The unit-portable, GPU-free
# modules: cell.zig (per-cell CellText fg glyph + CellBg vertex assembly from the
# Screen + font SharedGrid), cursor.zig (cursor rects/style), State.zig (the
# renderer State/mailbox), and size.zig (GridSize/ScreenSize/CellSize/padding).
# size.zig already landed in the rustty-render-types leaf (the A1 hoist) and is
# re-exported by rustty-renderer, so it maps to the render-types file. link.zig
# (link/hover highlight rects) is DEFERRED behind Oniguruma (oni.Regex is not yet
# ported anywhere; mirrors the config url-regex + input Link.zig deferrals —
# DEFERRALS.md), so apc_deferred=3 -> need=0. The GPU draw path (Metal/OpenGL/
# generic/image/custom shaders) is R-d/Phase 3.
GHR = "/Users/gael/projects/ghostty/src/renderer"
RSR = "/Users/gael/projects/rustty/crates/rustty-renderer/src"
RSRT = "/Users/gael/projects/rustty/crates/rustty-render-types/src"

# rustty-vt-c: the C-ABI projection crate (Stage B). GH base = src/terminal/c
# (the `c/*.zig` ABI wrappers behind lib_vt.zig's @export lines). All test bodies
# live in the rustty-vt-c crate (`crates/rustty-vt-c/src/**/*.rs`, incl. the
# render/ + selection_gesture/ test submodules). This is an AGGREGATE mapping
# (28 c/*.zig ref files vs the whole vt-c src tree) because the C wrappers are one
# tightly-coupled ABI surface; the per-crate `cargo test -p rustty-vt-c` count is
# the authoritative backstop and this gate guards rust #[test] >= zig test{}.
GHVC = "/Users/gael/projects/ghostty/src/terminal/c"
RSVC = "/Users/gael/projects/rustty/crates/rustty-vt-c/src"

# module -> (list of zig ref globs, list of rust globs, apc_deferred_count)
MAP = {
    "pagelist":        ([f"{GH}/PageList.zig"], [f"{RS}/pagelist/*.rs"], 0),
    "page":            ([f"{GH}/page.zig"], [f"{RS}/page_impl.rs", f"{RS}/page_tests.rs", f"{RS}/page.rs"], 0),
    "screen":          ([f"{GH}/Screen.zig"], [f"{RS}/screen.rs"], 0),
    "selection":       ([f"{GH}/Selection.zig"], [f"{RS}/selection.rs"], 0),
    "selection_gesture":([f"{GH}/SelectionGesture.zig"], [f"{RS}/selection_gesture.rs"], 0),
    "render":          ([f"{GH}/render.zig"], [f"{RS}/render.rs"], 0),
    "string_map":      ([f"{GH}/StringMap.zig"], [f"{RS}/string_map.rs"], 0),
    "screen_set":      ([f"{GH}/ScreenSet.zig"], [f"{RS}/screen_set.rs"], 0),
    "formatter":       ([f"{GH}/formatter.zig"], [f"{RS}/formatter.rs"], 0),
    "terminal":        ([f"{GH}/Terminal.zig"], [f"{RS}/terminal.rs"], 0),
    "stream_terminal": ([f"{GH}/stream_terminal.zig"], [f"{RS}/stream_terminal.rs"], 0),  # apc now wired (font-decode mandatory, Stage A)
    # Stage A — apc multiplexer + glyph subprotocol (apc.zig + apc/glyph/{request,Glossary,execute,response}.zig).
    # Lives in rustty-vt and deps rustty-font DIRECTLY (glyf-decode mandatory, NOT a runtime hook).
    "apc":             ([f"{GH}/apc.zig", f"{GH}/apc/glyph/request.zig", f"{GH}/apc/glyph/Glossary.zig", f"{GH}/apc/glyph/execute.zig", f"{GH}/apc/glyph/response.zig"], [f"{RS}/apc/**/*.rs"], 0),
    "osc":             ([f"{GH}/osc.zig", f"{GH}/osc/encoding.zig", f"{GH}/osc/parsers.zig"] + glob.glob(f"{GH}/osc/parsers/*.zig"), [f"{RS}/osc/**/*.rs"], 0),
    "parser":          ([f"{GH}/Parser.zig", f"{GH}/parse_table.zig"], [f"{RS}/parser.rs", f"{RS}/parse_table.rs"], 0),
    "stream":          ([f"{GH}/stream.zig"], [f"{RS}/stream.rs"], 0),
    "kitty":           ([f"{GH}/kitty.zig"] + glob.glob(f"{GH}/kitty/*.zig"), [f"{RS}/kitty/*.rs"], 0),
    "style":           ([f"{GH}/style.zig"], [f"{RS}/style.rs"], 0),
    "sgr":             ([f"{GH}/sgr.zig"], [f"{RS}/sgr.rs"], 0),
    "color":           ([f"{GH}/color.zig"], [f"{RS}/color.rs"], 0),
    "hash_map":        ([f"{GH}/hash_map.zig"], [f"{RS}/hash_map.rs"], 0),
    "bitmap_allocator":([f"{GH}/bitmap_allocator.zig"], [f"{RS}/bitmap_allocator.rs"], 0),
    "dcs":             ([f"{GH}/dcs.zig"], [f"{RS}/dcs.rs"], 0),
    "modes":           ([f"{GH}/modes.zig"], [f"{RS}/modes.rs"], 0),
    "mouse":           ([f"{GH}/mouse.zig"], [f"{RS}/mouse.rs"], 0),
    "tabstops":        ([f"{GH}/Tabstops.zig"], [f"{RS}/tabstops.rs"], 0),
    "utf8_decoder":    ([f"{GH}/UTF8Decoder.zig"], [f"{RS}/UTF8Decoder.zig" if False else f"{RS}/utf8_decoder.rs"], 0),
    # vt-14 input slice (src/input/, pulled into vt)
    "input_mouse":       (["/Users/gael/projects/ghostty/src/input/mouse.zig"], [f"{RS}/input/mouse.rs"], 0),
    "input_paste":       (["/Users/gael/projects/ghostty/src/input/paste.zig"], [f"{RS}/input/paste.rs"], 0),
    "input_key":         (["/Users/gael/projects/ghostty/src/input/key.zig"], [f"{RS}/input/key.rs"], 0),
    "input_mouse_encode":(["/Users/gael/projects/ghostty/src/input/mouse_encode.zig"], [f"{RS}/input/mouse_encode.rs"], 0),
    "input_key_encode":  (["/Users/gael/projects/ghostty/src/input/key_encode.zig"], [f"{RS}/input/key_encode.rs"], 0),
    "input_function_keys":(["/Users/gael/projects/ghostty/src/input/function_keys.zig"], [f"{RS}/input/function_keys.rs"], 0),
    # key_mods.zig is split across two crates: the VT-Mods slice (Mods packed
    # struct + VT methods + bit-layout test) in rustty-vt's input/key_mods.rs,
    # and the config-dependent RemapSet/translation slice (the 23 RemapSet: +
    # translation-macos-option-as-alt tests) in rustty-input's key_mods.rs. The
    # 23 are no longer deferred — they landed once OptionAsAlt + cfg-fmt became
    # available — so apc_deferred is now 0 and BOTH rust files count against the
    # one zig file (vt=1 anon test + input=23 -> 24 >= 23).
    "input_key_mods":    (["/Users/gael/projects/ghostty/src/input/key_mods.zig"], [f"{RS}/input/key_mods.rs", "/Users/gael/projects/rustty/crates/rustty-input/src/key_mods.rs"], 0),
    # vt-15 search/ (gated)
    "search":            ([f"{GH}/search/Thread.zig", f"{GH}/search/active.zig", f"{GH}/search/pagelist.zig", f"{GH}/search/screen.zig", f"{GH}/search/sliding_window.zig", f"{GH}/search/viewport.zig"], [f"{RS}/search/*.rs"], 0),
    # vt-16 tmux/ (control mode; oniguruma-gated control tests counted via static
    # #[test] text — the gate matches `#[test]` source lines regardless of cfg).
    # All four tmux modules are ported: control + layout + output parsers + viewer
    # (the control-mode multiplexer driving per-pane Terminals). This completes
    # Ghostty terminal/ in rustty-vt.
    "tmux":              ([f"{GH}/tmux/control.zig", f"{GH}/tmux/layout.zig", f"{GH}/tmux/output.zig", f"{GH}/tmux/viewer.zig"], [f"{RS}/tmux/control.rs", f"{RS}/tmux/layout.rs", f"{RS}/tmux/output.rs", f"{RS}/tmux/viewer.rs"], 0),
    # Stage A.2 — rustty-font pure-parsing slice (GH base = src/font, not src/terminal).
    # NOTE: zig_count() matches `test "name"`; sfnt.zig also has one unquoted
    # `test FixedPoint {` (caught by Rust's 3rd #[test] — rust >= zig holds).
    "font_glyf":         ([f"{GHF}/opentype/glyf.zig"], [f"{RSF}/opentype/glyf.rs"], 0),
    "font_sfnt":         ([f"{GHF}/opentype/sfnt.zig"], [f"{RSF}/opentype/sfnt.rs"], 0),
    "font_head":         ([f"{GHF}/opentype/head.zig"], [f"{RSF}/opentype/head.rs"], 0),
    "font_metrics":      ([f"{GHF}/Metrics.zig"], [f"{RSF}/metrics.rs"], 0),
    # Stage F0 — rustty-font heavy-tier base: Atlas.zig (texture-atlas rect packing,
    # 13 quoted tests) + CodepointMap.zig (codepoint->descriptor map, 1 test).
    # apc_deferred=1 for font_atlas: the indented `test "happy path"` (Atlas.zig:564)
    # lives INSIDE the wasm `Wasm` struct (the os/wasm + zig-js C-ABI/JS-canvas
    # surface), feature-gated OFF in pure rustty-font (no os/wasm infra, R4). It is
    # preserved verbatim as a commented reference in atlas.rs (mirrors the config/vt
    # wasm-export deferrals; DEFERRALS.md). So need = 13-1 = 12; rust = 12 live
    # #[test] (the 12 non-wasm Atlas tests). CodepointMap: 1 quoted test
    # ("codepointmap") -> codepoint_map.rs; the discovery.Descriptor edge is SEAMed
    # as a thin local `Descriptor` (family-only) — no C-lib/discovery edge pulled
    # into pure rustty-font (the real discovery surface lands in the F3 shaper tier).
    "font_atlas":        ([f"{GHF}/Atlas.zig"], [f"{RSF}/atlas.rs"], 1),
    "font_codepoint_map":([f"{GHF}/CodepointMap.zig"], [f"{RSF}/codepoint_map.rs"], 0),
    # Stage F0.1 — glyf_rasterize.zig (the z2d-backed glyf-outline rasterizer:
    # flatten contours -> z2d fill -> alpha8 bitmap, with cell-constraint /
    # design-metric placement). 13 quoted tests in glyf_rasterize.zig +
    # glyf_rasterize_png_test.zig's 1 `test "..."` (the bubbletea known-answer
    # PNG compare, decoded via the pure-Rust `png` dev-dep) = 14 zig. Rust:
    # glyf_rasterize.rs (13 mapped raster tests + 1 `reference_import_*` marker
    # mirroring the reference `test { _ = @import(png_test) }` inclusion) +
    # glyf_rasterize/glyf_rasterize_png_test.rs (1) = 15 -> 15 >= 14.
    "font_glyf_rasterize":([f"{GHF}/glyf_rasterize.zig", f"{GHF}/glyf_rasterize_png_test.zig"], [f"{RSF}/glyf_rasterize.rs", f"{RSF}/glyf_rasterize/glyf_rasterize_png_test.rs"], 0),
    # Stage F0 sprite — src/font/sprite (GHF base = src/font). The sprite.zig
    # module root (Sprite enum) -> sprite/mod.rs; canvas.zig (the z2d-backed draw
    # Canvas) -> sprite/canvas.rs; and the per-glyph draw/ modules. The draw
    # modules are 0-test BY DESIGN — their contract is the G3b comment-coverage
    # gate (check_comment_coverage.py), not test-count — so most rows are 0 vs the
    # rustty helper-test margin. The ONLY quoted sprite test is Face.zig's "sprite
    # face render all sprites" PNG known-answer (Face.zig:513), now LANDED in
    # sprite/face.rs along with the sprite/testdata PNG suite it decodes (pixel-
    # exact vs res/*.png). face.rs adds two helper #[test]s (ranges_have_no_overlap
    # — the reference's comptime overlap assertion; has_codepoint_and_blank_glyph —
    # the refAllDecls smoke), so rust=3 >= zig=1 (apc_deferred=0). branch.zig/
    # special.zig/symbols_for_legacy_computing(+supplement).zig are 0-test by design
    # (tracked by G3b), so not mapped here.
    "font_sprite":               ([f"{GHF}/sprite.zig"], [f"{RSF}/sprite/mod.rs"], 0),
    "font_sprite_canvas":        ([f"{GHF}/sprite/canvas.zig"], [f"{RSF}/sprite/canvas.rs"], 0),
    "font_sprite_face":          ([f"{GHF}/sprite/Face.zig"], [f"{RSF}/sprite/face.rs"], 0),
    "font_sprite_draw_block":    ([f"{GHF}/sprite/draw/block.zig"], [f"{RSF}/sprite/draw/block.rs"], 0),
    "font_sprite_draw_box":      ([f"{GHF}/sprite/draw/box.zig"], [f"{RSF}/sprite/draw/box.rs"], 0),
    "font_sprite_draw_braille":  ([f"{GHF}/sprite/draw/braille.zig"], [f"{RSF}/sprite/draw/braille.rs"], 0),
    "font_sprite_draw_common":   ([f"{GHF}/sprite/draw/common.zig"], [f"{RSF}/sprite/draw/common.rs"], 0),
    "font_sprite_draw_geometric_shapes":([f"{GHF}/sprite/draw/geometric_shapes.zig"], [f"{RSF}/sprite/draw/geometric_shapes.rs"], 0),
    "font_sprite_draw_powerline":([f"{GHF}/sprite/draw/powerline.zig"], [f"{RSF}/sprite/draw/powerline.rs"], 0),
    # Stage C — rustty-args CLI arg parser (GH base = src/cli, not src/terminal).
    # args.zig (36t) tests live in src/args/tests.rs (module include from args.rs).
    # diagnostics.zig has 0 tests (ported as a sibling required by args.zig).
    "args":              ([f"{GHC}/args.zig"], [f"{RSA}/args.rs", f"{RSA}/args/tests.rs"], 0),
    "comma_splitter":    ([f"{GHC}/CommaSplitter.zig"], [f"{RSA}/comma_splitter.rs"], 0),
    "cli_diagnostics":   ([f"{GHC}/diagnostics.zig"], [f"{RSA}/diagnostics.rs"], 0),
    # Stage C — rustty-terminfo (GH base = src/terminfo, not src/terminal).
    # Source.zig (2t: "xtgettcap map" + "encode") -> source.rs.
    # ghostty.zig (1t: "encode") -> entry.rs (the terminfo entry; names carry
    # the rustty R2 identity via rustty-identity, table ported verbatim).
    # main.zig has one anonymous `test { refAllDecls }` (NOT counted by zig_count,
    # which skips anonymous `test {`); its Rust analogue (lib.rs ref_all_decls)
    # is a +1 helper margin. terminfo total: zig=3 counted, rust=4 -> rust >= zig.
    "terminfo_source":   ([f"{GHT}/Source.zig"], [f"{RST}/source.rs"], 0),
    "terminfo_entry":    ([f"{GHT}/ghostty.zig"], [f"{RST}/entry.rs"], 0),
    "terminfo_main":     ([f"{GHT}/main.zig"], [f"{RST}/lib.rs"], 0),
    # Stage B — rustty-vt-c C-ABI projection. AGGREGATE: all c/*.zig (the 28 ABI
    # wrapper files; main.zig/result.zig/color.zig carry 0 behavioral tests) vs
    # the whole vt-c src tree (incl. render/ + selection_gesture/ test submods).
    "vt_c":              (glob.glob(f"{GHVC}/*.zig"), [f"{RSVC}/**/*.rs"], 0),
    # rustty-cfg-fmt — config/formatter.zig (8 quoted tests). The reference's
    # comptime formatEntry reflection -> the FormatEntry trait pattern; tests live
    # in formatter.rs. writer.rs/lib.rs carry 0 behavioral tests.
    "cfg_fmt_formatter": ([f"{GHCFG}/formatter.zig"], [f"{RSCF}/formatter.rs", f"{RSCF}/writer.rs", f"{RSCF}/lib.rs"], 0),
    # rustty-input — the non-vt half of src/input/.
    # Binding.zig: 85 quoted tests (the keybind grammar trigger>action parse).
    "input_binding":     ([f"{GHI}/Binding.zig"], [f"{RSI}/binding.rs"], 0),
    # command.zig: 1 quoted test.
    "input_command":     ([f"{GHI}/command.zig"], [f"{RSI}/command.rs"], 0),
    # config.zig: 0 tests (OptionAsAlt enum only).
    "input_config":      ([f"{GHI}/config.zig"], [f"{RSI}/config.rs"], 0),
    # keycodes.zig: 0 quoted (1 anon refAllDecls, not counted); rust +1 helper.
    "input_keycodes":    ([f"{GHI}/keycodes.zig"], [f"{RSI}/keycodes.rs"], 0),
    # KeymapDarwin.zig: 1 quoted test (+1 anon); rust=2 (>= zig).
    "input_keymap_darwin":([f"{GHI}/KeymapDarwin.zig"], [f"{RSI}/keymap_darwin.rs"], 0),
    # KeymapNoop.zig: 0 tests (non-macOS noop keymap).
    "input_keymap_noop": ([f"{GHI}/KeymapNoop.zig"], [f"{RSI}/keymap_noop.rs"], 0),
    # Link.zig: 0 tests (oniguruma-backed regex link).
    "input_link":        ([f"{GHI}/Link.zig"], [f"{RSI}/link.rs"], 0),
    # keyboard.zig: 0 tests (KeyboardLayout enum + OptionAsAlt glue).
    "input_keyboard":    ([f"{GHI}/keyboard.zig"], [f"{RSI}/keyboard.rs"], 0),
    # helpgen_actions.zig: 0 tests (build-time docs generator).
    "input_helpgen_actions":([f"{GHI}/helpgen_actions.zig"], [f"{RSI}/helpgen_actions.rs"], 0),
    # rustty-config C1 — leaf value-types the giant Config struct (C2, later)
    # composes. GH base = src/config (GHCFG, reused from cfg-fmt above). These are
    # Config-INDEPENDENT (string/path/io/command/url/... value-types). The
    # formatter.zig of src/config is mapped separately as cfg_fmt_formatter (a
    # different crate, rustty-cfg-fmt); Config.zig itself is C2 (deferred).
    "config_clipboard_codepoint_map":([f"{GHCFG}/ClipboardCodepointMap.zig"], [f"{RSCG}/clipboard_codepoint_map.rs"], 0),
    # command.zig: 9 quoted tests (+1 anon refAllDecls, not counted); rust=11 (>= zig).
    "config_command":    ([f"{GHCFG}/command.zig"], [f"{RSCG}/command.rs"], 0),
    # conditional.zig: 1 quoted test.
    "config_conditional":([f"{GHCFG}/conditional.zig"], [f"{RSCG}/conditional.rs"], 0),
    # edit.zig: 0 tests (config-file edit path resolution).
    "config_edit":       ([f"{GHCFG}/edit.zig"], [f"{RSCG}/edit.rs"], 0),
    # ErrorList.zig: 0 tests (diagnostic accumulator).
    "config_error_list": ([f"{GHCFG}/ErrorList.zig"], [f"{RSCG}/error_list.rs"], 0),
    # file_load.zig: 0 tests (config-file load helper).
    "config_file_load":  ([f"{GHCFG}/file_load.zig"], [f"{RSCG}/file_load.rs"], 0),
    # io.zig: 3 quoted tests (+1 anon); rust=5 (>= zig).
    "config_io":         ([f"{GHCFG}/io.zig"], [f"{RSCG}/io.rs"], 0),
    # path.zig: 8 quoted tests (path expansion / homedir).
    "config_path":       ([f"{GHCFG}/path.zig"], [f"{RSCG}/path.rs"], 0),
    # RepeatableStringMap.zig: 4 quoted tests.
    "config_repeatable_string_map":([f"{GHCFG}/RepeatableStringMap.zig"], [f"{RSCG}/repeatable_string_map.rs"], 0),
    # string.zig: 9 quoted tests (RepeatableString / parse).
    "config_string":     ([f"{GHCFG}/string.zig"], [f"{RSCG}/string.rs"], 0),
    # url.zig: 1 quoted test ("url regex"), oniguruma-backed -> DEFERRED behind the
    # off-by-default `oniguruma` cargo feature (SkipZigTest-style; documented in
    # .port/phase-2/DEFERRALS.md). The regex string-assembly logic is ported
    # normally; the test slot is `#[cfg(all(test, feature="oniguruma"))]`. So
    # apc_deferred=1 -> need=0, rust=0 (the gated test has no live #[test] line).
    "config_url":        ([f"{GHCFG}/url.zig"], [f"{RSCG}/url.rs"], 1),
    # rustty-config C2 — the giant `Config` struct of Config.zig (113 quoted
    # tests). The Config-struct integration tests live in config_struct.rs; the
    # in-file value-type tests (the `pub const X = enum/struct/union` types the
    # Config struct composes, each with its own `test{}`) live in value_types.rs.
    # BOTH map against Config.zig (the value types are defined inside Config.zig).
    # apc_deferred=0: the four `RepeatableCodepointMap` tests ("parseCLI",
    # "formatConfig single/range/multiple", Config.zig:8160-8224) are now ported in
    # value_types.rs — font/CodepointMap.zig landed PURE in rustty-font (its
    # discovery-descriptor seam carries `family` + `monospace`; see DEFERRALS.md).
    # The 5 `RepeatableClipboardCodepointMap` tests (8352-8415) are also ported
    # (ClipboardCodepointMap is a landed C1 leaf type). need = 113; rust
    # = config_struct.rs + value_types.rs (>= 113, with rustty round-trip margin).
    "config_struct":     ([f"{GHCFG}/Config.zig"], [f"{RSCG}/config_struct.rs", f"{RSCG}/value_types.rs"], 0),
    # rustty-config C2 — key.zig (the `Key` enum derived FROM the Config fields).
    # 1 quoted test ("Value", key.zig:52), ported in config_struct.rs (the `value`
    # test) alongside the `Key` enum it exercises.
    "config_key":        ([f"{GHCFG}/key.zig"], [f"{RSCG}/config_struct.rs"], 0),
    # rustty-config C2 — theme.zig (0 behavioral tests: theme path resolution
    # using the REAL rustty-os ResourcesDir — see DEFERRALS.md, no .resources
    # deferral). Mapped for completeness/G3b tracking; 0 >= 0.
    "config_theme":      ([f"{GHCFG}/theme.zig"], [f"{RSCG}/theme.rs"], 0),
    # rustty-config C3 — CApi.zig (the config C-ABI: ghostty_config_new/free/
    # clone/load_*/finalize/get/trigger/diagnostics_count/get_diagnostic/
    # open_path). 8 quoted tests (the ghostty_config_get bool/enum/optional/
    # unknown-key/optional-string/float/struct-cval round-trips + the
    # ghostty_config_trigger default-keybind). Ported in c_api.rs as the rustty_
    # config_* exports (R2 identity). CApi stays IN rustty-config (not a -c crate).
    "config_capi":       ([f"{GHCFG}/CApi.zig"], [f"{RSCG}/c_api.rs"], 0),
    # rustty-config C3 — c_get.zig (the typed config_get over the Key registry:
    # the comptime fieldByKey + getValue dispatch). 6 quoted tests (u8/enum/color/
    # optional/background-blur/split-preserve-zoom). Ported in c_get.rs.
    "config_c_get":      ([f"{GHCFG}/c_get.zig"], [f"{RSCG}/c_get.rs"], 0),
    # rustty-config — formatter_file.zig (the FileFormatter: emits the default
    # config file with `# ` doc-comments). 2 quoted tests ("format default
    # config" + "format default config changed"). help_strings docs deferral is
    # documented; the two behavioral tests are ported in formatter_file.rs.
    "config_formatter_file":([f"{GHCFG}/formatter_file.zig"], [f"{RSCG}/formatter_file.rs"], 0),
    # rustty-config C3 — Wasm.zig (0 behavioral tests: browser-gated wasm exports,
    # feature-gated OFF — see DEFERRALS.md, mirrors the vt wasm-export deferral).
    # Mapped for completeness/G3b tracking; 0 >= 0.
    "config_wasm":       ([f"{GHCFG}/Wasm.zig"], [f"{RSCG}/wasm.rs"], 0),
    # rustty-font-shaper F3 — the impure face/discovery/collection tier (D1 split).
    # face/freetype.zig: 8 quoted tests (the FreeType Face render/metrics/glyph
    # over freetype-sys). Rust=9 (>= zig, +1 rustty helper margin).
    "font_shaper_face_freetype": ([f"{GHF}/face/freetype.zig"], [f"{RSFS}/face/freetype.rs"], 0),
    # face/coretext.zig: 7 quoted tests (the CoreText Face over objc2-core-text).
    # macOS-only backend; Rust=8 (>= zig).
    "font_shaper_face_coretext": ([f"{GHF}/face/coretext.zig"], [f"{RSFS}/face/coretext.rs"], 0),
    # face.zig: 2 quoted tests (the backend-selecting Face facade). Rust=3 (>= zig).
    # web_canvas.zig face is DEFERRED (wasm-only, feature-gated OFF; DEFERRALS.md).
    "font_shaper_face":          ([f"{GHF}/face.zig"], [f"{RSFS}/face/mod.rs"], 0),
    # Collection.zig: 6 quoted + 6 named-unquoted behavioral tests (init/getFace/
    # getIndex/completeStyles/setSize/hasCodepoint) = 12 zig. Rust=13 (>= zig).
    "font_shaper_collection":    ([f"{GHF}/Collection.zig"], [f"{RSFS}/collection.rs"], 0),
    # DeferredFace.zig: 2 quoted tests (the lazy face loader). Rust=2 (>= zig).
    "font_shaper_deferred_face": ([f"{GHF}/DeferredFace.zig"], [f"{RSFS}/deferred_face.rs"], 0),
    # CodepointResolver.zig: 2 quoted + 1 named-unquoted (getIndex) = 3 zig.
    # Rust=3 (>= zig).
    "font_shaper_codepoint_resolver":([f"{GHF}/CodepointResolver.zig"], [f"{RSFS}/codepoint_resolver.rs"], 0),
    # discovery.zig: 8 quoted tests (descriptor hash x2 + fontconfig x2 + coretext
    # x3 + windows). The fontconfig (Linux) + windows (DirectWrite) tests are
    # DEFERRED with their backends (DEFERRALS.md) but ported as SkipZigTest-style
    # skip-stub #[test]s (the reference `!= backend -> SkipZigTest` early-return),
    # so rust=8 live #[test] == zig=8 (apc_deferred=0). The coretext discovery
    # tests are #[cfg(target_os = "macos")] (the macOS default backend).
    "font_shaper_discovery":     ([f"{GHF}/discovery.zig"], [f"{RSFS}/discovery.rs"], 0),
    # rustty-font-shaper F4 — the shaper framework + backends (D1 top tier). The
    # framework files (shape/feature/Cache/noop/SharedGrid/SharedGridSet) landed;
    # the HarfBuzz + CoreText *shaping backends* (the hb_buffer/hb_shape + CTLine/
    # CTRun run producers, 24 + 27 behavioral tests) are the core deliverable.
    # shape.zig: anon `test {` refify -> zig_count=0; shape.rs=1 (>= zig).
    "font_shaper_shape":         ([f"{GHF}/shape.zig"], [f"{RSFS}/shape.rs"], 0),
    # shaper/harfbuzz.zig: 24 quoted shaping tests -> shaper/harfbuzz.rs (the
    # `test { ... }` blocks live in the sibling shaper/harfbuzz_tests.rs, attached
    # as the `tests` submodule — same split as the `page` module's page_tests.rs).
    "font_shaper_harfbuzz":      ([f"{GHF}/shaper/harfbuzz.zig"], [f"{RSFS}/shaper/harfbuzz.rs", f"{RSFS}/shaper/harfbuzz_tests.rs"], 0),
    # shaper/coretext.zig: 27 quoted shaping tests -> shaper/coretext.rs (the
    # `test { ... }` blocks live in the sibling shaper/coretext_tests.rs, attached
    # as the `tests` submodule — same split as shaper/harfbuzz_tests.rs).
    "font_shaper_coretext":      ([f"{GHF}/shaper/coretext.zig"], [f"{RSFS}/shaper/coretext.rs", f"{RSFS}/shaper/coretext_tests.rs"], 0),
    # shaper/feature.zig: 2 quoted tests -> shaper/feature.rs.
    "font_shaper_feature":       ([f"{GHF}/shaper/feature.zig"], [f"{RSFS}/shaper/feature.rs"], 0),
    # shaper/Cache.zig: 1 named-unquoted behavioral test -> shaper/cache.rs.
    "font_shaper_cache":         ([f"{GHF}/shaper/Cache.zig"], [f"{RSFS}/shaper/cache.rs"], 0),
    # shaper/noop.zig: anon refAllDecls -> zig_count=0; shaper/noop.rs=1 (>= zig).
    "font_shaper_noop":          ([f"{GHF}/shaper/noop.zig"], [f"{RSFS}/shaper/noop.rs"], 0),
    # SharedGrid.zig: getIndex (named) + 2 quoted = 3 -> shared_grid.rs.
    "font_shaper_shared_grid":   ([f"{GHF}/SharedGrid.zig"], [f"{RSFS}/shared_grid.rs"], 0),
    # SharedGridSet.zig: SharedGridSet (named) + 3 quoted = 4 -> shared_grid_set.rs.
    "font_shaper_shared_grid_set":([f"{GHF}/SharedGridSet.zig"], [f"{RSFS}/shared_grid_set.rs"], 0),
    # rustty-renderer R-a — the headless renderer slice (heavy-tier R-a).
    # cell.zig: 4 quoted + 1 named-unquoted (`test Contents {`, real body) = 5 zig
    # -> cell.rs (the CellText fg-glyph + CellBg per-cell vertex assembly). rust=5.
    "renderer_cell":     ([f"{GHR}/cell.zig"], [f"{RSR}/cell.rs"], 0),
    # cursor.zig: 4 quoted tests (the renderer Style + style() cursor rects) ->
    # cursor.rs. rust=4.
    "renderer_cursor":   ([f"{GHR}/cursor.zig"], [f"{RSR}/cursor.rs"], 0),
    # State.zig: 2 quoted tests (the render State/Preedit) -> state.rs. rust=2.
    "renderer_state":    ([f"{GHR}/State.zig"], [f"{RSR}/state.rs"], 0),
    # size.zig: 6 quoted tests (Size.balancePadding x2 + Padding balanced + GridSize
    # update x2 + coordinate conversion). Already ported in the rustty-render-types
    # leaf (A1 hoist) and re-exported by rustty-renderer; maps to render-types
    # lib.rs (the 6 size.zig tests). rust=6.
    "renderer_size":     ([f"{GHR}/size.zig"], [f"{RSRT}/lib.rs"], 0),
    # link.zig: 3 quoted tests -> DEFERRED behind Oniguruma (Set.fromConfig compiles
    # an oni.Regex; renderCellMap runs regex.search — no honest port without the
    # binding, which is unported tree-wide; mirrors config url-regex + input Link
    # deferrals, DEFERRALS.md). apc_deferred=3 -> need=0, rust=0 (no link.rs yet).
    "renderer_link":     ([f"{GHR}/link.zig"], [], 3),
}

def expand(globs):
    out = []
    for g in globs:
        out += glob.glob(g, recursive=True) if ("*" in g) else ([g] if os.path.exists(g) else [])
    return out

def zig_count(files):
    # Counts BEHAVIORAL Zig tests: every `test "..."` (quoted), PLUS named-unquoted
    # `test Ident {` whose body is a real assertion (NOT the `_ = X;`/refAllDecls
    # reify idiom). Anonymous `test {` are skipped (≈always refAllDecls; the rare
    # behavioral one is covered by Rust's helper-test margin). One-time manual audit
    # (2026-06) confirmed every behavioral named-unquoted test — modes.modeFromInt/
    # ModeState, ScreenSet, paste.isSafe, encoding.isSafeUtf8 — is ported; the
    # excluded ones are color.Special/Dynamic (refAllDecls) + stream.Action (C-ABI->vt-c).
    n = 0
    for f in files:
        lines = open(f, errors="ignore").readlines()
        for i, ln in enumerate(lines):
            if re.match(r'\s*test "', ln):
                n += 1
            elif re.match(r'\s*test [A-Za-z_][A-Za-z_0-9]* \{', ln):
                body = ""
                for j in range(i + 1, min(i + 6, len(lines))):
                    s = lines[j].strip()
                    if not s or s.startswith("//"):
                        continue
                    body = s
                    break
                if not (body.startswith("_ = ") or "refAllDecls" in body):
                    n += 1
    return n

def rust_count(files):
    n = 0
    for f in files:
        n += sum(1 for ln in open(f, errors="ignore") if re.search(r'#\[test\]', ln))
    return n

def main():
    only = sys.argv[1] if len(sys.argv) > 1 else None
    bad = 0
    print("module            zig  rust  status")
    for mod, (zg, rg, apc) in MAP.items():
        if only and mod != only:
            continue
        z = zig_count(expand(zg)); r = rust_count(expand(rg))
        need = z - apc
        if r < need:
            print(f"  {mod:16} {z:4} {r:5}  RED  drop={need-r}" + (f" (apc_deferred={apc})" if apc else ""))
            bad += 1
        else:
            print(f"  {mod:16} {z:4} {r:5}  ok " + (f"(apc_deferred={apc})" if apc else ""))
    if bad:
        print(f"\nTEST-COUNT GATE: RED — {bad} module(s) with dropped tests (rust < zig - apc_deferred)")
        sys.exit(1)
    print("\nTEST-COUNT GATE: GREEN — every module has rust #[test] >= zig test{} (minus apc-deferred)")

if __name__ == "__main__":
    main()
