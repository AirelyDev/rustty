#!/usr/bin/env python3
"""
check_parity_surface.py — GATE G9 (full surface coverage) [R2]

DoP id:  G9  (DEFINITION_OF_PARITY §1, "Full surface coverage (config / keybind /
              action / terminfo)") + the R2-terminfo-cap-parity replacement.
Reads:   .port/phase-0/inventory/parity-surface.json  (every config option,
              keybind action, cli action, terminfo capability with a stable id)
Scans:   crates/<owner>/src/**/*.rs   (presence of each surface id in the ported
              Rust, matched by token; owners per PORT_PLAN: config->rustty-config,
              keybind/cli actions->rustty-input/rustty-cli, terminfo->rustty-terminfo)

PASS CONDITION (DoP G9):
  - every surface id is present (ported) AND has >= 1 parity test; the uncovered
    set is empty; exit 0. A missing option/keybind/action/cap is a blocker, listed
    as  UNCOVERED <kind>: <id>.

MODES (because the workspace is incrementally ported, the same "claimed-done"
semantics apply per owning crate):
  default (progress): reports covered/total per surface kind. The empty workspace
    reports 0/N WITHOUT erroring (exit 0), UNLESS the OWNING crate is claimed-done
    and a surface id it owns is uncovered (then RED with UNCOVERED list).
  --strict: full G9 — every surface id must be present (and test-covered when
    --require-tests) regardless of crate status; the uncovered set must be empty.
  --kind <config|keybind|cli|terminfo|modes|sgr|osc|csi|esc|c0>: scope to one kind.
  --require-tests: additionally require >=1 ported test token-referencing each id
    (default off in progress mode; on in --strict).
  --self-test: built-in flip proof.

"Present" = the id's stable token (option name / action name / cap name) appears
in the owning crate's ported Rust as an identifier (snake/camel tolerant). This
is a coverage floor, not a semantic check — the byte-exact behaviour is proven by
G4 differential. For terminfo, when `infocmp rustty` is available the gate also
diffs the compiled cap set against the surface (R2-terminfo-cap-parity).

STRUCTURAL KINDS (N523, N524). A token search cannot prove these, so they are
read from the Rust structure instead:
  csi / esc / c0: the ids are the finals of `csi_handlers.arms` / `esc_handlers.arms`
    (plus `else` when `has_else_unimplemented`) and the `c0_controls` names. An id
    is covered iff it is a top-level arm pattern of the Rust dispatch match named in
    DISPATCH_SPECS (`csi_dispatch`, `esc_dispatch`, `execute`), read by
    `dispatch_arms`. They carry no test-reference arm (a final byte has no name a
    test could reference; G4 proves the behaviour); their line says so.
  osc: an id is covered iff the Rust `osc::Command` enum (OSC_ENUM_SPEC) has a
    variant matching it under `_gatelib.name_candidates` (a common word such as
    `unknown` elsewhere in the crate no longer counts). Its --require-tests arm is
    unchanged.
A dispatch fn, match, enum or arm pattern the matcher cannot read is BLOCKED
(exit 3), never a silent pass.
"""
import argparse
import os
import re
import shutil
import subprocess
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import _gatelib as G  # noqa: E402


def _parity_names(root):
    """Load the port<->oracle name map (rebrand Task 0,
    .port/tools/PARITY_NAMING.md) so this gate never hardcodes either side's
    name literal — the terminfo entry probed via `infocmp` is the PORT name.
    Falls back to the pre-Task-0 hardcoded defaults if the map is absent (e.g.
    the G9 self-test's synthetic tmp tree, which has no .port/tools/ of its
    own) so this loader can never turn a missing-map case into a crash."""
    d = {"PORT_NAME": "rustty", "ORACLE_NAME": "ghostty"}
    try:
        with open(os.path.join(root, ".port/tools/parity_names.env")) as f:
            for line in f:
                line = line.strip()
                if line and not line.startswith("#") and "=" in line:
                    k, v = line.split("=", 1)
                    d[k] = v
    except OSError:
        pass
    return d


PARITY_NAMES = _parity_names(G.RUSTTY_ROOT)

# kind -> (owning crate keys, accessor that yields list of ids from surface json)
OWNERS = {
    "config": ["config"],
    "keybind": ["input"],
    "cli": ["cli"],
    "terminfo": ["terminfo"],
    "modes": ["vt"],
    "sgr": ["vt"],
    "osc": ["vt"],
    "csi": ["vt"],
    "esc": ["vt"],
    "c0": ["vt"],
}


def surface_ids(surface):
    """Return {kind: [ids]} from parity-surface.json."""
    out = {}
    co = surface.get("config_options", {})
    uf = co.get("user_facing", [])
    internal = co.get("internal", [])
    out["config"] = [o["name"] for o in uf] + [o["name"] for o in internal if isinstance(o, dict) and "name" in o]
    out["keybind"] = [a["name"] for a in surface.get("keybind_actions", {}).get("actions", [])]
    out["cli"] = [a["name"] for a in surface.get("cli_actions", {}).get("actions", [])]
    tf = surface.get("terminfo", {})
    out["terminfo"] = [c["name"] for c in tf.get("capabilities", [])]
    out["modes"] = [str(m.get("name", m)) for m in surface.get("terminal_modes", {}).get("modes", [])] \
        if isinstance(surface.get("terminal_modes", {}).get("modes", []), list) else []
    out["sgr"] = [str(a.get("name", a)) for a in surface.get("sgr_attributes", {}).get("attributes", [])]
    osc = surface.get("osc_handlers", {})
    out["osc"] = [str(k) for k in osc.get("command_kinds", [])] if isinstance(osc.get("command_kinds", []), list) else []
    # N523: the CSI / ESC finals (each final of each top-level dispatch arm, in
    # arm order, once; `else` for the fallback arm) and the C0 control names.
    for kind, key in (("csi", "csi_handlers"), ("esc", "esc_handlers")):
        sec = surface.get(key, {})
        finals = []
        for arm in sec.get("arms", []):
            for f in arm.get("finals", []):
                if f not in finals:
                    finals.append(f)
        if sec.get("has_else_unimplemented") and "else" not in finals:
            finals.append("else")
        out[kind] = finals
    out["c0"] = [c["name"] for c in surface.get("c0_controls", {}).get("controls", [])]
    return out


def crate_token_corpus(crates, relocations=None):
    """Concatenate all ported Rust source of the given crates, comment-stripped
    but with STRING LITERALS KEPT, for token presence checks. Cap/action ids
    live in `b"..."` literals (id_variants' documented quoted-string form);
    blanking strings alongside comments made every literal-only id a false
    MISSING (the terminfo 13/268 false RED once the crate claimed done).
    Comments stay stripped so a verbatim-commented Zig block can never fake
    coverage (tightening preserved)."""
    blobs = []
    for crate in crates:
        # Own src/ plus the crate's R4 relocations (_gatelib.RELOCATIONS).
        for _path, text in G.iter_rust_sources(crate, relocations):
            clean, _ = G._strip_rust(text, keep_strings=True)
            blobs.append(clean)
    return "\n".join(blobs)


def crate_test_corpus(crates, relocations=None):
    """Same as crate_token_corpus but only the bodies of #[test] fns (so a
    surface id is 'test-covered' if it appears inside a ported test).

    Structure (attrs/fn names/brace spans) is computed on the fully-stripped
    text so braces inside string literals cannot desync the block finder; the
    emitted body slice comes from the strings-kept text (both strippers are
    offset-preserving, so the spans map 1:1)."""
    blobs = []
    for crate in crates:
        # Own src/ plus the crate's R4 relocations (_gatelib.RELOCATIONS).
        for _path, text in G.iter_rust_sources(crate, relocations):
            clean, _ = G._strip_rust(text)
            kept, _ = G._strip_rust(text, keep_strings=True)
            for m in G.TEST_ATTR_RE.finditer(clean):
                fm = G.FN_NAME_RE.search(clean, m.end())
                if not fm:
                    continue
                brace = clean.find("{", fm.end())
                if brace == -1:
                    continue
                end = G._find_balanced_block(clean, brace)
                blobs.append(kept[brace:end])
    return "\n".join(blobs)


# Sanctioned per-id renames: Zig ids that are NOT legal Rust identifiers
# (digit-leading `@"..."` union fields) port under an explicitly chosen name.
# This table is exact-match (tighter than a fuzzy candidate form: the gate
# demands the one identifier the port committed to, with file evidence).
# Every entry cites the ported site carrying the verbatim `// "<zig id>"`
# marker. Adding an entry requires that marker to exist at the cited site.
SANCTIONED_RENAMES = {
    # rustty-vt/src/sgr.rs — Zig `@"..."` digit-leading Attribute fields.
    "256_underline_color": "Color256Underline",  # sgr.rs:142 // "256_underline_color"
    "8_bg": "Color8Bg",                          # sgr.rs:172 // "8_bg"
    "8_fg": "Color8Fg",                          # sgr.rs:173 // "8_fg"
    "8_bright_bg": "Color8BrightBg",             # sgr.rs:180 // "8_bright_bg"
    "8_bright_fg": "Color8BrightFg",             # sgr.rs:181 // "8_bright_fg"
    "256_bg": "Color256Bg",                      # sgr.rs:184 // "256_bg"
    "256_fg": "Color256Fg",                      # sgr.rs:187 // "256_fg"
}


def id_variants(name):
    """Token variants to look for: the literal, snake_case, camelCase, and the
    quoted-string form (cap names / cli action names often appear as strings).

    Delegates to _gatelib.name_candidates — the single G1 naming convention,
    including the composed port-slug form (non-alnum split, camel-split,
    lowercase, '_'-join) — so both gates share one matcher (H2/H4). Keeps the
    raw hyphen/space -> underscore form on top (cap/action ids that retain
    other punctuation verbatim), plus the SANCTIONED_RENAMES exact form for
    digit-leading Zig ids that cannot be Rust identifiers. Strictly a superset
    of the old variant set: more candidate forms can only turn false MISSING
    into matches; presence is still required for every id (tightening-safe,
    R5)."""
    out = set(G.name_candidates(name))
    # hyphen/space -> underscore on the raw token too
    out.add(re.sub(r"[-\s]", "_", name))
    if name in SANCTIONED_RENAMES:
        out.add(SANCTIONED_RENAMES[name])
    return {v for v in out if v}


def present(name, blob):
    for v in id_variants(name):
        if re.search(r"\b" + re.escape(v) + r"\b", blob):
            return True
    # also try the raw name as a substring inside a string-literal-ish context
    return name in blob


# ---------------------------------------------------------------------------
# Structural kinds (N523 csi/esc/c0, N524 osc)
# ---------------------------------------------------------------------------
# A final byte is one character, so a token search proves nothing (`m` occurs
# everywhere). These kinds are checked against the Rust dispatch STRUCTURE:
# kind -> [(file relative to the repository root, fn name, match scrutinee)].
# `csi_dispatch_final` (the parser fast path in the same file) has no switch
# on the final: it forwards every final to `csi_dispatch`, so only
# `csi_dispatch`'s match is read.
DISPATCH_SPECS = {
    "csi": [("crates/rustty-vt/src/stream/csi_dispatch.rs", "csi_dispatch", "input.final_")],
    "esc": [("crates/rustty-vt/src/stream/dispatch.rs", "esc_dispatch", "action.final_")],
    "c0": [("crates/rustty-vt/src/stream/dispatch.rs", "execute", "c0")],
}
# The osc kinds are the variants of this enum (file, enum name).
OSC_ENUM_SPEC = ("crates/rustty-vt/src/osc/mod.rs", "Command")

_PAT_TOKEN = re.compile(r"""
    (?P<byte>b'(?:\\x[0-9A-Fa-f]{2}|\\.|[^'\\])')
  | (?P<int>(?:0x[0-9A-Fa-f_]+|0o[0-7_]+|0b[01_]+|[0-9][0-9_]*)(?:u8|u16|u32|u64|usize|i32)?)
  | (?P<range>\.\.=)
  | (?P<bar>\|)
  | (?P<path>[A-Za-z_][A-Za-z0-9_]*(?:::[A-Za-z_][A-Za-z0-9_]*)*)
  | (?P<ws>\s+)
""", re.X)
_BYTE_ESCAPES = {"n": 0x0A, "r": 0x0D, "t": 0x09, "0": 0x00, "\\": 0x5C, "'": 0x27, '"': 0x22}


def _line_of(text, offset):
    return text.count("\n", 0, offset) + 1


def _find_unique(clean, regex, what, rel, lo=0, hi=None):
    hits = list(re.compile(regex).finditer(clean, lo, len(clean) if hi is None else hi))
    if len(hits) != 1:
        raise G.Blocked(f"{rel}: expected exactly one {what}, found {len(hits)} "
                        "(the G9 structural matcher must be updated with the code)")
    return hits[0]


def dispatch_arms(text, rel, fn_name, scrutinee):
    """THE structural matcher for G9's csi/esc/c0 kinds (N523).

    Reads the Rust source `text` (of `rel`): locates `fn <fn_name>` (exactly
    one), then inside its body `match <scrutinee> {` (exactly one), and returns
    that match's TOP-LEVEL arms, in order, as dicts
    {pattern, start, end, line}: `pattern` is the arm's pattern source (comments
    removed, literals kept), `start`/`end` the arm's offsets (pattern start to
    just past the arm and its comma). Arms of a match nested inside an arm body
    (the per-final `match input.intermediates.len()` and so on) are not arms of
    the dispatch and are skipped by construction. Structure is read on the
    fully-stripped text (char literals such as `b'{'` blanked, so braces and
    `|`/`,` inside them cannot desync it); the pattern text comes from the
    strings-kept text (both strippers preserve offsets). Missing fn or match:
    G.Blocked."""
    clean, _ = G._strip_rust(text)
    kept, _ = G._strip_rust(text, keep_strings=True)
    fm = _find_unique(clean, r"\bfn\s+" + re.escape(fn_name) + r"\b", f"`fn {fn_name}`", rel)
    fopen = clean.find("{", fm.end())
    if fopen == -1:
        raise G.Blocked(f"{rel}: `fn {fn_name}` has no body")
    fend = G._find_balanced_block(clean, fopen)
    scrut = r"\s*\.\s*".join(re.escape(p) for p in scrutinee.split("."))
    mm = _find_unique(clean, r"\bmatch\s+" + scrut + r"\s*\{", f"`match {scrutinee}` in `fn {fn_name}`",
                      rel, fopen, fend)
    mopen = mm.end() - 1
    mend = G._find_balanced_block(clean, mopen) - 1  # index of the closing `}`
    arms = []
    i = mopen + 1
    while True:
        while i < mend and clean[i].isspace():
            i += 1
        if i >= mend:
            break
        # Pattern: up to `=>` at depth 0.
        j, depth = i, 0
        while j < mend:
            ch = clean[j]
            if ch in "([{":
                depth += 1
            elif ch in ")]}":
                depth -= 1
            elif depth == 0 and clean.startswith("=>", j):
                break
            j += 1
        if j >= mend:
            raise G.Blocked(f"{rel}:{_line_of(text, i)}: unparsable arm in `match {scrutinee}`")
        pattern = kept[i:j]
        # Body: an expression ends at `,` at depth 0; a block-like body ends at
        # its closing `}` unless `.`/`?`/`else`/`as` continues the expression.
        k, depth = j + 2, 0
        while k < mend:
            ch = clean[k]
            if ch in "([{":
                depth += 1
            elif ch in ")]}":
                depth -= 1
                if depth == 0 and ch == "}":
                    m = k + 1
                    while m < mend and clean[m].isspace():
                        m += 1
                    if m < mend and clean[m] == ",":
                        k = m + 1
                        break
                    if not (clean[m:m + 1] in (".", "?") or re.match(r"(else|as)\b", clean[m:])):
                        k += 1
                        break
            elif depth == 0 and ch == ",":
                k += 1
                break
            k += 1
        arms.append({"pattern": pattern, "start": i, "end": k, "line": _line_of(text, i)})
        i = k
    return arms


def _byte_value(tok):
    body = tok[2:-1]
    if not body.startswith("\\"):
        return ord(body)
    if body[1] == "x":
        return int(body[2:], 16)
    if body[1:] in _BYTE_ESCAPES:
        return _BYTE_ESCAPES[body[1:]]
    raise ValueError(tok)


def pattern_values(pattern):
    """Alternatives of one arm pattern: a list of ints (byte/integer literals,
    inclusive ranges expanded), path strings (`ansi::C0::NUL`) and "else" (`_`).
    A guard (`if ...`) is ignored. Anything else raises ValueError."""
    toks = []
    pos = 0
    while pos < len(pattern):
        m = _PAT_TOKEN.match(pattern, pos)
        if not m:
            raise ValueError(pattern)
        pos = m.end()
        kind = m.lastgroup
        if kind == "ws":
            continue
        if kind == "path" and m.group() == "if":
            break
        toks.append((kind, m.group()))
    out = []
    alt = []
    for kind, tok in toks + [("bar", "|")]:
        if kind != "bar":
            alt.append((kind, tok))
            continue
        if len(alt) == 1:
            k, t = alt[0]
            if k == "byte":
                out.append(_byte_value(t))
            elif k == "int":
                out.append(int(re.sub(r"(u8|u16|u32|u64|usize|i32)$", "", t).replace("_", ""), 0))
            elif k == "path" and t == "_":
                out.append("else")
            elif k == "path":
                out.append(t)
            else:
                raise ValueError(pattern)
        elif len(alt) == 3 and alt[1][0] == "range" and {alt[0][0], alt[2][0]} <= {"byte", "int"}:
            lo, hi = (pattern_values(alt[0][1])[0], pattern_values(alt[2][1])[0])
            out.extend(range(lo, hi + 1))
        elif alt or out:
            raise ValueError(pattern)
        alt = []
    if not out:
        raise ValueError(pattern)
    return out


def structural_finals(kind, surface):
    """The set of ids of `kind` (csi/esc: final characters and "else"; c0:
    control names) that the Rust dispatch handles, per DISPATCH_SPECS, read by
    dispatch_arms. A pattern the matcher cannot read is G.Blocked, never a
    silent miss: extend pattern_values to the spelling the code uses."""
    c0_names = {}
    if kind == "c0":
        for c in surface.get("c0_controls", {}).get("controls", []):
            c0_names[int(c["value"], 16)] = c["name"]
    handled = set()
    for rel, fn_name, scrutinee in DISPATCH_SPECS[kind]:
        path = os.path.join(G.RUSTTY_ROOT, rel)
        try:
            with open(path, encoding="utf-8") as fh:
                text = fh.read()
        except OSError as e:
            raise G.Blocked(f"{rel}: cannot read ({e})")
        for arm in dispatch_arms(text, rel, fn_name, scrutinee):
            try:
                values = pattern_values(arm["pattern"])
            except ValueError:
                raise G.Blocked(f"{rel}:{arm['line']}: arm pattern {arm['pattern'].strip()!r} "
                                f"is not a spelling the G9 matcher reads for {kind}")
            for v in values:
                if v == "else":
                    handled.add("else")
                elif isinstance(v, int):
                    handled.add(c0_names.get(v, f"0x{v:02X}") if kind == "c0" else chr(v))
                elif kind == "c0":
                    handled.add(v.rsplit("::", 1)[-1])
                else:
                    raise G.Blocked(f"{rel}:{arm['line']}: arm pattern {v!r} is a named "
                                    f"constant the G9 matcher cannot resolve for {kind}")
    return handled


def osc_enum_variants():
    """N524: the variant names of the Rust `osc::Command` enum
    (OSC_ENUM_SPEC), read from the enum body: each top-level comma-separated
    item's leading identifier, attributes and comments skipped."""
    rel, enum_name = OSC_ENUM_SPEC
    try:
        with open(os.path.join(G.RUSTTY_ROOT, rel), encoding="utf-8") as fh:
            text = fh.read()
    except OSError as e:
        raise G.Blocked(f"{rel}: cannot read ({e})")
    clean, _ = G._strip_rust(text)
    em = _find_unique(clean, r"\benum\s+" + re.escape(enum_name) + r"\b", f"`enum {enum_name}`", rel)
    eopen = clean.find("{", em.end())
    eend = G._find_balanced_block(clean, eopen) - 1
    variants = []
    item_start = eopen + 1
    depth = 0
    for i in range(eopen + 1, eend + 1):
        ch = clean[i]
        if ch in "([{<":
            depth += 1
        elif ch in ")]}>" and i < eend:
            depth -= 1
        if (ch == "," and depth == 0) or i == eend:
            item = re.sub(r"#\s*\[[^\]]*\]", " ", clean[item_start:i])
            m = re.match(r"\s*([A-Za-z_][A-Za-z0-9_]*)", item)
            if m:
                variants.append(m.group(1))
            item_start = i + 1
    if not variants:
        raise G.Blocked(f"{rel}: `enum {enum_name}` has no variants the G9 matcher can read")
    return variants


def osc_variant_for(kind_id, variants):
    """The osc::Command variant that ports kind `kind_id`: the one whose name
    matches it under G1's naming convention (_gatelib.name_candidates, either
    direction), or None. An osc kind is covered iff this is not None."""
    cands = G.name_candidates(kind_id)
    for v in variants:
        if v in cands or kind_id in G.name_candidates(v):
            return v
    return None


def check_terminfo_infocmp(surface):
    """If `infocmp <PORT_NAME>` is available, diff its cap set against the
    surface (R2-terminfo-cap-parity). Returns (available, missing_caps_list, note)."""
    port_name = PARITY_NAMES.get("PORT_NAME", "rustty")
    if shutil.which("infocmp") is None:
        return False, [], "infocmp not on PATH"
    try:
        r = subprocess.run(["infocmp", "-1", port_name], capture_output=True, text=True)
    except Exception as e:  # noqa: BLE001
        return False, [], f"infocmp error: {e}"
    if r.returncode != 0:
        return False, [], f"{port_name} terminfo entry not compiled/installed yet"
    compiled = set()
    for line in r.stdout.splitlines():
        line = line.strip().rstrip(",")
        if not line or line.startswith("#") or "|" in line:
            continue
        capname = re.split(r"[=#@]", line)[0].strip()
        if capname:
            compiled.add(capname)
    want = {c["name"] for c in surface.get("terminfo", {}).get("capabilities", [])}
    missing = sorted(want - compiled)
    return True, missing, f"infocmp compared ({len(compiled)} caps present)"


def run(args):
    G.banner("G9", "full surface coverage (config/keybind/action/terminfo)")
    try:
        surface = G.load_parity_surface()
        G.crate_map()  # ensure crate_map loads (for owner crates)
    except G.Blocked as b:
        G.emit(G.BLOCKED_STATE, str(b))
        return 3

    ids = surface_ids(surface)
    kinds = [args.kind] if args.kind else list(OWNERS.keys())
    for k in kinds:
        if k not in OWNERS:
            G.emit(G.RED, f"unknown surface kind '{k}'")
            return 2

    # R4 relocations (_gatelib.RELOCATIONS): a stale entry is BLOCKED up front.
    try:
        G.check_relocations(sorted({o for k in kinds for o in OWNERS[k]}))
    except G.Blocked as b:
        G.emit(G.BLOCKED_STATE, str(b))
        return 3

    require_tests = args.require_tests or args.strict
    red = False
    grand_total = 0
    grand_covered = 0
    for kind in kinds:
        owners = OWNERS[kind]
        kind_ids = ids.get(kind, [])
        if not kind_ids:
            print(f"{kind:<10}: (no ids in parity-surface.json)")
            continue
        owners_done = all(G.is_claimed_done(o) for o in owners)
        # Structural kinds (N523, N524): presence is read from the dispatch
        # match arms / the osc::Command enum, never from a token search.
        handled = variants = None
        try:
            if kind in DISPATCH_SPECS:
                handled = structural_finals(kind, surface)
            elif kind == "osc":
                variants = osc_enum_variants()
        except G.Blocked as b:
            G.emit(G.BLOCKED_STATE, str(b))
            return 3
        blob = crate_token_corpus(owners) if handled is None and variants is None else ""
        # A final byte has no name a test could reference, so the csi/esc/c0
        # kinds carry no test-reference arm (stated on their line, not
        # skipped silently); their behaviour is G4's differential.
        kind_tests = require_tests and handled is None
        test_blob = crate_test_corpus(owners) if kind_tests else ""

        uncovered = []
        covered = 0
        for name in kind_ids:
            if handled is not None:
                is_present = name in handled
            elif variants is not None:
                variant = osc_variant_for(name, variants)
                is_present = variant is not None
            else:
                is_present = present(name, blob)
            # The --require-tests arm is unchanged (token in a #[test] body);
            # only presence is structural (N524).
            is_tested = (not kind_tests) or present(name, test_blob)
            if is_present and is_tested:
                covered += 1
            else:
                uncovered.append(name)
        grand_total += len(kind_ids)
        grand_covered += covered

        # RED logic: in strict, any uncovered id is RED. In progress, only RED
        # when the owning crate(s) are claimed-done.
        kind_red = False
        if args.strict and uncovered:
            kind_red = True
        elif owners_done and uncovered:
            kind_red = True
        if kind_red:
            red = True

        state = "RED" if kind_red else ("done" if owners_done else "progress")
        how = ""
        if handled is not None:
            how = "  [structural: dispatch match arms; test-reference n/a]"
        elif variants is not None:
            how = "  [structural: osc::Command variants]"
        print(f"{kind:<10}: covered {covered}/{len(kind_ids)}  owners={','.join(owners)} "
              f"({'done' if owners_done else 'stub'})  -> {state}{how}")
        if kind_red:
            for nm in uncovered[:40]:
                print(f"    UNCOVERED {kind}: {nm}")
            if len(uncovered) > 40:
                print(f"    UNCOVERED {kind}: ... (+{len(uncovered) - 40} more)")

    # terminfo cap parity via infocmp (R2-terminfo-cap-parity)
    if (not args.kind) or args.kind == "terminfo":
        avail, missing, note = check_terminfo_infocmp(surface)
        print(f"terminfo  : infocmp-parity: {note}")
        if avail and missing:
            owners_done = G.is_claimed_done("terminfo")
            if args.strict or owners_done:
                red = True
                for c in missing[:40]:
                    print(f"    UNCOVERED terminfo-cap: {c}")

    print(f"\nPROGRESS: surface ids covered {grand_covered}/{grand_total}")

    if args.strict:
        if red:
            G.emit(G.RED, "surface ids uncovered (see UNCOVERED above)")
            return 1
        G.emit(G.GREEN, f"all {grand_total} surface ids covered + test-referenced")
        return 0

    if red:
        G.emit(G.RED, "a claimed-done owner crate has uncovered surface ids")
        return 1
    G.emit(G.GREEN, f"progress {grand_covered}/{grand_total}; no claimed-done owner uncovered")
    return 0


def self_test():
    import tempfile
    import json

    print("=== G9 SELF-TEST ===")
    tmp = tempfile.mkdtemp(prefix="g9_selftest_")
    gates = os.path.join(tmp, ".port", "tools", "gates")
    inv = os.path.join(tmp, ".port", "phase-0", "inventory")
    cfg_src = os.path.join(tmp, "crates", "rustty-config", "src")
    os.makedirs(gates)
    os.makedirs(inv)
    os.makedirs(cfg_src)
    here = os.path.dirname(os.path.abspath(__file__))
    shutil.copy(os.path.join(here, "_gatelib.py"), gates)
    shutil.copy(os.path.join(here, "check_parity_surface.py"), gates)

    summary = {
        "crate_map": {
            "config": {"files": [{"file": "config/config.zig", "test_blocks": 0}],
                       "test_blocks": 0, "file_count": 1},
        },
    }
    with open(os.path.join(inv, "tests-summary.json"), "w") as fh:
        json.dump(summary, fh)
    surface = {
        "config_options": {"user_facing": [{"name": "font-size"}, {"name": "background"}],
                           "internal": []},
        "keybind_actions": {"actions": []},
        "cli_actions": {"actions": []},
        "terminfo": {"capabilities": []},
        "terminal_modes": {"modes": []},
        "sgr_attributes": {"attributes": []},
        "osc_handlers": {"command_kinds": []},
    }
    with open(os.path.join(inv, "parity-surface.json"), "w") as fh:
        json.dump(surface, fh)

    def write_cfg(status, present_ids):
        body = f"//! config\n//! PORT STATUS: {status}\n"
        for pid in present_ids:
            field = pid.replace("-", "_")
            body += f"pub struct C {{ pub {field}: u32 }}\n#[test]\nfn t_{field}() {{ let _ = C {{ {field}: 0 }}; }}\n"
        with open(os.path.join(cfg_src, "lib.rs"), "w") as fh:
            fh.write(body)

    script = os.path.join(gates, "check_parity_surface.py")

    def invoke(*extra):
        return subprocess.run([sys.executable, script, "--kind", "config", *extra],
                              capture_output=True, text=True)

    ok = True
    # (a) stub crate, no ids present => progress GREEN
    write_cfg("stub", [])
    r = invoke()
    print(f"[a] stub 0/2 progress -> exit {r.returncode} (expect 0)")
    ok &= (r.returncode == 0)

    # (b) done crate, missing 'background' => RED (UNCOVERED)
    write_cfg("done", ["font-size"])
    r = invoke()
    print(f"[b] done 1/2 -> exit {r.returncode} (expect non-zero), UNCOVERED: {'UNCOVERED' in r.stdout}")
    ok &= (r.returncode != 0 and "UNCOVERED" in r.stdout)

    # (c) done crate, both present + tested => GREEN
    write_cfg("done", ["font-size", "background"])
    r = invoke()
    print(f"[c] done 2/2 -> exit {r.returncode} (expect 0)")
    ok &= (r.returncode == 0)

    # (d) strict, stub crate, missing => RED (strict ignores crate status)
    write_cfg("stub", ["font-size"])
    r = invoke("--strict")
    print(f"[d] strict stub 1/2 -> exit {r.returncode} (expect non-zero)")
    ok &= (r.returncode != 0)

    # ---- R4 relocation arms (S-8c): 'background' is ported only in a HOST
    # crate's file that the config crate's relocation names.
    host_src = os.path.join(tmp, "crates", "rustty-host", "src")
    os.makedirs(host_src)
    with open(os.path.join(host_src, "moved.rs"), "w") as fh:
        fh.write("pub struct M { pub background: u32 }\n#[test]\n"
                 "fn t_background() { let _ = M { background: 0 }; }\n")
    reloc = {"config": ({"path": "crates/rustty-host/src/moved.rs"},)}
    write_cfg("done", ["font-size"])

    # (e) no relocation: the moved id is UNCOVERED => RED
    G.selftest_install_gatelib(gates, {})
    r = invoke()
    print(f"[e] done, id only in relocated file, no relocation -> exit "
          f"{r.returncode} (expect non-zero)")
    ok &= (r.returncode != 0 and "UNCOVERED config: background" in r.stdout)

    # (f) with the relocation: covered => GREEN (also under --strict)
    G.selftest_install_gatelib(gates, reloc)
    r = invoke()
    r2 = invoke("--strict")
    print(f"[f] same + relocation -> exit {r.returncode}/{r2.returncode} (expect 0/0)")
    ok &= (r.returncode == 0 and r2.returncode == 0)

    # (g) with the relocation, a genuine drop is still caught
    write_cfg("done", [])
    r = invoke()
    print(f"[g] relocation present, 'font-size' dropped -> exit {r.returncode} "
          f"(expect non-zero)")
    ok &= (r.returncode != 0 and "UNCOVERED config: font-size" in r.stdout
           and "UNCOVERED config: background" not in r.stdout)

    # (h) a relocation path that does not exist => BLOCKED (exit 3)
    G.selftest_install_gatelib(gates, {"config": ({"path": "crates/gone.rs"},)})
    write_cfg("done", ["font-size", "background"])
    r = invoke()
    print(f"[h] relocation path missing -> exit {r.returncode} (expect 3)")
    ok &= (r.returncode == 3 and "BLOCKED" in r.stdout)

    ok &= _self_test_structural(tmp, gates, inv, here, surface, script)

    shutil.rmtree(tmp, ignore_errors=True)
    print("SELF-TEST:", "PASS" if ok else "FAIL")
    return 0 if ok else 1


def _self_test_structural(tmp, gates, inv, here, surface, script):
    """Arms (i)-(q): the structural kinds (N523 csi/esc/c0, N524 osc) on COPIES
    of the REAL Rust dispatch files and on an osc enum fixture. Each negative
    control removes one arm (or one variant) and must turn exactly that id RED,
    while the removed final's token stays elsewhere in the file, so a token
    match could not have caught it."""
    import json

    ok = True
    real_root = os.path.abspath(os.path.join(here, "..", "..", ".."))
    # Relocations off: the synthetic tree carries none of the real ones.
    G.selftest_install_gatelib(gates, {})
    vt_src = os.path.join(tmp, "crates", "rustty-vt", "src")
    os.makedirs(os.path.join(vt_src, "stream"))
    os.makedirs(os.path.join(vt_src, "osc"))
    with open(os.path.join(vt_src, "lib.rs"), "w") as fh:
        fh.write("//! vt\n//! PORT STATUS: done\n")
    copies = {}
    for spec in [s for k in DISPATCH_SPECS for s in DISPATCH_SPECS[k]]:
        rel = spec[0]
        with open(os.path.join(real_root, rel), encoding="utf-8") as fh:
            copies[rel] = fh.read()

    def write_tree(files):
        for rel, text in files.items():
            with open(os.path.join(tmp, rel), "w", encoding="utf-8") as fh:
                fh.write(text)

    osc_rel = OSC_ENUM_SPEC[0]
    osc_enum = (
        "/// the command\n#[derive(Debug)]\npub enum Command<'p> {\n"
        "    /// zero value\n    Invalid,\n"
        "    Unknown { data: &'p [u8] },\n"
        "    HyperlinkEnd,\n    ChangeWindowTitle(&'p [u8]),\n}\n"
        "// unknown hyperlink_end program_status: words only, never variants\n"
        "pub fn f() -> &'static str { \"unknown program_status\" }\n"
        "pub const UNKNOWN_PROGRAM_STATUS: u8 = 0; fn unknown() {}\n"
    )
    sf = dict(surface)
    sf["csi_handlers"] = {"has_else_unimplemented": True,
                          "arms": [{"finals": ["A", "k"]}, {"finals": ["H", "f"]},
                                   {"finals": ["m"]}, {"finals": ["else"]}]}
    sf["esc_handlers"] = {"has_else_unimplemented": True,
                          "arms": [{"finals": ["7"]}, {"finals": ["\\"]}, {"finals": ["B"]}]}
    sf["c0_controls"] = {"controls": [{"name": "NUL", "value": "0x00"},
                                      {"name": "BEL", "value": "0x07"},
                                      {"name": "LF", "value": "0x0A"}]}
    sf["osc_handlers"] = {"command_kinds": ["invalid", "unknown", "hyperlink_end",
                                            "change_window_title"]}
    with open(os.path.join(inv, "parity-surface.json"), "w") as fh:
        json.dump(sf, fh)

    # Default mode with the owner crate claimed done: an uncovered id is RED.
    def kind_run(kind):
        return subprocess.run([sys.executable, script, "--kind", kind],
                              capture_output=True, text=True)

    def uncovered(r, kind):
        return sorted(l.split(f"UNCOVERED {kind}: ", 1)[1] for l in r.stdout.splitlines()
                      if f"UNCOVERED {kind}: " in l)

    # (i) the unmodified copies: every fixture id is covered => GREEN.
    write_tree(copies)
    write_tree({osc_rel: osc_enum})
    rs = {k: kind_run(k) for k in ("csi", "esc", "c0", "osc")}
    print("[i] real csi/esc/c0 copies + osc fixture -> exit "
          + "/".join(str(rs[k].returncode) for k in rs) + " (expect 0/0/0/0)")
    ok &= all(r.returncode == 0 for r in rs.values())

    def drop_arm(rel, fn_name, scrutinee, kind, pattern):
        text = copies[rel]
        arms = dispatch_arms(text, rel, fn_name, scrutinee)
        hit = [a for a in arms if a["pattern"].strip() == pattern]
        if len(hit) != 1:
            return None
        a = hit[0]
        return text[:a["start"]] + text[a["end"]:]

    # (j)/(k)/(l): one arm removed from a copy of the real file => that id RED.
    for tag, kind, pattern, want, token in (
            ("j", "csi", "b'm'", "m", "b'm'"),
            ("k", "esc", "b'\\\\'", "\\", None),
            ("l", "c0", "ansi::C0::BEL", "BEL", None)):
        rel, fn_name, scrutinee = DISPATCH_SPECS[kind][0]
        mutated = drop_arm(rel, fn_name, scrutinee, kind, pattern)
        if mutated is None:
            print(f"[{tag}] could not locate the {pattern} arm in {rel} (expect one)")
            ok = False
            continue
        write_tree({rel: mutated})
        r = kind_run(kind)
        got = uncovered(r, kind)
        still = token is None or token in mutated
        print(f"[{tag}] {kind} copy without the {pattern} arm -> exit {r.returncode}, "
              f"UNCOVERED {got} (expect non-zero, [{want!r}]); token kept elsewhere: {still}")
        ok &= (r.returncode == 1 and got == [want] and still)
        write_tree({rel: copies[rel]})

    # (m) osc: an enum body without `Unknown` => RED on `unknown`, although the
    #     word `unknown` stays in the file (comment, string, fn and const names).
    write_tree({osc_rel: osc_enum.replace("    Unknown { data: &'p [u8] },\n", "")})
    r = kind_run("osc")
    got = uncovered(r, "osc")
    print(f"[m] osc enum without Unknown -> exit {r.returncode}, UNCOVERED {got} "
          f"(expect non-zero, ['unknown'])")
    ok &= (r.returncode == 1 and got == ["unknown"])
    write_tree({osc_rel: osc_enum})

    # (n) a dispatch fn that cannot be found => BLOCKED (never a silent pass).
    rel, fn_name, _ = DISPATCH_SPECS["esc"][0]
    write_tree({rel: copies[rel].replace(f"fn {fn_name}(", "fn gone_dispatch(")})
    r = kind_run("esc")
    print(f"[n] esc dispatch fn renamed -> exit {r.returncode} (expect 3)")
    ok &= (r.returncode == 3 and "BLOCKED" in r.stdout)
    write_tree({rel: copies[rel]})

    # (o) an arm pattern the matcher cannot read (a named const) => BLOCKED.
    rel, fn_name, scrutinee = DISPATCH_SPECS["csi"][0]
    write_tree({rel: copies[rel].replace("b'm' =>", "FINAL_M =>", 1)})
    r = kind_run("csi")
    print(f"[o] csi arm spelled as a named const -> exit {r.returncode} (expect 3)")
    ok &= (r.returncode == 3 and "FINAL_M" in r.stdout)
    write_tree({rel: copies[rel]})

    # (p) no `enum Command` in the osc file => BLOCKED.
    write_tree({osc_rel: osc_enum.replace("enum Command", "enum Other")})
    r = kind_run("osc")
    print(f"[p] osc enum Command missing -> exit {r.returncode} (expect 3)")
    ok &= (r.returncode == 3 and "BLOCKED" in r.stdout)
    write_tree({osc_rel: osc_enum})

    # (q) a nested `match` on intermediates inside an arm does not count: the
    #     `b'D'` csi arm holds `0 =>`, `1 =>` and `_ =>` sub-arms; dropping the
    #     whole top-level `_ =>` arm must make `else` RED.
    mutated = drop_arm(rel, fn_name, scrutinee, "csi", "_")
    if mutated is None:
        print("[q] could not locate the top-level _ arm (expect one)")
        ok = False
    else:
        write_tree({rel: mutated})
        r = kind_run("csi")
        got = uncovered(r, "csi")
        print(f"[q] csi copy without the top-level _ arm -> exit {r.returncode}, "
              f"UNCOVERED {got} (expect non-zero, ['else'])")
        ok &= (r.returncode == 1 and got == ["else"])
        write_tree({rel: copies[rel]})
    return ok


def main():
    ap = argparse.ArgumentParser(description="G9 — full surface coverage")
    ap.add_argument("--surface", help="(ignored; canonical path)")
    ap.add_argument("--rust-root", help="(ignored; canonical path)")
    ap.add_argument("--terminfo", help="(ignored; uses infocmp rustty)")
    ap.add_argument("--kind", help="scope to one surface kind")
    ap.add_argument("--require-tests", action="store_true",
                    help="require >=1 ported test referencing each id")
    ap.add_argument("--strict", action="store_true")
    ap.add_argument("--self-test", action="store_true")
    args = ap.parse_args()
    if args.self_test:
        sys.exit(self_test())
    sys.exit(run(args))


if __name__ == "__main__":
    main()
