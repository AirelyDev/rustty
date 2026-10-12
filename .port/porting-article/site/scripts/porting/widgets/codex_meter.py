"""The one Codex weekly-meter treatment shared by the cost widget and the audit-funnel chart.

Both widgets draw the same hourly series (median reading per UTC hour with its min-max band and
the cumulative list-price usage), the same seven audit events with the same run names ("run 1",
"run 2"), and the same note on the unexplained readings of 08-01 00:00-03:59.

Sources:
- research/data/mock-cost-data.json  ``meter``: per UTC hour the median, min and max
  ``rate_limit_used_pct`` and the cumulative Rustty Codex list-price usage
- research/data/r2-codex-audit-timeline.csv  the audit events (UTC minute)
- research/r2-codex-audit.md fact-check items 5 and 7: the 100th round's first spawn is at
  03:36 UTC (the timeline's 03:00 is the hour), and run 1 went 0 -> 6% while run 2 began at 8%
"""
import csv
import json
from pathlib import Path

# EVENTS are the seven audit events both meters mark: (UTC minute, label, plain tooltip text).
EVENTS = [
    ("2026-07-31T18:51", "run 1 starts",
     "Codex run 1 starts (gpt-5.6-sol, high effort): an adversarial parity audit of the non-macOS code, writing only to its own notes."),
    ("2026-08-01T06:03", "run 1 ends · 20 gaps",
     "Run 1 ends with 20 verified gaps and a design for auditing every line."),
    ("2026-08-01T06:43", "run 2 starts",
     "Run 2 starts at 8% of the weekly limit; its main thread runs gpt-5.5 at high effort and spawns the first independent reviewers."),
    ("2026-08-01T14:56", "“use medium effort”",
     "The author: “use medium effort from now on”."),
    ("2026-08-02T03:36", "100th round",
     "The audit reaches its 100th round of reviewers."),
    ("2026-08-02T06:55", "659-row registry",
     "The findings registry is first reported at 659 rows."),
    ("2026-08-02T14:05", "last call · 99%",
     "Last Codex call, at 99% of the weekly limit; Codex's own strict line ledger credits 10,929 lines (0.35%)."),
]

# TIMELINE_HOUR_ONLY maps an event minute to the timeline row it refines (fact-check item 5).
TIMELINE_HOUR_ONLY = {"2026-08-02T03:36": "2026-08-02T03:00"}

# NOTE is the annotation on the unexplained readings: the hours it covers, the hour of the drop,
# two short label lines drawn on the chart, and the full text for tooltips and screen readers.
NOTE = {
    "from": "2026-08-01T00",
    "to": "2026-08-01T03",
    "drop": "2026-08-01T04",
    "lines": ["readings to 16%, then 1–6%", "run 1's log ends at 6%"],
    "text": "From 00:00 to 03:59 UTC on 08-01 the hourly readings rise to 16%, then read 1–6% from 04:00. "
            "Run 1's own log goes from 0% to 6%, and run 2 began at 8%, so the higher readings are "
            "unexplained; the logs do not record why.",
}

# RUNS are the two audit runs as UTC minute spans, with the shared run names.
RUNS = [
    {"start": "2026-07-31T18:51", "end": "2026-08-01T06:03", "label": "run 1"},
    {"start": "2026-08-01T06:43", "end": "2026-08-02T14:05", "label": "run 2"},
]


def meter(research: Path) -> list:
    """Return the hourly meter as [hour, median %, min %, max %, cumulative Rustty $] rows."""
    d = json.loads((Path(research) / "data/mock-cost-data.json").read_text())
    return [[m["h"], m["med"], m["min"], m["max"], m["cum"]] for m in d["meter"]]


def events(research: Path) -> list:
    """Return the events as {t, label, text}, each checked against the audit timeline."""
    with open(Path(research) / "data/r2-codex-audit-timeline.csv", newline="") as f:
        rows = {r["utc"][:16] for r in csv.DictReader(f) if r["utc"].endswith("Z")}
    out = []
    for t, label, text in EVENTS:
        if TIMELINE_HOUR_ONLY.get(t, t) not in rows:
            raise ValueError(f"Codex event {t} missing from the audit timeline")
        out.append({"t": t, "label": label, "text": text})
    return out
