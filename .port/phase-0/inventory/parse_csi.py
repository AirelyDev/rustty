#!/usr/bin/env python3
"""Parse CSI handler dispatch from ghostty src/terminal/stream.zig.

csiDispatch switch on input.final runs lines ~807-1943. Each top-level arm is
at 16-space indent: `'X' => ...` or `'X', 'Y' => ...` or `else => ...`.
We capture the final byte(s) and any leading `//` comment (the command name).
"""
import re
import json

PATH = "/Users/gael/projects/ghostty/src/terminal/stream.zig"
with open(PATH) as f:
    lines = f.readlines()

START = 807  # 1-based, switch start
END = 1943

arm_re = re.compile(r"^                ((?:'[^']*'(?:, '[^']*')*)|else)\s*=>")

arms = []
for idx in range(START - 1, END):
    line = lines[idx]
    m = arm_re.match(line)
    if not m:
        continue
    label = m.group(1)
    # gather preceding comment lines (// ...) immediately above (any indent)
    comments = []
    k = idx - 1
    while k >= 0:
        s = lines[k].strip()
        if s.startswith("//"):
            comments.insert(0, s.lstrip("/").strip())
            k -= 1
        elif s == "":
            break
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
