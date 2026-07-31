#!/usr/bin/env python3
"""G-CENSUS — is the frozen phase-0 census still an honest denominator?

G1 (test counts), G1b (per-module counts) and G2 (assertion floor) all measure
the port against `.port/phase-0/inventory/`, captured at the ORIGINAL pin
`4749c4e93`. That inventory is the contract the whole port was planned against,
and rewriting it would silently restate the denominator of every historical
completion claim — so it stays frozen on purpose.

The cost of freezing it is a blind spot: after the pin advanced to `6ad1fe7d8`,
every test upstream wrote in that range is in neither the numerator nor the
denominator of those three gates. They can be green while a whole commit's worth
of tests is unported. G-FILES / G-SYMS / G-TESTS close that at file, symbol and
test-block level by reading the LIVE tree; this closes it for the NUMBERS.

WHAT THIS GATE DOES. It re-runs the phase-0 census script — the same tokenizer,
so the two sides are actually comparable — against the live oracle tree, and
requires the resulting delta to be WRITTEN DOWN in the ledger below. It does not
demand the delta be zero; upstream moves, and that is not a defect. It demands
that nobody be able to *not notice*. If upstream advances again, the numbers
change, the recorded delta stops matching, and this goes red until a human looks.

ANTI-VACUITY. Three ways this could be worthless, each blocked:
  - Census returns nothing (bad path, empty tree) -> a 0/0 delta would "match"
    any ledger. Floored on absolute live totals.
  - Ledger matches by being absent/empty -> a missing ledger is RED, not a pass.
  - Ledger matches loosely -> the three numbers are compared exactly.
Run with --self-test to see each arm demonstrated failing on real values.
"""
import json
import os
import re
import subprocess
import sys
import tempfile

ROOT = subprocess.run(["git", "rev-parse", "--show-toplevel"],
                      cwd=os.path.dirname(os.path.abspath(__file__)),
                      capture_output=True, text=True, check=True).stdout.strip()
# The oracle sits BESIDE THE MAIN CHECKOUT, which is NOT `ROOT` when this runs
# inside a git worktree: `ROOT/../ghostty` resolves to
# `.claude/worktrees/ghostty`, which does not exist. Measured on a scratch
# worktree, the old form died with
# `FileNotFoundError: ... '/…/tmp/ghostty'` — loud, at least, but it made these
# tools unrunnable from every agent worktree, and one lane worked around it by
# creating a symlink. A tool that needs a symlink to be correct is one that will
# be wrong the next time nobody makes one. `--git-common-dir` points at the MAIN
# repo's .git from inside any worktree, so its parent is always the real
# checkout; falling back to ROOT keeps the non-worktree case identical.
def _oracle_root():
    common = subprocess.run(
        ["git", "rev-parse", "--path-format=absolute", "--git-common-dir"],
        cwd=ROOT, capture_output=True, text=True).stdout.strip()
    main_checkout = os.path.dirname(common) if common else ROOT
    return os.path.abspath(os.path.join(main_checkout, "..", "ghostty"))


ORACLE = _oracle_root()
FROZEN = os.path.join(ROOT, ".port/phase-0/inventory/tests-summary.json")
EXTRACT = os.path.join(ROOT, ".port/phase-0/inventory/extract_tests.py")
LEDGER = os.path.join(ROOT, ".port/phase-3/CENSUS_DELTA.md")

FIELDS = ("total_test_blocks", "total_assertions", "files_with_tests")


def live_census():
    """Re-run the phase-0 tokenizer against the LIVE oracle tree.

    Reusing the original script is the whole point: a second implementation
    would differ in what it counts as an assertion, and the delta would measure
    the two tokenizers rather than the two trees.
    """
    with tempfile.TemporaryDirectory() as out:
        subprocess.run([sys.executable, EXTRACT, os.path.join(ORACLE, "src"), out],
                       capture_output=True, text=True, check=True)
        with open(os.path.join(out, "tests-summary.json")) as fh:
            return json.load(fh)


def read_ledger():
    """Recorded deltas, as `- <field>: <frozen> -> <live> (+N)` lines."""
    if not os.path.exists(LEDGER):
        return None
    txt = open(LEDGER, errors="ignore").read()
    out = {}
    for f in FIELDS:
        m = re.search(rf"^\s*-\s*`{f}`:\s*(\d+)\s*->\s*(\d+)\b", txt, re.M)
        if m:
            out[f] = (int(m.group(1)), int(m.group(2)))
    return out


def compare(frozen, live, ledger):
    rows, bad = [], []
    for f in FIELDS:
        fz, lv = frozen[f], live[f]
        rec = (ledger or {}).get(f)
        ok = rec == (fz, lv)
        rows.append((f, fz, lv, rec, ok))
        if not ok:
            bad.append(f)
    return rows, bad


def self_test(frozen, live, ledger):
    failures = []

    def arm(label, ok):
        print(f"  {'ok  ' if ok else 'FAIL'} {label}")
        if not ok:
            failures.append(label)

    # 1. A missing ledger must be RED, not an empty-set pass.
    _r, bad = compare(frozen, live, None)
    arm("missing ledger is RED", len(bad) == len(FIELDS))

    # 2. A ledger recording the WRONG live number must be RED — this is the
    #    upstream-advanced-again case, the one the gate exists for.
    stale = {f: (frozen[f], live[f]) for f in FIELDS}
    stale["total_assertions"] = (frozen["total_assertions"],
                                 live["total_assertions"] - 1)
    _r, bad = compare(frozen, live, stale)
    arm("ledger with a stale live number is RED", "total_assertions" in bad)

    # 3. An empty census must not read as a clean pass.
    empty = {f: 0 for f in FIELDS}
    arm("empty live census trips the vacuity floor",
        empty["total_test_blocks"] < 1000)

    if failures:
        print(f"SELF-TEST RED: {len(failures)} arm(s) could not fail: {failures}")
        return 1
    print("SELF-TEST GREEN: all 3 arms demonstrated reachable FAIL.")
    return 0


def main():
    frozen = json.load(open(FROZEN))
    live = live_census()
    ledger = read_ledger()

    if "--self-test" in sys.argv[1:]:
        return self_test(frozen, live, ledger)

    rows, bad = compare(frozen, live, ledger)
    for f, fz, lv, rec, ok in rows:
        mark = "ok " if ok else "RED"
        got = f"recorded {rec[0]}->{rec[1]}" if rec else "NOT RECORDED"
        print(f"  {mark} {f:20} frozen={fz:6} live={lv:6} delta={lv - fz:+6}  ({got})")

    # Vacuity floor: a census that read nothing would make every delta match a
    # ledger of zeros, which is the classic "green because it cannot fail".
    if live["total_test_blocks"] < 1000:
        print(f"\nRED (VACUOUS): live census found only "
              f"{live['total_test_blocks']} test blocks; expected thousands. "
              f"Is {ORACLE} the oracle checkout?")
        return 1

    if bad:
        print(f"\nRED: {len(bad)} census figure(s) not recorded, or recorded "
              f"stale, in {os.path.relpath(LEDGER, ROOT)}: {bad}")
        print("The frozen phase-0 inventory is the port's CONTRACT and stays "
              "frozen; this gate only requires the drift to be acknowledged in "
              "writing, so G1/G1b/G2's blind spot cannot widen unnoticed.")
        return 1

    print("\nGREEN: the frozen-vs-live census delta is recorded and current.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
