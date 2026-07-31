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
    - The full G1 gate (DG3(a), 2026-07-10): RED unless global ported >= the
      contract AND every crate's ported >= its contract AND every crate's
      MISSING name-set is empty. EXTRA names are reported, never fatal:
      sanctioned rustty-only infra tests make exact `ported == contract`
      unsatisfiable, and the binding fidelity signal is "no reference test is
      missing" (MISSING == empty), not "no extra tests exist". The contract
      floors themselves are never lowered (R5). Used for the final/locked
      workspace.

  --crate <name>:  scope the check to one crate (per-crate P-T gate).
  --self-test:     run the built-in self-test (synthetic crate, flip proof).

NAME MATCHING: a reference test name counts as ported when the SHARED layered
resolver (_gatelib.resolve_ref_test — the exact binding G2 uses for
assertion-for-assertion) resolves it to a ported #[test] fn. Layers L1-L5 and
the verified symbol-worded (F7) forms are documented in _gatelib. Every
many-to-one binding is printed as MULTI-BOUND (a candidate form may never
silently fold two reference names onto one fn).

This gate NEVER edits a contract or threshold (R5). The contract is
tests-summary.json's per-crate test_blocks; the global floor is 3234.
"""
import argparse
import json
import os
import re
import subprocess
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

    if args.crate:
        args.crate = G.normalize_crate(args.crate, cm)
    crates = [args.crate] if args.crate else G.workspace_crates(cm)
    if args.crate and args.crate not in cm:
        G.emit(G.RED, f"unknown crate '{args.crate}' (not in crate_map)")
        return 2

    total_contract = 0
    deleted_hits = []
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
        multi = []
        # The name-set diff binds for claimed-done crates (progress mode) and
        # for EVERY crate under --strict (DG3(a): strict requires MISSING == ∅
        # workspace-wide, done-claim or not).
        if (claimed_done or args.strict) and ref_names.get(crate):
            ref_set = set()
            for fset in ref_names[crate].values():
                ref_set |= fset
            ported_set = set(ported_names)
            # A reference test name is considered ported if the SHARED layered
            # resolver (_gatelib.resolve_ref_test, L1-L5 — the same binding G2
            # uses for assertion parity) resolves it to at least one ported fn.
            # Rust fn names cannot contain spaces/punctuation, so a string-named
            # reference test is matched against its slug/canon forms.
            fn_index = G.build_fn_index(ported_set)
            bound = {}  # ported fn -> [ref names that resolved to it]
            for nm in sorted(ref_set):
                hits = G.resolve_ref_test(nm, fn_index)
                if hits:
                    for h in hits:
                        bound.setdefault(h, []).append(nm)
                else:
                    missing.append(nm)
            # Drop reference names upstream has since DELETED (verified against
            # the live oracle on every run — see _deleted_upstream).
            if missing:
                gone = _deleted_upstream()
                kept = []
                for nm in missing:
                    key = f"{crate}::{nm}"
                    if key in gone:
                        deleted_hits.append((key, gone[key]))
                    else:
                        kept.append(nm)
                missing = kept
            missing = sorted(missing)
            extra = sorted(ported_set - set(bound))
            # Multiplicity accounting: a candidate form must not silently fold
            # two reference names onto one ported fn — every many-to-one
            # binding is REPORTED (never hidden) so the EXTRA+MISSING numbers
            # stay reconcilable by eye.
            multi = sorted((fn, sorted(nms)) for fn, nms in bound.items()
                           if len(nms) > 1)

        crate_red = False
        if claimed_done:
            # A claimed-done crate must have ported >= contract and no MISSING.
            if ported < contract:
                crate_red = True
            if missing:
                crate_red = True
        if args.strict:
            # DG3(a) strict per-crate: ported >= contract AND MISSING == ∅.
            # (Exact equality was unsatisfiable: sanctioned rustty-only infra
            # tests exist workspace-wide. EXTRA is reported above, not fatal.)
            if ported < contract or missing:
                crate_red = True

        if crate_red:
            red = True
        rows.append((crate, status, ported, contract, claimed_done, missing, extra, multi, crate_red))

    # ---- report ----
    print(f"{'crate':<16}{'status':<10}{'ported':>7}{'contract':>10}  state")
    for (crate, status, ported, contract, done, missing, extra, multi, crate_red) in rows:
        state = "RED" if crate_red else ("done" if done else "progress")
        print(f"{crate:<16}{status:<10}{ported:>7}{contract:>10}  {state}")
        for nm in missing[:20]:
            print(f"    MISSING: {crate}::{nm}")
        if len(missing) > 20:
            print(f"    MISSING: ... (+{len(missing) - 20} more)")
        for nm in extra[:10]:
            print(f"    EXTRA:   {crate}::{nm}")
        if len(extra) > 10:
            print(f"    EXTRA:   ... (+{len(extra) - 10} more)")
        for fn, nms in multi[:10]:
            print(f"    MULTI-BOUND: {crate}::{fn} <- {len(nms)} ref names: "
                  + "; ".join(repr(n) for n in nms[:4])
                  + (" ..." if len(nms) > 4 else ""))
        if len(multi) > 10:
            print(f"    MULTI-BOUND: ... (+{len(multi) - 10} more)")

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
            # DG3(a): >= the workspace contract (sanctioned extra tests allowed;
            # a shortfall or any MISSING name below is still RED).
            if total_ported < total_contract:
                G.emit(G.RED, f"global ported {total_ported} < workspace contract "
                              f"{total_contract} (global floor {CONTRACT_TOTAL})")
                return 1
        if red:
            G.emit(G.RED, "per-crate ported < contract or MISSING names (strict, DG3(a))")
            return 1
        G.emit(G.GREEN, f"ported >= contract ({total_ported}/{total_contract}); "
                        f"no MISSING names (DG3(a))")
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

    # (e) DG3(a): done crate, 3 tests vs contract 2 => --strict GREEN
    # (ported >= contract with sanctioned extra tests; old `==` semantics
    # would have falsely RED'd this).
    write_lib("done", 3)
    r = invoke("--crate", "demo", "--strict")
    print(f"[e] done 3/2 --crate demo --strict -> exit {r.returncode} (expect 0)")
    ok &= (r.returncode == 0)

    # (f) DG3(a) flip proof: done crate, 1 of 2 tests => --strict RED
    # (ported < contract is still fatal; >= did not weaken the shortfall arm).
    write_lib("done", 1)
    r = invoke("--crate", "demo", "--strict")
    print(f"[f] done 1/2 --crate demo --strict -> exit {r.returncode} (expect non-zero)")
    ok &= (r.returncode != 0)

    # (g) NAME-BINDING flip proof (layered resolver): two string-named
    # reference tests; the port carries conforming slugs => GREEN. Renaming
    # one ported fn to an unrelated name (count unchanged!) must flip RED with
    # a MISSING line — proving the resolver did not degrade into count-only.
    with open(os.path.join(inv, "tests.jsonl"), "w") as fh:
        fh.write(json.dumps({"file": "demo.zig", "name": "Demo: case one",
                             "anonymous": False, "assertion_count": 1}) + "\n")
        fh.write(json.dumps({"file": "demo.zig", "name": "Demo: case two",
                             "anonymous": False, "assertion_count": 1}) + "\n")

    def write_lib_named(fn_one, fn_two):
        body = "//! demo\n//! PORT STATUS: done\n"
        for fn_name in (fn_one, fn_two):
            body += f"#[test]\nfn {fn_name}() {{ assert!(true); }}\n"
        with open(os.path.join(crate_src, "lib.rs"), "w") as fh:
            fh.write(body)

    write_lib_named("demo_case_one", "demo_case_two")
    r = invoke()
    print(f"[g] done, both names bound -> exit {r.returncode} (expect 0)")
    ok &= (r.returncode == 0)

    write_lib_named("demo_case_one", "totally_unrelated_name")
    r = invoke()
    caught = "MISSING: demo::Demo: case two" in r.stdout
    print(f"[g'] done, one fn renamed to unrelated (count constant) -> exit "
          f"{r.returncode} (expect non-zero), MISSING caught: {caught}")
    ok &= (r.returncode != 0 and caught)

    shutil.rmtree(tmp, ignore_errors=True)
    print("SELF-TEST:", "PASS" if ok else "FAIL")
    return 0 if ok else 1



# ---------------------------------------------------------------------------
# UPSTREAM-DELETED CONTRACT ENTRIES
# ---------------------------------------------------------------------------
# The contract is frozen at 4749c4e93 and deliberately never regenerated. The
# pin has since advanced to 6ad1fe7d8, so the contract now drifts in BOTH
# directions: it cannot see the tests upstream added (CENSUS_DELTA.md, +316
# blocks) and it still DEMANDS the tests upstream deleted. Demanding a test that
# no longer exists upstream is not fidelity, it is the opposite.
#
# THIS IS NOT A WEAKENING (R5). A ledger row is never trusted on its own; the
# exemption is re-derived here on every run and honoured only when BOTH hold:
#   1. the name is absent from the LIVE oracle tree, and
#   2. the named commit that removed it is reachable in the pin range.
# So a bogus row cannot hide a genuinely unported test — if the test still
# exists upstream, (1) fails and the row is ignored.
_ORACLE_SRC = os.path.join(
    os.path.dirname(subprocess.run(
        ["git", "rev-parse", "--path-format=absolute", "--git-common-dir"],
        cwd=G.RUSTTY_ROOT, capture_output=True, text=True).stdout.strip()
        or G.RUSTTY_ROOT),
    "..", "ghostty", "src")


def _deleted_upstream():
    """{contract entry -> commit}, only for rows that still verify."""
    led = os.path.join(G.RUSTTY_ROOT, ".port/phase-3/CONTRACT_DELETIONS.md")
    if not os.path.exists(led):
        return {}
    out = {}
    src = os.path.abspath(_ORACLE_SRC)
    for m in re.finditer(r"^\|\s*`([^`]+)`\s*\|\s*`([0-9a-f]{7,40})`\s*\|",
                         open(led, errors="ignore").read(), re.M):
        entry, sha = m.group(1), m.group(2)
        name = entry.split("::", 1)[1] if "::" in entry else entry
        base = name.split(": ", 1)[1] if ": " in name else name
        # (1) STILL PRESENT upstream? then the row does not apply.
        r = subprocess.run(["grep", "-rl", 'test "' + base, src],
                           capture_output=True, text=True)
        if r.returncode == 0 and r.stdout.strip():
            continue
        # (2) the named commit must actually be in the advanced range.
        r = subprocess.run(
            ["git", "-C", os.path.join(src, ".."), "merge-base",
             "--is-ancestor", sha, "HEAD"], capture_output=True, text=True)
        if r.returncode != 0:
            continue
        out[entry] = sha
    return out


def main():
    ap = argparse.ArgumentParser(description="G1 — 100% test blocks ported")
    ap.add_argument("--inventory", help="(ignored; uses canonical phase-0 path) "
                    "kept for DoP command compatibility")
    ap.add_argument("--rust-root", help="(ignored; uses canonical crates/ path)")
    ap.add_argument("--crate", help="scope to a single crate (P-T)")
    ap.add_argument("--strict", action="store_true",
                    help="full G1 (DG3(a)): ported >= contract AND empty "
                         "MISSING name-set; EXTRA reported, not fatal")
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
