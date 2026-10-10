"""Shared helper for parse_csi.py / parse_esc.py (N517).

Locates a Zig function in stream.zig by its header and brace-matches to the
function end, then locates the dispatch `switch (...final)` inside it and
brace-matches that too. The generators used to read hard-coded line windows,
which silently drift when upstream edits stream.zig.
"""
import json
import re
import sys

# A Zig char literal: one char or one backslash escape (`'\\'`, `'\''`, `'\x1b'`).
_CHAR_LIT = r"'(?:\\(?:x[0-9A-Fa-f]{2}|u\{[0-9A-Fa-f]+\}|.)|[^'\\])'"
ARM_RE = re.compile(r"^                ((?:" + _CHAR_LIT + r"(?:, " + _CHAR_LIT + r")*)|else)\s*=>")
_ZIG_ESCAPES = {"n": "\n", "r": "\r", "t": "\t", "\\": "\\", "'": "'", '"': '"'}


def zig_char(lit):
    """The character a Zig char literal denotes: `'\\\\'` -> `\\`, `'\\''` -> `'`,
    `'\\x1b'` -> ESC. The surface stores the decoded final, never the source
    spelling (a `'\\\\'` arm used to land in the JSON as two backslashes)."""
    body = lit[1:-1]
    if not body.startswith("\\"):
        return body
    esc = body[1:]
    if esc.startswith("x"):
        return chr(int(esc[1:], 16))
    if esc.startswith("u{"):
        return chr(int(esc[2:-1], 16))
    if esc in _ZIG_ESCAPES:
        return _ZIG_ESCAPES[esc]
    raise SystemExit(f"unknown Zig escape in char literal {lit!r}")


def _brace_delta(line):
    """Net `{`/`}` count of a Zig line, ignoring comments, "strings",
    'chars' and `\\\\` multiline-string lines."""
    s = line.lstrip()
    if s.startswith("\\\\"):
        return 0
    d = 0
    i = 0
    n = len(line)
    while i < n:
        c = line[i]
        if c == "/" and line[i + 1:i + 2] == "/":
            break
        if c in "\"'":
            q = c
            i += 1
            while i < n and line[i] != q:
                if line[i] == "\\":
                    i += 1
                i += 1
        elif c == "{":
            d += 1
        elif c == "}":
            d -= 1
        i += 1
    return d


def match_block(lines, start):
    """0-based index of the line where the block opened on/after `start`
    closes (inclusive)."""
    depth = 0
    opened = False
    for idx in range(start, len(lines)):
        d = _brace_delta(lines[idx])
        if d > 0:
            opened = True
        depth += d
        if opened and depth <= 0:
            return idx
    raise SystemExit(f"unbalanced braces from line {start + 1}")


def find_header(lines, header_re, lo=0, hi=None):
    hits = [i for i in range(lo, len(lines) if hi is None else hi + 1)
            if re.search(header_re, lines[i])]
    if len(hits) != 1:
        raise SystemExit(f"expected exactly one match for {header_re!r}, "
                         f"found {len(hits)}")
    return hits[0]


def dispatch_arms(lines, fn_header_re, switch_re):
    """Arms of the top-level `switch` inside the function, as the generators'
    JSON rows. Returns (arms, (switch_first_line, switch_last_line)) 1-based."""
    f0 = find_header(lines, fn_header_re)
    f1 = match_block(lines, f0)
    s0 = find_header(lines, switch_re, f0, f1)
    s1 = match_block(lines, s0)
    arms = []
    for idx in range(s0 + 1, s1):
        m = ARM_RE.match(lines[idx])
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
        finals = [zig_char(lit) for lit in re.findall(_CHAR_LIT, label)]
        arms.append({
            "finals": finals if finals else ["else"],
            "comment": " | ".join(comments) if comments else None,
            "line": idx + 1,
        })
    return arms, (s0 + 1, s1 + 1)


def result_of(arms):
    return {
        "count_arms": len([a for a in arms if a["finals"] != ["else"]]),
        "has_else": any(a["finals"] == ["else"] for a in arms),
        "arms": arms,
    }


def finals_of(arms):
    return {f for a in arms for f in a["finals"] if f != "else"}


def read_lines(path):
    with open(path) as f:
        return f.readlines()


def parse_args(argv, default_path, self_test):
    """`[--path FILE]` -> print JSON; `--self-test` -> run self_test()."""
    path = default_path
    args = list(argv)
    if "--path" in args:
        i = args.index("--path")
        if i + 1 >= len(args):
            print("usage: [--path stream.zig] [--self-test]", file=sys.stderr)
            sys.exit(2)
        path = args[i + 1]
    if "--self-test" in args:
        sys.exit(self_test(path))
    return path


def emit(result):
    print(json.dumps(result, indent=2))
