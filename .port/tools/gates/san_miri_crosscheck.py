#!/usr/bin/env python3
"""G-ASAN / G-LSAN's Miri-exception cross-check (sanitizers.sh, P7-W14).

Design §B (docs/superpowers/specs/2026-10-01-memory-safety-lanes-design.md):
"Add a check that every Miri-excepted test actually ran under G-ASAN." A
Miri-excepted test is any test function carrying `#[cfg_attr(<pred>, ignore...)]`
where <pred> is `miri`, `all(miri, ...)` or `any(miri, ...)` (with or without
a reason string): Miri never runs it, so the sanitizer lane is where it has to
run.

For every such test, in the logs of one sanitizer pass:

  * `ok`                    -> verified.
  * `ignored` / `FAILED`    -> RED, unless listed in the expected-absent file.
  * absent from every log   -> RED, unless listed in the expected-absent file.
  * its crate has no log    -> RED (excluded, or the pass did not run it).

The expected-absent file (san_expected_absent.tsv) names, per host OS, the
Miri-excepted tests that cannot run there: platform-gated (cfg'd out of the
build) or natively `#[ignore]`d. Each row has a reason. A row whose tests DID
run and pass is stale and RED, as is a row that matches no annotation.

Matching is by module path, not bare function name: a test in
`crates/<c>/src/a/b.rs` must appear as `a::b::<inline mods>::<fn>` in a unit
test target, and one in `crates/<c>/tests/x.rs` (or `tests/x/...`) as
`[<mods>::]<fn>` in the `tests/x.rs` target. The annotation is attached to the
`fn` that directly follows it (only attributes, comments and `pub`/`async`/
`unsafe`/`const` may sit between); anything else is reported as an
unattached annotation (RED).

Usage: san_miri_crosscheck.py --root R --logdir D --os Darwin|Linux --expected F [--src S]
Exit 0 GREEN, 1 RED.
"""
import argparse
import fnmatch
import glob
import os
import re
import sys

ap = argparse.ArgumentParser()
ap.add_argument("--root", required=True)
ap.add_argument("--src", help="source root to scan (default: --root)")
ap.add_argument("--logdir", required=True)
ap.add_argument("--os", required=True)
ap.add_argument("--expected", required=True)
ap.add_argument("--label", default="")
a = ap.parse_args()
src_root = os.path.abspath(a.src or a.root)
label = a.label or os.path.basename(os.path.normpath(a.logdir))

ATTR = re.compile(r'#\[cfg_attr\(\s*(miri|all\(\s*miri\b|any\(\s*miri\b)')


def skip_balanced(s, i, open_c, close_c):
    """s[i] == open_c; return the index after its matching close_c (string-aware)."""
    depth = 0
    n = len(s)
    while i < n:
        c = s[i]
        if c == '"':
            i += 1
            while i < n and s[i] != '"':
                i += 2 if s[i] == "\\" else 1
        elif c == open_c:
            depth += 1
        elif c == close_c:
            depth -= 1
            if depth == 0:
                return i + 1
        i += 1
    return n


def inline_mods(s, pos):
    """The inline `mod name { ... }` blocks enclosing offset `pos` of file text
    `s`, outermost first. Comments, strings and char literals are skipped so a
    brace in them does not count."""
    stack = []      # [(name or None, depth at which its body opened)]
    depth = 0
    i = 0
    n = min(pos, len(s))
    modre = re.compile(r'\bmod\s+([A-Za-z_][A-Za-z0-9_]*)\s*\{')
    pending = None  # (name, index of its "{")
    while i < n:
        if pending is None:
            m = modre.match(s, i) if (i == 0 or not (s[i - 1].isalnum() or s[i - 1] == "_")) else None
            if m and m.end() - 1 < n:
                pending = (m.group(1), m.end() - 1)
        c = s[i]
        if s.startswith("//", i):
            j = s.find("\n", i)
            i = n if j < 0 else j
            continue
        if s.startswith("/*", i):
            j = s.find("*/", i)
            i = n if j < 0 else j + 2
            continue
        rm = re.compile(r'r(#*)"').match(s, i) if c == "r" and (i == 0 or not (s[i - 1].isalnum() or s[i - 1] == "_")) else None
        if rm:
            close = '"' + rm.group(1)
            j = s.find(close, rm.end())
            i = n if j < 0 else j + len(close)
            continue
        if c == '"':
            i += 1
            while i < n and s[i] != '"':
                i += 2 if s[i] == "\\" else 1
            i += 1
            continue
        if c == "'":
            cm = re.compile(r"'(?:\\.|[^\\'])'").match(s, i)
            if cm:
                i = cm.end()
                continue
        if c == "{":
            depth += 1
            if pending is not None and pending[1] == i:
                stack.append((pending[0], depth))
                pending = None
            else:
                stack.append((None, depth))
        elif c == "}":
            if stack:
                stack.pop()
            depth -= 1
        i += 1
    return [name for name, _ in stack if name is not None]


def attached_fn(s, i):
    """From the end of an attribute, skip attributes/comments/qualifiers; return the fn name or None."""
    n = len(s)
    while i < n:
        while i < n and s[i].isspace():
            i += 1
        if s.startswith("//", i):
            i = s.find("\n", i)
            i = n if i < 0 else i
            continue
        if s.startswith("/*", i):
            j = s.find("*/", i)
            i = n if j < 0 else j + 2
            continue
        if s.startswith("#[", i):
            i = skip_balanced(s, i + 1, "[", "]")
            continue
        m = re.compile(r'(?:pub(?:\([^)]*\))?\s+)?(?:(?:const|async|unsafe)\s+)*fn\s+([A-Za-z_][A-Za-z0-9_]*)').match(s, i)
        return m.group(1) if m else None
    return None


def package_dirs(root):
    out = {}
    for man in glob.glob(os.path.join(root, "**", "Cargo.toml"), recursive=True):
        r = "/" + os.path.relpath(man, root)
        if "/vendor/" in r or "/target/" in r or "/.claude/" in r:
            continue
        txt = open(man, encoding="utf-8", errors="replace").read()
        m = re.search(r'^\[package\][^\[]*?^name\s*=\s*"([^"]+)"', txt, re.M | re.S)
        if m:
            out[os.path.dirname(os.path.abspath(man))] = m.group(1)
    return out


def locate(path, dirs):
    d = os.path.dirname(path)
    while d.startswith(src_root) and d != src_root:
        if d in dirs:
            return dirs[d], os.path.relpath(path, d)
        d = os.path.dirname(d)
    return None, None


def target_and_module(rel):
    """crate-relative file -> (target key, module path)."""
    parts = rel[:-3].split("/")  # drop .rs
    if parts[0] == "src":
        parts = parts[1:]
        if parts and parts[0] == "bin":
            return "bin", "::".join(p for p in parts[2:] if p != "mod")
        if parts and parts[-1] in ("lib", "main"):
            parts = parts[:-1]
        if parts and parts[-1] == "mod":
            parts = parts[:-1]
        return "unit", "::".join(parts)
    if parts[0] == "tests":
        if len(parts) == 2:
            return "tests/%s.rs" % parts[1], ""
        mods = [p for p in parts[2:] if p not in ("mod", "main")]
        return "tests/%s" % parts[1], "::".join(mods)
    return "other:" + parts[0], ""


dirs = package_dirs(src_root)


def in_comment(s, i):
    ls = s.rfind("\n", 0, i) + 1
    return "//" in s[ls:i]


# `#[path = "x.rs"] mod name;` remaps a file to a module (rustty-xev's
# `async.rs` is `async_watcher`). Relative to the declaring file's directory.
PATHMOD = re.compile(r'#\[path\s*=\s*"([^"]+)"\]\s*(?:#\[[^\]]*\]\s*)*(?:pub(?:\([^)]*\))?\s+)?mod\s+([A-Za-z_][A-Za-z0-9_]*)\s*;')
remap = {}  # abs file -> (declaring abs file, mod name)
for f in glob.glob(os.path.join(src_root, "**", "*.rs"), recursive=True):
    r = "/" + os.path.relpath(f, src_root)
    if "/vendor/" in r or "/target/" in r or "/.claude/" in r:
        continue
    s0 = open(f, encoding="utf-8", errors="replace").read()
    for m in PATHMOD.finditer(s0):
        if in_comment(s0, m.start()):
            continue
        remap[os.path.normpath(os.path.join(os.path.dirname(os.path.abspath(f)), m.group(1)))] = (os.path.abspath(f), m.group(2))


def module_of(absf, rel):
    tgt, mod = target_and_module(rel)
    if absf in remap:
        decl, name = remap[absf]
        _, drel = locate(decl, dirs)
        if drel is not None:
            dt, dmod = module_of(decl, drel)
            return dt, (dmod + "::" + name) if dmod else name
    return tgt, mod


claims = []  # (crate, rel file, target, module, fn)
unattached = []
for f in sorted(glob.glob(os.path.join(src_root, "**", "*.rs"), recursive=True)):
    r = "/" + os.path.relpath(f, src_root)
    if "/vendor/" in r or "/target/" in r or "/.claude/" in r:
        continue
    crate, rel = locate(os.path.abspath(f), dirs)
    if crate is None:
        continue
    s = open(f, encoding="utf-8", errors="replace").read()
    for m in ATTR.finditer(s):
        if in_comment(s, m.start()):
            continue
        end = skip_balanced(s, m.start() + 1, "[", "]")
        body = s[m.start():end]
        if not re.search(r",\s*ignore\b", body):
            continue
        fn = attached_fn(s, end)
        if fn is None:
            unattached.append("%s %s:%d" % (crate, rel, s.count("\n", 0, m.start()) + 1))
            continue
        tgt, mod = module_of(os.path.abspath(f), rel)
        claims.append((crate, rel, tgt, mod, fn, inline_mods(s, m.start())))

# Outcomes from the logs: crate -> list of (target, test path, status).
outcomes = {}
for lg in sorted(glob.glob(os.path.join(a.logdir, "*.log"))):
    crate = os.path.basename(lg)[:-4].split("_", 1)[0]
    rows = outcomes.setdefault(crate, [])
    target = "?"
    for line in open(lg, encoding="utf-8", errors="replace"):
        m = re.match(r"^\s+Running (unittests \S+|tests/\S+|benches/\S+) \(", line)
        if m:
            t = m.group(1)
            target = "unit" if t.startswith("unittests src/lib.rs") or t.startswith("unittests src/main.rs") else (
                "bin" if t.startswith("unittests src/bin/") else t)
            continue
        if re.match(r"^\s+Doc-tests ", line):
            target = "doc"
            continue
        m = re.match(r"^test (\S+)(?: - should panic)? \.\.\. (ok|ignored|FAILED)", line)
        if m:
            rows.append((target, m.group(1), m.group(2)))


def status_of(crate, tgt, mod, fn, inl):
    # The exact path: the file's module, then only the inline `mod`s that
    # enclose the annotation in its own file, then the fn. A same-named test in
    # another module (a child file, a sibling inline mod) cannot satisfy it.
    want = "::".join([p for p in [mod] + inl + [fn] if p])
    pat = re.compile(r"^%s$" % re.escape(want))
    best = None
    for t, path, st in outcomes.get(crate, []):
        if tgt.startswith("tests/") and not (t == tgt or t == tgt + ".rs" or t.startswith(tgt + "/")):
            continue
        if tgt in ("unit", "bin") and t not in ("unit", "bin"):
            continue
        if pat.match(path):
            if st == "ok":
                return "ok"
            best = st
    return best


# Expected-absent rows: OS<TAB>crate<TAB>glob over "module::fn"<TAB>reason
expected = []
for i, line in enumerate(open(a.expected, encoding="utf-8")):
    if not line.strip() or line.startswith("#"):
        continue
    cols = line.rstrip("\n").split("\t")
    if len(cols) != 4 or not cols[3].strip():
        print("  ❌ %s:%d: malformed row (want OS, crate, pattern, reason)" % (a.expected, i + 1))
        sys.exit(1)
    if cols[0] in ("*", a.os):
        expected.append([cols[1], cols[2], cols[3], 0, 0, i + 1])

red = []
ok = 0
absent_ok = 0
for crate, rel, tgt, mod, fn, inl in claims:
    name = "%s::%s" % (mod, fn) if mod else fn
    exp = None
    for row in expected:
        if row[0] == crate and fnmatch.fnmatchcase(name, row[1]):
            exp = row
            row[3] += 1
            break
    if crate not in outcomes:
        red.append("%s %s (%s): its crate has no %s log (excluded, or the pass did not run it)" % (crate, name, rel, label))
        continue
    st = status_of(crate, tgt, mod, fn, inl)
    if exp is not None:
        if st == "ok":
            exp[4] += 1
        elif st == "FAILED":
            red.append("%s %s (%s): on the %s expected-absent list, but it ran and FAILED under %s" % (crate, name, rel, a.os, label))
        else:
            absent_ok += 1
        continue
    if st == "ok":
        ok += 1
    elif st is None:
        red.append("%s %s (%s): Miri-excepted but absent from the %s logs, and not on the %s expected-absent list" % (crate, name, rel, label, a.os))
    else:
        red.append("%s %s (%s): Miri-excepted but %s under %s" % (crate, name, rel, st, label))
for row in expected:
    if row[3] == 0:
        red.append("%s:%d: expected-absent row (%s %s) matches no Miri-excepted test (stale)" % (a.expected, row[5], row[0], row[1]))
    elif row[4] > 0:
        red.append("%s:%d: expected-absent row (%s %s) but %d of its tests ran and passed here (stale)" % (a.expected, row[5], row[0], row[1], row[4]))
for u in unattached:
    red.append("cfg_attr(miri, ignore) not attached to a fn: %s" % u)

print("  Miri-exception cross-check (%s, %s): %d Miri-excepted test(s); %d ran and passed; %d expected absent on %s (listed, with reasons); %d problem(s)"
      % (label, a.os, len(claims), ok, absent_ok, a.os, len(red)))
for r in red[:200]:
    print("  ❌ " + r)
if len(red) > 200:
    print("  ❌ ... and %d more" % (len(red) - 200))
if not claims:
    print("  ❌ no Miri-excepted test parsed at all (the annotation grammar changed?)")
    sys.exit(1)
sys.exit(1 if red else 0)
