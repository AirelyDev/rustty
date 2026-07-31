#!/usr/bin/env python3
"""P3-W9 T7 step 3 — which catalogs carry the bare `Close Split` msgid?

Upstream `9c6f287aa` added it as an UNTRANSLATED entry to the catalogs that
lacked it. The plan and the area-G assessment both assumed rustty's catalogs
still lacked it (naming `po/be.po`). Running this reports `LACK (0): []` —
WS13 T4 (`485d65b`) already added it to all 31 when it regenerated the catalogs
against rustty's own paths. Kept as the reproduction for that finding, which is
recorded in `.port/phase-3/NON_PORTS.md` row G12.
"""
import glob
import os

BARE = 'msgid "Close Split"\nmsgstr'

have, lack = [], []
for p in sorted(glob.glob("po/*.po")):
    if BARE in open(p, encoding="utf-8").read():
        have.append(os.path.basename(p))
    else:
        lack.append(os.path.basename(p))

print(f"HAVE ({len(have)}): {have}")
print(f"LACK ({len(lack)}): {lack}")
