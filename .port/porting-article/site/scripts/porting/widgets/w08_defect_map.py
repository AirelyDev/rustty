"""Data builders for the "Defects on the map of Ghostty" widget (slugs defect-map, defect-map-details).

Ports the data step of the mockup build (mockup-build/mock/m08build.py, m08parse.py) onto the
persisted research files:

All three research files are read at the current snapshot (``snapshot.dated``).

- research/data/mock-08-defect-map.json  the mockup build's output: `tree` (Ghostty's Zig lines per
  file at 246f70287, `git grep -c ''`, grouped into 46 top tiles and 276 leaves) and `defs` (one row
  per ledger entry: title, class, method, severity, project, leaf, file, trimmed ledger texts,
  upstream status, recording commit, ReleaseFast consequence)
- research/data/bugs-upstream-defects.csv  class, project and upstream status per entry (cross-check)
- research/data/r2-bugs-upstream-method.csv  discovery method and hand-pass confidence per entry
- .port/parity-closure/UPSTREAM-DEFECTS.md  the ledger itself, re-read for the two entries that were
  private when the mockup was built (UD131, UD351): the author approved publishing them in full, so
  their file, mechanism, ReleaseFast and port-handling texts replace the mockup's category-only text

What changes on the way:

- entry ids are dropped from every visible field; each entry keeps only its ledger number `n` (not
  shown) as a key
- the long texts (mechanism, ReleaseFast, Rustty instead, upstream note) move to a second file,
  defect-map-details.json, that the widget imports on the first click (initial data ~70 KB)
- every string is checked with the shared id finder; the mockup's own scrub (internal ids and task
  labels replaced by plain words) is re-applied to the re-read private entries, and any rewrite is
  logged to jargon-review.txt
- discovery method and the ReleaseFast consequence stay ESTIMATED classifications (hand pass and
  keyword reading of the ledger's ReleaseFast field); the widget says so
"""
import csv
import json
import re
from pathlib import Path

import jargon

from .snapshot import RUSTTY_HEAD, dated

SLUG = "defect-map"
DETAILS_SLUG = "defect-map-details"

# CLASS maps the research CSV's `klass` value to the widget's class key.
CLASS = {"crash": "crash", "ub": "ub", "leak": "leak", "deadlock/hang": "hang",
         "intent-breaking": "intent", "latent": "latent", "test-weakness": "test"}

# STATUS maps the research CSV's upstream status to the widget's status key.
STATUS = {"not reported": "nr", "not reported (private disclosure pending)": "priv",
          "fixed upstream independently": "fixed", "partly fixed upstream": "partly",
          "obsolete upstream": "obsolete"}

# CONFIDENCE maps the hand pass's confidence to h (high), m (medium) or k (keyword label kept).
CONFIDENCE = {"high": "h", "medium": "m"}

# PRIVATE_US is the upstream-status sentence for the two formerly private entries, in plain words
# (the ledger's field is an instruction to the porting agents, in capitals).
PRIVATE_US = ("Not reported yet. It is to go to Ghostty privately, as a security advisory, because "
              "ordinary terminal output triggers it.")

# LEDGER_FIELDS are the ledger entry fields m08parse.py reads, lower-cased.
LEDGER_FIELDS = ("kind", "upstream project", "severity", "reference", "defect", "releasesafe",
                 "releasefast", "reachability", "port handling", "parity row(s)", "test",
                 "proposed fix", "upstream status")

# PATH_RE finds the first Ghostty path in a ledger `reference:` field (m08build.py's pattern).
PATH_RE = re.compile(r"((?:src|pkg|macos|include|test|example|dist|nix|flatpak|vendor|po)/[\w./+-]*"
                     r"|build\.zig|CMakeLists\.txt)")

# RF_OVERRIDE corrects the estimated ReleaseFast consequence of entries classified while their text
# was private: UD131's published ReleaseFast field says "the process still aborts … Not silent UB"
# (ruling R19), so it is a crash that still terminates, and the split becomes 37/13/10.
RF_OVERRIDE = {131: "term"}

_CACHE: dict = {}


def _parse_ledger(path: Path) -> dict:
    """Return {id: {title, field: text}} for every `### UD<n> — ` entry after `## Entries` (m08parse.py)."""
    txt = path.read_text()
    txt = txt[txt.index("## Entries"):]
    parts = re.split(r"\n### (UD\d+) — ", txt)[1:]
    out = {}
    for i in range(0, len(parts), 2):
        body = parts[i + 1]
        fields = {"title": body.split("\n", 1)[0].strip()}
        cur = None
        for line in body.split("\n")[1:]:
            m = re.match(r"- ([A-Za-z ()/-]+?):\s{1,}(.*)", line)
            if m and m.group(1).strip().lower() in LEDGER_FIELDS:
                cur = m.group(1).strip().lower()
                fields[cur] = m.group(2).strip()
                continue
            if line.startswith("## "):
                break
            if cur and line.strip():
                fields[cur] += "\n" + line.strip()
        out[parts[i]] = fields
    return out


def _trim(s: str, n: int) -> str:
    """Collapse whitespace and cut s to n characters at a sentence boundary (m08build.py's clean)."""
    if not s:
        return ""
    s = re.sub(r"\((?:Fix|Review) round[^)]*\)", "", s)
    s = re.sub(r"\s+", " ", s).strip()
    if len(s) <= n:
        return s
    cut = s[:n]
    k = max(cut.rfind(". "), cut.rfind("; "))
    return cut[:k + 1] if k > n * 0.5 else cut.rstrip() + "…"


def _scrub(x: str) -> str:
    """Replace internal ids and task labels with plain words (m08build.py's scrub, plus measurement labels)."""
    x = x.replace("`", "")
    x = re.sub(r"\bR13\b", "the never-port-a-bug rule", x)
    x = re.sub(r"\s*\([^()]*\b(?:UD\d+|N\d+|S\d+-F\d+|P\d-W\d+|T\d+|task \d+|R\d{1,2}|V\d{1,2}|U-\d+)\b[^()]*\)", "", x)
    x = re.sub(r"\bUD\d+(?:[-–/]UD\d+)*\b", "a related entry", x)
    x = re.sub(r"\bN\d{2,}\b", "a port finding", x)
    x = re.sub(r"\bS\d+-F\d+\b", "an audit finding", x)
    x = re.sub(r"\bR\d{1,2}\b", "a porting rule", x)
    x = re.sub(r"\bP\d-W\d+(?: T\d+\w*)?\b", "an upstream catch-up task", x)
    x = re.sub(r"\btask \d+\b", "a task", x)
    x = re.sub(r"PORT NOTE", "port note", x)
    return re.sub(r"\s{2,}", " ", x).strip()


def _private_texts(entry: dict) -> dict:
    """Return the file and trimmed texts of a formerly private ledger entry, as m08build.py trims public ones."""
    m = PATH_RE.search(entry.get("reference", ""))
    return {
        "f": m.group(1).rstrip(".:,`") if m else "",
        "mech": _scrub(_trim(entry.get("defect", ""), 330)),
        "rfast": _scrub(_trim(entry.get("releasefast", ""), 170)),
        "fix": _scrub(_trim(entry.get("port handling", ""), 170)),
        "us": PRIVATE_US,
    }


# TASK_LABELS rewrite the task labels the mockup scrub missed (and hex bytes the id finder mistakes
# for an ABI check id) as (pattern, replacement); applied only to strings the id finder flags.
TASK_LABELS = [
    (r"task T-C review, re-measured in T-C3", "re-measured"),
    (r"task T-C, ", ""),
    (r";\s*task [A-Za-z][\w-]*", ""),
    (r"\s+by task [A-Z]-\d+", ""),
    (r"Decided by task [\w-]+: ", "Decided: "),
    (r"\s*\(task [\w-]+, review \w+\)", ""),
    (r"\bED (A0|BF) (80|BF)\b", r"0xED 0x\1 0x\2"),
]


# PLAIN rewrites the porting-process labels the shared id finder does not know (task letters such as
# "task S-12", "controller" rulings, scrub leftovers such as "task the never-port-a-bug rule-X",
# records paths) as (pattern, replacement); applied to every string.
PLAIN = [
    (r"\bafter a pin advance\b", "after a reference move"),
    # "the pin" is the port's word for the Ghostty reference commit (Ghostty's Pin type stays).
    (r"\bat the pin\b", "at the reference commit"),
    (r"\ban ancestor of the pin\b", "an ancestor of the reference commit"),
    (r"\bThe pin requires\b", "The reference commit requires"),
    (r"\bGhostty's pin \(", "Ghostty's pinned version ("),
    (r"\s*\((?:task )?the never-port-a-bug rule-[A-Z]\w*\)", ""),
    (r"\s*\(task the never-port-a-bug rule-[A-Z]\w*; task the never-port-a-bug rule-[A-Z]\w*'s first version "
     r"degraded every dimension at once\)", ""),
    (r"\(task the never-port-a-bug rule-[A-Z]\w*, ", "("),
    (r"in task the never-port-a-bug rule-[A-Z]\w*", "later"),
    (r"\(the never-port-a-bug rule, task S-12; see the port handling \(task S-12\) line below\)\. Before S-12:",
     "(the never-port-a-bug rule). Before that fix:"),
    (r"[;,] ?controller ruling,? (?:in |after review )?(?:task )?[A-Z]-\d+\w*(?: F\d+)?", "; ruled in review"),
    (r"\(controller ruling, task [A-Z]-\d+\w*\)", "(ruled in review)"),
    (r"Ruled \(controller, review of task [A-Z]-\d+\w*, ", "Ruled (review, "),
    (r"\(found by task [A-Z]-\d+\w*; not in its assignment", "(found during a fix of a sibling entry"),
    (r"; the [A-Z]-\d+\w* tests compare", "; those tests compare"),
    (r"task [A-Z]-\d+\w* restored", "a later fix restored"),
    (r"\s*\(task [A-Z]-\d+\w*\)", ""),
    (r", task [A-Z]-\d+\w*\)", ")"),
    (r" is done: task [A-Z]-\d+\w*;", " is done;"),
    (r"the a port finding dead-site ratchet \(\.port/[^)]*\)", "a dead-site ratchet in the port's records"),
    (r"parity audit an audit finding", "parity audit"),
    (r"\s*\(PORT…$", "…"),
    (r"\*\*", ""),
    (r"a porting rule form 1", "the test-skip rule, form 1"),
    (r"\bthe a related entry\b", "a related entry's"),
    (r"adversarial reviewer of a task", "adversarial reviewer"),
    (r"\ba related entry's arithmetic", "a sibling entry's arithmetic"),
    (r"\b(MEASURED|UNVERIFIED|ORDINARY|NOT YET REPORTED|PER DIMENSION)\b", lambda m: m.group(1).lower()),
]


def _checked(text: str, changes: list) -> str:
    """Return text with process labels and internal ids rewritten in plain words (logged); fail if an id remains."""
    out = text
    for pat, rep in PLAIN:
        out = re.sub(pat, rep, out)
    if jargon.find_internal_ids(out):
        for pat, rep in TASK_LABELS:
            out = re.sub(pat, rep, out)
        out = _scrub(out)
    left = jargon.find_internal_ids(out)
    if left or re.search(r"\btask [A-Z]-\d|\.port/|controller", out):
        raise ValueError(f"{SLUG}: internal label {left} left in {out!r}")
    if out != text:
        changes.append((text, out))
    return out


def _assemble(research: Path) -> tuple:
    """Return (defs, details, tree, reported) built once per research directory."""
    key = str(research)
    if key in _CACHE:
        return _CACHE[key]
    data_dir = Path(research) / "data"
    mock = json.loads((data_dir / dated("mock-08-defect-map.json")).read_text())
    with open(data_dir / dated("bugs-upstream-defects.csv"), newline="") as f:
        rows = {r["id"]: r for r in csv.DictReader(f)}
    with open(data_dir / dated("r2-bugs-upstream-method.csv"), newline="") as f:
        meth = {r["id"]: r for r in csv.DictReader(f)}
    ledger = _parse_ledger(Path(research).parent.parent / "parity-closure/UPSTREAM-DEFECTS.md")
    changes: list = []
    defs, details = [], []
    for d in mock["defs"]:
        uid = d["id"]
        r = rows[uid]
        if (CLASS[r["klass"]], r["upstream_project"], STATUS[r["upstream_status"]]) != (d["c"], d["p"], d["s"]):
            raise ValueError(f"{SLUG}: mockup row disagrees with bugs-upstream-defects.csv for {uid}")
        texts = {k: d[k] for k in ("f", "mech", "rfast", "fix", "us")}
        if d["s"] == "priv":
            texts = _private_texts(ledger[uid])
        title = d["t"] if d["s"] != "priv" else _scrub(re.sub(r"\s+", " ", ledger[uid]["title"]))
        n = int(uid[2:])
        defs.append({
            "n": n,
            "t": _checked(title, changes),
            "c": d["c"],
            "m": d["m"],
            "md": _checked(d["md"], changes),
            "conf": CONFIDENCE.get(meth[uid]["confidence"], "k"),
            "sev": d["sev"],
            "p": d["p"],
            "leaf": d["leaf"],
            "f": texts["f"],
            "s": d["s"],
            "k": d["k"],
            "dt": d["dt"],
            "doc": d["doc"],
            "rf": RF_OVERRIDE.get(n, d["rf"]),
        })
        details.append([n] + [_checked(texts[k], changes) for k in ("mech", "rfast", "fix", "us")])
    reported = sum(1 for r in rows.values() if r["upstream_status"].startswith("reported"))
    _record_review(changes)
    _CACHE[key] = (defs, details, mock["tree"], reported)
    return _CACHE[key]


# GENERATED_FROM lists the inputs of both files, for _meta.
GENERATED_FROM = [
    f"research/data/{dated('mock-08-defect-map.json')} (Zig lines per file at 246f70287; ledger rows)",
    f"research/data/{dated('bugs-upstream-defects.csv')}",
    f"research/data/{dated('r2-bugs-upstream-method.csv')} (hand pass, estimated)",
    ".port/parity-closure/UPSTREAM-DEFECTS.md (the two formerly private entries)",
]


def build(research: Path, mockbuild: Path) -> dict:
    """Return the defect-map widget data: the treemap tree and one short row per defect."""
    defs, _details, tree, reported = _assemble(research)
    return {
        "_meta": {"slug": SLUG, "generatedFrom": GENERATED_FROM, "asOf": RUSTTY_HEAD},
        "tree": tree,
        "defs": defs,
        "reported": reported,
    }


def build_details(research: Path, mockbuild: Path) -> dict:
    """Return the lazily loaded detail texts: [n, mechanism, ReleaseFast, Rustty instead, upstream] per defect."""
    _defs, details, _tree, _reported = _assemble(research)
    return {
        "_meta": {"slug": DETAILS_SLUG, "generatedFrom": GENERATED_FROM, "asOf": RUSTTY_HEAD},
        "d": details,
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
