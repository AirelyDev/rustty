#!/usr/bin/env python3
"""G-PARITY — every audited parity finding is closed, waived-with-reason, or open.

WHY: docs/parity-audit/ found 497 surviving functional-parity findings. Without a
gate, "we fixed them all" is a claim; with one, it is a measurement. The ledger is
the single source of truth (.port/parity-closure/LEDGER.tsv) and this gate is the
only thing that reads its `state` column, so closing a row is a deliberate edit.

R5: this gate never lowers its own bar. A WAIVED row MUST name a reason file+line;
a row with state=WAIVED and no reason is RED, not green.
"""
import csv, os, sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
LEDGER = os.path.join(ROOT, "parity-closure", "LEDGER.tsv")
VALID = {"OPEN", "CLOSED", "WAIVED"}

def verdict(rows):
    """Return (rc, lines). rc 0 GREEN, 1 RED."""
    out, bad_state, waived_no_reason = [], [], []
    counts = {"OPEN": 0, "CLOSED": 0, "WAIVED": 0}
    for r in rows:
        st = (r.get("state") or "").strip()
        if st not in VALID:
            bad_state.append(f"{r.get('id')}: state={st!r}")
            continue
        counts[st] += 1
        if st == "WAIVED" and not (r.get("owner") or "").strip():
            waived_no_reason.append(r.get("id"))
    total = sum(counts.values())
    out.append(f"G-PARITY: {total} finding(s) — "
               f"{counts['CLOSED']} closed, {counts['WAIVED']} waived, {counts['OPEN']} open")
    rc = 0
    if bad_state:
        out.append(f"RED: {len(bad_state)} row(s) with an unknown state: {bad_state[:5]}")
        rc = 1
    if waived_no_reason:
        out.append("RED: WAIVED rows must put the justification (file:line or doc ref) in the")
        out.append(f"     `owner` column; missing on: {waived_no_reason[:5]}")
        rc = 1
    if counts["OPEN"]:
        out.append(f"RED: {counts['OPEN']} finding(s) still OPEN")
        rc = 1
    if rc == 0:
        out.append("GREEN: every audited parity finding is closed or waived-with-reason.")
    return rc, out

def load(path):
    with open(path, newline="") as fh:
        return list(csv.DictReader(fh, delimiter="\t"))

def self_test():
    print("=== G-PARITY SELF-TEST ===")
    green = [{"id": "S1-F1", "state": "CLOSED", "owner": ""},
             {"id": "S2-F1", "state": "WAIVED", "owner": "docs/parity-audit/src.md#s2-f1"}]
    red_open = green + [{"id": "S3-F1", "state": "OPEN", "owner": ""}]
    red_waive = [{"id": "S4-F1", "state": "WAIVED", "owner": ""}]
    red_bogus = [{"id": "S5-F1", "state": "DONE", "owner": ""}]
    rc_g, _ = verdict(green)
    rc_o, _ = verdict(red_open)
    rc_w, _ = verdict(red_waive)
    rc_b, _ = verdict(red_bogus)
    print(f"  all closed/waived -> {rc_g} (expect 0)")
    print(f"  one still OPEN    -> {rc_o} (expect 1)")
    print(f"  WAIVED, no reason -> {rc_w} (expect 1)")
    print(f"  unknown state     -> {rc_b} (expect 1)")
    ok = (rc_g, rc_o, rc_w, rc_b) == (0, 1, 1, 1)
    print("SELF-TEST GREEN: the gate flips RED on an open row, an unjustified waiver,"
          " and an unknown state." if ok else "SELF-TEST RED: the gate did not flip.")
    return 0 if ok else 1

if __name__ == "__main__":
    if "--self-test" in sys.argv:
        sys.exit(self_test())
    if not os.path.exists(LEDGER):
        print(f"BLOCKED: ledger not found at {LEDGER}", file=sys.stderr)
        sys.exit(3)
    rows = load(LEDGER)
    if not rows:
        print("BLOCKED: ledger has no rows — refusing to report a vacuous green", file=sys.stderr)
        sys.exit(3)
    rc, lines = verdict(rows)
    print("\n".join(lines))
    sys.exit(rc)
