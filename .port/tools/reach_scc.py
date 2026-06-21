#!/usr/bin/env python3
"""
File-level reachability + SCC condensation for ghostty src/.

(1) Computes the TRUE ghostty-vt file set = runtime reachability from lib_vt.zig
    (the actual module root), and checks whether it leaks into font/renderer/
    config/global (it must not, or the crown-jewel crate isn't clean).
(2) Computes file-level SCCs (runtime-only, test-block imports excluded) over
    the whole src/ tree. Each SCC>1 is an irreducible cluster: the finest
    acyclic crate decomposition achievable with ZERO refactoring. Cross-cluster
    structure that *would* be a clean crate but is glued by a few edges is what
    we must break.
"""
import os, re, sys
from collections import defaultdict

SRC = os.environ.get("GHOSTTY_SRC", "/Users/gael/projects/ghostty/src")
IMPORT_RE = re.compile(r'@import\("([^"]+)"\)')
TEST_OPEN = re.compile(r'^\s*test\b.*\{\s*$')

files = []
for dp, _, fns in os.walk(SRC):
    for fn in fns:
        if fn.endswith(".zig"):
            files.append(os.path.relpath(os.path.join(dp, fn), SRC))
file_set = set(files)

# runtime (non-test) file edges
edges = defaultdict(set)
edges_all = defaultdict(set)
for rel in files:
    d = os.path.dirname(rel)
    in_test = False; depth = 0
    with open(os.path.join(SRC, rel), encoding="utf-8", errors="replace") as fh:
        for line in fh:
            if TEST_OPEN.match(line):
                in_test = True; depth = line.count("{") - line.count("}")
            elif in_test:
                depth += line.count("{") - line.count("}")
                if depth <= 0: in_test = False
            for m in IMPORT_RE.finditer(line):
                s = m.group(1)
                if not s.endswith(".zig"): continue
                cand = os.path.normpath(os.path.join(d, s))
                if cand not in file_set:
                    c2 = os.path.normpath(s); cand = c2 if c2 in file_set else cand
                if cand in file_set:
                    edges_all[rel].add(cand)
                    if not in_test: edges[rel].add(cand)

def reach(root, g):
    seen=set(); st=[root]
    while st:
        v=st.pop()
        if v in seen: continue
        seen.add(v)
        st.extend(g.get(v, ()))
    return seen

# ---- (1) TRUE vt set from lib_vt.zig (runtime only) -----------------------
# lib_vt.zig conditionally pulls terminal/c via comptime; include c too.
vt_root = "lib_vt.zig"
vt_reach = reach(vt_root, edges) if vt_root in file_set else set()
def topdir(f): return f.split("/")[0] if "/" in f else "(root)"
vt_dirs = defaultdict(int)
for f in vt_reach: vt_dirs[topdir(f)] += 1
print("===== (1) ghostty-vt runtime reachability from lib_vt.zig =====")
print(f"files in vt closure: {len(vt_reach)}")
for d,n in sorted(vt_dirs.items(), key=lambda x:-x[1]):
    print(f"   {n:4d}  {d}/")
# leak check: does vt touch these app dirs?
LEAK = ["font","renderer","config","apprt","inspector","termio","cli","crash"]
print("\n  leak check (vt files importing app modules):")
for f in sorted(vt_reach):
    for dst in edges.get(f, ()):
        td = topdir(dst)
        if td in LEAK or dst in ("global.zig",):
            print(f"    LEAK: {f} -> {dst}")

# ---- (1b) WHAT-IF: model hub breaks, recompute irreducible SCCs ------------
# Universal removals: any edge pointing to these targets is "injected" (DI/hook)
# or generated, so it no longer creates an up-edge.
BREAK_DST = {"global.zig"}
# Specific edge breaks (src -> dst).
BREAK_EDGE = {
    ("quirks.zig", "font/main.zig"),
    ("build_config.zig", "apprt.zig"),
    ("build_config.zig", "font/main.zig"),
    ("build_config.zig", "renderer.zig"),
    ("build_config.zig", "build/Config.zig"),
    ("font/SharedGrid.zig", "renderer.zig"),   # -> render-types
    ("os/open.zig", "apprt.zig"),
    ("terminal/main.zig", "terminal/c/main.zig"),  # vt -> vt-c (conditional)
}
# entry points are pure sources; drop edges INTO them
ENTRY = {f for f in files if f.startswith("main") or f == "lib_vt.zig" or f == "helpgen.zig"}
# Round-2 pattern breaks: (src_topdir_or_file, dst) -> break this up-edge via
# trait/DI/extraction. Models splitting the upper-half SCC into real crates.
BREAK_PATTERN = [
    # core -> apprt  (apprt-api trait; Surface/App/Command depend on the iface)
    ("App.zig", "apprt.zig"), ("Surface.zig", "apprt.zig"), ("Command.zig", "apprt.zig"),
    # font -> config (extract font-config types / DI)
    ("font/", "config.zig"), ("font/", "config/Config.zig"),
    # renderer -> apprt (renderer-surface trait)
    ("renderer/", "apprt.zig"),
    # renderer -> core (Surface/App refs via trait)
    ("renderer/", "Surface.zig"), ("renderer/", "App.zig"),
    # inspector -> core (debug overlay takes refs/trait)
    ("inspector/", "Surface.zig"), ("inspector/", "App.zig"),
    # renderer <-> inspector (State hook)
    ("renderer/", "inspector/main.zig"),
    # config -> cli (args extraction)
    ("config/", "cli.zig"),
    # termio -> apprt (termio-surface trait)
    ("termio/", "apprt.zig"),
]
def is_pattern_break(a, b):
    for (sp, d) in BREAK_PATTERN:
        if b == d and (a == sp or (sp.endswith("/") and a.startswith(sp))):
            return True
    return False

def broken_edges(round2=False):
    g = defaultdict(set)
    for a, outs in edges.items():
        for b in outs:
            if b in BREAK_DST: continue
            if (a, b) in BREAK_EDGE: continue
            if b in ENTRY: continue
            if round2 and is_pattern_break(a, b): continue
            g[a].add(b)
    return g

# ---- (2) file-level SCC condensation (runtime only) -----------------------
sys.setrecursionlimit(100000)
def tarjan(g, nodes):
    idx={};low={};on={};st=[];out=[];c=[0]
    def dfs(v):
        idx[v]=low[v]=c[0];c[0]+=1;st.append(v);on[v]=True
        for w in g.get(v,()):
            if w not in idx: dfs(w);low[v]=min(low[v],low[w])
            elif on.get(w): low[v]=min(low[v],idx[w])
        if low[v]==idx[v]:
            comp=[]
            while True:
                w=st.pop();on[w]=False;comp.append(w)
                if w==v:break
            out.append(comp)
    for v in nodes:
        if v not in idx: dfs(v)
    return out

# exclude entry points / build orchestration that only fan OUT (won't form SCCs
# but keep them; they're harmless source nodes). We analyze the full file graph.
sccs=[c for c in tarjan(edges, files) if len(c)>1]
sccs.sort(key=lambda c:-len(c))
print(f"\n===== (2) file-level SCCs (runtime-only): {len(sccs)} with size>1 =====")
for i,comp in enumerate(sccs):
    dirs=defaultdict(int)
    for f in comp: dirs[topdir(f)]+=1
    span=", ".join(f"{d}:{n}" for d,n in sorted(dirs.items(),key=lambda x:-x[1]))
    print(f"\n  SCC#{i} size={len(comp)}  spans[{span}]")
    if len(comp)<=12:
        for f in sorted(comp): print(f"      {f}")

# ---- (3) condensation: treat each SCC as a node, show DAG layering ---------
comp_id={}
for i,comp in enumerate(tarjan(edges, files)):
    for f in comp: comp_id[f]=i
print(f"\ntotal file-SCC nodes (incl singletons): {len(set(comp_id.values()))}")

# ---- (4) SCCs AFTER modeled hub-breaks ------------------------------------
g2 = broken_edges()
sccs2 = [c for c in tarjan(g2, files) if len(c) > 1]
sccs2.sort(key=lambda c: -len(c))
print(f"\n===== (4) file-level SCCs AFTER modeled hub-breaks: {len(sccs2)} with size>1 =====")
print(f"    (broke: edges->global.zig, quirks->font, build_config->{{apprt,font,renderer}},")
print(f"     font/SharedGrid->renderer, os/open->apprt, terminal/main->terminal/c)")
for i, comp in enumerate(sccs2):
    dirs = defaultdict(int)
    for f in comp: dirs[topdir(f)] += 1
    span = ", ".join(f"{d}:{n}" for d, n in sorted(dirs.items(), key=lambda x:-x[1]))
    print(f"\n  SCC#{i} size={len(comp)}  spans[{span}]")
    if len(comp) <= 20:
        for f in sorted(comp): print(f"      {f}")

# ---- (5) SCCs AFTER round-2 (upper-half) breaks ---------------------------
g3 = broken_edges(round2=True)
sccs3 = [c for c in tarjan(g3, files) if len(c) > 1]
sccs3.sort(key=lambda c: -len(c))
print(f"\n===== (5) file-level SCCs AFTER round-2 upper-half breaks: {len(sccs3)} =====")
for i, comp in enumerate(sccs3):
    dirs = defaultdict(int)
    for f in comp: dirs[topdir(f)] += 1
    span = ", ".join(f"{d}:{n}" for d, n in sorted(dirs.items(), key=lambda x:-x[1]))
    print(f"  SCC#{i} size={len(comp)}  spans[{span}]")
