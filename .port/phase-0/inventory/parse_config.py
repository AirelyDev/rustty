#!/usr/bin/env python3
"""Parse config fields from ghostty src/config/Config.zig.

Config = @This(); fields are declared at file scope (column 0) in the region
between the `compatibility` StaticStringMap and `pub fn deinit`.
Format: `name: type = default,` or `@"quoted-name": type = default,`.
A field default may span multiple lines. We extract name, type, default text.
"""
import re
import json

PATH = "/Users/gael/projects/ghostty/src/config/Config.zig"

with open(PATH) as f:
    lines = f.readlines()

end = None
for i, l in enumerate(lines):
    if l.startswith("pub fn deinit(self: *Config) void"):
        end = i
        break

comp_start = None
for i, l in enumerate(lines):
    if l.startswith("pub const compatibility"):
        comp_start = i
        break
start = None
for i in range(comp_start, end):
    if lines[i].rstrip() == "});":
        start = i + 1
        break

field_re = re.compile(r'^(@"(?P<q>[^"]+)"|(?P<b>[A-Za-z_][A-Za-z0-9_]*))\s*:\s*(?P<rest>.*)$')


def scan(s):
    """Return (depth_delta, in_string_at_end) for string s."""
    depth = 0
    instr = False
    esc = False
    for ch in s:
        if instr:
            if esc:
                esc = False
            elif ch == '\\':
                esc = True
            elif ch == '"':
                instr = False
            continue
        if ch == '"':
            instr = True
        elif ch in '({[':
            depth += 1
        elif ch in ')}]':
            depth -= 1
    return depth, instr


def split_type_default(decl):
    """Split `type = default,` into (type, default). Find top-level ' = '."""
    decl = decl.rstrip()
    if decl.endswith(','):
        decl = decl[:-1].rstrip()
    depth = 0
    instr = False
    esc = False
    eq = -1
    i = 0
    while i < len(decl):
        ch = decl[i]
        if instr:
            if esc:
                esc = False
            elif ch == '\\':
                esc = True
            elif ch == '"':
                instr = False
            i += 1
            continue
        if ch == '"':
            instr = True
        elif ch in '({[':
            depth += 1
        elif ch in ')}]':
            depth -= 1
        elif ch == '=' and depth == 0:
            # ensure it's an assignment '=' not '==' or '=>' etc.
            prev = decl[i-1] if i > 0 else ' '
            nxt = decl[i+1] if i+1 < len(decl) else ' '
            if prev == ' ' and nxt == ' ':
                eq = i
                break
        i += 1
    if eq == -1:
        return decl.strip(), None
    typ = decl[:eq].strip()
    default = decl[eq+1:].strip()
    return typ, default


fields = []
i = start
while i < end:
    line = lines[i]
    s = line.strip()
    if s == "" or s.startswith("//"):
        i += 1
        continue
    m = field_re.match(line)
    if not m:
        i += 1
        continue
    name = m.group("q") if m.group("q") else m.group("b")
    rest = m.group("rest")
    decl = rest
    j = i
    depth, instr = scan(rest)
    while True:
        stripped = decl.rstrip()
        if depth <= 0 and not instr and stripped.endswith(','):
            break
        j += 1
        if j >= end:
            break
        decl += " " + lines[j].strip()
        d2, instr = scan(lines[j])
        depth += d2
    typ, default = split_type_default(decl)
    fields.append({"name": name, "type": typ, "default": default, "line": i + 1})
    i = j + 1

print(json.dumps(fields, indent=2))
