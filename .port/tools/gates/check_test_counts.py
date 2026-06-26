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
