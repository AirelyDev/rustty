#!/usr/bin/env python3
"""Parse keybind Action union tags from ghostty src/input/Binding.zig.

`pub const Action = union(enum) {` starts at line 303. Tags are at indent 4,
form `name,` (void) or `name: Type,` (payload). Sub-declarations (pub const/fn)
begin at line 973, so tag region = lines 304..972 (1-based, exclusive of decls).
Some payload types span multiple lines but the tag NAME is always on the line
that begins at indent 4 with an identifier followed by `,` or `:`.
"""
import re
import json

PATH = "/Users/gael/projects/ghostty/src/input/Binding.zig"
with open(PATH) as f:
    lines = f.readlines()

# Find union start
start = None
for i, l in enumerate(lines):
    if l.startswith("pub const Action = union(enum) {"):
        start = i + 1
        break
# Find first sub-declaration inside (indent-4 pub/const/fn) -> end of tag region
end = None
for i in range(start, len(lines)):
    if re.match(r'^    (pub )?(const|fn) ', lines[i]):
        end = i
        break

# Tag line: exactly 4 spaces indent, then identifier, then ',' or ':'
tag_re = re.compile(r'^    (?P<name>[A-Za-z_][A-Za-z0-9_]*)\s*(?P<sep>[,:])')

actions = []
for i in range(start, end):
    m = tag_re.match(lines[i])
    if not m:
        continue
    name = m.group("name")
    sep = m.group("sep")
    if sep == ",":
        payload = None
    else:
        # capture payload type text after ':' up to trailing comma on this line
        rest = lines[i].split(":", 1)[1].strip()
        if rest.endswith(","):
            rest = rest[:-1].strip()
        payload = rest if rest else None
    actions.append({"name": name, "payload": payload, "line": i + 1})

print(json.dumps(actions, indent=2))
