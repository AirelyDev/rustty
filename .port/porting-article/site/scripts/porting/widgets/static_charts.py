"""Data builders for the seven static charts of the porting article (one builder per slug).

Each builder reads the persisted research files named in its docstring and returns the JSON the
server-rendered chart draws. Visible strings are written here in plain words (no internal ids);
values that the fact-check corrected are applied here and say where the correction comes from.
Every string still passes through the shared id stripper; any rewrite goes to jargon-review.txt.
"""
import csv
from collections import defaultdict
from datetime import date, timedelta
from pathlib import Path

import jargon

from . import codex_meter
from .snapshot import RUSTTY_HEAD, SNAPSHOT, dated

# REVIEW_LOG is the hand-review file every stripped string is appended to.
REVIEW_LOG = Path(__file__).resolve().parent.parent / "jargon-review.txt"


def _rows(research: Path, name: str) -> list[dict]:
    """Read research/data/<name> as a list of dict rows."""
    with open(research / "data" / name, newline="") as f:
        return list(csv.DictReader(f))


def _clean(slug: str, text: str) -> str:
    """Strip internal ids from text, logging any change to jargon-review.txt."""
    clean, changed = jargon.strip_internal_ids(text)
    if changed:
        with open(REVIEW_LOG, "a") as f:
            f.write(f"{slug}\t{text}\t{clean}\n")
    return clean


def _meta(slug: str, sources: list[str], as_of: str = RUSTTY_HEAD) -> dict:
    """Return the _meta block for slug."""
    return {"slug": slug, "generatedFrom": sources, "asOf": as_of}


# ---------------------------------------------------------------- stats-card

# STATS_TILES are the stats-card tiles: every value, compare and extra is a fact id from
# facts.csv, so the card shows only fact-table numbers. Labels and notes carry no numbers; a note
# names a figure as a {fact_id} placeholder, which the card renders as that fact's linked display.
STATS_TILES = [
    dict(key="days", value="days_total", label="from the first commit to this snapshot",
         note="Agents worked on {commit_days} of them. Without usage limits, an estimated "
              "{days_no_limits_range} days."),
    dict(key="commits", value="commits_head", label="commits on the main line",
         extra="commits_nonmerge_head", extraLabel="excluding merges"),
    dict(key="lines", value="rust_code_lines", compare="zig_code_lines", compareLabel="lines of Zig",
         label="lines of Rust, from Zig",
         extra="rust_zig_ratio", extraLabel="Rust ÷ Zig code lines",
         note="Code lines counted by tokei: Ghostty at the reference commit, Rustty at head."),
    dict(key="tests", value="rust_tests", compare="zig_tests_pin", compareLabel="Zig test blocks in Ghostty's src/",
         label="Rust tests, all crates",
         note="Every Rust crate, including the ports of Ghostty's Zig dependencies and the -sys wrappers, "
              "against Ghostty's src/ alone at the current reference commit. The translation lens "
              "matches the two module by module."),
    dict(key="perf", value="perf_wins_pin", label="benchmarks won against Ghostty",
         extra="perf_suite_delta_pin", extraLabel="suite time vs Ghostty's release build",
         note="At the current reference commit: same machine, same session, Zig built with ReleaseFast."),
    dict(key="defects", value="defects_total", label="defects recorded in Ghostty and its dependencies",
         extra="defects_ghostty", extraLabel="in Ghostty itself",
         note="Found while porting. The live ones are fixed in Rustty rather than carried over; "
              "latent ones and weaknesses in Ghostty's tests are recorded."),
    dict(key="verify", value="miri_crates_pin", label="checked under Miri",
         extra="sanitizer_runs_latest", extraLabel="sanitizer and Valgrind test executions",
         note="Each lane at the latest reference commit it completed at: the macOS lanes and Valgrind at "
              "the current one, the Linux sanitizer lanes at the one before."),
    dict(key="cost", value="cost_paid", label="subscription paid",
         extra="cost_api_equiv", extraLabel="API-equivalent (reference)",
         note="Usage counted up to the as-of date shown on hover, two days before the snapshot. The API-equivalent figure is list price for the same usage, a reference value, not money spent."),
    dict(key="crates", value="crate_packages", label="Cargo packages",
         extra="scc_files", extraLabel="Ghostty files in one import cycle, cut apart",
         note="The workspace graph is acyclic."),
    dict(key="upstream", value="pin_advances", label="reference moves",
         extra="upstream_commits_absorbed", extraLabel="upstream non-merge commits absorbed"),
    dict(key="comments", value="comments_carried", label="Zig comments carried over",
         note="None missing by the comment-fidelity check."),
    dict(key="unsafe", value="unsafe_sites", label="unsafe sites",
         extra="unsafe_density", extraLabel="per thousand physical Rust lines"),
]


def build_stats_card(_research: Path, _mockbuild: Path) -> dict:
    """stats-card: the tile list; the component reads every value from facts.json by id."""
    slug = "stats-card"
    tiles = []
    for t in STATS_TILES:
        tile = {k: (_clean(slug, v) if k in ("label", "note", "extraLabel", "compareLabel") else v) for k, v in t.items()}
        tiles.append(tile)
    return {"_meta": _meta(slug, ["scripts/porting/facts.csv"], SNAPSHOT), "tiles": tiles}


# ---------------------------------------------------------------- strategy-timeline

# TIMELINE_LANES are the strategy-timeline rows, top to bottom.
TIMELINE_LANES = [
    dict(key="rules", label="Working rules"),
    dict(key="verify", label="Verification"),
    dict(key="ledger", label="Ledgers and audits"),
    dict(key="orch", label="Orchestration"),
    dict(key="platform", label="Upstream, platforms"),
]

# TIMELINE_EVENTS are the method changes of research/data/strategy-timeline.csv rewritten in plain
# words (no rule, check or detector ids), as (date, lane, short label, text, commit). Corrections
# from the strategy dossier's fact-check: the search aliasing fix landed the same day; upstream
# commits are quoted as 1,169 non-merge over eight moves (BRIEF row 22, refreshed 2026-10-10).
TIMELINE_EVENTS = [
    ("2026-06-21", "rules", "Definition of parity",
     "A written definition of parity with nine checks, and five rules: full fidelity, the rustty name, self-contained builds, an acyclic crate graph, and checks that are never weakened to pass.",
     "15c1a9bd7"),
    ("2026-06-21", "verify", "Miri first",
     "Miri adopted as the primary check for undefined behaviour and leaks; a custom tracking allocator was dropped.",
     "d9fd637ce"),
    ("2026-06-22", "orch", "Workflow scripts",
     "Orchestration through workflow scripts: design, adversarial review, port, verify, one script per milestone with structured outputs.",
     None),
    ("2026-06-23", "verify", "Sanitizer matrix",
     "A sanitizer check (ASan, LSan, MSan, TSan, HWASan), a Miri progress harness and a written route for tests Miri cannot run.",
     "161178d6a"),
    ("2026-06-24", "rules", "Raw-pointer arguments",
     "Zig pointer arguments whose targets are re-read during a call are ported as raw pointers, after Miri caught a Stacked Borrows violation in screen clearing.",
     "3ee9fd06d"),
    ("2026-07-02", "rules", "Placeholder leaves",
     "Placeholder types with unimplemented leaves let most of the surface and app files port as live Rust before the GPU, event loop and inspector existed.",
     "67579b58c"),
    ("2026-07-03", "orch", "Scripts kept in the repo",
     "Workflow scripts committed to the repository as executable provenance (61 on that day).",
     "f7411131f"),
    ("2026-07-07", "verify", "Miri proof harnesses",
     "Small isolated Miri harnesses prove aliasing questions instead of reasoning about them by hand; a Tree Borrows lane is added.",
     "f046d41a6"),
    ("2026-07-10", "rules", "Six new rules",
     "Six new working rules: explicit semantics for replaced Zig primitives, compile-time stays compile-time, unsafe accounting, greppable port markers, separate implementer and reviewer, and tracking of every upstream commit.",
     "ccd14200e"),
    ("2026-07-10", "ledger", "Pointer-lifetime ledger",
     "A retroactive ledger of who owns every pointer field (216 rows; its audit found 20 issues), and a checklist that requires ledger rows before any pointer-heavy part is ported.",
     "8f6d781dc"),
    ("2026-07-10", "ledger", "Unsafe census",
     "A committed script that counts every unsafe site, and a ledger of safe wrappers that are not yet sound.",
     "d8ee18a75"),
    ("2026-07-11", "verify", "First full engine Miri sweep",
     "Miri over all 2,037 terminal-engine tests found a 9-test aliasing bug in search; it was fixed the same day, about 7 hours later.",
     "3277f2f78"),
    ("2026-07-12", "verify", "Eight regression detectors",
     "Eight detectors for known classes of port regressions: byte-slice reinterpretation, fixed-buffer capacity, folded constants, compile-time phases and emitted bytes, cross-crate invariants, a sanitizer canary, injected copies and stale comments.",
     "94ca0243e"),
    ("2026-07-15", "orch", "Agent instructions",
     "A committed file of build, test and Miri commands for agents.",
     "648d1afeb"),
    ("2026-07-16", "orch", "Branches and merges",
     "Branch-and-merge integration starts; the history was linear before.",
     "d211a84a6"),
    ("2026-07-16", "platform", "Linux and a Linux VM",
     "The Linux port starts, with a Linux virtual machine for GTK and OpenGL.",
     "85d19023a"),
    ("2026-07-23", "orch", "Completion roadmap",
     "Twenty workstreams run by executors in isolated worktrees, each behind a review step.",
     None),
    ("2026-07-29", "platform", "Regular upstream catch-up",
     "Moving the Ghostty reference commit forward becomes a recurring step: 8 moves absorbed 1,169 upstream non-merge commits.",
     "3d79b3ba2"),
    ("2026-07-31", "verify", "False greens audited",
     "A check that passed on bookkeeping alone is found; struck rows no longer vote, and a staleness check guards the unsafe census.",
     "aeb05fbdc"),
    ("2026-08-11", "ledger", "Second opinion from Codex",
     "Codex's read-only audit (659 registry rows) is vetted by Claude: 144 findings confirmed, 3 refuted, 140 fixed and verified.",
     "8120dd10d"),
    ("2026-08-17", "platform", "Move first",
     "Move the reference first, record each check's state before and after, then port the difference.",
     None),
    ("2026-09-01", "orch", "Worktree subagents",
     "Hand-made worktrees and agent workers replace the workflow scripts: 1,142 subagents in 214 worktrees in September.",
     None),
    ("2026-09-08", "verify", "Parity closure starts",
     "Work starts on closing the 497 findings of a functional parity audit, each with a failing test first and a negative control.",
     None),
    ("2026-09-28", "verify", "Parity closure merged",
     "The parity closure is merged with one low-severity item open; the parity check is a hard check.",
     "079cc21e7"),
    ("2026-09-11", "rules", "Never port a bug",
     "A Ghostty defect reachable from real input is fixed, tested with a negative control and recorded instead of ported; the defect ledger starts with 66 entries.",
     "7dd991582"),
    ("2026-09-23", "rules", "No silent skips",
     "The never-port-a-bug rule is extended to non-crash bugs that break evident intent, and a test may no longer skip silently.",
     "f94ab8e81"),
    ("2026-09-23", "orch", "At most three building worktrees",
     "About seven parallel agents filled the disk, so at most three worktrees build at once and each deletes its build output after merging.",
     None),
    ("2026-09-24", "ledger", "Comments not carried",
     "Every reference comment not carried into Rust is recorded with its reason.",
     "f1c1639b0"),
    ("2026-09-27", "platform", "Windows VM",
     "A Windows 11 ARM64 virtual machine: the portable crates build and pass targeted tests on Windows.",
     "a8c4fea8f"),
    ("2026-10-01", "verify", "Memory-safety lanes redesigned",
     "Miri runs one process per test, plus Tree Borrows, LeakSanitizer on macOS and self-tests for every sanitizer.",
     "2d04310b0"),
    ("2026-10-07", "platform", "Seventh reference move",
     "The reference moves to a60e9e2a5, with an escape-sequence differential test against Ghostty.",
     "03709b897"),
    ("2026-10-09", "verify", "Full run on one machine",
     "The check script gains per-gate timings, a parallel scheduler with a cap on heavy checks, change-scoped long checks, sharded Valgrind and per-check build directories, so a full run fits on one machine.",
     "0182ccc35"),
    ("2026-10-09", "platform", "Eighth reference move",
     "Ghostty moved again during verification: the reference moves to 246f70287 (18 upstream commits, among them a soft terminal reset) and every reading check is green again 74 minutes later.",
     "2bf89b95a"),
    ("2026-10-10", "verify", "Final validation merged",
     "The upstream catch-up is merged after a full validation at the current reference commit on macOS and the Linux VM, with the benchmarks re-run.",
     "58aa62a81"),
]

# TIMELINE_TRIGGER is the single, plain mention of Bun in any chart: a small marker on 07-10.
TIMELINE_TRIGGER = dict(
    d="2026-07-10", label="trigger",
    text="Trigger: Bun's write-up of its own Zig-to-Rust port was published on 07-08; ideas from it were adopted here on 07-10.",
)

# TIMELINE_ERAS are the three method eras with their commit cadence (strategy dossier fact-check
# row 14: 452 commits in 19 days, 1,257 in 63; the last era re-counted to the 2026-10-10 snapshot:
# 1,376 commits in 30 days, refresh-2026-10-10.md era_r13_commits_per_day).
TIMELINE_ERAS = [
    dict(start="2026-06-21", end="2026-07-09", label="Founding rules", commits=452, days=19),
    dict(start="2026-07-10", end="2026-09-10", label="Rules tightened", commits=1257, days=63),
    dict(start="2026-09-11", end="2026-10-10", label="Never port a bug", commits=1376, days=30),
]


# CENSUS_TRACKED are the head points re-counted over tracked files only (codebase.md fact-check
# item 7: the census script also counted one untracked fuzz build artifact, 1 site / 5 lines).
CENSUS_TRACKED = {"1d22ba80d": (22310, 836905), "58aa62a81": (22240, 838127)}


def _census(research: Path) -> list[dict]:
    """Unsafe census points; the head points use the tracked-file counts of CENSUS_TRACKED."""
    out = []
    for r in _rows(research, dated("codebase-unsafe-census-timeseries.csv")):
        sites, lines = int(r["sites"]), int(r["total_lines"])
        if r["snapshot_commit"] in CENSUS_TRACKED:
            sites, lines = CENSUS_TRACKED[r["snapshot_commit"]]
        out.append(dict(d=r["date"], commit=r["snapshot_commit"], sites=sites, lines=lines,
                        density=round(sites / lines * 1000, 2)))
    return out


def build_strategy_timeline(research: Path, _mockbuild: Path) -> dict:
    """strategy-timeline: method changes by lane, eras, the ledger and registry step lines, census points."""
    slug = "strategy-timeline"
    events = [dict(d=d, lane=lane, label=_clean(slug, label), text=_clean(slug, text), commit=c)
              for d, lane, label, text, c in sorted(TIMELINE_EVENTS, key=lambda e: e[0])]
    ledger = [dict(d=r["datetime"][:10], t=r["datetime"], rows=int(r["lifetimes_data_rows"]), commit=r["commit"])
              for r in _rows(research, "r2-lifetimes-rows.csv")]
    registry = [dict(d=r["date"], n=int(r["registry_entries"]))
                for r in _rows(research, dated("strategy-gate-registry-growth.csv"))]
    eras = [dict(e, perDay=round(e["commits"] / e["days"], 1)) for e in TIMELINE_ERAS]
    return {
        "_meta": _meta(slug, [f"data/{dated('strategy-timeline.csv')}", "data/strategy-bun-adoptions.csv",
                              "data/r2-lifetimes-rows.csv", f"data/{dated('strategy-gate-registry-growth.csv')}"]),
        "range": ["2026-06-21", "2026-10-10"],
        "lanes": TIMELINE_LANES,
        "eras": eras,
        "events": events,
        "trigger": dict(TIMELINE_TRIGGER),
        "ledger": ledger,
        "registry": registry,
    }


# ---------------------------------------------------------------- checks-growth

# CHECK_GROUPS orders the registry's composition blocks: (family, tier, plain group label).
CHECK_GROUPS = [
    ("gate", "HARD", "Checks that must pass"),
    ("self-test", "HARD", "Self-tests that must pass"),
    ("detector", "HARD", "Detectors that must pass"),
    ("gate", "PROGRESS", "Checks tracked toward a target"),
    ("detector", "PROGRESS", "Detectors tracked toward a target"),
    ("abi", "PROGRESS", "C interface layout checks"),
    ("self-test", "PROGRESS", "Self-tests tracked toward a target"),
]

# CHECK_EXAMPLES are plain descriptions of a few checks per group, written from the glossary's
# plain-description column (research/r2-glossary.md); ids are never shown.
CHECK_EXAMPLES = {
    ("gate", "HARD"): [
        "The Cargo crate graph has no cycles.",
        "Byte-for-byte differential test of terminal output against Ghostty.",
        "AddressSanitizer, LeakSanitizer, MemorySanitizer, ThreadSanitizer and Valgrind lanes.",
        "Every parity finding is closed with a failing test first.",
    ],
    ("self-test", "HARD"): [
        "Each one injects a violation into its check and requires the check to fail.",
        "Proves every sanitizer catches a synthetic overflow, leak and race.",
        "Proves checks cannot be pointed at a different reference tree.",
    ],
    ("detector", "HARD"): [
        "No closed C enum is received by value across the C interface.",
        "A real C program passes an out-of-range enum value and gets a defined result.",
    ],
    ("gate", "PROGRESS"): [
        "Every Zig test block has a same-named Rust test.",
        "No ported test has fewer assertions than its Zig original.",
        "Every distinctive Ghostty comment has a match in the Rust code.",
        "Miri with Stacked Borrows and Tree Borrows over every pure-Rust crate.",
    ],
    ("detector", "PROGRESS"): [
        "Every byte-slice reinterpretation has a decision and a test for odd lengths.",
        "Every fixed buffer has a test that fills it to capacity.",
        "A sanitizer canary proves the lanes still see the port's allocations.",
    ],
    ("abi", "PROGRESS"): [
        "The embedding header is Ghostty's header with names renamed, nothing else.",
        "Every by-value C return type has the same size in Rust and C.",
        "Every C struct's size and field offsets match the Rust layout.",
    ],
    ("self-test", "PROGRESS"): [
        "Proves the C header layout check fails when a field is masked.",
    ],
}

# CAPI_PILLS are the C interface parity pills: (key, label, Rustty, Ghostty). Embedding uses 91,
# the codebase dossier's fact-check correction (the CSV's 90 is the check's floor literal).
CAPI_PILLS = [
    ("headers", "C headers", 35, 35),
    ("functions", "exported C functions", 208, 208),
    ("sentinels", "int-sized enum sentinels", 79, 79),
    ("embed", "embedding entry points", 91, 91),
]


def build_checks_growth(research: Path, _mockbuild: Path) -> dict:
    """checks-growth: registry and self-test step lines, the 101-entry composition, comment and C API pills."""
    slug = "checks-growth"
    registry = []
    for r in _rows(research, dated("strategy-gate-registry-growth.csv")):
        point = dict(d=r["date"], commit=r["commit"], n=int(r["registry_entries"]), self=int(r["self_test_entries"]))
        if registry and (registry[-1]["n"], registry[-1]["self"]) == (point["n"], point["self"]):
            continue
        registry.append(point)
    scripts = [dict(d=r["date"], total=int(r["gate_scripts"]), selfTest=int(r["with_self_test"]))
               for r in _rows(research, dated("strategy-gate-selftest-growth.csv"))]
    reg = _rows(research, dated("r2-gate-registry.csv"))
    counts: dict = defaultdict(int)
    for r in reg:
        counts[(r["family"], r["tier"])] += 1
    composition = [dict(family=fam, tier=tier.lower(), label=label, n=counts[(fam, tier)],
                        examples=[_clean(slug, e) for e in CHECK_EXAMPLES[(fam, tier)]])
                   for fam, tier, label in CHECK_GROUPS]
    crates = sorted(({"crate": r["crate"], "n": int(r["distinctive_ref_comments"]), "missing": int(r["missing"]),
                      "review": int(r["review"])}
                     for r in _rows(research, dated("codebase-comment-coverage-g3b.csv"))), key=lambda c: -c["n"])
    return {
        "_meta": _meta(slug, [f"data/{dated(n)}" for n in (
            "strategy-gate-registry-growth.csv", "strategy-gate-selftest-growth.csv", "r2-gate-registry.csv",
            "codebase-comment-coverage-g3b.csv")] + ["data/codebase-c-api.csv"]),
        "registry": registry,
        "scripts": scripts,
        "final": dict(entries=len(reg), hard=sum(r["tier"] == "HARD" for r in reg),
                      progress=sum(r["tier"] == "PROGRESS" for r in reg),
                      self=sum(r["is_self_test"] == "yes" for r in reg)),
        "composition": composition,
        "comments": dict(total=sum(c["n"] for c in crates), missing=sum(c["missing"] for c in crates),
                         notCarried=79, crates=crates),
        "capi": [dict(key=k, label=label, rust=a, ref=b) for k, label, a, b in CAPI_PILLS],
    }


# ---------------------------------------------------------------- audit-funnel

# FUNNEL_STAGES are the funnel stages: (stage_order in r2-codex-audit-funnel.csv, plain label, note).
FUNNEL_STAGES = [
    (7, "Registry rows Codex wrote", "Every row of the audit registry: findings, replays, equivalence results, rejections and duplicates."),
    (10, "Finding rows taken into vetting", "Rows that carry a severity: 190 registry rows plus the 20 findings of the first run, parsed to 179 rows and 19 folded duplicates."),
    (11, "Distinct findings", "Grouped by root cause."),
    (12, "Confirmed by Claude", "Checked against the code; Claude agreed with Codex's severity on 144 of 147. 3 were refuted."),
    (14, "Fixed and verified", "131 in the first fix wave and 9 platform fixes in the second; 3 deferred and 1 needed no change."),
]

def build_audit_funnel(research: Path, _mockbuild: Path) -> dict:
    """audit-funnel: five non-increasing stages, the severity split, and the shared Codex meter, events and note."""
    slug = "audit-funnel"
    by_order = {int(r["stage_order"]): r for r in _rows(research, "r2-codex-audit-funnel.csv")}
    stages = [dict(order=o, label=_clean(slug, label), n=int(by_order[o]["count"]), note=_clean(slug, note),
                   provenance=by_order[o]["provenance"].lower())
              for o, label, note in FUNNEL_STAGES]
    severity = {}
    for part in by_order[11]["detail"].split(","):
        sev, n = part.split()
        severity[sev] = int(n)
    return {
        "_meta": _meta(slug, ["data/r2-codex-audit-funnel.csv", "data/mock-cost-data.json (meter)",
                              "data/r2-codex-audit-timeline.csv"], "2026-08-11"),
        "stages": stages,
        "severity": severity,
        "meter": codex_meter.meter(research),
        "events": codex_meter.events(research),
        "meterNote": codex_meter.NOTE,
        "runs": codex_meter.RUNS,
        "remediation": dict(commit="8120dd10d", d="2026-08-11", files=532, tests=4660, provingTests=336),
    }


# ---------------------------------------------------------------- orchestration

# MODEL_COLUMNS maps r2-cost-model-weekly.csv columns to plain series names, in stacking order. The
# 2026-10-10 snapshot added "Claude Haiku 5.5" (two commits in the week of 10-05); every column of
# the file must be listed here (the builder raises on an unknown model column).
MODEL_COLUMNS = [
    ("Claude Opus 4.8", "Opus 4.8"),
    ("Claude Fable 5+Claude Opus 4.8", "Fable 5 with Opus 4.8"),
    ("Claude Fable 5", "Fable 5"),
    ("Claude Opus 5", "Opus 5"),
    ("Claude Opus 5.5", "Opus 5.5"),
    ("Claude Sonnet 5.5", "Sonnet 5.5"),
    ("Claude Haiku 5.5", "Haiku 5.5"),
    ("none", "no model trailer"),
]


def _weeks(first: str, last: str) -> list[str]:
    """Every Monday from first to last inclusive (ISO dates)."""
    d, end, out = date.fromisoformat(first), date.fromisoformat(last), []
    while d <= end:
        out.append(d.isoformat())
        d += timedelta(days=7)
    return out


def build_orchestration(research: Path, _mockbuild: Path) -> dict:
    """orchestration: commits per week by model, context per call by week, scripts and agent threads per day."""
    slug = "orchestration"
    raw = _rows(research, dated("r2-cost-model-weekly.csv"))
    unknown = set(raw[0]) - {col for col, _ in MODEL_COLUMNS} - {"week_monday", "total", "dominant_model"}
    if unknown:
        raise ValueError(f"orchestration: model columns {sorted(unknown)} are not in MODEL_COLUMNS")
    rows = {r["week_monday"]: r for r in raw}
    weeks = [dict(w=w, n=[int(rows[w][col]) if w in rows else 0 for col, _ in MODEL_COLUMNS])
             for w in _weeks(min(rows), max(rows))]
    ctx = [dict(w=r["week"], tool=r["tool"], calls=int(r["calls"]), median=round(float(r["median_context_tokens"])),
                p90=round(float(r["p90_context_tokens"])), max=round(float(r["max_context_tokens"])))
           for r in _rows(research, "usage-stats-context-size-weekly.csv")]
    scripts = [dict(d=r["first_commit_date"], n=int(r["scripts_added"]))
               for r in _rows(research, "strategy-workflow-scripts-by-date.csv") if r["first_commit_date"]]
    threads: dict = defaultdict(lambda: {"claude": 0, "codex": 0})
    for r in _rows(research, "usage-stats-activity-daily.csv"):
        threads[r["date"]][r["source"].split("/")[1]] += int(r["threads"])
    return {
        "_meta": _meta(slug, [f"data/{dated('r2-cost-model-weekly.csv')} (git, to {SNAPSHOT})",
                              "data/usage-stats-context-size-weekly.csv (transcripts, to 2026-10-08)",
                              "data/usage-stats-activity-daily.csv (transcripts, to 2026-10-08)",
                              "data/strategy-workflow-scripts-by-date.csv"],
                       SNAPSHOT),
        "range": ["2026-06-15", "2026-10-11"],
        "models": [name for _, name in MODEL_COLUMNS],
        "weeks": weeks,
        "ctx": ctx,
        "scripts": scripts,
        "threads": [dict(d=d, **n) for d, n in sorted(threads.items())],
        "totals": dict(scriptsCommitted=sum(s["n"] for s in scripts), subagents=1142, worktrees=214,
                       subShare=88.6, lostBefore="2026-09-08"),
    }


# ---------------------------------------------------------------- unsafe-density

# TOP_CRATES and TOP_AREAS are how many crates and engine areas get their own bar.
TOP_CRATES = 12
TOP_AREAS = 12


def build_unsafe_density(research: Path, _mockbuild: Path) -> dict:
    """unsafe-density: census density over time, per-crate sites (test vs non-test), engine areas."""
    slug = "unsafe-density"
    crates = []
    for r in _rows(research, dated("codebase-unsafe-per-crate.csv")):
        sites, lines, test = int(r["unsafe_sites"]), int(r["census_total_lines"]), int(r["sites_in_test_code"])
        if r["crate"] == "rustty-vt":
            # codebase.md fact-check item 7: the census also counted one untracked build artifact
            # (one unsafe extern) under rustty-vt (7,984 real sites at 1d22ba80d; 7,899 at 58aa62a81). Its line count
            # stays as the census recorded it (the artifact's own lines are not documented).
            sites -= 1
        crates.append(dict(crate=r["crate"], sites=sites, test=test, nonTest=sites - test, lines=lines,
                           codeLines=int(r["rust_code_loc"])))
    crates.sort(key=lambda c: -c["sites"])
    rest = crates[TOP_CRATES:]
    other = dict(crate=f"{len(rest)} other crates", sites=sum(c["sites"] for c in rest),
                 test=sum(c["test"] for c in rest), nonTest=sum(c["nonTest"] for c in rest),
                 lines=sum(c["lines"] for c in rest), codeLines=sum(c["codeLines"] for c in rest))
    areas = sorted(({"area": r["vt_area"].replace("_", " "), "codeLines": int(r["code_loc"]),
                     "nonTest": int(r["unsafe_non_test"]), "test": int(r["unsafe_test"])}
                    for r in _rows(research, dated("codebase-vt-unsafe-by-area.csv"))), key=lambda a: -a["nonTest"])
    arest = areas[TOP_AREAS:]
    aother = {"area": f"{len(arest)} other areas", "codeLines": sum(a["codeLines"] for a in arest),
              "nonTest": sum(a["nonTest"] for a in arest), "test": sum(a["test"] for a in arest)}
    series = _census(research)
    return {
        "_meta": _meta(slug, [f"data/{dated(n)}" for n in (
            "codebase-unsafe-census-timeseries.csv", "codebase-unsafe-per-crate.csv", "codebase-vt-unsafe-by-area.csv")]),
        "series": series,
        "physicalGrowth": round(series[-1]["lines"] / series[0]["lines"], 2),
        "testShare": dict(heuristic=58, strict=52),
        "crates": dict(rows=crates[:TOP_CRATES], other=other),
        "vtAreas": dict(rows=areas[:TOP_AREAS], other=aother),
        "basis": dict(
            series="unsafe sites per 1,000 physical lines of tracked Rust (vendored code excluded)",
            crates="unsafe sites per 1,000 physical lines of each crate",
            areas="non-test unsafe sites per 1,000 code lines of each terminal-engine area",
        ),
    }


# ---------------------------------------------------------------- platform-table

# PLATFORM_ROWS are the per-platform cells from research/r2-platforms.md (§2 table and fact-check),
# in plain words: each cell is (status, short text, detail). Lane names are described, not ids.
PLATFORM_ROWS = [
    dict(key="macos", platform="macOS", sub="arm64 · Swift app over the embedded C API · Metal",
         builds=("yes", "Workspace and app", "The Cargo workspace and the Xcode app build (BUILD SUCCEEDED)."),
         runs=("yes", "App runs", "First pixels 07-09, a runnable terminal 07-12."),
         hand=("yes", "Porting host", "The author ran the app directly; a menu-shortcut harness matches Ghostty on 51 actions, byte for byte."),
         lanes=("yes", "Full lanes", "At the current reference commit: Miri (Stacked Borrows on 38 crates, Tree Borrows including the app core), ASan 10,077 tests, LSan 6,643, TSan 3,900 (two test-only fixes, then clean scoped re-runs), 6,648 workspace tests, benchmarks 14 of 14 against Ghostty. At the previous one: 404 of 404 app tests."),
         last="2026-10-10",
         claim="The main platform: it builds, runs, and carries all the Miri evidence and most of the sanitizer evidence."),
    dict(key="linux", platform="Linux (GTK)", sub="aarch64 Fedora 44 VM · Wayland and X11 · OpenGL",
         builds=("yes", "Workspace, release, Flatpak", "Build and lints with the GTK, X11 and Wayland features; a release build; a Flatpak built, installed and run."),
         runs=("yes", "In a VM, software GL", "A live terminal in a GTK window since 07-18, rendered with software OpenGL (llvmpipe) only."),
         hand=("yes", "GNOME, Xfce, sway", "Tested by the author 07-18 to 07-23 on GNOME (Wayland), an Xfce X11 session and sway; later checks under nested KWin, headless mutter and the global-shortcuts portal."),
         lanes=("yes", "Tests and sanitizers", "Workspace tests with the GTK, X11 and Wayland features and 18 of 18 live GTK window tests; Valgrind 6,746 tests at the current reference commit; at the previous one ASan 10,307, LSan 10,307, MSan 6,375, HWASan 10,183, TSan 4,536 and LSan on a live GTK window."),
         last="2026-10-10",
         claim="The GTK app is ported and works in a Linux VM under Wayland and X11, tested by hand and by live-window tests; never run on a real GPU or on x86_64 Linux."),
    dict(key="windows", platform="Windows", sub="Windows 11 ARM64 VM · x86_64 MSVC under emulation, and GNU",
         builds=("yes", "Workspace (three C libraries excluded)", "Build and lints for MSVC, excluding libxml2, fontconfig and sentry; the C library in release; the font shaper for GNU."),
         runs=("partial", "Libraries only", "rustty.exe builds but opens no window: at this reference commit Ghostty's only Windows runtime is the embedded library, which refuses to start on Windows."),
         hand=("no", "No app to test", "A C example's output is byte-identical to the golden file; the console pseudo-terminal is driven in a unit test."),
         lanes=("yes", "Full workspace tests", "A full workspace test run for x86_64 MSVC, excluding libxml2, fontconfig and sentry: 6,418 passed, 0 failed at the current reference commit (one flaky test fixed after the previous one); the font shaper's 133 tests for GNU."),
         last="2026-10-10",
         claim="It builds, lints and passes the full workspace test suite on Windows; like Ghostty at this reference commit, there is no Windows app."),
    # The FreeBSD row is as of Rustty 110adad44 (after the article's snapshot), from
    # data/r2-platforms-matrix-2026-10-11b.csv; lastRef labels it in the "Latest evidence" cell.
    dict(key="freebsd", platform="FreeBSD", sub="FreeBSD 15.1 arm64 VM · GTK app, kqueue event loop",
         builds=("yes", "Native, with GTK", "Built natively on FreeBSD 15.1 (arm64, in a VM) with GTK 4.20, excluding the crash reporter library, which Ghostty also builds for macOS only. Lints show no FreeBSD-specific warning; the executable is not position-independent, as Ghostty's default (10-11)."),
         runs=("yes", "GTK app in a VM", "The app reports its GTK runtime, OpenGL renderer and kqueue event loop. A GTK window with a live shell ran under X11 and Wayland: typing, colours, CJK text, emoji, resizing, a new tab, and exit closing its tab (10-11). Before a fix in that run it crashed at its first window."),
         hand=("yes", "Author, X11 and Wayland", "The author used the app by hand on the VM on 10-11, as the committed run record states; the same checks were also driven by tools. Under Wayland a virtual keyboard garbles shifted keys exactly as in Ghostty built at the reference commit on that VM."),
         lanes=("yes", "Workspace and live GTK tests", "One native run, not a recurring lane. Workspace tests: 6,665 passed, 23 failed on the first run; every failure was fixed or explained and its test binary re-run green. GTK runtime tests: 163 passed, 0 failed. Live GTK window tests: 15 of 18; the 3 DMA-BUF tests need a GPU driver the VM lacks. No sanitizers (Rust supports them on FreeBSD only for x86_64) and no amd64 run."),
         last="2026-10-11", lastRef="110adad44",
         claim="Built, tested and run natively on FreeBSD in an arm64 VM: the GTK app runs under X11 and Wayland, tested by hand and by live-window tests; three DMA-BUF tests need a GPU driver the VM lacks, and it was never run on amd64."),
    # The WebAssembly row is as of Rustty 3a59e7edc (after the article's snapshot), from
    # data/r2-platforms-matrix-2026-10-11.csv; lastRef labels it in the "Latest evidence" cell.
    dict(key="wasm", platform="WebAssembly", sub="wasm32 · terminal core, font demo and event loop",
         builds=("yes", "Default build", "The freestanding WebAssembly build is part of the default check script (restored 10-07 after a regression). The event loop also builds for WASI, where its tests run (10-10)."),
         runs=("yes", "Browser, Node, wasmtime", "Three example pages ran in headless Chrome against Ghostty's own WebAssembly build: 139 outputs, all identical (10-10). The event loop's WASI backend ran under wasmtime (10-10). Earlier: headless SwiftShader and live Chrome on ANGLE Metal with byte-identical output (07-30); a logging site in Node (09-10)."),
         hand=("yes", "Author, in Chrome", "The author ran all five example pages by hand in Chrome on 10-10, at 3a59e7edc (the author's own statement; no run record is committed). Earlier: one live Chrome run (07-30). The automated example-page checks run in headless Chrome."),
         lanes=("yes", "Browser, Node and WASI", "The event loop's WASI backend runs its tests under wasmtime in the default check script: 34 passed, 2 ignored (WASI has no threads), 0 failed, with a check that every loop test ran and a negative control. A browser lane compares three example pages with Ghostty's and checks their constant tables against the module, each with negative controls; it found a bug in Ghostty's SGR example page, fixed in Rustty."),
         last="2026-10-10", lastRef="3a59e7edc",
         claim="The terminal core and the font demo compile to WebAssembly and run in a browser, in Node and under wasmtime, where the event loop's WASI backend passes its tests; three example pages give the same output as Ghostty's. There is no web terminal app, and Ghostty has none either."),
    dict(key="libvt", platform="libvt C API", sub="librustty-vt, the terminal library for C",
         builds=("yes", "macOS, Linux, Windows, wasm32", "Builds on all three desktop OSes (MSVC release on Windows) and for wasm32."),
         runs=("yes", "C examples run", "29 C examples on macOS and 25 on Linux build and run; on Windows one example's output is byte-identical."),
         hand=("na", "Not applicable", "A library: exercised by its example programs."),
         lanes=("yes", "Tests, Miri, ABI", "586 to 604 library tests per host, Miri on the C API crate, C consumer and header layout checks."),
         last="2026-10-08",
         claim="It works on all three desktop OSes and in WebAssembly, with example programs checked against goldens."),
]


def build_platform_table(_research: Path, _mockbuild: Path) -> dict:
    """platform-table: one row per platform with status, short text and detail for each column."""
    slug = "platform-table"
    rows = []
    for p in PLATFORM_ROWS:
        row = dict(key=p["key"], platform=p["platform"], sub=_clean(slug, p["sub"]), last=p["last"],
                   claim=_clean(slug, p["claim"]))
        if "lastRef" in p:
            row["lastRef"] = p["lastRef"]
        for col in ("builds", "runs", "hand", "lanes"):
            status, short, detail = p[col]
            row[col] = dict(status=status, text=_clean(slug, short), detail=_clean(slug, detail))
        rows.append(row)
    return {
        "_meta": _meta(slug, ["research/r2-platforms.md", "data/r2-platforms-matrix.csv",
                              "research/refresh-2026-10-10.md (lane cells refreshed)",
                              "data/r2-platforms-matrix-2026-10-11.csv (WebAssembly row, as of Rustty 3a59e7edc)",
                              "data/r2-platforms-matrix-2026-10-11b.csv (FreeBSD row, as of Rustty 110adad44)"],
                       SNAPSHOT),
        "columns": [dict(key="builds", label="Builds"), dict(key="runs", label="Runs"),
                    dict(key="hand", label="Tested by hand"), dict(key="lanes", label="Test and sanitizer lanes")],
        "rows": rows,
    }
