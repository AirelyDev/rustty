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
Exit 3 (BLOCKED) if any explicitly listed zig reference file is missing — the
zig side greps EXTERNAL read-only checkouts (the ghostty oracle + the pinned
zig-cache dep packages, R3), and a missing ref would silently count zig=0 and
turn the gate VACUOUSLY green. Never green on that: surface it (ci.sh renders
exit 3 as a WARNING, never a pass).
"""
import re, sys, glob, os

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import _gatelib as G  # noqa: E402  (the one oracle/zig-cache resolver)

# WS19 T1 (N1): the RUST side must follow the CHECKOUT THIS SCRIPT LIVES IN, not a
# hardcoded absolute path. Run from a git worktree, the old hardcodes made the gate
# count `main`'s tests while reporting on the worktree's changes — G1b was VACUOUS for
# every worktree change. Same bug `.port/tools/miri-run.sh` already fixed this way.
# R13-GF3: SCRIPT-RELATIVE (`.port/tools/gates/` is three levels below the root),
# no longer `git rev-parse --show-toplevel` — that honours GIT_DIR + GIT_WORK_TREE,
# so the environment could make the gate count another tree's tests (R5; the fix
# G-CENSUS got in R13-GF2). `--self-test` proves it.
GATES_DIR = os.path.dirname(os.path.abspath(__file__))
RS_ROOT = os.path.dirname(os.path.dirname(os.path.dirname(GATES_DIR)))

# The ORACLE root is DERIVED, not hardcoded (N483), and stays NON-overridable (R3: the
# oracle is an EXTERNAL reference, never shipped to CI): `_gatelib.oracle_root()` is
# `ghostty` beside the MAIN checkout (the same rule as `_oracle_root.sh`), with no
# environment override — `.port/tools/ci.sh:114-129` records why an env-redirectable
# oracle probe was killed as a spoof hole (probe passes against a fake dir while the
# script counts zig=0 everywhere -> vacuous green, R5). A missing or off-pin oracle is
# BLOCKED in `main()` (`_gatelib.oracle_pin_problem`). The zig package cache resolves the
# way zig does (`_gatelib.zig_cache_root`); a package dir that is absent is BLOCKED by
# the missing-reference guard in `main()`.
GH_ROOT = G.oracle_root()
ZIG_P = os.path.join(G.zig_cache_root(), "p")

GH = f"{GH_ROOT}/src/terminal"
RS = f"{RS_ROOT}/crates/rustty-vt/src"

# rustty-font: GH base differs (src/font, not src/terminal).
GHF = f"{GH_ROOT}/src/font"
RSF = f"{RS_ROOT}/crates/rustty-font/src"

# rustty-font-shaper: the D1 3-tier shaper crate (ABOVE vt+font). Same GH base
# (src/font) as rustty-font, but the impure C-lib/CoreText face+discovery+
# collection tier (freetype/coretext faces, Collection, DeferredFace,
# CodepointResolver, discovery) — the FFI surface rustty-font (pure) must never
# pull in. Face backends are cfg/feature matrixed (macOS default=coretext,
# Linux default=fontconfig_freetype); the fontconfig discoverer + web_canvas
# face are DEFERRED (documented in DEFERRALS.md), ported as SkipZigTest-style
# skip-stub #[test]s so the count still holds.
RSFS = f"{RS_ROOT}/crates/rustty-font-shaper/src"

# rustty-args: GH base differs (src/cli, not src/terminal). Stage C.
GHC = f"{GH_ROOT}/src/cli"
RSA = f"{RS_ROOT}/crates/rustty-args/src"

# rustty-cli: GH base = src/cli (same GHC) + src/cli.zig. Phase-2 CLI milestone.
RSCLI = f"{RS_ROOT}/crates/rustty-cli/src"

# rustty-extra: GH base = src/extra (shell completions + editor syntax). The five
# comptime generators; only fish.zig (`getDescription`) + vim.zig (anon `test {}`,
# skipped by zig_count) carry tests.
GHX = f"{GH_ROOT}/src/extra"
RSX = f"{RS_ROOT}/crates/rustty-extra/src"

# rustty-terminfo: GH base differs (src/terminfo, not src/terminal). Stage C.
GHT = f"{GH_ROOT}/src/terminfo"
RST = f"{RS_ROOT}/crates/rustty-terminfo/src"

# rustty-cfg-fmt: GH base = src/config (config/formatter.zig). Near-leaf below
# config<->input (formatter.zig's key.zig import is DEAD code; Key lives with
# Config later). Distinct from the EXISTING `formatter` module above, which maps
# the much larger src/terminal/formatter.zig (the Page/Screen/Terminal debug
# formatters in rustty-vt).
GHCFG = f"{GH_ROOT}/src/config"
RSCF = f"{RS_ROOT}/crates/rustty-cfg-fmt/src"

# rustty-config: C1 leaf value-types (same GH base = src/config as GHCFG above).
RSCG = f"{RS_ROOT}/crates/rustty-config/src"

# rustty-input: GH base = src/input. The vt input slice (mouse/paste/key/
# key_encode/mouse_encode/function_keys + the VT-Mods half of key_mods) already
# lives in rustty-vt (mapped above as input_*). rustty-input is the REST of
# src/input/: Binding/command/config/keycodes/KeymapDarwin/KeymapNoop/Link/
# keyboard/helpgen_actions (+ the config-dependent RemapSet/translation half of
# key_mods, counted by the existing input_key_mods entry above).
GHI = f"{GH_ROOT}/src/input"
RSI = f"{RS_ROOT}/crates/rustty-input/src"

# rustty-renderer: the R-a headless renderer slice (heavy-tier, R-a of memory
# heavy-tier-strategy). GH base = src/renderer. The unit-portable, GPU-free
# modules: cell.zig (per-cell CellText fg glyph + CellBg vertex assembly from the
# Screen + font SharedGrid), cursor.zig (cursor rects/style), State.zig (the
# renderer State/mailbox), and size.zig (GridSize/ScreenSize/CellSize/padding).
# size.zig already landed in the rustty-render-types leaf (the A1 hoist) and is
# re-exported by rustty-renderer, so it maps to the render-types file. shadertoy.zig
# (the custom-shader GLSL->SPIR-V->MSL/GLSL transpile path over glslang-sys +
# spirv-cross-sys) is ported (4 tests). link.zig (link/hover highlight rects) is
# ported now that Oniguruma landed — the previous apc_deferred=3 is flipped to 0
# (mirrors the config url-regex + input Link.zig resolutions, DEFERRALS.md). The GPU
# draw path (Metal/OpenGL/generic/image/custom shaders) is R-d/Phase 3.
GHR = f"{GH_ROOT}/src/renderer"
RSR = f"{RS_ROOT}/crates/rustty-renderer/src"
RSRT = f"{RS_ROOT}/crates/rustty-render-types/src"

# rustty-vt-c: the C-ABI projection crate (Stage B). GH base = src/terminal/c
# (the `c/*.zig` ABI wrappers behind lib_vt.zig's @export lines). All test bodies
# live in the rustty-vt-c crate (`crates/rustty-vt-c/src/**/*.rs`, incl. the
# render/ + selection_gesture/ test submodules). This is an AGGREGATE mapping
# (28 c/*.zig ref files vs the whole vt-c src tree) because the C wrappers are one
# tightly-coupled ABI surface; the per-crate `cargo test -p rustty-vt-c` count is
# the authoritative backstop and this gate guards rust #[test] >= zig test{}.
GHVC = f"{GH_ROOT}/src/terminal/c"
RSVC = f"{RS_ROOT}/crates/rustty-vt-c/src"

# rustty-apprt-api: the A5 keystone interface crate (break #8, breaks apprt<->core).
# GH base = src/apprt (the action.zig types + surface/ipc/runtime/structs seam).
# Only the 5 interface files are carved here; the bulk of src/apprt/ (gtk/embedded
# backends, newConfig, GObject registrations, byte-exact C-ABI projection) stays
# for the future rustty-apprt crate. renderer.Health (a 2-variant enum(c_int) with
# zero GPU dep) is hoisted into the rustty-renderer crate root (like the
# render-types A1 size hoist) and mapped by `renderer_health` below.
GHAP = f"{GH_ROOT}/src/apprt"
RSAP = f"{RS_ROOT}/crates/rustty-apprt-api/src"
# rustty-apprt: the concrete apprt backends (embedded/none/browser), EMBEDDED_BLUEPRINT
# §7. Same GH base = src/apprt as GHAP above (the backend files, not the seam). The
# libghostty C ABI wrappers live in the separate rustty-apprt-c top-sink crate.
RSAPPRT = f"{RS_ROOT}/crates/rustty-apprt/src"
# rustty-apprt-c: the libghostty C ABI top-sink crate. Ports embedded.zig's `CAPI`
# block (the extern structs, chunk E5 -> capi.rs; the 71 `ghostty_*` exports,
# E6..E10) + the main_c.zig C API hub (lib.rs). All 0-test reference files.
RSAPPRTC = f"{RS_ROOT}/crates/rustty-apprt-c/src"
# rustty (the `rustty` bin, member 27). GH base = src root (main.zig /
# main_ghostty.zig live beside App.zig). CLI_BLUEPRINT.md §5/§6 chunk C6.
RSBIN = f"{RS_ROOT}/crates/rustty/src"
# renderer.zig is at src/renderer.zig (crate root), one level above GHR (src/renderer).
GHSRC = f"{GH_ROOT}/src"

# rustty-crash: the crash tier (integration-tier blueprint stage 1). GH base =
# src/crash. The `pkg/sentry` Zig binding package is hosted in-crate as the
# `sentry_pkg` module (dep-glue over the vendored sentry-sys) and counted with
# crash_sentry (its only anon `test { refAllDecls }` is skipped by zig_count;
# the Rust side adds reify/seam helper tests — rust >= zig holds).
GHCR = f"{GH_ROOT}/src/crash"
GHPKGS = f"{GH_ROOT}/pkg/sentry"
RSCR = f"{RS_ROOT}/crates/rustty-crash/src"

# rustty-termio: the termio tier (integration-tier blueprint stage 2+). GH base
# = src/termio (+ termio.zig at src root). Stage 2 lands the dep wiring + the
# struct shapes of the 0-test files + message.zig; Exec/shell_integration/
# stream_handler (+ the relocated src/Command.zig, break #13b) are stages 3-4.
GHTIO = f"{GH_ROOT}/src/termio"
RSTIO = f"{RS_ROOT}/crates/rustty-termio/src"

# rustty-inspector: core-tier chunk C1 (CORE_BLUEPRINT §4). GH base =
# src/inspector. Only the pure-data key-widget surface (Event/EventRing/label)
# + the main.zig KeyEvent re-export are carved; ALL inspector GUI logic
# (Inspector.zig, units.zig, the other widgets, dcimgui) is Phase 3.
GHINS = f"{GH_ROOT}/src/inspector"
RSINS = f"{RS_ROOT}/crates/rustty-inspector/src"

# rustty-core: the core tier (CORE_BLUEPRINT §6/§7). GH base = src root
# (App.zig / Surface.zig / surface_mouse.zig live beside main_ghostty.zig).
RSCORE = f"{RS_ROOT}/crates/rustty-core/src"

# rustty-xev: the libxev event-loop crate (D6, XEV_BLUEPRINT §7 / [ADV-FIX F7]).
# The count-gate cannot see libxev via ghostty/src — libxev is a content-addressed
# zig-cache DEPENDENCY package (mitchellh/libxev), NOT ghostty src. Its base path
# is therefore a pinned zig cache src dir. COMMIT: GHXEV is libxev 34fa5087, the
# revision this port TRACKS (D6 Zig-port-defer; the ghostty pin 6ad1fe7d has since
# bumped to 9ce8e8e6 — reviewed as a non-port, `.port/phase-3/NON_PORTS.md` §G3,
# "rustty-xev is not tracking libxev revisions"). GHXEV_CUR below is the CURRENT
# 9ce8e8e6 pin, used only by rows whose oracle content exists solely there
# (watcher/file.zig's queuePWrite KAT — codex W13A-02). SCOPE =
# macOS/kqueue only (XEV_BLUEPRINT §0-§1); other backends/file/tcp/udp are DEFERRED
# (DEFERRALS.md), so only the in-scope files are mapped. C1 seeds the mapping with
# the three pure intrusive data-structure files (queue/queue_mpsc/heap); later
# chunks add darwin/loop/kqueue/async/timer/process/stream entries.
GHXEV = f"{ZIG_P}/libxev-0.0.0-86vtc4IcEwCqEYxEYoN_3KXmc6A9VLcm22aVImfvecYs/src"
# The CURRENT ghostty pin (libxev 9ce8e8e6, hash 86vtcwIR…), extracted from the
# tarball ghostty's build.zig.zon names. If this checkout is absent, re-extract
# <zig cache>/p/libxev-0.0.0-86vtcwIRFADbH4hk-EjROXxlrKIRPQdA41XiTSytYO-F.tar.gz
# to this path (the R3 guard below turns its absence into BLOCKED, never green).
GHXEV_CUR = f"{GH_ROOT}/zig-pkg/libxev-0.0.0-86vtcwIRFADbH4hk-EjROXxlrKIRPQdA41XiTSytYO-F/src"
RSXEV = f"{RS_ROOT}/crates/rustty-xev/src"

# rustty-wuffs: the pkg/wuffs safe-wrapper parity crate (the renderer's
# background-image decode path). GH base = pkg/wuffs/src (NOT ghostty src/), so it
# is not in the 3234-test src/ contract; scoped-mapped here. png.zig/jpeg.zig carry
# 3 named `test "..."` each (the *_decode_000000/FFFFFF + `too big` known-answer
# tests); main.zig's only `test {}` is the anonymous refAllDecls (skipped by
# zig_count); error.zig/swizzle.zig carry 0 (swizzle is an R5 seam). zig=6; the
# Rust side adds the ref_all_decls compile-touch (rust=7 >= 6).
# P7-W0 T4: zlib.zig (NEW at the Phase-7 pin, `4b86e3594`: kitty zlib through wuffs)
# carries 5 named tests: the row reads zig 8 -> 13 at arrival (still GREEN by
# slack, rust=20), so the 5 names are the per-test meter (DECISIONS B8).
GHWUFFS = f"{GH_ROOT}/pkg/wuffs/src"
RSWUFFS = f"{RS_ROOT}/crates/rustty-wuffs/src"

# rustty-zf: the vendored `zf` 0.11.0 (0.10.3 until parity tail S-10b) fuzzy-finder parity crate (the theme-picker
# rank/highlight `cli/list_themes.zig` sorts on). GH base = the content-addressed
# zig-cache dep dir (NOT ghostty src/), so it is not in the 3234-test src/ contract;
# scoped-mapped here. zf.zig carries 2 named `test "..."` (rank/highlight library
# interface) + the named `test deduplicateMatches` (0.11.0) + 1 anonymous refAllDecls
# `test {}` (skipped by zig_count); filter.zig carries 4 (path iterator, segmentLen,
# rankNeedle, FixedArrayList); clib.zig carries 2 (rank/highlight exported C library
# interface). zig=9; the Rust side adds the ref_all_decls compile-touch and the
# default-polarity pin (rust=11 >= 9).
GHZF = f"{ZIG_P}/zf-0.11.0-OIRy8X-RAAAwaRXHMYpj2uvBnuGTZWEE_3V7acqHQNtW/src/zf"
RSZF = f"{RS_ROOT}/crates/rustty-zf/src"

# rustty-vaxis: the vendored `libvaxis` 0.6.0 (0.5.1 until parity tail S-10b) TUI-lib parity crate (the interactive
# CLI's terminal-UI dependency). GH base = the content-addressed zig-cache dep dir
# (NOT ghostty src/), so it is not in the 3234-test src/ contract; scoped-mapped here.
# CHUNK 1a ports the core VALUE TYPES: Cell.zig carries 1 named `test "rgbFromSpec"`;
# Key.zig carries 5 (matches 'a'/'shift+a'/'shift+tab'/'shift+;' + name_map);
# Mouse.zig/GraphemeCache.zig/Image.zig carry 0. Chunk 1b landed gwidth.zig (11 test) +
# unicode.zig (0); Chunk 2 landed Screen.zig (1) + InternalScreen.zig (0) + Window.zig (8) —
# all now referenced. Chunk 3a landed Parser.zig (34 test) + event.zig/ctlseqs.zig (0).
# Chunk 3b landed queue.zig (7 test) + tty.zig (0) + Loop.zig (1, the `#[ignore]`d
# lifecycle test — still a #[test] the glob counts). Chunk 4b landed widgets/TextInput.zig
# (3 test) — the new `widgets` tier; the rust glob adds {RSVAXIS}/widgets/*.rs (the top-level
# {RSVAXIS}/*.rs glob is non-recursive and would MISS widgets/text_input.rs). zig = Cell 1 +
# Key 5 + gwidth 11 + Screen 1 + Window 8 + Parser 34 + queue 7 + Loop 1 + TextInput 3 = 71
# at 0.5.1; 0.6.0 adds Parser 2 + InternalScreen 1 + TextInput 13 (= 87; its new anonymous
# refAllDecls `test {}` blocks are not counted). The two rust
# globs pick up every ported #[test], keeping rust >= zig.
# S-10c (review S-10b F6): the row now also covers `Vaxis.zig` (1: "render: no output when
# no changes") and the vxfw files the crate ports — `vxfw/vxfw.zig` (4, counting the quoted
# "refAllDecls" and the source-scan "All widgets have a doctest and refAllDecls test"),
# `vxfw/App.zig` (1), `vxfw/Text.zig` (10), `vxfw/Center.zig` (2) — with the rust glob
# `{RSVAXIS}/vxfw/*.rs`. zig = 87 + 18 = 105. apc_deferred = 1: the doctest source scan
# (`vxfw.zig:541`) reads `./src/vxfw/*.zig` at run time and has no Rust counterpart
# (NEW-FINDINGS N240). `--self-test` binds this row (VAXIS_VXFW_FILES, VAXIS_APC).
GHVAXIS = f"{ZIG_P}/vaxis-0.6.0-BWNV_CrbCQCscGpzsAlR402rYQ_tV3aAl081c2iRRkka/src"
RSVAXIS = f"{RS_ROOT}/crates/rustty-vaxis/src"
VAXIS_VXFW_FILES = ["Vaxis.zig", "vxfw/vxfw.zig", "vxfw/App.zig", "vxfw/Text.zig", "vxfw/Center.zig"]
VAXIS_APC = 1  # the vxfw doctest source scan, see above

# module -> (list of zig ref globs, list of rust globs, apc_deferred_count)
MAP = {
    "wuffs":           ([f"{GHWUFFS}/png.zig", f"{GHWUFFS}/jpeg.zig", f"{GHWUFFS}/main.zig", f"{GHWUFFS}/error.zig", f"{GHWUFFS}/swizzle.zig", f"{GHWUFFS}/zlib.zig"], [f"{RSWUFFS}/*.rs"], 0),
    "zf":              ([f"{GHZF}/zf.zig", f"{GHZF}/filter.zig", f"{GHZF}/clib.zig"], [f"{RSZF}/*.rs"], 0),
    "vaxis":           ([f"{GHVAXIS}/Cell.zig", f"{GHVAXIS}/Key.zig", f"{GHVAXIS}/Mouse.zig", f"{GHVAXIS}/GraphemeCache.zig", f"{GHVAXIS}/Image.zig", f"{GHVAXIS}/gwidth.zig", f"{GHVAXIS}/unicode.zig", f"{GHVAXIS}/Screen.zig", f"{GHVAXIS}/InternalScreen.zig", f"{GHVAXIS}/Window.zig", f"{GHVAXIS}/Parser.zig", f"{GHVAXIS}/event.zig", f"{GHVAXIS}/ctlseqs.zig", f"{GHVAXIS}/queue.zig", f"{GHVAXIS}/tty.zig", f"{GHVAXIS}/Loop.zig", f"{GHVAXIS}/widgets/TextInput.zig"] + [f"{GHVAXIS}/{f}" for f in VAXIS_VXFW_FILES], [f"{RSVAXIS}/*.rs", f"{RSVAXIS}/widgets/*.rs", f"{RSVAXIS}/vxfw/*.rs"], VAXIS_APC),
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
    "osc":             ([f"{GH}/osc.zig", f"{GH}/osc/encoding.zig", f"{GH}/osc/kitty_metadata.zig", f"{GH}/osc/parsers.zig"] + glob.glob(f"{GH}/osc/parsers/*.zig"), [f"{RS}/osc/**/*.rs"], 0),
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
    "input_mouse":       ([f"{GH_ROOT}/src/input/mouse.zig"], [f"{RS}/input/mouse.rs"], 0),
    "input_paste":       ([f"{GH_ROOT}/src/input/paste.zig"], [f"{RS}/input/paste.rs"], 0),
    "input_key":         ([f"{GH_ROOT}/src/input/key.zig"], [f"{RS}/input/key.rs"], 0),
    "input_mouse_encode":([f"{GH_ROOT}/src/input/mouse_encode.zig"], [f"{RS}/input/mouse_encode.rs"], 0),
    "input_key_encode":  ([f"{GH_ROOT}/src/input/key_encode.zig"], [f"{RS}/input/key_encode.rs"], 0),
    "input_function_keys":([f"{GH_ROOT}/src/input/function_keys.zig"], [f"{RS}/input/function_keys.rs"], 0),
    # key_mods.zig is split across two crates: the VT-Mods slice (Mods packed
    # struct + VT methods + bit-layout test) in rustty-vt's input/key_mods.rs,
    # and the config-dependent RemapSet/translation slice (the 23 RemapSet: +
    # translation-macos-option-as-alt tests) in rustty-input's key_mods.rs. The
    # 23 are no longer deferred — they landed once OptionAsAlt + cfg-fmt became
    # available — so apc_deferred is now 0 and BOTH rust files count against the
    # one zig file (vt=1 anon test + input=23 -> 24 >= 23).
    "input_key_mods":    ([f"{GH_ROOT}/src/input/key_mods.zig"], [f"{RS}/input/key_mods.rs", f"{RS_ROOT}/crates/rustty-input/src/key_mods.rs"], 0),
    # vt-15 search/ (gated)
    "search":            ([f"{GH}/search/Thread.zig", f"{GH}/search/active.zig", f"{GH}/search/pagelist.zig", f"{GH}/search/screen.zig", f"{GH}/search/sliding_window.zig", f"{GH}/search/terminal.zig", f"{GH}/search/viewport.zig"], [f"{RS}/search/*.rs"], 0),
    # vt-16 tmux/ (control mode; oniguruma-gated control tests counted via static
    # #[test] text — the gate matches `#[test]` source lines regardless of cfg).
    # All four tmux modules are ported: control + layout + output parsers + viewer
    # (the control-mode multiplexer driving per-pane Terminals). This completes
    # Ghostty terminal/ in rustty-vt.
    "tmux":              ([f"{GH}/tmux/control.zig", f"{GH}/tmux/layout.zig", f"{GH}/tmux/output.zig", f"{GH}/tmux/viewer.zig"], [f"{RS}/tmux/control.rs", f"{RS}/tmux/layout.rs", f"{RS}/tmux/output.rs", f"{RS}/tmux/viewer.rs"], 0),
    # Phase-5 blind-spot closures (assessment B §5; "derived-gate green can be
    # bookkeeping"): files the gate previously could not see. terminal/paste.zig's
    # only zig test is an anonymous refAllDecls aggregator (0 behavioral — the
    # behavior is the stream_terminal `paste:` suite); clipboard.zig carries
    # `test isTextMime` (5984d6f73); sys.zig carries the randomSecure-override
    # test (dda8e6f31); src/simd/*.zig carries the base64 strict+Streaming suite
    # (bc2f7d7d2/a8c3ab191, incl. the scalar-lane crash regression) in the
    # rustty-simd crate.
    "terminal_paste":     ([f"{GH}/paste.zig"], [f"{RS}/paste.rs"], 0),
    "terminal_clipboard": ([f"{GH}/clipboard.zig"], [f"{RS}/clipboard.rs"], 0),
    "terminal_sys":       ([f"{GH}/sys.zig"], [f"{RS}/sys.rs"], 0),
    "simd":               (glob.glob(f"{GH_ROOT}/src/simd/*.zig"), [f"{RS_ROOT}/crates/rustty-simd/src/*.rs"], 0),
    # Stage A.2 — rustty-font pure-parsing slice (GH base = src/font, not src/terminal).
    # NOTE: zig_count() matches `test "name"`; sfnt.zig also has one unquoted
    # `test FixedPoint {` (caught by Rust's 3rd #[test] — rust >= zig holds).
    "font_glyf":         ([f"{GHF}/opentype/glyf.zig"], [f"{RSF}/opentype/glyf.rs"], 0),
    "font_sfnt":         ([f"{GHF}/opentype/sfnt.zig"], [f"{RSF}/opentype/sfnt.rs"], 0),
    "font_head":         ([f"{GHF}/opentype/head.zig"], [f"{RSF}/opentype/head.rs"], 0),
    "font_metrics":      ([f"{GHF}/Metrics.zig"], [f"{RSF}/metrics.rs"], 0),
    # Stage F0 — rustty-font heavy-tier base: Atlas.zig (texture-atlas rect packing,
    # 13 quoted tests) + CodepointMap.zig (codepoint->descriptor map, 1 test).
    # apc_deferred=0 for font_atlas since WS14 T4: the indented `test "happy path"`
    # (Atlas.zig:564) lives INSIDE the wasm `Wasm` struct, which is now ported LIVE
    # as `atlas::wasm_exports` under the off-by-default `wasm` cargo feature, with
    # ALL FOUR of its reference asserts running (`cargo test -p rustty-font
    # --features wasm`). The two blockers are gone: the `os/wasm.zig`
    # host-ownership tracker upstream deleted in `9dc2e5978` is resurrected in
    # rustty-os, and the `zig-js` canvas ops are declared locally as `env` imports
    # (no crate edge — R4). This gate counts `#[test]` source lines regardless of
    # cfg, so need = 13 and rust = 13 (the 12 non-wasm Atlas tests + happy_path).
    # CodepointMap: 1 quoted test
    # ("codepointmap") -> codepoint_map.rs; the discovery.Descriptor edge is SEAMed
    # as a thin local `Descriptor` (family-only) — no C-lib/discovery edge pulled
    # into pure rustty-font (the real discovery surface lands in the F3 shaper tier).
    "font_atlas":        ([f"{GHF}/Atlas.zig"], [f"{RSF}/atlas.rs"], 0),
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
    # rustty-cli C1 (CLI_BLUEPRINT.md §3+§7): the action dispatcher. action.zig
    # (7t) + ghostty.zig Action enum/run/runMain (4t) + cli.zig re-export
    # (anonymous refAllDecls `test {`, skipped by zig_count → 0) + help.zig (0t) +
    # version.zig (0t, PARTIAL). The remaining cli/ action impls are sequencing
    # stubs (0t, no MAP entry yet); their entries land with chunks C2–C5.
    "cli_action":        ([f"{GHC}/action.zig"], [f"{RSCLI}/action.rs"], 0),
    # WS19 T1: R2 renamed cli/ghostty.rs -> cli/rustty.rs at 410c387 and this mapping
    # was never updated. It counted rust=0 for a file with 4 live #[test]s — invisible
    # because the gate was BLOCKED on the evicted zig corpus. The new RUST-side
    # existence guard in main() is what makes this class loud instead of silent.
    "cli_ghostty":       ([f"{GHC}/ghostty.zig"], [f"{RSCLI}/rustty.rs"], 0),
    "cli_root":          ([f"{GHC}/../cli.zig"], [f"{RSCLI}/lib.rs"], 0),
    "cli_help":          ([f"{GHC}/help.zig"], [f"{RSCLI}/help.rs"], 0),
    "cli_version":       ([f"{GHC}/version.zig"], [f"{RSCLI}/version.rs"], 0),
    # rustty-cli C2 (CLI_BLUEPRINT.md §7): the pure config/introspection actions +
    # Pager. Pager.zig (2t: "pager: non-tty" + "pager: default writer") -> pager.rs.
    # show_config/validate_config/list_actions.zig are 0t. explain_config.zig has
    # 1t ("explain") which is now LIVE: the help_strings doc-table generator landed
    # (`config_help` / `keybind_help` build.rs codegen), so `explain_option` /
    # `explain_keybind` return their doc text and the test passes — rust=1,
    # apc_deferred=0 (DEFERRALS.md "cli C2 — explain_config" RESOLVED).
    # list_fonts.zig (0t) + show_face.zig (0t) are now
    # REAL ports against rustty-font-shaper's discovery + SharedGridSet (the earlier
    # "blocked on rustty-font discovery" C2 note was a mis-attribution; that
    # subsystem lives in the shaper crate) — both 0-test MAP entries below.
    "cli_pager":            ([f"{GHC}/Pager.zig"], [f"{RSCLI}/pager.rs"], 0),
    "cli_show_config":      ([f"{GHC}/show_config.zig"], [f"{RSCLI}/show_config.rs"], 0),
    "cli_validate_config":  ([f"{GHC}/validate_config.zig"], [f"{RSCLI}/validate_config.rs"], 0),
    "cli_list_actions":     ([f"{GHC}/list_actions.zig"], [f"{RSCLI}/list_actions.rs"], 0),
    "cli_explain_config":   ([f"{GHC}/explain_config.zig"], [f"{RSCLI}/explain_config.rs"], 0),
    "cli_list_fonts":       ([f"{GHC}/list_fonts.zig"], [f"{RSCLI}/list_fonts.rs"], 0),
    "cli_show_face":        ([f"{GHC}/show_face.zig"], [f"{RSCLI}/show_face.rs"], 0),
    # rustty-cli C3 (CLI_BLUEPRINT.md §4): the ssh subsystem, ported as an `ssh`
    # submodule tree under rustty-cli. Entry.zig (3 quoted) -> ssh_cache/entry.rs.
    # DiskCache.zig (9 quoted + isValidHost/isValidUser/isValidCacheKey named-
    # unquoted = 12) -> ssh_cache/disk_cache.rs (the 2 FailingAllocator OOM tests
    # run their single success path — Rust allocation is infallible; ownership +
    # Miri are the leak/UB backstop). ssh_cache.zig (matchesQuery/formatTimestamp/
    # relativeAge named-unquoted + "runInner rejects multiple actions" = 4; the anon
    # `test { _ = DiskCache; _ = Entry; }` reify is skipped by zig_count) ->
    # ssh_cache.rs (rust=5 with the ref_all_decls reify margin). ssh.zig (3
    # parseManuallyHook + 6 parseDestination quoted = 9; the anon Joined `test {` is
    # skipped) -> ssh.rs (rust=10 with the ported Joined test). xterm-ghostty ->
    # xterm-rustty and the DiskCache program dir ghostty -> rustty route through
    # rustty-identity (ADV-FIX 5).
    "cli_ssh_cache_entry":  ([f"{GHC}/ssh-cache/Entry.zig"], [f"{RSCLI}/ssh_cache/entry.rs"], 0),
    "cli_ssh_cache_diskcache": ([f"{GHC}/ssh-cache/DiskCache.zig"], [f"{RSCLI}/ssh_cache/disk_cache.rs"], 0),
    "cli_ssh_cache":        ([f"{GHC}/ssh_cache.zig"], [f"{RSCLI}/ssh_cache.rs"], 0),
    "cli_ssh":              ([f"{GHC}/ssh.zig"], [f"{RSCLI}/ssh.rs"], 0),
    # rustty-cli C4 (CLI_BLUEPRINT.md §7): the apprt-backed + misc actions. All
    # 0-test: crash_report (rustty-crash defaultDir + Report iterator),
    # edit_config (rustty-config + rustty-os getenv/ShellEscapeWriter/exec), the
    # none-backed IPC actions new_window (full parseManuallyHook: --class /
    # --working-directory realpath+~ / -e capture / cwd) + toggle_quick_terminal,
    # and the tui.zig `can_pretty_print` seam. new_window/toggle deliver over
    # `none::App::perform_ipc` (Ok(false)) -> print "not supported" + exit 1
    # (faithful, ADV-FIX 4 / Risk 4 — NOT a stub).
    "cli_crash_report":     ([f"{GHC}/crash_report.zig"], [f"{RSCLI}/crash_report.rs"], 0),
    "cli_edit_config":      ([f"{GHC}/edit_config.zig"], [f"{RSCLI}/edit_config.rs"], 0),
    "cli_new_window":       ([f"{GHC}/new_window.zig"], [f"{RSCLI}/new_window.rs"], 0),
    "cli_toggle_quick_terminal": ([f"{GHC}/toggle_quick_terminal.zig"], [f"{RSCLI}/toggle_quick_terminal.rs"], 0),
    "cli_tui":              ([f"{GHC}/tui.zig"], [f"{RSCLI}/tui.rs"], 0),
    # rustty-cli C5 (CLI_BLUEPRINT.md §1/§7 + Risk 1/2): the vaxis/zf-touching
    # actions. All 0-test. list_colors/list_keybinds land the `--plain`/non-tty
    # paths LIVE (x11_color enumeration / config.keybind.formatEntryDocs); their
    # `prettyPrint` (vaxis) branches are vaxis-deferred leaves. list_themes lands
    # the theme ENUMERATION LIVE (config/theme.zig LocationIterator + Config.loadFile);
    # its interactive vaxis+zf picker is a deferred leaf (reference body preserved
    # verbatim). boo is a pure-vaxis animation -> vaxis-deferred leaf. All deferred
    # render paths are 0-test (no contract impact); G3b-gated on comments.
    "cli_list_colors":      ([f"{GHC}/list_colors.zig"], [f"{RSCLI}/list_colors.rs"], 0),
    "cli_list_keybinds":    ([f"{GHC}/list_keybinds.zig"], [f"{RSCLI}/list_keybinds.rs"], 0),
    "cli_list_themes":      ([f"{GHC}/list_themes.zig"], [f"{RSCLI}/list_themes.rs"], 0),
    "cli_boo":              ([f"{GHC}/boo.zig"], [f"{RSCLI}/boo.rs"], 0),
    # Port of src/extra/ (shell completions + editor syntax). bash/zsh/sublime are
    # 0-test. fish.zig has 1 named test ("getDescription") -> fish.rs (rust>=1, +the
    # golden diff). vim.zig's sole test is the anonymous `test { _ = syntax; }`
    # (refAllDecls-style, skipped by zig_count -> zig=0) -> vim.rs (rust=5 with the
    # ported smoke test + golden diffs).
    "extra_bash":        ([f"{GHX}/bash.zig"], [f"{RSX}/bash.rs"], 0),
    "extra_zsh":         ([f"{GHX}/zsh.zig"], [f"{RSX}/zsh.rs"], 0),
    "extra_fish":        ([f"{GHX}/fish.zig"], [f"{RSX}/fish.rs"], 0),
    "extra_vim":         ([f"{GHX}/vim.zig"], [f"{RSX}/vim.rs"], 0),
    "extra_sublime":     ([f"{GHX}/sublime.zig"], [f"{RSX}/sublime.rs"], 0),
    # rustty bin C6 (CLI_BLUEPRINT.md §5/§6 + ADV-FIX 2/6): the `rustty` bin.
    # main.zig (anon `test { _ = entrypoint; }`, refAllDecls-style -> skipped by
    # zig_count -> zig=0) -> main.rs (rust=1 ref_entrypoint). main_ghostty.zig (anon
    # `test { _ = @import(..); }`, refAllDecls-style -> zig=0) -> main_ghostty.rs
    # (rust=1 ref_all_decls). The bin owes main 1 + main_ghostty 1 at the phase-0
    # inventory level (2t); at the GATE both are the skipped anonymous form (zig=0),
    # so rust>=zig holds with the ported reify tests.
    "main":              ([f"{GHSRC}/main.zig"], [f"{RSBIN}/main.rs"], 0),
    # WS19 T1: R2 renamed main_ghostty.rs -> main_rustty.rs at 410c387; mapping was stale.
    "main_ghostty":      ([f"{GHSRC}/main_ghostty.zig"], [f"{RSBIN}/main_rustty.rs"], 0),
    # ADV-FIX 6 (§6 option b): main_c.zig (3 quoted `ghostty_string_s` tests) is
    # RE-HOMED to rustty-apprt-c (the libghostty C-API hub the `String` port lives
    # in) — the bin does NOT dep apprt-c. zig=3; the 3 tests are ported into
    # rustty-apprt-c/lib.rs's main_c_tests mod (rust>=3). Note apprt-c's lib.rs is
    # also globbed by `apprt_embedded` (a per-module independent count); both
    # entries hold.
    "main_c":            ([f"{GHSRC}/main_c.zig"], [f"{RSAPPRTC}/lib.rs"], 0),
    # ADV-FIX 6: lib_vt.zig (the librustty-vt entry) is RE-HOMED to rustty-vt-c
    # (vt-c precedent). Its sole `test {}` is the anonymous refAllDecls-style reify
    # (`_ = terminal; _ = lib/main; refAllDecls(input); _ = terminal.c_api;`) -> the
    # zig_count anon-skip makes it zig=0, already covered by vt-c's C-ABI reify
    # surface (rust>=0).
    "lib_vt":            ([f"{GHSRC}/lib_vt.zig"], [f"{RSVC}/**/*.rs"], 0),
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
    # Link.zig: 0 tests (oniguruma-backed regex link). `oni_regex` is now wired
    # to the real `oni::Regex` (rustty-oniguruma, non-optional dep — the reference
    # imports oniguruma unconditionally in src/input/); no reference test exists,
    # so this stays a normally-counted 0 (NOT deferred/excluded).
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
    # url.zig: 1 quoted test ("url regex"), oniguruma-backed -> now PORTED behind
    # the off-by-default `oniguruma` cargo feature (which pulls in the `oni` safe
    # wrapper crate; documented in .port/phase-2/DEFERRALS.md, RESOLVED). The
    # ported test lives in the `#[cfg(all(test, feature="oniguruma"))]` module; its
    # `#[test]` line is counted by rust_count regardless of cfg. So apc_deferred=0
    # -> need=1, rust=1.
    "config_url":        ([f"{GHCFG}/url.zig"], [f"{RSCG}/url.rs"], 0),
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
    # shadertoy.zig: 4 quoted tests (spirv / spirv invalid / shadertoy to msl /
    # shadertoy to glsl) -> shadertoy.rs, over glslang-sys + spirv-cross-sys FFI.
    # rust=4. (loadFromFiles/loadFromFile are config-coupled and would cycle
    # renderer->config->renderer, so they are DEFERRED with verbatim bodies — but
    # they carry NO test{}, so there is no count impact; DEFERRALS.md.)
    "renderer_shadertoy":([f"{GHR}/shadertoy.zig"], [f"{RSR}/shadertoy.rs"], 0),
    # link.zig: 3 quoted tests (renderCellMap / renderCellMap hover links /
    # renderCellMap mods no match) -> link.rs. The Oniguruma deferral is RESOLVED
    # (oni.Regex/Region landed via rustty-oniguruma), so apc_deferred flips 3 -> 0
    # and rust=3 (>= zig=3).
    "renderer_link":     ([f"{GHR}/link.zig"], [f"{RSR}/link.rs"], 0),
    # renderer.zig crate root: `Health` (1 quoted test) + a refAllDecls anon test
    # (skipped by zig_count) -> zig=1. Ported into rustty-renderer/src/lib.rs. rust=1.
    "renderer_health":   ([f"{GHSRC}/renderer.zig"], [f"{RSR}/lib.rs"], 0),
    # rustty-apprt-api A5 keystone. action.zig: 16 quoted checkGhosttyHEnum tests +
    # a refAllDecls anon (skipped) -> zig=16; ported into action.rs (types) +
    # action_tests.rs (17 rust: 16 henum + the refAllDecls smoke). rust=17 >= 16.
    "apprt_api_action":  ([f"{GHAP}/action.zig"], [f"{RSAP}/action.rs", f"{RSAP}/action_tests.rs"], 0),
    # ipc.zig: 2 quoted tests (Target.Key / Action.Key henum) -> ipc.rs. rust=2.
    "apprt_api_ipc":     ([f"{GHAP}/ipc.zig"], [f"{RSAP}/ipc.rs"], 0),
    # runtime.zig: `test { _ = Runtime; }` anon reify (skipped by zig_count) -> zig=0;
    # ported as runtime.rs's reify_runtime smoke. rust=1 >= 0.
    "apprt_api_runtime": ([f"{GHAP}/runtime.zig"], [f"{RSAP}/runtime.rs"], 0),
    # surface.zig / structs.zig: no test{} blocks -> zig=0 each (0 >= 0).
    "apprt_api_surface": ([f"{GHAP}/surface.zig"], [f"{RSAP}/surface.rs"], 0),
    "apprt_api_structs": ([f"{GHAP}/structs.zig"], [f"{RSAP}/structs.rs"], 0),
    # rustty-apprt concrete backends (EMBEDDED_BLUEPRINT §4/§6.5, chunk E0). Both
    # reference files are 0-test -> zig=0 each (documented 0-test modules, same
    # treatment as the other 0-test files). none.zig: performIpc-only App + empty
    # Surface -> none.rs. browser.zig: empty App/Window structs -> browser.rs.
    # rust >= zig always holds (rustty-only helper #[test]s add freely). The
    # embedded backend maps in as chunks E1..E10 land their content (embedded.zig
    # is itself 0-test); the 71 libghostty C ABI exports (rustty-apprt-c) map in as
    # E5..E11 land theirs.
    "apprt_none":        ([f"{GHAP}/none.zig"], [f"{RSAPPRT}/none.rs"], 0),
    "apprt_browser":     ([f"{GHAP}/browser.zig"], [f"{RSAPPRT}/browser.rs"], 0),
    # embedded.zig: 0 test{} -> zig=0 (documented 0-test module, EMBEDDED_BLUEPRINT
    # §6.5). E1 lands the `App` backend + `impl apprt_api::App` (+ the E1->E2
    # Surface sequencing stub) in embedded.rs; E2..E10 fill the rest of the same
    # file. embedded.zig's `CAPI` block is split into the apprt-c top-sink crate:
    # the extern structs land in capi.rs (E5), the `ghostty_*` exports in E6..E10
    # (glob `{RSAPPRTC}/**/*.rs`). rust >= zig always holds (0-test reference).
    "apprt_embedded":    ([f"{GHAP}/embedded.zig"], [f"{RSAPPRT}/embedded.rs", f"{RSAPPRTC}/**/*.rs"], 0),
    # rustty-crash (integration-tier stage 1). dir.zig: no test{} -> zig=0.
    "crash_dir":         ([f"{GHCR}/dir.zig"], [f"{RSCR}/dir.rs"], 0),
    # sentry.zig: no test{} -> zig=0; the pkg/sentry binding package files are
    # counted here too (main.zig's anon refAllDecls is skipped by zig_count).
    # Rust adds the sentry_pkg reify + the ThreadStateSurface seam tests.
    "crash_sentry":      ([f"{GHCR}/sentry.zig"] + glob.glob(f"{GHPKGS}/*.zig"), [f"{RSCR}/sentry.rs", f"{RSCR}/sentry_pkg/*.rs"], 0),
    # sentry_envelope.zig: 8 quoted tests -> sentry_envelope.rs (8, R1 contract).
    "crash_sentry_envelope": ([f"{GHCR}/sentry_envelope.zig"], [f"{RSCR}/sentry_envelope.rs"], 0),
    # main.zig: 1 anon refAllDecls test (skipped by zig_count -> zig=0); ported
    # as lib.rs's ref_all_decls reify smoke. rust=1 >= 0 (the "main 1 reify" of
    # the blueprint's crash=9 accounting: envelope 8 + main 1).
    "crash_main":        ([f"{GHCR}/main.zig"], [f"{RSCR}/lib.rs"], 0),
    # rustty-renderer (integration-tier stage 2) — the minimal cross-thread
    # surface termio consumes. message.zig: no test{} -> zig=0.
    "renderer_message":  ([f"{GHR}/message.zig"], [f"{RSR}/message.rs"], 0),
    # Thread.zig: no test{} -> zig=0. Only the minimal surface (Thread::Mailbox
    # + the DerivedConfig deferred-seam placeholder) is ported in thread.rs; the
    # event loop is D6/GPU-deferred (DEFERRALS.md).
    "renderer_thread":   ([f"{GHR}/Thread.zig"], [f"{RSR}/thread.rs"], 0),
    # rustty-termio (integration-tier stage 2) — dep wiring + struct shapes.
    # message.zig: 2 anonymous test{} blocks (skipped by zig_count -> zig=0);
    # BOTH are ported (ref_all_decls reify + the @sizeOf anti-growth contract at
    # the Rust-layout constant) -> rust=2 >= 0.
    "termio_message":    ([f"{GHTIO}/message.zig"], [f"{RSTIO}/message.rs"], 0),
    # Options.zig: no test{} -> zig=0 (struct shape in options.rs).
    "termio_options":    ([f"{GHTIO}/Options.zig"], [f"{RSTIO}/options.rs"], 0),
    # Termio.zig: no test{} -> zig=0 (struct shapes + self-contained logic in
    # termio_impl.rs; live IO-thread bodies deferred verbatim — DEFERRALS.md).
    "termio_termio":     ([f"{GHTIO}/Termio.zig"], [f"{RSTIO}/termio_impl.rs"], 0),
    # backend.zig: no test{} -> zig=0 (delegating unions in backend.rs; Exec
    # payloads are stage-4 placeholders).
    "termio_backend":    ([f"{GHTIO}/backend.zig"], [f"{RSTIO}/backend.rs"], 0),
    # mailbox.zig: no test{} -> zig=0 (SPSC Mailbox in mailbox.rs; `send` slow
    # path deferred verbatim — DEFERRALS.md).
    "termio_mailbox":    ([f"{GHTIO}/mailbox.zig"], [f"{RSTIO}/mailbox.rs"], 0),
    # Thread.zig: no test{} -> zig=0 (writer-thread struct shape + init/deinit
    # in thread.rs; run loop deferred verbatim, D6 — DEFERRALS.md).
    "termio_thread":     ([f"{GHTIO}/Thread.zig"], [f"{RSTIO}/thread.rs"], 0),
    # termio.zig root: 1 anon refAllDecls test (skipped by zig_count -> zig=0);
    # ported as lib.rs's ref_all_decls reify smoke (the blueprint's "root 1"
    # of termio=44, accounted HERE exactly once; its shell_integration reify
    # half lands with stage 4's 21 tests). rust=1 >= 0.
    "termio_root":       ([f"{GH_ROOT}/src/termio.zig"], [f"{RSTIO}/lib.rs"], 0),
    # Exec.zig (integration-tier stage 4): 10 quoted tests -> 10 live #[test]
    # in exec.rs (the 2 darwin-only + 4 windows-only tests keep the reference's
    # SkipZigTest cfg-gating — stage-3 Command / F3 DirectWrite precedent). The
    # xev event-loop wiring + Subprocess.start are D6-deferred with verbatim
    # bodies (0 tests on those paths — DEFERRALS.md).
    "termio_exec":       ([f"{GHTIO}/Exec.zig"], [f"{RSTIO}/exec.rs"], 0),
    # shell_integration.zig (stage 4): 20 quoted + 1 named-unquoted behavioral
    # (`test detectShell {`) = 21 -> 21 live #[test] (byte-exact per-shell
    # string/env generation; R1 contract).
    "termio_shell_integration": ([f"{GHTIO}/shell_integration.zig"], [f"{RSTIO}/shell_integration.rs"], 0),
    # stream_handler.zig (stage 4): no test{} -> zig=0 (the StreamHandler
    # vt-Handler impl in stream_handler.rs; the cross-thread writer slow paths
    # are deferred verbatim — DEFERRALS.md).
    "termio_stream_handler": ([f"{GHTIO}/stream_handler.zig"], [f"{RSTIO}/stream_handler.rs"], 0),
    # src/Command.zig (RELOCATED, break #13b — blueprint §1): Command lives in
    # rustty-termio, NOT rustty-core (core placement would cycle: termio/Exec
    # constructs+starts Command while core already deps termio). The zig glob
    # points at the PHYSICAL reference location (src root); the Rust file is
    # termio's command.rs. 10 quoted tests -> 10 live #[test] (the 6
    # windows-SkipZigTest'd tests run their posix bodies; the windows halves
    # are cfg-gated, F3 DirectWrite precedent).
    "termio_command":    ([f"{GHSRC}/Command.zig"], [f"{RSTIO}/command.rs"], 0),
    # rustty-inspector (core-tier C1). widgets/key.zig: no test{} -> zig=0
    # (partial-file port: Event/EventRing/label live; the ImGui section is
    # deferred verbatim — DEFERRALS.md "rustty-inspector C1").
    "inspector_widgets_key": ([f"{GHINS}/widgets/key.zig"], [f"{RSINS}/widgets/key.rs"], 0),
    # widgets.zig: no test{} -> zig=0 (module wiring only; ImGui helpers Phase 3).
    "inspector_widgets": ([f"{GHINS}/widgets.zig"], [f"{RSINS}/widgets.rs"], 0),
    # main.zig: 1 anon refAllDecls test (skipped by zig_count -> zig=0); ported
    # as lib.rs's ref_all_decls reify smoke over the carved surface (termio_root
    # precedent). rust=1 >= 0.
    "inspector_main":    ([f"{GHINS}/main.zig"], [f"{RSINS}/lib.rs"], 0),
    # rustty-core (core tier, CORE_BLUEPRINT §7: map all three files so the
    # tracker proves coverage — config-cluster precedent).
    # surface_mouse.zig (chunk C2): 1 quoted test ("keyToMouseShape", 13
    # sub-blocks) -> 1 #[test] in surface_mouse.rs (ported as ONE test
    # mirroring the single reference test{} block; rust >= zig).
    "core_surface_mouse": ([f"{GHSRC}/surface_mouse.zig"], [f"{RSCORE}/surface_mouse.rs"], 0),
    # App.zig: no test{} -> zig=0. LANDED (chunk C4): app.rs is the full App.zig
    # port; its 13 #[test] are the §7 rustty-only `mod test_runtime` seam tests
    # (TestApp/TestSurface fake, honest test glue — NOT ported reference tests),
    # so rust=13 >= zig=0. (App.zig itself carries no reference test{}.)
    "core_app":          ([f"{GHSRC}/App.zig"], [f"{RSCORE}/app.rs"], 0),
    # Surface.zig: no test{} -> zig=0. LANDED (chunks C5-C15): surface.rs is the
    # full 6036-loc Surface.zig port. Surface<A> is untested-by-construction (§7:
    # init spawns pty/renderer/io threads; ghostty has zero Surface tests for the
    # same reason), so rust=0 == zig=0; the G3b comment-coverage gate + per-chunk
    # review carry Surface's quality (0 missing over the full file, C-F sweep).
    "core_surface":      ([f"{GHSRC}/Surface.zig"], [f"{RSCORE}/surface.rs"], 0),
    # rustty-xev C1 (XEV_BLUEPRINT §8) — the intrusive data structures. queue.zig
    # (1 test `Intrusive`: singly-linked FIFO), queue_mpsc.zig (1 test `Intrusive`:
    # Vyukov MPSC), heap.zig (6 tests: heap; remove root; remove w/ children; equal
    # values; million values; dangling next). Pure, Miri-clean. Later chunks add the
    # kqueue/async/timer/process/stream mappings.
    "xev_queue":         ([f"{GHXEV}/queue.zig"], [f"{RSXEV}/queue.rs"], 0),
    "xev_queue_mpsc":    ([f"{GHXEV}/queue_mpsc.zig"], [f"{RSXEV}/queue_mpsc.rs"], 0),
    "xev_heap":          ([f"{GHXEV}/heap.zig"], [f"{RSXEV}/heap.rs"], 0),
    # rustty-xev C2 (XEV_BLUEPRINT §3) — the darwin/syscall layer. darwin.zig has
    # 0 test{} (mach/KernE/MachMsgE bindings only), so zig=0 == rust=0; the G3b
    # comment-coverage gate carries its fidelity. Later chunks add the kqueue/
    # async/timer/process/stream mappings.
    "xev_darwin":        ([f"{GHXEV}/darwin.zig"], [f"{RSXEV}/darwin.rs"], 0),
    # rustty-xev C4 (XEV_BLUEPRINT §4/§7) — the Darwin mach-port async wakeup.
    # async.zig has 3 quoted `test{}` (`async`, `async: notify first`, `async
    # batches multiple notifications`, inside `AsyncTests`; the per-variant `test {
    # _ = AsyncTests(...); }` are anon refAllDecls, zig_count-skipped) → zig=3.
    # async.rs ports all 3 as live #[test] + rustty KAT 1 (cross-thread wakeup
    # round-trip) → rust=4 >= 3. The non-Darwin AsyncEventFd/LoopState/IOCP/Dynamic
    # variants are comptime-elided on macOS (§0) and deferred (§1); their doc
    # comments are preserved in async.rs (R3, G3b-covered).
    "xev_async":         ([f"{GHXEV}/watcher/async.zig"], [f"{RSXEV}/async.rs"], 0),
    # rustty-xev C5 (XEV_BLUEPRINT §4/§7) — the EVFILT_PROC subprocess exit watcher.
    # process.zig has 3 quoted `test{}` (`process wait`, `process wait with non-zero
    # exit code`, `process wait on a process that already exited`, inside
    # `ProcessTests`; the per-variant `test { _ = ProcessTests(...); }` are anon
    # refAllDecls, zig_count-skipped) → zig=3. process.rs ports all 3 as live #[test]
    # + rustty KAT 3 (posix_spawn /bin/true → wait → exit status 0) → rust=4 >= 3.
    # The non-kqueue ProcessPidFd/ProcessIocp/ProcessDynamic variants are
    # comptime-elided on macOS (§0) and deferred (§1); their doc comments are
    # preserved in process.rs (R3, G3b-covered).
    "xev_process":       ([f"{GHXEV}/watcher/process.zig"], [f"{RSXEV}/process.rs"], 0),
    # rustty-xev C6 (XEV_BLUEPRINT §4/§7) — the Stream watcher + ordered-write engine.
    # stream.zig has 4 quoted `test{}` (`Stream decls`, `pty: child to parent`, `pty:
    # parent to child`, `pty: queued writes`, inside `GenericStreamTests`; the two
    # per-variant `test { _ = GenericStreamTests(...); }` are anon refAllDecls,
    # zig_count-skipped) → zig=4. stream.rs ports all 4 as live #[test] + rustty KAT 4
    # (openpty → queue_write → read-back → assert bytes + WriteQueue drains) → rust=5
    # >= 4. The `xev.dynamic` Union/superset branches + the `.recv`/`.send` read/write
    # methods are comptime-elided on macOS (§0) and deferred (§1); their doc comments
    # are preserved in stream.rs (R3, G3b-covered).
    "xev_stream":        ([f"{GHXEV}/watcher/stream.zig"], [f"{RSXEV}/stream.rs"], 0),
    # rustty-xev C7 (XEV_BLUEPRINT §1/§7 + [ADV-FIX F7]) — THE FINALE mapping so R1
    # is enforced across the whole in-scope surface (rust #[test] >= zig test{}).
    # backend/kqueue.zig: 12 quoted `test "kqueue: ..."` (incl. `file IO on thread
    # pool`, ported as a documented SkipZigTest-style skip-stub #[test] — ThreadPool
    # body deferred, DEFERRALS.md; the count still holds) → zig=12; kqueue.rs=12.
    "xev_kqueue":        ([f"{GHXEV}/backend/kqueue.zig"], [f"{RSXEV}/kqueue.rs", f"{RSXEV}/kqueue/**/*.rs"], 0),
    # watcher/timer.zig: 5 quoted `test{}` (`timer`, `timer reset`, `timer reset
    # before tick`, `timer reset after trigger`, `timer cancel`, inside `TimerTests`;
    # the per-variant anon `test { _ = TimerTests(...); }` are refAllDecls, zig_count-
    # skipped) → zig=5. C7 un-defers `Timer::reset`/`cancel` and ports all 5 as live
    # #[test] + rustty KAT 2 (Timer fires) → timer.rs=6 >= 5. The `TimerDynamic`
    # variant is comptime-elided on macOS (§0); its doc comments are R3/G3b-covered.
    "xev_timer":         ([f"{GHXEV}/watcher/timer.zig"], [f"{RSXEV}/timer.rs"], 0),
    # api.zig: the `Xev(be, T)` decl-forwarding struct. 1 quoted `test "completion is
    # zero-able"` (R1) + 1 anon `test { refAllDecls }` (zig_count-skipped) → zig=1. On
    # macOS the Api() collapses to this crate's public surface (§0), so both tests live
    # at the package root (lib.rs `api_tests`): completion_is_zero_able (the R1 parity
    # test) + ref_all_decls (the +margin reify) → lib.rs=2 >= 1.
    "xev_api":           ([f"{GHXEV}/api.zig"], [f"{RSXEV}/lib.rs"], 0),
    # backend.zig: `Backend` enum + `default()`/`candidates()`/`Api()`. 0 test{} →
    # zig=0. backend.rs adds one +margin sanity #[test] (default==kqueue + single
    # candidate, the §0 fact). rust=1 >= 0.
    "xev_backend":       ([f"{GHXEV}/backend.zig"], [f"{RSXEV}/backend.rs"], 0),
    # main.zig: the package root re-export surface. Its sole `test {}` is the anon
    # heap/queue/ThreadPool/Dynamic aggregator (zig_count-skipped) → zig=0. Ported as
    # THIS crate's lib.rs re-export surface (shares lib.rs with xev_api; rust>=0).
    "xev_main":          ([f"{GHXEV}/main.zig"], [f"{RSXEV}/lib.rs"], 0),
    # loop.zig: Options/RunMode/Callback/NoopCallback/CallbackAction/CompletionState
    # (the callback ABI). 0 test{} → zig=0 (loop.rs; G3b carries fidelity).
    "xev_loop":          ([f"{GHXEV}/loop.zig"], [f"{RSXEV}/loop.rs"], 0),
    # watcher/common.zig: `userdataValue` (the typed→untyped bridge). 0 test{} →
    # zig=0 (common.rs).
    "xev_common":        ([f"{GHXEV}/watcher/common.zig"], [f"{RSXEV}/common.rs"], 0),
    # ThreadPool.zig: the generic thread pool. Ported as TYPE SURFACE only (body
    # DEFERRED — ghostty passes no pool, §1/§6; DEFERRALS.md). 0 test{} → zig=0
    # (thread_pool.rs). net.rs (a std.net.Address subset, not a libxev source file)
    # is intentionally UNMAPPED — it has no `.zig` reference oracle.
    "xev_thread_pool":   ([f"{GHXEV}/ThreadPool.zig"], [f"{RSXEV}/thread_pool.rs"], 0),
    # ---------------------------------------------------------------------
    # rustty-xev LINUX port (XEV_BLUEPRINT §1, tasks L1 LT1-LT8) — the epoll +
    # io_uring backends and the Dynamic selector that picks between them. These
    # three files were the "Other backends ... whole follow-up ports (§1)" bullet
    # of the C7 finale's DEFERRED list; LT8 lands them, so the rows move from
    # that ledger into this MAP ([ADV-FIX F7] precedent: the count-gate is what
    # makes R1 binding per module).
    #
    # SAME base path as every xev row above (GHXEV = the pinned content-addressed
    # libxev zig-cache package). No new base path or in-repo manifest is needed:
    # the Linux port's own read-only oracle checkout is the SAME libxev commit
    # this port tracks (34fa5087 — which WAS the ghostty pin when these rows
    # landed; ghostty 6ad1fe7d has since bumped to 9ce8e8e6, a reviewed non-port,
    # NON_PORTS.md §G3, and rustty-xev deliberately stays on 34fa5087), so these
    # rows bind against the existing GHXEV like their macOS siblings, and the R3
    # missing-ref guard below still protects them from vacuous greens.
    # ---------------------------------------------------------------------
    # backend/epoll.zig: 12 quoted `test "epoll: ..."` → zig=12. epoll.rs ports all
    # 12 as live #[test] + rustty KATs → rust=15 >= 12.
    "xev_epoll":         ([f"{GHXEV}/backend/epoll.zig"], [f"{RSXEV}/epoll.rs"], 0),
    # backend/io_uring.zig: 13 quoted `test "io_uring: ..."` → zig=13. io_uring.rs
    # ports all 13 as live #[test] + rustty KATs → rust=16 >= 13.
    "xev_io_uring":      ([f"{GHXEV}/backend/io_uring.zig"], [f"{RSXEV}/io_uring.rs"], 0),
    # dynamic.zig: the `Xev(bes)` multi-backend selector. 5 `test{}` total, of which
    # 4 are quoted (`completion is zero-able`, `detect`, `prefer`, `loop basics`)
    # and 1 is the anon `test { refAllDecls }` that zig_count skips → zig=4.
    # dynamic.rs ports all 5 (the anon one as the `dynamic_ref_all_decls`
    # compile-touch) + the ensure-tag/backend-default checks + 6 live KATs that run
    # BOTH candidate arms → rust=14 >= 4. NOTE dynamic.zig is comptime-elided on
    # macOS (§0, single candidate), so this row is only meaningful for the Linux
    # surface — but the COUNT binds on either host, since both sides are static text.
    "xev_dynamic":       ([f"{GHXEV}/dynamic.zig"], [f"{RSXEV}/dynamic.rs", f"{RSXEV}/dynamic/**/*.rs"], 0),
    # linux/timerfd.zig: the timerfd binding the epoll backend's timers ride. 1
    # behavioral named-unquoted `test` → zig=1; linux/timerfd.rs=1. `linux/eventfd.rs`
    # + `linux/net.rs` + `linux.rs` are intentionally UNMAPPED for the same reason
    # `net.rs` is: they have no 1:1 `.zig` reference oracle (libxev's eventfd lives
    # inline in watcher/async.zig's `AsyncEventFd`, and the other two are a
    # std.net/loop.zig-twin slice), so there is nothing to count them against.
    "xev_timerfd":       ([f"{GHXEV}/linux/timerfd.zig"], [f"{RSXEV}/linux/timerfd.rs"], 0),
    # watcher/file.zig (codex W13A-02): previously the ONE in-scope watcher input
    # the map never bound. Rustty has no file.rs — the File watcher twins live
    # INSIDE the backend files (epoll.rs/io_uring.rs `epoll_file_*`/
    # `io_uring_file_*`; macOS xev.File is a documented divergence, DEFERRALS.md
    # O10) — so the rust side of this row is the two backend files and OVER-COUNTS
    # (they also hold every non-file backend test). The count-bind is therefore
    # WEAK (bookkeeping, not proof); the row's real teeth are (a) the R3
    # missing-ref guard binding the CURRENT 9ce8e8e6 pin (GHXEV_CUR — the ONLY
    # revision carrying `test "queuePWrite"`, file.zig:318, whose final-content
    # "123000" KAT is ported as epoll_file_queue_pwrite_kat +
    # io_uring_file_queue_pwrite_kat), and (b) making a future file.zig test-count
    # jump past the combined backend total impossible to miss. Current pin zig=7
    # ("queuePWrite", "File: Stream decls", "kqueue: zero-length read for
    # readiness", "poll", "read/write", "pread/pwrite", "queued writes").
    "xev_file":          ([f"{GHXEV_CUR}/watcher/file.zig"], [f"{RSXEV}/epoll.rs", f"{RSXEV}/io_uring.rs"], 0),
    # ---------------------------------------------------------------------
    # WS20: the last two reference backends. These were previously UNMAPPED
    # because they were (wrongly) recorded as out-of-scope-by-platform. They are
    # ported, so they are counted like every other backend. Both files are
    # `#[cfg]`-gated to a target this host does not run, which is EXACTLY why the
    # count gate matters here: it is static text on both sides, so it binds on any
    # host even though the tests themselves cannot execute here.
    # ---------------------------------------------------------------------
    # backend/wasi_poll.zig: 8 quoted `test "wasi: ..."` (+1 anon `test {}` inside
    # Batch, which zig_count skips) → zig=8. wasi_poll.rs ports all 9 (the anon one
    # as `batch_get_put`; `wasi: file` as a documented skip-stub whose fixture needs
    # a WASI preopen — see VERIFICATION_DEBT.md) → rust=9 >= 8.
    "xev_wasi_poll":     ([f"{GHXEV}/backend/wasi_poll.zig"], [f"{RSXEV}/wasi_poll.rs"], 0),
    # backend/iocp.zig: 14 quoted `test "iocp: ..."` → zig=14. iocp.rs ports all 14
    # (9 live loop/timer/noop tests; the 2 file-IO and 3 socket tests as documented
    # skip-stubs whose FIXTURES need `sliceToPrefixedFileW` / Winsock startup — see
    # VERIFICATION_DEBT.md) → rust=14 >= 14.
    "xev_iocp":          ([f"{GHXEV}/backend/iocp.zig"], [f"{RSXEV}/iocp.rs"], 0),
    # `windows.rs` (libxev's own src/windows.zig) and `wasi.rs` are intentionally
    # UNMAPPED for the same reason `net.rs` and `linux.rs` are: windows.zig carries
    # 0 `test{}`, and wasi.rs is a std.os.wasi/loop.zig-twin slice with no 1:1
    # reference oracle.
    # ---------------------------------------------------------------------
    # P7-W0 T4 (Phase-7 start, the Phase-7 pin): test files the MAP could not see
    # (R5 §3 G1b; R1 §3 "Gate bookkeeping"). A gate that cannot see a new file is
    # bookkeeping green (memory derived-gate-green-can-be-bookkeeping), so every
    # row lands at phase start, before its port. A not-yet-ported Rust file is
    # named by a one-file GLOB (`<name>*.rs`), not a literal path: the literal
    # would trip the STALE-MAPPING arm and abort the whole gate, while the glob
    # expands to nothing and counts rust=0, so the row reads RED with the
    # upstream count until the port lands. Arrival state is in
    # `.port/phase-7/w0/README.md`.
    # ---------------------------------------------------------------------
    "datastruct_untouched_pool":   ([f"{GHSRC}/datastruct/untouched_pool.zig"], [f"{RS_ROOT}/crates/rustty-datastruct/src/untouched_pool*.rs"], 0),
    "datastruct_comptime_int_set": ([f"{GHSRC}/datastruct/comptime_int_set.zig"], [f"{RS_ROOT}/crates/rustty-datastruct/src/comptime_int_set*.rs"], 0),
    # Pre-existing (not new in the range), but never mapped; its `test WasmPagePool`
    # body changed in the range.
    "datastruct_wasm_page_pool":   ([f"{GHSRC}/datastruct/wasm_page_pool.zig"], [f"{RS_ROOT}/crates/rustty-datastruct/src/wasm_page_pool.rs"], 0),
    # parse_int.zig moves to rustty-lib (DECISIONS B4).
    "lib_parse_int":     ([f"{GHSRC}/lib/parse_int.zig"], [f"{RS_ROOT}/crates/rustty-lib/src/parse_int*.rs"], 0),
    "lib_types":         ([f"{GHSRC}/lib/types.zig"], [f"{RS_ROOT}/crates/rustty-lib/src/types.rs"], 0),
    "apprt_gtk_scale":   ([f"{GHAP}/gtk/scale.zig"], [f"{RSAPPRT}/gtk/scale*.rs"], 0),
    "font_nerd_font_attributes":([f"{GHF}/nerd_font_attributes.zig"], [f"{RSF}/nerd_font_attributes.rs"], 0),
    "snapshot_terminal": ([f"{GH}/snapshot/terminal.zig"], [f"{RS}/snapshot/terminal.rs"], 0),
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

def row_ok(z, r, apc):
    """The per-row verdict `main` applies: rust >= zig - apc_deferred."""
    return r >= z - apc


def self_test_rows():
    """S-10c arms (review S-10b F6), oracle-free: (1) the counting and the row verdict on
    fixture files — a zig fixture with 3 quoted tests, one named behavioural test, one
    named `_ =` reify test and one anonymous `test {}` counts 4, a rust fixture counts its
    `#[test]` lines, and with apc_deferred 1 three rust tests pass while two are RED;
    (2) the vaxis row still lists Vaxis.zig and the four vxfw files on the zig side, the
    vxfw rust glob, and exactly the one documented allowance."""
    import tempfile
    fails = []
    with tempfile.TemporaryDirectory() as d:
        zf = os.path.join(d, "a.zig")
        with open(zf, "w") as f:
            f.write('test "one" {\n}\ntest "two" {\n}\ntest "refAllDecls" {\n}\n'
                    'test Named {\n    try x();\n}\ntest Reify {\n    _ = X;\n}\ntest {\n}\n')
        rf = os.path.join(d, "a.rs")
        with open(rf, "w") as f:
            f.write("#[test]\nfn a() {}\n#[test]\nfn b() {}\n")
        z, r = zig_count([zf]), rust_count([rf])
        if z != 4:
            fails.append(f"zig_count fixture = {z}, expected 4")
        if r != 2:
            fails.append(f"rust_count fixture = {r}, expected 2")
        if row_ok(z, r, 1):
            fails.append("rust 2 vs zig 4 - apc 1 passed; expected RED")
        if not row_ok(z, r + 1, 1):
            fails.append("rust 3 vs zig 4 - apc 1 was RED; expected ok")
    zg, rg, apc = MAP["vaxis"]
    # A literal list, not VAXIS_VXFW_FILES: shrinking that constant must turn this RED.
    for f in ["Vaxis.zig", "vxfw/vxfw.zig", "vxfw/App.zig", "vxfw/Text.zig", "vxfw/Center.zig"]:
        if f"{GHVAXIS}/{f}" not in zg:
            fails.append(f"vaxis row lost {f}")
    if f"{RSVAXIS}/vxfw/*.rs" not in rg:
        fails.append("vaxis row lost the vxfw rust glob")
    if apc != 1:
        fails.append(f"vaxis apc_deferred = {apc}, expected exactly 1 (the doctest scan)")
    for m in fails:
        print(f"  FAIL {m}")
    print(f"  {'ok  ' if not fails else 'FAIL'} row counting + the vaxis/vxfw row binding")
    return not fails


def self_test_paths():
    """N483: path resolution, and the negative controls for the oracle check.

    A missing oracle and a wrong-pin oracle must each yield a reason (-> BLOCKED in
    `main()`), the right pin must yield none, and the zig cache must follow
    ZIG_GLOBAL_CACHE_DIR, then XDG_CACHE_HOME, then ~/.cache/zig."""
    import subprocess, tempfile
    fails = []

    def arm(ok, label):
        print(f"  {'ok  ' if ok else 'FAIL'} {label}")
        if not ok:
            fails.append(label)

    saved = {k: os.environ.pop(k, None) for k in ("ZIG_GLOBAL_CACHE_DIR", "XDG_CACHE_HOME")}
    try:
        arm(G.zig_cache_root() == os.path.join(os.path.expanduser("~"), ".cache", "zig"),
            "zig cache defaults to ~/.cache/zig")
        os.environ["XDG_CACHE_HOME"] = "/x/xdg"
        arm(G.zig_cache_root() == "/x/xdg/zig", "zig cache honours XDG_CACHE_HOME")
        os.environ["ZIG_GLOBAL_CACHE_DIR"] = "/x/zig"
        arm(G.zig_cache_root() == "/x/zig", "ZIG_GLOBAL_CACHE_DIR wins over XDG_CACHE_HOME")
    finally:
        for k, v in saved.items():
            os.environ.pop(k, None)
            if v is not None:
                os.environ[k] = v
    with tempfile.TemporaryDirectory() as td:
        fx = os.path.join(td, "ghostty")
        os.makedirs(fx)
        ident = ["-c", "user.name=t", "-c", "user.email=t@invalid", "-c", "commit.gpgsign=false"]
        G.oracle_git(fx, "init", "-q", check=True)
        G.oracle_git(fx, *ident, "commit", "-q", "--allow-empty", "-m", "f", check=True)
        head = G.oracle_git(fx, "rev-parse", "HEAD", check=True,
                            capture_output=True, text=True).stdout.strip()
        arm(G.oracle_pin_problem(fx, head) is None, "an oracle at the pin passes")
        arm(G.oracle_pin_problem(fx, "0" * 40) is not None, "NEGATIVE: a wrong-pin oracle is refused")
        arm(G.oracle_pin_problem(os.path.join(td, "absent"), head) is not None,
            "NEGATIVE: a missing oracle is refused")
        arm(G.oracle_pin_problem(td, head) is not None, "NEGATIVE: a non-checkout directory is refused")
        arm(G.oracle_pin_problem(fx, "") is not None, "NEGATIVE: a missing pin is refused")
    return self_test_main_pin_arm(arm) and not fails


def self_test_main_pin_arm(arm):
    """N483 fix round: the pin check must be CALLED by `main()`, not only correct.

    Runs a COPY of this script (and `_gatelib.py`) in a scratch checkout, end to end
    in a fresh process, beside an off-pin and beside a missing fixture oracle; each
    must exit 3 with the pin check's own reason. Either would also exit 3 through the
    missing-reference guard (the fixture oracle has no `src/`), so the arm binds the
    REASON: dropping the `oracle_pin_problem` call from `main()` turns it RED. The
    at-pin run is the control that the fixture reaches that later guard."""
    import shutil, subprocess, tempfile
    ok_all = True

    def run_copy(root):
        env = {k: v for k, v in os.environ.items() if not k.startswith("GIT_")}
        return subprocess.run([sys.executable, os.path.join(root, ".port", "tools", "gates",
                                                           "check_test_counts.py")],
                              cwd=root, env=env, capture_output=True, text=True)

    def check(ok, label):
        nonlocal ok_all
        arm(ok, label)
        ok_all = ok_all and ok

    with tempfile.TemporaryDirectory() as td:
        td = os.path.realpath(td)
        rs = os.path.join(td, "rustty")
        gates = os.path.join(rs, ".port", "tools", "gates")
        os.makedirs(gates)
        for f in ("check_test_counts.py", "_gatelib.py"):
            shutil.copy2(os.path.join(GATES_DIR, f), gates)
        ident = ["-c", "user.name=t", "-c", "user.email=t@invalid", "-c", "commit.gpgsign=false"]
        G.oracle_git(rs, "init", "-q", check=True)
        fx = os.path.join(td, "ghostty")
        pin_file = os.path.join(gates, "ORACLE_PIN")

        with open(pin_file, "w") as fh:
            fh.write("0" * 40 + "\n")
        r = run_copy(rs)
        check(r.returncode == 3 and "no oracle checkout at" in r.stdout,
              f"NEGATIVE: main() beside a MISSING oracle exits 3 on the pin check (rc {r.returncode})")

        os.makedirs(fx)
        G.oracle_git(fx, "init", "-q", check=True)
        G.oracle_git(fx, *ident, "commit", "-q", "--allow-empty", "-m", "f", check=True)
        head = G.oracle_git(fx, "rev-parse", "HEAD", check=True,
                            capture_output=True, text=True).stdout.strip()
        r = run_copy(rs)
        check(r.returncode == 3 and f"not the pin {'0' * 40}" in r.stdout,
              f"NEGATIVE: main() beside an OFF-PIN oracle exits 3 on the pin check (rc {r.returncode})")

        with open(pin_file, "w") as fh:
            fh.write(head + "\n")
        r = run_copy(rs)
        check(r.returncode == 3 and "missing zig reference file" in r.stdout
              and "not the pin" not in r.stdout and "no oracle checkout" not in r.stdout,
              f"control: main() beside an AT-PIN fixture passes the pin check (rc {r.returncode})")
    return ok_all


def self_test():
    """G1b's self-test (oracle-free, so ci.sh runs it before the oracle probe).

    The environment cannot move RS_ROOT: a fresh process imports this module
    with GIT_DIR + GIT_WORK_TREE naming a decoy repository and the decoy as its
    working directory, and RS_ROOT must still be this checkout. Restoring
    `git rev-parse --show-toplevel` turns this RED (it prints the decoy)."""
    sys.path.insert(0, GATES_DIR)
    import _gatelib as G
    ok, why = G.root_redirect_probe(GATES_DIR, "check_test_counts", ["RS_ROOT"], [RS_ROOT])
    print(f"  {'ok  ' if ok else 'FAIL'} GIT_DIR + GIT_WORK_TREE do not redirect RS_ROOT")
    if not ok:
        print(f"       ({why})")
        print("G1b SELF-TEST: RED")
        return 1
    if not self_test_paths():
        print("G1b SELF-TEST: RED")
        return 1
    if not self_test_rows():
        print("G1b SELF-TEST: RED")
        return 1
    print("G1b SELF-TEST: GREEN")
    return 0


def main():
    if "--self-test" in sys.argv[1:]:
        sys.exit(self_test())
    only = sys.argv[1] if len(sys.argv) > 1 else None
    # R3 guard against VACUOUS greens: every explicitly listed (non-wildcard)
    # zig reference file must exist. The zig side lives in external read-only
    # checkouts (the ghostty oracle + pinned zig-cache dep packages); if one is
    # absent/partial, its refs would silently glob/count to zig=0 and every
    # affected module would pass "rust >= 0" without checking anything. That
    # must never read as green — it is BLOCKED (exit 3; ci.sh surfaces it as a
    # WARNING, never a pass). This binds INSIDE the gate so no wrapper/probe
    # mismatch (e.g. an env override the probe honors but this script ignores)
    # can convert an absent oracle into a green.
    why = G.oracle_pin_problem(GH_ROOT)
    if why:
        print("TEST-COUNT GATE: BLOCKED — " + why + " (R3; N483): the oracle is the")
        print("`ghostty` checkout beside the main rustty checkout, at the ORACLE_PIN commit.")
        sys.exit(3)
    missing = []
    for mod, (zg, _rg, _apc) in MAP.items():
        if only and mod != only:
            continue
        missing += [f"  {mod}: {g}" for g in zg if "*" not in g and not os.path.exists(g)]
    if missing:
        print("TEST-COUNT GATE: BLOCKED — missing zig reference file(s); the external")
        print("oracle/dep checkouts are absent or incomplete (R3), so the per-module")
        print("counts cannot bind (a missing ref would vacuously count zig=0):")
        print("\n".join(missing))
        sys.exit(3)
    # WS19 T1 anti-vacuity guard, RUST side. Symmetric to the zig-side BLOCKED arm
    # above, but a DIFFERENT failure: a mapped RUST file that does not exist counts
    # rust=0, which is *silently wrong* rather than blocked — it either REDs for the
    # wrong reason (a rename read as "4 dropped tests", which is exactly what
    # cli/ghostty.rs -> cli/rustty.rs did at 410c387) or, when zig=0, passes green
    # while measuring nothing at all. A stale mapping is a GATE defect: name it as one.
    stale = []
    for mod, (_zg, rg, _apc) in MAP.items():
        if only and mod != only:
            continue
        stale += [f"  {mod}: {g}" for g in rg if "*" not in g and not os.path.exists(g)]
    if stale:
        print("TEST-COUNT GATE: RED — STALE MAPPING; mapped RUST file(s) do not exist.")
        print("These would count rust=0 and mis-report (or vacuously pass when zig=0).")
        print("Fix the MAP entry — do NOT delete the row:")
        print("\n".join(stale))
        sys.exit(1)
    bad = 0
    print("module            zig  rust  status")
    for mod, (zg, rg, apc) in MAP.items():
        if only and mod != only:
            continue
        z = zig_count(expand(zg)); r = rust_count(expand(rg))
        need = z - apc
        if not row_ok(z, r, apc):
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
