#!/usr/bin/env python3
"""
check_baseline_regression.py — GATE G8 (no perf/mem/size/startup regression) [R5]

DoP id:  G8  (DEFINITION_OF_PARITY §1, "Perf / memory / size / startup not worse
              than BASELINE.json")
Reads:   .port/phase-0/baseline/BASELINE.json  (frozen Phase-0 reference metrics)
         a METRICS file (the candidate rustty measurement; --metrics PATH)
Pass:    for every metric in both files, rustty is within tolerance of baseline.

Metric model:
  BASELINE.json carries:
    * benchmarks[]: each {id, metric, throughput_MiB_per_s | <metric-key>, median_s}
        - throughput metrics: HIGHER is better (regression = lower)
        - time metrics (median_s, *_s): LOWER is better (regression = higher)
    * binary_sizes_bytes{}: {label: {bytes, tolerance?}}  (LOWER-or-equal is
        the fence; a label's own `tolerance` fraction applies when present,
        else the default 0% for size per DoP. Keys starting with "_" are
        prose, not labels. P7-W22 / V16: the labels are the oracle artifacts
        at the pin, each with its Rustty counterpart and a tolerance set from
        the measured, itemized gap -- see BASELINE.json `_rule`)
  The METRICS file (produced by the future rustty bench/size run) uses the SAME
  shape: {"benchmarks": [...], "binary_sizes_bytes": {...}, "memory": {...},
          "startup": {...}}. Only metrics present in BOTH files are compared
  (a metric absent from the candidate is reported as NOT-MEASURED, never green).

Tolerances (BASELINE.json has no `tol` field, so these documented defaults apply,
overridable via --tol-time/--tol-rss/--tol-size; DoP: 0% for size, small band
for time/RSS):
    size:    0%   (a larger artifact is an immediate regression)
    time:    +7%  (median wall noise band on the M-series host)
    throughput: -7%
    rss:     +7%
    startup: +10%
A regression is reported as  REGRESS <metric>: baseline=<x> now=<y> (+z%).

MODES:
  default (progress): if --metrics is omitted OR the file is absent, the gate is
    in progress mode: it prints the baseline it WOULD compare against and exits 0
    (no candidate metrics yet => nothing can regress). It NEVER fabricates a green
    pass for a real regression.
  --metrics PATH: compare the candidate file vs baseline; RED on any regression.
  --strict: same comparison; additionally RED if the candidate is MISSING any
    baseline benchmark id or size label (no silent drop of a measured axis).
  --self-test: built-in flip proof.
"""
import argparse
import json
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import _gatelib as G  # noqa: E402

DEFAULTS = {
    "size": 0.0,        # fraction; 0 => no growth allowed
    "time": 0.07,
    "throughput": 0.07,
    "rss": 0.07,
    "startup": 0.10,
}

# benchmark metric keys whose direction is "higher is better"
HIGHER_BETTER = {"throughput_MiB_per_s", "throughput", "ops_per_s", "clones_per_s"}
# keys whose direction is "lower is better"
LOWER_BETTER = {"median_s", "min_s", "mean_s", "ns_per_op", "seconds", "wall_s"}


#: optimize mode the candidate metrics are always measured at, and therefore the
#: only half of BASELINE.json that may be compared against them.
CANDIDATE_OPTIMIZE = "ReleaseFast"

#: Benchmark ids whose WORKLOAD BASIS changed on the rustty side, so the frozen
#: BASELINE.json row (measured from the pinned oracle binary) is no longer the
#: same amount of work and a ratio against it is meaningless.
#:
#: This is NOT a tolerance and NOT a waiver. A tolerance says "a bit slower is
#: fine"; this says "these two numbers are not a ratio at all". The distinction
#: is what keeps it honest, and it costs the gate nothing, because every id
#: listed here is STILL fenced -- by `ws10_suite.py`'s FENCE 2, which compares
#: the candidate against rustty's own entry binary, where both sides run the
#: identical workload by construction. An id may only be listed with the change
#: that caused it, the normalized comparison, and the event that ends it.
#:
#: The set is REPORTED on every run (never silently skipped) and is exercised by
#: --self-test, which asserts that a listed id cannot turn the gate red no matter
#: how large the raw delta is, AND that an unlisted id still can.
BASIS_CHANGED = {
    "screen-clone.render/small":
        "WS10 T12 ported 446f80f4e's bench hunk: stepRender went from 1_000 to "
        "50_000 x --loops iterations. BASELINE.json's row is the PINNED oracle "
        "(4749c4e93) at 1_000. rustty therefore does 50x the work in this "
        "config: 0.0163 s baseline vs 0.385 s candidate is 0.0077 s normalized, "
        "i.e. -53% -- a WIN reported as +2258% by a raw ratio. Re-base when the "
        "oracle pin advances past 446f80f4e (Phase 3).",
}


def bench_index(obj):
    """{(id, optimize): (metric_key, value, direction)} for a benchmarks[] list.

    The key is a TUPLE, and that is load-bearing (fixed WS10 T2, June-2026 bug).
    `.port/phase-0/baseline/BASELINE.json` carries 28 rows: each of the 14
    benchmark ids appears TWICE, once with `"optimize": "ReleaseFast"` and once
    with `"optimize": "Debug"`. This function previously keyed on `id` alone, so
    the Debug rows -- which come second in the array -- silently overwrote the
    ReleaseFast ones, and the gate ended up comparing release-fast candidates
    against Debug oracle numbers. The effect was a gate that could not fail:
    `terminal-stream/ascii` was fenced at 2.073 MiB/s instead of 118.8 (a 57x
    loose band) and `screen-clone.clone/small` at 5.486 s instead of 0.0493 s
    (111x). Keying on (id, optimize) keeps both halves addressable; `run()` then
    compares only the CANDIDATE_OPTIMIZE half.

    Rows with no `optimize` field default to CANDIDATE_OPTIMIZE, so candidate
    metrics files -- which only ever measure the release-fast binary and do not
    carry the field -- still join against the ReleaseFast baseline rows.
    """
    out = {}
    for b in obj.get("benchmarks", []) or []:
        bid = b.get("id")
        if bid is None:
            continue
        key = (bid, b.get("optimize") or CANDIDATE_OPTIMIZE)
        mkey = b.get("metric")
        # pick the value: prefer the named metric key, else median_s
        val = None
        direction = None
        if mkey and mkey in b:
            val = b[mkey]
            direction = "higher" if mkey in HIGHER_BETTER else "lower"
        elif "median_s" in b:
            val = b["median_s"]
            mkey = "median_s"
            direction = "lower"
        if val is None:
            continue
        out[key] = (mkey, float(val), direction)
    return out


def size_index(obj):
    """{label: bytes} from binary_sizes_bytes ("_"-prefixed keys are prose)."""
    out = {}
    for label, info in (obj.get("binary_sizes_bytes", {}) or {}).items():
        if label.startswith("_"):
            continue
        if isinstance(info, dict):
            b = info.get("bytes")
        else:
            b = info
        if isinstance(b, (int, float)):
            out[label] = float(b)
    return out


def size_tolerances(obj):
    """{label: fraction} for the labels that carry their own `tolerance`.

    A per-label tolerance is a documented property of the BASELINE row (set
    from the measured gap, P7-W22), not a waiver: `--tol-size` still
    overrides every label, and a label without one keeps the 0% default.
    """
    out = {}
    for label, info in (obj.get("binary_sizes_bytes", {}) or {}).items():
        if label.startswith("_") or not isinstance(info, dict):
            continue
        t = info.get("tolerance")
        if isinstance(t, (int, float)) and not isinstance(t, bool):
            out[label] = float(t)
    return out


def pct(base, now):
    if base == 0:
        return 0.0 if now == 0 else float("inf")
    return (now - base) / base


def run(args):
    G.banner("G8", "no perf/memory/size/startup regression vs BASELINE.json")
    try:
        baseline = G.load_baseline()
    except G.Blocked as b:
        G.emit(G.BLOCKED_STATE, str(b))
        return 3

    tol = dict(DEFAULTS)
    if args.tol_time is not None:
        tol["time"] = args.tol_time
        tol["throughput"] = args.tol_time
    if args.tol_rss is not None:
        tol["rss"] = args.tol_rss
    if args.tol_size is not None:
        tol["size"] = args.tol_size

    base_bench = bench_index(baseline)
    base_size = size_index(baseline)
    size_tol = {} if args.tol_size is not None else size_tolerances(baseline)

    if not args.metrics or not os.path.exists(args.metrics):
        # progress mode: nothing to compare
        if args.metrics and not os.path.exists(args.metrics):
            print(f"NOTE: metrics file '{args.metrics}' not found -> progress mode")
        else:
            print("NOTE: no --metrics candidate supplied -> progress mode")
        eligible = [(k, v) for k, v in base_bench.items() if k[1] == CANDIDATE_OPTIMIZE]
        print(f"baseline benchmarks: {len(base_bench)} rows, "
              f"{len(eligible)} comparable ({CANDIDATE_OPTIMIZE}); "
              f"baseline size labels: {len(base_size)}")
        for (bid, opt), (mkey, val, direction) in eligible[:8]:
            print(f"  baseline bench {bid} [{opt}]: {mkey}={val} ({direction}-is-better)")
        for label, b in list(base_size.items())[:8]:
            print(f"  baseline size  {label}: {int(b)} bytes")
        G.emit(G.GREEN, "progress mode: no candidate metrics; nothing can regress")
        return 0

    try:
        with open(args.metrics, encoding="utf-8") as fh:
            cand = json.load(fh)
    except Exception as e:  # noqa: BLE001
        G.emit(G.BLOCKED_STATE, f"metrics file unreadable: {e}")
        return 3

    cand_bench = bench_index(cand)
    cand_size = size_index(cand)

    regressions = []
    missing = []
    basis_changed = []
    compared = 0

    # benchmarks. Only the CANDIDATE_OPTIMIZE half of BASELINE.json is eligible:
    # the candidate is always a release-fast build, so comparing it against the
    # Debug oracle rows is meaningless (and, before WS10 T2, was silently what
    # this gate did). The Debug rows are therefore neither compared nor counted
    # as missing-from-candidate under --strict.
    for key, (mkey, bval, direction) in base_bench.items():
        bid, optimize = key
        if optimize != CANDIDATE_OPTIMIZE:
            continue
        if key not in cand_bench:
            missing.append(("bench", bid))
            continue
        _ckey, cval, _cd = cand_bench[key]
        if bid in BASIS_CHANGED:
            basis_changed.append((bid, bval, cval))
            continue
        compared += 1
        t = tol["throughput"] if direction == "higher" else tol["time"]
        if direction == "higher":
            # regression if cval < bval*(1-t)
            if cval < bval * (1 - t):
                regressions.append((f"bench {bid} ({mkey})", bval, cval, pct(bval, cval)))
        else:
            # lower is better: regression if cval > bval*(1+t)
            if cval > bval * (1 + t):
                regressions.append((f"bench {bid} ({mkey})", bval, cval, pct(bval, cval)))

    # sizes (the label's own tolerance, else the 0% default)
    for label, bbytes in base_size.items():
        if label not in cand_size:
            missing.append(("size", label))
            continue
        cbytes = cand_size[label]
        compared += 1
        t = size_tol.get(label, tol["size"])
        verdict = "ok"
        if cbytes > bbytes * (1 + t):
            regressions.append((f"size {label} (tolerance {t*100:+.0f}%)",
                                bbytes, cbytes, pct(bbytes, cbytes)))
            verdict = "REGRESS"
        print(f"  size {label}: oracle={int(bbytes)} rustty={int(cbytes)} "
              f"({pct(bbytes, cbytes)*100:+.2f}%; tolerance {t*100:+.0f}%) {verdict}")

    # optional memory / startup top-level scalar maps {name: value} (lower better)
    for axis, key in (("rss", "memory"), ("startup", "startup")):
        bmap = baseline.get(key)
        cmap = cand.get(key)
        if isinstance(bmap, dict) and isinstance(cmap, dict):
            for name, bv in bmap.items():
                if not isinstance(bv, (int, float)):
                    continue
                if name not in cmap or not isinstance(cmap[name], (int, float)):
                    missing.append((axis, name))
                    continue
                compared += 1
                cv = cmap[name]
                if cv > bv * (1 + tol[axis]):
                    regressions.append((f"{axis} {name}", bv, cv, pct(bv, cv)))

    print(f"compared {compared} metrics; regressions {len(regressions)}; "
          f"missing-from-candidate {len(missing)}; "
          f"basis-changed {len(basis_changed)}")
    for (bid, bval, cval) in basis_changed:
        print(f"    BASIS-CHANGED {bid}: baseline={bval} now={cval} "
              f"({pct(bval, cval)*100:+.2f}%) -- NOT a ratio, excluded from the "
              f"verdict; still fenced by ws10_suite.py FENCE 2")
        print(f"      why: {BASIS_CHANGED[bid]}")
    for (name, bval, cval, p) in regressions:
        print(f"    REGRESS {name}: baseline={bval} now={cval} ({p*100:+.2f}%)")
    if args.strict:
        for (axis, name) in missing:
            print(f"    NOT-MEASURED {axis}: {name} (present in baseline, absent in candidate)")

    if regressions:
        G.emit(G.RED, "metric(s) regressed beyond tolerance; see REGRESS above")
        return 1
    if args.strict and missing:
        G.emit(G.RED, "candidate is missing baseline metrics (strict); see NOT-MEASURED")
        return 1
    G.emit(G.GREEN, f"no regression beyond tolerance across {compared} metrics")
    return 0


def self_test():
    import tempfile

    print("=== G8 SELF-TEST ===")
    tmp = tempfile.mkdtemp(prefix="g8_selftest_")
    base_path = os.path.join(tmp, "BASELINE.json")
    baseline = {
        "benchmarks": [
            {"id": "terminal-parser/utf8", "metric": "throughput_MiB_per_s",
             "throughput_MiB_per_s": 250.0, "median_s": 0.25},
        ],
        "binary_sizes_bytes": {"libvt.a": {"bytes": 1000000}},
    }
    with open(base_path, "w") as fh:
        json.dump(baseline, fh)

    here = os.path.dirname(os.path.abspath(__file__))
    # We invoke the *installed* script but point it at a temp baseline via a
    # small wrapper: copy script+lib into a synthetic phase-0 layout.
    import shutil
    gates = os.path.join(tmp, ".port", "tools", "gates")
    bdir = os.path.join(tmp, ".port", "phase-0", "baseline")
    os.makedirs(gates)
    os.makedirs(bdir)
    shutil.copy(os.path.join(here, "_gatelib.py"), gates)
    shutil.copy(os.path.join(here, "check_baseline_regression.py"), gates)
    shutil.copy(base_path, os.path.join(bdir, "BASELINE.json"))
    script = os.path.join(gates, "check_baseline_regression.py")

    def write_metrics(throughput, size_bytes):
        p = os.path.join(tmp, "metrics.json")
        with open(p, "w") as fh:
            json.dump({
                "benchmarks": [
                    {"id": "terminal-parser/utf8", "metric": "throughput_MiB_per_s",
                     "throughput_MiB_per_s": throughput, "median_s": 0.25},
                ],
                "binary_sizes_bytes": {"libvt.a": {"bytes": size_bytes}},
            }, fh)
        return p

    def invoke(*extra):
        return subprocess.run([sys.executable, script, *extra],
                              capture_output=True, text=True)

    import subprocess
    ok = True

    # (a) no metrics => progress GREEN
    r = invoke()
    print(f"[a] no metrics -> exit {r.returncode} (expect 0)")
    ok &= (r.returncode == 0)

    # (b) within tolerance (same throughput, same size) => GREEN
    m = write_metrics(250.0, 1000000)
    r = invoke("--metrics", m)
    print(f"[b] equal metrics -> exit {r.returncode} (expect 0)")
    ok &= (r.returncode == 0)

    # (c) throughput collapses 50% => RED
    m = write_metrics(125.0, 1000000)
    r = invoke("--metrics", m)
    print(f"[c] throughput -50% -> exit {r.returncode} (expect non-zero), REGRESS: {'REGRESS' in r.stdout}")
    ok &= (r.returncode != 0 and "REGRESS" in r.stdout)

    # (d) size grows 1 byte (0% tol) => RED
    m = write_metrics(250.0, 1000001)
    r = invoke("--metrics", m)
    print(f"[d] size +1B (0% tol) -> exit {r.returncode} (expect non-zero)")
    ok &= (r.returncode != 0 and "REGRESS" in r.stdout)

    # (e) throughput within 7% band (245) => GREEN
    m = write_metrics(245.0, 1000000)
    r = invoke("--metrics", m)
    print(f"[e] throughput -2% (within band) -> exit {r.returncode} (expect 0)")
    ok &= (r.returncode == 0)

    # (e2) BASIS_CHANGED must be an EXCLUSION, not a hole (WS10 T21). A listed
    #      id cannot turn the gate red however large the delta -- and an unlisted
    #      id with the SAME delta still must. Without the second half, the
    #      mechanism would be indistinguishable from a blanket waiver.
    def write_basis_metrics(bid, throughput):
        p = os.path.join(tmp, "metrics_basis.json")
        with open(p, "w") as fh:
            json.dump({
                "benchmarks": [
                    {"id": bid, "metric": "throughput_MiB_per_s",
                     "throughput_MiB_per_s": throughput, "median_s": 0.25},
                ],
                "binary_sizes_bytes": {"libvt.a": {"bytes": 1000000}},
            }, fh)
        return p

    if BASIS_CHANGED:
        listed = sorted(BASIS_CHANGED)[0]
        # Rewrite the synthetic baseline so the listed id exists in it.
        baseline["benchmarks"] = [
            {"id": listed, "optimize": "ReleaseFast",
             "metric": "throughput_MiB_per_s", "throughput_MiB_per_s": 250.0},
            {"id": "terminal-parser/utf8", "optimize": "ReleaseFast",
             "metric": "throughput_MiB_per_s", "throughput_MiB_per_s": 250.0},
        ]
        with open(os.path.join(bdir, "BASELINE.json"), "w") as fh:
            json.dump(baseline, fh)

        m = write_basis_metrics(listed, 5.0)          # 50x worse
        r = invoke("--metrics", m)
        shown = "BASIS-CHANGED" in r.stdout
        print(f"[e2] listed id 50x worse -> exit {r.returncode} (expect 0), "
              f"reported={shown} (expect True)")
        ok &= (r.returncode == 0 and shown)

        m = write_basis_metrics("terminal-parser/utf8", 5.0)  # same delta, unlisted
        r = invoke("--metrics", m)
        print(f"[e2] UNLISTED id 50x worse -> exit {r.returncode} "
              f"(expect non-zero)")
        ok &= (r.returncode != 0 and "REGRESS" in r.stdout)
    else:
        print("[e2] BASIS_CHANGED empty -> nothing to exercise")

    # (g) PER-LABEL SIZE TOLERANCE (P7-W22). A label's own `tolerance` admits
    #     growth up to it and no further; a label WITHOUT one still REDs on a
    #     single byte; `--tol-size` overrides it; a "_" prose key is not a
    #     label (so never NOT-MEASURED under --strict).
    baseline["benchmarks"] = [
        {"id": "terminal-parser/utf8", "metric": "throughput_MiB_per_s",
         "throughput_MiB_per_s": 250.0, "median_s": 0.25},
    ]
    baseline["binary_sizes_bytes"] = {
        "_rule": "prose",
        "tol.dylib": {"bytes": 1000000, "tolerance": 0.10},
        "zero.a": {"bytes": 1000000},
    }
    with open(os.path.join(bdir, "BASELINE.json"), "w") as fh:
        json.dump(baseline, fh)

    def write_sizes(tol_bytes, zero_bytes):
        p = os.path.join(tmp, "metrics_sizes.json")
        with open(p, "w") as fh:
            json.dump({
                "benchmarks": [
                    {"id": "terminal-parser/utf8", "metric": "throughput_MiB_per_s",
                     "throughput_MiB_per_s": 250.0, "median_s": 0.25},
                ],
                "binary_sizes_bytes": {"tol.dylib": {"bytes": tol_bytes},
                                       "zero.a": {"bytes": zero_bytes}},
            }, fh)
        return p

    r = invoke("--strict", "--metrics", write_sizes(1100000, 1000000))
    print(f"[g] +10% on a 10% label, strict -> exit {r.returncode} (expect 0)")
    ok &= (r.returncode == 0 and "NOT-MEASURED" not in r.stdout)
    r = invoke("--metrics", write_sizes(1100001, 1000000))
    print(f"[g] +10.0001% on a 10% label -> exit {r.returncode} (expect non-zero)")
    ok &= (r.returncode != 0 and "REGRESS size tol.dylib" in r.stdout)
    r = invoke("--metrics", write_sizes(1000000, 1000001))
    print(f"[g] +1 B on a label without a tolerance -> exit {r.returncode} "
          f"(expect non-zero)")
    ok &= (r.returncode != 0 and "REGRESS size zero.a" in r.stdout)
    r = invoke("--tol-size", "0", "--metrics", write_sizes(1000001, 1000000))
    print(f"[g] --tol-size 0 overrides the label's 10% -> exit {r.returncode} "
          f"(expect non-zero)")
    ok &= (r.returncode != 0 and "REGRESS size tol.dylib" in r.stdout)

    shutil.rmtree(tmp, ignore_errors=True)

    # (f) REAL-BASELINE ARM (WS10 T2). BASELINE.json carries each of the 14 ids
    # TWICE -- once "ReleaseFast", once "Debug". Before the (id, optimize) fix,
    # bench_index() keyed on `id` alone and the Debug rows (which come second in
    # the array) silently overwrote the ReleaseFast ones, so the gate compared
    # release-fast candidates against Debug oracle numbers. This arm pins the
    # ReleaseFast row and would have caught that bug.
    try:
        real = G.load_baseline()
        ridx = bench_index(real)
        ts = ridx[("terminal-stream/ascii", "ReleaseFast")]
        sc = ridx[("screen-clone.clone/small", "ReleaseFast")]
        ok_f = (abs(ts[1] - 118.8) / 118.8 < 0.01) and (abs(sc[1] - 0.0493) / 0.0493 < 0.01)
        detail = f"terminal-stream={ts[1]:.4g} screen-clone.clone={sc[1]:.4g}"
    except Exception as e:  # noqa: BLE001
        ok_f = False
        detail = f"{type(e).__name__}: {e}"
    print(f"[f] ReleaseFast row preferred -> {ok_f} (expect True) [{detail}]")
    ok &= ok_f

    print("SELF-TEST:", "PASS" if ok else "FAIL")
    return 0 if ok else 1


def main():
    ap = argparse.ArgumentParser(description="G8 — no regression vs BASELINE.json")
    ap.add_argument("--baseline", help="(ignored; canonical phase-0 baseline path)")
    ap.add_argument("--metrics", help="candidate rustty metrics JSON (same shape)")
    ap.add_argument("--measure", help="(reserved; live measurement not run here)")
    ap.add_argument("--tol-time", type=float, default=None)
    ap.add_argument("--tol-rss", type=float, default=None)
    ap.add_argument("--tol-size", type=float, default=None)
    ap.add_argument("--strict", action="store_true")
    ap.add_argument("--self-test", action="store_true")
    args = ap.parse_args()
    if args.self_test:
        sys.exit(self_test())
    sys.exit(run(args))


if __name__ == "__main__":
    main()
