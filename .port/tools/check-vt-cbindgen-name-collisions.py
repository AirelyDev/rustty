#!/usr/bin/env python3
"""Arm (f) of the vt-headers ABI gate (A5): no two rustty-vt-c types that
cbindgen emits into include/rustty.h share a name.

WHY (NEW-FINDINGS N529). cbindgen keys types by their bare Rust name, not by
module path. When two `pub struct|enum|union|type` items in different
rustty-vt-c modules share a name, it emits ONE of them and every declaration
that names the other silently uses the wrong layout. P8-W5 T3 added an OSC
`osc::UnknownC` beside the SGR `sgr::UnknownC`: `include/rustty.h` then
declared `rustty_sgr_unknown_full`/`_partial` and the SGR attribute value with
the OSC layout. Arms (a3) (byte-identical regeneration) and (c) (curated
header layouts) cannot see this: the regeneration reproduces the wrong header
faithfully, and the curated headers are hand-written.

METHOD
  1. Every non-test `pub (struct|enum|union|type) NAME` item under
     crates/rustty-vt-c/src (files named tests.rs and directories named tests
     are skipped, and so is each `#[cfg(test)]` item, found by brace matching
     with comments and string/char literals ignored; items after it are read).
  2. A name defined by two or more items is a COLLISION when include/rustty.h
     uses it as a type (a `} NAME;` or `typedef ... NAME;` definition).
  3. Every collision must be in BASELINE (pre-existing, each with a reason, the
     N that tracks it, and its exact set of defining items). A collision not in
     BASELINE is RED; one whose defining items differ from its row (a third
     definition) is RED; a BASELINE row that no longer collides is RED.

--self-test runs the same check over a fixture tree: a clean fixture is GREEN,
a fixture with an unlisted duplicate is RED, and a stale baseline row is RED.
"""
import os
import re
import sys
import tempfile

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))

# The pre-existing collisions of P8-W5 T3 (N529) were all fixed by P8-W5 T5;
# the list can only shrink, so it stays empty.
BASELINE = {
    # name: (reason, the exact set of "file (kind)" items that define it). A
    # name whose item set differs from its row (a third definition, or one
    # gone) is RED, so the baseline cannot absorb a new collision. EMPTY since
    # P8-W5 T5 (N529 closed): every vt-c type carries its upstream C name.
}

ITEM_RE = re.compile(r"^\s*pub (struct|enum|union|type) ([A-Za-z_][A-Za-z0-9_]*)")


def _strip_code(line, state):
    """Return `line` with comments and string/char literal contents removed,
    so braces inside them are not counted. `state` carries an open block
    comment or string across lines."""
    out = []
    i = 0
    while i < len(line):
        if state["block"]:
            j = line.find("*/", i)
            if j < 0:
                return "".join(out)
            state["block"] = False
            i = j + 2
            continue
        if state["string"]:
            c = line[i]
            if c == "\\":
                i += 2
                continue
            if c == '"':
                state["string"] = False
            i += 1
            continue
        if line.startswith("//", i):
            break
        if line.startswith("/*", i):
            state["block"] = True
            i += 2
            continue
        c = line[i]
        if c == '"':
            state["string"] = True
            i += 1
            continue
        if c == "'":
            m = re.match(r"'(\\.|[^\\'])'", line[i:])
            if m:
                i += len(m.group(0))
                continue
        out.append(c)
        i += 1
    return "".join(out)


def _non_test_lines(lines):
    """Yield (lineno, line) for every line outside `#[cfg(test)]` items. The
    attributed item is skipped by brace matching (or up to its `;` for
    `mod tests;`), so items after a test module are still read."""
    i = 0
    n = len(lines)
    while i < n:
        if re.match(r"\s*#\[cfg\(test\)\]", lines[i]):
            i += 1
            # Further attributes and doc comments on the same item.
            while i < n and re.match(r"\s*(#\[|///|//)", lines[i]):
                i += 1
            depth = 0
            opened = False
            state = {"block": False, "string": False}
            while i < n:
                code = _strip_code(lines[i], state)
                depth += code.count("{") - code.count("}")
                if "{" in code:
                    opened = True
                i += 1
                if opened and depth <= 0:
                    break
                if not opened and code.rstrip().endswith(";"):
                    break
            if opened and depth > 0:
                raise SystemExit("cbindgen-collision checker: unbalanced braces after #[cfg(test)]")
            continue
        yield i + 1, lines[i]
        i += 1


def rust_items(src):
    names = {}
    for root, dirs, files in os.walk(src):
        dirs[:] = [d for d in dirs if d != "tests"]
        for f in files:
            if not f.endswith(".rs") or f == "tests.rs":
                continue
            path = os.path.join(root, f)
            with open(path, encoding="utf-8") as fh:
                lines = fh.read().split("\n")
            for n, line in _non_test_lines(lines):
                m = ITEM_RE.match(line)
                if m:
                    names.setdefault(m.group(2), []).append(
                        (os.path.relpath(path, src), n, m.group(1))
                    )
    return names


def header_types(header):
    with open(header, encoding="utf-8") as fh:
        text = fh.read()
    out = set(re.findall(r"\}\s*([A-Za-z_][A-Za-z0-9_]*)\s*;", text))
    out |= set(re.findall(r"typedef[^;{]*?[\s*]([A-Za-z_][A-Za-z0-9_]*)\s*;", text))
    return out


def check(src, header, baseline):
    items = rust_items(src)
    emitted = header_types(header)
    collisions = {k: v for k, v in items.items() if len(v) > 1 and k in emitted}
    errors = []
    for name, where in sorted(collisions.items()):
        found = {f"{p} ({kind})" for p, _, kind in where}
        listed = ", ".join(f"{p}:{n} ({kind})" for p, n, kind in where)
        if name not in baseline:
            errors.append(
                f"NEW COLLISION `{name}`: {listed} — cbindgen emits one layout for all of them"
            )
        elif found != baseline[name][1]:
            errors.append(
                f"CHANGED COLLISION `{name}`: defined by {sorted(found)}, baseline row lists "
                f"{sorted(baseline[name][1])}"
            )
    for name in sorted(baseline):
        if name not in collisions:
            errors.append(f"STALE BASELINE row `{name}`: it no longer collides; remove the row")
    return collisions, errors


def self_test():
    rc = 0
    with tempfile.TemporaryDirectory() as td:
        src = os.path.join(td, "src")
        os.makedirs(os.path.join(src, "tests"))
        header = os.path.join(td, "rustty.h")

        def write(path, text):
            with open(os.path.join(src, path), "w", encoding="utf-8") as fh:
                fh.write(text)

        write(
            "a.rs",
            "#[repr(C)]\npub struct Alpha { x: u8 }\n#[cfg(test)]\nmod t {\n    pub struct Beta; // }\n"
            "    const S: &str = \"}\";\n}\npub struct Gamma { z: u8 }\n",
        )
        write("b.rs", "pub struct Beta { y: u8 }\n")
        write("tests/x.rs", "pub struct Alpha;\n")
        with open(header, "w", encoding="utf-8") as fh:
            fh.write(
                "typedef struct {\n  uint8_t x;\n} Alpha;\ntypedef struct { uint8_t y; } Beta;\n"
                "typedef struct { uint8_t z; } Gamma;\n"
            )
        _, errs = check(src, header, {})
        if errs:
            print("    self-test FAIL: clean fixture is RED:", errs)
            rc = 1
        else:
            print("    self-test PASS: clean fixture is green")

        # An item after a `#[cfg(test)]` module is still read.
        write("g.rs", "pub struct Gamma { w: u16 }\n")
        _, errs = check(src, header, {})
        if any("NEW COLLISION `Gamma`" in e for e in errs):
            print("    self-test PASS: an item after a #[cfg(test)] module is read")
        else:
            print("    self-test FAIL: an item after a #[cfg(test)] module was skipped:", errs)
            rc = 1
        os.remove(os.path.join(src, "g.rs"))

        write("c.rs", "#[repr(C)]\npub struct Alpha { z: u64 }\n")
        _, errs = check(src, header, {})
        if any("NEW COLLISION `Alpha`" in e for e in errs):
            print("    self-test PASS: an unlisted duplicate is RED")
        else:
            print("    self-test FAIL: an unlisted duplicate stayed green")
            rc = 1

        base = {"Alpha": ("x", {"a.rs (struct)", "c.rs (struct)"}), "Beta": ("stale", set())}
        _, errs = check(src, header, base)
        if any("STALE BASELINE row `Beta`" in e for e in errs) and not any("Alpha" in e for e in errs):
            print("    self-test PASS: a stale baseline row is RED, a listed one is accepted")
        else:
            print("    self-test FAIL: baseline handling:", errs)
            rc = 1

        write("d.rs", "pub struct Alpha { q: u32 }\n")
        _, errs = check(src, header, {"Alpha": base["Alpha"]})
        if any("CHANGED COLLISION `Alpha`" in e for e in errs):
            print("    self-test PASS: a third definition of a baselined name is RED")
        else:
            print("    self-test FAIL: a third definition stayed green:", errs)
            rc = 1
    print("    SELF-TEST:", "PASS" if rc == 0 else "FAIL")
    return rc


def main():
    if "--self-test" in sys.argv:
        return self_test()
    src = os.path.join(ROOT, "crates", "rustty-vt-c", "src")
    header = os.path.join(ROOT, "include", "rustty.h")
    collisions, errors = check(src, header, BASELINE)
    if errors:
        print("    CBINDGEN NAME COLLISIONS:")
        for e in errors:
            print("      " + e)
        return 1
    print(
        f"    no new cbindgen type-name collisions in include/rustty.h "
        f"({len(collisions)} pre-existing, baselined in N529)"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
