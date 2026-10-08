#!/usr/bin/env python3
"""Arm (f) of the vt-headers ABI gate (A5): no two rustty-vt-c types that
cbindgen emits into include/rustty.h share a name.

WHY (NEW-FINDINGS N551). cbindgen keys types by their bare Rust name, not by
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
     crates/rustty-vt-c/src (files named tests.rs, directories named tests,
     and everything after a `#[cfg(test)]` line in a file are skipped).
  2. A name defined by two or more items is a COLLISION when include/rustty.h
     uses it as a type (a `} NAME;` or `typedef ... NAME;` definition).
  3. Every collision must be in BASELINE (pre-existing, each with a reason and
     the N that tracks it). A collision not in BASELINE is RED; a BASELINE row
     that no longer collides is RED (stale allowlists stop binding).

--self-test runs the same check over a fixture tree: a clean fixture is GREEN,
a fixture with an unlisted duplicate is RED, and a stale baseline row is RED.
"""
import os
import re
import sys
import tempfile

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))

# Pre-existing collisions at P8-W5 T3 (2026-10-08). Each is recorded in
# NEW-FINDINGS N551; the list can only shrink.
BASELINE = {
    "Data": "render/kitty_graphics/selection_gesture `Data` enums; c_int-sized, the curated headers name each (N551)",
    "Encoder": "key_encode/mouse_encode opaque handle aliases; pointer-sized, rustty.h names the key encoder for both (N551)",
    "Event": "key/mouse/selection-gesture opaque handle aliases; pointer-sized (N551)",
    "KittyGraphics": "terminal::KittyGraphics is an alias of kitty_graphics::KittyGraphics, the same type (N551)",
    "Parser": "osc/sgr opaque handle aliases; pointer-sized, rustty.h names the OSC wrapper for both (N551)",
    "RowData": "render/row `RowData` enums; c_int-sized (N551)",
    "Size": "size_report::Size vs mouse_encode::Size: DIFFERENT layouts; rustty.h declares rustty_size_report_encode with the mouse layout (N551, open)",
}

ITEM_RE = re.compile(r"^\s*pub (struct|enum|union|type) ([A-Za-z_][A-Za-z0-9_]*)")


def rust_items(src):
    names = {}
    for root, dirs, files in os.walk(src):
        dirs[:] = [d for d in dirs if d != "tests"]
        for f in files:
            if not f.endswith(".rs") or f == "tests.rs":
                continue
            path = os.path.join(root, f)
            with open(path, encoding="utf-8") as fh:
                for n, line in enumerate(fh, 1):
                    if re.match(r"\s*#\[cfg\(test\)\]", line):
                        break
                    m = ITEM_RE.match(line)
                    if m:
                        names.setdefault(m.group(2), []).append(
                            f"{os.path.relpath(path, src)}:{n} ({m.group(1)})"
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
        if name not in baseline:
            errors.append(
                f"NEW COLLISION `{name}`: {', '.join(where)} — cbindgen emits one layout for all of them"
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

        write("a.rs", "#[repr(C)]\npub struct Alpha { x: u8 }\n#[cfg(test)]\nmod t { pub struct Beta; }\n")
        write("b.rs", "pub struct Beta { y: u8 }\n")
        write("tests/x.rs", "pub struct Alpha;\n")
        with open(header, "w", encoding="utf-8") as fh:
            fh.write("typedef struct {\n  uint8_t x;\n} Alpha;\ntypedef struct { uint8_t y; } Beta;\n")
        _, errs = check(src, header, {})
        if errs:
            print("    self-test FAIL: clean fixture is RED:", errs)
            rc = 1
        else:
            print("    self-test PASS: clean fixture is green")

        write("c.rs", "#[repr(C)]\npub struct Alpha { z: u64 }\n")
        _, errs = check(src, header, {})
        if any("NEW COLLISION `Alpha`" in e for e in errs):
            print("    self-test PASS: an unlisted duplicate is RED")
        else:
            print("    self-test FAIL: an unlisted duplicate stayed green")
            rc = 1

        _, errs = check(src, header, {"Alpha": "x", "Beta": "stale"})
        if any("STALE BASELINE row `Beta`" in e for e in errs) and not any("Alpha" in e for e in errs):
            print("    self-test PASS: a stale baseline row is RED, a listed one is accepted")
        else:
            print("    self-test FAIL: baseline handling:", errs)
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
        f"({len(collisions)} pre-existing, baselined in N551)"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
