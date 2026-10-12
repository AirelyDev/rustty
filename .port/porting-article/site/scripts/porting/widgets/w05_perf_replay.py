"""Data builder for the "Perf replay" widget (slug perf-replay).

Ports the data step of the mockup build (mockup-build/w05/build.py) onto the persisted
research files:

- research/data/mock-05-perf-replay-data.json  what build.py measured from the raw sweeps
  and git: the 14 benchmarks, the 14 recorded sweeps of 2026-07-15/16 (median, min and max
  of 7 timed runs per benchmark, Rustty and same-session Zig, with each sweep's results
  file), the 37 commits of the window (time, subject, diffstat, files), the per-benchmark
  command arguments and the 33-config matrix (before and after the compile-time fixes)
- research/data/perf-*.csv  the LTO stage chain and profile experiment, the waterfalls,
  the falsified optimisations, the gap classes and compile-time-flattening steps, the
  advanced-pin re-measure, the APC/hyperlink fix, memory and binary sizes
  (plus r2-libvt-sizes.csv for the dylib at HEAD)

The sweep statistics (wins, ties and losses with a ±1.5% tie band, suite sums, geomean,
the benchmarks that moved outside their spread) are recomputed here exactly as build.py
does, and the official stages are checked against perf-scoreboard-by-stage.csv.

What changes on the way:

- binary names such as `rustty-bench-r5-SHIP-RF` are round labels: every Rustty build is
  shown as `rustty-bench` (the crate it was built from); results-file names are kept
- the corpus path `.port/phase-0/baseline/corpora/…` becomes `…/corpora/…` (the prefix is
  an internal planning directory)
- commit subjects, step texts and table text go through the shared jargon stripper (code
  punctuation `::` and `()` is protected), after SUBSTITUTIONS spell out round labels
  ("r5" → "round 5") and the mockup's own rewrites ("by WS10" → "re-measured later",
  "(NONE-SAFE)" dropped); a hyphen used as a minus sign becomes U+2212
- each rewrite goes to jargon-review.txt

Current pin slot: the benchmark suite was re-run at the current reference commit 246f70287
(research/data/perf-ratio-pin-246f70287.csv, PIN_FILE, the same harness as the 12752b2ac
re-measure: N=9 medians, same-session Zig ReleaseFast, idle Mac). The file has the columns
PIN_COLUMNS, one row per benchmark; the slot becomes `"measured"` with those rows, its date
and Rustty head taken from the file. The run recorded medians only, so the min and max
columns are empty: an empty value becomes null and the widget draws no range for it. The
slot must hold exactly the 14 suite benchmarks, each with positive medians and provenance
MEASURED (ValueError otherwise). PIN_NOTE carries the one record sentence the CSV cannot.
"""
import csv
import json
import math
import re
from pathlib import Path

import jargon

from .snapshot import dated

SLUG = "perf-replay"

# PIN_REV is the current Ghostty reference commit; PIN_HEAD is Rustty's head at that pin.
PIN_REV = "246f70287"
PIN_HEAD = "11b4c74d0"

# PIN_NOTE is the run's record sentence about its closest benchmark (.port/phase-8/final/perf/SUMMARY.txt).
PIN_NOTE = ("The closest benchmark, terminal-stream, lost its first nine-run round (Ghostty 46.81 ms, Rustty 47.40 ms) and won "
            "the built-in eighteen-run re-run; the slot shows the accepted re-run.")

# PIN_FILE is the measured run at the current pin (the slot stays pending while it is absent).
PIN_FILE = f"perf-ratio-pin-{PIN_REV}.csv"

# PIN_COLUMNS are the columns PIN_FILE must have.
PIN_COLUMNS = ["bench", "ghostty_ms", "ghostty_min_ms", "ghostty_max_ms", "rustty_ms",
               "rustty_min_ms", "rustty_max_ms", "date", "rustty_head", "provenance"]

# KEYS renames build.py's stage keys (base, preA, …, r5) to plain words.
KEYS = {"base": "base", "preA": "session", "preB": "profile", "r1": "round1", "r2a": "render",
        "r2b": "arrayvec", "r2c": "clone", "r2d": "oscwin", "r2": "round2", "r3a": "clone3",
        "r3b": "render3", "r3": "round3", "r3s": "control", "r5": "round5"}

# OFFICIAL are the stages that perf-scoreboard-by-stage.csv lists, in its row order.
OFFICIAL = ["base", "session", "profile", "round1", "round2", "round3", "round5"]

# SUBSTITUTIONS spell out round labels and apply the mockup's plain-word rewrites, before the
# shared jargon stripper.
SUBSTITUTIONS = [
    (r"\bTIER-0 ", ""),
    (r" — r\d-[a-z0-9-]+:", ":"),
    (r"SHIP-RF stashed", "shipped binary stashed"),
    (r"\br(\d)-r(\d) ships", r"rounds \1–\2 ships"),
    (r"\br(\d) final gate record", r"round \1 final gate record"),
    (r"^r(\d)(?=[ :])", r"round \1"),
    (r" by WS10", " re-measured later"),
    (r"\b(at|until) r(\d)\b", r"\1 round \2"),
    (r"\s*\(NONE-SAFE\)", ""),
    (r"lane C NONE-SAFE", "no safe win found"),
    (r"(?<![\w.])-(?=\d)", "−"),
]

# PROTECT maps code punctuation the stripper's tidy would change to placeholders.
PROTECT = [("::", ""), ("()", "")]

# CHAIN_TEXT rewrites the LTO stage chain's binary and verdict labels.
CHAIN_TEXT = {"frozen_C (pre-fix)": "pre-fix code", "FINAL_C (merge incl. render-soa)": "merge incl. render-soa",
              "ACCEPT": "accepted", "REVERTED 408ddaa": "reverted (408ddaa)"}

# LTO_LABELS are the short legend labels of the five build profiles, in CSV order (mockup).
LTO_LABELS = ["release (checks on)", "overflow checks off (shipped)", "+ fat LTO, 1 CGU, abort",
              "+ thin LTO", "+ fat LTO, do_action demoted"]

# NEG_MARKS are the timeline's "measured worse" marks: experiment, time on 07-16 (the round it
# was tried in; only the date is recorded) and effect (mockup).
NEG_MARKS = [
    ("color-list prealloc", "02:02:23", "0 osc gain; reverted 408ddaa"),
    ("by-value InlineVec Action payloads", "05:00:15", "+25..60% terminal-parser"),
    ("csel mask restructure", "07:03:35", "screen-clone.clone 54.4 → 54.9 ms"),
    ("style value-memoization", "09:07:45", "+42% screen-clone.render"),
    ("plain-run memcpy for clone", "10:00:32", "+28% screen-clone.clone"),
    ("branchless UTF-8 DFA", "09:41:05", "+12..22% real lanes"),
    ("OSC loop variants ×4", "09:50:00", "all worse (store-forward floor)"),
]

# GAP_RANK is the gap-class table order (indices into perf-gap-classes.csv): ratio gaps
# first, then % slower, then share-of-time, then unquantified (mockup).
GAP_RANK = [1, 5, 0, 2, 4, 10, 8, 3, 9, 6, 7]

# WATERFALL_BENCHES are the five key benchmarks with a waterfall, in display order.
WATERFALL_BENCHES = ["terminal-parser", "screen-clone.clone", "screen-clone.render", "osc-parser", "terminal-stream"]

# COMPTIME_BENCHES are the four benchmarks of the compile-time-flattening steps.
COMPTIME_BENCHES = ["terminal-snapshot.encode", "hyperlink-map.churn", "terminal-resize.rows",
                    "terminal-formatter.format-vt"]


def _strip(text: str, changes: list) -> str:
    """Return text with SUBSTITUTIONS and the shared stripper applied (code punctuation kept), recording changes."""
    out = text
    for pat, rep in SUBSTITUTIONS:
        out = re.sub(pat, rep, out)
    for code, mark in PROTECT:
        out = out.replace(code, mark)
    out = jargon.strip_internal_ids(out)[0]
    for code, mark in PROTECT:
        out = out.replace(mark, code)
    left = jargon.find_internal_ids(out)
    if left:
        raise ValueError(f"{SLUG}: internal ids {left} left in {out!r}")
    if out != text:
        changes.append((text, out))
    return out


def _rows(research: Path, name: str) -> list:
    """Return the rows of research/data/<name> as dicts."""
    with open(Path(research) / "data" / name, newline="") as f:
        return list(csv.DictReader(f))


def _stats(rust: dict, zig: dict, bench: list) -> dict:
    """Return W/T/L (±1.5% tie band), suite sums, suite delta % and geomean of one sweep (build.py)."""
    w = t = l = 0
    logs, sr, sz = [], 0.0, 0.0
    for b in bench:
        d = (rust[b][0] / zig[b][0] - 1) * 100
        if abs(d) < 1.5:
            t += 1
        elif d < 0:
            w += 1
        else:
            l += 1
        logs.append(math.log(rust[b][0] / zig[b][0]))
        sr += rust[b][0]
        sz += zig[b][0]
    return {"W": w, "T": t, "L": l, "suiteR": round(sr, 1), "suiteZ": round(sz, 1),
            "suiteD": round((sr / sz - 1) * 100, 1), "geo": round(math.exp(sum(logs) / len(logs)), 3)}


def _moved(prev: dict, rust: dict, bench: list, floor: set) -> list:
    """Return [bench, % change] for benchmarks that moved ≥4% and outside their spread since prev (build.py)."""
    out = []
    for b in bench:
        a, n = prev[b], rust[b]
        ch = n[0] / a[0] - 1
        if abs(ch) >= 0.04 and (n[2] < a[1] or n[1] > a[2]) and b not in floor:
            out.append([b, round(ch * 100, 1)])
    return out


def _stages(mock: dict, changes: list) -> list:
    """Return the 14 sweeps with recomputed statistics, plain keys and cleaned texts."""
    bench, floor = mock["bench"], set(mock["floor"])
    out, prev = [], None
    for st in mock["stages"]:
        stats = _stats(st["rust"], st["zig"], bench)
        out.append({
            "key": KEYS[st["key"]],
            "results": st["rf"],
            "zigResults": st["zf"],
            "commit": st["commit"],
            "t": st["t"],
            "subject": _strip(st["subject"], changes),
            "label": _strip(st["label"], changes),
            "kind": st["kind"],
            "note": _strip(st["note"], changes),
            "rust": st["rust"],
            "zig": st["zig"],
            **stats,
            "moved": _moved(prev, st["rust"], bench, floor) if prev else [],
        })
        prev = st["rust"]
    return out


def _check_scoreboard(stages: list, research: Path) -> None:
    """Raise unless the official stages match perf-scoreboard-by-stage.csv (W/T/L, suites, delta)."""
    by_key = {s["key"]: s for s in stages}
    for key, row in zip(OFFICIAL, _rows(research, "perf-scoreboard-by-stage.csv")):
        st = by_key[key]
        want = (int(row["wins"]), int(row["ties_abs_lt_1.5pct"]), int(row["losses"]), float(row["suite_delta_pct"]))
        if (st["W"], st["T"], st["L"], st["suiteD"]) != want or abs(st["suiteR"] - float(row["suite_rust_ms"])) > 0.15:
            raise ValueError(f"{SLUG}: stage {key} disagrees with the scoreboard row {row['stage']}")


def _args(mock: dict) -> dict:
    """Return each benchmark's recorded arguments with the internal corpus prefix elided."""
    return {b: re.sub(r"--data=\.port/[^ ]*?/corpora/", "--data=…/corpora/", a) for b, a in mock["args"].items()}


def _commits(mock: dict, changes: list) -> list:
    """Return the 37 commits of the window with cleaned subjects (time, diffstat, up to 6 files)."""
    return [{"h": c["h"], "t": c["t"], "s": _strip(c["s"], changes), "stat": c["stat"], "files": c["files"][:6]}
            for c in mock["commits"]]


def _lto(research: Path) -> dict:
    """Return the LTO stage chain (12 binaries, 14 benchmarks each) and the 5-profile experiment."""
    chain: dict = {}
    for r in _rows(research, "perf-r1-stage-chain-lto-profile.csv"):
        step = chain.setdefault(int(r["order"]), {
            "order": int(r["order"]), "fix": CHAIN_TEXT.get(r["fix"], r["fix"]), "commit": r["commit"],
            "verdict": CHAIN_TEXT.get(r["verdict"], r["verdict"]), "ms": {}})
        step["ms"][r["bench"]] = float(r["rust_ms"])
    profiles: list = []
    for r in _rows(research, "perf-lto-profile-experiment.csv"):
        if not profiles or profiles[-1]["name"] != r["profile"]:
            profiles.append({"name": r["profile"], "label": LTO_LABELS[len(profiles)], "ms": {}})
        profiles[-1]["ms"][r["bench"]] = float(r["rust_ms"])
    return {"chain": [chain[k] for k in sorted(chain)], "profiles": profiles}


def _provisional(research: Path) -> dict:
    """Return the 07-30 paired re-measure from perf-timeline.csv: date and its "n/n" win count."""
    for r in _rows(research, "perf-timeline.csv"):
        if "provisional" in r["event"]:
            m = re.match(r"(\d+/\d+)", r["wins_of_total"])
            if not m:
                raise ValueError(f"{SLUG}: unparsed wins_of_total {r['wins_of_total']!r}")
            return {"date": r["date"], "wins": m.group(1)}
    raise ValueError(f"{SLUG}: no provisional re-measure row in perf-timeline.csv")


def _waterfall(research: Path, changes: list) -> list:
    """Return the five key benchmarks' official steps: label, round, commits, Rust and Zig ms, results file."""
    rows = _rows(research, "perf-waterfall-key-benches.csv")
    out = []
    for b in WATERFALL_BENCHES:
        steps = []
        for r in rows:
            if r["bench"] != b:
                continue
            m = re.match(r"r(\d)(?:-r(\d))?", r["step"])
            rnd = "pre-fix" if r["step"].startswith("pre-fix") else (
                f"rounds {m.group(1)}–{m.group(2)}" if m and m.group(2) else f"round {m.group(1)}")
            steps.append({"step": _strip(r["step"], changes), "round": rnd,
                          "commits": [] if r["commits"] == "-" else [h.strip() for h in r["commits"].split(",")],
                          "rust": float(r["rust_ms"]), "zig": float(r["zig_ms_same_session"]),
                          "results": r["source_under_.port/phase-2"]})
        out.append({"bench": b, "steps": steps})
    return out


def _negatives(research: Path, changes: list) -> list:
    """Return the 11 falsified optimisations (experiment, benchmark, date, effect, why)."""
    return [{"exp": _strip(r["experiment"], changes), "bench": r["target_bench"], "date": r["date"],
             "effect": _strip(r["measured_effect"], changes), "why": _strip(r["why"], changes)}
            for r in _rows(research, "perf-negative-results.csv")]


def _full33(mock: dict) -> list:
    """Return the 33 configs vs Ghostty main: [median, min, max] before/after, Rust and Zig."""
    return [{"id": r["id"], "bR": r["bR"], "bZ": r["bZ"], "fR": r["fR"], "fZ": r["fZ"]} for r in mock["full33"]]


def _adv_pin(research: Path) -> dict:
    """Return the advanced-pin re-measure (two independent runs per benchmark)."""
    rows = _rows(research, "perf-ratio-pin-12752b2ac-idle.csv")
    return {"rev": rows[0]["oracle_rev"], "date": rows[0]["date"], "head": rows[0]["rustty_head"],
            "rows": [{"bench": r["bench"], "ghostty": float(r["ghostty_ms"]), "rustty": float(r["rustty_ms"]),
                      "run1": float(r["ratio_ws10_fence1_n9"]), "run2": float(r["ratio_g8b_run"])} for r in rows]}


def _apc(research: Path, changes: list) -> list:
    """Return the APC scan and hyperlink fix: 3 configs × 3 stages (Rustty ÷ Ghostty)."""
    return [{"config": r["config"], "stage": _strip(r["stage"], changes), "ratio": float(r["rustty_over_ghostty"]),
             "metric": _strip(r["metric"], changes), "commit": r["commit"]}
            for r in _rows(research, "perf-n511-apc-hyperlink.csv")]


def _comptime(research: Path, changes: list) -> list:
    """Return the compile-time-flattening steps of the four benchmarks (fix, hazard subclass, gap after)."""
    rows = _rows(research, "perf-comptime-flattening-steps.csv")
    out = []
    for b in COMPTIME_BENCHES:
        rs = [r for r in rows if r["bench"] == b]
        out.append({"bench": b, "ratio": rs[0]["metric"] == "rust_over_zig_x", "steps": [
            {"order": int(r["order"]), "fix": _strip(r["fix"], changes),
             "cls": "" if r["hazard_subclass"] == "-" else _strip(r["hazard_subclass"], changes),
             "value": float(r["value_after"]), "src": _strip(r["provenance"], changes)} for r in rs]})
    return out


def _gaps(research: Path, changes: list) -> list:
    """Return the 11 hazard classes in GAP_RANK order."""
    rows = _rows(research, "perf-gap-classes.csv")
    keys = [("class", "class"), ("zig", "zig_construct"), ("lost", "port_spelling_that_lost"), ("fix", "fix"),
            ("example", "example"), ("worst", "worst_gap_before")]
    return [{k: _strip(rows[i][col].replace("-", "—") if rows[i][col] == "-" else rows[i][col], changes)
             for k, col in keys} for i in GAP_RANK]


def _mb2(v: float) -> str:
    """Formats an RSS value in MB as the mockup does: two decimals under 10, one above."""
    return f"{v:.2f}" if v < 10 else f"{v:.1f}"


def _memory(research: Path) -> list:
    """Return the memory and size rows (Rustty and Ghostty values, label, note) from the CSVs."""
    alloc = {r["metric"]: r for r in _rows(research, "perf-alloc-memory.csv")}
    size = {r["artifact"]: r for r in _rows(research, dated("perf-binary-size.csv"))}
    libvt = {r["artifact"]: r for r in _rows(research, dated("r2-libvt-sizes.csv"))}
    mib = 1048576

    def pair(metric: str) -> tuple:
        """Returns (rust, zig) of an alloc-memory metric as floats."""
        return float(alloc[metric]["rust"]), float(alloc[metric]["zig"])

    rows = []
    r, z = pair("page allocs, full PageList workload")
    rows.append(("page allocations", r, z, f"{r:g} = {z:g}",
                 'Full PageList workload: 6 vs 6 page allocations (parity). An earlier "7 vs 3" compared unequal scopes.'))
    r, z = pair("PageList workload latency (median of 9)")
    rows.append(("PageList latency", r, z, f"{r:g} vs {z:g} µs", "Median of 9, idle host."))
    r, z = pair("peak outstanding page bytes")
    rows.append(("peak page bytes*", r, z, f"{r / mib:.2f} vs {z / mib:.2f} MiB",
                 "Geometry difference against the old reference commit only: std page 409,600 vs 589,824 B. "
                 "From reference commit 20abdb50a on, Ghostty's std page is also 409,600 B."))
    for bench in ["terminal-parser", "screen-clone.clone", "screen-clone.render", "osc-parser"]:
        r, z = pair(f"peak RSS {bench}")
        note = "The one config of 8 where Rustty RSS is higher." if r > z else ""
        rows.append((f"RSS {bench}", r, z, f"{_mb2(r)} vs {_mb2(z)} MB", note))
    r, z = pair("peak RSS, alloc bench")
    rows.append(("RSS alloc bench*", r, z, f"{r / mib:.2f} vs {z / mib:.2f} MiB",
                 "Asymmetric: the Rust binary carries an allocation-tracking instrumentation seam."))
    s = size["ghostty-bench (ReleaseFast)"]
    r, z = float(s["rustty_bytes"]), float(s["ghostty_bytes"])
    rows.append(("bench binary", r, z, f"{r / mib:.2f} MB vs {z / mib:.2f} MB", ""))
    s = size["ghostty-gen (ReleaseFast)"]
    r, z = float(s["rustty_bytes"]), float(s["ghostty_bytes"])
    rows.append(("gen tool", r, z, f"{r / 1000:.0f} vs {z / 1000:.0f} KB",
                 "Unwind tables and longer mangled symbols; __text is 8 KB smaller."))
    # The two historical rows compare with Ghostty's library at their own reference commit
    # (12752b2ac, the earlier snapshot's oracle row); the HEAD row with the current one.
    oracle_key = "libghostty-vt.0.1.0.dylib (ReleaseFast) oracle"
    oracle = float(libvt[oracle_key]["bytes"])
    oracle_old = float({r["artifact"]: r for r in _rows(research, "r2-libvt-sizes.csv")}[oracle_key]["bytes"])
    head = libvt["librustty_vt_c.dylib (release; macOS arm64)"]["rev"].split()[1]
    for name, art, ref, note in [
        ("libvt, before size work", "librustty_vt_c.dylib before the N512 size work", oracle_old,
         "librustty_vt_c.dylib before the size work, against Ghostty's library at 12752b2ac."),
        ("libvt, re-base 10-07", "librustty_vt_c.dylib at the size re-base", oracle_old,
         "Against Ghostty's library at 12752b2ac."),
        ("libvt dylib at HEAD", "librustty_vt_c.dylib (release; macOS arm64)", oracle,
         f"HEAD {head}; Ghostty's library at the same reference commit, {PIN_REV}."),
    ]:
        r = float(libvt[art]["bytes"])
        rows.append((name, r, ref, f"{r:,.0f} B", note))
    return [{"name": n, "rust": r, "zig": z, "lab": lab, "note": note} for n, r, z, lab, note in rows]


def _ms(v: str):
    """Return a CSV millisecond value as a float, or None when the run did not record it (empty)."""
    return float(v) if v.strip() else None


def current_pin(research: Path, bench: list | None = None) -> dict:
    """Return the current-pin slot: pending (no rows) unless research/data/PIN_FILE has been recorded.

    A recorded file must hold exactly the suite's benchmarks (bench, when given; else 14 rows),
    each with positive medians and provenance MEASURED; empty min/max become null.
    """
    path = Path(research) / "data" / PIN_FILE
    slot = {"rev": PIN_REV, "head": PIN_HEAD, "status": "pending", "rows": []}
    if not path.exists():
        return slot
    rows = _rows(research, PIN_FILE)
    missing = [c for c in PIN_COLUMNS if rows and c not in rows[0]]
    if not rows or missing:
        raise ValueError(f"{SLUG}: {PIN_FILE} has no rows or lacks columns {missing}")
    names = [r["bench"] for r in rows]
    if len(rows) != 14 or len(set(names)) != 14 or (bench is not None and sorted(names) != sorted(bench)):
        raise ValueError(f"{SLUG}: {PIN_FILE} must hold the 14 suite benchmarks once each, got {names}")
    for r in rows:
        if r["provenance"] != "MEASURED":
            raise ValueError(f"{SLUG}: {PIN_FILE} row {r['bench']} is {r['provenance']!r}, not MEASURED")
        if not (_ms(r["ghostty_ms"]) or 0) > 0 or not (_ms(r["rustty_ms"]) or 0) > 0:
            raise ValueError(f"{SLUG}: {PIN_FILE} row {r['bench']} lacks a positive median")
    dates = {r["date"] for r in rows}
    heads = {r["rustty_head"] for r in rows}
    if len(dates) != 1 or len(heads) != 1:
        raise ValueError(f"{SLUG}: {PIN_FILE} mixes runs: dates {dates}, heads {heads}")
    slot.update(status="measured", head=heads.pop(), date=dates.pop(), note=PIN_NOTE, rows=[
        {"bench": r["bench"],
         "ghostty": [_ms(r["ghostty_ms"]), _ms(r["ghostty_min_ms"]), _ms(r["ghostty_max_ms"])],
         "rustty": [_ms(r["rustty_ms"]), _ms(r["rustty_min_ms"]), _ms(r["rustty_max_ms"])]} for r in rows])
    return slot


def build(research: Path, mockbuild: Path) -> dict:
    """Return the perf-replay widget data (see module docstring for sources)."""
    mock = json.loads((Path(research) / "data/mock-05-perf-replay-data.json").read_text())
    changes: list = []
    stages = _stages(mock, changes)
    _check_scoreboard(stages, research)
    data = {
        "_meta": {
            "slug": SLUG,
            "generatedFrom": [
                "research/data/mock-05-perf-replay-data.json (raw .jsonl sweeps and git, via mockup-build/w05/build.py)",
                "research/data/perf-scoreboard-by-stage.csv (cross-check)",
                "research/data/perf-r1-stage-chain-lto-profile.csv, perf-lto-profile-experiment.csv",
                "research/data/perf-timeline.csv (the provisional 07-30 re-measure)",
                "research/data/perf-waterfall-key-benches.csv, perf-negative-results.csv",
                "research/data/perf-ratio-pin-12752b2ac-idle.csv, perf-n511-apc-hyperlink.csv",
                "research/data/perf-comptime-flattening-steps.csv, perf-gap-classes.csv",
                f"research/data/perf-alloc-memory.csv, {dated('perf-binary-size.csv')}, {dated('r2-libvt-sizes.csv')}",
                f"research/data/{PIN_FILE} (the run at the current reference commit)",
            ],
            "asOf": PIN_REV,
        },
        "bench": mock["bench"],
        "floor": mock["floor"],
        "args": _args(mock),
        "stages": stages,
        "commits": _commits(mock, changes),
        "lto": _lto(research),
        "provisional": _provisional(research),
        "negatives": _negatives(research, changes),
        "negMarks": [{"exp": e, "t": f"2026-07-16T{t}", "effect": eff} for e, t, eff in NEG_MARKS],
        "waterfall": _waterfall(research, changes),
        "full33": _full33(mock),
        "advPin": _adv_pin(research),
        "apc": _apc(research, changes),
        "comptime": _comptime(research, changes),
        "gaps": _gaps(research, changes),
        "memory": _memory(research),
        "pin": current_pin(research, mock["bench"]),
    }
    _record_review(changes)
    return data


def _record_review(changes: list) -> None:
    """Append each (before, after) rewrite to jargon-review.txt once, for the hand review."""
    path = Path(__file__).resolve().parent.parent / "jargon-review.txt"
    existing = set(path.read_text().splitlines()) if path.exists() else set()
    lines = [f"{SLUG}\t{b}\t{a}" for b, a in dict.fromkeys(changes)]
    new = [ln for ln in lines if ln not in existing]
    if new:
        with open(path, "a") as f:
            f.write("".join(ln + "\n" for ln in new))
