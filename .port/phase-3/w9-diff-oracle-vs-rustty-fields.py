#!/usr/bin/env python3
"""P3-W9 T3 step 3 — set-diff the oracle `Config` fields against rustty's
`CONFIG_FIELD_NAMES`, at the pin and at oracle head."""

import re
import subprocess

FIELD = re.compile(r'^(@"[^"]+"|[A-Za-z_][A-Za-z0-9_]*)\s*:\s')
SKIP = re.compile(r"^(pub const |const |fn |test |var |pub var )")
END = re.compile(r"^pub fn ")


def oracle_fields(rev):
    txt = subprocess.check_output(
        ["git", "-C", "/Users/gael/projects/ghostty", "show", f"{rev}:src/config/Config.zig"],
        text=True,
    )
    out = []
    for line in txt.split("\n"):
        if END.match(line):
            break
        if SKIP.match(line):
            continue
        m = FIELD.match(line)
        if m and m.group(1) != "_":
            out.append(m.group(1).strip('@"'))
    return out


src = open("crates/rustty-config/src/config_struct.rs").read()
block = src.split("const CONFIG_FIELD_NAMES: &[&[u8]] = &[", 1)[1].split("\n    ];", 1)[0]
rustty = re.findall(r'b"([^"]+)"', block)

for rev in ("4749c4e93", "origin/main"):
    o = oracle_fields(rev)
    print(f"--- oracle {rev}: {len(o)} fields | rustty: {len(rustty)} ---")
    print("  in oracle, NOT in rustty:", sorted(set(o) - set(rustty)))
    print("  in rustty, NOT in oracle:", sorted(set(rustty) - set(o)))
