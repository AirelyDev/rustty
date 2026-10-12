#!/usr/bin/env python3
"""Compute data for widget 04 "Cutting the cycle".
Arg1: path to ghostty src/ extracted at 4749c4e93 (starting pin).
Arg2: path to ghostty src/ at a60e9e2a5 (current pin), for comparison.
Writes JSON to research/data/mock-cutting-the-cycle-*.json
Parsing mirrors .port/tools/reach_scc.py (runtime-only @import edges, test-block imports excluded).
"""
import os, re, sys, json, csv, math, random
from collections import defaultdict, deque

SRC, SRC_PIN, OUT = sys.argv[1], sys.argv[2], sys.argv[3]
DATA = "/Users/gael/projects/rustty/.port/journey-article/research/data"
IMPORT_RE = re.compile(r'@import\("([^"]+)"\)')
TEST_OPEN = re.compile(r'^\s*test\b.*\{\s*$')
sys.setrecursionlimit(100000)

def load(src):
    files = []
    for dp, _, fns in os.walk(src):
        for fn in fns:
            if fn.endswith(".zig"):
                files.append(os.path.relpath(os.path.join(dp, fn), src))
    fs = set(files)
    edges = defaultdict(set); lines = {}
    for rel in files:
        d = os.path.dirname(rel); in_test = False; depth = 0; n = 0
        with open(os.path.join(src, rel), encoding="utf-8", errors="replace") as fh:
            for line in fh:
                n += 1
                if TEST_OPEN.match(line):
                    in_test = True; depth = line.count("{") - line.count("}")
                elif in_test:
                    depth += line.count("{") - line.count("}")
                    if depth <= 0: in_test = False
                for m in IMPORT_RE.finditer(line):
                    s = m.group(1)
                    if not s.endswith(".zig"): continue
                    cand = os.path.normpath(os.path.join(d, s))
                    if cand not in fs:
                        c2 = os.path.normpath(s); cand = c2 if c2 in fs else cand
                    if cand in fs and not in_test:
                        edges[rel].add(cand)
        lines[rel] = n
    return files, edges, lines

def tarjan(g, nodes):
    idx = {}; low = {}; on = {}; st = []; out = []; c = [0]
    def dfs(v):
        idx[v] = low[v] = c[0]; c[0] += 1; st.append(v); on[v] = True
        for w in g.get(v, ()):
            if w not in idx: dfs(w); low[v] = min(low[v], low[w])
            elif on.get(w): low[v] = min(low[v], idx[w])
        if low[v] == idx[v]:
            comp = []
            while True:
                w = st.pop(); on[w] = False; comp.append(w)
                if w == v: break
            out.append(comp)
    for v in nodes:
        if v not in idx: dfs(v)
    return out

files, edges, lines = load(SRC)
sccs = sorted([c for c in tarjan(edges, files) if len(c) > 1], key=len, reverse=True)
big = set(sccs[0])
print("files", len(files), "SCC", len(big), "other sccs", [len(c) for c in sccs[1:]])
E = sorted((a, b) for a in big for b in edges[a] if b in big)
print("edges inside SCC", len(E), "all runtime edges", sum(len(v) for v in edges.values()))

# compare at current pin
f2, e2, l2 = load(SRC_PIN)
s2 = sorted([c for c in tarjan(e2, f2) if len(c) > 1], key=len, reverse=True)
pin_stats = {"files": len(f2), "scc": len(s2[0]),
             "scc_edges": sum(1 for a in s2[0] for b in e2[a] if b in set(s2[0]))}
print("pin", pin_stats)

def topdir(f): return f.split("/")[0] if "/" in f else "(root)"

# ---------------- file -> Rust crate (cycle_check.py crate_of + plan refinements) ----------------
EXTRACT = {"renderer/size.zig": "render-types", "terminal/size.zig": "render-types",
           "config/formatter.zig": "cfg-fmt", "cli/args.zig": "args", "cli/diagnostics.zig": "args",
           "cli/CommaSplitter.zig": "args", "global.zig": "global", "Command.zig": "termio",
           "helpgen.zig": "helpgen"}
APPRT_API = {"apprt/action.zig", "apprt/ipc.zig", "apprt/runtime.zig", "apprt/surface.zig", "apprt/structs.zig"}
SHAPER_FILES = {"font/face.zig", "font/shape.zig", "font/discovery.zig", "font/Collection.zig",
                "font/CodepointResolver.zig", "font/SharedGrid.zig", "font/SharedGridSet.zig",
                "font/DeferredFace.zig", "font/backend.zig", "font/library.zig"}
VT_INPUT = {"input/key.zig", "input/key_encode.zig", "input/mouse.zig", "input/mouse_encode.zig", "input/paste.zig"}
DIRMAP = {"terminal": "vt", "datastruct": "datastruct", "unicode": "unicode", "simd": "simd", "os": "os",
          "crash": "crash", "lib": "lib", "font": "font", "input": "input", "inspector": "inspector",
          "renderer": "renderer", "config": "config", "terminfo": "terminfo", "termio": "termio",
          "cli": "cli", "apprt": "apprt", "synthetic": "synthetic", "benchmark": "bench", "build": "xtask",
          "extra": "extra", "stb": "vt"}
ROOTZIG = {"apprt.zig": "apprt", "config.zig": "config", "input.zig": "input", "renderer.zig": "renderer",
           "terminal.zig": "vt", "termio.zig": "termio", "cli.zig": "cli"}
UTIL = {"math.zig", "fastmem.zig", "file_type.zig", "tripwire.zig", "quirks.zig"}
CORE = {"App.zig", "Surface.zig", "surface_mouse.zig"}
def crate_of(rel):
    if rel in EXTRACT: return EXTRACT[rel]
    if rel in APPRT_API: return "apprt-api"
    if rel in SHAPER_FILES or rel.startswith("font/shaper/") or rel.startswith("font/face/"): return "font-shaper"
    if rel in VT_INPUT: return "vt"
    if rel.startswith("terminal/c/") or rel == "terminal/c.zig": return "vt-c"
    top = rel.split("/")[0]
    if "/" in rel and top in DIRMAP: return DIRMAP[top]
    if rel in ROOTZIG: return ROOTZIG[rel]
    if rel in UTIL: return "util"
    if rel in CORE: return "core"
    if rel == "pty.zig": return "os"
    if rel == "build_config.zig": return "build-config"
    if rel.startswith("main") : return "rustty"
    if rel == "lib_vt.zig": return "vt-c"
    return None
# ---------------- break assignment (mirrors reach_scc.py BREAK_* + cycle-analysis table) ----------
def brk(a, b):
    if b == "global.zig": return 1
    if (a, b) == ("quirks.zig", "font/main.zig"): return 2
    if a == "build_config.zig" and b in ("apprt.zig", "font/main.zig", "renderer.zig", "build/Config.zig"): return 3
    if a.startswith("terminal/") and not a.startswith("terminal/c/") and (b.startswith("terminal/c/") or b == "terminal/c.zig"): return 4
    if (a, b) == ("font/SharedGrid.zig", "renderer.zig"): return 5
    if a.startswith("input") and b in ("config/formatter.zig", "config/key.zig", "config.zig", "config/Config.zig"): return 6
    if a.startswith("config/") and (b == "cli.zig" or b in ("cli/args.zig", "cli/diagnostics.zig", "cli/CommaSplitter.zig")): return 7
    if a in ("App.zig", "Surface.zig", "Command.zig") and b == "apprt.zig": return 8
    if a.startswith("font/") and b in ("config.zig", "config/Config.zig"): return 9
    if a.startswith("renderer/") and b in ("apprt.zig", "Surface.zig", "App.zig"): return 10
    if a.startswith("renderer/") and b == "inspector/main.zig": return 11
    if a.startswith("inspector/") and b in ("Surface.zig", "App.zig"): return 12
    if a.startswith("termio/") and b == "apprt.zig": return 13
    if (a, b) == ("os/open.zig", "apprt.zig"): return 14
    return 0

ENTRY = {f for f in files if f.startswith("main") or f == "lib_vt.zig" or f == "helpgen.zig"}
cut = {(a, b): brk(a, b) for (a, b) in E}
per_break = defaultdict(list)
for (a, b), k in cut.items():
    if k: per_break[k].append((a, b))
for k in range(1, 15):
    print(k, len(per_break[k]), per_break[k][:3])

def largest_after(removed_breaks, drop_entry=True):
    g = defaultdict(set)
    for a in files:
        for b in edges[a]:
            k = brk(a, b)
            if k and k in removed_breaks: continue
            if drop_entry and b in ENTRY: continue
            g[a].add(b)
    cs = sorted([c for c in tarjan(g, files) if len(c) > 1], key=len, reverse=True)
    return cs

ablation = {}
for k in range(1, 15):
    cs = largest_after({k})
    ablation[k] = len(cs[0])
print("single-cut ablation", ablation)
r1 = largest_after({1, 2, 3, 4, 5, 14})
r2 = largest_after({1, 2, 3, 4, 5, 14, 6, 7, 8, 9, 10, 11, 12, 13})
def span(c):
    d = defaultdict(int)
    for f in c: d[topdir(f)] += 1
    return dict(sorted(d.items(), key=lambda x: -x[1]))
stages = [
    {"label": "No refactoring", "largest": len(big), "span": span(big), "n_multi": len(sccs)},
    {"label": "After the 6 hub cuts (1-5, 14)", "largest": len(r1[0]), "span": span(r1[0]), "n_multi": len(r1)},
    {"label": "After all 14 cuts", "largest": len(r2[0]), "span": span(r2[0]), "n_multi": len(r2),
     "clusters": [{"size": len(c), "span": span(c)} for c in r2[:8]]},
]
print(stages)
# residual cross-directory edges in the 45-file cluster
res = [c for c in r2 if len(span(c))>2][0]; rs = set(res)
resid = sorted({(topdir(a), topdir(b)) for a in res for b in edges[a] if b in rs and topdir(a) != topdir(b) and brk(a, b) == 0 and b not in ENTRY})
res_edges = [(a, b) for a in res for b in edges[a] if b in rs and topdir(a) != topdir(b)]
print("residual cross-dir pairs", resid)
print("residual cross edges", len(res_edges), res_edges[:40])
cum = []
for k in range(1, 15):
    cs = largest_after(set(range(1, k + 1)))
    cum.append(len(cs[0]))
print("cumulative", cum)
# cumulative, but measuring the largest cluster that spans >1 future crate
def largest_cross(cs):
    for c in cs:
        if len({crate_of(f) for f in c}) > 1: return len(c)
    return 0
cumx = []
for k in range(0, 15):
    cs = largest_after(set(range(1, k + 1)))
    cumx.append(largest_cross(cs))
print("cumulative cross-crate", cumx)

# ---------------- example cycles: shortest cycle through chosen files ----------------
def shortest_cycle(v):
    prev = {v: None}; q = deque([v])
    while q:
        x = q.popleft()
        for w in sorted(edges[x]):
            if w not in big: continue
            if w == v:
                path = [x]
                while prev[path[-1]] is not None: path.append(prev[path[-1]])
                return list(reversed(path)) + [v]
            if w not in prev: prev[w] = x; q.append(w)
    return None
cyc = {}
for f in ["datastruct/circ_buf.zig", "simd/codepoint_width.zig", "terminal/Terminal.zig", "font/Atlas.zig", "os/open.zig", "crash/main.zig", "termio/Exec.zig", "unicode/main.zig"]:
    if f in big: cyc[f] = shortest_cycle(f)
for k, v in cyc.items(): print("cycle", k, len(v) - 1, v)
# distribution of shortest-cycle length over all SCC files
dist = defaultdict(int); longest = None
for f in big:
    c = shortest_cycle(f); L = len(c) - 1; dist[L] += 1
    if longest is None or L > len(longest) - 1: longest = c
print("shortest-cycle length distribution", dict(sorted(dist.items())), "max", longest)

# degree stats
indeg = defaultdict(int); outdeg = defaultdict(int)
for a, b in E: indeg[b] += 1; outdeg[a] += 1
top_in = sorted(big, key=lambda f: -indeg[f])[:8]
top_out = sorted(big, key=lambda f: -outdeg[f])[:8]
print("top in", [(f, indeg[f]) for f in top_in]); print("top out", [(f, outdeg[f]) for f in top_out])

unm = [f for f in big if crate_of(f) is None]
print("unmapped in SCC", unm)

# ---------------- radial layout by directory ----------------
dirs = defaultdict(list)
for f in big: dirs[topdir(f)].append(f)
DIR_ORDER = ["terminal", "datastruct", "simd", "os", "crash", "(root)", "config", "input", "cli",
             "font", "renderer", "inspector", "termio", "apprt", "benchmark"]
for d in dirs:
    if d not in DIR_ORDER: DIR_ORDER.append(d)
DIR_ORDER = [d for d in DIR_ORDER if d in dirs]
total = len(big); gap = 0.035
avail = 2 * math.pi - gap * len(DIR_ORDER)
ang = -math.pi / 2; sectors = {}; pos = {}
W, H = 1000, 1000; cx, cy = 500, 500
for d in DIR_ORDER:
    fs_ = sorted(dirs[d], key=lambda f: (-indeg[f], f))
    sweep = avail * len(fs_) / total
    sectors[d] = (ang, ang + sweep)
    # rings: hubs (high in-degree) inner, others outer; fill rows of increasing radius
    n = len(fs_)
    rings = [];
    # radius from 200..460 ; number per ring proportional to arc length
    r = 250; i = 0
    while i < n:
        cap = max(1, int(sweep * r / 15.5))
        rings.append((r, fs_[i:i + cap])); i += cap; r += 15.5
    for r, ring in rings:
        m = len(ring)
        for j, f in enumerate(ring):
            t = ang + sweep * (j + 0.5) / m
            pos[f] = (cx + r * math.cos(t), cy + r * math.sin(t))
    ang += sweep + gap
# global.zig at the centre (it is the hub)
pos["global.zig"] = (cx, cy)

# ---------------- crate DAG layout ----------------
G = json.load(open(os.path.join(DATA, "codebase-crate-graph.json")))
nodes = G["nodes"]; cedges = G["edges"]
short = lambda s: s.replace("rustty-", "") if s != "rustty" else "rustty"
depth = {n["id"]: n["depth"] for n in nodes}
nb = [e for e in cedges if e["kind"] != "dev"]
# recompute depth to verify
adj = defaultdict(set)
for e in nb: adj[e["source"]].add(e["target"])
memo = {}
def dep(v):
    if v in memo: return memo[v]
    memo[v] = 1 + max((dep(w) for w in adj[v]), default=-1); return memo[v]
for n in nodes: assert dep(n["id"]) == n["depth"], (n["id"], dep(n["id"]), n["depth"])
# longest chain
def chain(v):
    out = [v]
    while adj[out[-1]]:
        out.append(max(adj[out[-1]], key=lambda w: (dep(w), w)))
    return out
longest_chain = chain("xtask")
print("longest chain", [short(x) for x in longest_chain])
# transitive dependents (reverse reach, normal+build)
radj = defaultdict(set)
for e in nb: radj[e["target"]].add(e["source"])
def rreach(v):
    seen = set(); st = [v]
    while st:
        x = st.pop()
        for w in radj[x]:
            if w not in seen: seen.add(w); st.append(w)
    return seen
def freach(v):
    seen = set(); st = [v]
    while st:
        x = st.pop()
        for w in adj[x]:
            if w not in seen: seen.add(w); st.append(w)
    return seen
trans_in = {n["id"]: len(rreach(n["id"])) for n in nodes}
trans_out = {n["id"]: len(freach(n["id"])) for n in nodes}
fanin = defaultdict(int); fanout = defaultdict(int)
for e in nb: fanin[e["target"]] += 1; fanout[e["source"]] += 1
print("fan-in", sorted(fanin.items(), key=lambda x: -x[1])[:8])
print("fan-out", sorted(fanout.items(), key=lambda x: -x[1])[:8])
print("trans-in", sorted(trans_in.items(), key=lambda x: -x[1])[:8])

layers = defaultdict(list)
for n in nodes: layers[n["depth"]].append(n["id"])
# barycentric ordering, several sweeps
order = {d: sorted(v, key=lambda x: (x.endswith("-sys"), x)) for d, v in layers.items()}
def ypos():
    y = {}
    for d, v in order.items():
        m = len(v)
        for i, x in enumerate(v): y[x] = (i + 0.5) / m
    return y
undirected = defaultdict(set)
for e in nb: undirected[e["source"]].add(e["target"]); undirected[e["target"]].add(e["source"])
for it in range(40):
    y = ypos()
    rng = range(1, 19) if it % 2 == 0 else range(17, -1, -1)
    for d in rng:
        def bc(x):
            nbrs = [y[w] for w in undirected[x]]
            return sum(nbrs) / len(nbrs) if nbrs else y[x]
        order[d] = sorted(order[d], key=lambda x: (bc(x), x))
        y = ypos()
y = ypos()
loc = {n["id"]: n["rust_code"] for n in nodes}

# ---------------- per-crate detail ----------------
birth = {r["crate"]: r for r in csv.DictReader(open(os.path.join(DATA, "r2-crate-birth.csv")))}
unsafe = {r["crate"]: r for r in csv.DictReader(open(os.path.join(DATA, "codebase-unsafe-per-crate.csv")))}
mods = list(csv.DictReader(open(os.path.join(DATA, "codebase-zig-vs-rust-per-module.csv"))))
sysrows = {r["sys_crate"]: r for r in csv.DictReader(open(os.path.join(DATA, "codebase-third-party-sys.csv")))}
modof = {}
for m in mods:
    for c in m["rust_crates"].split("+"): modof[c] = m
# zig files per crate (whole src/ at starting pin) and dirs
zfiles = defaultdict(list)
for f in files:
    c = crate_of(f)
    if c: zfiles[c].append(f)
crate_detail = {}
for n in nodes:
    cid = n["id"]; s = short(cid)
    zf = zfiles.get(s, [])
    zd = defaultdict(int)
    for f in zf: zd[(f.rsplit("/", 1)[0] + "/") if "/" in f else f] += 1
    m = modof.get(cid)
    b = birth.get(cid, {})
    u = unsafe.get(cid, {})
    sr = sysrows.get(cid)
    crate_detail[cid] = {
        "zig_files": len(zf), "zig_in_scc": sum(1 for f in zf if f in big),
        "zig_dirs": sorted(zd.items(), key=lambda x: -x[1])[:6],
        "module": m["module"] if m else None, "module_crates": m["rust_crates"] if m else None,
        "zig_code": int(m["zig_code"]) if m and m["zig_code"] else None,
        "module_rust_code": int(m["rust_code"]) if m else None,
        "zig_tests": int(m["zig_tests"]) if m and m["zig_tests"] else None,
        "module_rust_tests": int(m["rust_tests"]) if m else None,
        "birth": b.get("first_date"), "birth_commit": b.get("first_commit"), "birth_subject": b.get("first_subject"),
        "k1000": b.get("first_date_rust_loc_ge_1000") or None,
        "unsafe_sites": int(u["unsafe_sites"]) if u else n["unsafe_total"],
        "unsafe_per_kloc": float(u["sites_per_kloc_code"]) if u else (round(1000 * n["unsafe_total"] / n["rust_code"], 2) if n["rust_code"] else None),
        "unsafe_nontest_per_kloc": float(u["non_test_sites_per_kloc_code"]) if u else None,
        "vendored_c": int(sr["vendored_c_cpp_loc"]) if sr else None,
        "glue": int(sr["rust_wrapper_loc"]) if sr else None,
        "ghostty_pkg": sr["ghostty_pkg"] if sr else None,
        "fanin": fanin[cid], "fanout": fanout[cid], "trans_in": trans_in[cid], "trans_out": trans_out[cid],
    }
print("vendored sum", sum(int(r["vendored_c_cpp_loc"]) for r in sysrows.values()), "glue", sum(int(r["rust_wrapper_loc"]) for r in sysrows.values()))

# ---------------- leaves-first: spearman(depth, date reaching 1k lines) ----------------
def rank(v):
    s = sorted(range(len(v)), key=lambda i: v[i]); r = [0] * len(v); i = 0
    while i < len(s):
        j = i
        while j + 1 < len(s) and v[s[j + 1]] == v[s[i]]: j += 1
        for k in range(i, j + 1): r[s[k]] = (i + j) / 2
        i = j + 1
    return r
pairs = [(depth[c], b["first_date_rust_loc_ge_1000"]) for c, b in birth.items() if b["first_date_rust_loc_ge_1000"] and c in depth]
rx = rank([p[0] for p in pairs]); ry = rank([p[1] for p in pairs])
n_ = len(pairs); mx = sum(rx) / n_; my = sum(ry) / n_
rho = sum((a - mx) * (b - my) for a, b in zip(rx, ry)) / math.sqrt(sum((a - mx) ** 2 for a in rx) * sum((b - my) ** 2 for b in ry))
print("spearman depth vs 1k-date", round(rho, 3), "n", n_)

# ---------------- daily LOC per crate ----------------
daily = defaultdict(dict); dates = set()
for r in csv.DictReader(open(os.path.join(DATA, "timeline-loc_by_day_long.csv"))):
    b = r["bucket"]; dates.add(r["date"])
    if b.startswith("crate:"): daily[b[6:]][r["date"]] = int(r["lines"])
    elif b == "rs:crates-sys": daily["__sys"][r["date"]] = int(r["lines"])
    elif b == "rs:xtask": daily["xtask"][r["date"]] = int(r["lines"])
dates = sorted(dates)
series = {k: [v.get(d, 0) for d in dates] for k, v in daily.items()}
tests = [(r["date"], int(r["rust_test_fns"])) for r in csv.DictReader(open(os.path.join(DATA, "timeline-tests_by_week.csv")))]
print("tests", tests[:3], tests[-2:])
# by-depth median of the 1k date per layer band
band = defaultdict(list)
for c, b in birth.items():
    if b["first_date_rust_loc_ge_1000"] and c in depth: band[depth[c]].append(b["first_date_rust_loc_ge_1000"])

bands = {}
for lo, hi in [(0, 4), (5, 9), (10, 14), (15, 18)]:
    v = sorted(x for d, l in band.items() if lo <= d <= hi for x in l)
    bands[f"{lo}-{hi}"] = {"n": len(v), "median": v[len(v)//2] if v else None, "first": v[0] if v else None, "last": v[-1] if v else None}
print("bands", bands)
out = {
    "zig": {
        "pin": "4749c4e93", "pin_date": "2026-06-20", "files_total": len(files),
        "runtime_edges_total": sum(len(v) for v in edges.values()),
        "scc_files": len(big), "scc_edges": len(E), "other_sccs": [len(c) for c in sccs[1:]],
        "dirs": DIR_ORDER, "sectors": {d: [round(a, 4) for a in v] for d, v in sectors.items()},
        "files": [{"f": f, "d": topdir(f), "x": round(pos[f][0], 1), "y": round(pos[f][1], 1),
                   "in": indeg[f], "out": outdeg[f], "ln": lines[f], "c": crate_of(f)} for f in sorted(big)],
        "cycles": cyc, "cycle_len_dist": dict(sorted(dist.items())), "longest_short_cycle": longest,
        "ablation": ablation, "stages": stages, "cumulative": cum, "cumulative_cross": cumx, "residual_edges": res_edges, "bands": bands,
        "now_pin": {"commit": "a60e9e2a5", **pin_stats},
    },
    "breaks_edges": {k: v for k, v in per_break.items()},
    "crates": {
        "nodes": [{"id": n["id"], "s": short(n["id"]), "tier": n["tier"], "group": n["group"], "depth": n["depth"],
                   "loc": n["rust_code"], "tests": n["tests"], "y": round(y[n["id"]], 4), **crate_detail[n["id"]]} for n in nodes],
        "edges": [[e["source"], e["target"], e["kind"]] for e in cedges],
        "longest_chain": longest_chain, "spearman_depth_1k": round(rho, 3), "spearman_n": n_,
        "layering": G["layering_decisions"],
    },
    "replay": {"dates": dates, "series": series, "tests": tests},
}
idx = {f["f"]: i for i, f in enumerate(out["zig"]["files"])}
out["zig"]["edges"] = [[idx[a], idx[b], cut[(a, b)]] for (a, b) in E]
json.dump(out, open(OUT, "w"), separators=(",", ":"))
print("wrote", OUT, os.path.getsize(OUT))

# ---- extra: residual cross-crate edges in the multi-crate leftover cluster; leave-one-out ----
rx_ = [(a, b, crate_of(a), crate_of(b)) for (a, b) in res_edges if brk(a, b) == 0 and b not in ENTRY and crate_of(a) != crate_of(b)]
print("RESID cross-crate unbroken", len(rx_))
for r in rx_: print("   ", r)
loo = {}
for k in range(1, 15):
    cs = largest_after(set(range(1, 15)) - {k})
    loo[k] = largest_cross(cs)
print("leave-one-out cross-crate largest", loo)
out["zig"]["residual_unbroken"] = rx_
out["zig"]["leave_one_out"] = loo
json.dump(out, open(OUT, "w"), separators=(",", ":"))

# ---- with the crash/sentry -> Surface seam (ThreadStateSurface trait in rustty-crash) ----
_brk_orig = brk
def brk(a, b):
    if (a, b) == ("crash/sentry.zig", "Surface.zig"): return 15
    return _brk_orig(a, b)
allx = set(range(1, 16))
cs = largest_after(allx)
print("ALL+15 largest", [(len(c), span(c)) for c in cs[:6]], "cross", largest_cross(cs))
loo15 = {}
for k in range(1, 16):
    loo15[k] = largest_cross(largest_after(allx - {k}))
print("leave-one-out with 15", loo15)
out["zig"]["with15"] = {"largest_cross": largest_cross(cs), "clusters": [{"size": len(c), "span": span(c), "crates": sorted({crate_of(f) for f in c})} for c in cs[:8]]}
out["zig"]["leave_one_out15"] = loo15
cum15 = [largest_cross(largest_after(set(range(1, k + 1)))) for k in range(0, 16)]
print("cum15", cum15)
out["zig"]["cumulative15"] = cum15
json.dump(out, open(OUT, "w"), separators=(",", ":"))

# ---- crate-level check: collapse files to target crates ----
def crate_sccs(removed, merge_api=True):
    g = defaultdict(set); nodes_ = set()
    for a in files:
        ca = crate_of(a)
        if ca is None: continue
        for b in edges[a]:
            k = brk(a, b)
            if k and k in removed: continue
            if b in ENTRY: continue
            cb = crate_of(b)
            if cb is None: continue
            if merge_api:
                ca2 = "apprt" if ca == "apprt-api" else ca; cb2 = "apprt" if cb == "apprt-api" else cb
            else: ca2, cb2 = ca, cb
            nodes_.add(ca2); nodes_.add(cb2)
            if ca2 != cb2: g[ca2].add(cb2)
    return sorted([sorted(c) for c in tarjan(g, sorted(nodes_)) if len(c) > 1], key=len, reverse=True)
print("crate-level none", crate_sccs(set()))
print("crate-level all15", crate_sccs(allx))
print("crate-level all15 nomerge", crate_sccs(allx, False))
cl = {}
for k in range(1, 16):
    r = crate_sccs(allx - {k})
    cl[k] = r
    print("skip", k, r)
out["zig"]["crate_level_skip"] = {k: v for k, v in cl.items()}
out["zig"]["crate_level_none"] = crate_sccs(set())
json.dump(out, open(OUT, "w"), separators=(",", ":"))

# ---- edges the final Rust crate graph does NOT allow (no dependency path) ----
def full(c): return "rustty" if c == "rustty" else ("xtask" if c == "xtask" else "rustty-" + c)
allowed_cache = {}
def allowed(ca, cb):
    A = full(ca); B = full(cb)
    if (A, B) not in allowed_cache:
        allowed_cache[(A, B)] = B in freach(A)
    return allowed_cache[(A, B)]
inv = []
for (a, b) in E:
    ca, cb = crate_of(a), crate_of(b)
    if ca == cb or b in ENTRY: continue
    if not allowed(ca, cb): inv.append((a, b, ca, cb, brk(a, b)))
print("inverted (not allowed in Rust) edges in SCC:", len(inv), "of cross-crate", sum(1 for (a,b) in E if crate_of(a)!=crate_of(b) and b not in ENTRY))
pc = defaultdict(list)
for r in inv: pc[(r[2], r[3], r[4])].append(r[0] + " -> " + r[1])
for k, v in sorted(pc.items(), key=lambda x: (x[0][2] or 99, -len(x[1]))): print("  ", k, len(v), v[:3])
out["zig"]["inverted"] = [list(r) for r in inv]
json.dump(out, open(OUT, "w"), separators=(",", ":"))

# ================= compact embed for the widget =================
base = {tuple(c) for c in crate_sccs(allx)}
skip_new = {k: [c for c in v if tuple(c) not in base] for k, v in cl.items()}
inv_set = {(r[0], r[1]) for r in inv}
def tag(a, b):
    # seam tag for an SCC edge, matched to how the Rust graph resolved it
    if a == "global.zig" and (a, b) in inv_set: return 1          # top-down imports moved up to core
    if b == "global.zig": return 21                                  # bottom-up importers (now point at a leaf)
    k = _brk_orig(a, b)
    if k == 1: k = 0
    if k: return k
    if (a, b) == ("crash/sentry.zig", "Surface.zig"): return 15
    if (a, b) == ("config/formatter.zig", "config/key.zig"): return 6
    if a.startswith("apprt/") and crate_of(a) == "apprt-api" and (a, b) in inv_set: return 8
    if (a, b) in inv_set:
        ca, cb = crate_of(a), crate_of(b)
        if (ca, cb) == ("font", "font-shaper"): return 16
        if (ca, cb) == ("renderer", "config"): return 17
        if (ca, cb) == ("vt", "input"): return 18
        if ca in ("bench", "rustty"): return 0   # binary / tool entry wiring
        return 19
    return 0
tags = defaultdict(int)
emb_edges = []
for (a, b) in E:
    t = tag(a, b); tags[t] += 1
    emb_edges.append([idx[a], idx[b], t])
print("tags", dict(tags))
other19 = [(a, b) for (a, b) in E if tag(a, b) == 19]
print("other", other19)
dir_index = {d: i for i, d in enumerate(DIR_ORDER)}
emb = {
    "zig": {
        "dirs": DIR_ORDER, "sectors": [[round(a, 4) for a in sectors[d]] for d in DIR_ORDER],
        "files": [[f["f"], dir_index[f["d"]], f["x"], f["y"], f["in"], f["out"], f["ln"], f["c"]] for f in out["zig"]["files"]],
        "edges": emb_edges,
        "stats": {"files_total": len(files), "runtime_edges": out["zig"]["runtime_edges_total"], "scc": len(big),
                  "scc_edges": len(E), "other_sccs": out["zig"]["other_sccs"], "cycle_len_dist": out["zig"]["cycle_len_dist"],
                  "now": out["zig"]["now_pin"], "cum": cum15, "inverted": len(inv),
                  "cross_crate": sum(1 for (a, b) in E if crate_of(a) != crate_of(b) and b not in ENTRY),
                  "ablation": ablation},
        "skip_new": skip_new,
        "examples": [out["zig"]["longest_short_cycle"], cyc["datastruct/circ_buf.zig"], cyc["os/open.zig"], cyc["font/Atlas.zig"]],
    },
    "crates": {
        "nodes": [[n["id"], n["tier"], n["depth"], n["loc"], n["tests"], n["y"], n["fanin"], n["fanout"], n["trans_in"], n["trans_out"],
                   n["zig_files"], n["zig_in_scc"], n["zig_dirs"], n["module"], n["module_crates"], n["zig_code"], n["module_rust_code"],
                   n["zig_tests"], n["module_rust_tests"], n["birth"], n["birth_commit"], n["k1000"], n["unsafe_sites"], n["unsafe_per_kloc"],
                   n["vendored_c"], n["ghostty_pkg"]] for n in out["crates"]["nodes"]],
        "edges": out["crates"]["edges"], "chain": out["crates"]["longest_chain"],
        "rho": out["crates"]["spearman_depth_1k"], "rho_n": out["crates"]["spearman_n"], "bands": out["zig"]["bands"],
    },
}
# replay: sparse series starting at first nonzero
rp = {}
for k, v in out["replay"]["series"].items():
    first = next((i for i, x in enumerate(v) if x), None)
    if first is None: continue
    rp[k] = [first, v[first:]]
emb["replay"] = {"d0": out["replay"]["dates"][0], "n": len(out["replay"]["dates"]), "dates": out["replay"]["dates"], "s": rp, "tests": out["replay"]["tests"]}
EMB = OUT.replace("-data.json", "-embed.json")
json.dump(emb, open(EMB, "w"), separators=(",", ":"))
print("embed", EMB, os.path.getsize(EMB))
