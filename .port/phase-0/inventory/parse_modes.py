#!/usr/bin/env python3
"""Parse terminal modes from ghostty src/terminal/modes.zig.

The `entries` table: `.{ .name = "X", .value = N, .ansi = true, .default = true },`
ansi defaults false (=> DEC mode), default defaults false.
"""
import re
import json

PATH = "/Users/gael/projects/ghostty/src/terminal/modes.zig"
with open(PATH) as f:
    lines = f.readlines()

entry_re = re.compile(r'\.\{ \.name = "(?P<name>[^"]+)", \.value = (?P<value>\d+)(?P<rest>[^}]*)\}')
modes = []
in_entries = False
for i, line in enumerate(lines):
    if "const entries: []const ModeEntry = &.{" in line:
        in_entries = True
        continue
    if in_entries and line.rstrip() == "};":
        break
    if not in_entries:
        continue
    m = entry_re.search(line)
    if not m:
        continue
    rest = m.group("rest")
    ansi = ".ansi = true" in rest
    default = ".default = true" in rest
    disabled = ".disabled = true" in rest
    modes.append({
        "name": m.group("name"),
        "value": int(m.group("value")),
        "kind": "ANSI" if ansi else "DEC",
        "default": default,
        "disabled": disabled,
        "line": i + 1,
    })

out = {
    "count": len(modes),
    "ansi": sum(1 for x in modes if x["kind"] == "ANSI"),
    "dec": sum(1 for x in modes if x["kind"] == "DEC"),
    "default_on": sum(1 for x in modes if x["default"]),
    "disabled": sum(1 for x in modes if x["disabled"]),
    "modes": modes,
}
print(json.dumps(out, indent=2))
