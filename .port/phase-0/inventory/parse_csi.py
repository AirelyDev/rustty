#!/usr/bin/env python3
"""Parse CSI handler dispatch from ghostty src/terminal/stream.zig.

`fn csiDispatch` is located by its header and brace-matched to the function
end (N517: the generator used to read a hard-coded line window, which drifted
and silently dropped arms). Within it, the top-level `switch (input.final)`
arms sit at 16-space indent: `'X' => ...` or `'X', 'Y' => ...` or
`else => ...`. We capture the final byte(s) and any leading `//` comment (the
command name).

Usage: parse_csi.py [--path stream.zig] | --self-test [--path stream.zig]
"""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import _zigspan as Z  # noqa: E402

PATH = "/Users/gael/projects/ghostty/src/terminal/stream.zig"
FN_RE = r"^\s*(?:pub )?(?:inline )?fn csiDispatch\("
SWITCH_RE = r"^            switch \(input\.final\) \{\s*$"
# Finals the CSI dispatch must carry (N517 regression set). '}' is the decsasd
# arm whose body contains a brace char literal (exercises the brace matcher).
REQUIRED = set("A B C D E F G H I J K L M P S T W X Z a b c d e f g h j k l m n p q r s t u @ ` }".split())
FIXTURE = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..",
                       "tools", "gates", "fixtures", "parity-surface",
                       "stream_no_csi_m.zig")


def parse(path):
    arms, _ = Z.dispatch_arms(Z.read_lines(path), FN_RE, SWITCH_RE)
    return arms


def self_test(path):
    ok = True
    arms = parse(path)
    got = Z.finals_of(arms)
    missing = sorted(REQUIRED - got)
    print(f"[a] {path}: {len(got)} CSI finals, missing required: {missing} (expect [])")
    ok &= not missing
    # No arm may come from outside csiDispatch's switch (e.g. `;` / SGR / OSC).
    lines = Z.read_lines(path)
    f0 = Z.find_header(lines, FN_RE)
    f1 = Z.match_block(lines, f0)
    stray = [a for a in arms if not (f0 + 1 <= a["line"] <= f1 + 1)]
    print(f"[b] arms outside csiDispatch: {len(stray)} (expect 0)")
    ok &= not stray
    # Negative control (R5): the fixture lacks 'm' in csiDispatch; the check
    # must notice.
    fx = Z.finals_of(parse(FIXTURE))
    caught = "m" in (REQUIRED - fx)
    print(f"[c] fixture without 'm': missing={sorted(REQUIRED - fx)} caught={caught} (expect True)")
    ok &= caught
    print("SELF-TEST:", "PASS" if ok else "FAIL")
    return 0 if ok else 1


if __name__ == "__main__":
    Z.emit(Z.result_of(parse(Z.parse_args(sys.argv[1:], PATH, self_test))))
