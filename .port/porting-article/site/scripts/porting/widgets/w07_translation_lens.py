"""Data builder for the "Translation lens: Zig ↔ Rust" widget (slug translation-lens).

Ports the data step of the mockup build (mockup-build/w07/build.py, whose output
assemble.py inlines into the template) onto the persisted research files:

- research/data/mock-translation-lens.json  the six patterns: Zig excerpts (Ghostty at
  a60e9e2a5, libxev from its vendored package) and Rust excerpts (Rustty at 1d22ba80d)
  with real line numbers, the hand-drawn line mappings, the carried comments and Zig
  lines kept as Rust comments (exact text match), excerpt stats, git grep occurrence
  counts, pointer-lifetime ledger rows, Miri harness tables, perf fix chains and the
  cell bit layout; plus the ledger (314 rows), its 18 class totals, the module line
  counts, the unsafe census per crate and the same-line unsafe heuristic
- research/data/mock-translation-lens-counts.csv            cross-check of every count
- research/data/mock-translation-lens-unsafe-heuristic.csv  cross-check of the heuristic

What changes on the way:

- excerpt lines become [n, text] (or [n, text, 1] when the mockup removed an internal id
  from that comment line; the page marks it •); an omitted-lines gap becomes null
- ledger text: the mockup's id redaction left fragments such as "(, …FIXED)"; they are
  tidied (LEDGER_TIDY) and every rewrite goes to jargon-review.txt for the hand review
- harness folders (internal paths) become plain names (HARNESS_NAMES)
- the class panel needs only 8 sample rows per class, so the full ledger is not shipped
- unsafe census: rustty-vt corrected to the fact-checked 7,984 sites (one untracked build
  artifact was counted); only the crates the panel shows (20 or more sites, not -sys)
- mapping-review.txt lists every hand-drawn mapping for the author review
"""
import csv
import json
import re
from pathlib import Path

import jargon

SLUG = "translation-lens"

# HARNESS_NAMES maps each Miri harness folder of the mockup to the plain name shown.
HARNESS_NAMES = {
    ".port/phase-3/mirihar-scroll-region-up": "Scroll-region harness",
    ".port/phase-2/mirihar-xev-mpsc": "Event-loop queue harness",
    ".port/phase-2/mirihar-threadpool": "Thread-pool harness",
    ".port/phase-2/mirihar-n452": "C API re-entry harness",
}

# HELPER_CALL_SITES is the call-site count of the one container_of helper of the C API
# terminal (fromHandler in Zig, wrapper_from_handler in Rust): git grep of the call token
# minus the definition, 18 on each side (mockup spec, computed insight 2).
HELPER_CALL_SITES = 18

# UNSAFE_FIXES corrects per-crate unsafe-site counts to the fact-checked census
# (research/codebase.md fact-check, row 7): the census counted one untracked fuzz build
# artifact under crates/rustty-vt (7,984 sites, 0 unsafe extern).
UNSAFE_FIXES = {"rustty-vt": {"unsafe_sites": (7985, 7984), "unsafe_extern": (1, 0)}}

# LEDGER_TIDY rewrites the fragments the mockup's id redaction left in ledger text
# ("…" marks a removed id): leftover review labels, then empty or dangling punctuation.
LEDGER_TIDY = [
    (r"…widened by [Rr]uling \d+", ""),
    (r";?\s*retires LEDGER …", ""),
    (r"\b[Rr]uling \d+", ""),
    (r"\bfix round \d+", ""),
    (r"(?:,\s*…)*,?\s*…FIXE…$", "…"),
    (r"…(?:FIXED|CLOSED)", ""),
    (r"^FIXED \(", "("),
    (r"\(-\d+ prospective", "(prospective"),
    (r"\s*\(/ … pr…$", "…"),
    (r"generic per …,", "generic,"),
    (r", matching …\.", "."),
    (r"…\s*/\s*…", ""),
    (r"…(?=\s*[,;:)/])", ""),
]

# LEDGER_PUNCT tidies the punctuation the removals leave; applied until nothing changes.
# An empty "()" is removed only after a space, so code calls such as as_mut() stay.
LEDGER_PUNCT = [
    (r"\(\s*[,;/]\s*", "("),
    (r"\s*[,;/]\s*\)", ")"),
    (r"(?:^| )\(\s*\)", ""),
    (r"\s+([,;:)])", r"\1"),
    (r"^\s*[:,;]\s*", ""),
    (r"\s{2,}", " "),
]

# GENERATED_FROM documents the inputs in _meta.
GENERATED_FROM = [
    "research/data/mock-translation-lens.json (mockup-build/w07/build.py)",
    "research/data/mock-translation-lens-counts.csv (cross-check)",
    "research/data/mock-translation-lens-unsafe-heuristic.csv (cross-check)",
    "research/codebase.md fact-check row 7 (rustty-vt unsafe sites 7,984)",
]


def tidy_ledger(text: str) -> str:
    """Return ledger text with the mockup's redaction fragments tidied (LEDGER_TIDY, LEDGER_PUNCT)."""
    out = text
    for pat, rep in LEDGER_TIDY:
        out = re.sub(pat, rep, out)
    if out == text:
        return text
    prev = None
    while prev != out:
        prev = out
        for pat, rep in LEDGER_PUNCT:
            out = re.sub(pat, rep, out)
    return out.strip()


def clean(text: str, changes: list, ledger: bool = False) -> str:
    """Return text without internal ids, recording every rewrite.

    Code-like text keeps its brackets: ids and process labels are cut out token by
    token (jargon.find_internal_ids) instead of running the prose tidy.
    """
    out = tidy_ledger(text) if ledger else text
    for tok in jargon.find_internal_ids(out):
        out = out.replace(tok, "")
    out = re.sub(r"\s{2,}", " ", out) if out != text else out
    if out != text:
        changes.append((text, out))
    return out


def _excerpt(lines: list, changes: list) -> list:
    """Return mockup excerpt lines as [n, text] / [n, text, 1] (id removed) / None (gap)."""
    out = []
    for ln in lines:
        if ln.get("gap"):
            out.append(None)
            continue
        if jargon.find_internal_ids(ln["t"]):
            raise ValueError(f"{SLUG}: internal id left in code line {ln['n']}: {ln['t']!r}")
        out.append([ln["n"], ln["t"], 1] if ln.get("r") else [ln["n"], ln["t"]])
    return out


def _sub(s: dict) -> dict:
    """Return one excerpt pair (Zig and Rust panes, mappings, auto links and stats)."""
    return {
        "key": s["key"], "label": s["label"],
        "zigFile": s["zig_file"], "rustFile": s["rust_file"],
        "zig": _excerpt(s["zig"], []), "rust": _excerpt(s["rust"], []),
        "map": s["map"], "carried": s["carried"], "echoes": s["echoes"], "stats": s["stats"],
    }


def _ledger_row(r: list, changes: list) -> list:
    """Return a ledger row [file, struct, field, zig type, class, rust shape] with tidied text."""
    return [r[0], clean(r[1], changes, True), clean(r[2], changes, True), clean(r[3], changes, True),
            r[4], clean(r[5], changes, True)]


def _perf(steps: list, changes: list) -> list:
    """Return the perf fix chain with numbers parsed and empty hazards as None."""
    return [{"bench": r["bench"], "order": int(r["order"]), "fix": clean(r["fix"], changes),
             "hazard": None if r["hazard_subclass"] == "-" else clean(r["hazard_subclass"], changes),
             "metric": r["metric"], "value": float(r["value_after"]),
             "provenance": jargon.strip_internal_ids(r["provenance"])[0]} for r in steps]


def _counts(c: dict, changes: list) -> dict:
    """Return the occurrence counts with labels checked for ids (the per-file lossy split is not shown)."""
    items = []
    for it in c["items"]:
        it = {k: v for k, v in it.items() if k != "lossy_files"}
        items.append({**it, "label": clean(it["label"], changes)})
    return {"primary": {**c["primary"], "label": clean(c["primary"]["label"], changes)},
            "items": items, "crates": c["crates"]}


def _pattern(p: dict, changes: list) -> dict:
    """Return one pattern for the page (text cleaned, harness folders named, perf parsed)."""
    out = {
        "id": p["id"], "title": clean(p["title"], changes), "short": clean(p["short"], changes),
        "why": clean(p["why"], changes), "ledgerNote": clean(p["ledger_note"], changes),
        "ledgerCount": p["ledger_count"], "ledger": [_ledger_row(r, changes) for r in p["ledger"]],
        "harness": [{"name": HARNESS_NAMES[h["dir"]], "date": h["date"], "what": clean(h["what"], changes),
                     "rows": h["rows"]} for h in p["harness"]],
        "counts": _counts(p["counts"], changes),
        "subs": [_sub(s) for s in p["subs"]],
    }
    if "bits" in p:
        out["bits"] = p["bits"]
    if "perf_steps" in p:
        out["perf"] = _perf(p["perf_steps"], changes)
    return out


def _classes(classes: list, ledger_all: list, changes: list) -> list:
    """Return the 18 ledger classes with their row count and the first 8 rows of each."""
    out = []
    for c in classes:
        rows = [r for r in ledger_all if r[4] == c["class"]]
        if len(rows) != int(c["fields"]):
            raise ValueError(f"{SLUG}: class {c['class']} has {len(rows)} ledger rows, expected {c['fields']}")
        sample = [[r[0], *(_ledger_row(r, changes)[1:4]), _ledger_row(r, changes)[5]] for r in rows[:8]]
        out.append({"cls": c["class"], "n": int(c["fields"]), "sample": sample})
    return out


def _modules(mods: list) -> list:
    """Return module rows: name, Rust crates and [code, comments, tests] on each side (ratios are computed on the page)."""
    return [{"module": m["module"], "crates": m["rust_crates"].split("+"),
             "zig": [int(m["zig_code"]), int(m["zig_comments"]), int(m["zig_tests"])],
             "rust": [int(m["rust_code"]), int(m["rust_comments"]), int(m["rust_tests"])]} for m in mods]


def _unsafe(crates: list) -> tuple[list, int]:
    """Return the shown unsafe crates (20+ sites, not -sys) and the census total, fact-check applied."""
    fixed = []
    for c in crates:
        c = dict(c)
        for key, (was, now) in UNSAFE_FIXES.get(c["crate"], {}).items():
            if int(c[key]) != was:
                raise ValueError(f"{SLUG}: {c['crate']} {key} is {c[key]}, expected {was} before the fix")
            c[key] = str(now)
        if c["crate"] in UNSAFE_FIXES:
            c["sites_per_kloc_code"] = f"{int(c['unsafe_sites']) / int(c['rust_code_loc']) * 1000:.2f}"
        fixed.append(c)
    total = sum(int(c["unsafe_sites"]) for c in fixed)
    shown = [{"crate": c["crate"], "tier": c["tier"], "code": int(c["rust_code_loc"]),
              "sites": int(c["unsafe_sites"]), "nonTest": int(c["sites_in_non_test_code"]),
              "perKloc": float(c["sites_per_kloc_code"]), "ntPerKloc": float(c["non_test_sites_per_kloc_code"]),
              "safety": int(c["safety_comments"])}
             for c in fixed if int(c["unsafe_sites"]) >= 20 and c["tier"] != "sys"]
    return shown, total


def _check_counts(research: Path, patterns: list) -> None:
    """Raise unless every occurrence count equals mock-translation-lens-counts.csv, row for row."""
    with open(research / "data" / "mock-translation-lens-counts.csv", newline="") as f:
        want = [(r["pattern"], r["side"], int(r["count"])) for r in csv.DictReader(f)]
    got = [(p["id"], side, it[side]) for p in patterns
           for it in [p["counts"]["primary"]] + p["counts"]["items"] for side in ("zig", "rust", "ledger") if side in it]
    if got != want:
        raise ValueError(f"{SLUG}: occurrence counts differ from the counts CSV")


def _heuristic(research: Path, heur: dict) -> dict:
    """Return the same-line unsafe heuristic (total, [category, count]) after checking it against its CSV."""
    with open(research / "data" / "mock-translation-lens-unsafe-heuristic.csv", newline="") as f:
        want = [(r["category"], int(r["same_line_unsafe_blocks_non_test"])) for r in csv.DictReader(f)]
    cats = list(heur["cats"].items())
    if cats != want or sum(n for _k, n in cats) != heur["total"]:
        raise ValueError(f"{SLUG}: unsafe heuristic differs from its CSV")
    return {"total": heur["total"], "cats": [list(c) for c in cats]}


def write_mapping_review(patterns: list) -> None:
    """Rewrite mapping-review.txt: one line per hand-drawn mapping, for the author review."""
    def ranges(ns: list) -> str:
        """Return line numbers as compact ranges: 1752-1757, 1760."""
        out, start, prev = [], None, None
        for n in sorted(ns):
            if start is None:
                start = prev = n
            elif n == prev + 1:
                prev = n
            else:
                out.append(f"{start}" if start == prev else f"{start}-{prev}")
                start = prev = n
        out.append(f"{start}" if start == prev else f"{start}-{prev}")
        return ", ".join(out)

    lines = ["# Hand-drawn Zig → Rust line mappings of the translation-lens widget, for author review.",
             "# slug\tpattern/excerpt\tZig file: lines\tRust file: lines\tfirst Zig line → first Rust line"]
    for p in patterns:
        for s in p["subs"]:
            zt = {ln[0]: ln[1].strip() for ln in s["zig"] if ln}
            rt = {ln[0]: ln[1].strip() for ln in s["rust"] if ln}
            for z, r in s["map"]:
                lines.append(f"{SLUG}\t{p['id']}/{s['key']}\t{s['zigFile']}: {ranges(z)}\t{s['rustFile']}: {ranges(r)}"
                             f"\t{zt[min(z)]} → {rt[min(r)]}")
    (Path(__file__).resolve().parent.parent / "mapping-review.txt").write_text("\n".join(lines) + "\n")


def build(research: Path, mockbuild: Path) -> dict:
    """Return the translation-lens widget data (see module docstring for sources)."""
    research = Path(research)
    mock = json.loads((research / "data" / "mock-translation-lens.json").read_text())
    if len(mock["ledger_all"]) != 314:
        raise ValueError(f"{SLUG}: ledger has {len(mock['ledger_all'])} rows, expected 314")
    changes: list = []
    patterns = [_pattern(p, changes) for p in mock["patterns"]]
    _check_counts(research, patterns)
    shown, total = _unsafe(mock["unsafe_crates"])
    data = {
        "_meta": {"slug": SLUG, "generatedFrom": GENERATED_FROM,
                  "asOf": f"{mock['meta']['oracle']} · {mock['meta']['rustty_head']}"},
        "commits": {"zig": mock["meta"]["oracle"], "rust": mock["meta"]["rustty_head"]},
        "patterns": patterns,
        "ledgerClasses": _classes(mock["ledger_classes"], mock["ledger_all"], changes),
        "modules": _modules(mock["modules"]),
        "unsafeCrates": shown,
        "unsafeTotal": total,
        "unsafeHeur": _heuristic(research, mock["unsafe_heur"]),
        "helperCallSites": HELPER_CALL_SITES,
    }
    write_mapping_review(patterns)
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
