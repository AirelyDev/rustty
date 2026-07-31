#!/usr/bin/env python3
"""P3-W9 T5 — compare the vendored theme corpus against a freshly-extracted
upstream tarball. Usage: w9-themes-diff.py <extracted-dir>"""
import hashlib
import os
import sys

VEND = "crates/rustty-extra/themes"
NEW = sys.argv[1]

vend = {f for f in os.listdir(VEND) if not f.endswith(".md")}
new = set(os.listdir(NEW))

added = sorted(new - vend)
removed = sorted(vend - new)
print(f"vendored (non-.md) = {len(vend)}   upstream = {len(new)}")
print(f"ADDED   ({len(added)}): {added[:12]}{' …' if len(added) > 12 else ''}")
print(f"REMOVED ({len(removed)}): {removed}")

changed = []
for f in sorted(vend & new):
    a = hashlib.sha256(open(os.path.join(VEND, f), "rb").read()).hexdigest()
    b = hashlib.sha256(open(os.path.join(NEW, f), "rb").read()).hexdigest()
    if a != b:
        changed.append(f)
print(f"CONTENT-CHANGED ({len(changed)}): {changed}")
