#!/usr/bin/env python3
"""T2 residue classifier (analysis only, NOT a gate — deleted before commit).
For each still-MISSING ref name, probe whether a ported fn exists under a
looser correspondence (case/underscore-insensitive slug, digit-split
insensitive, or suffix match = module-prefix dropped). Classifies residue:
  NONCONF  — test exists, name violates the composed-slug convention
  SUFFIX   — test exists with the leading type/module prefix dropped
  ABSENT   — no plausible ported fn found (potential REAL R1 gap)
"""
import os
import re
import sys

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "tools", "gates"))
import _gatelib as G  # noqa: E402
import check_tests_ported as CT  # noqa: E402


def canon(s):
    # case- and separator-insensitive canonical: alnum only, lowercase
    return re.sub(r"[^A-Za-z0-9]+", "", s).lower()


summary = G.load_tests_summary()
cm = G.crate_map(summary)
ref_names = CT.load_ref_names_per_crate(summary)

for crate in sys.argv[1:]:
    crate = G.normalize_crate(crate, cm)
    ref_set = set()
    for fset in ref_names.get(crate, {}).values():
        ref_set |= fset
    ported_set = set(G.scan_rust_tests(crate))
    ported_canon = {}
    for fn in ported_set:
        ported_canon.setdefault(canon(fn), []).append(fn)
    missing = [nm for nm in sorted(ref_set) if not (G.name_candidates(nm) & ported_set)]
    counts = {"NONCONF": 0, "SUFFIX": 0, "ABSENT": 0}
    absent = []
    for nm in missing:
        c = canon(nm)
        if c in ported_canon:
            counts["NONCONF"] += 1
            continue
        # module-prefix dropped: some ported fn's canon is a suffix of the
        # ref's canon (len >= 8 to avoid junk matches)
        hit = None
        for pc in ported_canon:
            if len(pc) >= 8 and c.endswith(pc):
                hit = pc
                break
        if hit:
            counts["SUFFIX"] += 1
        else:
            counts["ABSENT"] += 1
            absent.append(nm)
    print(f"{crate}: missing={len(missing)} -> NONCONF={counts['NONCONF']} "
          f"SUFFIX={counts['SUFFIX']} ABSENT={counts['ABSENT']}")
    for nm in absent:
        print(f"    ABSENT: {crate}::{nm}")
