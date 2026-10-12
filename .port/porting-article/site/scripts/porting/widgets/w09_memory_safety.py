"""Data builder for the "Memory-safety bugs" widget (slug memory-safety).

Ports the data step of the mockup build (mockup-build/mock/build09.py) onto the
persisted research file, read at the current snapshot (``snapshot.dated``):

- research/data/mock-09-memory-safety-events.csv  the 41 hand-curated port memory-safety
  events (date, reference, detecting tool, bug class, scope, crate area, defect count,
  summary) with the lane / class / crate grouping columns (rule-based from the tool,
  bug class and crate area, with four manual corrections). Source of the curated rows:
  research/data/bugs-port-memory-safety-timeline.csv (41 events, 85 defects; with the
  two test-isolation races left out (EXCLUDED_REFS, rulings R37/R39) the widget shows 39 events, 83 defects,
  64 found by Miri, 29 production events; the 2026-10-08 snapshot had 38 / 81 / 62 / 29).

What changes on the way:

- the mockup's substitutions (SUBSTITUTIONS) replace internal gate and finding names with
  plain descriptions; the shared id finder then checks every string (the stripper's punctuation
  tidy would also drop the `()` of code such as `join()`, so it is used only to detect)
- a reference that is a commit hash is kept as `ref`; a ledger reference (finding ids such
  as "N437/N440") becomes only the number of entries (`entries`), so no finding id is
  shipped
- the eight rule flags are the mockup's hand-written rules, each citing the commit that
  introduced the rule; the sentence that compared with another project's post is dropped
- each rewrite goes to jargon-review.txt
"""
import csv
import re
from pathlib import Path

import jargon

from .snapshot import dated

SLUG = "memory-safety"

# SUBSTITUTIONS are the mockup build's plain-word replacements for internal gate, harness
# and finding names, applied before the shared jargon stripper.
SUBSTITUTIONS = [
    (r"\bgate A4 added\b", "an ABI size gate was added"),
    (r"\s*\(M1\+M2\)", ""),
    (r"\(mirihar-chunk3b\)", "(an isolated harness)"),
    (r"Miri harnesses \(mirihar-[^)]*\)", "Miri harnesses (isolated models)"),
    (r"Miri harness \(mirihar-[^)]*\)", "Miri harness (isolated model)"),
    (r"Miri G6t", "Miri Tree Borrows lane"),
    (r"Miri G6\b", "Miri lane"),
    (r"widened G6 lane", "widened Miri lane"),
    (r"\(G-TSAN on the Linux VM\)", "(the Linux VM lane)"),
    (r"G-LSAN-GTK", "LeakSanitizer GTK lane"),
    (r"G-LSAN", "LeakSanitizer lane"),
    (r"A5 header gate", "C header enum check"),
    (r"P3 vt sweep", "terminal-engine sweep during the first upstream catch-up"),
    (r"\bF1: ", ""),
    (r"; F2: ", "; "),
    (r"LIFETIMES audit", "pointer-lifetime ledger audit"),
]

# RULES are the eight working rules the bug waves produced, in date order, each with the
# commit that introduced the rule (mockup spec, Rule flags row; counts from the brief).
RULES = [
    ("2026-06-24", "3662e56bf",
     "Port Zig's *Page, *Row and []Cell arguments as raw pointers: a Rust &mut argument is protected for the whole call, Zig's pointers are not."),
    ("2026-07-01", "6cf99b4cd",
     "Guard non-thread-safe C library initialisation with OnceLock: parallel cargo test runs what zig test ran serially."),
    ("2026-07-07", "f046d41a6",
     "Prove every unusual aliasing shape on a small isolated Miri harness instead of arguing it by hand (33 harness crates by October)."),
    ("2026-07-10", "8f6d781dc",
     "A pointer-lifetime ledger for every raw-pointer field (216 at creation, 314 by October) and an unsafe census."),
    ("2026-07-11", "90fa4ff71",
     "A test may skip Miri only under four written criteria; the first full sweep routed 74 such tests and found one real UB class."),
    ("2026-07-15", "40727a2cb",
     "Probe the size and field offsets of every by-value C-ABI struct against the header after a 20-byte vs 12-byte struct crashed the menu code."),
    ("2026-07-31", "6a07ed468",
     "Re-audit every Miri exception family adversarially and make the ASan control real; a misdiagnosed ASan crash had hidden a module."),
    ("2026-10-02", "5e8cbd918",
     "Miri on nextest (6.9× faster) plus a Tree Borrows lane, alongside the ASan, LSan, MSan, HWASan and TSan matrix."),
]


def _clean(text: str, changes: list) -> str:
    """Return text with the mockup substitutions applied, recording each change; fail if an internal id remains."""
    out = text
    for pat, rep in SUBSTITUTIONS:
        out = re.sub(pat, rep, out)
    left = jargon.find_internal_ids(out)
    if left:
        raise ValueError(f"{SLUG}: internal ids {left} left in {out!r}")
    if out != text:
        changes.append((text, out))
    return out


# EXCLUDED_REFS are dataset rows left out by ruling R37: test-isolation races and flakes are not
# memory-safety defects (the same definition the verification matrix's bug counts use). The
# rows are the 2026-10-07 UDP same-port test race (N515, R39) and the 2026-10-09 Windows log-sink
# race (fix 490a0e122, R37).
EXCLUDED_REFS = {"N515", "N541 (490a0e122)"}


def build(research: Path, mockbuild: Path) -> dict:
    """Return the memory-safety widget data (see module docstring for sources)."""
    path = Path(research) / "data" / dated("mock-09-memory-safety-events.csv")
    changes: list = []
    with open(path, newline="") as f:
        rows = list(csv.DictReader(f))
    events = []
    rows = [r for r in rows if r["ref"] not in EXCLUDED_REFS]
    for r in rows:
        # A reference is a commit hash, a ledger reference ("N437/N440"), or both ("N540 (adc5d6c12)").
        m = re.fullmatch(r"[0-9a-f]{7,}", r["ref"]) or re.search(r"\(([0-9a-f]{7,})\)", r["ref"])
        commit = m.group(m.lastindex or 0) if m else None
        events.append({
            "d": r["date"],
            "ref": commit,
            "entries": None if commit else len(r["ref"].split("/")),
            "tool": _clean(r["tool"], changes),
            "bc": _clean(r["bug_class"], changes),
            "scope": r["scope"],
            "area": r["crate_area"],
            "n": int(r["count"]),
            "sum": _clean(r["summary"], changes),
            "lane": r["lane"],
            "cls": r["cls"],
            "crate": r["crate"],
        })
    _record_review(changes)
    return {
        "_meta": {
            "slug": SLUG,
            "generatedFrom": [
                f"research/data/{dated('mock-09-memory-safety-events.csv')}",
                f"research/data/{dated('bugs-port-memory-safety-timeline.csv')} (the 41 curated rows, 39 shown: EXCLUDED_REFS)",
                "git history (the commit that introduced each rule)",
            ],
            "asOf": max(e["d"] for e in events),
        },
        "events": events,
        "rules": [{"d": d, "ref": ref, "t": t} for d, ref, t in RULES],
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
