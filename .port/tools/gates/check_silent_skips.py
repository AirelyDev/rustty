#!/usr/bin/env python3
"""G-SKIP: no silent skips in test code (PORT_RULES R14).

A test that returns early because something it needs is missing (a Metal device,
a GL context, a display, a font, the fuzz corpus, a `.git` checkout, a feature
flag, the target OS) reports `ok` in libtest. That is indistinguishable from a
pass, so a CI lane that loses the resource goes GREEN having checked nothing.
R14 allows exactly three shapes instead:

  (a) a COMPILE-TIME scope (target, feature, build profile, a const derived from
      them) is an attribute on the test -- `#[cfg(..)]` or
      `#[cfg_attr(<cond>, ignore = "..")]` -- never a runtime `return`;
  (b) a RUNTIME resource is probed through a strict-mode pivot
      (`require_<x>_or_skip`, `RUSTTY_REQUIRE_<X>=1` turns the skip into a
      failure, and CI sets the variable wherever the resource exists), or the
      test is `#[ignore]`d and FAILS when run without the resource;
  (c) a skip that hid nothing real is deleted.

What this gate scans: every `#[test]` fn under crates/, crates-sys/ and xtask/.
It flags

  RET   a `return` at test-body level (closures and nested fns excluded) whose
        enclosing block / guard contains no assertion, no strict-mode pivot and
        no call to a helper that (transitively) calls one;
  EMPTY a test body whose every top-level statement is `#[cfg(..)]`-gated under
        one predicate (or with an empty arm), so it compiles to an empty,
        passing test on the other platforms;
  WRAP  a test whose every assertion sits inside one else-less `if` -- the
        "inverted skip": a false condition runs nothing and reports `ok`;
  MSG   a `print!/eprint!(ln)` saying "skip" anywhere in test-only code
        (helpers included) with no pivot, pivoting-helper call or assertion
        before it in its block: a helper that prints "skipping" and returns
        `None` is the same silent pass one call away.

EMPTY and WRAP are accepted when the test carries `#[cfg_attr(<pred>, ignore ..)]`
(it then reports `ignored` where it cannot run). RET is never excused by an
ignore: an #[ignore]d test is run on request and must then FAIL, not return.

Accepted without an allow-list entry: a pivot or assertion before the return; a
let-else / match / `if` guard whose probe is a pivoting helper; the child arm of
a parent/child test (`std::env::var_os(CHILD)` guard -- the parent asserts on
what the child printed).

ALLOW-LIST (`ALLOW_SITES`, `ALLOW_GUARDS` below) is capped (`MAX_SITES`,
`MAX_GUARDS`) and every entry carries a reason. A GUARD entry is for a guard that
is statically dead in every build this workspace can produce; the gate re-checks
that claim (`verify`) on every run, so flipping the constant turns it RED.

Usage: check_silent_skips.py              # scan the checkout (binding)
       check_silent_skips.py --list       # print every site it reasons about
       check_silent_skips.py --self-test  # prove each RED arm and each accept arm
Exit 0 GREEN, 1 RED.
"""
import os
import re
import sys
import tempfile

GATES_DIR = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, GATES_DIR)
from _gatelib import _find_balanced_block, _strip_rust  # noqa: E402

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(GATES_DIR)))
SCAN_DIRS = ["crates", "crates-sys", "xtask"]

# ---------------------------------------------------------------------------
# Allow-list. Keep it SHORT: every entry is a test that reports `ok` without
# the resource and has been argued to be acceptable anyway.
# ---------------------------------------------------------------------------
MAX_SITES = 6
MAX_GUARDS = 2

# (path relative to ROOT, test fn name) -> reason
ALLOW_SITES = {
    ("crates/rustty-renderer/src/opengl/shaders.rs", "glsl_assets_match_reference_byte_for_byte"):
        "partial, not vacuous: the hash-pin assertions over every GLSL asset run "
        "unconditionally first; only the extra byte diff against the Ghostty oracle "
        "checkout is skipped, and CI never has the oracle (R3)",
    ("crates/rustty-font/src/glyf_rasterize/glyf_rasterize_png_test.rs",
     "bubbletea_glyph_protocol_examples_match_reference_image"):
        "not a skip: the opt-in RUSTTY_REGEN_FONT_KAT_PNGS arm returns only after "
        "the full pixel diff has proved the decoded atlas identical to the reference "
        "(outcome == BytesDifferPixelsMatch), having rewritten the reference bytes",
    ("xtask/src/release.rs", "build_info_reads_this_checkout"):
        "skips only in the SOURCE TARBALL, which ships no .git (parity S63-F8); a "
        "repo checkout always has .git, so the skip cannot hide a failure where it "
        "matters. A strict variable cannot be used: CI's distcheck runs the "
        "extracted tarball's tests with the CI environment inherited",
    ("xtask/tests/release_workflows.rs",
     "release_info_writes_github_output_and_rejects_a_mismatched_tag"):
        "same as build_info_reads_this_checkout: skips only in the source tarball "
        "(no .git), which a repo checkout cannot be",
}

# A guard regex (matched against the guard + block text before the return) that
# is statically false in every build, with the facts that make it so. `verify`
# is a list of (path, regex) that must ALL match, or the entry is void and the
# gate goes RED.
ALLOW_GUARDS = [
    {
        "guard": r"\bARTIFACT\b[\s\S]*?\bArtifact::Lib\b",
        "verify": [
            ("crates/rustty-vt/src/options.rs",
             r"(?m)^pub const ARTIFACT: Artifact = Artifact::Ghostty;"),
            ("crates/rustty-font/src/options.rs",
             r"(?m)^pub const ARTIFACT: Artifact = Artifact::Ghostty;"),
        ],
        "reason": "ports of `if (comptime build_options.artifact == .lib) return "
                  "error.SkipZigTest;`. rustty-vt and rustty-font hard-wire ARTIFACT to "
                  "the application artifact (no cfg selects .lib), so the arm is dead in "
                  "every build; it is kept verbatim for R1 and re-verified here",
    },
    {
        "guard": r"!\s*super::SIMD\b",
        "verify": [
            ("crates/rustty-simd/src/codepoint_width.rs", r"(?m)^const SIMD: bool = true;"),
        ],
        "reason": "rustty-simd hard-wires SIMD = true (the reference's `options.simd` "
                  "for the application); the `!SIMD` early return is dead in every build",
    },
]

# ---------------------------------------------------------------------------
TEST_ATTR = re.compile(r"#\s*\[\s*(?:[\w:]+::)?test\s*\]")
FN_RE = re.compile(r"\bfn\s+([A-Za-z_]\w*)")
CLOSURE = re.compile(r"(?:\|[^|{};]*\||\|\|)\s*(?:->\s*[^{;]*)?\{")
ASYNC = re.compile(r"\basync\s+(?:move\s+)?\{")
RET = re.compile(r"\breturn\b")
ASSERTS = re.compile(
    r"\b(?:assert|assert_eq|assert_ne|debug_assert|debug_assert_eq|panic|unreachable|"
    r"todo|unimplemented)\s*!"
)
PIVOT = re.compile(r"\b(?:require_\w+_or_skip|init_for_display_test)\s*\(")
REQUIRE_VAR = re.compile(r'"RUSTTY_REQUIRE_[A-Z0-9_]+"')
CHILD_ARM = re.compile(
    r"(?:\benv::var_os\s*\(\s*[A-Z_][A-Z0-9_]*\s*\)\s*\.is_some\s*\(\s*\)"
    r"|let\s+Some\s*\([^)]*\)\s*=\s*std::env::var_os\s*\(\s*[A-Z_][A-Z0-9_]*\s*\))"
)
CFG_ATTR = re.compile(r"#\s*\[\s*cfg\s*\(")
PRINT_MACRO = re.compile(r"\b(?:e?println|e?print)\s*!\s*\(")
SKIP_WORD = re.compile(r"(?i)\bskip(?:ping|ped|s)?\b")



def enclosing_fn_name(clean, idx):
    """Name of the innermost fn whose body contains `idx` (best effort)."""
    best = None
    for m in FN_RE.finditer(clean, 0, idx):
        b = clean.find("{", m.end())
        semi = clean.find(";", m.end())
        if b == -1 or b > idx or (semi != -1 and semi < b):
            continue
        if _find_balanced_block(clean, b) > idx:
            best = m.group(1)
    return best or "?"


def scoped_ignores(attrs):
    """The predicates of every `#[cfg_attr(<pred>, ignore ..)]` in `attrs`.
    (`cfg_attr(miri, ignore)` is a Miri exclusion, not a platform scope; the
    caller discounts it.)"""
    out = []
    for m in re.finditer(r"#\s*\[\s*cfg_attr\s*\(", attrs):
        i, depth = m.end(), 1
        while i < len(attrs) and depth:
            c = attrs[i]
            if c == "(":
                depth += 1
            elif c == ")":
                depth -= 1
            elif c == "," and depth == 1:
                if re.match(r",\s*ignore\b", attrs[i:]):
                    out.append(attrs[m.end():i])
                break
            i += 1
    return out


def lineno(text, idx):
    return text.count("\n", 0, idx) + 1


def iter_files(root):
    for d in SCAN_DIRS:
        base = os.path.join(root, d)
        for dp, dirs, fs in os.walk(base):
            dirs[:] = [x for x in dirs if x != "target" and not x.startswith(".")]
            for f in fs:
                if f.endswith(".rs"):
                    yield os.path.join(dp, f)


def fn_bodies(clean):
    """Yield (name, name_idx, body_open, body_end) for every fn with a body."""
    for m in FN_RE.finditer(clean):
        b = clean.find("{", m.end())
        semi = clean.find(";", m.end())
        if b == -1 or (semi != -1 and semi < b):
            continue
        yield m.group(1), m.start(), b, _find_balanced_block(clean, b)


def nested_ranges(clean, start, end):
    rs = []
    for m in FN_RE.finditer(clean, start, end):
        b = clean.find("{", m.end(), end)
        semi = clean.find(";", m.end(), end)
        if b == -1 or (semi != -1 and semi < b):
            continue
        rs.append((b, _find_balanced_block(clean, b)))
    for rx in (CLOSURE, ASYNC):
        for m in rx.finditer(clean, start, end):
            b = m.end() - 1
            rs.append((b, _find_balanced_block(clean, b)))
    return rs


def enclosing_open(clean, lo, idx):
    """Index of the innermost unmatched `{` in clean[lo:idx]."""
    depth = 0
    i = idx - 1
    while i >= lo:
        c = clean[i]
        if c == "}":
            depth += 1
        elif c == "{":
            if depth == 0:
                return i
            depth -= 1
        i -= 1
    return lo


def statement_start(clean, lo, brace):
    """Start of the statement/expression that owns the block opening at `brace`."""
    depth = 0
    i = brace - 1
    while i >= lo:
        c = clean[i]
        if c in ")]":
            depth += 1
        elif c in "([":
            depth -= 1
        elif depth == 0 and c in ";{}":
            return i + 1
        i -= 1
    return lo


CFG_TEST = re.compile(r"#\s*\[\s*cfg\s*\(\s*(?:all\s*\(\s*)?test\b")


def test_regions(path, clean):
    """(start, end) ranges of test-only code: the whole file for integration
    tests and test-named modules, else every `#[cfg(test)]` item."""
    base = os.path.basename(path)
    if f"{os.sep}tests{os.sep}" in path or "test" in base:
        return [(0, len(clean))]
    out = []
    for m in CFG_TEST.finditer(clean):
        b = clean.find("{", m.end())
        semi = clean.find(";", m.end())
        if b == -1 or (semi != -1 and semi < b):
            continue
        out.append((b, _find_balanced_block(clean, b)))
    return out


def helper_call_rx(local, shared):
    """A call to a pivoting helper: a bare/`Self::`-style call to one defined in
    the SAME file, or `test_support::<name>(` for one in a `test_support.rs`.
    Method calls (`.name(`) never count, and a same-named fn elsewhere (e.g.
    `thread::spawn`) is not the helper."""
    alts = []
    if local:
        # bare `name(`, or `Type::name(` / `Self::name(` (a CamelCase owner)
        alts.append(r"(?<![.\w:])(?:[A-Z]\w*::)?(?:" + "|".join(map(re.escape, sorted(local))) + r")")
    if shared:
        alts.append(r"\btest_support::(?:" + "|".join(map(re.escape, sorted(shared))) + r")")
    return re.compile("(?:" + "|".join(alts) + r")\s*\(") if alts else None


def guard_context(clean, lo, idx):
    """(block_open, text) for the code at `idx`: the text runs from the start
    of the statement that owns the enclosing block -- the let-else / `if` /
    `match` guard, extended to the `match <scrutinee>` for a match arm -- up to
    `idx`."""
    blk = enclosing_open(clean, lo, idx)
    stmt = statement_start(clean, lo + 1, blk) if blk > lo else blk
    if "=>" in clean[stmt:blk + 1] and stmt > lo + 1:
        outer = enclosing_open(clean, lo, stmt)
        if outer > lo:
            stmt = statement_start(clean, lo + 1, outer)
    return blk, clean[stmt:idx]


def pivoting_helpers(files):
    """Per file, the names of fns that are, or (transitively) call, a
    strict-mode pivot; plus the shared set defined in `test_support.rs` files.

    A ROOT is a test-only fn whose body reads a `"RUSTTY_REQUIRE_<X>"` variable
    (string literals kept for this check only) or calls `require_<x>_or_skip` /
    `init_for_display_test`. Everything that calls a root, transitively, is a
    pivoting helper: a test that skips on its `None` has been through a pivot.
    Only test-only code (integration tests, test-named files, `#[cfg(test)]`
    items) can define one, and `#[test]` fns never are one."""
    bodies = {}  # path -> [(name, body)]
    piv = {}     # path -> set(name)
    for path, text, clean in files:
        regions = test_regions(path, clean)
        tests = set()
        for m in TEST_ATTR.finditer(clean):
            fm = FN_RE.search(clean, m.end())
            if fm:
                tests.add(fm.start())
        bl, ps = [], set()
        for name, n_idx, b, e in fn_bodies(clean):
            if n_idx in tests:
                continue  # a #[test] is never a helper
            if not any(lo <= n_idx < hi for lo, hi in regions):
                continue  # production code is never a skip helper
            bl.append((name, clean[b:e]))
            if REQUIRE_VAR.search(text[b:e]) or PIVOT.search(clean[b:e]):
                ps.add(name)
        bodies[path], piv[path] = bl, ps
    changed = True
    while changed:
        changed = False
        shared = set()
        for path, names in piv.items():
            if os.path.basename(path) == "test_support.rs":
                shared |= names
        for path, bl in bodies.items():
            rx = helper_call_rx(piv[path], shared)
            if rx is None:
                continue
            for name, body in bl:
                if name not in piv[path] and rx.search(body):
                    piv[path].add(name)
                    changed = True
    shared = set()
    for path, names in piv.items():
        if os.path.basename(path) == "test_support.rs":
            shared |= names
    return piv, shared


def cfg_empty(clean, b, e):
    """True when every top-level statement of the body is `#[cfg(..)]`-gated."""
    body = clean[b + 1:e - 1]
    i, n, saw_cfg = 0, len(body), False
    rest, preds, empty_arm = [], set(), False
    while i < n:
        m = CFG_ATTR.match(body, i) if body[i] == "#" else None
        if m:
            saw_cfg = True
            j = body.find("]", m.end())
            # balanced: attribute args may nest parens
            depth, j = 0, m.end() - 1
            while j < n:
                if body[j] == "(":
                    depth += 1
                elif body[j] == ")":
                    depth -= 1
                    if depth == 0:
                        break
                j += 1
            preds.add(re.sub(r"\s+", "", body[m.end():j]))
            j = body.find("]", j) + 1
            while j < n and body[j].isspace():
                j += 1
            if j < n and body[j] == "{":
                i = _find_balanced_block(body, j)
                if not body[j + 1:i - 1].strip():
                    empty_arm = True
            else:
                depth = 0
                while j < n:
                    c = body[j]
                    if c in "{([":
                        depth += 1
                    elif c in "})]":
                        depth -= 1
                    elif c == ";" and depth == 0:
                        j += 1
                        break
                    j += 1
                i = j
            continue
        if body[i] == "{":
            end = _find_balanced_block(body, i)
            rest.append(body[i:end])
            i = end
            continue
        rest.append(body[i])
        i += 1
    # Every statement is cfg-gated AND either one predicate gates them all (the
    # other platforms get an empty body) or one arm is literally empty.
    return saw_cfg and not "".join(rest).strip() and (len(preds) == 1 or empty_arm)


def top_level_statements(body):
    """Split a (stripped) fn body into its top-level statements."""
    stmts, i, n, cur, depth = [], 0, len(body), 0, 0
    while i < n:
        c = body[i]
        if c == "{":
            j = _find_balanced_block(body, i)
            k = j
            while k < n and body[k].isspace():
                k += 1
            if body.startswith("else", k) or (k < n and body[k] in ".?;),"):
                i = k if body.startswith("else", k) else j
                continue
            stmts.append(body[cur:j])
            cur = i = j
            continue
        if c in "([":
            depth += 1
        elif c in ")]":
            depth -= 1
        elif c == ";" and depth == 0:
            stmts.append(body[cur:i + 1])
            cur = i + 1
        i += 1
    stmts.append(body[cur:])
    return [x.strip() for x in stmts if x.strip()]


def wrapped_in_if(clean, b, e):
    """True when every assertion of the test sits inside ONE else-less `if`, so a
    false condition runs nothing and passes (the 'inverted skip' shape)."""
    stmts = top_level_statements(clean[b + 1:e - 1])
    for idx, st in enumerate(stmts):
        bare = re.sub(r"^(?:#\[[^\]]*\]\s*)+", "", st)
        if not bare.startswith("if ") or re.search(r"\}\s*else\b", bare):
            continue
        outside = "".join(stmts[:idx] + stmts[idx + 1:])
        if ASSERTS.search(bare) and not ASSERTS.search(outside):
            # `if let Err(e) = r { panic!() }` is an assertion, not a skip.
            if re.match(r"if\s+let\s+Err\s*\(", bare) or re.match(r"if\s+[^{]*\.is_err\(\)", bare):
                continue
            return True
    return False


def scan(root):
    files = {}
    for p in iter_files(root):
        text = open(p, encoding="utf-8", errors="replace").read()
        if "#[" not in text or "test" not in text:
            continue
        clean, _ = _strip_rust(text)
        files[p] = (text, clean)
    piv, shared = pivoting_helpers([(p, t, c) for p, (t, c) in files.items()])

    findings, accepted = [], []
    guard_hits = {i: 0 for i in range(len(ALLOW_GUARDS))}
    site_hits = {k: 0 for k in ALLOW_SITES}
    for p, (text, clean) in sorted(files.items()):
        rel = os.path.relpath(p, root)
        call_piv = helper_call_rx(piv.get(p, set()), shared)
        for m in TEST_ATTR.finditer(clean):
            fm = FN_RE.search(clean, m.end())
            if not fm:
                continue
            name = fm.group(1)
            b = clean.find("{", fm.end())
            if b == -1:
                continue
            e = _find_balanced_block(clean, b)
            key = (rel, name)
            # the attribute block: from the end of the previous item to `fn`
            prev = max(clean.rfind("}", 0, m.start()), clean.rfind(";", 0, m.start()))
            attrs = clean[prev + 1:fm.start()]
            scoped = any(re.sub(r"\s+", "", pr) != "miri" for pr in scoped_ignores(attrs))
            if cfg_empty(clean, b, e) and not scoped:
                site = f"{rel}:{lineno(text, b)} [{name}]"
                if key in ALLOW_SITES:
                    site_hits[key] += 1
                    accepted.append(("EMPTY", site, "allow-site"))
                else:
                    findings.append(("EMPTY", site, "every statement of the body is #[cfg]-gated"))
            if wrapped_in_if(clean, b, e) and not scoped:
                site = f"{rel}:{lineno(text, b)} [{name}]"
                if key in ALLOW_SITES:
                    site_hits[key] += 1
                    accepted.append(("WRAP", site, "allow-site"))
                else:
                    findings.append(("WRAP", site, "every assertion is inside one else-less `if`"))
            nr = nested_ranges(clean, b + 1, e)
            for r in RET.finditer(clean, b, e):
                if any(s < r.start() < t for s, t in nr):
                    continue
                blk, ctx = guard_context(clean, b, r.start())
                site = f"{rel}:{lineno(text, r.start())} [{name}]"
                why = None
                if ASSERTS.search(clean[blk:r.start()]):
                    why = "assertion-before-return"
                elif PIVOT.search(ctx):
                    why = "pivot"
                elif call_piv and call_piv.search(ctx):
                    why = "pivoting-helper"
                elif CHILD_ARM.search(ctx):
                    why = "child-arm"
                else:
                    for gi, g in enumerate(ALLOW_GUARDS):
                        if re.search(g["guard"], ctx):
                            guard_hits[gi] += 1
                            why = f"allow-guard#{gi}"
                            break
                if why is None and key in ALLOW_SITES:
                    site_hits[key] += 1
                    why = "allow-site"
                if why is None:
                    findings.append(("RET", site, "early return with no assertion or strict-mode pivot"))
                else:
                    accepted.append(("RET", site, why))
        # SKIPMSG: a "skipping" line anywhere in test-only code (helpers
        # included) must follow a pivot, a pivoting-helper call or an assertion
        # in its block -- a helper that prints "skip" and returns is the same
        # silent pass one call away.
        for lo, hi in test_regions(p, clean):
            for mm in PRINT_MACRO.finditer(clean, lo, hi):
                open_paren = clean.find("(", mm.end() - 1)
                close = open_paren
                depth = 0
                while close < len(clean):
                    if clean[close] == "(":
                        depth += 1
                    elif clean[close] == ")":
                        depth -= 1
                        if depth == 0:
                            break
                    close += 1
                if not SKIP_WORD.search(text[open_paren:close]):
                    continue
                _blk, ctx = guard_context(clean, lo, mm.start())
                fn_name = enclosing_fn_name(clean, mm.start())
                key = (rel, fn_name)
                site = f"{rel}:{lineno(text, mm.start())} [{fn_name}]"
                if ASSERTS.search(ctx) or PIVOT.search(ctx) or (call_piv and call_piv.search(ctx)):
                    accepted.append(("MSG", site, "pivot-before-skip-line"))
                elif key in ALLOW_SITES:
                    site_hits[key] += 1
                    accepted.append(("MSG", site, "allow-site"))
                else:
                    findings.append(("MSG", site, "a skip line with no strict-mode pivot before it"))
    return findings, accepted, guard_hits, site_hits


def verify_allowlist(root, guard_hits, site_hits):
    errs = []
    if len(ALLOW_SITES) > MAX_SITES:
        errs.append(f"ALLOW_SITES has {len(ALLOW_SITES)} entries, cap is {MAX_SITES}")
    if len(ALLOW_GUARDS) > MAX_GUARDS:
        errs.append(f"ALLOW_GUARDS has {len(ALLOW_GUARDS)} entries, cap is {MAX_GUARDS}")
    for k, reason in ALLOW_SITES.items():
        if not reason or len(reason) < 20:
            errs.append(f"ALLOW_SITES {k} has no real reason")
        if site_hits.get(k, 0) == 0:
            errs.append(f"ALLOW_SITES {k} matches nothing (stale entry: delete it)")
    for gi, g in enumerate(ALLOW_GUARDS):
        if not g.get("reason") or len(g["reason"]) < 20:
            errs.append(f"ALLOW_GUARDS#{gi} has no real reason")
        if guard_hits.get(gi, 0) == 0:
            errs.append(f"ALLOW_GUARDS#{gi} matches nothing (stale entry: delete it)")
        for path, rx in g["verify"]:
            full = os.path.join(root, path)
            try:
                body = open(full, encoding="utf-8").read()
            except OSError:
                errs.append(f"ALLOW_GUARDS#{gi}: verify file {path} is missing")
                continue
            if not re.search(rx, body):
                errs.append(
                    f"ALLOW_GUARDS#{gi}: {path} no longer matches {rx!r} -- the guard "
                    f"is no longer statically dead, so its skips are live: convert them"
                )
    return errs


def run(root, verbose=False, check_allow=True):
    findings, accepted, gh, sh = scan(root)
    errs = verify_allowlist(root, gh, sh) if check_allow else []
    if verbose:
        for kind, site, why in accepted:
            print(f"  ok   {kind:5} {site}  ({why})")
    for kind, site, why in findings:
        print(f"  RED  {kind:5} {site}: {why}")
    for e in errs:
        print(f"  RED  ALLOW {e}")
    red = bool(findings or errs)
    print(
        f"G-SKIP: {len(findings)} silent-skip site(s), {len(errs)} allow-list error(s); "
        f"{len(accepted)} guarded return(s) accepted"
    )
    if red:
        print(
            "\nA test that returns early without asserting reports `ok` having checked\n"
            "nothing (PORT_RULES R14). Compile-time scope -> #[cfg_attr(<cond>, ignore = \"..\")];\n"
            "runtime resource -> a require_<x>_or_skip pivot (RUSTTY_REQUIRE_<X>=1) or\n"
            "#[ignore] + a loud failure; otherwise delete the skip."
        )
    return 1 if red else 0


SELF_CASES = [
    # (name, source, expect_red)
    ("silent let-else", """
#[test]
fn t() {
    let Some(d) = probe_device() else {
        eprintln!("no device; skipping");
        return;
    };
    assert!(d.ok());
}
""", True),
    ("silent comptime guard", """
#[test]
fn t() {
    if !cfg!(target_os = "macos") {
        return;
    }
    assert!(true);
}
""", True),
    ("silent match arm", """
#[test]
fn t() {
    let d = match probe_device() { Some(d) => d, None => return };
    assert!(d.ok());
}
""", True),
    ("cfg-only body", """
#[test]
fn t() {
    #[cfg(windows)]
    {
        assert!(true);
    }
}
""", True),
    ("ignored but still silent", """
#[test]
#[ignore = "needs a display"]
fn t() {
    let Some(h) = display() else { return; };
    assert!(h.ok());
}
""", True),
    ("inverted skip (all assertions under one if)", """
#[test]
fn t() {
    if BACKEND == Backend::Fontconfig {
        assert!(load().is_ok());
    }
}
""", True),
    ("helper that skips silently", """
#[cfg(test)]
mod tests {
    fn device() -> Option<u8> {
        eprintln!("no device; skipping");
        None
    }
}
""", True),
    ("pivot before return", """
#[test]
fn t() {
    let Some(d) = probe_device() else {
        require_metal_or_skip("t", &"nil");
        return;
    };
    assert!(d.ok());
}
""", False),
    ("pivoting helper", """
#[cfg(test)]
mod tests {
    fn offscreen() -> Option<u8> {
        match open() { Ok(x) => Some(x), Err(e) => { require_gl_or_skip("offscreen", &e); None } }
    }
    #[test]
    fn t() {
        let Some(_g) = offscreen() else { return; };
        assert!(true);
    }
}
""", False),
    ("same-named fn elsewhere is not the helper", """
#[cfg(test)]
mod tests {
    fn spawn() -> Option<u8> { require_metal_or_skip("x"); None }
    #[test]
    fn t() {
        let Some(_h) = std::thread::spawn(|| 1).join().ok() else { return; };
        assert!(true);
    }
}
""", True),
    ("production fn is never a helper", """
pub fn offscreen() -> Option<u8> {
    if std::env::var_os("RUSTTY_REQUIRE_GL").is_some() { panic!() }
    None
}
#[cfg(test)]
mod tests {
    #[test]
    fn t() {
        let Some(_g) = super::offscreen() else { return; };
        assert!(true);
    }
}
""", True),
    ("assertion arm", """
#[test]
fn t() {
    if !checkout() {
        assert_eq!(hash(), None);
        return;
    }
    assert!(hash().is_some());
}
""", False),
    ("child arm", """
#[test]
fn t() {
    if std::env::var_os(CHILD).is_some() {
        child();
        return;
    }
    assert!(run_child().success());
}
""", False),
    ("closure return is not a test return", """
#[test]
fn t() {
    let f = |x: u8| { if x == 0 { return 1; } x };
    assert_eq!(f(0), 1);
}
""", False),
    ("cfg-only body with a scoped ignore", """
#[test]
#[cfg_attr(windows, ignore = "not on Windows")]
fn t() {
    #[cfg(not(windows))]
    {
        assert!(true);
    }
}
""", False),
    ("a Miri-only ignore does not scope a cfg-only body", """
#[test]
#[cfg_attr(miri, ignore)]
fn t() {
    #[cfg(not(windows))]
    {
        assert!(true);
    }
}
""", True),
    ("attribute scope is fine", """
#[test]
#[cfg_attr(not(target_os = "macos"), ignore = "macOS only")]
fn t() {
    assert!(true);
}
""", False),
]


def self_test():
    ok = True
    for name, src, expect_red in SELF_CASES:
        with tempfile.TemporaryDirectory(prefix="gskip_") as tmp:
            os.makedirs(os.path.join(tmp, "crates", "x", "src"))
            with open(os.path.join(tmp, "crates", "x", "src", "lib.rs"), "w") as fh:
                fh.write(src)
            findings, _acc, _gh, _sh = scan(tmp)
            red = bool(findings)
            good = red == expect_red
            ok &= good
            print(f"  [{'PASS' if good else 'FAIL'}] {name}: expected "
                  f"{'RED' if expect_red else 'GREEN'}, got {'RED' if red else 'GREEN'}")
    # allow-list arms: a stale site, an over-cap list, and a guard whose verify fails
    with tempfile.TemporaryDirectory(prefix="gskip_") as tmp:
        global ALLOW_SITES, ALLOW_GUARDS
        saved = (ALLOW_SITES, ALLOW_GUARDS)
        try:
            os.makedirs(os.path.join(tmp, "crates", "x", "src"))
            with open(os.path.join(tmp, "crates", "x", "src", "lib.rs"), "w") as fh:
                fh.write("#[test]\nfn t() {\n    if CONST_X == Kind::B {\n        return;\n    }\n"
                         "    assert!(true);\n}\n")
            with open(os.path.join(tmp, "defs.rs"), "w") as fh:
                fh.write("pub const CONST_X: Kind = Kind::A;\n")
            ALLOW_SITES = {}
            ALLOW_GUARDS = [{"guard": r"CONST_X\s*==\s*Kind::B",
                             "verify": [("defs.rs", r"(?m)^pub const CONST_X: Kind = Kind::A;")],
                             "reason": "statically dead in this synthetic tree"}]
            f, _a, gh, sh = scan(tmp)
            green = not f and not verify_allowlist(tmp, gh, sh)
            print(f"  [{'PASS' if green else 'FAIL'}] verified dead guard: expected GREEN")
            ok &= green
            with open(os.path.join(tmp, "defs.rs"), "w") as fh:
                fh.write("pub const CONST_X: Kind = Kind::B;\n")
            f, _a, gh, sh = scan(tmp)
            red = bool(verify_allowlist(tmp, gh, sh))
            print(f"  [{'PASS' if red else 'FAIL'}] guard whose constant flipped: expected RED")
            ok &= red
            ALLOW_SITES = {("crates/x/src/lib.rs", "gone"): "a reason that is long enough"}
            red = any("stale" in e for e in verify_allowlist(tmp, gh, {}))
            print(f"  [{'PASS' if red else 'FAIL'}] stale allow-site: expected RED")
            ok &= red
            ALLOW_SITES = {("f", str(i)): "a reason that is long enough" for i in range(MAX_SITES + 1)}
            red = any("cap" in e for e in verify_allowlist(tmp, gh, {}))
            print(f"  [{'PASS' if red else 'FAIL'}] allow-list over its cap: expected RED")
            ok &= red
        finally:
            ALLOW_SITES, ALLOW_GUARDS = saved
    print("SELF-TEST: " + ("PASS" if ok else "FAIL"))
    return 0 if ok else 1


if __name__ == "__main__":
    print("=== G-SKIP :: no silent skips in test code (R14) ===")
    if "--self-test" in sys.argv:
        sys.exit(self_test())
    sys.exit(run(ROOT, verbose="--list" in sys.argv))
