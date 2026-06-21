#!/usr/bin/env python3
"""Parse terminfo capabilities from ghostty src/terminfo/ghostty.zig.

Each entry: `.{ .name = "X", .value = .{ .TYPE = VALUE } },`
TYPE in {boolean, numeric, string}. Also capture the entry names list.
"""
import re
import json

PATH = "/Users/gael/projects/ghostty/src/terminfo/ghostty.zig"
with open(PATH) as f:
    text = f.read()
    lines = text.splitlines()

# names: lines inside `.names = &.{` ... `},` that are a bare string literal
names = []
in_names = False
for line in lines:
    if ".names = &.{" in line:
        in_names = True
        continue
    if in_names:
        if line.strip() == "},":
            break
        m = re.match(r'^\s*"([^"]+)",\s*$', line)
        if m:
            names.append(m.group(1))
# Capabilities
cap_re = re.compile(
    r'\.\{ \.name = "(?P<name>[^"]+)", \.value = \.\{ \.(?P<type>boolean|numeric|string)(?:\s*=\s*(?P<val>.*?))?\s*\} \}'
)
caps = []
for i, line in enumerate(lines):
    m = cap_re.search(line)
    if m:
        val = m.group("val")
        if val is not None:
            val = val.strip()
            if val == "{}":
                val = None
        caps.append({
            "name": m.group("name"),
            "type": m.group("type"),
            "value": val,
            "line": i + 1,
        })

out = {
    "entry_names": names,
    "count": len(caps),
    "by_type": {
        "boolean": sum(1 for c in caps if c["type"] == "boolean"),
        "numeric": sum(1 for c in caps if c["type"] == "numeric"),
        "string": sum(1 for c in caps if c["type"] == "string"),
    },
    "capabilities": caps,
}
print(json.dumps(out, indent=2))
