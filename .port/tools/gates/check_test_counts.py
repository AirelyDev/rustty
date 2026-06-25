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
    "stream_terminal": ([f"{GH}/stream_terminal.zig"], [f"{RS}/stream_terminal.rs"], 4),  # apc/glyph/kitty-APC arms font-blocked
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
    "input_key_mods":    (["/Users/gael/projects/ghostty/src/input/key_mods.zig"], [f"{RS}/input/key_mods.rs"], 23),  # 23 RemapSet/translation tests -> config/apprt-seam deferred
    # vt-15 search/ (gated)
    "search":            ([f"{GH}/search/Thread.zig", f"{GH}/search/active.zig", f"{GH}/search/pagelist.zig", f"{GH}/search/screen.zig", f"{GH}/search/sliding_window.zig", f"{GH}/search/viewport.zig"], [f"{RS}/search/*.rs"], 0),
    # vt-16 tmux/ (control mode; oniguruma-gated control tests counted via static
    # #[test] text — the gate matches `#[test]` source lines regardless of cfg).
    # All four tmux modules are ported: control + layout + output parsers + viewer
    # (the control-mode multiplexer driving per-pane Terminals). This completes
    # Ghostty terminal/ in rustty-vt.
    "tmux":              ([f"{GH}/tmux/control.zig", f"{GH}/tmux/layout.zig", f"{GH}/tmux/output.zig", f"{GH}/tmux/viewer.zig"], [f"{RS}/tmux/control.rs", f"{RS}/tmux/layout.rs", f"{RS}/tmux/output.rs", f"{RS}/tmux/viewer.rs"], 0),
}

def expand(globs):
    out = []
    for g in globs:
        out += glob.glob(g, recursive=True) if ("*" in g) else ([g] if os.path.exists(g) else [])
    return out

def zig_count(files):
    n = 0
    for f in files:
        n += sum(1 for ln in open(f, errors="ignore") if re.match(r'\s*test "', ln))
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
