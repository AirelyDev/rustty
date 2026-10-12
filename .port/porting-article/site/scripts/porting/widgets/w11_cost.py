"""Data builder for the "What it cost" widget (slug cost).

Ports the embed step of the mockup build (mockup-build/embed.py, which trims
mock-cost-data.json for the page) onto the persisted research files:

- research/data/mock-cost-data.json  per quota week recovered $ and tokens by
  model or tool for both scopes, the modeled lost Mac Claude window re-binned
  into quota weeks (its totals are the fact-checked r2-cost-weekly.csv sums),
  trailer models per quota week, coverage per UTC day, eras, recovered-week
  units, token mix, call kinds, the Codex weekly meter and spend by model
  (made by mockup-build/cost_mock.py from rustty-stats messages.csv.gz,
  prompts.csv, r2-cost-weekly.csv and r2-cost-model-commits.csv)
- research/data/r2-human-story-limit-mentions.csv  the author's limit prompts
  and slash commands (Pacific time)
- research/data/r2-cost-limit-events.csv  Claude Code's own limit messages
- research/data/usage-stats-context-size-weekly.csv, usage-stats-activity-daily.csv
- research/data/r2-codex-audit-timeline.csv  the Codex audit events
- research/data/r2-cost-quota-weeks.csv  the measured ceilings of a maxed week
- research/data/r2-cost-summary.json  paid and API-equivalent figures

What changes on the way:

- prompts ship only the short verbatim fragment the mockup labels them with,
  never the whole prompt
- the first era is named without the other project the mockup referred to
- the Codex meter, its seven events and the note on the unexplained readings come from the
  shared codex_meter module (one treatment with the audit-funnel chart); the events get plain labels and texts (no
  paths, no planning labels)
- the paid range's top uses the fact-check's five billing cycles, not the
  summary file's four
"""
import csv
import json
from datetime import datetime, timedelta, timezone
from pathlib import Path

import jargon

from . import codex_meter

SLUG = "cost"

# UTC is the timezone every timestamp in the widget is shown in.
UTC = timezone.utc

# Q0 is the start of the first quota week (Tue 22:00 UTC, 3 pm Pacific).
Q0 = datetime(2026, 6, 16, 22, tzinfo=UTC)

# SHORT maps each author prompt (Pacific timestamp) to the short verbatim fragment shown for it
# (the mockup build's hand-picked fragments; typos are the author's own).
SHORT = {
    "2026-06-22 06:57": "API throttling",
    "2026-06-26 09:30": "approaching my token limit",
    "2026-06-30 18:38": "token limit is reset",
    "2026-07-01 05:26": "token limit is reset",
    "2026-07-04 08:41": "I am out of token usage",
    "2026-07-28 19:20": "I previously ran out of usage",
    "2026-08-10 19:13": "My Fable usage is about to expire",
    "2026-08-11 13:29": "we ran out of Fable usage for now",
    "2026-08-25 07:17": "I ran our of usage credits",
    "2026-08-25 12:15": "session limit has reset",
    "2026-08-25 12:16": "session limit has reset",
    "2026-08-25 14:55": "we're about to run out of usage quota",
    "2026-09-01 12:07": "my session limits expiring",
    "2026-09-07 11:24": "my session limits expiring",
    "2026-09-08 12:44": "my session limits expiring",
    "2026-09-15 15:14": "limit is reset",
    "2026-09-15 15:15": "limit is reset",
    "2026-09-24 08:33": "we're nearing limits",
    "2026-09-24 16:30": "we've hit usage limits",
    "2026-09-28 21:48": "approching usage limits",
    "2026-10-06 19:03": "we previously hit the session limit",
}

# SYSTEM names Claude Code's own limit message kinds.
SYSTEM = {
    "session": "Session limit",
    "weekly": "Weekly limit",
    "usage_credits_exhausted": "You're out of usage credits",
}

# ERA_NAMES are the plain names of the three eras, in order.
ERA_NAMES = [
    "Before the written working rules",
    "Rules, ledgers, VM and Codex",
    "Upstream catch-ups, orchestrator",
]

# CEIL_WEEKS are the measured maxed quota weeks used as ceilings: low, central, high.
CEIL_WEEKS = ["2026-09-29", "2026-09-08", "2026-09-22"]

# BILLING_CYCLES is the number of Max billing cycles the port touches (fact-check item 9: 05-28 to 09-28).
BILLING_CYCLES = 5

# CODEX_AUDIT holds the Codex audit totals (research BRIEF items 72 and 75, r2-cost fact-check item 11).
CODEX_AUDIT = {
    "calls": 28377,
    "threads": 902,
    "tokens": 2.97e9,
    "cacheRead": 2.82e9,
    "hours": 43,
    "otherUsd": 4.45,
}


def _qidx(dt: datetime) -> int:
    """Return the quota-week index of a UTC instant."""
    return int((dt - Q0).total_seconds() // (7 * 86400))


def _noon(day: str) -> datetime:
    """Return noon UTC of an ISO day (the instant used to file a day under a quota week)."""
    return datetime.fromisoformat(day).replace(tzinfo=UTC) + timedelta(hours=12)


def _pt2utc(s: str) -> str:
    """Convert a Pacific (PDT, UTC-7) 'YYYY-MM-DD HH:MM' stamp to 'YYYY-MM-DDTHH:MM' UTC."""
    return (datetime.strptime(s, "%Y-%m-%d %H:%M") + timedelta(hours=7)).strftime("%Y-%m-%dT%H:%M")


def _read_csv(path: Path) -> list:
    """Return the rows of a CSV file as dicts."""
    with open(path, newline="") as f:
        return list(csv.DictReader(f))


def _quota_weeks(d: dict) -> list:
    """Return the per-quota-week series (mockup embed qw): recovered and modeled, both scopes and units."""
    out = []
    for r in d["quota_weeks"]:
        days = [c for c in d["coverage"] if _qidx(_noon(c["d"])) == r["q"]]
        o = {
            "s": r["start"][:10], "lf": r["lost_frac"], "mc": r["models_commits"],
            "p": sum(c["p_mac"] + c["p_rvm"] for c in days), "po": sum(c["p_other"] for c in days),
            "c": sum(c["commits"] for c in days), "dl": [r["rustty_delegated"], r["all_delegated"]],
        }
        for sc in ("rustty", "all"):
            o[sc] = {
                "usd": [{k: round(v, 2) for k, v in r[f"{sc}_usd_rec"].items()}, r[f"{sc}_usd_ext"]],
                "tok": [{k: round(v) for k, v in r[f"{sc}_tok_rec"].items()}, [round(x) for x in r[f"{sc}_tok_ext"]]],
            }
        out.append(o)
    return out


def _coverage(d: dict) -> list:
    """Return one row per UTC day: [day, class, Rustty prompts, commits, recovered $, modeled $ central, other prompts]."""
    out = []
    for c in d["coverage"]:
        if c["mac_claude"] > 0 and (not c["lost"] or c["d"] == "2026-08-18" or c["d"] >= "2026-09-01"):
            cl = "rec"
        elif c["rvm"] or c["codex"]:
            cl = "vmcodex"
        elif c["p_mac"] or c["p_rvm"] or c["commits"]:
            cl = "prompt"
        else:
            cl = "none"
        if c["mac_claude"] > 0 and c["lost"]:
            cl = "remnant"
        out.append([c["d"], cl, c["p_mac"] + c["p_rvm"], c["commits"], c["usd"], c["ext_c"], c["p_other"]])
    return out


def _events(research: Path) -> list:
    """Return the limit lane: author prompts (short fragment), slash commands and Claude Code limit messages, by time."""
    data = research / "data"
    ev = []
    for r in _read_csv(data / "r2-human-story-limit-mentions.csv"):
        t, rustty = r["text"], int(r["project_is_rustty"])
        if t.startswith("/"):
            ev.append({"t": _pt2utc(r["ts_pt"]), "k": "slash", "r": rustty, "s": t})
            continue
        short = SHORT[r["ts_pt"]]
        if short not in t:
            raise ValueError(f"{SLUG}: fragment {short!r} not in the prompt of {r['ts_pt']}")
        ev.append({"t": _pt2utc(r["ts_pt"]), "k": "prompt", "r": rustty, "s": short})
    for r in _read_csv(data / "r2-cost-limit-events.csv"):
        if r["kind"] not in SYSTEM:
            continue
        name = SYSTEM[r["kind"]]
        full = f'{name} · "resets {r["resets"]}"' if r["resets"] else name
        if r["kind"] == "usage_credits_exhausted":
            full = "\"You're out of usage credits. Run /usage-credits to keep using Fable 5.1\""
        ev.append({
            "t": r["ts_utc"][:16], "k": r["kind"], "r": 1 if int(r["messages_in_rustty"]) > 0 else 0,
            "n": int(r["messages"]), "s": name + (f", resets {r['resets']}" if r["resets"] else ""), "f": full,
        })
    ev.sort(key=lambda e: e["t"])
    return ev


def _ceilings(research: Path) -> dict:
    """Return the measured maxed-week ceilings (all projects): $, billable tokens and output tokens, low / central / high."""
    rows = {r["quota_week_start_utc"][:10]: r for r in _read_csv(research / "data/r2-cost-quota-weeks.csv")}
    return {
        "usd": [float(rows[w]["mac_claude_cost_all_usd"]) for w in CEIL_WEEKS],
        "tok": [float(rows[w]["total_tokens_B"]) * 1e9 for w in CEIL_WEEKS],
        "out": [float(rows[w]["output_M"]) * 1e6 for w in CEIL_WEEKS],
    }


def build(research: Path, mockbuild: Path) -> dict:
    """Return the cost widget data (see module docstring for sources)."""
    research = Path(research)
    data = research / "data"
    d = json.loads((data / "mock-cost-data.json").read_text())
    summary = json.loads((data / "r2-cost-summary.json").read_text())
    sub = summary["subscription"]
    changes: list = []
    eras = [dict(e, name=name) for e, name in zip(d["eras"], ERA_NAMES, strict=True)]
    changes += [(e["name"], name) for e, name in zip(d["eras"], ERA_NAMES, strict=True) if e["name"] != name]
    ctx = [{"w": r["week"], "tool": r["tool"], "n": int(r["calls"]), "med": float(r["median_context_tokens"]),
            "p90": float(r["p90_context_tokens"]), "mx": float(r["max_context_tokens"])}
           for r in _read_csv(data / "usage-stats-context-size-weekly.csv")]
    act = [{"d": r["date"], "s": r["source"], "th": int(r["threads"]), "b": float(r["busy_hours_union"]),
            "a": float(r["agent_thread_hours"])}
           for r in _read_csv(data / "usage-stats-activity-daily.csv")]
    price = summary["prices_assumed"]
    out = {
        "_meta": {
            "slug": SLUG,
            "generatedFrom": [
                "research/data/mock-cost-data.json (rustty-stats messages.csv.gz, prompts.csv, r2-cost-weekly.csv, r2-cost-model-commits.csv)",
                "research/data/r2-human-story-limit-mentions.csv, r2-cost-limit-events.csv",
                "research/data/usage-stats-context-size-weekly.csv, usage-stats-activity-daily.csv",
                "research/data/r2-codex-audit-timeline.csv, r2-cost-quota-weeks.csv, r2-cost-summary.json",
            ],
            "asOf": "2026-10-08",
        },
        "qw": _quota_weeks(d),
        "cov": _coverage(d),
        "ev": _events(research),
        "mix": d["mix"],
        "kinds": d["kinds"],
        "ctx": ctx,
        "act": act,
        "meter": codex_meter.meter(research),
        "cev": codex_meter.events(research),
        "meterNote": codex_meter.NOTE,
        "eras": eras,
        "units": d["units"],
        "model_usd": d["model_usd"],
        "ceil": _ceilings(research),
        "paid": {
            "claude": sub["claude_max_rustty_50pct_usd"],
            "codex": sub["codex_pro_rustty_measured_share_usd"],
            "central": sub["rustty_total_central_usd"],
            "cycles": BILLING_CYCLES,
            "high": round(BILLING_CYCLES * price["claude_max20_month"] * 0.5 + price["codex_pro_month"] * 0.5),
            "months": summary["port_months"],
            "days": summary["port_days"],
            "price": price["claude_max20_month"],
            "codexShare": sub["codex_pro_month_rustty_share_by_api_usd"],
            "codexMonth": sub["codex_pro_month_api_usd"],
        },
        "api": {
            "recovered": summary["recovered_rustty_api_usd"],
            "modeled": [summary["extrapolated_lost_mac_claude_usd"][k] for k in ("low", "central", "high")],
            "total": [summary["api_equivalent_total_usd"][k] for k in ("low", "central", "high")],
        },
        "share": summary["measured_rustty_share_recovered_full_weeks"],
        "sensitivity": summary["sensitivity_prompt_share_central"],
        "codexAudit": CODEX_AUDIT,
    }
    _check_plain(out)
    _record_review(changes)
    return out


def _record_review(changes: list) -> None:
    """Append each (before, after) rewrite to jargon-review.txt once, for the hand review."""
    path = Path(__file__).resolve().parent.parent / "jargon-review.txt"
    existing = set(path.read_text().splitlines()) if path.exists() else set()
    lines = [f"{SLUG}\t{b}\t{a}" for b, a in dict.fromkeys(changes)]
    new = [ln for ln in lines if ln not in existing]
    if new:
        with open(path, "a") as f:
            f.write("".join(ln + "\n" for ln in new))


def _check_plain(out: dict) -> None:
    """Fail if any visible string carries an internal id or a planning label."""
    def walk(node):
        """Yield every string inside node."""
        if isinstance(node, str):
            yield node
        elif isinstance(node, dict):
            for v in node.values():
                yield from walk(v)
        elif isinstance(node, list):
            for v in node:
                yield from walk(v)

    for s in walk({k: v for k, v in out.items() if k != "_meta"}):
        if jargon.strip_internal_ids(s)[0] != s:
            raise ValueError(f"{SLUG}: internal id or planning label in {s!r}")
