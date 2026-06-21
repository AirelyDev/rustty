#!/usr/bin/env python3
"""
cargo_cycle_check.py — actual-rustty-crate-graph cycle checker (backs GATE G7).

Unlike .port/tools/cycle_check.py (which models the *reference* ghostty file
import graph against the PROPOSED crate mapping), this checks the REAL rustty
Cargo workspace: it parses every crates/*/Cargo.toml `[dependencies]` (and
`[build-dependencies]`) section, builds the inter-rustty-crate edge set, and runs
Tarjan SCC detection. Any cross-crate SCC of size > 1 is a cycle (Cargo refuses
to build a normal-dep cycle; R4 requires the graph be acyclic).

Dev-dependencies are EXCLUDED by default (test-only edges are permitted to be
cyclic per DoP G7 part-1). Pass --include-dev to include them.

Exit codes:
  0  -> no cross-crate runtime SCC of size > 1 (GREEN for G7's structural check)
  1  -> at least one cross-crate cycle (RED) ; the cycle members + edges printed

This is intentionally dependency-free (no `toml` module needed): it does a small
hand-rolled scan of the `[dependencies]` / `[build-dependencies]` tables, which
is sufficient for the simple, generated rustty manifests (one dep per line,
`rustty-foo = { workspace = true }` or `rustty-foo = "x"` or `rustty-foo = {...}`).
"""
import argparse
import os
import re
import sys
from collections import defaultdict

GATES_DIR = os.path.dirname(os.path.abspath(__file__))
TOOLS_DIR = os.path.dirname(GATES_DIR)
PORT_DIR = os.path.dirname(TOOLS_DIR)
RUSTTY_ROOT = os.path.dirname(PORT_DIR)
CRATES_DIR = os.path.join(RUSTTY_ROOT, "crates")

SECTION_RE = re.compile(r"^\s*\[([^\]]+)\]\s*$")
# a dep line begins with a bare/quoted crate name then `=`
DEP_NAME_RE = re.compile(r'^\s*["\']?([A-Za-z0-9_-]+)["\']?\s*=')
PKG_NAME_RE = re.compile(r'^\s*name\s*=\s*["\']([^"\']+)["\']')


def parse_manifest(path, include_dev):
    """Return (package_name, set(rustty-dep-names)) for one Cargo.toml."""
    pkg = None
    deps = set()
    section = None
    want = {"dependencies", "build-dependencies"}
    if include_dev:
        want.add("dev-dependencies")
    try:
        with open(path, encoding="utf-8") as fh:
            lines = fh.readlines()
    except Exception:  # noqa: BLE001
        return None, deps
    for line in lines:
        msec = SECTION_RE.match(line)
        if msec:
            section = msec.group(1).strip()
            continue
        if section == "package" and pkg is None:
            mp = PKG_NAME_RE.match(line)
            if mp:
                pkg = mp.group(1)
            continue
        # normalise target-specific dependency tables:
        #   [target.'cfg(...)'.dependencies] -> ends with .dependencies
        sect = section or ""
        sect_tail = sect.split(".")[-1]
        if sect_tail in want:
            mdn = DEP_NAME_RE.match(line)
            if mdn:
                name = mdn.group(1)
                if name.startswith("rustty"):
                    deps.add(name)
    return pkg, deps


def build_graph(include_dev):
    """Return (adj, manifests) where adj maps package-name -> set(dep-package-names),
    restricted to rustty workspace crates that actually exist."""
    adj = defaultdict(set)
    name_set = set()
    raw = {}
    if not os.path.isdir(CRATES_DIR):
        return adj, name_set
    for entry in sorted(os.listdir(CRATES_DIR)):
        mani = os.path.join(CRATES_DIR, entry, "Cargo.toml")
        if not os.path.exists(mani):
            continue
        pkg, deps = parse_manifest(mani, include_dev)
        if pkg is None:
            pkg = entry  # fall back to dir name
        raw[pkg] = deps
        name_set.add(pkg)
    # keep only edges to crates that exist in this workspace
    for pkg, deps in raw.items():
        for d in deps:
            if d in name_set:
                adj[pkg].add(d)
    return adj, name_set


def tarjan(adj, nodes):
    idx = {}
    low = {}
    onstk = {}
    stk = []
    out = []
    counter = [0]
    sys.setrecursionlimit(100000)

    def dfs(v):
        idx[v] = low[v] = counter[0]
        counter[0] += 1
        stk.append(v)
        onstk[v] = True
        for w in adj.get(v, ()):
            if w not in idx:
                dfs(w)
                low[v] = min(low[v], low[w])
            elif onstk.get(w):
                low[v] = min(low[v], idx[w])
        if low[v] == idx[v]:
            comp = []
            while True:
                w = stk.pop()
                onstk[w] = False
                comp.append(w)
                if w == v:
                    break
            out.append(comp)

    for v in nodes:
        if v not in idx:
            dfs(v)
    return out


def main():
    ap = argparse.ArgumentParser(description="rustty Cargo-graph cycle check (G7)")
    ap.add_argument("--include-dev", action="store_true",
                    help="include [dev-dependencies] edges (default: excluded)")
    ap.add_argument("--fail-on-scc", action="store_true", default=True,
                    help="exit non-zero on any cross-crate SCC (default on)")
    ap.add_argument("--no-fail-on-scc", dest="fail_on_scc", action="store_false")
    args = ap.parse_args()

    adj, names = build_graph(args.include_dev)
    nodes = sorted(names)
    sccs = [c for c in tarjan(adj, nodes) if len(c) > 1]

    print(f"rustty workspace crates parsed: {len(names)}")
    print(f"inter-crate runtime edges: {sum(len(v) for v in adj.values())}")
    print(f"cross-crate SCCs (size>1): {len(sccs)}")
    for comp in sccs:
        cs = set(comp)
        print(f"\n  CYCLE: {{{', '.join(sorted(comp))}}}")
        for a in sorted(comp):
            for b in sorted(adj.get(a, ())):
                if b in cs:
                    print(f"    {a} -> {b}")

    if sccs and args.fail_on_scc:
        print("\nRED: cross-crate dependency cycle in the rustty Cargo graph")
        return 1
    print("\nGREEN: no cross-crate cycle in the rustty Cargo graph")
    return 0


if __name__ == "__main__":
    sys.exit(main())
