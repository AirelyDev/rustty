#!/usr/bin/env python3
"""Payoff triage v2: classify every G1 MISSING name on a claimed-done crate
into naming-convention families vs truly-absent. Read-only."""
import re
import sys
import os

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "tools", "gates"))
import _gatelib as G  # noqa: E402
from check_tests_ported import load_ref_names_per_crate  # noqa: E402

CAMEL = r"(?<=[a-z0-9])(?=[A-Z])|(?<=[A-Z])(?=[A-Z][a-z])|(?<=[A-Za-z])(?=[0-9])"


def canon(name):
    words = []
    for tok in re.split(r"[^A-Za-z0-9]+", name):
        if tok:
            words.extend(w for w in re.split(CAMEL, tok) if w)
    return "_".join(w.lower() for w in words)


def squash(c):
    return c.replace("_", "")


summary = G.load_tests_summary()
cm = G.crate_map(summary)
ref_names = load_ref_names_per_crate(summary)

fam_totals = {}
for crate in G.workspace_crates(cm):
    if not G.is_claimed_done(crate) or not ref_names.get(crate):
        continue
    ported = list(G.scan_rust_tests(crate))
    ported_set = set(ported)
    by_canon, by_squash = {}, {}
    for fn in ported:
        base = fn[:-5] if fn.endswith("_test") else fn
        for variant in {fn, base}:
            c = canon(variant)
            by_canon.setdefault(c, fn)
            by_squash.setdefault(squash(c), fn)
    ref_set = set()
    for fset in ref_names[crate].values():
        ref_set |= fset
    rows = []
    for nm in sorted(ref_set):
        if G.name_candidates(nm) & ported_set:
            continue
        rc = canon(nm)
        rc_to = canon(nm.replace("->", " to "))
        fam, fn = None, None
        if rc in by_canon:
            fam, fn = "F2-canon", by_canon[rc]
        elif squash(rc) in by_squash:
            fam, fn = "F3-squash", by_squash[squash(rc)]
        elif rc_to in by_canon or squash(rc_to) in by_squash:
            fam, fn = "F5-arrow-to", by_canon.get(rc_to) or by_squash[squash(rc_to)]
        else:
            sq = squash(rc)
            sq_to = squash(rc_to)
            hits = [f for s, f in by_squash.items() if s.endswith(sq) or s.endswith(sq_to)]
            if hits:
                fam, fn = "F4-prefix", hits[0]
            else:
                fam, fn = "F6-ABSENT", ""
        rows.append((fam, nm, fn))
    if not rows:
        continue
    print(f"== {crate}")
    for fam, nm, fn in rows:
        fam_totals[fam] = fam_totals.get(fam, 0) + 1
        print(f"   {fam:<11} {nm!r} -> {fn}")
print("\nFAMILY TOTALS:", dict(sorted(fam_totals.items())))
