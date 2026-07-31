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
test). The resolver lives in _gatelib (build_fn_index/resolve_ref_test) and is
SHARED with G1's name-set diff so both gates bind names one way; layers L1-L5
are documented on _gatelib.resolve_ref_test (L1 additionally accepts the
symbol-worded F7 slug variants, and canonicalization applies Unicode NFC —
both only ADD bindings).
When several ported fns resolve, the comparison bar is max(rust counts) vs the
strongest reference test of that name (same max-dedup as the ref side) — the
`>=` semantics are unchanged. Ported tests with no name match still contribute
to the crate total (so they can only help, never trigger a false WEAKENED).
This is a matcher correction, not a loosening: every pair compared before is
still compared, and ~2,900 previously-uncompared pairs now bind (R5 tightens).

UPSTREAM-SHRUNK TESTS (.port/audits/upstream-assertion-drift.tsv)
The frozen inventory is pinned at `4749c4e93`; the oracle has moved to
`6ad1fe7d8`. When upstream REWRITES a test with fewer assertions, a faithful
port sits below the FROZEN bar and is reported WEAKENED although nothing was
weakened. Such a test may be listed in that ledger — but the row is NOT trusted:
each one is RE-VERIFIED at runtime against the LIVE oracle (same tokenizer as
the frozen inventory), and the comparison is RE-BASED on the live count rather
than dropped (rust >= live still binds). A row that misstates the live tree,
names a commit that never touched the file, claims a shrink that did not happen,
or is no longer needed is itself RED. See that file's header for the full
contract; `--self-test` demonstrates each arm failing.
"""
import argparse
import json
import os
import subprocess
import sys
import tempfile

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import _gatelib as G  # noqa: E402
from _gatelib import build_fn_index, resolve_ref_test  # noqa: E402,F401 (shared L1-L5 resolver)

CONTRACT_TOTAL_ASSERTIONS = 11975

DRIFT_LEDGER = os.path.join(G.PORT_DIR, "audits", "upstream-assertion-drift.tsv")
EXTRACT_TESTS = os.path.join(G.INVENTORY_DIR, "extract_tests.py")


def _oracle_root():
    """The oracle checkout, which sits BESIDE THE MAIN checkout.

    Copied from check_census_staleness.py: inside a git worktree `ROOT/..` is
    `.claude/worktrees/`, not the parent of the real checkout, so `--git-common-dir`
    is what makes these tools runnable from an agent worktree without a symlink.
    """
    common = subprocess.run(
        ["git", "rev-parse", "--path-format=absolute", "--git-common-dir"],
        cwd=G.RUSTTY_ROOT, capture_output=True, text=True).stdout.strip()
    main_checkout = os.path.dirname(common) if common else G.RUSTTY_ROOT
    return os.path.abspath(os.path.join(main_checkout, "..", "ghostty"))


def load_drift_ledger():
    """{(crate, test): {frozen, live, commit, notes}} from the drift TSV.

    A missing ledger is simply an empty ledger: no row can then excuse anything,
    which is the fail-closed direction.
    """
    rows = {}
    if not os.path.exists(DRIFT_LEDGER):
        return rows
    with open(DRIFT_LEDGER, encoding="utf-8") as fh:
        for line in fh:
            line = line.rstrip("\n")
            if not line.strip() or line.lstrip().startswith("#"):
                continue
            parts = line.split("\t")
            if parts[0] == "crate":  # header
                continue
            if len(parts) < 5:
                continue
            crate, test, frozen, live, commit = (p.strip() for p in parts[:5])
            try:
                rows[(crate, test)] = {
                    "frozen": int(frozen),
                    "live": int(live),
                    "commit": commit,
                    "notes": parts[5] if len(parts) > 5 else "",
                }
            except ValueError:
                # An unparseable row must not silently vanish into "no row".
                rows[(crate, test)] = {"frozen": None, "live": None,
                                       "commit": commit, "notes": "MALFORMED"}
    return rows


class _LiveOracle:
    """Lazy, once-per-run census of the LIVE oracle tree.

    Uses the phase-0 tokenizer itself so live and frozen counts are produced by
    the same code — a second implementation would measure the two tokenizers
    rather than the two trees.
    """

    def __init__(self):
        self._tests = None      # {name: (assertion_count, oracle_file)}
        self.available = None   # None = not yet probed
        self.reason = ""

    def _load(self):
        if self._tests is not None or self.available is False:
            return
        oracle = _oracle_root()
        if not os.path.isdir(os.path.join(oracle, "src")):
            self.available = False
            self.reason = f"oracle checkout not found at {oracle}"
            return
        if not os.path.exists(EXTRACT_TESTS):
            self.available = False
            self.reason = f"phase-0 tokenizer missing at {EXTRACT_TESTS}"
            return
        with tempfile.TemporaryDirectory() as out:
            r = subprocess.run([sys.executable, EXTRACT_TESTS,
                                os.path.join(oracle, "src"), out],
                               capture_output=True, text=True)
            if r.returncode != 0:
                self.available = False
                self.reason = f"live census failed: {r.stderr.strip()[:300]}"
                return
            tests = {}
            with open(os.path.join(out, "tests.jsonl"), encoding="utf-8") as fh:
                for line in fh:
                    line = line.strip()
                    if not line:
                        continue
                    row = json.loads(line)
                    nm = row.get("name")
                    if not nm or row.get("anonymous"):
                        continue
                    ac = int(row.get("assertion_count", 0))
                    prev = tests.get(nm)
                    # Same max-dedup as the frozen side: the bar is the
                    # strongest live test of that name.
                    if prev is None or ac > prev[0]:
                        tests[nm] = (ac, row.get("file"))
            self._tests = tests
            self.available = True
        self.oracle = oracle

    def lookup(self, name):
        self._load()
        if not self.available:
            return None
        return self._tests.get(name)

    def commit_touches(self, sha, oracle_file):
        """Is `sha` a real oracle commit that modified `src/<oracle_file>`?"""
        self._load()
        if not self.available:
            return False
        if not sha or not oracle_file:
            return False
        path = os.path.join("src", oracle_file)
        r = subprocess.run(["git", "-C", self.oracle, "log", "-1",
                            "--format=%H", sha, "--", path],
                           capture_output=True, text=True)
        return r.returncode == 0 and r.stdout.strip() != ""


def check_drift_row(live_oracle, crate, name, refc, rust_c, row):
    """Decide whether an upstream-shrink ledger row excuses this WEAKENED pair.

    Returns (verdict, message) where verdict is one of:
      'accept'  — re-verified against the live oracle; rust >= live bar.
      'reject'  — the row does not hold up; the WEAKENED verdict stands (RED).
      'blocked' — the live oracle is unavailable, so nothing can be verified.
    """
    if row.get("frozen") is None or row.get("live") is None:
        return "reject", "malformed ledger row (frozen/live are not integers)"
    live = live_oracle.lookup(name)
    if live is None and not live_oracle.available:
        return "blocked", live_oracle.reason
    if live is None:
        return "reject", ("ledger claims an upstream shrink but the LIVE oracle "
                          "has no test of this name (deleted? renamed? then the "
                          "row is wrong)")
    live_count, oracle_file = live
    if row["frozen"] != refc:
        return "reject", (f"ledger frozen={row['frozen']} but the frozen "
                          f"inventory says {refc}")
    if row["live"] != live_count:
        return "reject", (f"ledger live={row['live']} but the LIVE oracle "
                          f"measures {live_count} — re-measure and re-argue")
    if live_count >= refc:
        return "reject", (f"ledger claims a shrink, but live={live_count} is not "
                          f"below frozen={refc}")
    if not live_oracle.commit_touches(row["commit"], oracle_file):
        return "reject", (f"upstream_commit {row['commit']!r} is not an oracle "
                          f"commit that touched src/{oracle_file}")
    if rust_c < live_count:
        return "reject", (f"rust={rust_c} is below the LIVE bar {live_count} — "
                          f"this IS a weakened test, drift or no drift")
    return "accept", (f"upstream shrank {refc}->{live_count} in {row['commit']}; "
                      f"rust={rust_c} >= live bar {live_count}")


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
    drift_ledger = load_drift_ledger()
    live_oracle = _LiveOracle()
    drift_used = set()      # ledger rows actually consumed this run
    drift_notes = []        # (crate, name, verdict, message)
    blocked_rows = []
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
        accepted_drift = 0   # sum of VERIFIED (frozen - live) deltas, this crate
        for name, refc in rt.items():
            matched = resolve_ref_test(name, fn_index)
            if not matched:
                continue
            compared_pairs += 1
            rust_c = max(max(rust_per_test[fn]) for fn in matched)
            if claimed_done and rust_c < refc:
                # Below the FROZEN bar. It may be that upstream itself shrank
                # this test after the pin — but only a ledger row that survives
                # live re-verification may say so, and the comparison is then
                # RE-BASED on the live oracle, never dropped.
                row = drift_ledger.get((crate, name))
                if row is None:
                    weakened.append((name, refc, rust_c))
                    continue
                verdict, msg = check_drift_row(live_oracle, crate, name,
                                               refc, rust_c, row)
                drift_used.add((crate, name))
                drift_notes.append((crate, name, verdict, msg))
                if verdict == "accept":
                    accepted_drift += row["frozen"] - row["live"]
                    continue
                if verdict == "blocked":
                    blocked_rows.append((crate, name, msg))
                    continue
                weakened.append((name, refc, rust_c))

        # The crate TOTAL is measured against the same frozen inventory, so a
        # verified upstream shrink has to come off it too — otherwise the coarse
        # total re-creates the false RED the per-test check just resolved. The
        # subtraction is EXACTLY the live-verified (frozen - live) delta, never
        # a round number and never a hand-set floor.
        crate_bar = ref_crate_total - accepted_drift
        if claimed_done:
            # also a claimed-done crate must not have a lower crate total
            if rust_total < crate_bar:
                weakened.append(("<crate-total>", crate_bar, rust_total))

        crate_red = bool(weakened) if claimed_done else False
        if args.strict:
            # strict per-crate: rust >= ref crate total
            if rust_total < crate_bar:
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

    # ---- upstream-shrink ledger: report, and keep the ledger itself honest --
    for (crate, name, verdict, msg) in drift_notes:
        tag = {"accept": "DRIFT-OK", "reject": "DRIFT-REJECTED",
               "blocked": "DRIFT-UNVERIFIABLE"}[verdict]
        print(f"    {tag}: {crate}::{name} — {msg}")
    # A row that excused nothing this run is STALE: either the port has since
    # regained the assertions, or the row was never needed. Same rule as D1's
    # allowlist — unused rows must not accumulate into a standing subtraction.
    # (Only meaningful on a full-workspace run; --crate scopes the numerator.)
    if not args.crate:
        for (crate, name) in sorted(drift_ledger):
            if (crate, name) not in drift_used:
                print(f"    DRIFT-STALE: {crate}::{name} — ledger row excused "
                      f"nothing this run; remove it")
                red = True

    scope = args.crate if args.crate else "workspace"
    print(f"\nPER-TEST [{scope}]: {compared_pairs}/{named_ref_tests} named reference "
          f"tests bound to a ported fn and compared (L1-L5)")
    print(f"PROGRESS [{scope}]: assertions rust={grand_rust} ref={grand_ref} "
          f"(global contract floor = {CONTRACT_TOTAL_ASSERTIONS})")

    # An unverifiable ledger row is never a pass: the gate declines to give a
    # verdict (exit 3, surfaced by ci.sh as a WARNING) instead of greening on an
    # unchecked claim. Genuine WEAKENED findings still take precedence and go RED.
    if blocked_rows and not red:
        for (crate, name, msg) in blocked_rows:
            G.emit(G.BLOCKED_STATE,
                   f"{crate}::{name}: upstream-shrink ledger row cannot be "
                   f"re-verified ({msg})")
        return 3

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
    ok &= drift_self_test()
    print("SELF-TEST:", "PASS" if ok else "FAIL")
    return 0 if ok else 1


def drift_self_test():
    """Hermetic proof that the upstream-shrink ledger cannot excuse a real gap.

    Builds a throwaway rustty root NEXT TO a throwaway git 'oracle', so the live
    re-verification path runs for real: the phase-0 tokenizer is executed over
    the fake oracle tree and `git log` is asked about the cited commit.
    """
    import tempfile
    import subprocess as sp
    import shutil

    print("=== G2 SELF-TEST (upstream-shrink ledger) ===")
    base = tempfile.mkdtemp(prefix="g2_drift_")
    root = os.path.join(base, "rustty")
    oracle = os.path.join(base, "ghostty")
    gates = os.path.join(root, ".port", "tools", "gates")
    inv = os.path.join(root, ".port", "phase-0", "inventory")
    audits = os.path.join(root, ".port", "audits")
    crate_src = os.path.join(root, "crates", "rustty-demo", "src")
    for d in (gates, inv, audits, crate_src, os.path.join(oracle, "src")):
        os.makedirs(d)
    here = os.path.dirname(os.path.abspath(__file__))
    for f in ("_gatelib.py", "check_assertions.py"):
        shutil.copy(os.path.join(here, f), gates)
    shutil.copy(os.path.join(G.INVENTORY_DIR, "extract_tests.py"), inv)

    # --- fake oracle: the test now has ONE assertion (upstream shrank it) ----
    def write_oracle(n_assert):
        body = 'const std = @import("std");\n\ntest "Demo: case one" {\n'
        body += "    const testing = std.testing;\n"
        for _ in range(n_assert):
            body += "    try testing.expectEqual(1, 1);\n"
        body += "}\n"
        with open(os.path.join(oracle, "src", "demo.zig"), "w") as fh:
            fh.write(body)

    write_oracle(1)
    env = dict(os.environ, GIT_AUTHOR_NAME="g2", GIT_AUTHOR_EMAIL="g2@x",
               GIT_COMMITTER_NAME="g2", GIT_COMMITTER_EMAIL="g2@x")
    sp.run(["git", "init", "-q"], cwd=oracle, env=env, capture_output=True)
    # An unrelated first commit, so "the cited sha exists" and "the cited sha
    # touched this file" are genuinely different questions.
    with open(os.path.join(oracle, "README"), "w") as fh:
        fh.write("unrelated\n")
    sp.run(["git", "add", "README"], cwd=oracle, env=env, capture_output=True)
    sp.run(["git", "commit", "-qm", "unrelated"], cwd=oracle, env=env, capture_output=True)
    unrelated_sha = sp.run(["git", "rev-parse", "HEAD"], cwd=oracle, env=env,
                           capture_output=True, text=True).stdout.strip()
    sp.run(["git", "add", "src/demo.zig"], cwd=oracle, env=env, capture_output=True)
    sp.run(["git", "commit", "-qm", "shrink the test"], cwd=oracle, env=env,
           capture_output=True)
    shrink_sha = sp.run(["git", "rev-parse", "HEAD"], cwd=oracle, env=env,
                        capture_output=True, text=True).stdout.strip()

    # --- frozen inventory: the test USED to have 2 assertions ---------------
    with open(os.path.join(inv, "tests-summary.json"), "w") as fh:
        json.dump({"total_assertions": 2,
                   "crate_map": {"demo": {
                       "files": [{"file": "demo.zig", "test_blocks": 1}],
                       "test_blocks": 1, "file_count": 1}}}, fh)
    with open(os.path.join(inv, "tests.jsonl"), "w") as fh:
        fh.write(json.dumps({"file": "demo.zig", "name": "Demo: case one",
                             "anonymous": False, "assertion_count": 2}) + "\n")

    def write_port(n_assert):
        body = "//! demo\n//! PORT STATUS: done\n#[test]\nfn demo_case_one() {\n"
        for _ in range(n_assert):
            body += "    assert!(true);\n"
        body += "}\n"
        # An unrelated ported test, so the CRATE TOTAL is not just this one test.
        # Without it every arm below would trip the coarse crate-total check as
        # well, and could not tell the per-test verdicts apart.
        body += "#[test]\nfn filler_unrelated() {\n    assert!(true);\n}\n"
        with open(os.path.join(crate_src, "lib.rs"), "w") as fh:
            fh.write(body)

    def write_ledger(frozen, live, sha, name="Demo: case one"):
        with open(os.path.join(audits, "upstream-assertion-drift.tsv"), "w") as fh:
            fh.write("crate\ttest\tfrozen\tlive\tupstream_commit\tnotes\n")
            fh.write(f"demo\t{name}\t{frozen}\t{live}\t{sha}\tself-test row\n")

    script = os.path.join(gates, "check_assertions.py")

    def invoke():
        return sp.run([sys.executable, script], capture_output=True, text=True)

    ok = True

    def arm(label, cond, r):
        nonlocal ok
        print(f"  {'ok  ' if cond else 'FAIL'} {label} (exit {r.returncode})")
        if not cond:
            print(r.stdout)
        ok &= cond

    # (h) no ledger at all: port 1 < frozen 2 => RED (the baseline behavior).
    write_port(1)
    r = invoke()
    arm("[h] no ledger, rust 1 < frozen 2 -> RED",
        r.returncode != 0 and "WEAKENED" in r.stdout, r)

    # (i) honest row, live-verified: frozen 2 -> live 1, rust 1 => GREEN.
    write_ledger(2, 1, shrink_sha)
    r = invoke()
    arm("[i] verified row (2->1, rust 1 >= live 1) -> GREEN",
        r.returncode == 0 and "DRIFT-OK" in r.stdout, r)

    # (j) the row MISSTATES the live tree (claims live=0) => RED.
    write_ledger(2, 0, shrink_sha)
    r = invoke()
    arm("[j] ledger live=0 but oracle measures 1 -> RED",
        r.returncode != 0 and "DRIFT-REJECTED" in r.stdout, r)

    # (k) the cited commit never touched the file => RED. This is the arm that
    #     stops a real sha from being pasted in as generic-looking evidence.
    write_ledger(2, 1, unrelated_sha)
    r = invoke()
    arm("[k] commit did not touch the oracle file -> RED",
        r.returncode != 0 and "DRIFT-REJECTED" in r.stdout, r)

    # (l) THE ONE THAT MATTERS: a correct row, but the port is below the LIVE
    #     bar too. The drift is real and the test is STILL gutted => RED.
    write_ledger(2, 1, shrink_sha)
    write_port(0)
    r = invoke()
    arm("[l] rust 0 < live bar 1 (real gap behind a true row) -> RED",
        r.returncode != 0 and "below the LIVE bar" in r.stdout, r)

    # (m) a row that excuses nothing is STALE => RED.
    write_port(2)
    r = invoke()
    arm("[m] stale row (port meets the frozen bar) -> RED",
        r.returncode != 0 and "DRIFT-STALE" in r.stdout, r)

    # (n) oracle gone: the row cannot be verified => BLOCKED (3), never GREEN.
    write_port(1)
    shutil.rmtree(oracle)
    r = invoke()
    arm("[n] oracle absent -> BLOCKED (exit 3), not green",
        r.returncode == 3 and "BLOCKED" in (r.stdout + r.stderr), r)

    shutil.rmtree(base, ignore_errors=True)
    return ok


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
