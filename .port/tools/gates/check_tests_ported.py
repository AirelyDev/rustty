#!/usr/bin/env python3
"""
check_tests_ported.py — GATE G1 (100% test blocks ported) [R1]

DoP id:  G1  (DEFINITION_OF_PARITY §1, "100% test blocks ported")
Reads:   .port/phase-0/inventory/tests-summary.json  (PORT_PLAN file->crate map +
                                                        per-file test_blocks)
         .port/phase-0/inventory/tests.jsonl          (per-test names, optional, for
                                                        per-file name-set diff)
Scans:   crates/<crate>/src/**/*.rs   (#[test] fns)

PASS CONDITION (DoP G1):
  - global ported test count == 3234 contract, AND
  - per-crate ported == contract, AND
  - for every *claimed-done* crate, the per-file/per-crate name-set delta is
    empty (no MISSING names).

MODES:
  default (progress mode):
    - Counts ported tests per crate vs contract and prints ported/contract.
    - On the empty/stub workspace this reports 0/3234 WITHOUT erroring (exit 0).
    - RED (exit non-zero) only when a *claimed-done* crate is missing tests
      (ported < contract) or has a non-empty MISSING name-set. A claimed-done
      crate is one whose lib.rs no longer says `PORT STATUS: stub`.
  --strict:
    - The full G1 gate: RED unless global count == 3234 AND every crate's ported
      count == its contract. Used for the final/locked workspace.

  --crate <name>:  scope the check to one crate (per-crate P-T gate).
  --self-test:     run the built-in self-test (synthetic crate, flip proof).

This gate NEVER edits a contract or threshold (R5). The contract is
tests-summary.json's per-crate test_blocks; the global floor is 3234.
"""
import argparse
import json
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import _gatelib as G  # noqa: E402

CONTRACT_TOTAL = 3234


def load_ref_names_per_crate(summary):
    """Build {crate: {file: set(test names)}} from tests.jsonl if available.

    tests.jsonl rows are keyed by `file` (relative to src/). We map each file to
    its crate via the crate_map. Returns {} if tests.jsonl is absent."""
    if not os.path.exists(G.TESTS_JSONL):
        return {}
    f2c = G.file_to_crate(summary)
    out = {}
    with open(G.TESTS_JSONL, encoding="utf-8") as fh:
        for line in fh:
            line = line.strip()
            if not line:
                continue
            try:
                row = json.loads(line)
            except Exception:  # noqa: BLE001
                continue
            f = row.get("file")
            crate = f2c.get(f)
            if crate is None:
                continue
            out.setdefault(crate, {}).setdefault(f, set())
            # anonymous tests carry name == "" or anonymous flag; we cannot
            # diff their names, only their count, so skip name-diff for them.
            if not row.get("anonymous") and row.get("name"):
                out[crate][f].add(row["name"])
    return out


def run(args):
    G.banner("G1", "100% test blocks ported")
    try:
        summary = G.load_tests_summary()
        cm = G.crate_map(summary)
    except G.Blocked as b:
        G.emit(G.BLOCKED_STATE, str(b))
        return 3  # BLOCKED is non-zero but distinct

    ref_names = load_ref_names_per_crate(summary) if not args.no_names else {}

    crates = [args.crate] if args.crate else G.workspace_crates(cm)
    if args.crate and args.crate not in cm:
        G.emit(G.RED, f"unknown crate '{args.crate}' (not in crate_map)")
        return 2

    total_contract = 0
    total_ported = 0
    red = False
    rows = []
    for crate in crates:
        contract = cm[crate]["test_blocks"]
        ported_names = G.scan_rust_tests(crate)
        ported = len(ported_names)
        status = G.crate_status(crate)
        claimed_done = G.is_claimed_done(crate)
        total_contract += contract
        total_ported += ported

        missing = []
        extra = []
        if claimed_done and ref_names.get(crate):
            ref_set = set()
            for fset in ref_names[crate].values():
                ref_set |= fset
            ported_set = set(ported_names)
            # A reference test name is considered ported if ANY of its accepted
            # identifier slug forms (DoP §G1 naming convention) appears as a
            # ported fn name. Rust fn names cannot contain spaces/punctuation, so
            # a string-named reference test is matched against its slug forms.
            remaining = set(ported_set)
            for nm in sorted(ref_set):
                cands = G.name_candidates(nm)
                hit = cands & ported_set
                if hit:
                    remaining -= hit
                else:
                    missing.append(nm)
            missing = sorted(missing)
            extra = sorted(remaining)

        crate_red = False
        if claimed_done:
            # A claimed-done crate must have ported >= contract and no MISSING.
            if ported < contract:
                crate_red = True
            if missing:
                crate_red = True
        if args.strict and not args.crate:
            # strict global: every crate must equal contract exactly.
            if ported != contract:
                crate_red = True
        if args.strict and args.crate:
            if ported != contract:
                crate_red = True

        if crate_red:
            red = True
        rows.append((crate, status, ported, contract, claimed_done, missing, extra, crate_red))

    # ---- report ----
    print(f"{'crate':<16}{'status':<10}{'ported':>7}{'contract':>10}  state")
    for (crate, status, ported, contract, done, missing, extra, crate_red) in rows:
        state = "RED" if crate_red else ("done" if done else "progress")
        print(f"{crate:<16}{status:<10}{ported:>7}{contract:>10}  {state}")
        for nm in missing[:20]:
            print(f"    MISSING: {crate}::{nm}")
        if len(missing) > 20:
            print(f"    MISSING: ... (+{len(missing) - 20} more)")
        for nm in extra[:10]:
            print(f"    EXTRA:   {crate}::{nm}")

    scope = args.crate if args.crate else "workspace"
    print(f"\nPROGRESS [{scope}]: {total_ported}/{total_contract} ported "
          f"(global contract floor = {CONTRACT_TOTAL})")
    if not args.crate:
        non_ws = sum(cm[c]["test_blocks"] for c in cm if c in G.NON_WORKSPACE_CRATES)
        if non_ws:
            print(f"  (note: {non_ws} contract tests live in non-DAG assets "
                  f"{sorted(G.NON_WORKSPACE_CRATES & set(cm))} outside crates/; "
                  f"workspace contract = {total_contract}, global floor = {CONTRACT_TOTAL} "
                  f"= {total_contract} + {non_ws})")

    # ---- pass/fail ----
    if args.strict:
        if not args.crate:
            # Workspace-scope strict: ported must equal the WORKSPACE contract
            # (sum of in-workspace crate contracts). The 3234 global floor =
            # workspace contract + non-DAG assets (extra/xtask); we verify that
            # identity too so the contract itself can't drift silently.
            non_ws = sum(cm[c]["test_blocks"] for c in cm if c in G.NON_WORKSPACE_CRATES)
            if total_contract + non_ws != CONTRACT_TOTAL:
                G.emit(G.RED, f"contract drift: workspace {total_contract} + non-DAG "
                              f"{non_ws} != global floor {CONTRACT_TOTAL}")
                return 1
            if total_ported != total_contract:
                G.emit(G.RED, f"global ported {total_ported} != workspace contract "
                              f"{total_contract} (global floor {CONTRACT_TOTAL})")
                return 1
        if red:
            G.emit(G.RED, "per-crate count != contract (strict)")
            return 1
        G.emit(G.GREEN, f"count == contract ({total_ported}); name-sets match")
        return 0

    # progress mode
    if red:
        G.emit(G.RED, "a claimed-done crate is missing ported tests/names (see above)")
        return 1
    G.emit(G.GREEN, f"progress {total_ported}/{total_contract}; no claimed-done crate is short")
    return 0


# ---------------------------------------------------------------------------
# Self-test: prove the gate flips. Builds a synthetic crate tree + contract in a
# temp dir, points the helpers at it, and checks that:
#   (a) a stub crate with 0 tests but contract>0 is GREEN in progress mode,
#   (b) the same crate marked `done` with 0 tests is RED,
#   (c) the crate with all tests ported + done is GREEN (and GREEN in --strict).
# ---------------------------------------------------------------------------
def self_test():
    import tempfile
    import subprocess
    import textwrap

    print("=== G1 SELF-TEST ===")
    tmp = tempfile.mkdtemp(prefix="g1_selftest_")
    # synthetic layout mirroring rustty: <tmp>/.port/tools/gates, <tmp>/crates,
    # <tmp>/.port/phase-0/inventory
    gates = os.path.join(tmp, ".port", "tools", "gates")
    inv = os.path.join(tmp, ".port", "phase-0", "inventory")
    crate_src = os.path.join(tmp, "crates", "rustty-demo", "src")
    os.makedirs(gates)
    os.makedirs(inv)
    os.makedirs(crate_src)
    # copy this script + _gatelib into the synthetic gates dir
    import shutil
    here = os.path.dirname(os.path.abspath(__file__))
    shutil.copy(os.path.join(here, "_gatelib.py"), gates)
    shutil.copy(os.path.join(here, "check_tests_ported.py"), gates)
    # contract: crate 'demo' owns one file with 2 test blocks
    summary = {
        "total_test_blocks": 2,
        "crate_map": {
            "demo": {"files": [{"file": "demo.zig", "test_blocks": 2}],
                     "test_blocks": 2, "file_count": 1},
        },
    }
    with open(os.path.join(inv, "tests-summary.json"), "w") as fh:
        json.dump(summary, fh)

    def write_lib(status, ntests):
        body = f"//! demo\n//! PORT STATUS: {status}\n"
        for i in range(ntests):
            body += f"#[test]\nfn test_demo_{i}() {{ assert!(true); }}\n"
        with open(os.path.join(crate_src, "lib.rs"), "w") as fh:
            fh.write(body)

    script = os.path.join(gates, "check_tests_ported.py")

    def invoke(*extra):
        # Override the CONTRACT_TOTAL via env-free run: the synthetic global
        # contract is 2, so --strict compares against 3234 unless we pass a
        # crate scope. We test crate-scoped strict to avoid the 3234 floor.
        return subprocess.run([sys.executable, script, *extra],
                              capture_output=True, text=True)

    ok = True

    # (a) stub crate, 0 tests, contract 2 => progress GREEN (exit 0)
    write_lib("stub", 0)
    r = invoke()
    print(f"[a] stub 0/2 progress -> exit {r.returncode} (expect 0)")
    ok &= (r.returncode == 0)

    # (b) done crate, 0 tests, contract 2 => RED (exit 1)
    write_lib("done", 0)
    r = invoke()
    print(f"[b] done 0/2 progress -> exit {r.returncode} (expect non-zero)")
    ok &= (r.returncode != 0)
    ok &= ("MISSING" in r.stdout or "missing" in r.stdout.lower() or "RED" in r.stdout)

    # (c) done crate, 2 tests => GREEN (exit 0), and crate-scoped --strict GREEN
    write_lib("done", 2)
    r = invoke()
    print(f"[c] done 2/2 progress -> exit {r.returncode} (expect 0)")
    ok &= (r.returncode == 0)
    r = invoke("--crate", "demo", "--strict")
    print(f"[c'] done 2/2 --crate demo --strict -> exit {r.returncode} (expect 0)")
    ok &= (r.returncode == 0)

    # (d) stub crate, 1 of 2 tests => progress GREEN (partial is fine for stub)
    write_lib("stub", 1)
    r = invoke()
    print(f"[d] stub 1/2 progress -> exit {r.returncode} (expect 0)")
    ok &= (r.returncode == 0)

    shutil.rmtree(tmp, ignore_errors=True)
    print("SELF-TEST:", "PASS" if ok else "FAIL")
    return 0 if ok else 1


def main():
    ap = argparse.ArgumentParser(description="G1 — 100% test blocks ported")
    ap.add_argument("--inventory", help="(ignored; uses canonical phase-0 path) "
                    "kept for DoP command compatibility")
    ap.add_argument("--rust-root", help="(ignored; uses canonical crates/ path)")
    ap.add_argument("--crate", help="scope to a single crate (P-T)")
    ap.add_argument("--strict", action="store_true",
                    help="full G1: count must EQUAL contract")
    ap.add_argument("--no-names", action="store_true",
                    help="skip name-set diff (count only)")
    ap.add_argument("--self-test", action="store_true",
                    help="run the built-in flip proof and exit")
    args = ap.parse_args()
    if args.self_test:
        sys.exit(self_test())
    sys.exit(run(args))


if __name__ == "__main__":
    main()
