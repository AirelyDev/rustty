#!/usr/bin/env python3
"""P3-W7 T2 — add the `EnvMapExt` import to the files that call put/getv/removev/hasv."""
import re
import sys

for p in sys.argv[1:]:
    s = open(p, encoding="utf-8").read()
    if "EnvMapExt" in s:
        continue
    m = re.search(r"^use [^\n]*\n", s, re.M)
    s = s[: m.end()] + "use rustty_global::EnvMapExt as _;\n" + s[m.end() :]
    open(p, "w", encoding="utf-8").write(s)
    print("patched", p)
