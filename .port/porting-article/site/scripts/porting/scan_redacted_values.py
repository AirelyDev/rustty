"""Count the author-approved redacted values in a git history, without printing them.

Reads the "redact" rows of the untracked approved-redactions CSVs (the only files that hold the
values; every extract's review, 2026-10-10, 2026-10-11 and 2026-10-11b to 2026-10-11e by default) and, for the given revisions
of a repository, reports per CSV file and line number:
- hits: whole-word occurrences in `git log -p` over the revisions (patches and commit messages;
  JSON files are scanned decoded, so a value right after an escape such as \\n still counts),
- substr: plain case-insensitive substring occurrences over the raw patches (every reason but
  person), a safety net for boundary misses; it may over-count a short value inside longer words,
- commits: commits whose patch or message contains the value,
- files@<rev>: files containing it in the tree of each tip revision named (not ranges).
Matching is case-insensitive, whole-word (a value is not counted inside a longer word, so a short
project name does not match unrelated words), plain and JSON-escaped. Values never reach the output.
A person-reason value (the author's username, which legitimately appears all over the repository
history in paths and trailers) is counted only in the prompt-log dataset files (DATASET_GLOBS);
every other reason is counted everywhere.

Usage (before any push of Rustty):
  python3 -I scripts/porting/scan_redacted_values.py --repo ~/projects/rustty main
  python3 -I scripts/porting/scan_redacted_values.py --repo ~/projects/rustty main..journey-article-research
Exit status is 1 when any value is found, else 0.
"""

import argparse
import csv
import fnmatch
import json
import re
import subprocess
import sys
from pathlib import Path

# APPROVED are the default approved-redactions CSVs (untracked research data; hold the values), one
# per extract; a value approved in either is scanned.
_DATA = Path.home() / "projects/rustty/.port/journey-article/research/data"
REVIEW_TAGS = ("2026-10-10", "2026-10-11", "2026-10-11b", "2026-10-11c", "2026-10-11d", "2026-10-11e")
APPROVED = [_DATA / f"prompts-redactions-approved-{tag}.csv" for tag in REVIEW_TAGS]
# MARK starts each commit in the log stream so hits can be counted per commit.
MARK = "\x1ecommit "
# DATASET_GLOBS are the prompt-log dataset files, the only places a person-reason value is counted.
DATASET_GLOBS = ["*prompts-redacted*.json", "public/porting/every-prompt-log.json",
                 "src/app/porting/data/every-prompt.json"]
DATASET_ONLY_REASONS = {"person"}


def is_dataset(path: str | None) -> bool:
    """Whether a repository path is a prompt-log dataset file (None is a commit message)."""
    return path is not None and any(fnmatch.fnmatchcase(path, g) or fnmatch.fnmatchcase(path, "*/" + g)
                                    for g in DATASET_GLOBS)


def redact_rows(path: Path) -> list[tuple[str, str, list[str]]]:
    """("<file name> L<line>", reason, [lowercased value, JSON-escaped form]) for each redact row."""
    out = []
    with open(path, newline="", encoding="utf-8") as f:
        for n, r in enumerate(csv.DictReader(f), start=2):
            if r["decision"] == "redact":
                v = r["match"].lower()
                out.append((f"{path.name} L{n}", r["reason"], sorted({v, json.dumps(v)[1:-1]})))
    return out


def all_redact_rows(paths: list[Path]) -> list[tuple[str, str, list[str]]]:
    """redact_rows of every file, a value already listed by an earlier file left out."""
    out, seen = [], set()
    for path in paths:
        for row in redact_rows(path):
            if tuple(row[2]) not in seen:
                seen.add(tuple(row[2]))
                out.append(row)
    return out


def word_re(value: str) -> re.Pattern:
    """Matches value (lowercased) where no ASCII letter, digit or underscore touches it.

    ASCII-only, so a value right after an accented letter (decoded from a \\uXXXX escape) counts.
    """
    return re.compile(r"(?<![A-Za-z0-9_])" + re.escape(value) + r"(?![A-Za-z0-9_])")


# JSON_STRING_RE finds JSON string literals (in a .json/.jsonl file or patch line).
JSON_STRING_RE = re.compile(r'"(?:[^"\\\n]|\\.)*"')


def decode_json_strings(text: str) -> str:
    """text with every JSON string literal replaced by its decoded content.

    In raw JSON a value right after an escape ("\\n", "\\t", "\\u00e9") has a letter before it
    and would not count as a whole word; decoded, it does.
    """

    def dec(m: re.Match) -> str:
        """The decoded literal, or the literal itself when it is not valid JSON."""
        try:
            return " " + json.loads(m.group(0)) + " "
        except ValueError:
            return m.group(0)

    return JSON_STRING_RE.sub(dec, text)


def count_text(path: str | None, text: str, reason: str, vals: list[str]) -> tuple[int, int]:
    """(whole-word hits, substring hits) of a row's values in one file's text (or a message).

    Whole-word hits are counted on the JSON-decoded text for .json/.jsonl files, else on the raw
    text. Substring hits (plain, case-insensitive, over the raw text) are counted for every reason
    but person, as a safety net against boundary misses. A person-reason row counts only in
    dataset files, and only whole words (the username is part of many unrelated words).
    """
    if reason in DATASET_ONLY_REASONS and not is_dataset(path):
        return 0, 0
    low = text.lower()
    words = decode_json_strings(low) if path and path.endswith((".json", ".jsonl")) else low
    hits = sum(len(word_re(v).findall(words)) for v in vals)
    sub = 0 if reason in DATASET_ONLY_REASONS else sum(low.count(v) for v in vals)
    return hits, sub


def scan_log(repo: str, revs: list[str], rows) -> dict:
    """Per row: [hits, commits, substring hits] over `git log -p` of revs, commit by commit.

    A person-reason row counts only the hits in the patches of dataset files (is_dataset).
    """
    res = {n: [0, 0, 0] for n, _r, _v in rows}
    proc = subprocess.Popen(["git", "-C", repo, "log", "-p", "--format=%x1ecommit %H%n%B", *revs],
                            stdout=subprocess.PIPE, text=True, errors="replace")
    buf: list[tuple[str | None, list[str]]] = []  # (file of the patch or None for the message, lines)

    def flush():
        """Count the buffered commit's hits."""
        if not buf:
            return
        for n, reason, vals in rows:
            hits = sub = 0
            for f, lines in buf:
                h, u = count_text(f, "".join(lines), reason, vals)
                hits += h
                sub += u
            if hits or sub:
                res[n][0] += hits
                res[n][1] += 1
                res[n][2] += sub
        buf.clear()

    for line in proc.stdout:
        if line.startswith(MARK):
            flush()
            buf.append((None, []))
        elif line.startswith("diff --git ") and buf:
            m = re.match(r"diff --git a/(.*) b/(.*)$", line.rstrip("\n"))
            buf.append((m.group(2) if m else "", []))
        if not buf:
            buf.append((None, []))
        buf[-1][1].append(line)
    flush()
    proc.wait()
    return res


def files_at(repo: str, rev: str, vals: list[str], datasets_only: bool = False) -> int:
    """Number of files in rev's tree that contain any of vals (case-insensitive); with
    datasets_only, dataset files only."""
    args = ["git", "-C", repo, "grep", "-l", "-i", "-w", "-F"]
    for v in vals:
        args += ["-e", v]
    out = subprocess.run([*args, rev, "--"], capture_output=True, text=True).stdout
    files = [x.split(":", 1)[1] if x.startswith(rev + ":") else x for x in out.splitlines() if x]
    return len([f for f in files if not datasets_only or is_dataset(f)])


def main() -> None:
    """Parse args, scan, print counts per CSV line number only."""
    ap = argparse.ArgumentParser()
    ap.add_argument("--repo", required=True)
    ap.add_argument("--approved", type=Path, action="append",
                    help="approved-redactions CSV (repeatable; default: every extract's review)")
    ap.add_argument("revs", nargs="+")
    a = ap.parse_args()
    rows = all_redact_rows(a.approved or APPROVED)
    log = scan_log(a.repo, a.revs, rows)
    tips = [r for r in a.revs if ".." not in r and not r.startswith("^")]
    total = 0
    for n, reason, vals in rows:
        only = reason in DATASET_ONLY_REASONS
        files = " ".join(f"files@{t}={files_at(a.repo, t, vals, only)}" for t in tips)
        print(f"row {n} ({reason}): hits={log[n][0]} substr={log[n][2]} commits={log[n][1]} {files}".rstrip())
        total += log[n][0] + log[n][2]
    print(f"TOTAL hits={total}")
    sys.exit(1 if total else 0)


if __name__ == "__main__":
    main()
