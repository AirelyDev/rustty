#!/usr/bin/env python3
"""
_gatelib.py — shared helpers for the rustty parity gates (G1, G2, G3, G9, G8).

This module is imported by the per-gate scripts. It encapsulates the things every
gate needs:

  * Locating the contract artifacts under .port/phase-0/.
  * The authoritative file -> crate map (from tests-summary.json's `crate_map`,
    which is exactly PORT_PLAN's file->crate partition and sums to the 3234
    contract; see PORT_PLAN Section 0).
  * Discovering each rustty crate's ported Rust sources under crates/<crate>/src,
    plus its R4 relocations (`RELOCATIONS`, the single map every
    crate-scanning gate reads; see `iter_rust_sources`).
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
import unicodedata

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
# The status is the run of word-tokens after the colon, separated by SINGLE
# spaces, stopping at any other punctuation ('done (chunk 1a' -> 'done';
# 'in progress' -> 'in progress', not the truncated 'in').
PORT_STATUS_RE = re.compile(
    r"PORT\s*STATUS\s*:\s*([A-Za-z0-9_-]+(?: [A-Za-z0-9_-]+)*)", re.IGNORECASE)


def dir_status(crate_dir):
    """Declared PORT STATUS for an explicit crate DIRECTORY (works for both
    crates/ and crates-sys/ trees — check_leaks.sh's enumerator shims through
    this so G6 shares one done-detection with G1/G2/G3b).

    Reads the crate's src/lib.rs (or src/main.rs for a bin crate) in FULL —
    a marker past a former 8KB read cap no longer yields 'unknown'; the FIRST
    marker in file order (the header) wins. Returns 'unknown' if absent."""
    for root_name in ("lib.rs", "main.rs"):
        p = os.path.join(crate_dir, "src", root_name)
        if os.path.exists(p):
            try:
                with open(p, encoding="utf-8", errors="replace") as fh:
                    text = fh.read()
            except Exception:  # noqa: BLE001
                continue
            m = PORT_STATUS_RE.search(text)
            if m:
                return m.group(1).lower()
    return "unknown"


def crate_status(crate):
    """Return the declared PORT STATUS of a crate ('stub', 'done', ...).

    Reads the crate's lib.rs (or main.rs for the bin crate). Returns 'absent'
    if no crate dir, 'unknown' if the marker is missing."""
    src = crate_src_dir(crate)
    if src is None:
        return "absent"
    return dir_status(os.path.dirname(src))


# A crate is "claimed-done" when it declares a status that is NOT 'stub'/'unknown'/
# 'absent'. 'done', 'complete', 'parity' all count as a completion claim.
DONE_STATUSES = {"done", "complete", "parity", "ported", "final"}


def is_claimed_done(crate):
    return crate_status(crate) in DONE_STATUSES


# ---------------------------------------------------------------------------
# Rust source scanning
# ---------------------------------------------------------------------------
def iter_rust_files(crate):
    """The .rs files under the crate's OWN `crates/<dir>/src` only. Gates scan
    through `iter_rust_sources`, which adds the crate's R4 relocations."""
    src = crate_src_dir(crate)
    if src is None:
        return
    for dirpath, _dirs, fnames in os.walk(src):
        for fn in fnames:
            if fn.endswith(".rs"):
                yield os.path.join(dirpath, fn)


# ---------------------------------------------------------------------------
# R4 RELOCATIONS — the single source of truth (S-8a introduced it in G3b as
# `EXTRA_PORT_PATHS`; S-8c hoisted it here so every crate-scanning gate reads
# the same map).
# ---------------------------------------------------------------------------
# The crate map partitions the ORACLE files by crate key, but R4 moved some of
# those files' ports into a DIFFERENT crate (to break a dependency cycle or to
# honour a layering decision). The crate's own `src/` then lacks the port, and
# every gate that scans only `src/` reads the relocated tests, assertions,
# comments, constants and surface ids as drops that were never dropped. Each
# entry names the EXACT extra port location — a subtree, a file, or named items
# inside a file, never a whole foreign crate when less suffices — plus the
# decision that put it there. Every gate then scans the crate's own `src/` PLUS
# these locations: the same logic, the correct port side.
#
# NOT A WEAKENING (R5):
#   * A relocation adds only code that implements the crate's own reference
#     files; the reference side (contract, census, surface) is unchanged, so a
#     reference item still has to appear in the port that implements it.
#   * A listed path that does not exist, or an item that cannot be found in it,
#     raises `RelocationMissing` (a `Blocked`): every gate reports BLOCKED, never
#     a silent fall-back — a vanished relocation would otherwise hide that the
#     port moved again.
#   * A relocated location is usually ALSO scanned by its host crate (the vt
#     input slice is in rustty-vt's `src/`). Workspace-wide totals therefore
#     count each (file, offset) ONCE (`*_located` scanners below), so a
#     relocation can never inflate a global floor by double counting.
#
# `items` (optional): regexes, each matching the first line of ONE top-level
# item in the file (`^pub enum Key\b`). The item's span runs from the contiguous
# comment/attribute block directly above that line (up to a blank line) to the
# end of the item's balanced braces. Everything outside the spans is blanked
# (newlines kept, so line numbers stay true); only the named items count.
RELOCATIONS = {
    # D1 three-tier font split (memory `apc-glyf-font-layering-d1`): the
    # shaper/backend-face tier of src/font/ (face*, shape/shaper*, Collection,
    # CodepointResolver, DeferredFace, discovery, SharedGrid*, library, backend
    # and the main.zig glue) lives ABOVE rustty-vt in rustty-font-shaper, which
    # has no crate-map key of its own (every file it ports is a `font` file).
    "font": (
        {"path": "crates/rustty-font-shaper/src"},
    ),
    # The VT slice of src/input/ is owned by rustty-vt (see the ownership doc in
    # crates/rustty-input/src/lib.rs). Only the files that port INPUT's mapped
    # oracle files are listed: function_keys.zig and kitty.zig in full, and the
    # VT halves of key_mods.zig (the VT-Mods half) and config.zig
    # (`OptionAsAlt`). key.zig, key_encode.zig, mouse*.zig and paste.zig are
    # rustty-vt's own crate-map files and are NOT relocated.
    "input": (
        {"path": "crates/rustty-vt/src/input/function_keys.rs"},
        {"path": "crates/rustty-vt/src/input/kitty.rs"},
        {"path": "crates/rustty-vt/src/input/key_mods.rs"},
        {"path": "crates/rustty-vt/src/input/config.rs"},
    ),
    # config/key.zig's `Key` enum and `Value` fn are ported with the `Config`
    # struct they reflect over (rustty-cfg-fmt stays a near-leaf, R4 break #6).
    # Only key.zig's three items count: `Key`, `value_type` (the `Value` port)
    # and the `test "Value"` module — never the rest of config_struct/key.rs
    # (split out of config_struct.rs by P7-W13 F), which ports config/Config.zig
    # for the `config` crate.
    "cfg-fmt": (
        {"path": "crates/rustty-config/src/config_struct/key.rs",
         "items": (r"^pub enum Key\b", r"^pub fn value_type\b",
                   r"^mod key_tests\b")},
    ),

    # ---- per-reference-file relocations (parity tail S-8b, 2026-09-24; folded
    # in here by S-8c). Each entry names the ONE reference file (`ref`, relative
    # to the oracle's src/) whose port lives at `path` in another crate; it
    # belongs to the crate that owns `ref` in the crate map (G3b's self-test
    # checks that). Before these, `--crate bin` reported 146 MISSING comments of
    # noise (the DEFERRALS "main_c.zig (3t) re-homed ..." row called that
    # "EXPECTED") that hid the real drops.
    #
    # bin (`crates/rustty`): the non-exe artifact roots live in their own crates.
    "bin": (
        # lib_vt.zig: the librustty-vt root — the C-lib root/panic/log sink in
        # vt-c, the Zig-module public API in rustty-vt's crate root (ADV-FIX 6).
        {"ref": "lib_vt.zig", "path": "crates/rustty-vt-c/src/lib.rs"},
        {"ref": "lib_vt.zig", "path": "crates/rustty-vt-c/src/panic.rs"},
        {"ref": "lib_vt.zig", "path": "crates/rustty-vt-c/src/sys.rs"},
        {"ref": "lib_vt.zig", "path": "crates/rustty-vt/src/lib.rs"},
        # main_c.zig: the C API hub, re-homed to rustty-apprt-c (ADV-FIX 6).
        {"ref": "main_c.zig", "path": "crates/rustty-apprt-c/src/lib.rs"},
        # main_ghostty.zig: `logFn`/`std_options` moved to the core wiring tier
        # (the lib root re-uses them too); the rest stays in the bin's
        # main_rustty.rs.
        {"ref": "main_ghostty.zig", "path": "crates/rustty-core/src/logging.rs"},
        # main_gen.zig / main_wasm.zig / main_build_data.zig: separate artifacts.
        {"ref": "main_gen.zig", "path": "crates/rustty-synthetic/src/main_gen.rs"},
        {"ref": "main_wasm.zig", "path": "crates/rustty-wasm/src/lib.rs"},
        {"ref": "main_build_data.zig", "path": "xtask/src/main_build_data.rs"},
    ),
    # renderer: Thread.zig's run loop lives in core (it names configpkg and
    # core's App.Mailbox, both cycles from rustty-renderer); backend.zig's
    # `Backend` enum is defined in build-config (break #3).
    "renderer": (
        {"ref": "renderer/Thread.zig", "path": "crates/rustty-core/src/renderer_seam.rs"},
        {"ref": "renderer/backend.zig", "path": "crates/rustty-build-config/src/renderer.rs"},
    ),
    # apprt: embedded.zig's `CAPI` block (the libghostty C exports + their extern
    # structs) lives in the rustty-apprt-c top-sink crate (vt-c precedent,
    # EMBEDDED_BLUEPRINT §3); the GTK build helpers blueprint.zig/gresource.zig
    # are the crate's build script, outside `src/`.
    "apprt": (
        {"ref": "apprt/embedded.zig", "path": "crates/rustty-apprt-c/src/lib.rs"},
        {"ref": "apprt/embedded.zig", "path": "crates/rustty-apprt-c/src/capi.rs"},
        {"ref": "apprt/gtk/build/blueprint.zig", "path": "crates/rustty-apprt/build.rs"},
        {"ref": "apprt/gtk/build/gresource.zig", "path": "crates/rustty-apprt/build.rs"},
    ),
}


class RelocationMissing(Blocked):
    """An R4 relocation names a path or item that does not exist (stale map)."""


def _relocation_abs(path):
    return path if os.path.isabs(path) else os.path.join(RUSTTY_ROOT, path)


def _rs_under(path):
    """Every .rs file at/under `path` (a directory or a single .rs file), in a
    deterministic order."""
    if os.path.isfile(path):
        return [path] if path.endswith(".rs") else []
    out = []
    for dirpath, _dirs, fnames in os.walk(path):
        for fn in fnames:
            if fn.endswith(".rs"):
                out.append(os.path.join(dirpath, fn))
    return sorted(out)


def _item_spans(text, patterns, path="<text>"):
    """[(start, end)] character spans of the named top-level items in `text`.
    Raises RelocationMissing if a pattern matches no line or opens no block."""
    clean, _ = _strip_rust(text)
    lines = text.split("\n")
    offsets = []
    pos = 0
    for ln in lines:
        offsets.append(pos)
        pos += len(ln) + 1
    spans = []
    for pat in patterns:
        rx = re.compile(pat)
        hit = None
        for i, ln in enumerate(lines):
            # match against the comment-blanked line so a quoted item in a
            # comment can never stand in for the real one
            if rx.search(clean[offsets[i]:offsets[i] + len(ln)]):
                hit = i
                break
        if hit is None:
            raise RelocationMissing(
                f"R4 relocation item {pat!r} not found in {path} (stale map)")
        first = hit
        while first > 0:
            prev = lines[first - 1].strip()
            if prev.startswith("//") or prev.startswith("#["):
                first -= 1
                continue
            break
        brace = clean.find("{", offsets[hit])
        semi = clean.find(";", offsets[hit])
        if brace == -1 or (semi != -1 and semi < brace):
            raise RelocationMissing(
                f"R4 relocation item {pat!r} in {path} opens no block (stale map)")
        spans.append((offsets[first], _find_balanced_block(clean, brace)))
    return sorted(spans)


def _mask_to_spans(text, spans):
    """`text` with every character outside `spans` blanked (newlines kept)."""
    out = []
    pos = 0
    for s, e in spans:
        if s > pos:
            out.append(re.sub(r"[^\n]", " ", text[pos:s]))
        out.append(text[max(s, pos):e])
        pos = max(pos, e)
    out.append(re.sub(r"[^\n]", " ", text[pos:]))
    return "".join(out)


def relocation_sources(crate, relocations=None):
    """[(abs_path, text)] for `crate`'s R4 relocations (see RELOCATIONS).
    `relocations` overrides the map (self-tests only). Raises RelocationMissing
    for a path or item that does not exist."""
    rel_map = RELOCATIONS if relocations is None else relocations
    out = []
    for entry in rel_map.get(crate, ()):
        full = _relocation_abs(entry["path"])
        if not os.path.exists(full):
            raise RelocationMissing(
                f"R4 relocation path for [{crate}] does not exist: {full} "
                f"(RELOCATIONS is stale)")
        items = entry.get("items")
        for p in _rs_under(full):
            try:
                with open(p, encoding="utf-8", errors="replace") as fh:
                    text = fh.read()
            except OSError as e:
                raise RelocationMissing(f"R4 relocation file unreadable: {p}: {e}")
            if items:
                text = _mask_to_spans(text, _item_spans(text, items, p))
            out.append((p, text))
    return out


def check_relocations(crates=None, relocations=None):
    """Resolve every relocation of `crates` (default: every mapped crate) and
    raise RelocationMissing on the first stale one. Gates call this up front so
    a stale map is BLOCKED before any verdict is printed."""
    rel_map = RELOCATIONS if relocations is None else relocations
    for crate in (rel_map if crates is None else crates):
        relocation_sources(crate, rel_map)


def selftest_install_gatelib(gates_dir, relocations=None):
    """Self-test fixture helper: copy THIS module into a synthetic root's
    `.port/tools/gates/`, optionally replacing RELOCATIONS (paths relative to
    that root). Used by every gate's relocation self-test arm; the override is
    an appended module-level assignment, so the live map is never edited."""
    with open(os.path.abspath(__file__), encoding="utf-8") as fh:
        text = fh.read()
    if relocations is not None:
        text += "\n\n# SELF-TEST FIXTURE OVERRIDE\nRELOCATIONS = %r\n" % (relocations,)
    with open(os.path.join(gates_dir, "_gatelib.py"), "w", encoding="utf-8") as fh:
        fh.write(text)


def iter_rust_sources(crate, relocations=None):
    """(path, text) for the crate's port side: every .rs under its own `src/`,
    then its R4 relocations (a relocated file already under the crate's own
    `src/` is not yielded twice). This is what every crate-scanning gate
    reads."""
    seen = set()
    for path in iter_rust_files(crate):
        try:
            with open(path, encoding="utf-8", errors="replace") as fh:
                text = fh.read()
        except Exception:  # noqa: BLE001
            continue
        seen.add(os.path.realpath(path))
        yield path, text
    for path, text in relocation_sources(crate, relocations):
        if os.path.realpath(path) in seen:
            continue
        seen.add(os.path.realpath(path))
        yield path, text


def _strip_rust(text, keep_strings=False):
    """Return text with string/char literals and comments blanked out (replaced
    by spaces of equal length so offsets are preserved), plus a parallel list of
    (kind, content) for every comment encountered. Handles // line comments,
    /* */ (nesting) block comments, "..." strings, r#"..."# raw strings, and
    'c' char literals. Good enough for counting; not a full Rust lexer.

    `keep_strings=True` blanks ONLY comments and keeps string/char literals in
    place (G9 surface-id matching: cap/action ids live in `b"..."` literals —
    real code, unlike comments which could fake coverage via verbatim Zig
    blocks). Default behavior is unchanged."""
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
                out.append(text[i:k] if keep_strings else " " * (k - i))
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
            out.append(text[i:j] if keep_strings else " " * (j - i))
            i = j
            continue
        # char / lifetime: only treat as char literal if it looks like 'x' or '\n'
        if c == "'":
            m = re.match(r"'(\\.|[^'\\])'", text[i:])
            if m:
                out.append(m.group(0) if keep_strings else " " * len(m.group(0)))
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


def port_slug(name):
    """Composed port-slug form (the port's actual uniform convention):
    split on every non-alphanumeric run, camel-split each token, lowercase,
    join with '_'.  'DynamicPalette: changeDefault preserves changes'
      -> dynamic_palette_change_default_preserves_changes
    'bash: ENV' -> bash_env ; 'parseIntoField: bool' -> parse_into_field_bool"""
    words = []
    for tok in re.split(r"[^A-Za-z0-9]+", name):
        if not tok:
            continue
        words.extend(
            w for w in re.split(r"(?<=[a-z0-9])(?=[A-Z])|(?<=[A-Z])(?=[A-Z][a-z])|(?<=[A-Za-z])(?=[0-9])", tok) if w
        )
    return "_".join(w.lower() for w in words)


# Symbol -> spelled-out word observed in the port's fn names (family F7,
# "symbol-worded slug"). A Rust identifier cannot carry `?`/`+`/`;`/... so the
# port spells the symbol out as a word WHEN it is semantically load-bearing
# (and simply drops it when it is a mere separator, e.g. the `+` in
# `shift+alt+.`). Every entry below is VERIFIED against a real ported test:
#   '->' -> 'to'         'OSC 9;4: progress -> desktop notification 1'
#                        -> osc_9_4_progress_to_desktop_notification_1
#                           (rustty-vt/src/osc/parsers/osc9.rs:954)
#   '='  -> 'eq'         "OSC 133: fresh_line_new_prompt with '=' in aid"
#                        -> osc_133_fresh_line_new_prompt_with_eq_in_aid
#                           (rustty-vt/src/osc/parsers/semantic_prompt.rs:1004)
#   ';'  -> 'semicolon'  'OSC 133: fresh_line_new_prompt with trailing ;'
#                        -> osc_133_fresh_line_new_prompt_with_trailing_semicolon
#                           (semantic_prompt.rs:1067); also
#                        "kitty: report alternates colon (shift+';')"
#                        -> kitty_report_alternates_colon_shift_semicolon
#                           (rustty-vt/src/input/key_encode.rs:2195)
#   '>'  -> 'gt'         'Terminal: OSC133C at x>0 on prompt row ...'
#                        -> terminal_osc133c_at_xgt0_on_prompt_row_...
#                           (rustty-vt/src/terminal.rs:15710)
#   '['  -> 'bracket'    'csi: ESC [ H' -> csi_esc_bracket_h
#                           (rustty-vt/src/parser.rs:753, also :776)
#   '('  -> 'paren'      'esc: ESC ( B' -> esc_esc_paren_b (parser.rs:729)
#   '.'  -> 'period'     'legacy: shift+alt+. macos'
#                        -> legacy_shift_alt_period_macos (key_encode.rs:2960)
#   '?'  -> 'question'   'stream CSI ? W reset tab stops'
#                        -> stream_csi_question_w_reset_tab_stops
#                           (rustty-vt/src/stream.rs:3956)
#   '+'  -> 'plus'       'stream: SGR with 17+ parameters for underline color'
#                        -> stream_sgr_with_17_plus_parameters_for_underline_color
#                           (stream.rs:3999)
#   'ф'  -> 'f cyrillic' 'legacy: alt+ф' -> legacy_alt_f_cyrillic
#                           (key_encode.rs:2990)
#   'ő'  -> 'o double acute'
#                        'legacy: hu layout ctrl+ő sends proper codepoint'
#                        -> legacy_hu_layout_ctrl_o_double_acute_sends_proper_codepoint
#                           (key_encode.rs:3644)
# This only ADDS candidate forms (R5-safe): a form still has to match a ported
# fn exactly (or canon-exactly in the resolver); genuine misses stay MISSING.
SYMBOL_WORDS = (
    ("->", "to"),
    ("=", "eq"),
    (";", "semicolon"),
    (">", "gt"),
    ("[", "bracket"),
    ("(", "paren"),
    (".", "period"),
    ("?", "question"),
    ("+", "plus"),
    ("ф", "f cyrillic"),          # ф
    ("ő", "o double acute"),      # ő
)

# Cap the variant fan-out (a name with many mapped symbols would otherwise
# explode 2^k; real reference names carry only a handful).
_VARIANT_CAP = 64


def symbol_worded_variants(name):
    """Variants of `name` with each SYMBOL_WORDS symbol either left in place
    (it slugs away as a separator) or spelled out as its word. Both choices are
    generated because the same symbol can be load-bearing in one name ('17+'
    -> '17_plus') and a separator in another ('shift+alt' -> 'shift_alt').
    Returns only the variants that differ from `name` itself."""
    variants = {name}
    for sym, word in SYMBOL_WORDS:
        if sym not in name:
            continue
        new = set()
        for v in variants:
            new.add(v)                                  # symbol as separator
            new.add(v.replace(sym, " %s " % word))      # symbol spelled out
        variants = new
        if len(variants) >= _VARIANT_CAP:
            break
    variants.discard(name)
    return variants


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
    `errorAfter`), camelCase, PascalCase, snake_case, SCREAMING_SNAKE, the
    raw token with hyphen/space -> underscore, the composed port slug, and the
    composed port slug of every symbol-worded variant (family F7; each mapping
    in SYMBOL_WORDS is verified against a real ported test — see the table)."""
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
    # Composed port-slug form (the port's actual uniform convention).
    out.add(port_slug(name))
    # Symbol-worded slugs (family F7): the port spells load-bearing symbols out
    # as words; each candidate is still an EXACT fn-name form.
    for v in symbol_worded_variants(name):
        out.add(port_slug(v))
    return {v for v in out if v}


# ---------------------------------------------------------------------------
# Layered reference-name -> ported-fn resolver (shared by G1 and G2)
# ---------------------------------------------------------------------------
# Formerly private to check_assertions.py (H6 fix); hoisted here so G1's
# name-set diff and G2's per-test comparison bind reference tests to ported
# fns THE SAME way (one convention, one matcher — no gate can drift from the
# other). The layers are unchanged from check_assertions' documented L1-L5;
# canonicalization gains Unicode NFC (canonically-equivalent strings — e.g. a
# decomposed `ò` in tests.jsonl vs the composed form — canonicalize
# identically; pure-ASCII names are unaffected).
def canon_id(s):
    """Canonical identifier form: Unicode NFC, then lowercase with every
    non-[a-z0-9] stripped. 'PageList pointFromPin active' and
    pagelist_pointFromPin_active canonicalize identically."""
    return re.sub(r"[^a-z0-9]", "", unicodedata.normalize("NFC", s).lower())


def build_fn_index(fn_names):
    """Precompute the per-crate resolution index over ported test fn names
    (any iterable of names): (fn_set, canon->fns map, [(fn, canon)] list)."""
    fn_set = set(fn_names)
    canon_map = {}
    canon_list = []
    for fn in fn_set:
        cf = canon_id(fn)
        canon_map.setdefault(cf, []).append(fn)
        canon_list.append((fn, cf))
    return fn_set, canon_map, canon_list


def resolve_ref_test(name, fn_index):
    """Resolve a reference test name to the set of ported fn names it may
    legitimately have been ported as (layers L1-L5, see check_assertions.py's
    module docstring). Returns a (possibly empty) set. First matching layer
    wins so the most precise binding is used.

      L1  name_candidates(ref) ∩ ported fn names (exact slug forms, incl. the
          symbol-worded F7 variants)
      L2  canonical form equality — for the raw name AND each symbol-worded
          variant (covers camel-preserved slugs like
          pagelist_pointFromPin_active_no_history, F2/F3 canon families)
      L3  canonical form + 'test' prefix/suffix (fn-name-collision convention,
          e.g. vt_write -> vt_write_test)
      L4  disambiguating type/module prefix ADDED by the port: fn canon ENDS
          WITH ref canon (>= 5 chars) ('parseCLI base' -> palette_parse_cli_base)
      L5  redundant type prefix DROPPED by the port: ref canon ENDS WITH fn
          canon (fn canon >= 10 chars) ('SelectionGesture drag selection logic'
          -> drag_selection_logic)"""
    fn_set, canon_map, canon_list = fn_index
    # L1: exact slug-candidate intersection (DoP §G1 convention)
    hits = name_candidates(name) & fn_set
    if hits:
        return hits
    # L2: canonical equality (raw name first, then symbol-worded variants)
    canons = [canon_id(name)]
    canons += [canon_id(v) for v in symbol_worded_variants(name)]
    for c in canons:
        if c and c in canon_map:
            return set(canon_map[c])
    c = canons[0]
    if not c:
        return set()
    # L3: 'test' suffix/prefix (fn-name-collision convention)
    hits = set(canon_map.get(c + "test", [])) | set(canon_map.get("test" + c, []))
    if hits:
        return hits
    # L4: port added a disambiguating type/module prefix
    if len(c) >= 5:
        hits = {fn for fn, cf in canon_list if cf.endswith(c) and cf != c}
        if hits:
            return hits
    # L5: port dropped a redundant leading type name
    return {fn for fn, cf in canon_list
            if len(cf) >= 10 and c.endswith(cf) and cf != c}

# In-test assertion macros / forms (count, not parse).
#
# UNDILUTED COUNT (M7 fix): the reference side (tests.jsonl assertion_count)
# counts only `testing.expect*` calls — the Zig `try` on an ordinary fallible
# call is NOT an assertion there. The Rust equivalents of that plumbing
# (`.unwrap()`, `.expect(..)`, `?`-propagation) therefore must NOT count
# toward assertion parity: with them included, a test could lose ALL of its
# real assertions and still satisfy rust >= ref. Error-ASSERTING forms stay:
# `.unwrap_err()` / `.expect_err(..)` assert that a Result IS an error
# (Zig `testing.expectError`), and `#[should_panic]` is a whole-test assert.
ASSERTION_PATTERNS = [
    re.compile(r"\bassert!\s*\("),
    re.compile(r"\bassert_eq!\s*\("),
    re.compile(r"\bassert_ne!\s*\("),
    re.compile(r"\bdebug_assert!\s*\("),
    re.compile(r"\bdebug_assert_eq!\s*\("),
    re.compile(r"\bdebug_assert_ne!\s*\("),
    re.compile(r"\bassert_matches!\s*\("),
    re.compile(r"\.unwrap_err\s*\(\s*\)"),
    re.compile(r"\.expect_err\s*\("),
    re.compile(r"\bpanic!\s*\("),
    re.compile(r"\bunreachable!\s*\("),
]


def scan_rust_tests_located(crate, relocations=None):
    """[(path, offset, name)] for every ported test fn in the crate's port side
    (own `src/` plus R4 relocations). The (path, offset) pair identifies a test
    uniquely across crates, which is how workspace totals avoid counting a
    relocated test twice (once for its host crate, once for its owner)."""
    out = []
    for path, text in iter_rust_sources(crate, relocations):
        clean, _ = _strip_rust(text)
        for m in TEST_ATTR_RE.finditer(clean):
            # find the next `fn NAME` after the attribute
            fm = FN_NAME_RE.search(clean, m.end())
            if fm:
                out.append((os.path.realpath(path), fm.start(), fm.group(1)))
    return out


def scan_rust_tests(crate, relocations=None):
    """Return a list of ported test fn names found in the crate's Rust sources
    (its own `src/` plus its R4 relocations).

    A "test" is a `fn` immediately preceded (ignoring other attributes/whitespace)
    by a `#[test]`-family attribute. Anonymous-block equivalents follow the fixed
    convention `test_anon_<n>` and are matched by name like any other."""
    return [name for _p, _o, name in scan_rust_tests_located(crate, relocations)]


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


def scan_rust_test_assertions(crate, relocations=None):
    """Return {test_name: [assertion_count, ...]} for each ported test fn —
    a LIST because the same fn name may legitimately appear in several test
    modules (e.g. utf8.rs and cli/utf8.rs both define `fn utf8()`); a plain
    dict overwrite would silently drop all but the last occurrence, corrupting
    both the crate total and the per-test comparison.

    Counts assertion-family macros + `.unwrap_err/.expect_err` + a +1 for a
    `#[should_panic]` test body inside the *cleaned* (comment/string free)
    test body. `.unwrap()`/`.expect(..)`/`?` are deliberately NOT counted
    (M7): they are Zig-`try` plumbing, not assertions — see ASSERTION_PATTERNS."""
    result = {}
    for _path, _off, name, count in scan_rust_test_assertions_located(crate, relocations):
        result.setdefault(name, []).append(count)
    return result


def scan_rust_test_assertions_located(crate, relocations=None):
    """[(path, offset, name, assertion_count)] per ported test fn, over the
    crate's own `src/` plus its R4 relocations (see scan_rust_tests_located for
    why the location is carried)."""
    result = []
    for path, text in iter_rust_sources(crate, relocations):
        loc = os.path.realpath(path)
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
                result.append((loc, fm.start(), name, 1 if should_panic else 0))
                continue
            end = _find_balanced_block(clean, brace)
            body = clean[brace:end]
            count = 0
            for pat in ASSERTION_PATTERNS:
                count += len(pat.findall(body))
            if should_panic:
                count += 1
            result.append((loc, fm.start(), name, count))
    return result


def census_rust_comments(crate, relocations=None):
    """Return {"doc-module": n, "doc-decl": n, "line": n, "total": n} comment-line
    counts over the crate's Rust sources, mirroring comments-census classes.

    A comment *unit* is counted per-line (matching the census, which counts
    `total_comment_lines`)."""
    counts = {"doc-module": 0, "doc-decl": 0, "line": 0}
    for _path, _idx, kind, lines in census_rust_comments_located(crate, relocations):
        counts[kind] += lines
    counts["total"] = counts["doc-module"] + counts["doc-decl"] + counts["line"]
    return counts


def census_rust_comments_located(crate, relocations=None):
    """[(path, comment_index, kind, lines)] per comment over the crate's own
    `src/` plus its R4 relocations (the location lets workspace totals count a
    relocated comment once). A masked (item-scoped) relocation keeps only the
    comments inside its items: everything else was blanked."""
    out = []
    for path, text in iter_rust_sources(crate, relocations):
        loc = os.path.realpath(path)
        _, comments = _strip_rust(text)
        for idx, (kind, ctext) in enumerate(comments):
            # count one unit per physical line of the comment
            # line comments are single-line; block comments may span many
            if "\n" in ctext:
                lines = ctext.count("\n") + 1
            else:
                lines = 1
            out.append((loc, idx, kind, lines))
    return out


# ---------------------------------------------------------------------------
# CONTRACT DRIFT — entries upstream DELETED or RENAMED after the frozen pin
# ---------------------------------------------------------------------------
# The contract (.port/phase-0/inventory/) is frozen at 4749c4e93 and never
# regenerated. The oracle pin has since advanced, so the contract drifts in both
# directions: it cannot see the tests upstream WROTE (CENSUS_DELTA.md) and it
# still DEMANDS the tests upstream DELETED or RENAMED. Demanding a test that no
# longer exists upstream is not fidelity, it is the opposite.
#
# `.port/phase-3/CONTRACT_DELETIONS.md` is the ledger of those entries. It is
# read by G1 (name-set diff) and G2 (crate assertion total), which is why the
# loader lives here rather than in either gate.
#
# NOT A WEAKENING (R5). A row is never trusted: every claim it makes is
# RE-DERIVED against the live oracle on every run, and a row that fails any arm
# is REJECTED (the exemption is refused and the gate says so). The arms:
#
#   1. the contract name is ABSENT upstream, matched EXACTLY (`test "<name>" {`,
#      leading whitespace allowed for a nested test). Exactness matters both
#      ways: the old prefix match let `datastruct::get` collide with every
#      `test "getSize"` in the tree (a false REJECT), while splitting the name
#      at ": " searched for `test "scrollUp with…"` when the real declaration
#      was `test "Terminal: scrollUp with…"` — a string that never existed, so
#      the arm passed vacuously for every `X: Y` row (a false ACCEPT).
#   2. the named commit is an ancestor of the oracle's HEAD, and
#   3. that commit's own diff REMOVED that exact declaration. Arm 2 alone
#      accepts any ancestor sha — which is how `os::args` came to cite
#      `b5053153f`, a commit that never touched `src/os/args.zig`.
#   4. if the row names successor test(s) — the rename case — each must exist
#      upstream today, have been ADDED by that same commit, and (checked by the
#      caller, which holds the crate's ported fn index) resolve to a ported
#      Rust fn. So a rename row is honoured only when the port demonstrably
#      carries the renamed test; it can never hide an unported one.
CONTRACT_DELETIONS_MD = os.path.join(PORT_DIR, "phase-3", "CONTRACT_DELETIONS.md")

# `| `crate::name` | `sha` | `succ`, `succ` (or an em dash) | prose |`
_LEDGER_ROW = re.compile(
    r"^\|\s*`([^`]+)`\s*\|\s*`([0-9a-f]{7,40})`\s*\|([^|]*)\|", re.M)


def git_env():
    """The process environment with every `GIT_*` variable removed.

    For git calls that read the oracle or locate a checkout: `GIT_DIR`,
    `GIT_WORK_TREE`, `GIT_COMMON_DIR` and the `GIT_CONFIG_*` injection family
    would otherwise redirect them to another repository (R5)."""
    return {k: v for k, v in os.environ.items() if not k.startswith("GIT_")}


def oracle_root(checkout=RUSTTY_ROOT):
    """The read-only ghostty oracle checkout: `ghostty` BESIDE THE MAIN checkout.

    The one resolver every sibling-oracle gate uses. `checkout/../ghostty` is
    wrong inside a git worktree — from `.claude/worktrees/<wt>` it names
    `.claude/worktrees/ghostty`, which does not exist (G-LOGFILE reported
    BLOCKED from every agent worktree until a stopgap symlink was planted
    there). `--git-common-dir` names the MAIN repository's `.git` from inside
    any worktree, so its parent is the real checkout.

    R5 — nothing here is caller-settable. There is no environment override, and
    git runs with every `GIT_*` variable removed: `GIT_DIR` would otherwise
    redirect `--git-common-dir` to a decoy repository and, with it, the oracle.
    `checkout` exists for the gates' own self-tests; production callers pass
    their script-relative root. git's stdout is trusted only when it exits 0.
    If git cannot answer, the fallback is the checkout's own sibling — the
    pre-worktree behaviour, which from a worktree names a missing directory.

    This function only LOCATES the oracle; it never checks that it exists. Every
    caller must treat a missing (or off-pin, see `oracle_pin`) oracle as BLOCKED
    — never as a skip that lets the gate go green. That is the caller's
    contract, and each caller's self-test asserts it.

    `.port/tools/gates/_oracle_root.sh` is the shell twin of this function; its
    self-test checks that the two agree.
    """
    import subprocess
    try:
        r = subprocess.run(
            ["git", "rev-parse", "--path-format=absolute", "--git-common-dir"],
            cwd=checkout, capture_output=True, text=True, env=git_env())
        common = r.stdout.strip() if r.returncode == 0 else ""
    except OSError:
        common = ""
    main_checkout = os.path.dirname(common) if common else checkout
    return os.path.abspath(os.path.join(main_checkout, "..", "ghostty"))


# The ONE place the oracle pin is written down: `ORACLE_PIN`, a one-line file
# beside this module. `_oracle_root.sh`'s `rustty_oracle_pin` reads the same
# file, and G-ORACLE-SH-SELF asserts the two readers agree. Advancing the pin
# (`.port/ORACLE_SYNC.md`) means editing that file and nothing else in the gates.
ORACLE_PIN_FILE = os.path.join(GATES_DIR, "ORACLE_PIN")


def oracle_pin(path=None):
    """The full 40-hex oracle pin from `ORACLE_PIN`, or None if it is missing
    or malformed. Callers treat None as BLOCKED, never as "no pin to check"."""
    try:
        with open(path or ORACLE_PIN_FILE, encoding="utf-8") as fh:
            pin = fh.read().strip()
    except OSError:
        return None
    return pin if re.fullmatch(r"[0-9a-f]{40}", pin) else None


def oracle_pin_problem(oracle=None, pin=None):
    """Why the ORACLE checkout cannot be trusted at the pin, or None when it can.

    The Python twin of `_oracle_root.sh`'s `rustty_oracle_pin_status` (N483): a
    missing checkout, a directory that is not a repository, an unreadable or
    malformed `ORACLE_PIN`, and a HEAD that is not exactly the pin each return a
    one-line reason. A gate that reads the oracle's WORKING TREE calls this
    first and turns any reason into BLOCKED (exit 3), never into a skip. Every
    read of the oracle goes through `oracle_git` (GIT_* stripped)."""
    oracle = oracle if oracle is not None else oracle_root()
    pin = pin if pin is not None else oracle_pin()
    if not os.path.isdir(oracle):
        return f"no oracle checkout at {oracle}"
    if not pin:
        return f"the oracle pin file {ORACLE_PIN_FILE} is missing or not a 40-hex commit"
    try:
        r = oracle_git(oracle, "rev-parse", "--verify", "-q", "HEAD",
                       capture_output=True, text=True)
    except OSError:
        return f"{oracle} is not a checkout with a HEAD"
    if r.returncode != 0 or not r.stdout.strip():
        return f"{oracle} is not a checkout with a HEAD"
    head = r.stdout.strip()
    if head != pin:
        return f"the oracle at {oracle} is at {head}, not the pin {pin}"
    return None


def zig_cache_root():
    """zig's global cache directory, resolved the way zig does (N483): the
    `ZIG_GLOBAL_CACHE_DIR` override, else `$XDG_CACHE_HOME/zig`, else
    `~/.cache/zig` (zig's default on Linux and macOS alike). Never a fixed home.

    Content-addressed packages live under `<root>/p/<name-version-hash>`. The
    path is only LOCATED here: a package directory that is absent is the
    caller's BLOCKED, and `restore_zig_deps.sh` is the way back."""
    env = os.environ
    if env.get("ZIG_GLOBAL_CACHE_DIR"):
        return env["ZIG_GLOBAL_CACHE_DIR"]
    if env.get("XDG_CACHE_HOME"):
        return os.path.join(env["XDG_CACHE_HOME"], "zig")
    return os.path.join(os.path.expanduser("~"), ".cache", "zig")


def oracle_git(oracle, *args, **kw):
    """`git -C <oracle> <args>` with every `GIT_*` variable removed (R5).

    The Python twin of `_oracle_root.sh`'s `rustty_oracle_git`: every read of
    the oracle's CONTENTS (`show`, `log`, `merge-base`, `rev-parse`) goes
    through here, because `GIT_DIR` would otherwise answer `git -C <oracle>`
    from another repository. Keyword arguments pass to `subprocess.run`."""
    import subprocess
    kw.setdefault("capture_output", True)
    return subprocess.run(["git", "-C", oracle, *args], env=git_env(), **kw)


def root_redirect_probe(gates_dir, module, attrs, want):
    """Self-test arm helper: import `module` from `gates_dir` in a FRESH process
    whose GIT_DIR + GIT_WORK_TREE name a decoy repository and whose working
    directory is that decoy (outside this checkout), print `attrs`, and compare
    them with `want`. Returns (ok, detail). This is how a gate proves its root
    is script-relative: `git rev-parse --show-toplevel` would print the decoy.
    """
    import shutil
    import subprocess
    import tempfile
    base = os.path.realpath(tempfile.mkdtemp(prefix="gate_root_redirect_"))
    try:
        decoy = os.path.join(base, "rustty")
        os.makedirs(decoy)
        env = git_env()
        subprocess.run(["git", "init", "-q"], cwd=decoy, env=env,
                       capture_output=True, check=True)
        env.update({"GIT_DIR": os.path.join(decoy, ".git"), "GIT_WORK_TREE": decoy})
        probe = ("import sys; d = sys.argv[1]; sys.argv = [sys.argv[0]]; "
                 f"sys.path.insert(0, d); import {module} as M; "
                 + "; ".join(f"print(M.{a})" for a in attrs))
        r = subprocess.run([sys.executable, "-c", probe, gates_dir],
                           cwd=decoy, env=env, capture_output=True, text=True)
        got = r.stdout.split("\n")[:len(attrs)]
        if r.returncode != 0 or got != list(want):
            return False, (f"probe rc={r.returncode} got {got}, want {list(want)}; "
                           f"stderr={r.stderr.strip()[-300:]!r}")
        return True, ""
    except (OSError, subprocess.CalledProcessError) as exc:
        return False, f"fixture could not be built: {exc}"
    finally:
        shutil.rmtree(base, ignore_errors=True)


def _oracle_src():
    """`../ghostty/src` beside the MAIN checkout (correct inside a worktree)."""
    return os.path.join(oracle_root(), "src")


def _test_decl_re(name, prefix=""):
    """Match a zig test declaration for exactly `name`, any indentation.

    Both Zig spellings count: the string form `test "name" {` and the
    identifier form `test name {` (which the contract records under the same
    name — `lib/string.zig`'s `test cutPrefix {` is one). `prefix` prepends a
    diff marker (`-`/`+`) for the "this commit removed/added it" arms."""
    alts = ["\"" + re.escape(name) + "\""]
    if re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*", name):
        alts.append(re.escape(name))
    return re.compile("^" + prefix + r"[ \t]*test (?:" + "|".join(alts) + r") \{",
                      re.M)


def contract_deletions():
    """Verified rows of CONTRACT_DELETIONS.md.

    Returns (rows, rejects) where
      rows    = {"crate::name": {"sha", "successors": [...], "crate", "name"}}
                — every arm 1-3 verified against the LIVE oracle, and
      rejects = [(entry, why)] for rows that failed an arm.
    An unreadable/absent ledger or oracle yields ({}, []) — the caller then has
    no exemptions at all, which is the fail-closed direction."""
    if not os.path.exists(CONTRACT_DELETIONS_MD):
        return {}, []
    src = _oracle_src()
    if not os.path.isdir(src):
        return {}, []
    oracle = os.path.dirname(src)

    # One read of the oracle test corpus, kept per source DIRECTORY so arm 1 can
    # be scoped to the crate the contract entry belongs to. Scope matters: a
    # tree-wide name search rejects `datastruct::get` forever, because
    # `src/terminal/c/key_event.zig` declares an unrelated `test "get"`.
    by_dir = {}
    for dirpath, _dirs, files in os.walk(src):
        rel = os.path.relpath(dirpath, src)
        rel = "" if rel == "." else rel
        for fn in files:
            if not fn.endswith(".zig"):
                continue
            try:
                with open(os.path.join(dirpath, fn), encoding="utf-8",
                          errors="replace") as fh:
                    by_dir.setdefault(rel, []).append(fh.read())
            except OSError:
                continue
    by_dir = {d: "\n".join(t) for d, t in by_dir.items()}
    corpus = "\n".join(by_dir.values())

    # crate -> the src/ directories its contract files live in.
    try:
        crate_dirs = {}
        for cr, info in crate_map().items():
            crate_dirs[cr] = {os.path.dirname(f) for f, _n in info["files"]}
    except Exception:  # noqa: BLE001 — no map: fall back to the whole tree
        crate_dirs = {}

    rows, rejects = {}, []
    text = open(CONTRACT_DELETIONS_MD, encoding="utf-8", errors="replace").read()
    for m in _LEDGER_ROW.finditer(text):
        entry, sha, succ_cell = m.group(1), m.group(2), m.group(3)
        if "::" not in entry:
            continue
        crate, name = entry.split("::", 1)
        successors = re.findall(r"`([^`]+)`", succ_cell)

        # (1) exact absence from the crate's own upstream directories
        scope = "\n".join(by_dir.get(d, "") for d in crate_dirs[crate]) \
            if crate in crate_dirs else corpus
        if _test_decl_re(name).search(scope):
            rejects.append((entry, "the test still exists upstream"))
            continue
        # (2) the commit is reachable from the oracle's HEAD
        if oracle_git(oracle, "merge-base", "--is-ancestor",
                      sha, "HEAD").returncode != 0:
            rejects.append((entry, f"{sha} is not an ancestor of the oracle HEAD"))
            continue
        # (3) that commit removed exactly this declaration
        diff = oracle_git(oracle, "show", "--format=", "-U0", sha, "--", "src",
                          text=True).stdout
        if not _test_decl_re(name, prefix="-").search(diff):
            rejects.append((entry, f"{sha} does not remove `test \"{name}\"`"))
            continue
        # (3b) …and did not merely MOVE it: a commit that re-adds the same
        # declaration elsewhere deleted nothing, and the entry is still owed.
        if _test_decl_re(name, prefix=r"\+").search(diff):
            rejects.append((entry, f"{sha} re-adds `test \"{name}\"` elsewhere "
                                   f"(a move, not a deletion)"))
            continue
        # (4) every named successor exists now and was added by that commit
        bad = None
        for s in successors:
            if not _test_decl_re(s).search(corpus):
                bad = f"successor `{s}` does not exist upstream"
                break
            if not _test_decl_re(s, prefix=r"\+").search(diff):
                bad = f"{sha} does not add successor `{s}`"
                break
        if bad:
            rejects.append((entry, bad))
            continue
        rows[entry] = {"sha": sha, "successors": successors,
                       "crate": crate, "name": name}
    return rows, rejects


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
