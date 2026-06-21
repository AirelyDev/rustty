#!/usr/bin/env python3
"""Parse ESC handler dispatch from ghostty src/terminal/stream.zig.

escDispatch switch runs lines ~2105-2376. Top-level arms at 16-space indent.
"""
import re
import json

PATH = "/Users/gael/projects/ghostty/src/terminal/stream.zig"
with open(PATH) as f:
    lines = f.readlines()

START = 2105
END = 2376
arm_re = re.compile(r"^                ((?:'[^']*'(?:, '[^']*')*)|else)\s*=>")

arms = []
for idx in range(START - 1, END):
    m = arm_re.match(lines[idx])
    if not m:
        continue
    label = m.group(1)
    comments = []
    k = idx - 1
    while k >= 0:
        s = lines[k].strip()
        if s.startswith("//"):
            comments.insert(0, s.lstrip("/").strip())
            k -= 1
        else:
            break
    finals = re.findall(r"'([^']*)'", label)
    arms.append({
        "finals": finals if finals else ["else"],
        "comment": " | ".join(comments) if comments else None,
        "line": idx + 1,
    })

result = {
    "count_arms": len([a for a in arms if a["finals"] != ["else"]]),
    "has_else": any(a["finals"] == ["else"] for a in arms),
    "arms": arms,
}
print(json.dumps(result, indent=2))
