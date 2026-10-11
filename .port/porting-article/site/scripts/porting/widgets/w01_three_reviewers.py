"""Data builder for the "Three reviewers" widget (slug three-reviewers).

Ports the data step of the mockup build (mockup-build/w01/py/build.py, which inlines
the persisted model into the template) onto the persisted research files:

- research/data/mock-three-reviewers-bugs.json  the 38-event rail (81 defects, laned by
  the tool that found each event) and the four replayed bugs: life in git, implementer
  excerpts at the commit before the fix (B: the proof-harness model of the first pass,
  never committed), verbatim reviewer quotes, the Miri trace with hand-built borrow-stack
  steps for Stacked and Tree Borrows, Miri's output regenerated on 2026-10-08 against the
  pre-fix commits (or the in-repo harnesses), the fix diff with the verdict matrix and
  Miri-clean output, the rule that came out of it and the pattern counts
- research/data/mock-three-reviewers-pattern-counts.csv  cross-check of every count

What changes on the way:

- keys become camelCase for the page; the Tree Borrows commands are spelled out
  (cmdTb, cleanCmdTb) the way the mockup derives them, so the page never builds a command
- source paths of internal records (the repo's .port/ folder) become plain names
  (SOURCE_NAMES), and the proof-harness folder in file headers becomes "proof harness"
  (HARNESS_FILES); the visible headers then name the crate, test or harness only
- the two agent-memory rule quotes are author-approved (author confirmed 2026-10-08), so the "author to confirm" and
  glossary notes are dropped from their sources (SOURCE_NAMES)
- every string is checked for internal ids; each rewrite goes to jargon-review.txt
"""
import csv
import json
import re
from pathlib import Path

import jargon

SLUG = "three-reviewers"

# FIX_COMMITS are the four replayed bugs' fix commits, in the mockup's A–D order.
FIX_COMMITS = ["3662e56bf", "f046d41a6", "9e3fe1c25", "dcae8d61e"]

# TB_FLAGS is the environment prefix the Tree Borrows runs used (mockup spec, Miri errors row).
TB_FLAGS = "MIRIFLAGS=-Zmiri-tree-borrows "

# SOURCE_NAMES maps each quote or rule source that names an internal record path to the
# plain description shown.
SOURCE_NAMES = {
    "Miri (vt sweep, 3rd resync)": "Miri (terminal-engine sweep during the first upstream catch-up)",
    ".port/phase-2/wf-clearcells-sb-fix.js": "fix workflow script, verifier step",
    ".port/phase-2/GPU_RD_CHUNK3B_DESIGN.md §H.2": "cell-fill design document, §H.2",
    ".port/phase-2/GPU_RD_CHUNK3B_DESIGN.md §C.3(2)": "cell-fill design document, §C.3(2)",
    ".port/phase-2/VT_MIRI_SWEEP.md (commit 3277f2f78)": "terminal-engine Miri sweep record (commit 3277f2f78)",
    ".port/phase-2/VT_MIRI_SWEEP.md": "terminal-engine Miri sweep record",
    ".port/audits/LIFETIMES_AUDIT_2026-07-10.md (as filed, b4845f427)":
        "pointer-lifetime audit of 2026-07-10 (as filed, b4845f427)",
    "pointer-lifetime ledger, class definitions (.port/audits/GHOSTTY_LIFETIMES.tsv header)":
        "pointer-lifetime ledger, class definitions (ledger header)",
    "porting rules, adversarial-verification rule (.port/PORT_RULES.md)":
        "porting rules, adversarial-verification rule",
    "agent memory note on protected &mut arguments (not in the repo; quoted in the glossary, author to confirm)":
        "the port's agent memory note on protected `&mut` arguments",
    "agent memory note on the shaper-hook interleave (quoted in the glossary, author to confirm); "
    "the in-repo rules point to the same “isolated non-FFI Miri proof harness” doctrine":
        "the port's agent memory note on the shaper-hook interleave; the in-repo rules point to the "
        "same “isolated non-FFI Miri proof harness” doctrine",
}

# HARNESS_FILES maps proof-harness source paths (internal folders) to the header shown.
HARNESS_FILES = {
    ".port/phase-2/mirihar-chunk3b/src/main.rs": "proof harness · src/main.rs",
}

# GENERATED_FROM documents the inputs in _meta.
GENERATED_FROM = [
    "research/data/mock-three-reviewers-bugs.json (mockup-build/w01/py/build.py)",
    "research/data/mock-three-reviewers-pattern-counts.csv (cross-check)",
]


def clean(text: str, changes: list) -> str:
    """Return text with internal record paths named plainly and ids cut, recording every rewrite.

    Code-like text keeps its brackets: ids and process labels are cut out token by token
    (jargon.find_internal_ids) instead of running the prose tidy.
    """
    out = SOURCE_NAMES.get(text, HARNESS_FILES.get(text, text))
    for tok in jargon.find_internal_ids(out):
        out = out.replace(tok, "")
    if out != text:
        out = re.sub(r"\s{2,}", " ", out) if jargon.find_internal_ids(text) else out
        changes.append((text, out))
    if re.search(r"mirihar|\.port/", out):
        raise ValueError(f"{SLUG}: internal path left in {out!r}")
    return out


def _verbatim(text: str) -> str:
    """Return code or Miri output unchanged, raising if it carries an internal id or path."""
    if jargon.find_internal_ids(text) or re.search(r"mirihar|\.port/", text):
        raise ValueError(f"{SLUG}: internal id or path in verbatim text: {text!r}")
    return text


def _seg(seg: dict, changes: list) -> dict:
    """Return one code excerpt: header (file and function) and [line number, text] rows."""
    return {"file": clean(seg["file"], changes), "lines": [[n, _verbatim(t)] for n, t in seg["lines"]]}


def _step(st: dict, changes: list) -> dict:
    """Return one borrow-stack step: code lines executed, narration, access and stack items."""
    out = {"at": [[clean(f, changes), n] for f, n in st["at"]], "say": clean(st["say"], changes),
           "items": [_item(it, changes) for it in st["items"]]}
    if "acc" in st:
        out["acc"] = {"ok": bool(st["acc"]["ok"]), "text": clean(st["acc"]["text"], changes)}
    return out


def _item(it: dict, changes: list) -> dict:
    """Return one borrow-stack or borrow-tree item (tag, permission, creator, protector, state)."""
    out = {"id": it["id"], "perm": it["perm"], "label": clean(it["label"], changes)}
    if it.get("prot"):
        out["prot"] = True
    if "depth" in it:
        out["depth"] = it["depth"]
    if it.get("st"):
        out["st"] = it["st"]
    return out


def _miri(m: dict, changes: list) -> dict:
    """Return the Miri stage: commands, tracked location, code trace, SB/TB steps and output."""
    return {
        "cmd": m["cmd"], "cmdTb": m.get("cmd_tb") or TB_FLAGS + m["cmd"],
        "loc": clean(m["loc"], changes), "map": clean(m["map"], changes) if m.get("map") else None,
        "trace": [_seg(s, changes) for s in m["trace"]],
        "sb": [_step(s, changes) for s in m["sb"]], "tb": [_step(s, changes) for s in m["tb"]],
        "logSb": _verbatim(m["log_sb"]), "logTb": _verbatim(m["log_tb"]),
    }


def _fix(f: dict, changes: list) -> dict:
    """Return the fix stage: commit, diffstat, diff hunks or before/after, verdicts and clean runs."""
    ba = f.get("before_after")
    return {
        "commit": {**f["commit"], "subject": clean(f["commit"]["subject"], changes)},
        "stat": clean(f["stat"], changes),
        "diff": [[_verbatim(ln) for ln in h] for h in f["diff"]] if f.get("diff") else None,
        "beforeAfter": {"before": _seg(ba["before"], changes),
                        "after": [_seg(ba["after"], changes), _seg(ba["after2"], changes)]} if ba else None,
        "matrix": [[clean(m[0], changes), m[1], m[2]] for m in f["matrix"]],
        "cleanCmd": f["clean_cmd"], "cleanCmdTb": TB_FLAGS + f["clean_cmd"],
        "cleanSb": _verbatim(f["clean_sb"]), "cleanTb": _verbatim(f["clean_tb"]) if f.get("clean_tb") else None,
    }


def _bug(b: dict, changes: list) -> dict:
    """Return one replayed bug for the page (text cleaned, keys camelCase)."""
    imp = b["impl"]
    count = {"what": clean(b["count"]["what"], changes), "series": [[clean(lb, changes), n] for lb, n in b["count"]["series"]],
             "note": clean(b["count"]["note"], changes)}
    if "births" in b["count"]:
        count["births"] = b["count"]["births"]
    return {
        "key": b["key"], "ref": b["ref"], "title": clean(b["title"], changes), "short": clean(b["short"], changes),
        "crate": clean(b["crate"], changes), "scope": b["scope"], "models": b["models"],
        "caughtBy": clean(b["caught_by"], changes), "mechanism": clean(b["mechanism"], changes),
        "life": [{**ev, "what": clean(ev["what"], changes), "subject": clean(ev["subject"], changes)} for ev in b["life"]],
        "impl": {"commit": {**imp["commit"], "subject": clean(imp["commit"]["subject"], changes)} if imp.get("commit") else None,
                 "note": clean(imp["note"], changes), "segs": [_seg(s, changes) for s in imp["segs"]]},
        "review": {"kind": b["review"]["kind"], "head": clean(b["review"]["head"], changes),
                   "quotes": [{k: clean(q[k], changes) for k in ("who", "src", "text")} for q in b["review"]["quotes"]]},
        "miri": _miri(b["miri"], changes),
        "fix": _fix(b["fix"], changes),
        "rule": {"text": clean(b["rule"]["text"], changes), "src": clean(b["rule"]["src"], changes)},
        "count": count,
    }


def _check_counts(research: Path, bugs: list) -> None:
    """Raise unless every pattern count equals mock-three-reviewers-pattern-counts.csv, row for row."""
    with open(research / "data" / "mock-three-reviewers-pattern-counts.csv", newline="") as f:
        want = [(r["bug"], r["snapshot"], int(r["count"])) for r in csv.DictReader(f)]
    got = [(b["key"], lb, n) for b in bugs for lb, n in b["count"]["series"]]
    if got != want:
        raise ValueError(f"{SLUG}: pattern counts differ from the counts CSV")


# EXCLUDED_EVENTS are rail events left out by rulings R37/R39: test-isolation races and flakes are
# not memory-safety defects (the memory-safety widget and the verification matrix use the same
# definition). Key: (date, bug class) as in the mockup data; the 07 Oct UDP same-port test race.
EXCLUDED_EVENTS = {("2026-10-07", "port collision (SO_REUSEPORT)")}


def build(research: Path, mockbuild: Path) -> dict:
    """Return the three-reviewers widget data (see module docstring for sources)."""
    research = Path(research)
    mock = json.loads((research / "data" / "mock-three-reviewers-bugs.json").read_text())
    tl = mock["timeline"]
    if len(tl) != 38 or sum(e["n"] for e in tl) != 81:
        raise ValueError(f"{SLUG}: timeline has {len(tl)} events / {sum(e['n'] for e in tl)} defects, expected 38 / 81")
    tl = [e for e in tl if (e["d"], e["cls"]) not in EXCLUDED_EVENTS]
    if len(tl) != 38 - len(EXCLUDED_EVENTS):
        raise ValueError(f"{SLUG}: an excluded rail event was not found")
    if [b["ref"] for b in mock["bugs"]] != FIX_COMMITS:
        raise ValueError(f"{SLUG}: replayed bugs are {[b['ref'] for b in mock['bugs']]}, expected {FIX_COMMITS}")
    changes: list = []
    bugs = [_bug(b, changes) for b in mock["bugs"]]
    _check_counts(research, bugs)
    data = {
        "_meta": {"slug": SLUG, "generatedFrom": GENERATED_FROM, "asOf": mock["head"]},
        "head": mock["head"],
        "lanes": [{"key": k, "label": clean(lb, changes)} for k, lb in mock["lanes"]],
        "timeline": [{**e, "tool": clean(e["tool"], changes), "cls": clean(e["cls"], changes),
                      "area": clean(e["area"], changes), "sum": clean(e["sum"], changes)} for e in tl],
        "bugs": bugs,
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
