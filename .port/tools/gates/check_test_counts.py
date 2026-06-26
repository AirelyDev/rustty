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
    # apc_deferred=4: the four `RepeatableCodepointMap` tests ("parseCLI",
    # "formatConfig single/range/multiple", Config.zig:8160-8224) are font-blocked
    # (wraps font/CodepointMap.zig, not yet in rustty-font — see DEFERRALS.md).
    # NOTE: the 5 `RepeatableClipboardCodepointMap` tests (8352-8415) ARE ported
    # (ClipboardCodepointMap is a landed C1 leaf type). need = 113-4 = 109; rust
    # = config_struct.rs + value_types.rs (>= 109, with rustty round-trip margin).
    "config_struct":     ([f"{GHCFG}/Config.zig"], [f"{RSCG}/config_struct.rs", f"{RSCG}/value_types.rs"], 4),
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
