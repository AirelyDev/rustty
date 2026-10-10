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
     carried each id TWICE (ReleaseFast + Debug), so the Debug rows silently
     overwrote the ReleaseFast ones. Fixed, but see the docstring there. Since
     P7-W23 (V17) BASELINE.json's rows are this oracle at the pin, ReleaseFast
     only, measured on an idle host.

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
The oracle is Ghostty's `ghostty-bench` built ReleaseFast at THE ORACLE PIN
(`.port/tools/gates/ORACLE_PIN`), stored at
`~/rustty-oracle-artifacts/ghostty-bench-<first 9 hex of the pin>` and checked
against ORACLE_SHA256 before every oracle run (a mismatch is BLOCKED, exit 3).
Every number is comparable to that binary and to NO OTHER ORACLE REVISION. The
series recorded before task S-4 (2026-09-23) is anchored on the retired
`4749c4e93` binary (sha256 0fb47d26..., backup
`~/rustty-oracle-artifacts/ghostty-bench-4749c4e93`); numbers from the two are
not a ratio. How the binary is rebuilt: `.port/tools/perf/README.md`.

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
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)),
                                "..", "gates"))
import _gatelib  # noqa: E402  (the one reader of ORACLE_PIN)

#: The oracle pin, read from ORACLE_PIN (never spelled here: G-ORACLE-SH's pin
#: arm forbids a copy of it in any tracked .port/tools file).
ORACLE_PIN = _gatelib.oracle_pin()
ARTIFACTS = os.path.expanduser("~/rustty-oracle-artifacts")
ORACLE_DEFAULT = os.path.join(
    ARTIFACTS, "ghostty-bench-" + (ORACLE_PIN or "MISSING-PIN")[:9])
#: sha256 of the stored oracle binary (rebuilt 2026-10-09 by P8-W10 T0 with
#: Zig 0.16.0 ReleaseFast from an archive of the current pin, recipe in
#: README.md; the previous pin's binary, rebuilt by P8-W9, was 84f5dc9f...,
#: and the S-4 binary before it 46b5adfb...). Its __TEXT/__DATA contents are
#: reproducible (S-4: a second build from a fresh archive and cache matched
#: them); the whole-file hash is not, because the linker's LC_UUID and the
#: debug map record the build directory. So the file is kept, and this pins it.
ORACLE_SHA256 = "37c6317668c020e6be5b62ca2e4d2d45c9afee41ff9e60d2920c4171490fee49"
#: The pin ORACLE_SHA256 was built at, as `pin_digest(pin)` (the first 16 hex
#: of the pin's sha256: a digest, not a copy of the pin, so G-ORACLE-SH's pin
#: arms stay green). When ORACLE_PIN advances this stops matching and every
#: oracle run BLOCKs until the oracle is rebuilt and BOTH constants are updated
#: in the same commit -- so the old binary copied to the new file name cannot
#: pass as the new oracle (review S-4 F3).
ORACLE_SHA256_PIN_DIGEST = "ef1a9f5859c2ce05"
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
#:
#: Empty since task S-4 (2026-09-23). Its one entry, `screen-clone.render/small`
#: (WS10 T12's port of 446f80f4e's 50x stepRender re-scale against an oracle
#: still at 1_000 iterations), ended as its own text said it would: the oracle
#: is now built at the pin, which contains 446f80f4e, so both benches run the
#: same 50_000 iterations (measured: 3.785e9 vs 3.874e9 instructions retired).
#: `codepoint-width.wcwidth/utf8` (N173) is NOT listed: both benches now run the
#: reference's global init and its `ensureLocale`, so both measure libc
#: `wcwidth` under the same locale and the ratio is a ratio again.
INCOMPARABLE: "dict[str, str]" = {}


# --------------------------------------------------------------------------
# Pure verdict logic (no subprocesses -- this is what --self-test exercises)
# --------------------------------------------------------------------------
def evaluate_oracle(zig_med, rust_med, incomparable=None):
    """Fence 1. -> (lines, ok). Medians are {id: seconds}, lower is better.

    `incomparable` defaults to the module's INCOMPARABLE; the self-test passes a
    synthetic set so the exclusion logic is exercised even while it is empty."""
    if incomparable is None:
        incomparable = INCOMPARABLE
    lines, ok = [], True
    for bid in ALL_IDS:
        if bid not in zig_med or bid not in rust_med:
            continue
        if bid in incomparable:
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
def size_metrics(rust_bin, baseline=None, root=None):
    """`binary_sizes_bytes` for the Rustty counterparts BASELINE.json names.

    V16 (P7-W22): each size label of BASELINE.json is an oracle artifact at
    the pin with a `rustty_counterpart` path under the repo root. The bench
    label's counterpart is the candidate actually measured (`--rust`), not
    whatever sits at the default path. A counterpart that is not built is
    LEFT OUT, so G8 --strict reports it NOT-MEASURED rather than passing.
    Sizes are deterministic, so unlike the timings they need no idle host.
    -> ({label: {"bytes", "path"}}, [absent label notes])
    """
    if baseline is None:
        baseline = _gatelib.load_baseline()
    root = root or _gatelib.RUSTTY_ROOT
    out, absent = {}, []
    for label, info in (baseline.get("binary_sizes_bytes") or {}).items():
        if label.startswith("_") or not isinstance(info, dict):
            continue
        rel = info.get("rustty_counterpart")
        if not rel:
            continue
        path = rust_bin if os.path.basename(rel) == "rustty-bench" else os.path.join(root, rel)
        if os.path.isfile(path):
            out[label] = {"bytes": os.path.getsize(path), "path": path}
        else:
            absent.append(f"{label}: {path} not built ({info.get('rustty_build', '?')})")
    return out, absent


def size_builds(baseline=None):
    """The `rustty_build` commands of the live size labels, in label order,
    each once (the .dylib and the gen/bench share none, but a command named
    by two labels is run once). `binary_sizes_dropped` is not read: a dropped
    label is not fenced, so G8b does not build for it. check_perf_fence.sh
    --build runs these before measuring, so a fresh tree measures fresh
    artifacts and a stale one is never measured silently (P7-W22 review).
    """
    if baseline is None:
        baseline = _gatelib.load_baseline()
    out = []
    for label, info in (baseline.get("binary_sizes_bytes") or {}).items():
        if label.startswith("_") or not isinstance(info, dict):
            continue
        cmd = info.get("rustty_build")
        if cmd and cmd not in out:
            out.append(cmd)
    return out


def emit_metrics(path, rust_med, reps, n, rust_bin=None):
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
    doc = {"benchmarks": benches}
    if rust_bin:
        sizes, absent = size_metrics(rust_bin)
        doc["binary_sizes_bytes"] = sizes
        for a in absent:
            print(f"NOTE: size counterpart absent -> NOT-MEASURED in G8 --strict: {a}")
    with open(path, "w", encoding="utf-8") as fh:
        json.dump(doc, fh, indent=2)
    print(f"metrics written: {path} ({len(benches)} benchmarks, "
          f"{len(doc.get('binary_sizes_bytes', {}))} sizes)")


# --------------------------------------------------------------------------
# Self-test (FAIL arm #2 of three)
# --------------------------------------------------------------------------
def self_test():
    print("=== ws10_suite SELF-TEST ===")
    ok = True
    base = {bid: 0.100 for bid in ALL_IDS}
    # Cases run per arm. An arm that runs zero cases is a FAIL: a vacuous arm
    # (arm [e] over an empty INCOMPARABLE, before review S-4 F1) is not a pass.
    cases = {arm: 0 for arm in "abcdefg"}

    # (a) rust uniformly 0.9x zig -> green
    lines, good = evaluate_oracle(base, {k: v * 0.9 for k, v in base.items()})
    print(f"[a] rust = 0.90x zig on all 14 -> ok={good} (expect True)")
    ok &= good
    cases["a"] += 1

    # (b) one STRICT config loses -> RED, and names it
    r = {k: v * 0.9 for k, v in base.items()}
    r["grapheme-break.noop/utf8"] = base["grapheme-break.noop/utf8"] * 1.10
    lines, good = evaluate_oracle(base, r)
    named = any("LOST bench grapheme-break.noop/utf8" in ln for ln in lines)
    print(f"[b] grapheme-break.noop 1.10x zig -> ok={good} (expect False), "
          f"names it={named} (expect True)")
    ok &= (not good) and named
    cases["b"] += 1

    # (c) candidate self-regresses 8% on a STRICT config -> RED at +8.0%
    c = dict(base)
    c["codepoint-width.table/utf8"] = base["codepoint-width.table/utf8"] * 1.08
    lines, good = evaluate_self(base, c)
    named = any("SELF-REGRESS bench codepoint-width.table/utf8 (+8.0%)" in ln
                for ln in lines)
    print(f"[c] candidate 1.08x entry -> ok={good} (expect False), "
          f"names it at +8.0%={named} (expect True)")
    ok &= (not good) and named
    cases["c"] += 1

    # (d) the BANDED arm must NOT fire inside its band, and MUST outside it.
    c = dict(base)
    c["screen-clone.clone/small"] = base["screen-clone.clone/small"] * 1.12
    _l, in_band = evaluate_self(base, c)
    c["screen-clone.clone/small"] = base["screen-clone.clone/small"] * 1.20
    _l, out_band = evaluate_self(base, c)
    print(f"[d] short-config +12% -> ok={in_band} (expect True); "
          f"+20% -> ok={out_band} (expect False)")
    ok &= in_band and (not out_band)
    cases["d"] += 1

    # (e) an INCOMPARABLE config cannot turn FENCE 1 red no matter how bad the
    #     raw ratio is -- but it MUST still be reported, and it MUST still be
    #     fenced by FENCE 2. Anything less would make the exclusion a hole.
    #     Run over the live set PLUS a synthetic entry, so the arm is never
    #     vacuous while INCOMPARABLE is empty.
    for bid in INCOMPARABLE:
        if bid not in ALL_IDS:
            print(f"[e] INCOMPARABLE names an unknown config {bid!r}")
            ok = False
    synthetic = dict(INCOMPARABLE)
    synthetic.setdefault("screen-clone.render/small", "self-test")
    for bid in synthetic:
        if bid not in ALL_IDS:
            print(f"[e] INCOMPARABLE names an unknown config {bid!r}")
            ok = False
            continue
        r = {k: v * 0.9 for k, v in base.items()}
        r[bid] = base[bid] * 50.0
        lines, good = evaluate_oracle(base, r, synthetic)
        shown = any(f"INCOMPARABLE {bid}" in ln for ln in lines)
        # ... and WITHOUT the exclusion the same 50x ratio is a loss.
        _l, unexcluded_ok = evaluate_oracle(base, r, {})
        # ... and Fence 2 still catches a self-regression on the same config.
        c = dict(base)
        c[bid] = base[bid] * 1.30
        _l, self_ok = evaluate_self(base, c)
        print(f"[e] {bid}: 50x oracle -> fence1 ok={good} (expect True), "
              f"reported={shown} (expect True), unexcluded ok={unexcluded_ok} "
              f"(expect False); +30% vs entry -> fence2 ok={self_ok} "
              f"(expect False)")
        ok &= good and shown and (not unexcluded_ok) and (not self_ok)
        cases["e"] += 1

    # (f) the oracle identity check: a binary whose sha256 differs BLOCKs, the
    #     matching one does not, a missing pin BLOCKs, and a pin whose digest is
    #     not the one ORACLE_SHA256 was recorded for BLOCKs (F3).
    import tempfile
    with tempfile.TemporaryDirectory() as td:
        good_bin = os.path.join(td, "oracle-good")
        bad_bin = os.path.join(td, "oracle-bad")
        with open(good_bin, "wb") as fh:
            fh.write(b"pinned oracle stand-in")
        with open(bad_bin, "wb") as fh:
            fh.write(b"some other binary")
        want_sha = sha256_of(good_bin)
        fake_pin = "0123456789abcdef0123456789abcdef01234567"
        want_digest = pin_digest(fake_pin)
        f_cases = [
            ("matching sha and pin", good_bin, fake_pin, False),
            ("sha mismatch", bad_bin, fake_pin, True),
            ("missing pin", good_bin, None, True),
            ("pin advanced (digest mismatch)", good_bin,
             "f" + fake_pin[1:], True),
        ]
        for label, path, pin, expect_block in f_cases:
            msg = oracle_identity_blocker(path, pin, want_sha, want_digest)
            blocked = msg is not None
            print(f"[f] {label} -> blocked={blocked} (expect {expect_block})")
            ok &= blocked == expect_block
            cases["f"] += 1
        # The live constants must agree with the live pin (else every real
        # run BLOCKs): checked here so a pin advance shows up in the self-test.
        live = ORACLE_PIN is not None and pin_digest(ORACLE_PIN) == ORACLE_SHA256_PIN_DIGEST
        print(f"[f] ORACLE_SHA256_PIN_DIGEST matches ORACLE_PIN -> {live} "
              f"(expect True)")
        ok &= live
        cases["f"] += 1

    # (g) size counterparts (V16): a built counterpart is measured, the bench
    #     label measures the --rust candidate, an unbuilt one is LEFT OUT (so
    #     G8 --strict says NOT-MEASURED), and "_" prose keys are skipped.
    with tempfile.TemporaryDirectory() as td:
        os.makedirs(os.path.join(td, "target", "release"))
        with open(os.path.join(td, "target", "release", "lib.dylib"), "wb") as fh:
            fh.write(b"x" * 123)
        cand = os.path.join(td, "cand-bench")
        with open(cand, "wb") as fh:
            fh.write(b"y" * 45)
        fake = {"binary_sizes_bytes": {
            "_rule": "prose",
            "dylib": {"bytes": 1, "rustty_counterpart": "target/release/lib.dylib"},
            "bench": {"bytes": 1, "rustty_counterpart": "target/release-fast/rustty-bench"},
            "gen": {"bytes": 1, "rustty_counterpart": "target/release-fast/rustty-gen"},
        }}
        sizes, absent = size_metrics(cand, fake, td)
        got = {k: v["bytes"] for k, v in sizes.items()}
        g_ok = got == {"dylib": 123, "bench": 45} and len(absent) == 1 and "gen" in absent[0]
        print(f"[g] sizes {got}, absent {len(absent)} -> {g_ok} (expect True)")
        ok &= g_ok
        cases["g"] += 1
        live, _a = size_metrics(cand)
        g_live = set(live) | {a.split(":")[0] for a in _a} == {
            k for k in _gatelib.load_baseline()["binary_sizes_bytes"] if not k.startswith("_")}
        print(f"[g] every live BASELINE size label has a counterpart -> {g_live} "
              f"(expect True)")
        ok &= g_live
        cases["g"] += 1
        # size_builds: deduplicated, label order, "_" prose and the dropped
        # labels ignored; every live label's build is a `cargo build`.
        fake["binary_sizes_bytes"]["dylib"]["rustty_build"] = "cargo build -p a"
        fake["binary_sizes_bytes"]["gen"]["rustty_build"] = "cargo build -p b"
        fake["binary_sizes_bytes"]["bench"]["rustty_build"] = "cargo build -p a"
        fake["binary_sizes_dropped"] = {"x.a": {"rustty_build": "cargo build -p z"}}
        b = size_builds(fake)
        g_b = b == ["cargo build -p a", "cargo build -p b"]
        print(f"[g] size_builds {b} -> {g_b} (expect True)")
        ok &= g_b
        cases["g"] += 1
        live_b = size_builds()
        g_lb = bool(live_b) and all(c.split()[:2] == ["cargo", "build"] for c in live_b)
        print(f"[g] live size_builds are cargo builds ({len(live_b)}) -> {g_lb} "
              f"(expect True)")
        ok &= g_lb
        cases["g"] += 1

    empty = [arm for arm, n in cases.items() if n == 0]
    if empty:
        print(f"FAIL: arm(s) {', '.join(empty)} ran zero cases")
        ok = False
    print(f"cases per arm: {cases}")
    print("SELF-TEST:", "PASS" if ok else "FAIL")
    return 0 if ok else 1


# --------------------------------------------------------------------------
def substrate_missing(oracle, need_oracle):
    missing = [p for p in (UTF8, ASCII, OSCP, SMALL) if not os.path.exists(p)]
    if need_oracle and not os.path.exists(oracle):
        missing.append(oracle)
    return missing


def sha256_of(path):
    import hashlib
    h = hashlib.sha256()
    with open(path, "rb") as fh:
        for chunk in iter(lambda: fh.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def pin_digest(pin):
    """The first 16 hex of sha256(pin): identifies a pin without copying it."""
    import hashlib
    return hashlib.sha256(pin.encode("ascii")).hexdigest()[:16]


def oracle_identity_blocker(path, pin, expected_sha=None,
                            expected_pin_digest=None):
    """None if `path` is the pinned oracle, else the BLOCKED reason.

    Blocks when the pin is missing, when the pin is not the one the expected
    sha256 was recorded for, or when the binary's sha256 differs."""
    if expected_sha is None:
        expected_sha = ORACLE_SHA256
    if expected_pin_digest is None:
        expected_pin_digest = ORACLE_SHA256_PIN_DIGEST
    if pin is None:
        return ("ORACLE_PIN is missing or malformed, so the oracle's identity "
                "cannot be checked")
    if pin_digest(pin) != expected_pin_digest:
        return (f"ORACLE_PIN ({pin[:9]}) is not the pin ORACLE_SHA256 was "
                f"recorded for (digest {pin_digest(pin)}, expected "
                f"{expected_pin_digest}): rebuild the oracle at the pin and "
                f"update ORACLE_SHA256 and ORACLE_SHA256_PIN_DIGEST together")
    got = sha256_of(path)
    if got != expected_sha:
        return (f"the oracle binary {path} is not the pinned oracle (sha256 "
                f"{got}, expected {expected_sha})")
    return None


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
    ap.add_argument("--print-size-builds", action="store_true",
                    help="print the size labels' rustty_build commands, one per line")
    ap.add_argument("--preflight", action="store_true",
                    help="only check the measurement substrate (exit 3 if absent)")
    args = ap.parse_args()

    if args.self_test:
        sys.exit(self_test())
    if args.print_size_builds:
        print("\n".join(size_builds()))
        sys.exit(0)
    if not args.rust and not args.preflight:
        ap.error("--rust is required")

    need_oracle = not args.no_oracle
    missing = substrate_missing(args.oracle, need_oracle)
    if missing:
        print("BLOCKED: measurement substrate absent (untracked; lives in the "
              "shared checkout, not in worktrees):")
        for m in missing:
            print("   missing:", m)
        sys.exit(3)
    if need_oracle:
        why = oracle_identity_blocker(args.oracle, ORACLE_PIN)
        if why is not None:
            print(f"BLOCKED: {why}. Numbers against any other binary are not "
                  f"comparable; see README.md for the rebuild recipe.")
            sys.exit(3)
    if args.preflight:
        print("preflight: measurement substrate present")
        sys.exit(0)

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
    print(f"NOTE: all numbers are comparable to the pinned oracle "
          f"{(ORACLE_PIN or '?')[:9]} (sha256 {ORACLE_SHA256[:8]}...) and to NO "
          f"other oracle revision.")

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
            emit_metrics(args.emit_metrics, rust_med, reps, args.n, args.rust)

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
