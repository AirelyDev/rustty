#!/usr/bin/env python3
"""Parse ESC handler dispatch from ghostty src/terminal/stream.zig.

`fn escDispatch` is located by its header and brace-matched to the function
end (N517: no hard-coded line window). Its top-level `switch (action.final)`
arms sit at 16-space indent.

Usage: parse_esc.py [--path stream.zig] | --self-test [--path stream.zig]
"""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import _zigspan as Z  # noqa: E402
import parse_csi as C  # noqa: E402

PATH = C.PATH
FN_RE = r"^\s*(?:pub )?(?:inline )?fn escDispatch\("
SWITCH_RE = r"^            switch \(action\.final\) \{\s*$"
# Finals the ESC dispatch must carry (charsets, DECSC/DECRC, IND/NEL/HTS/RI,
# SS2/SS3, RIS, LS2/LS3, DECKPAM/DECKPNM, and the backslash final).
REQUIRED = set("B A 0 7 8 D E H M N O V W Z c n o ~ } | = > \\\\".split())


def parse(path):
    arms, _ = Z.dispatch_arms(Z.read_lines(path), FN_RE, SWITCH_RE)
    return arms


def self_test(path):
    ok = True
    arms = parse(path)
    lines = Z.read_lines(path)
    f0 = Z.find_header(lines, FN_RE)
    f1 = Z.match_block(lines, f0)
    stray = [a for a in arms if not (f0 + 1 <= a["line"] <= f1 + 1)]
    print(f"[a] {path}: {len(arms)} ESC arms; outside escDispatch: {len(stray)} (expect 0)")
    ok &= not stray
    # The 5 rows the stale window recorded were CSI arms (n p q r s) read out
    # of csiDispatch; none of those lines may appear in the ESC set.
    c0 = Z.find_header(lines, C.FN_RE)
    c1 = Z.match_block(lines, c0)
    from_csi = [a for a in arms if c0 + 1 <= a["line"] <= c1 + 1]
    print(f"[b] ESC arms located inside csiDispatch: {len(from_csi)} (expect 0)")
    ok &= not from_csi
    got = Z.finals_of(arms)
    missing = sorted(REQUIRED - got)
    print(f"[c] missing required ESC finals: {missing} (expect [])")
    ok &= not missing
    # Negative control (R5) for [a]-[c]: rename the '7' arm (DECSC) in memory;
    # the required-finals check must report it missing.
    mut = [l.replace("                '7' =>", "                'Q' =>", 1) for l in lines]
    m_arms, _ = Z.dispatch_arms(mut, FN_RE, SWITCH_RE)
    caught = "7" in (REQUIRED - Z.finals_of(m_arms))
    print(f"[f] arm '7' dropped -> caught={caught} (expect True)")
    ok &= caught
    # Negative control (R5): a stream.zig with no escDispatch must be refused,
    # and one whose escDispatch header moved must still be found.
    try:
        Z.dispatch_arms([l.replace("fn escDispatch(", "fn escGone(") for l in lines],
                        FN_RE, SWITCH_RE)
        refused = False
    except SystemExit:
        refused = True
    print(f"[d] header removed -> refused={refused} (expect True)")
    ok &= refused
    shifted = ["\n"] * 500 + lines
    s_arms, _ = Z.dispatch_arms(shifted, FN_RE, SWITCH_RE)
    same = [(a["finals"], a["comment"]) for a in s_arms] == [(a["finals"], a["comment"]) for a in arms]
    print(f"[e] +500 lines shift -> same arms={same} (expect True)")
    ok &= same
    print("SELF-TEST:", "PASS" if ok else "FAIL")
    return 0 if ok else 1


if __name__ == "__main__":
    Z.emit(Z.result_of(parse(Z.parse_args(sys.argv[1:], PATH, self_test))))
