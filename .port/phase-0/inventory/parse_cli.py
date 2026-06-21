#!/usr/bin/env python3
"""Parse CLI +action enum from ghostty src/cli/ghostty.zig.

`pub const Action = enum {` ... tags until `pub fn detectSpecialCase`.
Maps each action to its impl file (cli/<name with - => _>.zig) per Action.file().
"""
import re
import json

PATH = "/Users/gael/projects/ghostty/src/cli/ghostty.zig"
with open(PATH) as f:
    lines = f.readlines()

start = None
for i, l in enumerate(lines):
    if l.startswith("pub const Action = enum {"):
        start = i + 1
        break
end = None
for i in range(start, len(lines)):
    if re.match(r'^    (pub )?(const|fn) ', lines[i]):
        end = i
        break

tag_re = re.compile(r'^    (@"(?P<q>[^"]+)"|(?P<b>[a-z][a-z0-9_]*)),\s*$')
actions = []
for i in range(start, end):
    m = tag_re.match(lines[i])
    if not m:
        continue
    name = m.group("q") if m.group("q") else m.group("b")
    impl = "cli/" + name.replace("-", "_") + ".zig"
    actions.append({"name": name, "invocation": "+" + name, "impl_file": impl, "line": i + 1})

out = {"count": len(actions), "actions": actions}
print(json.dumps(out, indent=2))
