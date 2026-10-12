"""Data builder for the "Agents vs human" widget (slug agents-vs-human).

Ports the data step of the mockup's embed script (mockup-build/embed_w02.py)
onto the persisted research files:

- research/data/mock-agents-vs-human-grid.csv       day x hour commits, merges, prompts, lines, top subject
- research/data/mock-agents-vs-human-commit-lag.csv per-commit time and lag since the last port prompt
- research/data/mock-agents-vs-human-daily.csv      model mix, recovered API-equivalent cost, threads
- research/data/mock-agents-vs-human-annotations.json  eras, human eras, events, quiet stretches
- mockup-build/w02-data.json                        prompt minutes (P)
- research/data/prompts-founding-sessions-approved-2026-10-11.csv  the author's founding-session list
- research/data/prompts-raw-2026-10-11.jsonl        prompt times of those sessions (only session and ts are read)

The prompt minutes come from the author's private prompt history, which is not
persisted under research/, so they are read from the mockup's embedded data. The
origin prompts (hollow rings) are the prompts of the approved founding sessions
typed before the repository's first commit.
"""
import csv, datetime as dt, json, re
from pathlib import Path
from zoneinfo import ZoneInfo

import jargon

from .w06_moving_target import ADVANCES

SLUG = "agents-vs-human"
PT = ZoneInfo("America/Los_Angeles")
T0 = dt.datetime(2026, 6, 20, tzinfo=PT)
MODELS = ["Claude Opus 4.8", "Claude Fable 5", "Claude Opus 5", "Claude Opus 5.5", "Claude Sonnet 5.5", "none"]

# NO_BUN rewrites the few annotation and subject strings that name Bun's port:
# the article credits it once in prose, never inside a widget.
NO_BUN = [
    ("Bun learnings + realignment: ", "Realignment: "),
    ("Bun ideas, macOS app, perf", "New porting rules, macOS app, perf"),
    ("Bun ideas pivot", "Validation pivot"),
    ("Bun learnings committed", "Porting lessons committed"),
    ("Bun-rewrite learnings", "porting lessons"),
    ("(Bun lifetime-c", "(lifetime-c"),
]


# PLAIN rewrites internal process words in the authored era and event labels (not in
# commit subjects, which are quoted git history): a "resync" is the work of porting a
# new upstream range, a "pin" the Ghostty reference commit.
PLAIN = [
    (re.compile(r"\bUpstream resync\b"), "Upstream catch-up"),
    (re.compile(r"\boracle harness\b"), "reference-comparison harness"),
    (re.compile(r"\bfirst resyncs\b"), "first upstream catch-ups"),
    (re.compile(r"\blast resync\b"), "last upstream catch-up"),
    (re.compile(r"^Pin \+(\d+)$"), r"Upstream +\1"),
    (re.compile(r"^Upstream pin (\w+) → (\w+)"), r"Reference moved \1 → \2"),
]


# ERA_FIXES corrects catch-up era labels in the annotations that lack the reference commits they
# moved between (the numbers are recomputed by _number_catch_ups, so the ones here are ignored).
ERA_FIXES = {
    "Upstream resync 5 + memory-safety lanes (Miri, ASan/LSan/MSan/TSan)":
        "Upstream resync 6 (20abdb50a → 12752b2ac, 333 upstream commits) + memory-safety lanes"
        " (Miri, ASan/LSan/MSan/TSan)",
}
# MISSING_ERAS are catch-up eras the annotations leave out: the small fourth move of 25–26 August
# (683d8db64 → 88f57ee66, 4 upstream commits counting merges; timeline-upstream_pins.csv).
MISSING_ERAS = [
    ["resync", "2026-08-25", "2026-08-26", "Upstream resync 4 (683d8db64 → 88f57ee66, 4 upstream commits)"],
]
# EVENT_TEXT_FIXES rewrites event texts that use another word for a reference move.
EVENT_TEXT_FIXES = {
    "Drift absorb 683d8db64 → 88f57ee66": "Upstream pin 683d8db64 → 88f57ee66 (4 upstream commits)",
}
# CATCH_UP_RE reads a catch-up era label: its number and the reference commit it moved to.
CATCH_UP_RE = re.compile(r"^Upstream catch-up (\d+) \((\w+) → (\w+)\b")


def _number_catch_ups(eras: list) -> list:
    """Return the eras with every catch-up numbered by its reference move in the moving-target
    data (w06 ADVANCES), so the widget counts moves the way the prose and that widget do.
    Raises ValueError for a catch-up era that names no reference move."""
    targets = [a["to"] for a in ADVANCES]
    out = []
    for era in eras:
        grp, s, e, label = era
        if grp == "resync":
            m = CATCH_UP_RE.match(label)
            if not m or m.group(3) not in targets:
                raise ValueError(f"{SLUG}: catch-up era without a known reference move: {label!r}")
            n = targets.index(m.group(3)) + 1
            label = f"Upstream catch-up {n}" + label[len(f"Upstream catch-up {m.group(1)}"):]
        out.append([grp, s, e, label])
    return out


def _plain(text: str, slug_changes: list) -> str:
    """Return an authored label with the PLAIN rewrites applied, recording each change."""
    out = text
    for pat, rep in PLAIN:
        out = pat.sub(rep, out)
    if out != text:
        slug_changes.append((text, out))
    return out


def _clean(text: str, slug_changes: list) -> str:
    """Return text with internal ids and process labels stripped (jargon.strip_internal_ids)
    and Bun mentions rewritten, recording each change for the hand review."""
    original = text
    out = text
    for before, after in NO_BUN:
        out = out.replace(before, after)
    out, _ = jargon.strip_internal_ids(out)
    if out != original:
        slug_changes.append((original, out))
    if re.search(r"\bBun\b", out):
        raise ValueError(f"{SLUG}: unhandled Bun mention: {out!r}")
    return out


def _minutes(stamp: str) -> int:
    """Return minutes since 2026-06-20 00:00 PT for a 'YYYY-MM-DD HH:MM' Pacific timestamp."""
    t = dt.datetime.strptime(stamp, "%Y-%m-%d %H:%M").replace(tzinfo=PT)
    return int((t - T0).total_seconds() // 60)


def _mix(spec: str) -> list:
    """Return per-model commit counts from a 'model:count;…' string; joint trailers split evenly."""
    out = [0.0] * len(MODELS)
    for kv in filter(None, spec.split(";")):
        k, v = kv.rsplit(":", 1)
        parts = k.split("+")
        for part in parts:
            out[MODELS.index(part)] += int(v) / len(parts)
    return [round(x, 1) for x in out]


def _read_csv(path: Path) -> list:
    """Return the rows of a CSV file as dicts."""
    with open(path, newline="") as f:
        return list(csv.DictReader(f))


# FOUNDING is the author's approved founding-session list; FOUNDING_RAW holds those sessions' prompt
# times (the extract of the same date; only the session and ts fields are read).
FOUNDING = "prompts-founding-sessions-approved-2026-10-11.csv"
FOUNDING_RAW = "prompts-raw-2026-10-11.jsonl"


def _origin_minutes(d: Path, first_commit: int) -> list[int]:
    """Minutes (from T0) of the founding sessions' prompts typed before the first commit, sorted."""
    with open(d / FOUNDING, newline="", encoding="utf-8") as f:
        include = {r["session"] for r in csv.DictReader(f) if r["decision"] == "include"}
    out = []
    with open(d / FOUNDING_RAW, encoding="utf-8") as f:
        for line in f:
            r = json.loads(line)
            if r.get("session") not in include:
                continue
            t = dt.datetime.fromisoformat(r["ts"].replace("Z", "+00:00")).astimezone(PT)
            m = int((t - T0).total_seconds() // 60)
            if m < first_commit:
                out.append(m)
    return sorted(out)


def build(research: Path, mockbuild: Path) -> dict:
    """Return the agents-vs-human widget data (see module docstring for sources)."""
    d = Path(research) / "data"
    changes: list = []

    grid = _read_csv(d / "mock-agents-vs-human-grid.csv")
    g = {
        "commits": [int(r["commits_non_merge"]) for r in grid],
        "merges": [int(r["commits_all_refs"]) - int(r["commits_non_merge"]) for r in grid],
        "prompts": [int(r["prompts"]) for r in grid],
        "added": [int(r["lines_added"]) for r in grid],
        "deleted": [int(r["lines_deleted"]) for r in grid],
    }
    subjects = {str(i): [r["top_hash"], _clean(r["top_subject_scrubbed"], changes)]
                for i, r in enumerate(grid) if r["top_hash"]}

    lag = _read_csv(d / "mock-agents-vs-human-commit-lag.csv")
    commits = {
        "t": [_minutes(r["author_ts_pt"]) for r in lag],
        "lag": [-1 if r["lag_hours_since_last_rustty_prompt"] == ""
                else round(float(r["lag_hours_since_last_rustty_prompt"]) * 60) for r in lag],
    }

    mock = json.loads((Path(mockbuild) / "w02-data.json").read_text())
    origin = _origin_minutes(d, min(commits["t"]))

    daily_rows = _read_csv(d / "mock-agents-vs-human-daily.csv")
    daily = {
        "date": [r["date"] for r in daily_rows],
        "mix": [_mix(r["model_mix"]) for r in daily_rows],
        "costMacClaude": [float(r["cost_mac_claude"]) for r in daily_rows],
        "costVmClaude": [float(r["cost_rvm_claude"]) for r in daily_rows],
        "costCodex": [float(r["cost_mac_codex"]) for r in daily_rows],
        "lost": [int(r["mac_claude_transcripts_lost"]) for r in daily_rows],
        "threads": [int(r["agent_threads"]) for r in daily_rows],
        "otherPrompts": [int(r["other_project_prompts"] or 0) for r in daily_rows],
    }

    ann = json.loads((d / "mock-agents-vs-human-annotations.json").read_text())
    raw_eras = sorted([[g, s, e, ERA_FIXES.get(lb, lb)] for g, s, e, lb in ann["eras"]] + MISSING_ERAS,
                      key=lambda era: era[1])
    eras = _number_catch_ups([[grp, s, e, _plain(_clean(label, changes), changes)]
                              for grp, s, e, label in raw_eras])
    human_eras = [[s, e, _plain(_clean(label, changes), changes)] for s, e, label in ann["human_eras"]]
    events = [[day, time, kind, _plain(_clean(label, changes), changes),
               _plain(_clean(EVENT_TEXT_FIXES.get(text, text), changes), changes)]
              for day, time, kind, label, text in ann["events"]]
    quiet = [[s, e, _clean(text, changes)] for s, e, text in ann["quiet"]]

    summary = json.loads((d / "mock-agents-vs-human-summary.json").read_text())
    _record_review(changes)
    return {
        "_meta": {
            "slug": SLUG,
            "generatedFrom": [
                "research/data/mock-agents-vs-human-grid.csv",
                "research/data/mock-agents-vs-human-commit-lag.csv",
                "research/data/mock-agents-vs-human-daily.csv",
                "research/data/mock-agents-vs-human-annotations.json",
                "research/data/mock-agents-vs-human-summary.json",
                "mockup-build/w02-data.json (prompt minutes)",
                f"research/data/{FOUNDING}",
                f"research/data/{FOUNDING_RAW} (session and time only)",
            ],
            "asOf": "2026-10-08 14:00 PT",
        },
        "nCommits": summary["n_commits"],
        "grid": g,
        "subjects": subjects,
        "commits": commits,
        "prompts": mock["P"],
        "originPrompts": origin,
        "daily": daily,
        "models": MODELS,
        "eras": eras,
        "humanEras": human_eras,
        "events": events,
        "quiet": quiet,
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
