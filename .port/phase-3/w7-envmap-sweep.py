#!/usr/bin/env python3
"""P3-W7 T2 — mechanical half of the EnvMap byte re-typing sweep.

Rewrites the `HashMap<String,String>` spellings of the environment map to the
byte-typed `EnvMapExt` API (`put`/`getv`/`removev`/`hasv`). Only touches
mechanical, unambiguous forms; everything else is fixed by hand.

Run once; it is recorded here (not in a scratch dir) so the diff is auditable.
"""
import re
import sys

FILES = sys.argv[1:]

# `X.insert("KEY".to_string(), ` -> `X.put(b"KEY", `
INSERT_LIT = re.compile(r'\b(\w+)\.insert\(\s*"([^"\\]*)"\.to_string\(\),\s*')
# `X.get("KEY")` -> `X.getv(b"KEY")`
GET_LIT = re.compile(r'\b(\w+)\.get\("([^"\\]*)"\)')
# `X.remove("KEY")` -> `X.removev(b"KEY")`
REMOVE_LIT = re.compile(r'\b(\w+)\.remove\("([^"\\]*)"\)')
# `X.contains_key("KEY")` -> `X.hasv(b"KEY")`
CONTAINS_LIT = re.compile(r'\b(\w+)\.contains_key\("([^"\\]*)"\)')

VARS = {"env", "envmap", "environ_map", "env_map"}


def keep(name: str) -> bool:
    return name in VARS


for path in FILES:
    src = open(path, encoding="utf-8").read()
    out = []
    for line in src.split("\n"):
        # never rewrite reference-comment lines (they are the Zig original)
        stripped = line.lstrip()
        if stripped.startswith("//"):
            out.append(line)
            continue
        line = INSERT_LIT.sub(
            lambda m: f'{m.group(1)}.put(b"{m.group(2)}", ' if keep(m.group(1)) else m.group(0),
            line,
        )
        line = GET_LIT.sub(
            lambda m: f'{m.group(1)}.getv(b"{m.group(2)}")' if keep(m.group(1)) else m.group(0),
            line,
        )
        line = REMOVE_LIT.sub(
            lambda m: f'{m.group(1)}.removev(b"{m.group(2)}")' if keep(m.group(1)) else m.group(0),
            line,
        )
        line = CONTAINS_LIT.sub(
            lambda m: f'{m.group(1)}.hasv(b"{m.group(2)}")' if keep(m.group(1)) else m.group(0),
            line,
        )
        out.append(line)
    open(path, "w", encoding="utf-8").write("\n".join(out))
    print(f"swept {path}")
