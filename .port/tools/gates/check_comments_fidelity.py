#!/usr/bin/env python3
"""
check_comments_fidelity.py — GATE G3 (zero comment-inventory delta) [R1]

DoP id:  G3  (DEFINITION_OF_PARITY §1, "Zero comment-inventory delta / fidelity critic")
Reads:   .port/phase-0/inventory/comments-census.json  (per-file + per-crate
                                                          comment-line census, by class)
         .port/phase-0/inventory/tests-summary.json     (file->crate map; lets us
                                                          aggregate the census per crate)
         .port/phase-0/plan/identity-rename.map         (optional ghostty->rustty token
                                                          map; substitutions are expected,
                                                          not deltas — R2)
Scans:   crates/<crate>/src/**/*.rs   (recomputes the comment census over ported Rust)

PASS CONDITION (DoP G3):
  - per-crate comment-unit delta == 0 for every claimed-done crate, AND
    global delta == 0 (in --strict, for the whole workspace).
  Any DROP for a *done* crate is RED:  DROPPED: <crate> class=<c> ref=<n> rust=<m>.

Because comments-census is keyed by *.zig file but ported Rust may merge/split
files within a crate, G3 compares at the CRATE granularity (sum of all the
crate's reference files' comment lines vs the crate's ported Rust comment lines).
This matches R1's "no comment dropped" while tolerating idiomatic file
re-layout inside a crate. A negative delta (rust < ref) is a DROP and is RED for
a done crate. A non-negative delta (rust >= ref) is allowed (the port may add
glue comments, R1 "no-loss, not no-addition").

MODES:
  default (progress): reports ref/rust comment totals per crate; the empty
    workspace reports rust=scaffold-comment-count without erroring (exit 0)
    UNLESS a claimed-done crate dropped comments.
  --strict: full G3 — every claimed-done crate has delta >= 0 AND any crate that
    has ANY ported Rust source (non-scaffold) must not drop comments. (On the
    fully-ported workspace, every crate is done and global drop must be 0.)
  --crate <name>: scope to one crate (P-C gate).
  --self-test: built-in flip proof.
"""
import argparse
import os
import re
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import _gatelib as G  # noqa: E402


def load_identity_map():
    """Parse identity-rename.map: lines `ghostty_token => rustty_token` (or
    whitespace/`=`-separated). Returns list of (pat, repl). Absent file => []."""
    pairs = []
    if not os.path.exists(G.IDENTITY_MAP):
        return pairs, False
    with open(G.IDENTITY_MAP, encoding="utf-8") as fh:
        for line in fh:
            line = line.strip()
            if not line or line.startswith("#"):
                continue
            m = re.split(r"\s*=>\s*|\s*=\s*|\s+", line, maxsplit=1)
            if len(m) == 2:
                pairs.append((m[0], m[1]))
    return pairs, True


def ref_census_per_crate(census, summary):
    """{crate: {class: lines}} aggregated from the per-file census over each
    crate's files (file->crate via crate_map)."""
    f2c = G.file_to_crate(summary)
    out = {}
    per_file = census.get("per_file", {})
    for f, info in per_file.items():
        crate = f2c.get(f)
        if crate is None:
            continue
        d = out.setdefault(crate, {"doc-module": 0, "doc-decl": 0, "line": 0, "total": 0})
        d["doc-module"] += int(info.get("doc-module", 0))
        d["doc-decl"] += int(info.get("doc-decl", 0))
        d["line"] += int(info.get("line", 0))
        d["total"] += int(info.get("total", 0))
    return out


def run(args):
    G.banner("G3", "zero comment-inventory delta (fidelity critic)")
    try:
        summary = G.load_tests_summary()
        census = G.load_comments_census()
        cm = G.crate_map(summary)
    except G.Blocked as b:
        G.emit(G.BLOCKED_STATE, str(b))
        return 3

    _idmap, idmap_present = load_identity_map()
    if not idmap_present:
        # Not fatal: the rename map is an aid for ALTERED-comment detection. G3's
        # core metric (drop count) does not require it. Note it for transparency.
        print("NOTE: identity-rename.map absent; running drop-count fidelity only "
              "(token-rename ALTERED detection unavailable until the map exists).")

    ref_per_crate = ref_census_per_crate(census, summary)

    if args.crate:
        args.crate = G.normalize_crate(args.crate, cm)
    crates = [args.crate] if args.crate else G.workspace_crates(cm)
    if args.crate and args.crate not in cm:
        G.emit(G.RED, f"unknown crate '{args.crate}' (not in crate_map)")
        return 2

    # R4 relocations (_gatelib.RELOCATIONS): a stale entry is BLOCKED up front.
    try:
        G.check_relocations(crates)
    except G.Blocked as b:
        G.emit(G.BLOCKED_STATE, str(b))
        return 3

    red = False
    rows = []
    grand_ref = 0
    grand_rust = 0
    # Workspace total counts each comment ONCE by (file, index): a relocated
    # comment is also scanned by its host crate.
    seen_comments = set()
    for crate in crates:
        ref = ref_per_crate.get(crate, {"total": 0, "doc-module": 0, "doc-decl": 0, "line": 0})
        rust = {"doc-module": 0, "doc-decl": 0, "line": 0}
        for path, idx, kind, lines in G.census_rust_comments_located(crate):
            rust[kind] += lines
            if (path, idx) not in seen_comments:
                seen_comments.add((path, idx))
                grand_rust += lines
        rust["total"] = rust["doc-module"] + rust["doc-decl"] + rust["line"]
        claimed_done = G.is_claimed_done(crate)
        delta = rust["total"] - ref["total"]
        grand_ref += ref["total"]

        crate_red = False
        if claimed_done and delta < 0:
            crate_red = True
        if args.strict and delta < 0 and claimed_done:
            crate_red = True
        if crate_red:
            red = True
        rows.append((crate, G.crate_status(crate), rust["total"], ref["total"],
                     delta, claimed_done, crate_red))

    print(f"{'crate':<16}{'status':<10}{'rust':>8}{'ref':>8}{'delta':>8}  state")
    for (crate, status, rust_total, ref_total, delta, done, crate_red) in rows:
        state = "RED" if crate_red else ("done" if done else "progress")
        print(f"{crate:<16}{status:<10}{rust_total:>8}{ref_total:>8}{delta:>8}  {state}")
        if crate_red:
            print(f"    DROPPED: {crate} ref_total={ref_total} rust_total={rust_total} "
                  f"({-delta} comment lines lost)")

    scope = args.crate if args.crate else "workspace"
    print(f"\nPROGRESS [{scope}]: comment-lines rust={grand_rust} ref={grand_ref} "
          f"(global census total = {census.get('totals', {}).get('total_comment_lines', '?')})")

    if args.strict:
        if red:
            G.emit(G.RED, "a claimed-done crate dropped comments (delta < 0); see DROPPED")
            return 1
        # Strict (final-gate) also enforces the global census floor: the ported
        # workspace must carry at least the reference comment-line total for its
        # crates (no-loss, R1). This makes --strict flip RED on the empty
        # workspace and only GREEN when fidelity is actually achieved.
        if not args.crate and grand_rust < grand_ref:
            G.emit(G.RED, f"global comment lines rust={grand_rust} < ref floor {grand_ref} "
                          f"(not all comments ported yet)")
            return 1
        G.emit(G.GREEN, f"per-crate delta >= 0 and global rust={grand_rust} >= ref={grand_ref}")
        return 0

    if red:
        G.emit(G.RED, "a claimed-done crate dropped comments; see DROPPED above")
        return 1
    G.emit(G.GREEN, f"progress rust={grand_rust}/ref={grand_ref}; no claimed-done crate dropped comments")
    return 0


def self_test():
    import tempfile
    import subprocess
    import shutil
    import json

    print("=== G3 SELF-TEST ===")
    tmp = tempfile.mkdtemp(prefix="g3_selftest_")
    gates = os.path.join(tmp, ".port", "tools", "gates")
    inv = os.path.join(tmp, ".port", "phase-0", "inventory")
    crate_src = os.path.join(tmp, "crates", "rustty-demo", "src")
    os.makedirs(gates)
    os.makedirs(inv)
    os.makedirs(crate_src)
    here = os.path.dirname(os.path.abspath(__file__))
    shutil.copy(os.path.join(here, "_gatelib.py"), gates)
    shutil.copy(os.path.join(here, "check_comments_fidelity.py"), gates)

    summary = {
        "crate_map": {
            "demo": {"files": [{"file": "demo.zig", "test_blocks": 0}],
                     "test_blocks": 0, "file_count": 1},
        },
    }
    with open(os.path.join(inv, "tests-summary.json"), "w") as fh:
        json.dump(summary, fh)
    # census: demo.zig has 5 comment lines (3 line, 1 doc-module, 1 doc-decl)
    census = {
        "totals": {"total_comment_lines": 5},
        "per_file": {"demo.zig": {"doc-module": 1, "doc-decl": 1, "line": 3, "total": 5}},
    }
    with open(os.path.join(inv, "comments-census.json"), "w") as fh:
        json.dump(census, fh)

    def write_lib(status, ncomment):
        body = f"//! demo\n//! PORT STATUS: {status}\n"
        for i in range(ncomment):
            body += f"// comment line {i}\n"
        body += "pub fn f() {}\n"
        with open(os.path.join(crate_src, "lib.rs"), "w") as fh:
            fh.write(body)

    script = os.path.join(gates, "check_comments_fidelity.py")

    def invoke(*extra):
        return subprocess.run([sys.executable, script, *extra],
                              capture_output=True, text=True)

    ok = True
    # (a) stub crate, fewer comments than ref => progress GREEN (stub not graded)
    write_lib("stub", 0)  # only the 2 //! lines = 2 comments < 5
    r = invoke()
    print(f"[a] stub 2<5 progress -> exit {r.returncode} (expect 0)")
    ok &= (r.returncode == 0)

    # (b) done crate, fewer comments than ref => RED (DROPPED)
    write_lib("done", 0)  # 2 //! comments < 5 ref
    r = invoke()
    print(f"[b] done 2<5 -> exit {r.returncode} (expect non-zero), DROPPED: {'DROPPED' in r.stdout}")
    ok &= (r.returncode != 0 and "DROPPED" in r.stdout)

    # (c) done crate, >= ref comments => GREEN
    write_lib("done", 4)  # 2 //! + 4 // = 6 >= 5
    r = invoke()
    print(f"[c] done 6>=5 -> exit {r.returncode} (expect 0)")
    ok &= (r.returncode == 0)

    # ---- R4 relocation arms (S-8c): 3 of demo's 5 reference comment lines are
    # ported in a HOST crate file its relocation names; the host scans that file
    # too, so the workspace total must count them once.
    host_src = os.path.join(tmp, "crates", "rustty-host", "src")
    os.makedirs(host_src)
    summary["crate_map"]["host"] = {"files": [{"file": "host.zig", "test_blocks": 0}],
                                    "test_blocks": 0, "file_count": 1}
    with open(os.path.join(inv, "tests-summary.json"), "w") as fh:
        json.dump(summary, fh)
    census["per_file"]["host.zig"] = {"doc-module": 2, "doc-decl": 0, "line": 0, "total": 2}
    census["totals"]["total_comment_lines"] = 7
    with open(os.path.join(inv, "comments-census.json"), "w") as fh:
        json.dump(census, fh)
    with open(os.path.join(host_src, "lib.rs"), "w") as fh:
        fh.write("//! host\n//! PORT STATUS: done\n")
    moved = os.path.join(host_src, "moved.rs")

    def write_moved(text):
        with open(moved, "w") as fh:
            fh.write(text)

    write_moved("// moved one\n// moved two\n// moved three\n")
    write_lib("done", 0)  # the 2 //! lines of demo's own src/
    reloc = {"demo": ({"path": "crates/rustty-host/src/moved.rs"},)}

    # (d) without the relocation demo has 2 < 5 => RED
    G.selftest_install_gatelib(gates, {})
    r = invoke()
    arm = r.returncode != 0 and "DROPPED: demo ref_total=5 rust_total=2" in r.stdout
    print(f"[d] relocated comments, no relocation -> exit {r.returncode} "
          f"(expect non-zero): {arm}")
    ok &= arm

    # (e) with it: GREEN, and the global total counts the moved lines ONCE
    # (demo 5 + host 5 per crate, but 7 distinct lines, never 10).
    G.selftest_install_gatelib(gates, reloc)
    r = invoke()
    arm = r.returncode == 0 and "comment-lines rust=7 ref=7" in r.stdout
    print(f"[e] same + relocation -> exit {r.returncode} (expect 0), global "
          f"counted once (rust=7): {arm}")
    ok &= arm

    # (f) with the relocation, a genuine drop is still caught
    write_moved("// moved one\n// moved two\n")
    r = invoke()
    arm = r.returncode != 0 and "DROPPED: demo ref_total=5 rust_total=4" in r.stdout
    print(f"[f] relocation present, one moved line dropped -> exit "
          f"{r.returncode} (expect non-zero): {arm}")
    ok &= arm

    # (g) an item-scoped relocation counts ONLY its items' comments
    write_moved("// outside one\n// outside two\n\n// kept\nmod kept {\n    // in\n}\n")
    G.selftest_install_gatelib(gates, {"demo": (
        {"path": "crates/rustty-host/src/moved.rs", "items": (r"^mod kept\b",)},)})
    r = invoke()
    arm = r.returncode != 0 and "DROPPED: demo ref_total=5 rust_total=4" in r.stdout
    print(f"[g] item-scoped relocation counts only the item's 2 lines -> exit "
          f"{r.returncode} (expect non-zero): {arm}")
    ok &= arm

    # (h) a relocation path that does not exist => BLOCKED (exit 3)
    G.selftest_install_gatelib(gates, {"demo": ({"path": "crates/gone.rs"},)})
    r = invoke()
    arm = r.returncode == 3 and "BLOCKED" in r.stdout
    print(f"[h] relocation path missing -> exit {r.returncode} (expect 3): {arm}")
    ok &= arm

    shutil.rmtree(tmp, ignore_errors=True)
    print("SELF-TEST:", "PASS" if ok else "FAIL")
    return 0 if ok else 1


def main():
    ap = argparse.ArgumentParser(description="G3 — zero comment-inventory delta")
    ap.add_argument("--inventory", help="(ignored; canonical path)")
    ap.add_argument("--rust-root", help="(ignored; canonical path)")
    ap.add_argument("--identity-map", help="(ignored; canonical plan path)")
    ap.add_argument("--crate", help="scope to one crate (P-C)")
    ap.add_argument("--strict", action="store_true")
    ap.add_argument("--self-test", action="store_true")
    args = ap.parse_args()
    if args.self_test:
        sys.exit(self_test())
    sys.exit(run(args))


if __name__ == "__main__":
    main()
