#!/usr/bin/env python3
"""G-VTALLOC ESCAPE arm: nothing may switch off the vt crates' clippy
`disallowed-methods` ban (crates/rustty-vt{,-c}/clippy.toml, N261).

`disallowed_methods` is a clippy `style` lint that ci.sh turns into an error
with `-D warnings`. It can be silenced in several ways, and every one of them
is RED here:

  * any `.rs` file in either vt crate that mentions `disallowed_methods`,
    `clippy::style` or `clippy::all` at all. That covers `allow`/`expect`,
    combined lists, multi-line attributes, inner `#![..]` and `cfg_attr`;
  * `allow`/`expect` of the `warnings` group, on one line or several;
  * a `[lints]` table in either crate's Cargo.toml that is anything other than
    `workspace = true` (a `[lints.clippy]` override, for example);
  * the workspace `[workspace.lints.clippy]` table setting `disallowed_methods`,
    `style` or `all` to `allow`.

A lexical gate cannot be a proof. It is deliberately broad: the vt crates have
no legitimate reason to name these lints at all.

Usage:
  check_vt_alloc_vec_ban_escape.py [ROOT]     # exit 0 clean, 1 escape found
  check_vt_alloc_vec_ban_escape.py --self-test
"""
import os
import re
import sys
import tempfile

CRATES = ("crates/rustty-vt", "crates/rustty-vt-c")
RS_MENTION = re.compile(r"disallowed_methods|clippy\s*::\s*(style|all)\b")
RS_WARNINGS = re.compile(r"\b(allow|expect)\s*\(([^()]*)\)", re.S)
LINT_KEYS = ("disallowed_methods", "style", "all")


def toml_sections(text):
    """Yield (section header, body) pairs of a TOML file (a lexical split)."""
    cur, body = "", []
    for line in text.splitlines():
        m = re.match(r"\s*\[([^\]]+)\]\s*(#.*)?$", line)
        if m:
            yield cur, "\n".join(body)
            cur, body = m.group(1).strip(), []
        else:
            body.append(line)
    yield cur, "\n".join(body)


def check(root):
    errs = []
    for crate in CRATES:
        base = os.path.join(root, crate)
        if not os.path.isfile(os.path.join(base, "Cargo.toml")):
            # Anti-vacuity: scanning a tree without the crates would pass.
            errs.append(f"{crate}/Cargo.toml not found under {root}")
            continue
        for dp, dns, fns in os.walk(base):
            dns[:] = [d for d in dns if d != "target"]
            for fn in fns:
                p = os.path.join(dp, fn)
                rel = os.path.relpath(p, root)
                if fn.endswith(".rs"):
                    s = open(p, encoding="utf-8", errors="replace").read()
                    for m in RS_MENTION.finditer(s):
                        errs.append(f"{rel}:{s.count(chr(10), 0, m.start()) + 1}: mentions `{m.group(0)}`")
                    for m in RS_WARNINGS.finditer(s):
                        if re.search(r"\bwarnings\b", m.group(2)):
                            errs.append(f"{rel}:{s.count(chr(10), 0, m.start()) + 1}: {m.group(1)}(warnings)")
                elif fn == "Cargo.toml" and dp == base:
                    s = open(p, encoding="utf-8").read()
                    for sec, body in toml_sections(s):
                        if sec == "lints":
                            keys = [l.strip() for l in body.splitlines()
                                    if l.strip() and not l.strip().startswith("#")]
                            if keys != ["workspace = true"]:
                                errs.append(f"{rel}: [lints] must be exactly `workspace = true`, got {keys}")
                        elif sec.startswith("lints."):
                            errs.append(f"{rel}: [{sec}] overrides the workspace lints")
    ws = os.path.join(root, "Cargo.toml")
    if os.path.exists(ws):
        for sec, body in toml_sections(open(ws, encoding="utf-8").read()):
            if sec != "workspace.lints.clippy":
                continue
            for line in body.splitlines():
                m = re.match(r"\s*([\w-]+)\s*=\s*(.*)", line)
                if m and m.group(1).replace("-", "_") in LINT_KEYS and "allow" in m.group(2):
                    errs.append(f"Cargo.toml [workspace.lints.clippy]: `{line.strip()}`")
    return errs


CLEAN_WS = '[workspace]\nmembers = []\n\n[workspace.lints.clippy]\nall = { level = "warn", priority = -1 }\nmanual_div_ceil = "allow"\n'
CLEAN_CRATE = '[package]\nname = "x"\n\n[lints]\nworkspace = true\n\n[dependencies]\n'
CLEAN_RS = '#[allow(dead_code)]\nfn f() {}\n// a disallowed-methods ban (hyphenated prose is fine)\n'

PLANTS = [
    ("allow(clippy::style)", "crates/rustty-vt/src/lib.rs", "#[allow(clippy::style)]\nfn g() {}\n"),
    ("combined allow", "crates/rustty-vt/src/lib.rs", "#[allow(unused, clippy::disallowed_methods)]\nfn g() {}\n"),
    ("multi-line allow", "crates/rustty-vt-c/src/lib.rs", "#[allow(\n    clippy::disallowed_methods\n)]\nfn g() {}\n"),
    ("expect", "crates/rustty-vt/src/lib.rs", "#[expect(clippy::disallowed_methods)]\nfn g() {}\n"),
    ("inner allow(clippy::all)", "crates/rustty-vt-c/src/lib.rs", "#![allow(clippy::all)]\n"),
    ("cfg_attr allow", "crates/rustty-vt/src/lib.rs", "#[cfg_attr(test, allow(clippy::disallowed_methods))]\nfn g() {}\n"),
    ("multi-line allow(warnings)", "crates/rustty-vt/src/lib.rs", "#[allow(\n    unused,\n    warnings\n)]\nfn g() {}\n"),
    ("crate [lints.clippy]", "crates/rustty-vt/Cargo.toml", '\n[lints.clippy]\ndisallowed_methods = "allow"\n'),
    ("crate [lints] not workspace", "crates/rustty-vt-c/Cargo.toml", None),
    ("workspace style allow", "Cargo.toml", 'style = { level = "allow", priority = -1 }\n'),
    ("workspace disallowed_methods allow", "Cargo.toml", 'disallowed-methods = "allow"\n'),
]


def build_tree(root):
    with open(os.path.join(root, "Cargo.toml"), "w") as fh:
        fh.write(CLEAN_WS)
    for crate in CRATES:
        os.makedirs(os.path.join(root, crate, "src"), exist_ok=True)
        with open(os.path.join(root, crate, "Cargo.toml"), "w") as fh:
            fh.write(CLEAN_CRATE)
        with open(os.path.join(root, crate, "src", "lib.rs"), "w") as fh:
            fh.write(CLEAN_RS)


def self_test():
    ok = True
    with tempfile.TemporaryDirectory() as root:
        build_tree(root)
        errs = check(root)
        print(f"  {'ok  ' if not errs else 'FAIL'}    escape self-test: clean tree passes {errs or ''}")
        ok &= not errs
        with tempfile.TemporaryDirectory() as empty:
            errs = check(empty)
            print(f"  {'ok  ' if errs else 'FAIL'}    escape self-test: a tree without the vt crates -> {'RED' if errs else 'vacuous pass'}")
            ok &= bool(errs)
        for name, rel, text in PLANTS:
            build_tree(root)
            p = os.path.join(root, rel)
            s = open(p).read()
            if text is None:
                s = s.replace("workspace = true", "workspace = false")
            else:
                s = s + text
            with open(p, "w") as fh:
                fh.write(s)
            errs = check(root)
            print(f"  {'ok  ' if errs else 'FAIL'}    escape self-test: planted {name} -> {'RED' if errs else 'missed'}")
            ok &= bool(errs)
    return 0 if ok else 1


def main():
    if sys.argv[1:] == ["--self-test"]:
        sys.exit(self_test())
    root = sys.argv[1] if len(sys.argv) > 1 else os.path.dirname(os.path.dirname(os.path.dirname(
        os.path.dirname(os.path.abspath(__file__)))))
    errs = check(root)
    for e in errs:
        print("  ESCAPE  " + e)
    sys.exit(1 if errs else 0)


if __name__ == "__main__":
    main()
