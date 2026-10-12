"""Data builder for the "Verification matrix" widget (slug verification-matrix).

Reproduces the data of mockup 10 (hosts x tools across the 8 reference commits). The
mockup embeds its data in its HTML, so widgets/extract_embed.py cut it out once into the
committed input

    scripts/porting/inputs/verification-matrix.json
        pins, hosts, tools, na, runs, notes, durations   (as the mockup wrote them)

which this builder reads. The mockup's run list was reconstructed from the port's records
and commit subjects (research/data/bugs-verification-gates.csv, r2-platforms-matrix.csv,
the resync and closure records), so there is no gate-run log to read instead.

What changes on the way:

- run status letters become words (green, fixed = "found bugs, fixed, green", incomplete,
  pending); tool names are spelled out; record names that carried planning labels are
  replaced by plain descriptions of the record (SOURCES)
- every string goes through the shared id finder and the id stripper must leave it
  unchanged; each rewrite is appended to jargon-review.txt
- hero figures are derived from the runs (the headline lanes are described below); the Miri 6.9x speed-up and its two timings are the mockup's literals
  (HERO_LITERALS)

Per-lane results at the last two reference commits
--------------------------------------------------
The mockup stopped at a60e9e2a5 with its sanitizer, Valgrind and Miri cells `pending`. The
results recorded since live in the research snapshot

    research/data/verification-lanes-2026-10-10.csv   (LANES_FILE, through snapshot.dated)

(UTF-8 CSV; extra columns are ignored) with this header

    pin,tool,host,status,tests,seconds,date,text,port_defects,ghostty_defects,source

pin is one of the last two reference commits (a60e9e2a5 or 246f70287); tool is a tool key
(sb, tb, capi, asan, lsan, msan, tsan, hwasan, vg, xb, ts); host is mac, lin, msvc or gnu;
status is green, fixed (found bugs, fixed, green) or incomplete; tests and seconds may be
empty; date is MM-DD; text is the record sentence; source names the record. Each row
replaces whatever the cell held (a pending placeholder or an earlier, narrower run) or fills
an empty cell; a row for any other commit is an error. Each lane is thus shown at every
commit it completed at, and no cell stays pending once a result exists for it.

Counts keep their original definitions: a test-only flake that a run found and fixed is not
counted as a port defect (TEST_FLAKES), as no flake was counted in the mockup's data.

The headline counts the eight sanitizer and Valgrind lanes of the first complete matrix
(HERO_LANES, 61,403 test runs at 12752b2ac), each at the latest reference commit it completed
at. The Miri seconds tile and the wall-time bars stay those of the first complete matrix at
12752b2ac: the later Miri timings were taken inside a full parallel check run and are not
comparable.
"""
import csv
import json
import re
from pathlib import Path

import jargon

from .snapshot import dated

SLUG = "verification-matrix"

# INPUT is the committed extraction of the mockup's embedded data.
INPUT = Path(__file__).resolve().parent.parent / "inputs/verification-matrix.json"

# LANES_FILE is the research dataset of per-lane results at the last two reference commits.
LANES_FILE = dated("verification-lanes.csv")

# TEST_FLAKES are lane results whose fix was to a test-only flake (a test that counted the test
# runner's thread, a load-dependent timeout, a log line caught from another test): they keep
# their status and record but count no port defect. Key: (reference commit, tool, host).
TEST_FLAKES = {("a60e9e2a5", "ts", "msvc"), ("246f70287", "lsan", "mac"), ("246f70287", "tsan", "mac")}

# CONTENDED_SECS are the reference commit and host whose lane timings were taken inside one full
# parallel check run (about 110 checks sharing the Mac); they are not comparable with earlier
# single-lane timings, so those cells show no wall time (the record sentence keeps the figures).
CONTENDED_SECS = ("246f70287", "mac")

# HERO_LANES are the eight sanitizer and Valgrind lanes of the first complete matrix (12752b2ac).
HERO_LANES = [("asan", "mac"), ("lsan", "mac"), ("lsan", "lin"), ("msan", "lin"), ("tsan", "mac"),
              ("tsan", "lin"), ("hwasan", "lin"), ("vg", "lin")]

# FIRST_COMPLETE is the reference commit where every lane first ran to completion; the Miri
# seconds tile and the wall-time bars are its figures.
FIRST_COMPLETE = "12752b2ac"

# STATUS maps the mockup's status letters to the words shipped in the data.
STATUS = {"g": "green", "f": "fixed", "x": "incomplete", "p": "pending"}

# TOOL_NAMES spells every tool out (key -> visible name); abbreviations are defined once in the widget legend.
TOOL_NAMES = {
    "sb": "Miri Stacked Borrows",
    "tb": "Miri Tree Borrows",
    "capi": "Miri, C API (vt-c)",
    "asan": "AddressSanitizer",
    "lsan": "LeakSanitizer",
    "msan": "MemorySanitizer",
    "tsan": "ThreadSanitizer",
    "hwasan": "HWAddressSanitizer",
    "vg": "Valgrind",
    "xb": "cross-platform build",
    "ts": "test suites",
}

# HOST_HW is the hardware line shown under each host column (the Mac wording is the article's fixed phrase).
HOST_HW = {
    "mac": ("macOS", "MacBook Pro, Apple M4 Pro"),
    "lin": ("Linux VM", "Fedora 44 aarch64 VM, 8 vCPU"),
    "msvc": ("Windows MSVC", "x86_64 under emulation"),
    "gnu": ("Windows GNU", "x86_64-pc-windows-gnu"),
}

# DURATION_NAMES spells out the abbreviations of the mockup's wall-time row names.
DURATION_NAMES = {
    "Miri SB, 38 crates": "Miri Stacked Borrows, 38 crates",
    "Miri TB, two runs": "Miri Tree Borrows, two runs",
    "HWASan": "HWAddressSanitizer",
    "MSan": "MemorySanitizer",
    "TSan": "ThreadSanitizer",
    "LSan": "LeakSanitizer",
    "ASan": "AddressSanitizer",
    "Miri, assertions off": "Miri, debug assertions off",
}

# SOURCES replaces record names that carry planning labels by a plain description of the record.
SOURCES = {
    "phase-6 README · 09-27 lane commits": "upstream catch-up record (September) · 09-27 lane commits",
    "phase-6 README": "upstream catch-up record (September)",
    "phase-7 README": "upstream catch-up record (09-30)",
    "phase-8 README": "upstream catch-up record (October)",
    "parity-closure README": "closure record",
    "bugs-verification-gates.csv": "verification ledger",
    "rvm, 10-08": "Linux VM run, 10-08",
    "cross-host record": "cross-host build record",
    "timeline dossier": "project timeline",
}

# TEXT_FIXES are plain-word replacements in record sentences and unavailability reasons.
TEXT_FIXES = [
    (r"\bIOCP backend\b", "Windows I/O completion port backend"),
    (r"\bXCTest app tests\b", "macOS app tests"),
    (r"\bXCTest (?=\d)", "macOS app tests "),
    (r"\bvt-c DLL\b", "vt-c shared library"),
    (r"\brenderer-only RED\b", "renderer-only failure"),
    (r"\bEach red-dot cell found bugs\b", "Each diamond (found and fixed) cell found bugs"),
    (r"\bpin move\b", "reference move"),
    (r"\b([Cc])urrent-pin\b", r"\1urrent-reference"),
    (r"\bthe Linux lane ran at the finale\b", "the Linux lane ran when its catch-up ended"),
    (r"\(same day as the second finale\)", "(the day the second catch-up ended)"),
    (r"^Miri lanes run on the Mac$", "Miri lanes run on the MacBook Pro, Apple M4 Pro"),
    (r"^compiler-rt has no MSan for Darwin$", "MemorySanitizer is not available on macOS (compiler-rt has no Darwin port)"),
    (r"^HWASan runs on aarch64 Linux only$", "HWAddressSanitizer runs on aarch64 Linux only"),
]

# HERO_LITERALS are the mockup's hero figures that no run row carries: the crate count of the
# Stacked Borrows sweep and the one-process-per-test Miri speed-up with its two timings (seconds).
HERO_LITERALS = {"crates": 38, "speedup": 6.9, "before": 11722, "after": 1687}

# PARTIAL_COUNTS are the runs whose recorded count covers only part of what the record names (one
# crate of 18 / 29 packages, or of 38 crates; the second of two Tree Borrows runs): their cell shows
# no test count and the sentence keeps the figures. Key: (pin index, tool, host).
PARTIAL_COUNTS = {(6, "ts", "lin"), (7, "ts", "lin"), (6, "sb", "mac"), (6, "tb", "mac")}

# TARGETED are the test-suite runs that covered only the packages the move touched, not the whole
# workspace; their cell carries a "targeted" tag. Key: (pin index, tool, host).
TARGETED = {(6, "ts", "lin"), (7, "ts", "lin"), (6, "ts", "msvc"), (6, "ts", "gnu"), (7, "ts", "msvc")}

# SANITIZERS are the tool keys whose test counts add up to the headline run count.
SANITIZERS = ("asan", "lsan", "msan", "tsan", "hwasan", "vg")

# FOUND_NOTE explains the Ghostty-side bars of the "what the lanes found" chart.
FOUND_NOTE = (
    "Ghostty-side: Valgrind (font collection leak), LeakSanitizer (Dear ImGui Metal leak), "
    "ThreadSanitizer (Highway race), plus a font-matching double destroy from the Linux "
    "test lane. Cross-platform bring-up found 25 more by hand."
)



def _clean(text: str, changes: list) -> str:
    """Return text with the plain-word fixes applied (recorded in changes); fail if an internal id remains or the stripper would change it."""
    out = SOURCES.get(text, text)
    for pat, rep in TEXT_FIXES:
        out = re.sub(pat, rep, out)
    if jargon.find_internal_ids(out) or jargon.strip_internal_ids(out)[0] != out:
        raise ValueError(f"{SLUG}: internal ids in {out!r}")
    if out != text:
        changes.append((text, out))
    return out


def _run(r: list, changes: list) -> dict:
    """Return one run row of the mockup as a keyed dict with a status word and cleaned strings."""
    key = (r[0], r[1], r[2])
    return {
        "pin": r[0], "tool": r[1], "host": r[2], "st": STATUS[r[3]],
        "tests": None if key in PARTIAL_COUNTS else r[4], "targeted": key in TARGETED, "secs": r[5],
        "date": r[6], "txt": _clean(r[7], changes), "port": r[8], "gh": r[9], "src": _clean(r[10], changes),
    }


def apply_lane_results(runs: list, pins: list, path: Path, changes: list, na: dict | None = None) -> list:
    """Return runs with each row of the lane results CSV at path placed in its cell at one of the last two pins (see the module docstring); ValueError on a bad row."""
    allowed = {pins[i]["sha"]: i for i in (len(pins) - 2, len(pins) - 1)}
    na = na or {}
    out = list(runs)
    with open(path, newline="", encoding="utf-8") as f:
        rows = list(csv.DictReader(f))
    tools = set(TOOL_NAMES)
    seen = set()
    for row in rows:
        key = (row["tool"], row["host"])
        if row["pin"] not in allowed:
            raise ValueError(f"{SLUG}: lane result for {row['pin']}, expected one of {sorted(allowed)}")
        if row["tool"] not in tools or row["host"] not in HOST_HW or f"{row['tool']}|{row['host']}" in na:
            raise ValueError(f"{SLUG}: unknown or unavailable lane {key}")
        if row["status"] not in ("green", "fixed", "incomplete"):
            raise ValueError(f"{SLUG}: status {row['status']!r} is not green, fixed or incomplete")
        if (row["pin"], *key) in seen:
            raise ValueError(f"{SLUG}: two lane results for {key} at {row['pin']}")
        seen.add((row["pin"], *key))
        pin = allowed[row["pin"]]
        flake = (row["pin"], *key) in TEST_FLAKES
        run = {
            "pin": pin, "tool": row["tool"], "host": row["host"], "st": row["status"],
            "tests": int(row["tests"]) if row["tests"] else None, "targeted": False,
            "secs": int(row["seconds"]) if row["seconds"] and (row["pin"], row["host"]) != CONTENDED_SECS else None,
            "date": row["date"], "txt": _clean(row["text"], changes),
            "port": 0 if flake else int(row["port_defects"] or 0), "gh": int(row["ghostty_defects"] or 0),
            "src": _clean(row["source"], changes),
        }
        idx = [i for i, r in enumerate(out) if r["pin"] == pin and (r["tool"], r["host"]) == key]
        if idx:
            out[idx[-1]] = run
            for i in reversed(idx[:-1]):
                del out[i]
        else:
            out.append(run)
    return out


def hero_lanes(runs: list, pins: list) -> list:
    """Return each HERO_LANES lane at the latest pin where it completed with a test count: tool, host, tests, pin sha."""
    out = []
    for tool, host in HERO_LANES:
        done = [r for r in runs if (r["tool"], r["host"]) == (tool, host) and r["tests"] is not None
                and r["st"] in ("green", "fixed")]
        if not done:
            raise ValueError(f"{SLUG}: no completed run for headline lane {(tool, host)}")
        last = max(done, key=lambda r: r["pin"])
        out.append({"tool": tool, "host": host, "tests": last["tests"], "pin": pins[last["pin"]]["sha"]})
    return out


def build(research: Path, mockbuild: Path, lanes_path: Path | None = None) -> dict:
    """Return the verification-matrix widget data (see the module docstring for sources); lanes_path overrides the lane results file."""
    raw = json.loads(INPUT.read_text(encoding="utf-8"))
    changes: list = []
    notes = raw["notes"]
    pins = [
        {"sha": p[0], "date": p[1], "label": label, "note": _clean(notes[str(i)], changes)}
        for i, (p, label) in enumerate(zip(raw["pins"], _pin_labels(len(raw["pins"]))))
    ]
    runs = [_run(r, changes) for r in raw["runs"]]
    lanes = Path(research) / "data" / LANES_FILE if lanes_path is None else lanes_path
    runs = apply_lane_results(runs, pins, lanes, changes, raw["na"])
    stale = [(r["pin"], r["tool"], r["host"]) for r in runs if r["st"] == "pending" and any(
        (o["tool"], o["host"]) == (r["tool"], r["host"]) and o["pin"] >= r["pin"] and o["st"] != "pending"
        for o in runs)]
    if stale:
        raise ValueError(f"{SLUG}: pending cells for lanes that have a result: {stale}")
    first = next(i for i, p in enumerate(pins) if p["sha"] == FIRST_COMPLETE)
    hl = hero_lanes(runs, pins)
    hero = {
        "ref": pins[-1]["sha"],
        "lanes": hl,
        "runs": sum(h["tests"] for h in hl),
        "ghostty": sum(r["gh"] for r in runs if r["tool"] in SANITIZERS),
        "miriRef": FIRST_COMPLETE,
        "miriSecs": next(r["secs"] for r in runs if r["pin"] == first and r["tool"] == "sb"),
        **HERO_LITERALS,
    }
    data = {
        "_meta": {
            "slug": SLUG,
            "generatedFrom": [
                "scripts/porting/inputs/verification-matrix.json (extracted from the approved mockup)",
                "research/data/bugs-verification-gates.csv",
                "research/data/r2-platforms-matrix.csv",
                "the upstream catch-up, closure and cross-host build records",
                f"research/data/{LANES_FILE} (lane results at the last two reference commits)",
            ],
            "asOf": pins[-1]["sha"],
        },
        "pins": pins,
        "hosts": [{"key": h[0], "name": HOST_HW[h[0]][0], "hw": HOST_HW[h[0]][1]} for h in raw["hosts"]],
        "tools": [{"key": t[0], "name": TOOL_NAMES[t[0]]} for t in raw["tools"]],
        "na": {k: _clean(v, changes) for k, v in raw["na"].items()},
        "runs": runs,
        "durations": [{"name": DURATION_NAMES.get(n, n), "host": h, "secs": s} for n, h, s in raw["durations"]],
        "durationsRef": FIRST_COMPLETE,
        "hero": hero,
        "foundNote": FOUND_NOTE,
    }
    _record_review(changes)
    return data


def _pin_labels(n: int) -> list:
    """Return the selector labels: baseline, move 1..n-1 (the 2-commit drift is move 4), the previous and the current commit marked."""
    labels = ["baseline"] + [f"move {i}" for i in range(1, n)]
    labels[4] = "move 4 (drift)"
    labels[n - 2] += " · previous"
    labels[n - 1] += " · current"
    return labels


def _record_review(changes: list) -> None:
    """Append each (before, after) rewrite to jargon-review.txt once, for the hand review."""
    path = Path(__file__).resolve().parent.parent / "jargon-review.txt"
    existing = set(path.read_text().splitlines()) if path.exists() else set()
    lines = [f"{SLUG}\t{b}\t{a}" for b, a in dict.fromkeys(changes)]
    new = [ln for ln in lines if ln not in existing]
    if new:
        with open(path, "a") as f:
            f.write("".join(ln + "\n" for ln in new))
