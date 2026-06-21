#!/usr/bin/env python3
"""
rustty crate-graph cycle checker.

Builds the resolved file-level @import graph for ghostty's src/, maps every
file to its PROPOSED rustty crate, collapses to a crate-level graph, and runs
Tarjan SCC detection. Any SCC with >1 crate is a cross-crate cycle (illegal in
Cargo). For each cyclic crate pair it prints the concrete file edges in both
directions so the break point is unambiguous.

This is a PARITY GATE: once crates exist, re-run against the rustty tree (or
keep checking the reference mapping) and fail CI if any unexpected SCC appears.
"""
import os, re, sys
from collections import defaultdict

SRC = os.environ.get("GHOSTTY_SRC", "/Users/gael/projects/ghostty/src")
IMPORT_RE = re.compile(r'@import\("([^"]+)"\)')
TEST_OPEN = re.compile(r'^\s*test\b.*\{\s*$')

# ---- proposed file -> crate mapping ---------------------------------------
# Single-file extractions (the planned cycle-breaks): these files move to a new
# low crate, so any import OF them routes to that crate automatically.
EXTRACT = {
    "renderer/size.zig": "render-types",
    "config/formatter.zig": "cfg-fmt",
    "cli/args.zig": "args",
    "cli/diagnostics.zig": "args",
}
# The VT input slice that lib_vt.zig pulls into the ghostty-vt module.
VT_INPUT = {
    "input/key.zig", "input/key_encode.zig", "input/mouse.zig",
    "input/mouse_encode.zig", "input/paste.zig",
}
DIRMAP = {
    "terminal": "vt", "datastruct": "datastruct", "unicode": "unicode",
    "simd": "simd", "os": "os", "crash": "crash", "lib": "lib",
    "font": "font", "input": "input", "inspector": "inspector",
    "renderer": "renderer", "config": "config", "terminfo": "terminfo",
    "termio": "termio", "cli": "cli", "apprt": "apprt",
    "synthetic": "synthetic", "benchmark": "bench", "build": "xtask",
    "extra": "extra", "stb": "vt", "shell-integration": "resources",
}
ROOTZIG = {
    "apprt.zig": "apprt", "config.zig": "config", "input.zig": "input",
    "renderer.zig": "renderer", "terminal.zig": "vt", "termio.zig": "termio",
    "cli.zig": "cli",
}
UTIL = {"math.zig", "fastmem.zig", "file_type.zig", "tripwire.zig", "quirks.zig"}
CORE = {"App.zig", "Surface.zig", "Command.zig", "surface_mouse.zig", "global.zig"}

def crate_of(rel):
    if rel in EXTRACT: return EXTRACT[rel]
    if rel in VT_INPUT: return "vt"
    if rel.startswith("terminal/c/") or rel == "terminal/c.zig": return "vt-c"
    top = rel.split("/")[0]
    if "/" in rel and top in DIRMAP: return DIRMAP[top]
    if rel in ROOTZIG: return ROOTZIG[rel]
    if rel in UTIL: return "util"
    if rel in CORE: return "core"
    if rel == "pty.zig": return "os"
    if rel == "build_config.zig": return "build-config"
    if rel == "helpgen.zig": return "xtask"
    if rel.startswith("main") or rel == "lib_vt.zig": return "bin"
    return None  # unmapped

# ---- collect files & imports ----------------------------------------------
files = []
for dirpath, _, fnames in os.walk(SRC):
    for fn in fnames:
        if fn.endswith(".zig"):
            files.append(os.path.relpath(os.path.join(dirpath, fn), SRC))

file_set = set(files)
# file -> list of (target_rel, lineno, in_test)
edges_file = defaultdict(list)
external = defaultdict(set)   # crate -> set(named external modules)
unresolved = []

for rel in files:
    abspath = os.path.join(SRC, rel)
    d = os.path.dirname(rel)
    in_test = False
    depth = 0
    try:
        with open(abspath, encoding="utf-8", errors="replace") as fh:
            for i, line in enumerate(fh, 1):
                # crude test-block tracking (heuristic)
                if TEST_OPEN.match(line):
                    in_test = True; depth = line.count("{") - line.count("}")
                elif in_test:
                    depth += line.count("{") - line.count("}")
                    if depth <= 0: in_test = False
                for m in IMPORT_RE.finditer(line):
                    s = m.group(1)
                    if s in ("std", "builtin", "root"): continue
                    if s.endswith(".zig"):
                        cand = os.path.normpath(os.path.join(d, s))
                        if cand not in file_set:
                            cand2 = os.path.normpath(s)
                            cand = cand2 if cand2 in file_set else cand
                        if cand in file_set:
                            edges_file[rel].append((cand, i, in_test))
                        else:
                            unresolved.append((rel, s, i))
                    else:
                        external[crate_of(rel) or "?"].add(s)
    except Exception as e:
        print(f"!! read error {rel}: {e}", file=sys.stderr)

# ---- collapse to crate graph ----------------------------------------------
unmapped = sorted({f for f in files if crate_of(f) is None})
# crate -> crate -> list of (src_file, dst_file, line, in_test)
cedges = defaultdict(lambda: defaultdict(list))
crates = set()
for src, lst in edges_file.items():
    cs = crate_of(src)
    if cs is None: continue
    crates.add(cs)
    for (dst, ln, intest) in lst:
        cd = crate_of(dst)
        if cd is None or cd == cs: continue
        crates.add(cd)
        cedges[cs][cd].append((src, dst, ln, intest))

def crate_adj(include_test=True):
    adj = defaultdict(set)
    for a, m in cedges.items():
        for b, lst in m.items():
            if include_test or any(not t for (_,_,_,t) in lst):
                adj[a].add(b)
    return adj

# ---- Tarjan SCC -----------------------------------------------------------
def tarjan(adj, nodes):
    idx = {}; low = {}; onstk = {}; stk = []; out = []; c = [0]
    import sys as _s; _s.setrecursionlimit(10000)
    def dfs(v):
        idx[v]=low[v]=c[0]; c[0]+=1; stk.append(v); onstk[v]=True
        for w in adj.get(v, ()):
            if w not in idx:
                dfs(w); low[v]=min(low[v],low[w])
            elif onstk.get(w): low[v]=min(low[v],idx[w])
        if low[v]==idx[v]:
            comp=[]
            while True:
                w=stk.pop(); onstk[w]=False; comp.append(w)
                if w==v: break
            out.append(comp)
    for v in nodes:
        if v not in idx: dfs(v)
    return out

def report(title, include_test):
    adj = crate_adj(include_test)
    sccs = [c for c in tarjan(adj, sorted(crates)) if len(c) > 1]
    print(f"\n========== {title} ==========")
    print(f"crates: {len(crates)}   cross-crate SCCs (size>1): {len(sccs)}")
    for comp in sccs:
        cs = set(comp)
        print(f"\n  CYCLE: {{{', '.join(sorted(comp))}}}")
        for a in sorted(comp):
            for b in sorted(cedges[a]):
                if b in cs:
                    exs = cedges[a][b]
                    if not include_test:
                        exs = [e for e in exs if not e[3]]
                    if not exs: continue
                    tag = "  (TEST-ONLY)" if all(e[3] for e in exs) else ""
                    print(f"    {a} -> {b}{tag}")
                    for (sf, df, ln, t) in exs[:4]:
                        tt = " [test]" if t else ""
                        print(f"        {sf}:{ln} -> {df}{tt}")
    return sccs

print(f"src={SRC}")
print(f"zig files: {len(files)}   mapped crates: {len(crates)}")
if unmapped:
    print(f"\nUNMAPPED FILES ({len(unmapped)}):")
    for f in unmapped: print(f"   {f}")
if unresolved:
    print(f"\nUNRESOLVED IMPORTS ({len(unresolved)}) (first 20):")
    for (f,s,l) in unresolved[:20]: print(f"   {f}:{l} -> {s}")

report("ALL IMPORTS (incl. tests)", include_test=True)
report("RUNTIME ONLY (excl. test-block imports)", include_test=False)

# Full mutual-pair (direct 2-cycle) inventory, runtime only
print("\n========== DIRECT MUTUAL PAIRS (runtime) ==========")
seen=set()
for a in sorted(cedges):
    for b in sorted(cedges[a]):
        if b in cedges and a in cedges[b] and (b,a) not in seen:
            seen.add((a,b))
            ra=[e for e in cedges[a][b] if not e[3]]
            rb=[e for e in cedges[b][a] if not e[3]]
            if ra and rb:
                print(f"  {a} <-> {b}")
