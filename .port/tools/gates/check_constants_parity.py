#!/usr/bin/env python3
"""
check_constants_parity.py — Detector D3: constants-by-expression parity.

Kills Bun semantic-mismatch **Class 2b** (BUN_PORT_LEARNINGS.md §3.2 / §4 D3):
"placeholder constants ported by value silently shrink capacity 32x". In Bun a
derived Zig constant `const cap = count / 4;` (= 2048) was ported as a folded
literal `64` behind a `TODO(port)` marker — cutting a store ceiling ~32x and
making the Class-2a OOB reachable. The two rules D3 enforces:

  (a) MARKER GATE — zero surviving port-incompleteness markers in a claimed-done
      crate at its sub-batch gate (scoped per per-subbatch-scoped-g3b; never
      deferred to a finale). The unambiguous port markers `TODO(port)` /
      `FIXME(port)` / `XXX(port)` are zero-tolerance. Because the token
      "placeholder" is heavily overloaded in this codebase for a legitimate
      ARCHITECTURAL doctrine (INHABITED PLACEHOLDER structs, placeholder-leaf
      seams, Kitty/GPU placeholder cells — see memory placeholder-leaf-doctrine),
      a bare-word "placeholder" grep would flood with false positives and violate
      the detector's own purpose. So "placeholder" is flagged ONLY in its Class-2b
      shape: a **placeholder CONSTANT** — a `const`/`static` bound to a bare
      numeric literal whose OWN-LINE trailing comment is annotated "placeholder"
      (a doc/prose "placeholder" on a line ABOVE an unrelated real const does not
      flag — that overload is the doctrine, not the bug). That is exactly the Bun
      hazard and nothing else.

  (b) EXPRESSION-PARITY DIFF — extract numeric consts from each Zig reference file
      (phase-0 inventory `crate_map` supplies the paths; oracle at $ORACLE) and
      diff against the Rust twin. A Zig constant whose RHS is DERIVED (references
      another identifier — the `count / 4` shape) MUST appear in the Rust twin as
      the same const EXPRESSION, never a folded literal. A twin that folded the
      expression to a bare literal is the Class-2b divergence -> RED.

R5-clean, additive, never weakens a gate. Exit-code contract (mirrors ci.sh's
gate protocol and check_test_counts.py):
    0  PASS / progress (stub crates count for progress only, never error)
    3  BLOCKED — the ghostty oracle checkout is absent, so the (b) expression
       diff cannot bind here (R3: the oracle is an EXTERNAL reference, never
       shipped to CI). Surfaced by ci.sh as a WARNING, never a silent green.
       The (a) marker gate still binds and can still turn the run RED first.
    1  RED — a claimed-done crate carries a surviving port marker / placeholder
       constant, OR a derived Zig constant was folded to a literal in its twin.

Usage:
    check_constants_parity.py                 # all claimed-done workspace crates
    check_constants_parity.py --crate vt      # one crate (its sub-batch gate)
    check_constants_parity.py --all-done      # explicit: every claimed-done crate
    check_constants_parity.py --selftest      # prove the detector: folded->RED
    check_constants_parity.py -v              # also print advisory notes
"""
import os
import re
import sys
import tempfile

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import _gatelib as g  # noqa: E402  (shared gate helpers; read-only import)


def _parity_names(root):
    """Load the port<->oracle name map (rebrand Task 0,
    .port/tools/PARITY_NAMING.md) — the single sourced port<->oracle map so a
    rename never makes the port look artificially divergent from the oracle.
    Falls back to the pre-Task-0 hardcoded default if the map is absent so
    this loader can never turn a missing-map case into a crash."""
    d = {"ORACLE_TREE": "/Users/gael/projects/ghostty"}
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


PARITY_NAMES = _parity_names(g.RUSTTY_ROOT)

# R3: the ghostty oracle is a read-only EXTERNAL reference, never shipped to CI.
# Read from the checked-in .port/tools/parity_names.env (ORACLE_TREE) — NOT an
# environment variable: this is still the one committed source of truth
# ci.sh/check_test_counts.py mirror (no runtime env override; an override that
# redirects the probe away from the greped path is a vacuous-green spoof hole;
# see ci.sh header). Changing the oracle path still requires a repo edit to the
# sourced map, same trust boundary as editing the script itself.
ORACLE = PARITY_NAMES["ORACLE_TREE"]
ORACLE_SRC = os.path.join(ORACLE, "src")

# ---------------------------------------------------------------------------
# (a) Marker gate
# ---------------------------------------------------------------------------
# Unambiguous port-incompleteness markers: zero tolerance in a claimed-done crate.
PORT_MARKER_RE = re.compile(r"\b(?:TODO|FIXME|XXX|HACK)\(port\)", re.IGNORECASE)

# A Rust const/static bound to a BARE numeric literal (the folded-value shape).
# Captures name + the literal so the diff and the placeholder-const gate share it.
RUST_LITERAL_CONST_RE = re.compile(
    r"^\s*(?:pub\s*(?:\([^)]*\)\s*)?)?(?:const|static)\s+(?:mut\s+)?"
    r"([A-Za-z_]\w*)\s*:\s*[^=]+=\s*"
    r"(-?\s*(?:0x[0-9A-Fa-f_]+|0b[01_]+|0o[0-7_]+|[0-9][0-9_]*)"
    r"(?:_?(?:u|i)(?:8|16|32|64|128|size))?)\s*;"
)


def strip_line_comment(line):
    """Return (code, comment) splitting a Rust/Zig source line at the first `//`
    that is not inside a string/char literal. Good enough for const scanning."""
    in_s = in_c = False
    i = 0
    n = len(line)
    while i < n:
        ch = line[i]
        if ch == "\\" and (in_s or in_c):
            i += 2
            continue
        if ch == '"' and not in_c:
            in_s = not in_s
        elif ch == "'" and not in_s:
            in_c = not in_c
        elif ch == "/" and i + 1 < n and line[i + 1] == "/" and not in_s and not in_c:
            return line[:i], line[i + 2:]
        i += 1
    return line, None


def scan_markers(crate):
    """Return list of (relpath, lineno, kind, text) marker hits in a crate's
    Rust sources. `kind` is 'port-marker' or 'placeholder-const'."""
    hits = []
    for path in g.iter_rust_files(crate):
        try:
            with open(path, encoding="utf-8", errors="replace") as fh:
                lines = fh.readlines()
        except OSError:
            continue
        rel = os.path.relpath(path, g.RUSTTY_ROOT)
        for idx, raw in enumerate(lines, start=1):
            line = raw.rstrip("\n")
            code, comment = strip_line_comment(line)
            # (a.1) unambiguous port marker, anywhere on the line.
            if PORT_MARKER_RE.search(line):
                hits.append((rel, idx, "port-marker", line.strip()))
            # (a.2) placeholder CONSTANT: a const/static bound to a bare literal
            # whose OWN-LINE trailing comment says "placeholder". This is the
            # exact Class-2b shape; architectural "placeholder" prose on a line
            # above a real const is NOT this and is deliberately not flagged.
            m = RUST_LITERAL_CONST_RE.match(code)
            if m and "placeholder" in (comment or "").lower():
                hits.append((rel, idx, "placeholder-const",
                             f"{m.group(1)} = {m.group(2).strip()}  ({line.strip()})"))
    return hits


# ---------------------------------------------------------------------------
# (b) Expression-parity diff
# ---------------------------------------------------------------------------
# Zig uses `const` for BOTH file-scope constants and function-LOCAL immutable
# bindings (`const len = self.rows - x;`, the Rust `let` role). Only the former
# are the Class-2b target. A file-scope decl starts at column 0 (optionally after
# `pub `); a local binding is always indented inside a block. So we anchor the
# module-const regex at the start of line with NO leading whitespace. This alone
# drops every `const len = self.<field>` local-binding false positive.
ZIG_CONST_RE = re.compile(
    r"^(?:pub\s+)?const\s+([A-Za-z_]\w*)\s*(?::\s*[^=]+?)?\s*=\s*([^;]+);"
)
# An identifier immediately followed by `.` — a member/namespace access
# (`self.rows`, `windows.FILE_ATTRIBUTE_NORMAL`, `std.math.maxInt`). Such an RHS
# is NOT a peer-constant derivation: `windows.FOO` faithfully ports to its
# literal value (Rust has no `std.os.windows` namespace to alias), and `self.x`
# is a runtime field, not a const. Excluding member access is what keeps the
# detector pinned to the exact `count / 4` peer-derived-capacity shape.
MEMBER_ACCESS_RE = re.compile(r"[A-Za-z_]\w*\s*\.")

# A bare numeric literal (Zig or Rust flavour), the "folded" shape.
BARE_LITERAL_RE = re.compile(
    r"^-?\s*(?:0x[0-9A-Fa-f_]+|0b[01_]+|0o[0-7_]+|[0-9][0-9_]*(?:\.[0-9_]+)?)"
    r"(?:_?(?:u|i|f)(?:8|16|32|64|128|size)?)?$"
)

# Tokens that make an RHS a *type* / aggregate / non-numeric — reject as candidate.
_NON_NUMERIC_TOKENS = re.compile(
    r"@import|@This|@Type|@TypeOf|@field|struct\b|enum\b|union\b|opaque\b|"
    r"\bfn\b|error\b|\.\{|\bpacked\b|\bextern\b|\[|\"|\bundefined\b|\bnull\b|"
    r"\btrue\b|\bfalse\b|orelse|\.\.\."
)
_IDENT_RE = re.compile(r"[A-Za-z_]\w*")
# Zig numeric builtins that keep an RHS "derived" and numeric.
_NUMERIC_BUILTINS = {
    "sizeOf", "alignOf", "bitSizeOf", "intCast", "as", "min", "max", "divExact",
    "divFloor", "divTrunc", "shl", "shr", "mod", "rem", "clz", "ctz",
    "typeInfo", "enumToInt", "intFromEnum", "truncate", "sqrt", "abs",
}
# Non-const identifiers that are types/keywords, not derived-from-another-const.
_TYPE_WORDS = {
    "u2", "u3", "u4", "u5", "u6", "u7", "u8", "u16", "u21", "u24", "u32", "u48",
    "u64", "u128", "usize", "i8", "i16", "i32", "i64", "i128", "isize", "f16",
    "f32", "f64", "f80", "f128", "bool", "void", "comptime_int", "comptime_float",
    "c_int", "c_uint", "c_long", "c_ulong", "c_short", "type", "anytype",
}


def classify_zig_rhs(rhs):
    """Classify a Zig const RHS. Returns one of:
        'literal'  — a bare numeric literal (folding is value-preserving; soft).
        'derived'  — numeric-valued expression referencing >=1 other identifier
                     (the `count / 4` shape) — MUST stay an expression in Rust.
        None       — not a numeric-valued const (type/struct/import/string/...).
    Second element: set of referenced non-type identifiers (for 'derived')."""
    r = rhs.strip()
    if not r:
        return None, set()
    # early reject: any type/aggregate/import/string token means this is not a
    # numeric-valued const (structs, enums, arrays, strings, @import, ...).
    if _NON_NUMERIC_TOKENS.search(r):
        return None, set()
    if BARE_LITERAL_RE.match(r):
        return "literal", set()
    # strip @builtin names, keep their identifier arguments for ref analysis.
    # An RHS with @<name> where name not numeric-builtin -> reject.
    for bi in re.findall(r"@([A-Za-z_]\w*)", r):
        if bi not in _NUMERIC_BUILTINS:
            return None, set()
    # collect referenced identifiers (excluding @builtins, type words, numeric
    # suffixes already inside literals).
    body = re.sub(r"@[A-Za-z_]\w*", " ", r)
    idents = set()
    for tok in _IDENT_RE.findall(body):
        if tok in _TYPE_WORDS or tok in _NUMERIC_BUILTINS:
            continue
        # a lone type-suffix like the `u32` in `1u32` is glued to a digit -> the
        # regex still splits it; treat pure suffix words as types (handled above).
        idents.add(tok)
    # Must look numeric: only digits, idents, operators, parens, whitespace, dots.
    if re.search(r"[^\w\s+\-*/%<>&|^~().]", body):
        return None, set()
    if not re.search(r"[-+*/%<>&|^~]", r) and not idents:
        # no operator and no identifier -> shouldn't reach (literal caught above)
        return None, set()
    if idents:
        return "derived", idents
    # operator over pure literals (e.g. `1 << 11`): numeric expression but folding
    # is value-preserving. Report as a soft 'expr-literal' (advisory only).
    return "expr-literal", set()


def rust_consts(crate):
    """Return {NAME: (rhs_text, is_bare_literal, relpath, lineno)} over all Rust
    const/static defs in the crate. NAME is the raw Rust identifier."""
    out = {}
    # const/static NAME: TYPE = RHS ;  (RHS up to the terminating ; on the line)
    rc = re.compile(
        r"(?:pub\s*(?:\([^)]*\)\s*)?)?(?:const|static)\s+(?:mut\s+)?"
        r"([A-Za-z_]\w*)\s*:\s*[^=]+=\s*([^;]+);"
    )
    for path in g.iter_rust_files(crate):
        try:
            with open(path, encoding="utf-8", errors="replace") as fh:
                text = fh.read()
        except OSError:
            continue
        rel = os.path.relpath(path, g.RUSTTY_ROOT)
        for lineno, raw in enumerate(text.splitlines(), start=1):
            code, _ = strip_line_comment(raw)
            m = rc.search(code)
            if not m:
                continue
            name = m.group(1)
            rhs = m.group(2).strip()
            out.setdefault(name, (rhs, bool(BARE_LITERAL_RE.match(rhs)), rel, lineno))
    return out


def norm(name):
    """Normalize an identifier across case conventions: Zig `max_size` /
    camelCase vs Rust `MAX_SIZE` -> a case/underscore-insensitive key."""
    return re.sub(r"_", "", name).upper()


def zig_file_scope_consts(text):
    """Parse one Zig file's FILE-SCOPE numeric consts (col-0 decls only).
    Returns {name: (rhs, kind, refs)} for kind in {'literal','expr-literal',
    'derived'}. Member-access RHS (`ns.MEMBER`) is dropped as non-numeric-peer."""
    out = {}
    for raw in text.splitlines():
        code, _ = strip_line_comment(raw)
        m = ZIG_CONST_RE.match(code)   # anchored: file scope only
        if not m:
            continue
        name, rhs = m.group(1), m.group(2).strip()
        if MEMBER_ACCESS_RE.search(rhs):
            continue
        kind, refs = classify_zig_rhs(rhs)
        if kind is None:
            continue
        out.setdefault(name, (rhs, kind, refs))
    return out


def genuine_peer_derived(consts):
    """From a file's file-scope consts, yield (name, rhs, refs) for those that
    are GENUINELY peer-derived: kind 'derived' AND every referenced identifier is
    itself a file-scope numeric const in the same file (the `count / 4` shape).
    This is the precise Class-2b target; a ref to anything not a peer const is
    excluded (kills namespace/type/runtime false positives)."""
    peers = set(consts.keys())
    for name, (rhs, kind, refs) in consts.items():
        if kind != "derived":
            continue
        if refs and refs.issubset(peers):
            yield name, rhs, refs


def check_expression_parity(crate, cm, verbose):
    """Return (reds, notes). reds are folded-literal divergences (bind RED);
    notes are advisory (missing twins, value-preserving literal folds)."""
    relpaths = [f[0] for f in cm[crate]["files"]]
    rconsts = rust_consts(crate)
    rby_norm = {}
    for name, tup in rconsts.items():
        rby_norm.setdefault(norm(name), (name, tup))
    reds, notes = [], []
    for rel in relpaths:
        zpath = os.path.join(ORACLE_SRC, rel)
        if not os.path.isfile(zpath):
            continue
        try:
            with open(zpath, encoding="utf-8", errors="replace") as fh:
                text = fh.read()
        except OSError:
            continue
        consts = zig_file_scope_consts(text)
        for name, rhs, refs in genuine_peer_derived(consts):
            twin = rby_norm.get(norm(name))
            if twin is None:
                if verbose:
                    notes.append(f"    note: zig {rel}:{name} = {rhs}  [derived] — "
                                 f"no Rust twin in crate '{crate}' (renamed/inlined?)")
                continue
            rname, (rrhs, r_is_literal, rrel, rline) = twin
            if r_is_literal:
                reds.append(
                    f"    FOLD: zig {rel}: `const {name} = {rhs};` (derived from "
                    f"{', '.join(sorted(refs))}) folded to a literal in Rust "
                    f"{rrel}:{rline} `{rname} = {rrhs};` — Class-2b divergence "
                    f"(derived const must port as the same EXPRESSION).")
    return reds, notes


# ---------------------------------------------------------------------------
# Driver
# ---------------------------------------------------------------------------
def run_crate(crate, cm, oracle_ok, verbose):
    """Return (state, red_msgs, note_msgs). state in {'pass','skip','red'}."""
    if crate not in cm:
        return "red", [f"  unknown crate '{crate}' (not in crate_map)"], []
    if g.crate_src_dir(crate) is None:
        return "skip", [], [f"  {crate}: no crates/ dir yet (progress)"]
    if not g.is_claimed_done(crate):
        return "skip", [], [f"  {crate}: PORT STATUS not done ({g.crate_status(crate)}) "
                            f"— progress, not binding"]
    reds = []
    notes = []
    # (a) marker gate — always binds for a claimed-done crate.
    for rel, ln, kind, text in scan_markers(crate):
        reds.append(f"    MARKER[{kind}] {rel}:{ln}: {text}")
    # (b) expression parity — only when the oracle is present.
    if oracle_ok:
        r, n = check_expression_parity(crate, cm, verbose)
        reds.extend(r)
        notes.extend(n)
    return ("red" if reds else "pass"), reds, notes


def main(argv):
    verbose = False
    crate_arg = None
    selftest = False
    i = 1
    while i < len(argv):
        a = argv[i]
        if a in ("-v", "--verbose"):
            verbose = True
        elif a == "--selftest":
            selftest = True
        elif a == "--all-done":
            crate_arg = None
        elif a == "--crate":
            i += 1
            crate_arg = argv[i]
        elif a.startswith("--crate="):
            crate_arg = a.split("=", 1)[1]
        else:
            print(f"unknown arg: {a}", file=sys.stderr)
            return 2
        i += 1

    if selftest:
        return run_selftest()

    try:
        cm = g.crate_map()
    except g.Blocked as e:
        print(f"BLOCKED: {e}", file=sys.stderr)
        return 3

    oracle_ok = os.path.isdir(ORACLE_SRC)

    if crate_arg:
        crates = [g.normalize_crate(crate_arg, cm)]
    else:
        crates = [c for c in g.workspace_crates(cm) if g.is_claimed_done(c)]

    print("== D3 constants-by-expression parity ==")
    print(f"   oracle: {ORACLE_SRC} {'(present)' if oracle_ok else '(ABSENT)'}")
    print(f"   crates: {', '.join(crates) if crates else '(none)'}")

    any_red = False
    for crate in crates:
        state, reds, notes = run_crate(crate, cm, oracle_ok, verbose)
        if state == "red":
            any_red = True
            print(f"  RED  {crate}:")
            for m in reds:
                print(m)
        elif state == "pass":
            print(f"  ok   {crate}  (markers clean; constants expression-faithful)")
        else:  # skip
            if verbose:
                for m in notes:
                    print(m)
        if verbose:
            for m in notes:
                print(m)

    if any_red:
        print(">> D3 RED: surviving port markers and/or folded-literal constant "
              "divergences (Bun Class 2b).", file=sys.stderr)
        return 1
    if not oracle_ok:
        print(">> D3 BLOCKED: markers clean, but the ghostty oracle is absent so "
              "the (b) expression-parity diff could not bind (R3).", file=sys.stderr)
        return 3
    print(">> D3 PASS: no port markers; every derived Zig constant ports as the "
          "same expression in its Rust twin.")
    return 0


# ---------------------------------------------------------------------------
# Self-test — proves the detector fires on a folded-literal divergence and stays
# green on the faithful same-expression port. Uses synthetic fixtures in a
# tempdir so it is independent of the live (in-progress) tree.
# ---------------------------------------------------------------------------
def run_selftest():
    print("== D3 self-test ==")
    ok = True

    # --- Fixture 1: the exact Bun Class-2b shape. --------------------------
    zig = (
        "const count = 8192;\n"
        "// derived capacity — one quarter of count\n"
        "pub const cap = count / 4;\n"
        "pub const flag_bits = 1 << 11;\n"   # literal-only expr (advisory)
        "pub const name = \"rustty\";\n"      # non-numeric — ignored
    )
    # FAITHFUL Rust twin: same expression preserved.
    rust_good = (
        "pub const COUNT: usize = 8192;\n"
        "pub const CAP: usize = COUNT / 4;\n"
    )
    # BENT Rust twin: derived expr folded to a literal (the 32x-shrink bug).
    rust_bad = (
        "pub const COUNT: usize = 8192;\n"
        "pub const CAP: usize = 64;   // placeholder, wire up later\n"
    )

    with tempfile.TemporaryDirectory() as td:
        _write(td, "ref.zig", zig)

        # classify the Zig side directly.
        derived = {
            name: (kind, refs)
            for _, name, _, kind, refs in
            _zig_scan_text(zig)
        }
        ok &= _expect(derived.get("cap", (None,))[0] == "derived",
                      "zig `cap = count / 4` classified DERIVED",
                      f"got {derived.get('cap')}")
        ok &= _expect(derived.get("flag_bits", (None,))[0] == "expr-literal",
                      "zig `flag_bits = 1 << 11` classified expr-literal (advisory)",
                      f"got {derived.get('flag_bits')}")
        ok &= _expect("name" not in derived,
                      "zig `name = \"rustty\"` (string) ignored as non-numeric",
                      f"got {derived.get('name')}")

        # GOOD twin: no divergence.
        reds_good = _diff_text(zig, rust_good)
        ok &= _expect(reds_good == [],
                      "faithful same-expression twin -> GREEN (no divergence)",
                      f"got {reds_good}")

        # BAD twin: exactly one folded-literal divergence -> RED.
        reds_bad = _diff_text(zig, rust_bad)
        ok &= _expect(len(reds_bad) == 1 and "FOLD" in reds_bad[0],
                      "folded-literal twin (`CAP=64`) -> RED (Class-2b caught)",
                      f"got {reds_bad}")

        # --- Fixture 2: the marker gate (a). --------------------------------
        markers = _marker_scan_text(
            "pub const LIMIT: usize = 64; // placeholder until wired\n"
            "pub const OK_CONST: usize = 4096;\n"
            "// TODO(port): finish this\n"
            "fn f() {}\n"
        )
        kinds = sorted(k for _, _, k, _ in markers)
        ok &= _expect(kinds == ["placeholder-const", "port-marker"],
                      "marker gate flags TODO(port) AND the placeholder constant",
                      f"got {kinds}")

        # a legitimate architectural 'placeholder' doc comment must NOT flag.
        clean = _marker_scan_text(
            "//! INHABITED PLACEHOLDER (blueprint E4) — faithful placeholder leaf.\n"
            "pub const REAL: usize = 2048;\n"
        )
        ok &= _expect(clean == [],
                      "architectural 'placeholder' prose (no const literal) -> clean",
                      f"got {clean}")

    print()
    if ok:
        print(">> D3 self-test PASS: detector fires on folded-literal + surviving "
              "markers, and stays green on the faithful port.")
        return 0
    print(">> D3 self-test FAIL: detector logic is broken.", file=sys.stderr)
    return 1


# ---- self-test helpers: run the core logic over in-memory text -------------
def _write(d, name, text):
    p = os.path.join(d, name)
    with open(p, "w", encoding="utf-8") as fh:
        fh.write(text)
    return p


def _zig_scan_text(text):
    out = []
    for name, (rhs, kind, refs) in zig_file_scope_consts(text).items():
        out.append(("<mem>", name, rhs, kind, refs))
    return out


def _rust_scan_text(text):
    out = {}
    rc = re.compile(
        r"(?:pub\s*(?:\([^)]*\)\s*)?)?(?:const|static)\s+(?:mut\s+)?"
        r"([A-Za-z_]\w*)\s*:\s*[^=]+=\s*([^;]+);"
    )
    for lineno, raw in enumerate(text.splitlines(), start=1):
        code, _ = strip_line_comment(raw)
        m = rc.search(code)
        if not m:
            continue
        name = m.group(1)
        rhs = m.group(2).strip()
        out.setdefault(name, (rhs, bool(BARE_LITERAL_RE.match(rhs)), "<mem>", lineno))
    return out


def _diff_text(zig_text, rust_text):
    """Mirror check_expression_parity over in-memory Zig+Rust text."""
    rconsts = _rust_scan_text(rust_text)
    rby_norm = {}
    for name, tup in rconsts.items():
        rby_norm.setdefault(norm(name), (name, tup))
    reds = []
    consts = zig_file_scope_consts(zig_text)
    for name, rhs, refs in genuine_peer_derived(consts):
        twin = rby_norm.get(norm(name))
        if twin is None:
            continue
        rname, (rrhs, r_is_literal, rrel, rline) = twin
        if r_is_literal:
            reds.append(f"FOLD: zig {name}={rhs} -> Rust {rname}={rrhs}")
    return reds


def _marker_scan_text(text):
    """Mirror scan_markers over in-memory Rust text; return (line, text, kind, _)."""
    hits = []
    for idx, raw in enumerate(text.splitlines(), start=1):
        code, comment = strip_line_comment(raw)
        if PORT_MARKER_RE.search(raw):
            hits.append((idx, raw.strip(), "port-marker", None))
        m = RUST_LITERAL_CONST_RE.match(code)
        if m and "placeholder" in (comment or "").lower():
            hits.append((idx, raw.strip(), "placeholder-const", None))
    return hits


def _expect(cond, label, detail=""):
    print(f"  [{'PASS' if cond else 'FAIL'}] {label}"
          + (f"  ({detail})" if not cond else ""))
    return bool(cond)


if __name__ == "__main__":
    sys.exit(main(sys.argv))
