#!/usr/bin/env python3
"""negative_control.py — prove the D10 tolerance envelope still DISCRIMINATES.

WHY THIS FILE EXISTS. `run.sh` allows ghostty's `ReleaseFast` build (and the host
`cc`'s other optimization levels) to differ from the -O2 reference by max delta 1 on
<= 3 % of bytes -- the bound is set from measurement: 9/512 = 1.76 % and 3/136 =
2.21 % are the largest observed, across two compiler families and two operating
systems. Any tolerance is a hole until you show what it does NOT admit. This script builds the consensus with the host `cc`,
then computes two *wrong* resamplings of the same source — nearest neighbour (the
exact substitution that left all 9 pre-WS11 `face::freetype` tests green, because
they only ever resized at scale 1.0) and bilinear (a plausible near-miss) — and
requires both to be REJECTED by the envelope, with margin.

It is fail-closed in the useful direction: if a wrong resampler ever landed inside
the envelope, this exits non-zero, and D10 goes RED. It also exits non-zero if the
consensus cannot be built at all, so it can never report a silent green.

Assertions, per case and per wrong resampler:
  1. NOT (maxdelta <= 1 and differing_fraction <= 0.03)   -- outside the envelope
  2. differing_fraction > 0.5                              -- margin, ~40x
  3. maxdelta >= 10                                        -- margin, ~14x

Measured at plan-authoring time under zig 0.15.2 and re-measured under zig 0.16.0;
see the table this script prints. A large deviation from those numbers means the
harness is wrong, not that the resampler moved.
"""

import os
import pathlib
import shutil
import subprocess
import sys
import tempfile

DIR = pathlib.Path(__file__).resolve().parent
ROOT = DIR.parent.parent.parent
VENDOR = ROOT / "crates" / "rustty-font-shaper" / "vendor"

ENVELOPE_MAX_DELTA = 1
ENVELOPE_MAX_FRACTION = 0.03
MARGIN_MIN_FRACTION = 0.5
MARGIN_MIN_DELTA = 10

SRC_W, SRC_H = 8, 16
CASES = [(16, 32), (8, 17)]


def ramp() -> list[int]:
    """Byte-identical to `ramp_8x16()` in crates/rustty-font-shaper/src/stb.rs and to
    the built-in ramp in oracle.c. KEEP ALL THREE IN SYNC."""
    return [(x * 31 + y * 17) & 0xFF for y in range(SRC_H) for x in range(SRC_W)]


def nearest(src: list[int], ow: int, oh: int) -> list[int]:
    """The substitution that the pre-WS11 suite could not detect."""
    out = []
    for oy in range(oh):
        sy = oy * SRC_H // oh
        for ox in range(ow):
            sx = ox * SRC_W // ow
            out.append(src[sy * SRC_W + sx])
    return out


def bilinear(src: list[int], ow: int, oh: int) -> list[int]:
    """A plausible near-miss: pixel-centre mapping, clamped edges, no reconstruction
    filter wider than 2 taps. stb's default is Mitchell/Catmull-Rom, which is wider."""

    def at(x: int, y: int) -> int:
        x = min(max(x, 0), SRC_W - 1)
        y = min(max(y, 0), SRC_H - 1)
        return src[y * SRC_W + x]

    out = []
    for oy in range(oh):
        fy = (oy + 0.5) * SRC_H / oh - 0.5
        y0 = int(fy // 1)
        wy = fy - y0
        for ox in range(ow):
            fx = (ox + 0.5) * SRC_W / ow - 0.5
            x0 = int(fx // 1)
            wx = fx - x0
            v = (
                at(x0, y0) * (1 - wx) * (1 - wy)
                + at(x0 + 1, y0) * wx * (1 - wy)
                + at(x0, y0 + 1) * (1 - wx) * wy
                + at(x0 + 1, y0 + 1) * wx * wy
            )
            out.append(min(255, max(0, int(v + 0.5))))
    return out


def compare(a: bytes | list[int], b: bytes | list[int]) -> tuple[int, int, int]:
    assert len(a) == len(b), f"length mismatch {len(a)} vs {len(b)}"
    n = sum(1 for x, y in zip(a, b) if x != y)
    mx = max((abs(x - y) for x, y in zip(a, b)), default=0)
    return n, len(a), mx


def build_consensus(work: pathlib.Path) -> pathlib.Path:
    cc = os.environ.get("CC") or ("/usr/bin/cc" if os.path.exists("/usr/bin/cc") else shutil.which("cc"))
    if not cc:
        print("negative_control.py: no C compiler found", file=sys.stderr)
        sys.exit(3)
    exe = work / "cons"
    cmd = [cc, "-O2", f"-I{VENDOR}", "-o", str(exe), str(DIR / "oracle.c"), str(VENDOR / "stb_impl.c")]
    if sys.platform != "darwin":
        cmd.append("-lm")
    r = subprocess.run(cmd, capture_output=True, text=True)
    if r.returncode != 0:
        print(f"negative_control.py: consensus build failed\n{r.stderr}", file=sys.stderr)
        sys.exit(1)
    return exe


def near_miss(stb: list[int]) -> list[int]:
    """`--self-test` only: a resampler that IS inside the envelope (+1 on 1 % of
    bytes). The script must report RED on it — that is the proof its RED arm is
    reachable, so a future edit cannot quietly turn it into a no-op."""
    out = list(stb)
    step = max(1, len(out) // max(1, int(0.01 * len(out))))
    for i in range(0, len(out), step):
        out[i] = min(255, out[i] + 1)
    return out


def main() -> int:
    self_test = "--self-test" in sys.argv[1:]
    src = ramp()
    failures: list[str] = []
    with tempfile.TemporaryDirectory() as td:
        work = pathlib.Path(td)
        exe = build_consensus(work)
        for ow, oh in CASES:
            r = subprocess.run([str(exe), str(ow), str(oh)], capture_output=True)
            if r.returncode != 0 or len(r.stdout) != ow * oh:
                failures.append(f"{ow}x{oh}: consensus harness failed (rc={r.returncode})")
                continue
            stb = list(r.stdout)
            label = f"{ow}x{oh}"
            candidates = [("nearest", nearest(src, ow, oh)), ("bilinear", bilinear(src, ow, oh))]
            if self_test:
                candidates = [("NEARMISS", near_miss(stb))]
            for name, wrong in candidates:
                n, total, mx = compare(stb, wrong)
                frac = n / total
                mean = sum(abs(x - y) for x, y in zip(stb, wrong)) / total
                print(
                    f"{label:6s} stb-vs-{name:<9s} {n}/{total} differ, "
                    f"maxdelta={mx}, mean={mean:.1f}"
                )
                if mx <= ENVELOPE_MAX_DELTA and frac <= ENVELOPE_MAX_FRACTION:
                    failures.append(
                        f"{label} {name}: INSIDE the +/-{ENVELOPE_MAX_DELTA}-on-"
                        f"{ENVELOPE_MAX_FRACTION:.0%} envelope — the envelope cannot "
                        f"discriminate a wrong resampler"
                    )
                if frac <= MARGIN_MIN_FRACTION:
                    failures.append(
                        f"{label} {name}: only {frac:.1%} of bytes differ, expected "
                        f">{MARGIN_MIN_FRACTION:.0%} margin"
                    )
                if mx < MARGIN_MIN_DELTA:
                    failures.append(
                        f"{label} {name}: maxdelta {mx} < required margin {MARGIN_MIN_DELTA}"
                    )

    if self_test:
        if failures:
            print("\nnegative_control.py --self-test: PASS — the RED arm is reachable:")
            for f in failures:
                print(f"  would-be-RED: {f}")
            return 0
        print(
            "\nnegative_control.py --self-test: FAIL — a +1-on-1% near miss was NOT "
            "flagged, so the detector is vacuous"
        )
        return 1

    if failures:
        print("\nnegative_control.py: RED")
        for f in failures:
            print(f"  {f}")
        return 1
    print(
        f"\nnegative_control.py: GREEN — both wrong resamplers are far outside the "
        f"maxdelta<={ENVELOPE_MAX_DELTA}/frac<={ENVELOPE_MAX_FRACTION:.0%} envelope "
        f"(margin: >{MARGIN_MIN_FRACTION:.0%} of bytes, maxdelta>={MARGIN_MIN_DELTA})"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
