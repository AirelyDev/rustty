"""Data builder for the "Cutting the cycle" widget (slug cutting-the-cycle).

Ports the data step of the mockup build (mockup-build/w04/build.py inlines the
embed file into the template) onto the persisted research files:

- research/data/mock-cutting-the-cycle-embed.json  written by mock-cutting-the-cycle-compute.py
  (the 345-file import cycle at Ghostty 4749c4e93 with the radial layout, edge tags,
  shatter curve and leave-one-out crate cycles; the 61-package crate graph with node
  metrics and the y-positions from mock-cutting-the-cycle-relax.py; the daily replay)

It adds what the mockup kept in its script:

- zig.plan   [[edge index, planned cut], …] for every edge the cycle-analysis plan's
             predicates assign to cuts 1-14 (compute.py `brk`); the shatter curve is
             computed from these, not from the display tags (which also mark two
             resolved edges outside the plan, e.g. formatter → key under cut 6)
- zig.entry  indices of entry-point files (main*.zig, lib_vt.zig, helpgen.zig), whose
             incoming edges the curve treats as absent (entry points as pure sources)
- crates.cratesIo  crates.io packages resolved and direct (research/codebase.md)
- text       the plain-words cut, extra-seam and layering-decision notes (sources:
             .port/cycle-analysis.md, PORT_PLAN.md, crate port notes,
             codebase-crate-graph.json layering_decisions)
"""
import json
from pathlib import Path

import jargon

SLUG = "cutting-the-cycle"

# CUTS are the 14 planned cuts: number, short name, mechanism (trait, move, split,
# inject), the crate that carries the cut (short name) and a plain explanation.
CUTS = [
    {"n": 1, "name": "global.zig, the hub", "m": "split", "cr": "global",
     "d": "Process-wide state. 23 files in the cycle import it from below (terminal, os, crash, renderer, termio, config, cli), and it imports apprt, cli, crash, os and renderer from above. The state stays in a leaf crate (rustty-global, set once at startup). The top-down wiring moved up into core."},
    {"n": 2, "name": "quirks.zig → font", "m": "trait", "cr": "util",
     "d": "quirks.zig is the assert helper, imported by 113 files in the cycle. A single font quirk made it import font. In Rust that quirk takes a small trait (quirks::Face), so rustty-util stays a true leaf."},
    {"n": 3, "name": "build_config → apprt, font, renderer", "m": "move", "cr": "build-config",
     "d": "The build config pulled the backend enums (app runtime, renderer, font backend) up from the subsystems. They are now defined low, in rustty-build-config, and imported downward."},
    {"n": 4, "name": "terminal/c split out", "m": "split", "cr": "vt-c",
     "d": "The C ABI layer became rustty-vt-c, a crate that sits on top of rustty-vt. The terminal engine can no longer reach its own C bindings."},
    {"n": 5, "name": "renderer/size.zig extracted", "m": "move", "cr": "render-types",
     "d": "Grid and screen size types moved into rustty-render-types, a leaf shared by font, renderer, config, input and the C API. terminal/size.zig moved with them."},
    {"n": 6, "name": "config/formatter.zig extracted", "m": "move", "cr": "cfg-fmt",
     "d": "The config formatter moved into rustty-cfg-fmt, below both config and input, which breaks the config ↔ input loop. Its import of config/key.zig was unused there, so it was dropped."},
    {"n": 7, "name": "cli/args.zig extracted", "m": "move", "cr": "args",
     "d": "Argument parsing (args, diagnostics, CommaSplitter) moved into rustty-args, so config can parse flags without depending on the CLI."},
    {"n": 8, "name": "apprt interface crate", "m": "trait", "cr": "apprt-api",
     "d": "The app-runtime interface (actions, surface messages, ipc, value structs) became rustty-apprt-api. Core depends on its traits (App, Surface, CoreSurfaceHandle), not on the GTK or macOS backends. Command.zig moved into termio, because leaving it in core would create a new cycle."},
    {"n": 9, "name": "font ↔ config", "m": "inject", "cr": "font-shaper",
     "d": "Font read a handful of config types. The shaper tier models only the subset it uses, and config supplies the real values by injection, so font never depends on config."},
    {"n": 10, "name": "renderer → apprt, core", "m": "trait", "cr": "renderer",
     "d": "The renderers (Metal, OpenGL, Thread) reached up into apprt and into core's Surface and App. They now go through a surface trait."},
    {"n": 11, "name": "renderer ↔ inspector", "m": "trait", "cr": "renderer",
     "d": "The render state held a pointer to the inspector. The renderer now defines an InspectorOverlay trait that the inspector implements, so the dependency runs only from inspector to renderer."},
    {"n": 12, "name": "inspector → core", "m": "trait", "cr": "inspector",
     "d": "The inspector took core's Surface directly. It now receives the surface data it reads through a narrow seam, without naming any core type."},
    {"n": 13, "name": "termio → apprt", "m": "trait", "cr": "termio",
     "d": "The terminal I/O thread posted messages straight to the app runtime. It now uses the mailbox seam in rustty-apprt-api."},
    {"n": 14, "name": "os/open.zig → apprt", "m": "move", "cr": "os",
     "d": "A single import from the URL-opening helper up into apprt. It was relocated so that rustty-os stays near the bottom of the graph."},
]

# EXTRA are the seams outside the 14 planned cuts that the Rust graph also needed,
# found edge by edge (numbers 15-19 match the edge tags in the embed file).
EXTRA = [
    {"n": 15, "name": "crash/sentry.zig → Surface", "m": "trait", "cr": "crash",
     "d": "Not one of the original 14. After the 14 cuts, a 45-file knot (renderer, inspector, termio, core, crash) still held together, and this was the only edge pointing up out of it. The crash reporter now reads surface metadata through a local ThreadStateSurface trait that core's Surface implements."},
    {"n": 16, "name": "font → font-shaper (three-tier font)", "m": "split", "cr": "font",
     "d": "The pure font crate (OpenType parsing, sprites, atlas, metrics) sits below the terminal engine and never depends on it. Shaping, backend faces and the shared grid sit above, in rustty-font-shaper (decided 06-25). These 10 edges were re-exports in font/main.zig that now point up."},
    {"n": 17, "name": "renderer → config", "m": "trait", "cr": "renderer",
     "d": "The renderer reads configuration through a RendererConfig accessor trait it owns. rustty-config implements that trait, so renderer never names a config type and the edge flips to config → renderer."},
    {"n": 18, "name": "VT input slice", "m": "move", "cr": "vt",
     "d": "lib_vt pulls five input/ files into the terminal engine. Their helpers (key_mods, function_keys, kitty) moved into rustty-vt's input module too."},
    {"n": 19, "name": "two edges with no Rust dependency", "m": "other", "cr": None,
     "d": "datastruct/split_tree → build_config (a GTK-only member) and renderer/Thread → crash. Neither has a matching crate dependency in today's graph. This widget doesn't trace the individual mechanism."},
]

# LAYER are the layering decisions (codebase-crate-graph.json layering_decisions)
# in plain words: the crate edges they explain (e), the text (t) and the crates
# whose detail panel lists them (c).
LAYER = [
    {"e": [["rustty-vt", "rustty-font"], ["rustty-font-shaper", "rustty-vt"], ["rustty-font-shaper", "rustty-font"]],
     "t": "Font sits below the terminal engine and never depends on it; shaping (CoreText, FreeType, HarfBuzz) sits above it in font-shaper.",
     "c": ["rustty-font", "rustty-font-shaper", "rustty-vt"]},
    {"e": [["rustty-config", "rustty-renderer"]],
     "t": "The renderer owns a RendererConfig accessor trait that config implements, so the renderer never depends on config.",
     "c": ["rustty-renderer", "rustty-config"]},
    {"e": [["rustty-core", "rustty-termio"]],
     "t": "Process spawning (Command.zig) lives in termio, not core, because placing it in core created a core ↔ termio cycle.",
     "c": ["rustty-termio", "rustty-core"]},
    {"e": [["rustty-vt-c", "rustty-vt"]],
     "t": "The C ABI is a sink over the terminal engine (cut 4).",
     "c": ["rustty-vt-c", "rustty-vt"]},
    {"e": [["rustty-core", "rustty-apprt-api"], ["rustty-apprt", "rustty-core"]],
     "t": "Core depends on the app-runtime interface crate, not on the app backends (cut 8).",
     "c": ["rustty-core", "rustty-apprt-api", "rustty-apprt"]},
    {"e": [["rustty-xev", "rustty-global"]],
     "t": "The global hub became a leaf; its wiring moved into core (cut 1).",
     "c": ["rustty-global", "rustty-core"]},
]

# CRATES_IO is the external crates.io package count of the Cargo graph (research/codebase.md,
# MEASURED with cargo metadata, confirmed in its fact-check): 138 packages resolved, 37
# distinct direct normal/build dependencies.
CRATES_IO = {"resolved": 138, "direct": 37}

# UNSAFE_FIXES corrects per-crate unsafe-site counts to the fact-checked census
# (research/codebase.md fact-check, row 7): the census script counted one untracked
# fuzz build artifact under crates/rustty-vt, which really has 7,984 sites, not 7,985.
UNSAFE_FIXES = {"rustty-vt": (7985, 7984)}

# VENDORED_SYS_RS removes vendored upstream .rs files from the -sys line series
# (timeline bucket rs:crates-sys counts every .rs under crates-sys/, vendor/ included).
# Measured with `git grep -c ''` over crates-sys/*/vendor/*.rs and crates-sys/*/cbits/*.rs
# at each of the 110 daily revisions: 0 lines before 2026-06-30, then exactly 1,432
# (three HarfBuzz lib.rs files under crates-sys/harfbuzz-sys/vendor/) on every day from
# 2026-06-30 (2e8277608) through 2026-10-08 (1d22ba80d). With them removed the last day
# totals 836,905 physical lines, the fact-checked tracked non-vendor count (BRIEF rows 6, 43).
VENDORED_SYS_RS = {"from": "2026-06-30", "lines": 1432}

# CRATE_CUTS lists, per crate, the cuts that shaped it beyond the ones it carries
# (the mockup's CUT_OF_CRATE additions): core got cut 1's wiring and cut 8's traits,
# config injects cut 9 and implements cut 17, inspector implements cut 11, apprt cut 8.
CRATE_CUTS = {
    "rustty-core": [1, 8],
    "rustty-config": [9, 17],
    "rustty-inspector": [11],
    "rustty-apprt": [8],
}


def planned_cut(a: str, b: str) -> int:
    """Return the planned cut (1-14) whose cycle-analysis predicate matches file edge a → b, or 0.

    A port of compute.py `brk` (mirrors reach_scc.py BREAK_* and the cycle-analysis table).
    """
    if b == "global.zig":
        return 1
    if (a, b) == ("quirks.zig", "font/main.zig"):
        return 2
    if a == "build_config.zig" and b in ("apprt.zig", "font/main.zig", "renderer.zig", "build/Config.zig"):
        return 3
    if a.startswith("terminal/") and not a.startswith("terminal/c/") and (b.startswith("terminal/c/") or b == "terminal/c.zig"):
        return 4
    if (a, b) == ("font/SharedGrid.zig", "renderer.zig"):
        return 5
    if a.startswith("input") and b in ("config/formatter.zig", "config/key.zig", "config.zig", "config/Config.zig"):
        return 6
    if a.startswith("config/") and b in ("cli.zig", "cli/args.zig", "cli/diagnostics.zig", "cli/CommaSplitter.zig"):
        return 7
    if a in ("App.zig", "Surface.zig", "Command.zig") and b == "apprt.zig":
        return 8
    if a.startswith("font/") and b in ("config.zig", "config/Config.zig"):
        return 9
    if a.startswith("renderer/") and b in ("apprt.zig", "Surface.zig", "App.zig"):
        return 10
    if a.startswith("renderer/") and b == "inspector/main.zig":
        return 11
    if a.startswith("inspector/") and b in ("Surface.zig", "App.zig"):
        return 12
    if a.startswith("termio/") and b == "apprt.zig":
        return 13
    if (a, b) == ("os/open.zig", "apprt.zig"):
        return 14
    return 0


def is_entry(name: str) -> bool:
    """Return whether a src/ file is an entry point (main*.zig, lib_vt.zig, helpgen.zig)."""
    return name.startswith("main") or name in ("lib_vt.zig", "helpgen.zig")


def _clean_text(node, changes: list):
    """Return node with every string stripped of internal ids, recording each change."""
    if isinstance(node, str):
        out, changed = jargon.strip_internal_ids(node)
        if changed:
            changes.append((node, out))
        return out
    if isinstance(node, dict):
        return {k: _clean_text(v, changes) for k, v in node.items()}
    if isinstance(node, list):
        return [_clean_text(v, changes) for v in node]
    return node


def _without_vendored_sys(replay: dict) -> dict:
    """Return the replay with VENDORED_SYS_RS subtracted from the -sys series on the days it applies."""
    start, values = replay["s"]["__sys"]
    dates = replay["dates"]
    fixed = [v - VENDORED_SYS_RS["lines"] if dates[start + i] >= VENDORED_SYS_RS["from"] else v
             for i, v in enumerate(values)]
    if min(fixed) < 0:
        raise ValueError(f"{SLUG}: vendored .rs correction exceeds the -sys series")
    return {**replay, "s": {**replay["s"], "__sys": [start, fixed]}}


def build(research: Path, mockbuild: Path) -> dict:
    """Return the cutting-the-cycle widget data (see module docstring for sources)."""
    emb = json.loads((Path(research) / "data" / "mock-cutting-the-cycle-embed.json").read_text())
    changes: list = []
    z = emb["zig"]
    names = [f[0] for f in z["files"]]
    plan = [[i, k] for i, (a, b, _t) in enumerate(z["edges"]) if (k := planned_cut(names[a], names[b]))]
    entry = [i for i, n in enumerate(names) if is_entry(n)]
    nodes = [list(n) for n in emb["crates"]["nodes"]]
    for n in nodes:
        if n[0] in UNSAFE_FIXES:
            was, now = UNSAFE_FIXES[n[0]]
            if n[22] != was:
                raise ValueError(f"{SLUG}: {n[0]} unsafe count is {n[22]}, expected {was} before the fix")
            n[22] = now
            n[23] = round(now / n[3] * 1000, 2)
    replay = _without_vendored_sys(emb["replay"])
    text = _clean_text({"cuts": CUTS, "extra": EXTRA, "layer": LAYER, "crateCuts": CRATE_CUTS}, changes)
    _record_review(changes)
    return {
        "_meta": {
            "slug": SLUG,
            "generatedFrom": [
                "research/data/mock-cutting-the-cycle-embed.json (compute.py + relax.py)",
                "scripts/porting/widgets/w04_cutting_the_cycle.py (cut predicates and notes)",
                "research/codebase.md (crates.io package counts; fact-checked unsafe count of rustty-vt)",
                "git grep over rustty crates-sys/*/vendor and */cbits .rs per daily revision (vendored lines removed from the -sys series)",
            ],
            "asOf": "4749c4e93 · 1d22ba80d",
        },
        "zig": {**z, "plan": plan, "entry": entry},
        "crates": {**emb["crates"], "nodes": nodes, "cratesIo": CRATES_IO},
        "replay": replay,
        "text": text,
    }


def _record_review(changes: list) -> None:
    """Append each (before, after) rewrite to jargon-review.txt once, for the hand review."""
    path = Path(__file__).resolve().parent.parent / "jargon-review.txt"
    existing = set(path.read_text().splitlines()) if path.exists() else set()
    lines = [f"{SLUG}\t{b}\t{a}" for b, a in dict.fromkeys(changes)]
    new = [ln for ln in lines if ln not in existing]
    if new:
        with open(path, "a") as f:
            f.write("".join(ln + "\n" for ln in new))
