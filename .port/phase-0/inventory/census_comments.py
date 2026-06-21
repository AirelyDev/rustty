#!/usr/bin/env python3
"""
census_comments.py  --  R1 comment-fidelity baseline census.

Censuses EVERY comment in the READ-ONLY ghostty reference tree
(/Users/gael/projects/ghostty/src/**.zig, pinned at 4749c4e, Zig 0.15.2) and
emits a machine-checkable baseline that the port's fidelity critic diffs against.
Per PORT_RULES R1: no comment may be dropped, so this is the contract count.

Zig comment grammar (from the language reference):
  //   line comment
  ///  doc comment   (documents the *declaration* that follows)
  //!  top-level doc comment (documents the enclosing *module*/container)
A comment runs from the marker to end-of-line.

The hard part is NOT counting `//` that appears inside string/char literals.
Zig has:
  - normal strings:   "...."   with backslash escapes (\" \\ etc.)
  - char  literals:   '....'   with backslash escapes
  - multiline strings: a line whose first non-space token is `\\`; the rest of
    the *physical line* is literal string content with NO escapes and NO
    comments. (So `\\// foo` is string data, not a comment.)

We therefore scan each physical line with a tiny state machine that respects
normal-string and char-literal context, and treats `\\` (when reached outside a
string/char) as "rest of line is multiline-string content" -> no comment there.

Classification of each detected comment marker:
  doc-module  -> //!
  doc-decl    -> ///   (but NOT //// which Zig lexes as a normal // comment)
  line        -> //    (everything else)

Additional (non-exclusive) tagging applied to the comment TEXT:
  license/header block : detected per FILE (license-ish markers in a comment
                         within the first header region of the file)
  TODO / FIXME / NOTE  : marker words inside comment text
  spec/citation        : references to terminal specs / prior art
                         (ECMA-48, xterm, DEC, VT###, kitty, ISO 2022, RFC, etc.)
  ascii-diagram        : box-drawing / heavy punctuation "diagram" comment lines

Output: inventory/comments-census.json
"""

from __future__ import annotations

import json
import os
import re
import sys
from collections import defaultdict

SRC_ROOT = "/Users/gael/projects/ghostty/src"
OUT_DIR = "/Users/gael/projects/rustty/.port/phase-0/inventory"
OUT_JSON = os.path.join(OUT_DIR, "comments-census.json")
GHOSTTY_PIN = "4749c4e93731067049bfbf2e4572061cef2bdd17"


# --------------------------------------------------------------------------
# Core scanner: find the start column of a real `//` comment on a physical
# line, honoring string/char/multiline-string context. Returns the index of
# the first `/` of the comment, or None if the line has no comment.
# --------------------------------------------------------------------------
def find_comment_start(line: str) -> int | None:
    i = 0
    n = len(line)
    while i < n:
        c = line[i]

        # Normal string literal: consume to its closing quote, honoring escapes.
        if c == '"':
            i += 1
            while i < n:
                if line[i] == "\\":
                    i += 2
                    continue
                if line[i] == '"':
                    i += 1
                    break
                i += 1
            continue

        # Char literal: consume to its closing quote, honoring escapes.
        # Zig char literals are short, but be safe with escapes.
        if c == "'":
            i += 1
            while i < n:
                if line[i] == "\\":
                    i += 2
                    continue
                if line[i] == "'":
                    i += 1
                    break
                i += 1
            continue

        # Comment vs multiline string both begin with a backslash-or-slash run.
        if c == "/":
            if i + 1 < n and line[i + 1] == "/":
                return i  # real comment starts here
            i += 1
            continue

        if c == "\\":
            # `\\` begins a multiline string literal: the rest of the physical
            # line is string content -> no comment can appear after it.
            if i + 1 < n and line[i + 1] == "\\":
                return None
            i += 1
            continue

        i += 1
    return None


# --------------------------------------------------------------------------
# Tagging regexes (applied to the comment text, i.e. starting at the marker).
# --------------------------------------------------------------------------
TODO_RE = re.compile(r"\b(TODO|FIXME)\b")
NOTE_RE = re.compile(r"\b(NOTE|XXX|HACK|WARNING|SAFETY|IMPORTANT|BUG)\b")

# Spec / citation references: terminal-emulation prior art & standards.
SPEC_RE = re.compile(
    r"""(
        ECMA[\s-]?48
      | ECMA[\s-]?35
      | ISO[\s-]?2022
      | ISO[\s-]?8859
      | \bxterm\b
      | \bDEC\s+[A-Z]
      | \bDECSET\b | \bDECRST\b | \bDECSTBM\b | \bDECRQM\b | \bDECSCUSR\b
      | \bVT\d{2,3}\b
      | vt100\.net
      | invisible-island
      | \bkitty\b
      | \biTerm2?\b
      | \bsixel\b | \bSixel\b
      | \bRFC\s?\d+
      | \bUAX\s?#?\d+
      | \bUnicode\b
      | \bxterm\.js\b
      | \bmintty\b
      | \bwezterm\b
      | \bfoot\b
      | \bkonsole\b
      | \bw3m\b
    )""",
    re.VERBOSE,
)

# ASCII-diagram heuristic: box-drawing characters, or a comment body that is
# mostly diagram punctuation (-, |, +, =, etc.) rather than prose.
BOX_DRAWING_RE = re.compile(r"[─-╿▀-▟■-◿]")
DIAGRAM_PUNCT_RE = re.compile(r"^[\s|+\-=/\\^v<>*#._:()\[\]{}~]+$")

# License / header markers: matched inside the file's leading header region.
LICENSE_RE = re.compile(
    r"""(
        \bSPDX-License-Identifier\b
      | \bCopyright\b | \bCopyright\s*\(c\)
      | \(c\)\s*\d{4}
      | \bLICENSE\b | \blicense\b
      | \bMIT License\b
      | \bApache License\b
      | \bGPL\b | \bLGPL\b | \bGNU\b
      | \bBSD-style\b | \bBSD\s+license\b
      | \bMozilla Public License\b | \bMPL\b
      | \bAll rights reserved\b
      | \bPermission is hereby granted\b
      | \bgoverned by\b.*\blicense\b
    )""",
    re.VERBOSE | re.IGNORECASE,
)


def classify_marker(comment: str) -> str:
    """comment starts at the `//` marker. Return doc-module|doc-decl|line."""
    # //!  -> doc-module
    if comment.startswith("//!"):
        return "doc-module"
    # ///  -> doc-decl, BUT //// (4+) is a normal line comment in Zig.
    if comment.startswith("///") and not comment.startswith("////"):
        return "doc-decl"
    return "line"


def is_ascii_diagram(text: str) -> bool:
    """text is the comment body (after stripping the marker)."""
    if BOX_DRAWING_RE.search(text):
        return True
    stripped = text.strip()
    if len(stripped) < 3:
        return False
    # A run of pure diagram punctuation with at least a few symbols.
    if DIAGRAM_PUNCT_RE.match(stripped):
        # require some structure, not just a divider of a single char type
        symbols = set(ch for ch in stripped if not ch.isspace())
        if len(stripped) >= 3 and (
            len(symbols) >= 2 or any(ch in "|+/\\^v<>" for ch in symbols)
        ):
            return True
    return False


def marker_body(comment: str) -> str:
    """Strip the leading comment marker (//, ///, //!) and one space."""
    m = re.match(r"//[/!]?", comment)
    if m:
        return comment[m.end():]
    return comment


def census():
    if not os.path.isdir(SRC_ROOT):
        sys.exit(f"ERROR: source root not found: {SRC_ROOT}")

    files = []
    for dirpath, _dirnames, filenames in os.walk(SRC_ROOT):
        for fn in filenames:
            if fn.endswith(".zig"):
                files.append(os.path.join(dirpath, fn))
    files.sort()

    totals = {
        "doc-module": 0,
        "doc-decl": 0,
        "line": 0,
    }
    total_comment_lines = 0
    total_todo_fixme = 0
    total_note = 0
    total_spec = 0
    total_ascii_diagram = 0

    per_file = {}  # relpath -> dict of counts
    license_files = []  # relpath list
    todo_fixme_locations = []  # {file,line,text}
    license_header_details = []  # {file, lines:[...]} for transparency

    for path in files:
        rel = os.path.relpath(path, SRC_ROOT)
        with open(path, "r", encoding="utf-8", errors="surrogateescape") as fh:
            raw_lines = fh.readlines()

        fcounts = {
            "doc-module": 0,
            "doc-decl": 0,
            "line": 0,
            "total": 0,
            "todo_fixme": 0,
            "note": 0,
            "spec": 0,
            "ascii_diagram": 0,
        }

        # Track the file's leading "header region": comment lines (and blank
        # lines interleaved) that occur before the first real code token.
        header_region_active = True

        file_has_license = False
        file_license_lines = []

        for lineno, raw in enumerate(raw_lines, start=1):
            line = raw.rstrip("\n").rstrip("\r")
            cstart = find_comment_start(line)

            # Determine whether code precedes any comment on this line.
            code_before = line[:cstart] if cstart is not None else line
            has_code = bool(code_before.strip())

            if cstart is None:
                # No comment on this line. If there's code, header region ends.
                if has_code:
                    header_region_active = False
                continue

            comment = line[cstart:]
            cls = classify_marker(comment)
            body = marker_body(comment)

            fcounts[cls] += 1
            fcounts["total"] += 1
            totals[cls] += 1
            total_comment_lines += 1

            # TODO/FIXME / NOTE-family tagging.
            if TODO_RE.search(comment):
                fcounts["todo_fixme"] += 1
                total_todo_fixme += 1
                todo_fixme_locations.append(
                    {"file": rel, "line": lineno, "text": comment.strip()}
                )
            if NOTE_RE.search(comment):
                fcounts["note"] += 1
                total_note += 1

            # Spec / citation tagging.
            if SPEC_RE.search(comment):
                fcounts["spec"] += 1
                total_spec += 1

            # ASCII diagram tagging.
            if is_ascii_diagram(body):
                fcounts["ascii_diagram"] += 1
                total_ascii_diagram += 1

            # License-header detection: only within the leading header region,
            # AND only for comment-only lines (no code before the comment).
            if header_region_active and not has_code:
                if LICENSE_RE.search(comment):
                    file_has_license = True
                    file_license_lines.append(
                        {"line": lineno, "text": comment.strip()}
                    )
            elif has_code:
                # code on the line (before the comment) ends the header region
                header_region_active = False

        if fcounts["total"] > 0 or file_has_license:
            per_file[rel] = fcounts
        if file_has_license:
            license_files.append(rel)
            license_header_details.append(
                {"file": rel, "matches": file_license_lines}
            )

    license_files.sort()
    todo_fixme_locations.sort(key=lambda d: (d["file"], d["line"]))

    result = {
        "_meta": {
            "description": (
                "R1 comment-fidelity baseline census of the READ-ONLY ghostty "
                "reference. Every comment line in src/**.zig is counted and "
                "classified. This is the contract the port's fidelity critic "
                "diffs against; no comment may be dropped."
            ),
            "source_root": SRC_ROOT,
            "ghostty_pin": GHOSTTY_PIN,
            "zig_version": "0.15.2",
            "files_scanned": len(files),
            "files_with_comments": len(per_file),
            "generator": "census_comments.py",
            "classes": {
                "doc-module": "//!  top-level/module doc comment",
                "doc-decl": "///  declaration doc comment (//// is a plain line comment)",
                "line": "//   ordinary line comment",
            },
            "notes": (
                "`//` inside string literals, char literals, and multiline "
                "string content (\\\\...) is NOT counted as a comment. A comment "
                "may carry multiple tags (todo_fixme, note, spec, ascii_diagram); "
                "tags are non-exclusive overlays, not a partition."
            ),
        },
        "totals": {
            "total_comment_lines": total_comment_lines,
            "by_class": totals,
            "total_todo_fixme": total_todo_fixme,
            "total_note_family": total_note,
            "total_spec_citation": total_spec,
            "total_ascii_diagram": total_ascii_diagram,
            "files_with_license_headers": len(license_files),
        },
        "files_with_license_headers": license_files,
        "license_header_details": license_header_details,
        "todo_fixme_locations": todo_fixme_locations,
        "per_file": per_file,
    }

    os.makedirs(OUT_DIR, exist_ok=True)
    with open(OUT_JSON, "w", encoding="utf-8") as fh:
        json.dump(result, fh, indent=2, ensure_ascii=False)
        fh.write("\n")

    # Concise stderr summary for the operator / orchestrator.
    print(f"files scanned:            {len(files)}", file=sys.stderr)
    print(f"files with comments:      {len(per_file)}", file=sys.stderr)
    print(f"total comment lines:      {total_comment_lines}", file=sys.stderr)
    print(f"  doc-module (//!):       {totals['doc-module']}", file=sys.stderr)
    print(f"  doc-decl   (///):       {totals['doc-decl']}", file=sys.stderr)
    print(f"  line       (//):        {totals['line']}", file=sys.stderr)
    print(f"total TODO/FIXME:         {total_todo_fixme}", file=sys.stderr)
    print(f"total NOTE-family:        {total_note}", file=sys.stderr)
    print(f"total spec/citation:      {total_spec}", file=sys.stderr)
    print(f"total ascii-diagram:      {total_ascii_diagram}", file=sys.stderr)
    print(f"files w/ license headers: {len(license_files)}", file=sys.stderr)
    print(f"written: {OUT_JSON}", file=sys.stderr)

    return result


if __name__ == "__main__":
    census()
