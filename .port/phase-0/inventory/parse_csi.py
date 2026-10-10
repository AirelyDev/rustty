#!/usr/bin/env python3
"""Parse CSI handler dispatch from ghostty src/terminal/stream.zig.

`fn csiDispatch` is located by its header and brace-matched to the function
end (N517: the generator used to read a hard-coded line window, which drifted
and silently dropped arms). Within it, the top-level `switch (input.final)`
arms sit at 16-space indent: `'X' => ...` or `'X', 'Y' => ...` or
`else => ...`. We capture the final byte(s) and any leading `//` comment (the
command name).

P8-W10 T4: each arm also carries `intermediates`, the intermediate-byte
sequences its body distinguishes (`.port/tools/gates/_csi_intermediates.py`,
the same reader G9 runs on the Rust dispatch). A CSI id is intermediates plus
final, so commands sharing a final (DECRQM `CSI $ p` / `CSI ? $ p` and DECSTR
`CSI ! p`) are separate ids.

Usage: parse_csi.py [--path stream.zig] | --self-test [--path stream.zig]
"""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import _zigspan as Z  # noqa: E402

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)),
                                "..", "..", "tools", "gates"))
import _csi_intermediates as CI  # noqa: E402

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
    lines = Z.read_lines(path)
    arms, (_, s1) = Z.dispatch_arms(lines, FN_RE, SWITCH_RE)
    for i, arm in enumerate(arms):
        # The arm runs from its label line to the line before the next arm
        # (that arm's leading comments are dropped by the reader) or to the
        # switch's closing brace.
        end = arms[i + 1]["line"] - 1 if i + 1 < len(arms) else s1 - 1
        body = "".join(lines[arm["line"] - 1:end]).split("=>", 1)[1]
        try:
            arm["intermediates"] = CI.arm_intermediates(body, "zig")
        except ValueError as e:
            raise SystemExit(f"{path}:{arm['line']}: {e}")
    return arms


def ids_of(arms):
    """The CSI ids (intermediates + final) of the parsed arms."""
    return {CI.csi_id(i, f) for a in arms for f in a["finals"] if f != "else"
            for i in a["intermediates"]}


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
    # [d] P8-W10 T4: commands sharing a final are separate ids. The 'p' arm
    # holds DECRQM (`$`, `?$`) and DECSTR (`!`); a copy of the arm without the
    # DECSTR test must lose exactly `!p` (negative control).
    ids = ids_of(arms)
    want = {"!p", "$p", "?$p", "?h", "h", ">q", " q", "$}"}
    print(f"[d] {path}: ids missing {sorted(want - ids)} (expect []); 'p' arm "
          f"intermediates {next(a['intermediates'] for a in arms if 'p' in a['finals'])}")
    ok &= not (want - ids)
    p_arm = next(a for a in arms if "p" in a["finals"])
    p_end = arms[arms.index(p_arm) + 1]["line"] - 1
    body = "".join(lines[p_arm["line"] - 1:p_end]).split("=>", 1)[1]
    cut = body.replace("input.intermediates[0] == '!'", "false")
    got = CI.arm_intermediates(cut, "zig")
    print(f"[e] 'p' arm without the DECSTR test -> {got} (expect ['$', '?$'])")
    ok &= got == ["$", "?$"] and cut != body
    print("SELF-TEST:", "PASS" if ok else "FAIL")
    return 0 if ok else 1


if __name__ == "__main__":
    Z.emit(Z.result_of(parse(Z.parse_args(sys.argv[1:], PATH, self_test))))
