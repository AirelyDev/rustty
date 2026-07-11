#!/usr/bin/env python3
"""T2 measurement helper: exact MISSING/EXTRA counts per crate using the SAME
name-set logic as check_tests_ported.py, ignoring claimed-done status (i.e. a
done-flip simulation for crates like vt that are not yet flipped). Temporary —
not a gate; deleted before commit."""
import os
import sys

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "tools", "gates"))
import _gatelib as G  # noqa: E402
import check_tests_ported as CT  # noqa: E402

summary = G.load_tests_summary()
cm = G.crate_map(summary)
ref_names = CT.load_ref_names_per_crate(summary)

for crate in sys.argv[1:]:
    crate = G.normalize_crate(crate, cm)
    ref_set = set()
    for fset in ref_names.get(crate, {}).values():
        ref_set |= fset
    ported_set = set(G.scan_rust_tests(crate))
    remaining = set(ported_set)
    missing = []
    for nm in sorted(ref_set):
        hit = G.name_candidates(nm) & ported_set
        if hit:
            remaining -= hit
        else:
            missing.append(nm)
    print(f"{crate}: ref_named={len(ref_set)} ported={len(ported_set)} "
          f"MISSING={len(missing)} EXTRA={len(remaining)}")
    for nm in missing:
        print(f"    MISSING: {crate}::{nm}")
