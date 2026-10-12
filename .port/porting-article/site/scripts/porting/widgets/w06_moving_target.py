"""Data builder for the "Chasing a moving target" widget (slug moving-target).

Ports the data step of the mockup build (mockup-build/mock/m06.py and the mockup's
embedded data object) onto the persisted research files:

Every research file below is read at the current snapshot (``snapshot.dated``: the
``-2026-10-10`` regeneration of the named dataset).

- research/data/timeline-upstream_pins.csv  the pin chain: upstream dates, upstream commits
  (all / non-merge), files changed, lines inserted and deleted, and the Ghostty ``.zig`` line
  count at each pin
- research/data/mock-06-moving-target-ghostty-size.csv  Ghostty ``.zig`` lines at the
  first-parent commit of each week (the rows flagged ``pin`` are the pins, not weekly samples)
- research/data/mock-06-moving-target-areas.csv  the end-to-end diffstat of each pin range
  grouped by top-level directory, with the non-merge commits touching each directory
- research/data/timeline-loc_by_day_wide.csv  Rustty ``.rs`` lines per day (every third day,
  the days a pin moved, and the last day)

The hand-recorded git facts (upstream commit time of each new pin, time of the Rustty commit
that adopted it, the commit itself, the era-classifier commit counts, the arrival gaps and
the pin-moving commit messages) are the ADVANCES table below; the wait in hours is computed
from the two times. Strings pass through the shared id stripper and the plain-word
substitutions; each rewrite goes to jargon-review.txt.
"""
import csv
import re
from datetime import datetime
from pathlib import Path

import jargon

from .snapshot import dated

SLUG = "moving-target"

# SUBSTITUTIONS are plain-word replacements for the port's own vocabulary, applied before the
# shared stripper (the internal name for the reference build is "oracle").
SUBSTITUTIONS = [
    (r"\boracle sync point\b", "reference commit"),
]

# MAIN_ROWS are the matrix rows that keep their own line, in display order; every other
# top-level directory is folded into one "other" row.
MAIN_ROWS = [
    "src/terminal", "po", "pkg", "src/font", "src/lib", "include", "src/apprt", "macos",
    "src/ (root files)", "src/renderer", "src/benchmark", "src/os", "src/datastruct",
    "src/termio", "src/build", "src/cli", "example", "src/config", "src/input",
]

# NOTES are the per-advance remarks under the churn bars, keyed by advance number.
NOTES = {
    2: "27,267 of the 72,554 inserted lines are translation catalogues (po/).",
    3: "Upstream added kitty OSC 5522 clipboard, OSC 72 drag-and-drop and graphics animation: 202 of the 295 dropped tests were kitty tests.",
    6: "pkg churn is mostly regenerated fontconfig override headers (pkg/fontconfig/override) plus a wuffs zlib decoder; src/font and src/lib moved off cImport to translate-c.",
}

# ADVANCES holds, per pin move in date order: the target pin, the upstream commit time of the
# new pin and the time of the Rustty commit that adopted it (both local, Pacific), that commit,
# how long until the resync finished, the Rustty commits counted by the era classifier (era) and
# by git between the move and the end of the resync (fin), the reference comments missing on arrival (cm),
# the test gap on arrival (ts) and the move's commit message in plain words (q).
ADVANCES = [
    dict(to="6ad1fe7d8", up="2026-07-29T08:18", adv="2026-07-31T07:55", commit="0efe06a1c", dur="~2 d",
         durTxt="07-29 → 07-31: the range was ported first, then the reference moved at the end",
         era=224, fin=None, cm=None, ts=None,
         q="Moved the reference commit to 6ad1fe7d8 after porting the range; the local Ghostty checkout the checks read was moved to it as well."),
    dict(to="12967b68f", up="2026-08-17T13:39", adv="2026-08-17T22:27", commit="c8e76c6eb", dur="7.6 d",
         durTxt="moved 08-17 22:27 → catch-up finished 08-25 12:06",
         era=13, fin=13, fin_note="the work was squashed into few commits", cm=1230, ts="36 dropped tests in 10 modules",
         q="Moved at the START of the catch-up, by the author's direction. Before: every check green. After: 1,230 reference comments missing (921 in the terminal engine, 187 in its C API, the rest over 15 crates) and 36 Zig tests without a Rust counterpart in 10 modules. These red counts became the progress meter."),
    dict(to="683d8db64", up="2026-08-25T11:08", adv="2026-08-25T12:50", commit="cc8c65309", dur="9.7 h",
         durTxt="moved 08-25 12:50 → catch-up finished 08-25 22:34",
         era=10, fin=10, cm=1536, ts="295 dropped tests in 5 modules",
         q="Before: green. After: 295 Zig tests without a counterpart (kitty 202 for the new OSC 5522 clipboard, OSC 72 drag-and-drop and graphics animation; OSC 47; stream 38; paste 7; parser 1) and 1,536 reference comments missing (1,217 in the terminal engine)."),
    dict(to="88f57ee66", up="2026-08-25T21:27", adv="2026-08-26T00:58", commit="414426956", dur="one commit",
         durTxt="a single commit absorbed the 2-commit tail",
         era=1, fin=1, cm=None, ts=None, label="2-commit drift",
         q="Absorbed a 2-commit tail of upstream stream.zig changes (reference 683d8db64 → 88f57ee66)."),
    dict(to="20abdb50a", up="2026-08-31T23:55", adv="2026-09-01T01:49", commit="62c1690d9", dur="6.0 h",
         durTxt="moved 09-01 01:49 → catch-up finished 09-01 07:51; both CI lanes recorded 09-08",
         era=35, fin=33, cm=51, ts="23 Zig tests unseen (test gate green only by bookkeeping)",
         q="After the move the test-count check stayed green only because two new upstream files were outside its list (23 unseen tests), and 51 reference comments were missing (terminal engine 38, config 6, C API 4, lib 2, core 1)."),
    dict(to="12752b2ac", up="2026-09-28T11:40", adv="2026-09-28T19:06", commit="9f007b10e", dur="31.4 h",
         durTxt="moved 09-28 19:06 → catch-up finished 09-30 02:32; long verification runs to 10-07",
         era=327, fin=130, cm=610, ts="94 Zig test blocks without a counterpart",
         q="On arrival: 610 reference comments missing, 94 Zig test blocks without a counterpart, 8 new files, 57 new public functions; the C header and reference-geometry checks stayed green."),
    dict(to="a60e9e2a5", up="2026-10-07T06:42", adv="2026-10-07T21:38", commit="7719ebb4d", dur="9.7 h",
         durTxt="moved 10-07 21:38 → catch-up finished 10-08 07:22",
         era=81, fin=54, cm=411, ts="73 Zig test blocks without a counterpart",
         q="On arrival: 411 reference comments missing (433 in strict mode), 73 Zig test blocks without a counterpart, 6 new files, 8 new public functions; the C header and reference-geometry checks stayed green."),
    dict(to="246f70287", up="2026-10-09T08:57", adv="2026-10-09T16:36", commit="2bf89b95a", dur="1.2 h",
         durTxt="moved 10-09 16:36 → catch-up finished 10-09 17:50 (part of the porting merged just before the move)",
         era=9, fin=4, cm=19, ts="8 Zig test blocks without a counterpart",
         q="On arrival: 19 reference comments missing (19 in strict mode, all in the terminal engine), 8 Zig test blocks without a counterpart (7 soft-reset tests in Terminal.zig, 1 in stream.zig), no new files, 1 new public function without a counterpart; the C header and reference-geometry checks stayed green."),
]


def _clean(text: str, changes: list) -> str:
    """Return text with the plain-word substitutions and the shared id stripper applied, recording each change."""
    out = text
    for pat, rep in SUBSTITUTIONS:
        out = re.sub(pat, rep, out)
    out, _ = jargon.strip_internal_ids(out)
    if out != text:
        changes.append((text, out))
    return out


def _hours(up: str, adv: str) -> float:
    """Return the hours from the upstream commit time up to the Rustty pin-moving commit time adv, to one decimal."""
    fmt = "%Y-%m-%dT%H:%M"
    return round((datetime.strptime(adv, fmt) - datetime.strptime(up, fmt)).total_seconds() / 3600, 1)


def _rows(path: Path) -> list:
    """Return the rows of a research CSV as dicts."""
    with open(path, newline="") as f:
        return list(csv.DictReader(f))


def _areas(research: Path, pins: list) -> tuple:
    """Return (rows, other_names): per-directory [ins, del, files, commits] per advance, with the minor directories folded."""
    by_dir: dict = {}
    for r in _rows(research / "data" / dated("mock-06-moving-target-areas.csv")):
        i = next(k for k, p in enumerate(pins) if r["range"].endswith(".." + p))
        cell = [int(r["insertions"]), int(r["deletions"]), int(r["files"]), int(r["nonmerge_commits_touching"])]
        by_dir.setdefault(r["area"], [None] * len(pins))[i] = cell
    other_names = sorted(a for a in by_dir if a not in MAIN_ROWS)
    other: list = [None] * len(pins)
    for a in other_names:
        for i, cell in enumerate(by_dir[a]):
            if cell:
                other[i] = [x + y for x, y in zip(other[i] or [0, 0, 0, 0], cell)]
    rows = [{"a": a, "v": by_dir[a]} for a in MAIN_ROWS]
    rows.append({"a": f"other ({len(other_names)} dirs)", "v": other, "other": True})
    return rows, other_names


def _rust_series(research: Path, advance_days: set) -> list:
    """Return [date, .rs lines] for every third day from the first day, the pin-move days and the last day."""
    days = _rows(research / "data" / dated("timeline-loc_by_day_wide.csv"))
    keep = [i for i, r in enumerate(days) if i % 3 == 0 or r["date"] in advance_days or i == len(days) - 1]
    return [[days[i]["date"], int(days[i]["rust_total"])] for i in keep]


def build(research: Path, mockbuild: Path) -> dict:
    """Return the moving-target widget data (see the module docstring for sources)."""
    research = Path(research)
    changes: list = []
    chain = _rows(research / "data" / dated("timeline-upstream_pins.csv"))
    base = chain[0]
    by_to = {r["to_pin"]: r for r in chain[1:]}
    pins = [a["to"] for a in ADVANCES]
    areas, other_names = _areas(research, pins)
    adv = []
    prev = base["to_pin"]
    for n, a in enumerate(ADVANCES, 1):
        r = by_to[a["to"]]
        if r["from_pin"] != prev:
            raise ValueError(f"{SLUG}: pin chain broken at {a['to']}")
        cells = [row["v"][n - 1] for row in areas if row["v"][n - 1]]
        drift = not r["insertions"]
        adv.append({
            "n": n,
            "label": a.get("label"),
            "from": prev,
            "to": a["to"],
            "up": a["up"],
            "adv": a["adv"],
            "lag": _hours(a["up"], a["adv"]),
            "all": int(r["upstream_commits_incl_merges"]),
            "nm": int(r["upstream_nonmerge_commits"]),
            "files": sum(c[2] for c in cells) if drift else int(r["files_changed"]),
            "ins": sum(c[0] for c in cells) if drift else int(r["insertions"]),
            "del": sum(c[1] for c in cells) if drift else int(r["deletions"]),
            "loc": int(r["ghostty_zig_loc_at_to_pin_all_files"]),
            "dur": a["dur"],
            "durTxt": a["durTxt"],
            "era": a["era"],
            "fin": a["fin"],
            "finNote": a.get("fin_note"),
            "cm": a["cm"],
            "ts": a["ts"],
            "commit": a["commit"],
            "q": _clean(a["q"], changes),
            "note": NOTES.get(n),
            "drift": drift,
        })
        prev = a["to"]
    size = [r for r in _rows(research / "data" / dated("mock-06-moving-target-ghostty-size.csv")) if not r["pin"]]
    rust = _rust_series(research, {a["adv"][:10] for a in ADVANCES})
    _record_review(changes)
    return {
        "_meta": {
            "slug": SLUG,
            "generatedFrom": [
                f"research/data/{dated('timeline-upstream_pins.csv')}",
                f"research/data/{dated('mock-06-moving-target-ghostty-size.csv')}",
                f"research/data/{dated('mock-06-moving-target-areas.csv')}",
                f"research/data/{dated('timeline-loc_by_day_wide.csv')}",
                "git history (pin-moving commit times and messages)",
            ],
            "asOf": rust[-1][0],
        },
        "baseline": {"sha": base["to_pin"], "date": base["to_pin_upstream_date"], "loc": int(base["ghostty_zig_loc_at_to_pin_all_files"])},
        "gh": [[r["date"], int(r["ghostty_zig_lines"])] for r in size],
        "rs": rust,
        "adv": adv,
        "rows": areas,
        "otherNames": other_names,
    }


def _record_review(changes: list) -> None:
    """Append each (before, after) rewrite to jargon-review.txt once, for the hand review."""
    path = Path(__file__).resolve().parent.parent / "jargon-review.txt"
    existing = set(path.read_text().splitlines()) if path.exists() else set()
    lines = [f"{SLUG}\t{b}\t{a}" for b, a in dict.fromkeys(changes)]
    new = [ln for ln in lines if ln not in existing]
    if new:
        with open(path, "a") as f:
            f.write("".join(ln + "\n" for ln in new))
