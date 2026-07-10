#!/usr/bin/env python3
"""
check_assertions.py — GATE G2 (assertion-for-assertion) [R1]

DoP id:  G2  (DEFINITION_OF_PARITY §1, "Assertion-for-assertion")
Reads:   .port/phase-0/inventory/tests-summary.json  (global 11975 assertion floor,
                                                        per-file assertion counts)
         .port/phase-0/inventory/tests.jsonl          (per-test assertion counts +
                                                        names, for per-test parity)
Scans:   crates/<crate>/src/**/*.rs   (assertion-family macros,
                                        .unwrap_err/.expect_err, #[should_panic];
                                        .unwrap()/.expect(..)/? are NOT counted —
                                        they are Zig-`try` plumbing, not
                                        assertions; M7 undiluted count)

PASS CONDITION (DoP G2):
  - per-test  rust_assertions >= ref_assertions  (>= because idiomatic Rust may
    need an extra check to express one Zig `try`; never <), AND
  - global    sum(rust_assertions) >= sum(ref_assertions)  (floor 11975).
  A test with FEWER assertions than its contract is a blocker; reported as
    WEAKENED: <crate>::<test> ref=<n> rust=<m>.

MODES:
  default (progress mode):
    - Reports rust/ref assertion totals per crate; on empty/stub workspace the
      rust totals are 0 and the run does NOT error (exit 0) UNLESS a *claimed-done*
      crate weakens an assertion (rust < ref on a matched test, or crate total
      rust < ref). A claimed-done crate's lib.rs no longer says PORT STATUS: stub.
  --strict:
    - Full G2: RED unless global rust >= 11975 contract floor AND no test is
      WEAKENED.
  --crate <name>: scope to one crate (per-crate P-A gate).
  --self-test:    built-in flip proof.

Per-test matching resolves a reference test NAME (from tests.jsonl) to its
ported Rust fn(s) via layered identifier matching (H6 fix — raw string equality
compared only 43/3,065 named tests; a Rust fn cannot equal a string-named Zig
test):
  L1  name_candidates(ref) ∩ ported fn names   (DoP §G1 slug convention)
  L2  canonical form equality (lowercase, non-alphanumerics stripped) — covers
      camel-preserved slugs like pagelist_pointFromPin_active_no_history
  L3  canonical form + 'test' prefix/suffix — the fn-name-collision convention
      (ctrlOrSuper -> ctrl_or_super_test)
  L4  disambiguating-prefix ADDED by the port (Zig duplicates one name across
      types; Rust prefixes the type): fn canon ENDS WITH ref canon (>= 5 chars)
      ('parseCLI base' -> palette_parse_cli_base)
  L5  redundant type-prefix DROPPED by the port (the Rust test lives in the
      type's own module): ref canon ENDS WITH fn canon (fn canon >= 10 chars)
      ('SelectionGesture drag selection logic' -> drag_selection_logic)
When several ported fns resolve, the comparison bar is max(rust counts) vs the
strongest reference test of that name (same max-dedup as the ref side) — the
`>=` semantics are unchanged. Ported tests with no name match still contribute
to the crate total (so they can only help, never trigger a false WEAKENED).
This is a matcher correction, not a loosening: every pair compared before is
still compared, and ~2,900 previously-uncompared pairs now bind (R5 tightens).
"""
import argparse
import json
import os
import re
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import _gatelib as G  # noqa: E402

CONTRACT_TOTAL_ASSERTIONS = 11975


def _canon(s):
    """Canonical identifier form: lowercase with every non-alphanumeric
    stripped. 'PageList pointFromPin active' and
    pagelist_pointFromPin_active canonicalize identically."""
    return re.sub(r"[^a-z0-9]", "", s.lower())


def build_fn_index(rust_per_test):
    """Precompute the per-crate resolution index over ported test fns
    (rust_per_test maps fn name -> [assertion_count, ...], one entry per
    occurrence of the fn name): (fn_set, canon->fns map, [(fn, canon)] list)."""
    fn_set = set(rust_per_test)
    canon_map = {}
    canon_list = []
    for fn in fn_set:
        cf = _canon(fn)
        canon_map.setdefault(cf, []).append(fn)
        canon_list.append((fn, cf))
    return fn_set, canon_map, canon_list


def resolve_ref_test(name, fn_index):
    """Resolve a reference test name to the set of ported fn names it may
    legitimately have been ported as (layers L1-L5, see module docstring).
    Returns a (possibly empty) set. First matching layer wins so the most
    precise binding is used."""
    fn_set, canon_map, canon_list = fn_index
    # L1: exact slug-candidate intersection (DoP §G1 convention)
    hits = G.name_candidates(name) & fn_set
    if hits:
        return hits
    c = _canon(name)
    if not c:
        return set()
    # L2: canonical equality
    if c in canon_map:
        return set(canon_map[c])
    # L3: 'test' suffix/prefix (fn-name-collision convention)
    hits = set(canon_map.get(c + "test", [])) | set(canon_map.get("test" + c, []))
    if hits:
        return hits
    # L4: port added a disambiguating type/module prefix
    if len(c) >= 5:
        hits = {fn for fn, cf in canon_list if cf.endswith(c) and cf != c}
        if hits:
            return hits
    # L5: port dropped a redundant leading type name
    return {fn for fn, cf in canon_list
            if len(cf) >= 10 and c.endswith(cf) and cf != c}


def load_ref_assertions(summary):
    """Return (per_crate_total, per_crate_test_assertions) where:
      per_crate_total[crate] = sum of ref assertions for the crate's files
      per_crate_test_assertions[crate] = {test_name: ref_assertion_count}
    Built from tests.jsonl (authoritative per-test). Falls back to per-file
    totals from tests-summary.json when tests.jsonl is absent."""
    f2c = G.file_to_crate(summary)
    per_crate_total = {}
    per_test = {}
    if os.path.exists(G.TESTS_JSONL):
        with open(G.TESTS_JSONL, encoding="utf-8") as fh:
            for line in fh:
                line = line.strip()
                if not line:
                    continue
                try:
                    row = json.loads(line)
                except Exception:  # noqa: BLE001
                    continue
                crate = f2c.get(row.get("file"))
                if crate is None:
                    continue
                ac = int(row.get("assertion_count", 0))
                per_crate_total[crate] = per_crate_total.get(crate, 0) + ac
                nm = row.get("name")
                if nm and not row.get("anonymous"):
                    # if duplicate names within a crate, keep the max (so the
                    # parity bar is the strongest reference test of that name)
                    d = per_test.setdefault(crate, {})
                    d[nm] = max(d.get(nm, 0), ac)
    else:
        # fall back to summary per_file
        pf = summary.get("per_file", {})
        for f, info in pf.items():
            crate = f2c.get(f)
            if crate is None:
                continue
            per_crate_total[crate] = per_crate_total.get(crate, 0) + int(info.get("assertions", 0))
    return per_crate_total, per_test


def run(args):
    G.banner("G2", "assertion-for-assertion")
    try:
        summary = G.load_tests_summary()
        cm = G.crate_map(summary)
    except G.Blocked as b:
        G.emit(G.BLOCKED_STATE, str(b))
        return 3

    ref_total, ref_test = load_ref_assertions(summary)

    if args.crate:
        args.crate = G.normalize_crate(args.crate, cm)
    crates = [args.crate] if args.crate else G.workspace_crates(cm)
    if args.crate and args.crate not in cm:
        G.emit(G.RED, f"unknown crate '{args.crate}' (not in crate_map)")
        return 2

    grand_rust = 0
    grand_ref = 0
    compared_pairs = 0
    named_ref_tests = 0
    red = False
    rows = []
    weakened_all = []
    for crate in crates:
        rust_per_test = G.scan_rust_test_assertions(crate)
        rust_total = sum(sum(counts) for counts in rust_per_test.values())
        ref_crate_total = ref_total.get(crate, 0)
        claimed_done = G.is_claimed_done(crate)
        grand_rust += rust_total
        grand_ref += ref_crate_total

        # Resolve every named reference test to its ported fn(s) (L1-L5).
        # The compared-pair count is reported for ALL crates; the WEAKENED
        # verdict binds (RED) only for claimed-done crates, as before.
        fn_index = build_fn_index(rust_per_test)
        rt = ref_test.get(crate, {})
        named_ref_tests += len(rt)
        weakened = []
        for name, refc in rt.items():
            matched = resolve_ref_test(name, fn_index)
            if not matched:
                continue
            compared_pairs += 1
            rust_c = max(max(rust_per_test[fn]) for fn in matched)
            if claimed_done and rust_c < refc:
                weakened.append((name, refc, rust_c))
        if claimed_done:
            # also a claimed-done crate must not have a lower crate total
            if rust_total < ref_crate_total:
                weakened.append(("<crate-total>", ref_crate_total, rust_total))

        crate_red = bool(weakened) if claimed_done else False
        if args.strict:
            # strict per-crate: rust >= ref crate total
            if rust_total < ref_crate_total:
                crate_red = True
        if crate_red:
            red = True
        for w in weakened:
            weakened_all.append((crate, *w))
        rows.append((crate, G.crate_status(crate), rust_total, ref_crate_total,
                     claimed_done, crate_red))

    # ---- report ----
    print(f"{'crate':<16}{'status':<10}{'rust':>8}{'ref':>8}  state")
    for (crate, status, rust_total, ref_crate_total, done, crate_red) in rows:
        state = "RED" if crate_red else ("done" if done else "progress")
        print(f"{crate:<16}{status:<10}{rust_total:>8}{ref_crate_total:>8}  {state}")
    for (crate, name, refc, rustc) in weakened_all:
        print(f"    WEAKENED: {crate}::{name} ref={refc} rust={rustc}")

    scope = args.crate if args.crate else "workspace"
    print(f"\nPER-TEST [{scope}]: {compared_pairs}/{named_ref_tests} named reference "
          f"tests bound to a ported fn and compared (L1-L5)")
    print(f"PROGRESS [{scope}]: assertions rust={grand_rust} ref={grand_ref} "
          f"(global contract floor = {CONTRACT_TOTAL_ASSERTIONS})")

    if args.strict:
        if not args.crate and grand_rust < CONTRACT_TOTAL_ASSERTIONS:
            G.emit(G.RED, f"global rust assertions {grand_rust} < contract {CONTRACT_TOTAL_ASSERTIONS}")
            return 1
        if red:
            G.emit(G.RED, "an assertion was weakened (rust < ref); see WEAKENED above")
            return 1
        G.emit(G.GREEN, f"rust >= ref everywhere (rust={grand_rust})")
        return 0

    if red:
        G.emit(G.RED, "a claimed-done crate weakened an assertion (see WEAKENED above)")
        return 1
    G.emit(G.GREEN, f"progress rust={grand_rust}/ref={grand_ref}; no claimed-done crate weakened")
    return 0


def self_test():
    import tempfile
    import subprocess
    import shutil

    print("=== G2 SELF-TEST ===")
    tmp = tempfile.mkdtemp(prefix="g2_selftest_")
    gates = os.path.join(tmp, ".port", "tools", "gates")
    inv = os.path.join(tmp, ".port", "phase-0", "inventory")
    crate_src = os.path.join(tmp, "crates", "rustty-demo", "src")
    os.makedirs(gates)
    os.makedirs(inv)
    os.makedirs(crate_src)
    here = os.path.dirname(os.path.abspath(__file__))
    shutil.copy(os.path.join(here, "_gatelib.py"), gates)
    shutil.copy(os.path.join(here, "check_assertions.py"), gates)

    summary = {
        "total_assertions": 3,
        "crate_map": {
            "demo": {"files": [{"file": "demo.zig", "test_blocks": 1}],
                     "test_blocks": 1, "file_count": 1},
        },
    }
    with open(os.path.join(inv, "tests-summary.json"), "w") as fh:
        json.dump(summary, fh)
    # tests.jsonl: one ref test 'test_demo_0' with 3 assertions
    with open(os.path.join(inv, "tests.jsonl"), "w") as fh:
        fh.write(json.dumps({"file": "demo.zig", "name": "test_demo_0",
                             "anonymous": False, "assertion_count": 3}) + "\n")

    def write_lib(status, n_assert):
        body = f"//! demo\n//! PORT STATUS: {status}\n#[test]\nfn test_demo_0() {{\n"
        for _ in range(n_assert):
            body += "    assert!(true);\n"
        body += "}\n"
        with open(os.path.join(crate_src, "lib.rs"), "w") as fh:
            fh.write(body)

    script = os.path.join(gates, "check_assertions.py")

    def invoke(*extra):
        return subprocess.run([sys.executable, script, *extra],
                              capture_output=True, text=True)

    ok = True
    # (a) stub, 0 assertions, ref 3 => progress GREEN (exit 0)
    write_lib("stub", 0)
    r = invoke()
    print(f"[a] stub 0 vs ref 3 progress -> exit {r.returncode} (expect 0)")
    ok &= (r.returncode == 0)

    # (b) done, 2 assertions, ref 3 => RED (WEAKENED)
    write_lib("done", 2)
    r = invoke()
    print(f"[b] done 2 vs ref 3 -> exit {r.returncode} (expect non-zero), WEAKENED in out: {'WEAKENED' in r.stdout}")
    ok &= (r.returncode != 0 and "WEAKENED" in r.stdout)

    # (c) done, 3 assertions, ref 3 => GREEN
    write_lib("done", 3)
    r = invoke()
    print(f"[c] done 3 vs ref 3 -> exit {r.returncode} (expect 0)")
    ok &= (r.returncode == 0)

    # (d) done, 4 assertions, ref 3 => GREEN (>= allowed)
    write_lib("done", 4)
    r = invoke()
    print(f"[d] done 4 vs ref 3 -> exit {r.returncode} (expect 0)")
    ok &= (r.returncode == 0)

    # ---- per-test injection scenario (H6's escape) ----
    # Two STRING-named reference tests (the 94.6% case the raw-name comparison
    # could never bind): deleting an assertion from one test while padding the
    # other keeps the crate total constant — the per-test comparison alone must
    # catch it.
    summary2 = {
        "total_assertions": 3,
        "crate_map": {
            "demo": {"files": [{"file": "demo.zig", "test_blocks": 2}],
                     "test_blocks": 2, "file_count": 1},
        },
    }
    with open(os.path.join(inv, "tests-summary.json"), "w") as fh:
        json.dump(summary2, fh)
    with open(os.path.join(inv, "tests.jsonl"), "w") as fh:
        fh.write(json.dumps({"file": "demo.zig", "name": "Demo: case one",
                             "anonymous": False, "assertion_count": 2}) + "\n")
        fh.write(json.dumps({"file": "demo.zig", "name": "Demo: case two",
                             "anonymous": False, "assertion_count": 1}) + "\n")

    def write_lib2(status, n_one, n_two, pad_unwraps=0):
        body = f"//! demo\n//! PORT STATUS: {status}\n"
        for fn_name, n in (("demo_case_one", n_one), ("demo_case_two", n_two)):
            body += f"#[test]\nfn {fn_name}() {{\n"
            for _ in range(n):
                body += "    assert!(true);\n"
            for _ in range(pad_unwraps):
                body += "    let _ = Some(1).unwrap();\n"
            body += "}\n"
        with open(os.path.join(crate_src, "lib.rs"), "w") as fh:
            fh.write(body)

    # (e) faithful port: 2 + 1 == ref 2 + 1 => GREEN, both pairs bound
    write_lib2("done", 2, 1)
    r = invoke()
    bound = "PER-TEST [workspace]: 2/2" in r.stdout
    print(f"[e] done string-named 2+1 vs ref 2+1 -> exit {r.returncode} "
          f"(expect 0), both pairs bound: {bound}")
    ok &= (r.returncode == 0 and bound)

    # (f) INJECTION: delete one assertion from test one, pad test two — the
    # crate total is unchanged (3 == 3) so only the per-test check can flip RED.
    write_lib2("done", 1, 2)
    r = invoke()
    caught = "WEAKENED: demo::Demo: case one ref=2 rust=1" in r.stdout
    print(f"[f] done injected 1+2 vs ref 2+1 (total constant) -> exit "
          f"{r.returncode} (expect non-zero), per-test WEAKENED caught: {caught}")
    ok &= (r.returncode != 0 and caught)

    # (g) DILUTION INJECTION (M7): same deletion, but the gutted test is padded
    # with .unwrap() plumbing — must still be RED (unwrap is not an assertion).
    write_lib2("done", 1, 2, pad_unwraps=5)
    r = invoke()
    caught = "WEAKENED: demo::Demo: case one ref=2 rust=1" in r.stdout
    print(f"[g] done injected 1+2 with 5 .unwrap() pads -> exit {r.returncode} "
          f"(expect non-zero), undiluted WEAKENED caught: {caught}")
    ok &= (r.returncode != 0 and caught)

    shutil.rmtree(tmp, ignore_errors=True)
    print("SELF-TEST:", "PASS" if ok else "FAIL")
    return 0 if ok else 1


def main():
    ap = argparse.ArgumentParser(description="G2 — assertion-for-assertion")
    ap.add_argument("--inventory", help="(ignored; canonical path)")
    ap.add_argument("--rust-root", help="(ignored; canonical path)")
    ap.add_argument("--crate", help="scope to one crate (P-A)")
    ap.add_argument("--strict", action="store_true",
                    help="full G2: global rust >= 11975 and none weakened")
    ap.add_argument("--self-test", action="store_true")
    args = ap.parse_args()
    if args.self_test:
        sys.exit(self_test())
    sys.exit(run(args))


if __name__ == "__main__":
    main()
