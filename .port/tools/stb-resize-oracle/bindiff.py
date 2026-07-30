#!/usr/bin/env python3
"""bindiff.py A B — report how far apart two raw byte streams are.

Prints exactly one line:  ``<n>/<total> differ, maxdelta=<d>``

WS11 uses this as the single measurement primitive so that every comparison in the
D10 gate is reported on the same scale — which is what makes the tolerated envelope
(max delta 1 on <= 2 % of bytes, cross-toolchain) visibly disjoint from a wrong
resampler (nearest neighbour / bilinear: 70-98 % of bytes at max delta 14-192).
Length mismatch is a hard error, never a silent truncating compare.
"""

import sys


def main() -> int:
    if len(sys.argv) != 3:
        print("usage: bindiff.py A B", file=sys.stderr)
        return 2
    a = open(sys.argv[1], "rb").read()
    b = open(sys.argv[2], "rb").read()
    if len(a) != len(b):
        print(
            f"bindiff.py: length mismatch {sys.argv[1]}={len(a)} {sys.argv[2]}={len(b)}",
            file=sys.stderr,
        )
        return 1
    n = sum(1 for x, y in zip(a, b) if x != y)
    mx = max((abs(x - y) for x, y in zip(a, b)), default=0)
    print(f"{n}/{len(a)} differ, maxdelta={mx}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
