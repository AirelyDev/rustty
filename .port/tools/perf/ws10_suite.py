#!/usr/bin/env python3
"""ws10_suite.py — the WS10 paired same-session perf fence (gate G8b).

WHY THIS EXISTS (read before changing a threshold)
--------------------------------------------------
The repo's only perf gate before WS10 was `check_baseline_regression.py` (G8),
which compares a candidate against the FROZEN `.port/phase-0/baseline/BASELINE.json`.
Two things make that insufficient as the primary fence:

  1. Cross-session variance. The SAME unchanged oracle bench binary measured
     terminal-parser at 256.2 ms (BASELINE.json) / 286.4 ms (PERF_COMPARISON.md) /
     263.8 ms (PERF_LOG.md r5) / 265.5 ms (WS10 probe) across four sessions — an
     11.8% max/min span. A frozen-JSON comparison therefore cannot be tightened
     below ~12% without emitting false REDs, which makes it useless for catching
     the 5-25% regressions this workstream can introduce.

  2. Until WS10 T2, G8's `bench_index()` keyed on `id` alone while BASELINE.json
     carries each id TWICE (ReleaseFast + Debug), so the Debug rows silently
     overwrote the ReleaseFast ones. Fixed, but see the docstring there.

This harness removes session variance instead of budgeting for it: it measures
BOTH binaries alternately WITHIN ONE PROCESS, so the only noise left is the
intra-session spread (measured <=1.05 on all ten large-corpus configs, even at
load 15). That is why the self fence can be +5% and mean something. It is also
why Fence 2 stores a BINARY (`.perf-bins/`) rather than a JSON: a stored number
is a cross-session comparison in disguise.

THE TWO FENCES
--------------
Fence 1 -- ORACLE ("hold the 14/14 wins"):  rustty vs the pinned oracle binary.
Fence 2 -- SELF   ("no self-regression"):   candidate vs a stored entry binary.

ORACLE COMPARABILITY -- non-negotiable
--------------------------------------
Every number this harness produces is comparable to oracle revision `4749c4e93`
(binary sha256 0fb47d26...) and to NO OTHER ORACLE REVISION. `origin/main` needs
Zig 0.16.0 and has gained `src/quirks_memset.zig`, which makes it ~2.8x faster on
+terminal-stream; comparing against it reads as a phantom rustty regression. See
the WS10 plan's PREMISE CORRECTION section 1 and VERIFICATION_DEBT V6.

EXIT CODES
----------
  0  fence green
  1  fence RED (a real regression)
  2  UNUSABLE MEASUREMENT -- load average above IDLE_MAX. Deliberately distinct
     from both 0 and 1: a contended run is not a pass and not a failure, and
     must never become a recorded baseline.
  3  BLOCKED -- measurement substrate absent (oracle binary or corpora missing;
     they are untracked and live only in the shared checkout, not in worktrees).
"""
import argparse
import json
import os
import statistics
import subprocess
import sys
import time

# --------------------------------------------------------------------------
# Measurement substrate (untracked; shared checkout only -- NOT in worktrees)
# --------------------------------------------------------------------------
SHARED = "/Users/gael/projects/rustty"
ORACLE_DEFAULT = os.path.join(
    SHARED, ".port/phase-0/baseline/bench-releasefast/bin/ghostty-bench")
ORACLE_SHA256 = "0fb47d268c7b9a7f9f93ed14834ca221383ba189404b69d15f750ff76abf5cdb"
COR = os.path.join(SHARED, ".port/phase-0/baseline/corpora")
UTF8 = os.path.join(COR, "utf8.bin")
ASCII = os.path.join(COR, "ascii.bin")
OSCP = os.path.join(COR, "osc-parser.bin")
SMALL = os.path.join(COR, "ascii-small.bin")

# (id, corpus, throughput_basis_bytes | 0, argv-tail). Ids MUST match
# BASELINE.json's so `check_baseline_regression.py --metrics` joins on them.
CONFIGS = [
    ("terminal-parser/utf8",          UTF8,  UTF8,  ["+terminal-parser"]),
    ("terminal-stream/ascii",         ASCII, ASCII, ["+terminal-stream"]),
    ("codepoint-width.noop/utf8",     UTF8,  UTF8,  ["+codepoint-width", "--mode=noop"]),
    ("codepoint-width.table/utf8",    UTF8,  UTF8,  ["+codepoint-width", "--mode=table"]),
    ("codepoint-width.simd/utf8",     UTF8,  UTF8,  ["+codepoint-width", "--mode=simd"]),
    ("codepoint-width.wcwidth/utf8",  UTF8,  UTF8,  ["+codepoint-width", "--mode=wcwidth"]),
    ("grapheme-break.noop/utf8",      UTF8,  UTF8,  ["+grapheme-break", "--mode=noop"]),
    ("grapheme-break.table/utf8",     UTF8,  UTF8,  ["+grapheme-break", "--mode=table"]),
    ("is-symbol.table/utf8",          UTF8,  UTF8,  ["+is-symbol", "--mode=table"]),
    ("is-symbol.uucode/utf8",         UTF8,  UTF8,  ["+is-symbol", "--mode=uucode"]),
    ("osc-parser/osc-parser",         OSCP,  OSCP,  ["+osc-parser"]),
    ("screen-clone.noop/small",       SMALL, None,  ["+screen-clone", "--mode=noop"]),
    ("screen-clone.clone/small",      SMALL, None,  ["+screen-clone", "--mode=clone"]),
    ("screen-clone.render/small",     SMALL, None,  ["+screen-clone", "--mode=render"]),
]
ALL_IDS = [c[0] for c in CONFIGS]

# --------------------------------------------------------------------------
# Fence thresholds. Each carries the measured spread that justifies it.
# --------------------------------------------------------------------------

#: Large-corpus configs: >=50 ms of real work, so process-spawn floor is noise.
#: Measured intra-session max/min spread <=1.05 on ALL TEN even at load 15
#: (cw.simd 1.008, cw.wcwidth 1.010, gb.noop 1.014). Fence 1 requires a strict
#: win (0% tolerance) because the thinnest measured margin is grapheme-break.noop
#: at -7.5%, roughly 5x that spread.
STRICT_IDS = {
    "terminal-parser/utf8", "terminal-stream/ascii",
    "codepoint-width.table/utf8", "codepoint-width.simd/utf8",
    "codepoint-width.wcwidth/utf8", "grapheme-break.noop/utf8",
    "grapheme-break.table/utf8", "is-symbol.table/utf8",
    "is-symbol.uucode/utf8", "osc-parser/osc-parser",
}

#: Short/small-corpus configs: 3.7-25 ms, dominated by process spawn + setup
#: (PERF_COMPARISON.md "Process-spawn / setup-floor caveat"). Measured spreads
#: 1.53 / 1.20 / 1.22 / 1.13 -- an order of magnitude worse than the strict set,
#: so Fence 1 allows a 15% band rather than pretending to a strict win.
BANDED_IDS = {
    "codepoint-width.noop/utf8", "screen-clone.noop/small",
    "screen-clone.clone/small", "screen-clone.render/small",
}
ORACLE_BAND = 1.15

#: Fence 2 (self) tolerances. +5% is ~5x the residual intra-session spread once
#: session variance is removed by measuring both binaries in the same process.
SELF_BAND_STRICT = 1.05
SELF_BAND_SHORT = 1.15

#: Load average above which a measurement is declared UNUSABLE (exit 2).
IDLE_MAX = 2.0

#: Deliberate, measured, net-positive pessimizations. A task that trades a
#: slower path for a faster one adds its config here WITH before/after numbers
#: and a justification; the table is printed on every run so there are no silent
#: waivers. Empty at T3 by design.
ACCEPTED_TRADEOFFS: "dict[str, str]" = {}

#: Configs where rustty's bench and the PINNED oracle binary no longer measure
#: the same amount of work, so a rustty-vs-oracle ratio is meaningless. These are
#: excluded from FENCE 1 (oracle) and KEPT in FENCE 2 (self vs entry), where both
#: sides run the identical rustty bench and the basis matches by construction.
#:
#: This is NOT a waiver and NOT a tradeoff. A waiver says "rustty is slower and
#: we accept it"; this says "the two numbers are not a ratio". The distinction
#: matters because a waiver would let a REAL regression hide behind it, whereas
#: an incomparable config is still fully fenced against rustty's own entry
#: binary. Every entry must carry the workload-basis mismatch that causes it and
#: the normalized comparison, so the exclusion can be checked rather than
#: trusted. The set is printed on every run.
INCOMPARABLE: "dict[str, str]" = {
    "screen-clone.render/small":
        "WS10 T12 ported 446f80f4e's bench hunk, which re-scaled stepRender from "
        "1_000 to 50_000 x --loops iterations. The PINNED oracle binary is at "
        "4749c4e93, i.e. still 1_000 (verified: "
        "`git -C ../ghostty show 4749c4e93:src/benchmark/ScreenClone.zig` line "
        "181), so rustty does 50x the oracle's work in this config. Measured "
        "T21: zig 17.69ms (1_000 iters) vs rust 388.32ms (50_000 iters) = "
        "7.77ms normalized, i.e. -56% against the oracle -- a WIN, reported as "
        "+2096% by a raw ratio. Comparability returns when the oracle pin "
        "advances past 446f80f4e (Phase 3).",
}


# --------------------------------------------------------------------------
# Pure verdict logic (no subprocesses -- this is what --self-test exercises)
# --------------------------------------------------------------------------
def evaluate_oracle(zig_med, rust_med):
    """Fence 1. -> (lines, ok). Medians are {id: seconds}, lower is better."""
    lines, ok = [], True
    for bid in ALL_IDS:
        if bid not in zig_med or bid not in rust_med:
            continue
        if bid in INCOMPARABLE:
            z, r = zig_med[bid], rust_med[bid]
            delta = (r - z) / z * 100.0
            lines.append(f"    INCOMPARABLE {bid:26s} zig={z*1000:8.2f}ms "
                         f"rust={r*1000:8.2f}ms ({delta:+.1f}%) -- NOT a ratio; "
                         f"see INCOMPARABLE[]")
            continue
        z, r = zig_med[bid], rust_med[bid]
        limit = z if bid in STRICT_IDS else z * ORACLE_BAND
        arm = "STRICT" if bid in STRICT_IDS else "BANDED"
        delta = (r - z) / z * 100.0
        if r < limit:
            lines.append(f"    PASS {arm:6s} {bid:30s} zig={z*1000:8.2f}ms "
                         f"rust={r*1000:8.2f}ms ({delta:+.1f}%)")
        elif bid in ACCEPTED_TRADEOFFS:
            lines.append(f"    WAIVED       {bid:30s} ({delta:+.1f}%) "
                         f"-- {ACCEPTED_TRADEOFFS[bid]}")
        else:
            ok = False
            lines.append(f"    LOST bench {bid} ({arm}): zig={z*1000:.2f}ms "
                         f"rust={r*1000:.2f}ms ({delta:+.1f}%)")
    return lines, ok


def evaluate_self(entry_med, cand_med):
    """Fence 2. -> (lines, ok). Candidate must not regress vs the entry binary."""
    lines, ok = [], True
    for bid in ALL_IDS:
        if bid not in entry_med or bid not in cand_med:
            continue
        e, c = entry_med[bid], cand_med[bid]
        band = SELF_BAND_STRICT if bid in STRICT_IDS else SELF_BAND_SHORT
        delta = (c - e) / e * 100.0
        if c <= e * band:
            lines.append(f"    PASS         {bid:30s} entry={e*1000:8.2f}ms "
                         f"cand={c*1000:8.2f}ms ({delta:+.1f}%)")
        elif bid in ACCEPTED_TRADEOFFS:
            lines.append(f"    WAIVED       {bid:30s} ({delta:+.1f}%) "
                         f"-- {ACCEPTED_TRADEOFFS[bid]}")
        else:
            ok = False
            lines.append(f"    SELF-REGRESS bench {bid} ({delta:+.1f}%): "
                         f"entry={e*1000:.2f}ms cand={c*1000:.2f}ms "
                         f"(band +{(band-1)*100:.0f}%)")
    return lines, ok


# --------------------------------------------------------------------------
# Measurement
# --------------------------------------------------------------------------
def time_one(argv):
    s = time.monotonic()
    subprocess.run(argv, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    return time.monotonic() - s


def measure(binary, cfg, n):
    """Warmup + n timed reps. -> list of seconds."""
    _id, corpus, _basis, args = cfg
    argv = [binary] + args + ["--data=" + corpus]
    time_one(argv)  # warmup, discarded
    return [time_one(argv) for _ in range(n)]


def run_paired(a_path, b_path, n, label_a, label_b):
    """Measure both binaries alternately, one config at a time.

    Alternates which binary goes FIRST between configs so a monotonic thermal
    or frequency trend across the run cannot bias one side systematically.
    """
    a_med, b_med, reps = {}, {}, {}
    for i, cfg in enumerate(CONFIGS):
        bid = cfg[0]
        print(f"## {bid}", file=sys.stderr, flush=True)
        if i % 2 == 0:
            ra = measure(a_path, cfg, n)
            rb = measure(b_path, cfg, n)
        else:
            rb = measure(b_path, cfg, n)
            ra = measure(a_path, cfg, n)
        a_med[bid], b_med[bid] = statistics.median(ra), statistics.median(rb)
        reps[bid] = {label_a: ra, label_b: rb}
        print(f"   {label_a}={a_med[bid]*1000:.2f}ms  "
              f"{label_b}={b_med[bid]*1000:.2f}ms  "
              f"spread({label_b})={max(rb)/min(rb):.3f}",
              file=sys.stderr, flush=True)
    return a_med, b_med, reps


def rerun_configs(a_path, b_path, n, ids, label_a, label_b):
    """Two-strike rule: re-measure only the failing configs at 2N."""
    a_med, b_med = {}, {}
    for cfg in CONFIGS:
        if cfg[0] not in ids:
            continue
        print(f"## RE-RUN(2N) {cfg[0]}", file=sys.stderr, flush=True)
        ra = measure(a_path, cfg, n)
        rb = measure(b_path, cfg, n)
        a_med[cfg[0]] = statistics.median(ra)
        b_med[cfg[0]] = statistics.median(rb)
    return a_med, b_med


def failing_ids(lines):
    out = set()
    for ln in lines:
        for marker in ("LOST bench ", "SELF-REGRESS bench "):
            if marker in ln:
                out.add(ln.split(marker, 1)[1].split(" ")[0])
    return out


# --------------------------------------------------------------------------
# Metrics emission (BASELINE.json shape, so G8 --metrics consumes it directly)
# --------------------------------------------------------------------------
def emit_metrics(path, rust_med, reps, n):
    benches = []
    for bid, corpus, basis, _args in CONFIGS:
        if bid not in rust_med:
            continue
        med = rust_med[bid]
        rec = {
            "id": bid,
            "optimize": "ReleaseFast",
            "corpus": os.path.basename(corpus),
            "n_reps": n,
            "median_s": med,
        }
        r = reps.get(bid, {}).get("rust")
        if r:
            rec["reps_s"] = r
            rec["min_s"], rec["max_s"] = min(r), max(r)
        if basis:
            nbytes = os.path.getsize(basis)
            rec["corpus_bytes"] = nbytes
            rec["metric"] = "throughput_MiB_per_s"
            rec["throughput_MiB_per_s"] = (nbytes / (1024 * 1024)) / med
        else:
            # Fixed 1000-iteration step loop; the baseline's `metric` string is
            # prose (not a key), so bench_index falls back to median_s. Mirror
            # it EXACTLY or the join silently changes direction.
            rec["corpus_bytes"] = 0
            rec["metric"] = "clones_per_s (fixed 1000-iteration step loop; data-independent)"
            rec["clones_per_s"] = 1000.0 / med
        benches.append(rec)
    with open(path, "w", encoding="utf-8") as fh:
        json.dump({"benchmarks": benches}, fh, indent=2)
    print(f"metrics written: {path} ({len(benches)} benchmarks)")


# --------------------------------------------------------------------------
# Self-test (FAIL arm #2 of three)
# --------------------------------------------------------------------------
def self_test():
    print("=== ws10_suite SELF-TEST ===")
    ok = True
    base = {bid: 0.100 for bid in ALL_IDS}

    # (a) rust uniformly 0.9x zig -> green
    lines, good = evaluate_oracle(base, {k: v * 0.9 for k, v in base.items()})
    print(f"[a] rust = 0.90x zig on all 14 -> ok={good} (expect True)")
    ok &= good

    # (b) one STRICT config loses -> RED, and names it
    r = {k: v * 0.9 for k, v in base.items()}
    r["grapheme-break.noop/utf8"] = base["grapheme-break.noop/utf8"] * 1.10
    lines, good = evaluate_oracle(base, r)
    named = any("LOST bench grapheme-break.noop/utf8" in ln for ln in lines)
    print(f"[b] grapheme-break.noop 1.10x zig -> ok={good} (expect False), "
          f"names it={named} (expect True)")
    ok &= (not good) and named

    # (c) candidate self-regresses 8% on a STRICT config -> RED at +8.0%
    c = dict(base)
    c["codepoint-width.table/utf8"] = base["codepoint-width.table/utf8"] * 1.08
    lines, good = evaluate_self(base, c)
    named = any("SELF-REGRESS bench codepoint-width.table/utf8 (+8.0%)" in ln
                for ln in lines)
    print(f"[c] candidate 1.08x entry -> ok={good} (expect False), "
          f"names it at +8.0%={named} (expect True)")
    ok &= (not good) and named

    # (d) the BANDED arm must NOT fire inside its band, and MUST outside it.
    c = dict(base)
    c["screen-clone.clone/small"] = base["screen-clone.clone/small"] * 1.12
    _l, in_band = evaluate_self(base, c)
    c["screen-clone.clone/small"] = base["screen-clone.clone/small"] * 1.20
    _l, out_band = evaluate_self(base, c)
    print(f"[d] short-config +12% -> ok={in_band} (expect True); "
          f"+20% -> ok={out_band} (expect False)")
    ok &= in_band and (not out_band)

    # (e) an INCOMPARABLE config cannot turn FENCE 1 red no matter how bad the
    #     raw ratio is -- but it MUST still be reported, and it MUST still be
    #     fenced by FENCE 2. Anything less would make the exclusion a hole.
    for bid in INCOMPARABLE:
        if bid not in ALL_IDS:
            print(f"[e] INCOMPARABLE names an unknown config {bid!r}")
            ok = False
            continue
        r = {k: v * 0.9 for k, v in base.items()}
        r[bid] = base[bid] * 50.0
        lines, good = evaluate_oracle(base, r)
        shown = any(f"INCOMPARABLE {bid}" in ln for ln in lines)
        # ... and Fence 2 still catches a self-regression on the same config.
        c = dict(base)
        c[bid] = base[bid] * 1.30
        _l, self_ok = evaluate_self(base, c)
        print(f"[e] {bid}: 50x oracle -> fence1 ok={good} (expect True), "
              f"reported={shown} (expect True); +30% vs entry -> fence2 "
              f"ok={self_ok} (expect False)")
        ok &= good and shown and (not self_ok)

    print("SELF-TEST:", "PASS" if ok else "FAIL")
    return 0 if ok else 1


# --------------------------------------------------------------------------
def substrate_missing(oracle, need_oracle):
    missing = [p for p in (UTF8, ASCII, OSCP, SMALL) if not os.path.exists(p)]
    if need_oracle and not os.path.exists(oracle):
        missing.append(oracle)
    return missing


def main():
    ap = argparse.ArgumentParser(description="WS10 paired perf fence (G8b)")
    ap.add_argument("--rust", help="candidate rustty-bench")
    ap.add_argument("--oracle", default=ORACLE_DEFAULT)
    ap.add_argument("--self", dest="self_bin", help="entry binary for Fence 2")
    ap.add_argument("-n", type=int, default=9)
    ap.add_argument("--emit-metrics")
    ap.add_argument("--json")
    ap.add_argument("--self-test", action="store_true")
    ap.add_argument("--no-oracle", action="store_true",
                    help="run Fence 2 only")
    args = ap.parse_args()

    if args.self_test:
        sys.exit(self_test())
    if not args.rust:
        ap.error("--rust is required")

    need_oracle = not args.no_oracle
    missing = substrate_missing(args.oracle, need_oracle)
    if missing:
        print("BLOCKED: measurement substrate absent (untracked; lives in the "
              "shared checkout, not in worktrees):")
        for m in missing:
            print("   missing:", m)
        sys.exit(3)

    load1 = os.getloadavg()[0]
    provisional = load1 > IDLE_MAX
    print(f"load average (1 min): {load1:.2f}  (IDLE_MAX={IDLE_MAX})")
    if ACCEPTED_TRADEOFFS:
        print("ACCEPTED_TRADEOFFS:")
        for k, v in ACCEPTED_TRADEOFFS.items():
            print(f"   {k}: {v}")
    else:
        print("ACCEPTED_TRADEOFFS: (none)")
    if INCOMPARABLE:
        print("INCOMPARABLE (excluded from FENCE 1 only; still fenced by "
              "FENCE 2):")
        for k, v in INCOMPARABLE.items():
            print(f"  {k}: {v}")
    else:
        print("INCOMPARABLE: (none)")
    print("NOTE: all numbers are comparable to oracle 4749c4e93 "
          f"(sha256 {ORACLE_SHA256[:8]}...) and to NO other oracle revision.")

    result = {"load1": load1, "n": args.n, "provisional": provisional,
              "oracle_sha256_expected": ORACLE_SHA256}
    rc = 0

    if need_oracle:
        print(f"\n=== FENCE 1: ORACLE (rustty vs pinned oracle, N={args.n}) ===")
        zig_med, rust_med, reps = run_paired(
            args.oracle, args.rust, args.n, "zig", "rust")
        lines, ok = evaluate_oracle(zig_med, rust_med)
        bad = failing_ids(lines)
        if bad:
            print(f"  two-strike: re-running {len(bad)} config(s) at N={args.n*2}")
            z2, r2 = rerun_configs(args.oracle, args.rust, args.n * 2, bad,
                                   "zig", "rust")
            zig_med.update(z2)
            rust_med.update(r2)
            lines, ok = evaluate_oracle(zig_med, rust_med)
        print("\n".join(lines))
        wins = sum(1 for ln in lines if "PASS" in ln)
        print(f"  ORACLE FENCE: {'GREEN' if ok else 'RED'}  "
              f"({wins}/{len(ALL_IDS)} configs pass)")
        result["oracle"] = {"zig_median_s": zig_med, "rust_median_s": rust_med,
                            "ok": ok}
        if not ok:
            rc = 1
        if args.emit_metrics:
            emit_metrics(args.emit_metrics, rust_med, reps, args.n)

    if args.self_bin:
        print(f"\n=== FENCE 2: SELF (candidate vs entry binary, N={args.n}) ===")
        entry_med, cand_med, _reps = run_paired(
            args.self_bin, args.rust, args.n, "entry", "cand")
        lines, ok = evaluate_self(entry_med, cand_med)
        bad = failing_ids(lines)
        if bad:
            print(f"  two-strike: re-running {len(bad)} config(s) at N={args.n*2}")
            e2, c2 = rerun_configs(args.self_bin, args.rust, args.n * 2, bad,
                                   "entry", "cand")
            entry_med.update(e2)
            cand_med.update(c2)
            lines, ok = evaluate_self(entry_med, cand_med)
        print("\n".join(lines))
        print(f"  SELF FENCE: {'GREEN' if ok else 'RED'}")
        result["self"] = {"entry_median_s": entry_med,
                          "cand_median_s": cand_med, "ok": ok}
        if not ok:
            rc = 1

    if args.json:
        with open(args.json, "w", encoding="utf-8") as fh:
            json.dump(result, fh, indent=2)
        print(f"json written: {args.json}")

    if provisional:
        print(f"\n!!! PROVISIONAL: 1-minute load average {load1:.2f} exceeds "
              f"IDLE_MAX {IDLE_MAX}. These numbers are NOT authoritative and "
              f"MUST NOT be recorded as a baseline. Exiting 2 (unusable "
              f"measurement), regardless of the fence verdicts above.")
        sys.exit(2)
    sys.exit(rc)


if __name__ == "__main__":
    main()
