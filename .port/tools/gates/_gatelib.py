#!/usr/bin/env python3
"""
_gatelib.py — shared helpers for the rustty parity gates (G1, G2, G3, G9, G8).

This module is imported by the per-gate scripts. It encapsulates the things every
gate needs:

  * Locating the contract artifacts under .port/phase-0/.
  * The authoritative file -> crate map (from tests-summary.json's `crate_map`,
    which is exactly PORT_PLAN's file->crate partition and sums to the 3234
    contract; see PORT_PLAN Section 0).
  * Discovering each rustty crate's ported Rust sources under crates/<crate>/src.
  * Detecting whether a crate is "claimed-done" (its lib.rs no longer says
    `PORT STATUS: stub`). Per DEFINITION_OF_PARITY a *claimed-done* crate that is
    missing tests/comments/assertions is RED; an unclaimed (stub) crate is
    counted for progress only and never errors the run (progress mode).
  * Lightweight Rust scanners for `#[test]` fns, in-test assertions, and comment
    units — string/char/line-comment aware so we never miscount `//` inside a
    string literal.

NOTHING here weakens a gate (R5): the scanners only COUNT; pass/fail logic lives
in each gate. The helpers never edit a contract.
"""
import json
import os
import re
import sys

# ---------------------------------------------------------------------------
# Paths
# ---------------------------------------------------------------------------
# This file lives at $RUSTTY/.port/tools/gates/_gatelib.py
GATES_DIR = os.path.dirname(os.path.abspath(__file__))
TOOLS_DIR = os.path.dirname(GATES_DIR)
PORT_DIR = os.path.dirname(TOOLS_DIR)
RUSTTY_ROOT = os.path.dirname(PORT_DIR)

PHASE0 = os.path.join(PORT_DIR, "phase-0")
INVENTORY_DIR = os.path.join(PHASE0, "inventory")
BASELINE_DIR = os.path.join(PHASE0, "baseline")
PLAN_DIR = os.path.join(PHASE0, "plan")
CRATES_DIR = os.path.join(RUSTTY_ROOT, "crates")

TESTS_SUMMARY = os.path.join(INVENTORY_DIR, "tests-summary.json")
TESTS_JSONL = os.path.join(INVENTORY_DIR, "tests.jsonl")
COMMENTS_CENSUS = os.path.join(INVENTORY_DIR, "comments-census.json")
PARITY_SURFACE = os.path.join(INVENTORY_DIR, "parity-surface.json")
BASELINE_JSON = os.path.join(BASELINE_DIR, "BASELINE.json")
IDENTITY_MAP = os.path.join(PLAN_DIR, "identity-rename.map")

# Crates that own ported reference code but live in crates/<name> where <name>
# is "rustty-<crate>". The bin crate `rustty` maps to crate key "bin".
CRATE_DIR_OVERRIDES = {
    "bin": "rustty",
}

# Crates listed in the inventory crate_map that are NOT part of the crates/
# parity workspace (they are supporting / not-in-DAG per PORT_PLAN Section 4).
# NOTE: `extra` graduated to a real workspace crate (crates/rustty-extra) with the
# src/extra/ generator port, so it is no longer excluded here.
NON_WORKSPACE_CRATES = {"xtask", "resources"}


# ---------------------------------------------------------------------------
# Contract loading
# ---------------------------------------------------------------------------
class Blocked(Exception):
    """Raised when a required contract artifact/field is absent. A gate that
    catches this must report BLOCKED (not green, not skipped) per DoP §0."""


def load_json(path, what):
    if not os.path.exists(path):
        raise Blocked(f"contract artifact missing: {what} ({path})")
    try:
        with open(path, encoding="utf-8") as fh:
            return json.load(fh)
    except Exception as e:  # noqa: BLE001
        raise Blocked(f"contract artifact unreadable: {what} ({path}): {e}")


def load_tests_summary():
    return load_json(TESTS_SUMMARY, "tests-summary.json")


def load_comments_census():
    return load_json(COMMENTS_CENSUS, "comments-census.json")


def load_parity_surface():
    return load_json(PARITY_SURFACE, "parity-surface.json")


def load_baseline():
    return load_json(BASELINE_JSON, "BASELINE.json")


# ---------------------------------------------------------------------------
# File -> crate map
# ---------------------------------------------------------------------------
def crate_map(summary=None):
    """Return {crate: {"files": [(relpath, test_blocks), ...],
                       "test_blocks": int, "file_count": int}}.

    Source of truth: tests-summary.json's `crate_map` (== PORT_PLAN file->crate
    partition; sums to 3234)."""
    if summary is None:
        summary = load_tests_summary()
    if "crate_map" not in summary:
        raise Blocked("tests-summary.json lacks `crate_map` (PORT_PLAN file->crate map)")
    out = {}
    for crate, info in summary["crate_map"].items():
        files = [(f["file"], int(f["test_blocks"])) for f in info["files"]]
        out[crate] = {
            "files": files,
            "test_blocks": int(info["test_blocks"]),
            "file_count": int(info["file_count"]),
        }
    return out


def file_to_crate(summary=None):
    """{relpath: crate} reverse index over the whole tree."""
    out = {}
    for crate, info in crate_map(summary).items():
        for rel, _ in info["files"]:
            out[rel] = crate
    return out


def workspace_crates(cm=None):
    """Crate keys that have a crates/<dir> directory in the rustty workspace."""
    if cm is None:
        cm = crate_map()
    res = []
    for crate in cm:
        if crate in NON_WORKSPACE_CRATES:
            continue
        if crate_src_dir(crate) is not None:
            res.append(crate)
    return res


def crate_dir_name(crate):
    """Map a crate key (e.g. 'vt', 'bin') to its crates/ directory name."""
    if crate in CRATE_DIR_OVERRIDES:
        return CRATE_DIR_OVERRIDES[crate]
    return "rustty-" + crate


def normalize_crate(arg, cm=None):
    """Accept either the crate_map key ('datastruct') OR the real Cargo crate
    name ('rustty-datastruct', and 'rustty' for the bin). Returns the canonical
    crate_map key; returns the arg unchanged if no match so the caller still
    reports an honest 'unknown crate' (never a silent pass)."""
    if not arg:
        return arg
    if cm is None:
        cm = crate_map()
    if arg in cm:
        return arg
    if arg == "rustty" and "bin" in cm:
        return "bin"
    if arg.startswith("rustty-"):
        stripped = arg[len("rustty-"):]
        if stripped in cm:
            return stripped
    return arg


def crate_src_dir(crate):
    """Absolute path to crates/<dir>/src, or None if the crate dir is absent."""
    d = os.path.join(CRATES_DIR, crate_dir_name(crate), "src")
    return d if os.path.isdir(d) else None


# ---------------------------------------------------------------------------
# Done-status detection
# ---------------------------------------------------------------------------
PORT_STATUS_RE = re.compile(r"PORT\s*STATUS\s*:\s*([A-Za-z0-9_-]+)", re.IGNORECASE)


def crate_status(crate):
    """Return the declared PORT STATUS of a crate ('stub', 'done', ...).

    Reads the crate's lib.rs (or main.rs for the bin crate). Returns 'absent'
    if no crate dir, 'unknown' if the marker is missing."""
    src = crate_src_dir(crate)
    if src is None:
        return "absent"
    for root_name in ("lib.rs", "main.rs"):
        p = os.path.join(src, root_name)
        if os.path.exists(p):
            try:
                with open(p, encoding="utf-8", errors="replace") as fh:
                    head = fh.read(8192)
            except Exception:  # noqa: BLE001
                continue
            m = PORT_STATUS_RE.search(head)
            if m:
                return m.group(1).lower()
    return "unknown"


# A crate is "claimed-done" when it declares a status that is NOT 'stub'/'unknown'/
# 'absent'. 'done', 'complete', 'parity' all count as a completion claim.
DONE_STATUSES = {"done", "complete", "parity", "ported", "final"}


def is_claimed_done(crate):
    return crate_status(crate) in DONE_STATUSES


# ---------------------------------------------------------------------------
# Rust source scanning
# ---------------------------------------------------------------------------
def iter_rust_files(crate):
    src = crate_src_dir(crate)
    if src is None:
        return
    for dirpath, _dirs, fnames in os.walk(src):
        for fn in fnames:
            if fn.endswith(".rs"):
                yield os.path.join(dirpath, fn)


def _strip_rust(text):
    """Return text with string/char literals and comments blanked out (replaced
    by spaces of equal length so offsets are preserved), plus a parallel list of
    (kind, content) for every comment encountered. Handles // line comments,
    /* */ (nesting) block comments, "..." strings, r#"..."# raw strings, and
    'c' char literals. Good enough for counting; not a full Rust lexer."""
    out = []
    comments = []  # (kind, text) kind in {"doc-module","doc-decl","line"}
    i = 0
    n = len(text)
    while i < n:
        c = text[i]
        nxt = text[i + 1] if i + 1 < n else ""
        # line comment
        if c == "/" and nxt == "/":
            j = i + 2
            # classify
            kind = "line"
            if text[i:i + 3] == "//!":
                kind = "doc-module"
            elif text[i:i + 3] == "///" and not text[i:i + 4] == "////":
                kind = "doc-decl"
            while j < n and text[j] != "\n":
                j += 1
            comments.append((kind, text[i:j]))
            out.append(" " * (j - i))
            i = j
            continue
        # block comment (nesting)
        if c == "/" and nxt == "*":
            kind = "line"
            if text[i:i + 3] in ("/**", "/*!") and text[i:i + 4] != "/**/":
                kind = "doc-decl" if text[i:i + 3] == "/**" else "doc-module"
            depth = 1
            j = i + 2
            while j < n and depth > 0:
                if text[j] == "/" and j + 1 < n and text[j + 1] == "*":
                    depth += 1
                    j += 2
                elif text[j] == "*" and j + 1 < n and text[j + 1] == "/":
                    depth -= 1
                    j += 2
                else:
                    j += 1
            comments.append((kind, text[i:j]))
            out.append(" " * (j - i))
            i = j
            continue
        # raw string r"..." / r#"..."#
        if (c == "r" or (c == "b" and nxt == "r")) and re.match(r'b?r#*"', text[i:i + 8] or ""):
            m = re.match(r'b?r(#*)"', text[i:])
            if m:
                hashes = m.group(1)
                close = '"' + hashes
                start = i + m.end()
                k = text.find(close, start)
                if k == -1:
                    k = n
                else:
                    k += len(close)
                out.append(" " * (k - i))
                i = k
                continue
        # normal string "..."
        if c == '"':
            j = i + 1
            while j < n:
                if text[j] == "\\":
                    j += 2
                    continue
                if text[j] == '"':
                    j += 1
                    break
                j += 1
            out.append(" " * (j - i))
            i = j
            continue
        # char / lifetime: only treat as char literal if it looks like 'x' or '\n'
        if c == "'":
            m = re.match(r"'(\\.|[^'\\])'", text[i:])
            if m:
                out.append(" " * len(m.group(0)))
                i += len(m.group(0))
                continue
            out.append(c)
            i += 1
            continue
        out.append(c)
        i += 1
    return "".join(out), comments


# Rust test attribute: #[test] possibly with cfg, and #[tokio::test] etc.
TEST_ATTR_RE = re.compile(r"#\s*\[\s*(?:[\w:]+::)?test\b")
SHOULD_PANIC_RE = re.compile(r"#\s*\[\s*should_panic\b")
FN_NAME_RE = re.compile(r"\bfn\s+([A-Za-z_][A-Za-z0-9_]*)")


def name_candidates(name):
    """Canonical set of valid-Rust-identifier forms a reference test name may
    legitimately take when ported (DoP §G1: named tests "match name-for-name"
    via the fixed naming convention). A Rust `fn` cannot contain spaces or most
    punctuation, so a string-named reference test (e.g. `"module as error set"`)
    is ported to an identifier slug. This returns every accepted slug form so the
    name-set diff matches the ported fn to its reference name. Mirrors the same
    normalization used by check_parity_surface.py — it is NOT a weakening: every
    reference test must still have a corresponding ported test, and genuine
    misses/extras are still flagged.

    Accepted forms include the raw name (for already-identifier names like
    `errorAfter`), camelCase, PascalCase, snake_case, SCREAMING_SNAKE, and the
    raw token with hyphen/space -> underscore."""
    out = set()
    out.add(name)
    # split on word boundaries (space, hyphen, underscore) and drop empties
    parts = [p for p in re.split(r"[-_ ]", name) if p]
    if len(parts) > 1:
        # camelCase and PascalCase
        out.add(parts[0] + "".join(p.capitalize() for p in parts[1:]))
        out.add("".join(p.capitalize() for p in parts))
        # snake_case (normalize hyphen/space -> underscore)
        out.add("_".join(p.lower() for p in parts))
        # SCREAMING_SNAKE
        out.add("_".join(p.upper() for p in parts))
    # camelCase/PascalCase -> snake (for ids that arrive already camel)
    out.add(re.sub(r"(?<!^)(?=[A-Z])", "_", name).lower())
    # hyphen/space (and any non-identifier run) -> underscore on the raw token
    out.add(re.sub(r"[^\w]+", "_", name).strip("_"))
    return {v for v in out if v}

# In-test assertion macros / forms (count, not parse).
ASSERTION_PATTERNS = [
    re.compile(r"\bassert!\s*\("),
    re.compile(r"\bassert_eq!\s*\("),
    re.compile(r"\bassert_ne!\s*\("),
    re.compile(r"\bdebug_assert!\s*\("),
    re.compile(r"\bdebug_assert_eq!\s*\("),
    re.compile(r"\bdebug_assert_ne!\s*\("),
    re.compile(r"\bassert_matches!\s*\("),
    re.compile(r"\.expect\s*\("),
    re.compile(r"\.unwrap\s*\(\s*\)"),
    re.compile(r"\.unwrap_err\s*\(\s*\)"),
    re.compile(r"\bpanic!\s*\("),
    re.compile(r"\bunreachable!\s*\("),
]
# `?`-propagation used as an assertion inside a `-> Result` test body.
QMARK_RE = re.compile(r"\?[\s;,)\]]")


def scan_rust_tests(crate):
    """Return a list of ported test fn names found in the crate's Rust sources.

    A "test" is a `fn` immediately preceded (ignoring other attributes/whitespace)
    by a `#[test]`-family attribute. Anonymous-block equivalents follow the fixed
    convention `test_anon_<n>` and are matched by name like any other."""
    names = []
    for path in iter_rust_files(crate):
        try:
            with open(path, encoding="utf-8", errors="replace") as fh:
                text = fh.read()
        except Exception:  # noqa: BLE001
            continue
        clean, _ = _strip_rust(text)
        for m in TEST_ATTR_RE.finditer(clean):
            # find the next `fn NAME` after the attribute
            fm = FN_NAME_RE.search(clean, m.end())
            if fm:
                names.append(fm.group(1))
    return names


def _find_balanced_block(text, open_idx):
    """Given index of a '{', return index just past the matching '}'."""
    depth = 0
    i = open_idx
    n = len(text)
    while i < n:
        if text[i] == "{":
            depth += 1
        elif text[i] == "}":
            depth -= 1
            if depth == 0:
                return i + 1
        i += 1
    return n


def scan_rust_test_assertions(crate):
    """Return {test_name: assertion_count} for each ported test fn.

    Counts assertion-family macros + `.expect/.unwrap` + `?`-propagation + a
    +1 for a `#[should_panic]` test body inside the *cleaned* (comment/string
    free) test body."""
    result = {}
    for path in iter_rust_files(crate):
        try:
            with open(path, encoding="utf-8", errors="replace") as fh:
                text = fh.read()
        except Exception:  # noqa: BLE001
            continue
        clean, _ = _strip_rust(text)
        for m in TEST_ATTR_RE.finditer(clean):
            fm = FN_NAME_RE.search(clean, m.end())
            if not fm:
                continue
            name = fm.group(1)
            # should_panic detection: scan ONLY the attribute block bounded by
            # the previous item terminator (a `}` or `;`) and this test's `fn`,
            # so it cannot leak into the next item's attributes.
            prev_term = max(clean.rfind("}", 0, m.start()),
                            clean.rfind(";", 0, m.start()))
            attr_block = clean[prev_term + 1:fm.start()] if prev_term >= 0 else clean[:fm.start()]
            should_panic = bool(SHOULD_PANIC_RE.search(attr_block))
            brace = clean.find("{", fm.end())
            if brace == -1:
                result[name] = 1 if should_panic else 0
                continue
            end = _find_balanced_block(clean, brace)
            body = clean[brace:end]
            count = 0
            for pat in ASSERTION_PATTERNS:
                count += len(pat.findall(body))
            count += len(QMARK_RE.findall(body))
            if should_panic:
                count += 1
            result[name] = count
    return result


def census_rust_comments(crate):
    """Return {"doc-module": n, "doc-decl": n, "line": n, "total": n} comment-line
    counts over the crate's Rust sources, mirroring comments-census classes.

    A comment *unit* is counted per-line (matching the census, which counts
    `total_comment_lines`)."""
    counts = {"doc-module": 0, "doc-decl": 0, "line": 0}
    for path in iter_rust_files(crate):
        try:
            with open(path, encoding="utf-8", errors="replace") as fh:
                text = fh.read()
        except Exception:  # noqa: BLE001
            continue
        _, comments = _strip_rust(text)
        for kind, ctext in comments:
            # count one unit per physical line of the comment
            lines = max(1, ctext.count("\n") + (0 if ctext.endswith("\n") else 1))
            # line comments are single-line; block comments may span many
            if "\n" in ctext:
                lines = ctext.count("\n") + 1
            else:
                lines = 1
            counts[kind] += lines
    counts["total"] = counts["doc-module"] + counts["doc-decl"] + counts["line"]
    return counts


# ---------------------------------------------------------------------------
# Reporting helpers
# ---------------------------------------------------------------------------
GREEN = "GREEN"
RED = "RED"
BLOCKED_STATE = "BLOCKED"


def banner(gate_id, name):
    print(f"=== {gate_id} :: {name} ===")


def emit(state, msg=""):
    line = state if not msg else f"{state}: {msg}"
    print(line)
