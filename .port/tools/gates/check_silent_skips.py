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

What this gate scans: every test fn (`#[test]`, `#[<path>::test]`, `#[rstest]`,
`#[test_case(..)]`) in the TRACKED `.rs` files (`git ls-files`) under crates/,
crates-sys/ and xtask/. It flags

  RET   a bare or valued `return` in a test body -- and a bare `return` inside a
        closure or nested fn of it -- whose guard asserts nothing: no
        non-trivial assertion in the guard block itself (for a top-level
        `cond || return;` the statement, for a `None => return` match arm the
        arm), no verified strict-mode pivot and no call to a helper that
        (transitively) both reads a `RUSTTY_REQUIRE_<X>` variable and fails;
  EMPTY a test body whose every top-level statement is `#[cfg(..)]`-gated under
        one predicate (or with an empty arm), so it compiles to an empty,
        passing test on the other platforms;
  WRAP  a test whose every non-trivial assertion sits inside the branches of
        one `if` / `if let` while another path through it (a missing `else`, or
        an `else` that only prints or does nothing checkable) asserts nothing --
        the "inverted skip": the silent path runs nothing and reports `ok`;
  ARM   a `match` in a test with an asserting arm and a `None` / `Err(..)` arm
        that is empty, `()` or print-only: the same silent path, spelled as a
        match;
  LOOP  a test whose every non-trivial assertion sits inside a `for` loop over
        a function call's result, or inside a closure passed to `.map(` /
        `.for_each(` / `.inspect(` / `.and_then(` / `.map_or(` / `.map_or_else(`
        / `.filter_map(`: an empty probe result (no devices, no files, `None`)
        runs no assertion at all;
  MSG   a `print!/eprint!(ln)` saying "skip" anywhere in test-only code
        (helpers included) with no pivot, pivoting-helper call or assertion
        before it in its block: a helper that prints "skipping" and returns
        `None` is the same silent pass one call away;
  PIVOT a fn named `require_<x>_or_skip` that does not (transitively, within its
        file) both read a `"RUSTTY_REQUIRE_<X>"` variable and fail (a
        non-debug assertion or panic), or an `init_for_display_test` that
        cannot fail. The name alone is not a pivot: a no-op called
        `require_x_or_skip` would otherwise excuse every skip after it.

A "trivial" assertion never counts: `debug_assert*!` (compiled out in release),
and an `assert!`/`assert_eq!`/`assert_ne!` whose arguments name no identifier but
`true`/`false`, or whose two compared operands are the same text.

"Test-only code" is: a file under a `tests/` directory; a file that a
`#[cfg(test)]`-scoped (`cfg(test)`, `cfg(all(test, ..))`, `cfg(any(test, ..))`)
`mod name;` declaration loads -- resolved like rustc does, `#[path]` included,
transitively; and every `#[cfg(test)]`-scoped item. A file's NAME never makes it
test code (`latest.rs`, `testing.rs` and `test_host.rs` are production files).

EMPTY, WRAP, ARM and LOOP are accepted when the test carries
`#[cfg_attr(<pred>, ignore ..)]` (it then reports `ignored` where it cannot
run). RET is never excused by an ignore: an #[ignore]d test is run on request and
must then FAIL, not return.

Accepted without an allow-list entry: a non-trivial assertion in the guard
block before the return; a let-else / match / `if` guard that calls a verified
pivot or a pivoting helper; the child arm of a parent/child test
(`std::env::var_os(CHILD)` guard -- the parent asserts on what the child
printed).

ALLOW-LIST (`ALLOW_SITES`, `ALLOW_GUARDS` below) is capped (`MAX_SITES`,
`MAX_GUARDS`) and every entry carries a reason. A GUARD entry is for a guard that
is statically dead in every build this workspace can produce; the gate re-checks
that claim (`verify`) on every run, so flipping the constant turns it RED.

The file list comes from `git ls-files`: an untracked scratch file cannot change
the verdict, and a checkout that is not a git work tree is an ERROR (exit 1),
never a silent fallback to walking the disk.

Usage: check_silent_skips.py              # scan the checkout (binding)
       check_silent_skips.py --list       # print every site it reasons about
       check_silent_skips.py --self-test  # prove each RED arm and each accept arm
Exit 0 GREEN, 1 RED.
"""
import os
import re
import subprocess
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
TEST_ATTR = re.compile(
    r"#\s*\[\s*(?:(?:[\w:]+::)?test|rstest|test_case\s*\([^\]]*\))\s*\]"
)
FN_RE = re.compile(r"\bfn\s+([A-Za-z_]\w*)")
CLOSURE = re.compile(r"(?:\|[^|{};]*\||\|\|)\s*(?:->\s*[^{;]*)?\{")
ASYNC = re.compile(r"\basync\s+(?:move\s+)?\{")
RET = re.compile(r"\breturn\b")
ASSERTS = re.compile(
    r"\b(?:assert|assert_eq|assert_ne|debug_assert|debug_assert_eq|debug_assert_ne|panic|"
    r"unreachable|todo|unimplemented)\s*!"
)
FAIL_MACROS = {"panic", "unreachable", "todo", "unimplemented"}
PIVOT_NAME = re.compile(r"^require_\w+_or_skip$")
PIVOT_CALL = re.compile(r"(?<![\w.])(?:[\w]+::)*(require_\w+_or_skip|init_for_display_test)\s*\(")
REQUIRE_VAR = re.compile(r'"RUSTTY_REQUIRE_[A-Z0-9_]+"')
CHILD_ARM = re.compile(
    r"(?:\benv::var_os\s*\(\s*[A-Z_][A-Z0-9_]*\s*\)\s*\.is_some\s*\(\s*\)"
    r"|let\s+Some\s*\([^)]*\)\s*=\s*std::env::var_os\s*\(\s*[A-Z_][A-Z0-9_]*\s*\))"
)
CFG_ATTR = re.compile(r"#\s*\[\s*cfg\s*\(")
PRINT_MACRO = re.compile(r"\b(?:e?println|e?print)\s*!\s*\(")
SKIP_WORD = re.compile(r"(?i)\bskip(?:ping|ped|s)?\b")
IDENT = re.compile(r"(?<![\w.'])[A-Za-z_]\w*")
LITERAL_WORDS = {"true", "false", "as"}
MAP_LIKE = re.compile(
    r"\.\s*(?:map|for_each|inspect|and_then|map_or|map_or_else|filter_map)\s*\("
)
MATCH_KW = re.compile(r"\bmatch\b")
FOR_LOOP = re.compile(r"\bfor\s+[^{};]*?\bin\s+")
# A free or path function call (not a method call, not a macro, not a tuple
# constructor): `probe()`, `std::fs::read_dir(..)`, `crate::x::list(..)`.
FREE_CALL = re.compile(r"(?<![\w.:!])(?:[a-z_]\w*::)*[a-z_]\w*\s*\(")

# `#[cfg(test)]`, `#[cfg(all(test, ..))]`, `#[cfg(any(test, ..))]`: code that is
# compiled into the test build and exists for it.
CFG_TEST = re.compile(r"#\s*\[\s*cfg\s*\(\s*(?:(?:all|any)\s*\(\s*)?test\b")
MOD_DECL = re.compile(r"\bmod\s+([A-Za-z_]\w*)\s*;")


class GateError(Exception):
    pass


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


def git_env():
    """The environment for git with every GIT_* variable removed, so GIT_DIR /
    GIT_WORK_TREE cannot redirect the file list to a decoy repository."""
    return {k: v for k, v in os.environ.items() if not k.startswith("GIT_")}


def iter_files(root):
    """Every TRACKED `.rs` file under SCAN_DIRS (`git ls-files`). A root that is
    not a git work tree is an error, never a fallback to walking the disk."""
    try:
        out = subprocess.run(
            ["git", "-C", root, "ls-files", "-z", "--", *SCAN_DIRS],
            capture_output=True, env=git_env(), check=False,
        )
    except OSError as e:
        raise GateError(f"cannot run git: {e}") from e
    if out.returncode != 0:
        raise GateError(
            f"`git ls-files` failed in {root} (exit {out.returncode}): "
            f"{out.stderr.decode(errors='replace').strip()} -- G-SKIP scans tracked "
            f"files only and needs a git work tree"
        )
    for rel in out.stdout.decode("utf-8", errors="surrogateescape").split("\0"):
        if rel.endswith(".rs"):
            p = os.path.join(root, rel)
            if os.path.isfile(p):
                yield p


def fn_bodies(clean):
    """Yield (name, name_idx, body_open, body_end) for every fn with a body."""
    for m in FN_RE.finditer(clean):
        b = clean.find("{", m.end())
        semi = clean.find(";", m.end())
        if b == -1 or (semi != -1 and semi < b):
            continue
        yield m.group(1), m.start(), b, _find_balanced_block(clean, b)


def nested_ranges(clean, start, end):
    """(open, end) of every closure / async block / nested fn body in [start, end)."""
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


def balanced_args(clean, open_idx):
    """For the delimiter at `open_idx` (`(`, `[` or `{`), the index just past its
    match (nesting counted over all three kinds)."""
    depth, i, n = 0, open_idx, len(clean)
    while i < n:
        c = clean[i]
        if c in "([{":
            depth += 1
        elif c in ")]}":
            depth -= 1
            if depth == 0:
                return i + 1
        i += 1
    return n


def split_top(s, sep=","):
    """Split `s` at `sep` characters outside any bracket."""
    parts, depth, cur = [], 0, 0
    for i, c in enumerate(s):
        if c in "([{":
            depth += 1
        elif c in ")]}":
            depth -= 1
        elif c == sep and depth == 0:
            parts.append(s[cur:i])
            cur = i + 1
    parts.append(s[cur:])
    return parts


def assertions(clean, lo, hi):
    """(start, name, args) of every assertion-family macro in clean[lo:hi]."""
    out = []
    for m in ASSERTS.finditer(clean, lo, hi):
        name = re.match(r"\w+", clean[m.start():]).group(0)
        j = m.end()
        while j < len(clean) and clean[j].isspace():
            j += 1
        if j < len(clean) and clean[j] in "([{":
            args = clean[j + 1:balanced_args(clean, j) - 1]
        else:
            args = ""
        out.append((m.start(), name, args))
    return out


def trivial_assertion(name, args):
    """An assertion that cannot fail (or is compiled out of release builds)."""
    if name.startswith("debug_"):
        return True
    if name in FAIL_MACROS:
        return False
    parts = [p.strip() for p in split_top(args)]
    if name == "assert":
        return not (set(IDENT.findall(parts[0])) - LITERAL_WORDS)
    # assert_eq / assert_ne
    if len(parts) >= 2:
        a, b = re.sub(r"\s+", "", parts[0]), re.sub(r"\s+", "", parts[1])
        if a == b:
            return True
        return not ((set(IDENT.findall(a)) | set(IDENT.findall(b))) - LITERAL_WORDS)
    return False


def nontrivial_asserts(clean, lo, hi):
    return [a for a in assertions(clean, lo, hi) if not trivial_assertion(a[1], a[2])]


def has_nontrivial_assert(text_clean):
    return bool(nontrivial_asserts(text_clean, 0, len(text_clean)))


def unconditional_assert(seg):
    """A non-trivial assertion at the top nesting level of `seg` (a guard
    block's text up to the return): one inside a nested `if` / loop / closure
    in the guard runs only sometimes, so it cannot excuse the return."""
    for start, _name, _args in nontrivial_asserts(seg, 0, len(seg)):
        depth = 0
        for c in seg[:start]:
            if c == "{":
                depth += 1
            elif c == "}":
                depth -= 1
        if depth == 0:
            return True
    return False


# ---------------------------------------------------------------------------
# Test-only code
# ---------------------------------------------------------------------------
def cfg_test_items(clean):
    """(attr_start, item_start, item_end) of every `#[cfg(test)]`-scoped item:
    the attribute's item runs to its `{..}` block or to its `;`."""
    out = []
    for m in CFG_TEST.finditer(clean):
        close = balanced_args(clean, clean.find("[", m.start()))
        b = clean.find("{", close)
        semi = clean.find(";", close)
        if b == -1 and semi == -1:
            continue
        if b == -1 or (semi != -1 and semi < b):
            out.append((m.start(), close, semi + 1))
        else:
            out.append((m.start(), close, _find_balanced_block(clean, b)))
    return out


def mod_decl_path(decl_file, name, attrs):
    """The file a `mod name;` declaration in `decl_file` loads (rustc's rule)."""
    d = os.path.dirname(decl_file)
    pm = re.search(r'#\s*\[\s*path\s*=\s*"([^"]+)"\s*\]', attrs)
    if pm:
        return os.path.normpath(os.path.join(d, pm.group(1)))
    base = os.path.basename(decl_file)
    if base not in ("mod.rs", "lib.rs", "main.rs"):
        d = os.path.join(d, os.path.splitext(base)[0])
    for cand in (os.path.join(d, name + ".rs"), os.path.join(d, name, "mod.rs")):
        if os.path.isfile(cand):
            return os.path.normpath(cand)
    return os.path.normpath(os.path.join(d, name + ".rs"))


def test_only_files(files):
    """The set of paths that are test-only as a whole: every file under a
    `tests/` directory, plus every file a test-scoped `mod name;` loads
    (transitively). `files` maps path -> (text, clean)."""
    whole = {p for p in files if f"{os.sep}tests{os.sep}" in p}
    changed = True
    while changed:
        changed = False
        for p, (text, clean) in files.items():
            regions = [(0, len(clean))] if p in whole else [
                (a, e) for a, _i, e in cfg_test_items(clean)]
            for m in MOD_DECL.finditer(clean):
                if not any(lo <= m.start() < hi for lo, hi in regions):
                    continue
                prev = max(clean.rfind("}", 0, m.start()), clean.rfind(";", 0, m.start()))
                attrs = text[prev + 1:m.start()]
                child = mod_decl_path(p, m.group(1), attrs)
                if child in files and child not in whole:
                    whole.add(child)
                    changed = True
    return whole


def test_regions(path, clean, whole):
    """(start, end) ranges of test-only code in one file."""
    if path in whole:
        return [(0, len(clean))]
    return [(a, e) for a, _i, e in cfg_test_items(clean)]


# ---------------------------------------------------------------------------
# Pivots and pivoting helpers
# ---------------------------------------------------------------------------
def local_call_rx(names):
    """A bare / `Self::` / `Type::` call to one of `names` (fns of the same
    file), capturing the name. Method calls (`.name(`) and other paths
    (`thread::spawn`) never count."""
    if not names:
        return None
    return re.compile(r"(?<![.\w:])(?:[A-Z]\w*::)?(" + "|".join(
        map(re.escape, sorted(names, key=len, reverse=True))) + r")\s*\(")


def shared_flags(flags):
    """{name: [READS, FAILS]} over every `test_support.rs` file."""
    shared = {}
    for p, fl in flags.items():
        if os.path.basename(p) == "test_support.rs":
            for n, f in fl.items():
                sf = shared.setdefault(n, [False, False])
                sf[0] |= f[0]
                sf[1] |= f[1]
    return shared


def analyze_helpers(files, whole):
    """Per file, {fn name: (READS, FAILS)} for its test-only non-test fns,
    closed transitively over same-file calls, `test_support::` calls and calls
    to verified pivots. READS: reaches a `"RUSTTY_REQUIRE_<X>"` literal. FAILS:
    reaches a non-trivial assertion or panic. Also returns the verified pivot
    names and the PIVOT findings for pivot-named fns that are not real pivots."""
    bodies = {}  # path -> {name: (raw, clean)}
    for path, (text, clean) in files.items():
        regions = test_regions(path, clean, whole)
        tests = set()
        for m in TEST_ATTR.finditer(clean):
            fm = FN_RE.search(clean, m.end())
            if fm:
                tests.add(fm.start())
        fns = {}
        for name, n_idx, b, e in fn_bodies(clean):
            if n_idx in tests or not any(lo <= n_idx < hi for lo, hi in regions):
                continue
            prev = fns.get(name)
            raw, cl = text[b:e], clean[b:e]
            fns[name] = (prev[0] + raw, prev[1] + cl) if prev else (raw, cl)
        bodies[path] = fns

    flags = {p: {n: [bool(REQUIRE_VAR.search(r)), has_nontrivial_assert(c)]
                 for n, (r, c) in fns.items()} for p, fns in bodies.items()}
    # Each fn's calls, computed once: ("l", same-file fn), ("s", test_support
    # fn), ("p", pivot-named fn).
    calls = {}
    for p, fns in bodies.items():
        rx = local_call_rx(fns)
        for name, (_raw, cl) in fns.items():
            cs = set()
            if rx:
                cs |= {("l", m.group(1)) for m in rx.finditer(cl) if m.group(1) != name}
            cs |= {("s", m.group(1)) for m in re.finditer(r"\btest_support::(\w+)\s*\(", cl)}
            cs |= {("p", m.group(1)) for m in PIVOT_CALL.finditer(cl)}
            calls[(p, name)] = cs
    pivots = set()
    changed = True
    while changed:
        changed = False
        shared = shared_flags(flags)
        new_pivots = {n for p, fl in flags.items() for n, f in fl.items()
                      if (PIVOT_NAME.match(n) and f[0] and f[1])
                      or (n == "init_for_display_test" and f[1])}
        if new_pivots != pivots:
            pivots = new_pivots
            changed = True
        for p, fl in flags.items():
            for name in fl:
                f = fl[name]
                if f[0] and f[1]:
                    continue
                for kind, other in calls[(p, name)]:
                    if kind == "l":
                        g = fl.get(other, [False, False])
                    elif kind == "s":
                        g = shared.get(other, [False, False])
                    else:
                        g = [True, True] if other in pivots else [False, False]
                    nf = [f[0] or g[0], f[1] or g[1]]
                    if nf != f:
                        fl[name] = f = nf
                        changed = True
    findings = []
    for p, fl in flags.items():
        for name, (reads, fails) in fl.items():
            if PIVOT_NAME.match(name) and not (reads and fails):
                why = []
                if not reads:
                    why.append('reads no "RUSTTY_REQUIRE_<X>" variable')
                if not fails:
                    why.append("never fails (no non-debug assertion or panic)")
                findings.append((p, name, " and ".join(why)))
            if name == "init_for_display_test" and not fails:
                findings.append((p, name, "never fails (no panic or assertion)"))
    return flags, shared_flags(flags), pivots, findings


def guard_pivoted(ctx, local_flags, local_rx, shared, pivots, raw_ctx):
    """True when the guard text `ctx` goes through a strict-mode pivot: it calls
    a verified pivot, or the helpers it calls (plus its own text) together both
    read a RUSTTY_REQUIRE_<X> variable and fail."""
    for pm in PIVOT_CALL.finditer(ctx):
        if pm.group(1) in pivots:
            return True
    reads = bool(REQUIRE_VAR.search(raw_ctx))
    fails = has_nontrivial_assert(ctx)
    if local_rx:
        for m in local_rx.finditer(ctx):
            r, f = local_flags.get(m.group(1), (False, False))
            reads |= r
            fails |= f
    for sm in re.finditer(r"\btest_support::(\w+)\s*\(", ctx):
        r, f = shared.get(sm.group(1), (False, False))
        reads |= r
        fails |= f
    return reads and fails


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


def is_match_block(clean, lo, brace):
    """True when the `{` at `brace` opens a `match` expression's arm list."""
    st = statement_start(clean, lo, brace)
    head = clean[st:brace]
    return re.search(r"\bmatch\b", head) is not None and "=>" not in head


def guard_segment(clean, body_open, idx, body_opens):
    """The text that must hold the assertion excusing a `return` at `idx`: the
    whole guard block for a braced guard (`if` / `else` / let-else / braced
    arm); only the statement for a return directly in a fn or closure body
    (`cond || return;`); only the arm for an unbraced match arm
    (`None => return`)."""
    blk = enclosing_open(clean, body_open, idx)
    if blk in body_opens:
        i = idx - 1
        depth = 0
        while i > blk:
            c = clean[i]
            if c in ")]}":
                depth += 1
            elif c in "([{":
                depth -= 1
            elif depth == 0 and c == ";":
                break
            i -= 1
        return clean[i + 1:idx]
    if is_match_block(clean, body_open, blk):
        i = idx - 1
        depth = 0
        while i > blk:
            c = clean[i]
            if c in ")]":
                depth += 1
            elif c in "([":
                depth -= 1
            elif c == "}" and depth == 0:
                break  # end of the previous (braced) arm
            elif c == "," and depth == 0:
                break
            i -= 1
        return clean[i + 1:idx]
    return clean[blk + 1:idx]


# ---------------------------------------------------------------------------
# Body shapes
# ---------------------------------------------------------------------------
def cfg_empty(clean, b, e):
    """True when every top-level statement of the body is `#[cfg(..)]`-gated."""
    body = clean[b + 1:e - 1]
    i, n, saw_cfg = 0, len(body), False
    rest, preds, empty_arm = [], set(), False
    while i < n:
        m = CFG_ATTR.match(body, i) if body[i] == "#" else None
        if m:
            saw_cfg = True
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


def if_branches(st):
    """For a statement starting with `if`, (branch blocks, has_final_else)."""
    blocks, i, n = [], 0, len(st)
    while True:
        depth, j = 0, i
        while j < n:
            c = st[j]
            if c in "([":
                depth += 1
            elif c in ")]":
                depth -= 1
            elif c == "{" and depth == 0:
                break
            j += 1
        if j >= n:
            return blocks, False
        end = _find_balanced_block(st, j)
        blocks.append(st[j + 1:end - 1])
        k = end
        while k < n and st[k].isspace():
            k += 1
        if not st.startswith("else", k):
            return blocks, False
        k += 4
        while k < n and st[k].isspace():
            k += 1
        if st.startswith("if", k) and not re.match(r"if\w", st[k:]):
            i = k + 2
            continue
        if k < n and st[k] == "{":
            end2 = _find_balanced_block(st, k)
            blocks.append(st[k + 1:end2 - 1])
            return blocks, True
        return blocks, False


def fail_only(block):
    """A branch that is itself a failure (`panic!`, `unreachable!`, ...): the
    `if` is an assertion, not a wrapper."""
    m = re.match(r"\s*(\w+)\s*!", block)
    return bool(m and m.group(1) in FAIL_MACROS)


def silent(block, pivots):
    """A branch that checks nothing: no non-trivial assertion, no pivot call,
    no `return` (a return is RET's to judge)."""
    if has_nontrivial_assert(block) or RET.search(block):
        return False
    return not any(pm.group(1) in pivots for pm in PIVOT_CALL.finditer(block))


def wrapped_in_if(clean, b, e, pivots):
    """True when every non-trivial assertion of the test sits inside the
    branches of ONE `if` while some path through it (no `else`, or a silent
    branch) asserts nothing."""
    stmts = top_level_statements(clean[b + 1:e - 1])
    for idx, st in enumerate(stmts):
        bare = re.sub(r"^(?:#\[[^\]]*\]\s*)+", "", st)
        if not re.match(r"if\b", bare):
            continue
        outside = " ".join(stmts[:idx] + stmts[idx + 1:])
        if has_nontrivial_assert(outside) or not has_nontrivial_assert(bare):
            continue
        # `if let Err(e) = r { panic!() }` is an assertion, not a skip.
        if re.match(r"if\s+let\s+Err\s*\(", bare) or re.match(r"if\s+[^{]*\.is_err\(\)", bare):
            continue
        blocks, final_else = if_branches(bare)
        asserting = [bl for bl in blocks if has_nontrivial_assert(bl)]
        if asserting and all(fail_only(bl) for bl in asserting):
            continue
        if not final_else or any(silent(bl, pivots) for bl in blocks):
            return True
    return False


def match_arms(clean, brace):
    """(pattern, body) of every arm of the match block opening at `brace`."""
    end = _find_balanced_block(clean, brace)
    i, n, arms = brace + 1, end - 1, []
    while i < n:
        while i < n and (clean[i].isspace() or clean[i] == ","):
            i += 1
        if i >= n:
            break
        arrow = -1
        depth, j = 0, i
        while j < n:
            c = clean[j]
            if c in "([{":
                depth += 1
            elif c in ")]}":
                depth -= 1
            elif depth == 0 and clean.startswith("=>", j):
                arrow = j
                break
            j += 1
        if arrow == -1:
            break
        pat = clean[i:arrow]
        k = arrow + 2
        while k < n and clean[k].isspace():
            k += 1
        if k < n and clean[k] == "{":
            bend = _find_balanced_block(clean, k)
            arms.append((pat, clean[k + 1:bend - 1]))
            i = bend
            continue
        depth, j = 0, k
        while j < n:
            c = clean[j]
            if c in "([{":
                depth += 1
            elif c in ")]}":
                depth -= 1
            elif c == "," and depth == 0:
                break
            j += 1
        arms.append((pat, clean[k:j]))
        i = j + 1
    return arms


def print_only(body):
    """Empty, `()`, or nothing but print macros."""
    s = body
    while True:
        m = PRINT_MACRO.search(s)
        if not m:
            break
        close = balanced_args(s, m.end() - 1)
        s = s[:m.start()] + s[close:]
    return not re.sub(r"[\s;,()]", "", s)


def silent_arm_matches(clean, b, e):
    """Every `match` in the test with an asserting arm and a silent
    `None` / `Err(..)` arm, in a test with no non-trivial assertion outside
    that match (as for WRAP: a test that checks something on every path is not
    vacuous, and an `Err(_) => {}` arm there is an accepted outcome): yields the
    match's offset."""
    for m in MATCH_KW.finditer(clean, b, e):
        depth, j = 0, m.end()
        while j < e:
            c = clean[j]
            if c in "([":
                depth += 1
            elif c in ")]":
                depth -= 1
            elif c == "{" and depth == 0:
                break
            elif c == ";" and depth == 0:
                j = e
                break
            j += 1
        if j >= e:
            continue
        arms = match_arms(clean, j)
        if not any(has_nontrivial_assert(body) for _p, body in arms):
            continue
        end = _find_balanced_block(clean, j)
        if nontrivial_asserts(clean, b, m.start()) or nontrivial_asserts(clean, end, e):
            continue
        for pat, body in arms:
            if re.match(r"\s*(?:None|Err)\b", pat) and print_only(body):
                yield m.start()
                break


def loop_only(clean, b, e):
    """True when the test has non-trivial assertions and every one of them sits
    inside a `for` loop over a function call's result or inside a closure
    passed to a map-like combinator."""
    asserts = nontrivial_asserts(clean, b, e)
    if not asserts:
        return False
    regions = []
    for m in FOR_LOOP.finditer(clean, b, e):
        depth, j = 0, m.end()
        while j < e:
            c = clean[j]
            if c in "([":
                depth += 1
            elif c in ")]":
                depth -= 1
            elif c == "{" and depth == 0:
                break
            j += 1
        if j >= e:
            continue
        iterable = clean[m.end():j]
        # A literal table (`[..]`, `&[..]`, `vec![..]`) is not a probe result,
        # whatever it calls to build its rows.
        if re.match(r"\s*(?:&\s*(?:mut\s+)?)?(?:\[|vec\s*!)", iterable):
            continue
        if FREE_CALL.search(iterable):
            regions.append((j, _find_balanced_block(clean, j)))
    for m in MAP_LIKE.finditer(clean, b, e):
        regions.append((m.end() - 1, balanced_args(clean, m.end() - 1)))
    return all(any(s <= a[0] < t for s, t in regions) for a in asserts)


# ---------------------------------------------------------------------------
def scan(root):
    files = {}
    for p in iter_files(root):
        text = open(p, encoding="utf-8", errors="replace").read()
        clean, _ = _strip_rust(text)
        files[p] = (text, clean)
    whole = test_only_files(files)
    flags, shared, pivots, pivot_findings = analyze_helpers(files, whole)

    findings, accepted = [], []
    for p, name, why in pivot_findings:
        findings.append(("PIVOT", f"{os.path.relpath(p, root)} [{name}]",
                         f"a pivot-named fn that is not a strict-mode pivot: {why}"))
    guard_hits = {i: 0 for i in range(len(ALLOW_GUARDS))}
    site_hits = {k: 0 for k in ALLOW_SITES}

    def flag(kind, key, site, why):
        if key in ALLOW_SITES:
            site_hits[key] += 1
            accepted.append((kind, site, "allow-site"))
        else:
            findings.append((kind, site, why))

    for p, (text, clean) in sorted(files.items()):
        rel = os.path.relpath(p, root)
        local = flags.get(p, {})
        local_rx = local_call_rx(local)
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
            site = f"{rel}:{lineno(text, b)} [{name}]"
            if not scoped:
                if cfg_empty(clean, b, e):
                    flag("EMPTY", key, site, "every statement of the body is #[cfg]-gated")
                if wrapped_in_if(clean, b, e, pivots):
                    flag("WRAP", key, site,
                         "every assertion is inside one `if` with a path that asserts nothing")
                for at in silent_arm_matches(clean, b, e):
                    flag("ARM", key, f"{rel}:{lineno(text, at)} [{name}]",
                         "a match with an asserting arm and a silent None/Err arm")
                if loop_only(clean, b, e):
                    flag("LOOP", key, site,
                         "every assertion is inside a loop / combinator over a probe "
                         "result that may be empty")
            nr = nested_ranges(clean, b + 1, e)
            body_opens = {b} | {s for s, _t in nr}
            for r in RET.finditer(clean, b, e):
                inner = [(s, t) for s, t in nr if s < r.start() < t]
                nested = bool(inner)
                if nested:
                    after = clean[r.end():r.end() + 40].lstrip()
                    if after[:1] not in (";", "}", ",", ")"):
                        continue  # a closure/nested fn returning a value
                    # A callback or worker that asserts nothing after its
                    # return skips no check; one that does is the test body
                    # carried one level down.
                    s_in, t_in = min(inner, key=lambda st: st[1] - st[0])
                    if not nontrivial_asserts(clean, r.end(), t_in):
                        continue
                blk, ctx = guard_context(clean, b, r.start())
                raw_ctx = text[r.start() - len(ctx):r.start()]
                seg = guard_segment(clean, b, r.start(), body_opens)
                rsite = f"{rel}:{lineno(text, r.start())} [{name}]"
                why = None
                if unconditional_assert(seg):
                    why = "assertion-before-return"
                elif guard_pivoted(ctx, local, local_rx, shared, pivots, raw_ctx):
                    why = "pivot"
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
                    where = " (in a closure / nested fn)" if nested else ""
                    findings.append(("RET", rsite,
                                     "early return with no assertion or strict-mode pivot" + where))
                else:
                    accepted.append(("RET", rsite, why))
        # MSG: a "skipping" line anywhere in test-only code (helpers included)
        # must follow a pivot, a pivoting-helper call or an assertion in its
        # block -- a helper that prints "skip" and returns is the same silent
        # pass one call away.
        for lo, hi in test_regions(p, clean, whole):
            for mm in PRINT_MACRO.finditer(clean, lo, hi):
                open_paren = clean.find("(", mm.end() - 1)
                close = balanced_args(clean, open_paren)
                if not SKIP_WORD.search(text[open_paren:close]):
                    continue
                _blk, ctx = guard_context(clean, lo, mm.start())
                raw_ctx = text[mm.start() - len(ctx):mm.start()]
                fn_name = enclosing_fn_name(clean, mm.start())
                key = (rel, fn_name)
                site = f"{rel}:{lineno(text, mm.start())} [{fn_name}]"
                if has_nontrivial_assert(ctx) or guard_pivoted(ctx, local, local_rx, shared, pivots, raw_ctx):
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
    try:
        findings, accepted, gh, sh = scan(root)
    except GateError as e:
        print(f"  RED  ERROR {e}")
        print("G-SKIP: ERROR (no verdict)")
        return 1
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


# A real pivot, prepended to the self-test sources that need one.
PIVOT_DEF = """
#[cfg(test)]
mod pivots {
    pub(crate) fn require_metal_or_skip(what: &str) {
        let required = std::env::var_os("RUSTTY_REQUIRE_METAL").is_some();
        assert!(!required, "Metal required: {what}");
    }
}
"""

SELF_CASES = [
    # (name, source, expect): expect is False (GREEN) or the comma-separated
    # finding kinds the arm must produce -- exactly those, so an arm cannot
    # pass by going RED for some other reason.
    ("silent let-else", """
#[test]
fn t() {
    let Some(d) = probe_device() else {
        eprintln!("no device; skipping");
        return;
    };
    assert!(d.ok());
}
""", "RET"),
    ("silent comptime guard", """
#[test]
fn t() {
    if !cfg!(target_os = "macos") {
        return;
    }
    assert!(d.ok());
}
""", "RET"),
    ("silent match arm", """
#[test]
fn t() {
    let d = match probe_device() { Some(d) => d, None => return };
    assert!(d.ok());
}
""", "RET"),
    ("cfg-only body", """
#[test]
fn t() {
    #[cfg(windows)]
    {
        assert!(x.ok());
    }
}
""", "EMPTY"),
    ("ignored but still silent", """
#[test]
#[ignore = "needs a display"]
fn t() {
    let Some(h) = display() else { return; };
    assert!(h.ok());
}
""", "RET"),
    ("inverted skip (all assertions under one if)", """
#[test]
fn t() {
    if BACKEND == Backend::Fontconfig {
        assert!(load().is_ok());
    }
}
""", "WRAP"),
    ("helper that skips silently", """
#[cfg(test)]
mod tests {
    fn device() -> Option<u8> {
        eprintln!("no device; skipping");
        None
    }
}
""", "MSG"),
    ("pivot before return", PIVOT_DEF + """
#[test]
fn t() {
    let Some(d) = probe_device() else {
        pivots::require_metal_or_skip("t");
        return;
    };
    assert!(d.ok());
}
""", False),
    ("pivoting helper", PIVOT_DEF + """
#[cfg(test)]
mod tests {
    fn offscreen() -> Option<u8> {
        match open() { Ok(x) => Some(x), Err(e) => { require_metal_or_skip("offscreen"); None } }
    }
    #[test]
    fn t() {
        let Some(g) = offscreen() else { return; };
        assert!(g.ok());
    }
}
""", False),
    ("same-named fn elsewhere is not the helper", PIVOT_DEF + """
#[cfg(test)]
mod tests {
    fn spawn() -> Option<u8> { require_metal_or_skip("x"); None }
    #[test]
    fn t() {
        let Some(h) = std::thread::spawn(|| 1).join().ok() else { return; };
        assert!(h.ok());
    }
}
""", "RET"),
    ("production fn is never a helper", """
pub fn offscreen() -> Option<u8> {
    if std::env::var_os("RUSTTY_REQUIRE_GL").is_some() { panic!() }
    None
}
#[cfg(test)]
mod tests {
    #[test]
    fn t() {
        let Some(g) = super::offscreen() else { return; };
        assert!(g.ok());
    }
}
""", "RET"),
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
    ("closure returning a value is not a skip", """
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
        assert!(x.ok());
    }
}
""", False),
    ("a Miri-only ignore does not scope a cfg-only body", """
#[test]
#[cfg_attr(miri, ignore)]
fn t() {
    #[cfg(not(windows))]
    {
        assert!(x.ok());
    }
}
""", "EMPTY"),
    ("attribute scope is fine", """
#[test]
#[cfg_attr(not(target_os = "macos"), ignore = "macOS only")]
fn t() {
    assert!(x.ok());
}
""", False),
    # --- S-15 (S-3b): the review's bypasses, each planted RED, plus clean arms --
    ("B1 an earlier assertion does not excuse a later top-level return", """
#[test]
fn t() {
    assert_eq!(parse("1"), 1);
    matches!(probe(), Some(_)) || return;
    assert!(run().ok());
}
""", "RET"),
    ("B1 an earlier arm's assertion does not excuse a None => return", """
#[test]
fn t() {
    match probe() {
        Some(d) => assert!(d.ok()),
        None => return,
    }
}
""", "RET"),
    ("B1 a trivially-true assertion does not excuse a return", """
#[test]
fn t() {
    if probe().is_none() {
        assert!(true);
        return;
    }
    assert!(run().ok());
}
""", "RET"),
    ("B1 a debug_assert does not excuse a return", """
#[test]
fn t() {
    let Some(d) = probe() else {
        debug_assert!(d_missing());
        return;
    };
    assert!(d.ok());
}
""", "RET"),
    ("B1 clean: a real assertion in the guard block", """
#[test]
fn t() {
    let Some(d) = probe() else {
        assert!(expected_absent(), "the probe must find the device here");
        return;
    };
    assert!(d.ok());
}
""", False),
    ("B2 a match with an empty None arm", """
#[test]
fn t() {
    match probe() {
        Some(d) => assert!(d.ok()),
        None => {}
    }
}
""", "ARM"),
    ("B2 a match with a print-only Err arm", """
#[test]
fn t() {
    match open() {
        Ok(d) => { assert!(d.ok()); }
        Err(e) => { eprintln!("unavailable: {e}"); }
    }
    assert_eq!(1 + 1, 2);
}
""", "ARM"),
    ("B2 clean: the None arm asserts", """
#[test]
fn t() {
    match probe() {
        Some(d) => assert!(d.ok()),
        None => panic!("no device"),
    }
}
""", False),
    ("B3 an else that only prints, without the word skip", """
#[test]
fn t() {
    if let Some(d) = probe() {
        assert!(d.ok());
    } else {
        eprintln!("device unavailable");
    }
}
""", "WRAP"),
    ("B3 an if-let wrapper plus a trivial assertion outside it", """
#[test]
fn t() {
    assert!(true);
    if let Some(d) = probe() {
        assert!(d.ok());
    }
}
""", "WRAP"),
    ("B3 clean: both branches assert", """
#[test]
fn t() {
    if cfg!(windows) {
        assert_eq!(sep(), '\\\\');
    } else {
        assert_eq!(sep(), '/');
    }
}
""", False),
    ("B3 clean: an if that is itself a failure check", """
#[test]
fn t() {
    let r = run();
    if !r.ok() {
        panic!("run failed: {r:?}");
    }
}
""", False),
    ("B4 a bare return inside a closure that is the whole test", """
#[test]
fn t() {
    let body = || {
        let Some(d) = probe() else { return; };
        assert!(d.ok());
    };
    body();
}
""", "RET"),
    ("B4 a bare return inside a nested fn that is the whole test", """
#[test]
fn t() {
    fn inner() {
        if probe().is_none() {
            return;
        }
        assert!(run().ok());
    }
    inner();
}
""", "RET"),
    ("B4 clean: a closure's return after a pivot", PIVOT_DEF + """
#[test]
fn t() {
    let body = || {
        let Some(d) = probe() else { pivots::require_metal_or_skip("t"); return; };
        assert!(d.ok());
    };
    body();
}
""", False),
    ("B5 a no-op named require_x_or_skip is not a pivot", """
#[cfg(test)]
mod tests {
    fn require_metal_or_skip(_what: &str) {}
    #[test]
    fn t() {
        let Some(d) = probe() else {
            require_metal_or_skip("t");
            return;
        };
        assert!(d.ok());
    }
}
""", "PIVOT,RET"),
    ("B5 a pivot that reads the variable but never fails", """
#[cfg(test)]
mod tests {
    fn require_metal_or_skip(_what: &str) {
        let _ = std::env::var_os("RUSTTY_REQUIRE_METAL");
        eprintln!("no metal");
    }
}
""", "PIVOT"),
    ("B5 clean: split pivot (reader + failing impl) through a helper", """
#[cfg(test)]
mod tests {
    fn metal_required() -> bool {
        std::env::var_os("RUSTTY_REQUIRE_METAL").as_deref() == Some(std::ffi::OsStr::new("1"))
    }
    fn require_metal_or_skip_impl(required: bool, what: &str) {
        assert!(!required, "Metal required but unavailable: {what}");
    }
    fn require_metal_or_skip(what: &str) {
        require_metal_or_skip_impl(metal_required(), what);
    }
    #[test]
    fn t() {
        let Some(d) = probe() else {
            require_metal_or_skip("t");
            return;
        };
        assert!(d.ok());
    }
}
""", False),
    ("B6 every assertion inside a for over a probe result", """
#[test]
fn t() {
    for d in probe_devices() {
        assert!(d.ok());
    }
}
""", "LOOP"),
    ("B6 every assertion inside .map over an Option", """
#[test]
fn t() {
    probe().map(|d| assert!(d.ok()));
}
""", "LOOP"),
    ("B6 clean: a for over a literal table", """
#[test]
fn t() {
    for (input, want) in [("1", 1), ("2", 2)] {
        assert_eq!(parse(input), want);
    }
}
""", False),
    ("B6 clean: the probe result is asserted non-empty first", """
#[test]
fn t() {
    let ds = probe_devices();
    assert!(!ds.is_empty(), "no devices");
    for d in ds {
        assert!(d.ok());
    }
}
""", False),
    ("B7 an #[rstest] test is scanned", """
#[rstest]
fn t() {
    let Some(d) = probe() else { return; };
    assert!(d.ok());
}
""", "RET"),
    ("B7 a #[test_case(..)] test is scanned", """
#[test_case(1)]
fn t(n: u8) {
    let Some(d) = probe(n) else { return; };
    assert!(d.ok());
}
""", "RET"),
    ("B1 an assertion inside a conditional in the guard does not excuse the return", """
#[test]
fn t() {
    if !PORTED {
        if env!("LEDGER") == "1" {
            assert_eq!("1", env!("LEDGERED"));
        }
        return;
    }
    assert!(run().ok());
}
""", "RET"),
    ("B4 clean: a callback's bare return that skips no assertion", """
#[test]
fn t() {
    fn on_event(p: Option<u8>, out: &mut Vec<u8>) {
        let Some(p) = p else { return; };
        out.push(p);
    }
    let mut out = Vec::new();
    on_event(Some(1), &mut out);
    assert_eq!(out, vec![1]);
}
""", False),
    ("B2 clean: an Err arm that is an accepted outcome in a test that asserts elsewhere", """
#[test]
fn t() {
    let png = encode();
    assert!(png.len() > 8);
    match decode(&corrupt(png)) {
        Err(_) => {}
        Ok(d) => assert_ne!(d, original()),
    }
}
""", False),
    ("B6 clean: a literal table whose rows call a fn", """
#[test]
fn t() {
    for &(want, target) in &[(load_target(1), Target::Normal), (load_target(2), Target::Light)] {
        assert_eq!(want, target.flags());
    }
}
""", False),
    ("MSG accept: a pivot before the skip line", PIVOT_DEF + """
#[cfg(test)]
mod tests {
    fn device() -> Option<u8> {
        super::pivots::require_metal_or_skip("device");
        eprintln!("no device; skipping");
        None
    }
}
""", False),
    ("WRAP accept: an if-wrapped test with a scoped ignore", """
#[test]
#[cfg_attr(not(feature = "fontconfig"), ignore = "fontconfig only (R14)")]
fn t() {
    if BACKEND == Backend::Fontconfig {
        assert!(load().is_ok());
    }
}
""", False),
]

# B8: the scanned file set. (path relative to the tree -> source); `untracked`
# names files written to disk but never `git add`ed; expect as in SELF_CASES.
FILESET_CASES = [
    ("B8 an untracked scratch file is not scanned", {
        "crates/x/src/lib.rs": "#[test]\nfn t() { assert!(x.ok()); }\n",
        "crates/x/src/scratch.rs": "#[test]\nfn t() { let Some(d) = p() else { return; }; assert!(d.ok()); }\n",
    }, {"crates/x/src/scratch.rs"}, False),
    ("B8 a tracked file with a skip is scanned", {
        "crates/x/src/lib.rs": "#[test]\nfn t() { let Some(d) = p() else { return; }; assert!(d.ok()); }\n",
    }, set(), "RET"),
    ("B8 a production file NAMED like a test file is not test code", {
        "crates/x/src/lib.rs": "pub mod latest;\npub mod testing;\n",
        "crates/x/src/latest.rs": "pub fn warn() { eprintln!(\"skipping stale entry\"); }\n",
        "crates/x/src/testing.rs": "pub fn log() { println!(\"skipped: nothing to do\"); }\n",
    }, set(), False),
    ("B8 a file loaded by a #[cfg(test)] mod decl IS test code", {
        "crates/x/src/lib.rs": "#[cfg(test)]\nmod fixtures;\n",
        "crates/x/src/fixtures.rs": "pub fn device() -> Option<u8> { eprintln!(\"no device; skipping\"); None }\n",
    }, set(), "MSG"),
    ("B8 a #[path]-loaded test module IS test code", {
        "crates/x/src/lib.rs": "pub mod shaper;\n",
        "crates/x/src/shaper.rs": "#[cfg(test)]\n#[path = \"shaper_checks.rs\"]\nmod tests;\n",
        "crates/x/src/shaper_checks.rs": "fn font() -> Option<u8> { eprintln!(\"font missing; skipping\"); None }\n",
    }, set(), "MSG"),
    ("B8 a cfg(any(test, ..)) item is test code", {
        "crates/x/src/lib.rs": "#[cfg(any(test, feature = \"testing\"))]\npub fn device() -> Option<u8> { eprintln!(\"skipping\"); None }\n",
    }, set(), "MSG"),
]


def git_tree(tmp, files, untracked=()):
    for rel, src in files.items():
        full = os.path.join(tmp, rel)
        os.makedirs(os.path.dirname(full), exist_ok=True)
        with open(full, "w") as fh:
            fh.write(src)
    env = git_env()
    subprocess.run(["git", "-C", tmp, "init", "-q"], check=True, env=env)
    tracked = [r for r in files if r not in untracked]
    if tracked:
        subprocess.run(["git", "-C", tmp, "add", "--", *tracked], check=True, env=env)


def self_test():
    ok = True

    def check(name, findings, expect):
        got = ",".join(sorted({k for k, _s, _w in findings}))
        want = ",".join(sorted(expect.split(","))) if expect else ""
        good = got == want
        print(f"  [{'PASS' if good else 'FAIL'}] {name}: expected "
              f"{('RED ' + want) if want else 'GREEN'}, got {('RED ' + got) if got else 'GREEN'}")
        return good

    for name, src, expect in SELF_CASES:
        with tempfile.TemporaryDirectory(prefix="gskip_") as tmp:
            git_tree(tmp, {"crates/x/src/lib.rs": src})
            findings, _acc, _gh, _sh = scan(tmp)
            ok &= check(name, findings, expect)
    for name, tree, untracked, expect in FILESET_CASES:
        with tempfile.TemporaryDirectory(prefix="gskip_") as tmp:
            git_tree(tmp, tree, untracked)
            findings, _acc, _gh, _sh = scan(tmp)
            ok &= check(name, findings, expect)
    # B8: not a git work tree -> an error, never a disk walk.
    with tempfile.TemporaryDirectory(prefix="gskip_") as tmp:
        os.makedirs(os.path.join(tmp, "crates", "x", "src"))
        with open(os.path.join(tmp, "crates", "x", "src", "lib.rs"), "w") as fh:
            fh.write("#[test]\nfn t() { assert!(x.ok()); }\n")
        try:
            scan(tmp)
            erred = False
        except GateError:
            erred = True
        red = erred and run(tmp) == 1
        print(f"  [{'PASS' if red else 'FAIL'}] B8 a root that is not a git work tree: "
              f"expected an ERROR (exit 1), got {'ERROR' if red else 'a verdict'}")
        ok &= red
    # allow-list arms: a stale site, over-cap lists, and a guard whose verify fails
    with tempfile.TemporaryDirectory(prefix="gskip_") as tmp:
        global ALLOW_SITES, ALLOW_GUARDS
        saved = (ALLOW_SITES, ALLOW_GUARDS)
        try:
            git_tree(tmp, {
                "crates/x/src/lib.rs": "#[test]\nfn t() {\n    if CONST_X == Kind::B {\n"
                                       "        return;\n    }\n    assert!(run().ok());\n}\n",
                "defs.rs": "pub const CONST_X: Kind = Kind::A;\n",
            })
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
            red = any("ALLOW_SITES" in e and "cap" in e for e in verify_allowlist(tmp, gh, {}))
            print(f"  [{'PASS' if red else 'FAIL'}] ALLOW_SITES over its cap: expected RED")
            ok &= red
            ALLOW_SITES = {}
            ALLOW_GUARDS = [dict(ALLOW_GUARDS[0]) for _ in range(MAX_GUARDS + 1)]
            red = any("ALLOW_GUARDS" in e and "cap" in e for e in verify_allowlist(tmp, gh, {}))
            print(f"  [{'PASS' if red else 'FAIL'}] ALLOW_GUARDS over its cap: expected RED")
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
