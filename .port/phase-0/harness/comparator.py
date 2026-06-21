#!/usr/bin/env python3
"""comparator.py — Differential VT comparator for the rustty port.

Feeds an IDENTICAL VT byte stream + IDENTICAL grid/format options into:
  * the REFERENCE oracle  (libghostty-vt via bin/vt_driver_ref), and
  * the PORT hook         (port_hook/vt_driver_port.sh; a stub while rustty is
                           empty),
captures each side's CANONICAL Screen/PageList serialization (the formatter
output), and diffs them.

This is the VT-core leg of the harness described in DESIGN.md (config
parse-then-serialize, input-encode, and render perceptual legs are documented
there and stubbed pending the corresponding port APIs).

Divergence metric (explicit, reproducible):
  We report BYTE divergence and LINE divergence. The headline number is
  byte-level: 1 - (2*M / (len(ref)+len(port))) where M is the total number of
  matching bytes found by difflib.SequenceMatcher (a normalized edit-similarity).
  This is symmetric, in [0,1], == 0.0 for identical output and == 1.0 (100%)
  when one side is empty and the other is not (the empty-port case). A perfect
  match is 0% divergence; total mismatch is 100% divergence.

Exit code: 0 if divergence == 0 (byte-identical), 1 otherwise. Use --json for a
machine-readable result on stdout (nothing else on stdout in that mode).
"""

import argparse
import difflib
import json
import os
import subprocess
import sys

HARNESS_DIR = os.path.dirname(os.path.abspath(__file__))
DEFAULT_REF = os.path.join(HARNESS_DIR, "bin", "vt_driver_ref")
DEFAULT_PORT = os.path.join(HARNESS_DIR, "port_hook", "vt_driver_port.sh")


def run_driver(driver, args, input_bytes, timeout):
    """Run a driver, return (exit_code, stdout_bytes, stderr_text, timed_out)."""
    cmd = driver if isinstance(driver, list) else [driver]
    cmd = cmd + args
    try:
        p = subprocess.run(
            cmd,
            input=input_bytes,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            timeout=timeout,
        )
        return p.returncode, p.stdout, p.stderr.decode("utf-8", "replace"), False
    except subprocess.TimeoutExpired:
        return None, b"", "TIMEOUT", True
    except FileNotFoundError as e:
        return None, b"", f"NOT FOUND: {e}", False


def byte_similarity(a: bytes, b: bytes) -> float:
    """Normalized byte-level similarity in [0,1] (1.0 == identical)."""
    if len(a) == 0 and len(b) == 0:
        return 1.0
    sm = difflib.SequenceMatcher(a=a, b=b, autojunk=False)
    return sm.ratio()


def line_divergence(a: bytes, b: bytes):
    """Return (divergence_fraction, diff_text) at the LINE granularity."""
    al = a.decode("utf-8", "replace").splitlines(keepends=True)
    bl = b.decode("utf-8", "replace").splitlines(keepends=True)
    if not al and not bl:
        return 0.0, ""
    sm = difflib.SequenceMatcher(a=al, b=bl, autojunk=False)
    matched = sum(blk.size for blk in sm.get_matching_blocks())
    total = max(len(al), len(bl)) or 1
    div = 1.0 - (matched / total)
    diff = "".join(
        difflib.unified_diff(al, bl, fromfile="reference", tofile="port", n=2)
    )
    return div, diff


def compare(ref_driver, port_driver, drv_args, input_bytes, timeout):
    ref_rc, ref_out, ref_err, ref_to = run_driver(
        ref_driver, drv_args, input_bytes, timeout
    )
    port_rc, port_out, port_err, port_to = run_driver(
        port_driver, drv_args, input_bytes, timeout
    )

    sim = byte_similarity(ref_out, port_out)
    byte_div = 1.0 - sim
    line_div, diff_text = line_divergence(ref_out, port_out)
    identical = (ref_out == port_out)

    return {
        "input_len": len(input_bytes),
        "driver_args": drv_args,
        "reference": {
            "exit_code": ref_rc,
            "out_len": len(ref_out),
            "timed_out": ref_to,
            "stderr": ref_err.strip(),
        },
        "port": {
            "exit_code": port_rc,
            "out_len": len(port_out),
            "timed_out": port_to,
            "stderr": port_err.strip(),
        },
        "byte_similarity": round(sim, 6),
        "byte_divergence": round(byte_div, 6),
        "byte_divergence_pct": round(byte_div * 100.0, 4),
        "line_divergence": round(line_div, 6),
        "line_divergence_pct": round(line_div * 100.0, 4),
        "identical": identical,
        "_diff_text": diff_text,
        "_ref_out": ref_out,
        "_port_out": port_out,
    }


def build_driver_args(ns):
    a = ["--cols", str(ns.cols), "--rows", str(ns.rows),
         "--scrollback", str(ns.scrollback), "--format", ns.format]
    if ns.trim:
        a.append("--trim")
    else:
        a.append("--no-trim")
    if ns.unwrap:
        a.append("--unwrap")
    return a


def main():
    ap = argparse.ArgumentParser(description="rustty differential VT comparator")
    ap.add_argument("input", nargs="?", help="VT input file (else stdin)")
    ap.add_argument("--ref", default=DEFAULT_REF, help="reference driver path")
    ap.add_argument("--port", default=DEFAULT_PORT, help="port driver path")
    ap.add_argument("--cols", type=int, default=80)
    ap.add_argument("--rows", type=int, default=24)
    ap.add_argument("--scrollback", type=int, default=0)
    ap.add_argument("--format", choices=["plain", "vt", "html"], default="plain")
    ap.add_argument("--trim", dest="trim", action="store_true", default=True)
    ap.add_argument("--no-trim", dest="trim", action="store_false")
    ap.add_argument("--unwrap", action="store_true", default=False)
    ap.add_argument("--timeout", type=float, default=30.0)
    ap.add_argument("--json", action="store_true", help="emit JSON result on stdout")
    ap.add_argument("--show-diff", action="store_true", help="print unified diff to stderr")
    ns = ap.parse_args()

    if ns.input:
        with open(ns.input, "rb") as f:
            input_bytes = f.read()
    else:
        input_bytes = sys.stdin.buffer.read()

    drv_args = build_driver_args(ns)
    res = compare(ns.ref, ns.port, drv_args, input_bytes, ns.timeout)

    diff_text = res.pop("_diff_text")
    ref_out = res.pop("_ref_out")
    port_out = res.pop("_port_out")

    if ns.json:
        json.dump(res, sys.stdout, indent=2)
        sys.stdout.write("\n")
    else:
        print(f"input_len           = {res['input_len']}", file=sys.stderr)
        print(f"driver_args         = {' '.join(res['driver_args'])}", file=sys.stderr)
        print(f"ref  exit/out_len   = {res['reference']['exit_code']} / {res['reference']['out_len']}", file=sys.stderr)
        print(f"port exit/out_len   = {res['port']['exit_code']} / {res['port']['out_len']}", file=sys.stderr)
        if res["port"]["stderr"]:
            print(f"port stderr         = {res['port']['stderr']}", file=sys.stderr)
        print(f"byte_divergence     = {res['byte_divergence_pct']:.4f}%", file=sys.stderr)
        print(f"line_divergence     = {res['line_divergence_pct']:.4f}%", file=sys.stderr)
        print(f"identical           = {res['identical']}", file=sys.stderr)

    if ns.show_diff and diff_text:
        sys.stderr.write("\n--- unified diff (reference vs port) ---\n")
        sys.stderr.write(diff_text)
        sys.stderr.write("\n")

    sys.exit(0 if res["identical"] else 1)


if __name__ == "__main__":
    main()
