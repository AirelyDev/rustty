#!/usr/bin/env python3
"""Parse SGR attribute kinds from ghostty src/terminal/sgr.zig.

The authoritative list is the `Tag = lib.Enum(lib.target, &.{ "...", ... })`
string list (lines ~80-115). Cross-checked against the union member tags.
"""
import re
import json

PATH = "/Users/gael/projects/ghostty/src/terminal/sgr.zig"
with open(PATH) as f:
    lines = f.readlines()

# Find the Tag = lib.Enum block
start = None
for i, l in enumerate(lines):
    if l.strip().startswith("pub const Tag = lib.Enum("):
        start = i
        break
# Collect string literals until the closing of the &.{ ... }
tags = []
depth = 0
collecting = False
for i in range(start, len(lines)):
    line = lines[i]
    if "&.{" in line:
        collecting = True
        continue
    if collecting:
        if re.match(r'^\s*\},?\s*$', line) and "}" in line:
            # closing of the &.{ } list
            break
        m = re.match(r'^\s*"([^"]+)",\s*$', line)
        if m:
            tags.append({"name": m.group(1), "line": i + 1})

out = {"count": len(tags), "attributes": tags}
print(json.dumps(out, indent=2))
