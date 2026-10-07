#!/usr/bin/env python3
"""w8-t1-transcribe-nerd-font-tables.py -- transcribe ghostty's generated
`src/font/nerd_font_tables.zig` (a3f3331d4, unchanged at the pin 12752b2ac)
into rustty-font's `src/generated/nerd_font_tables.rs` (P7-W8 T1).

Input: the pinned Zig file, read from the read-only oracle without building it
(R3):

    git -C /Users/gael/projects/ghostty show 12752b2ac:src/font/nerd_font_tables.zig

Usage:
  w8-t1-transcribe-nerd-font-tables.py <nerd_font_tables.zig> <out.rs>

Then `rustfmt --edition 2021 <out.rs>`.

The transcription is mechanical and total: stage1/stage2 integers are copied
in order, and every stage3 element is either `null` -> `None` or a struct
literal whose `.field = value` lines map one to one onto `Constraint` fields
(`..Constraint::none` supplies the Zig field defaults, since the reference's
`none` is `.{}`). Float literals are copied as text, never re-printed. Any
line the script does not recognise aborts the run. The script also re-parses
its own Rust output and checks it against the Zig input value for value.
"""
import re
import sys

PIN = "12752b2ac"

SIZE = {"none": "None", "fit": "Fit", "cover": "Cover",
        "fit_cover1": "FitCover1", "stretch": "Stretch"}
ALIGN = {"none": "None", "start": "Start", "end": "End",
         "center": "Center", "center1": "Center1"}
HEIGHT = {"cell": "Cell", "icon": "Icon"}
FLOAT_FIELDS = {"pad_top", "pad_left", "pad_right", "pad_bottom",
                "relative_width", "relative_height", "relative_x", "relative_y"}
FLOAT_RE = re.compile(r"^-?[0-9]+(\.[0-9]+)?$")


def parse(zig):
    lines = zig.split("\n")
    header = []
    for ln in lines:
        if not ln.startswith("//!"):
            break
        header.append(ln)

    def int_array(name, size):
        m = re.search(r"pub const %s: \[(\d+)\]u16 = \.\{ ([0-9, ]+) \};" % name, zig)
        assert m, name
        vals = [int(v) for v in m.group(2).split(",")]
        assert int(m.group(1)) == size == len(vals), (name, m.group(1), len(vals))
        assert all(0 <= v <= 0xFFFF for v in vals)
        return vals

    stage1 = int_array("stage1", 8192)
    stage2 = int_array("stage2", 6400)

    m = re.search(r"pub const stage3: \[(\d+)\]Elem = \.\{\n(.*?)\n        \};\n", zig, re.S)
    assert m
    n3 = int(m.group(1))
    body = m.group(2).split("\n")
    stage3 = []
    cur = None
    for ln in body:
        s = ln.strip()
        if cur is None:
            if s == "null,":
                stage3.append(None)
            elif s == ".{":
                cur = []
            else:
                raise SystemExit("unrecognised stage3 line: %r" % ln)
        else:
            if s == "},":
                stage3.append(cur)
                cur = None
                continue
            fm = re.match(r"^\.([a-z_]+) = (.+),$", s)
            if not fm:
                raise SystemExit("unrecognised field line: %r" % ln)
            cur.append((fm.group(1), fm.group(2)))
    assert cur is None
    assert len(stage3) == n3 == 277, (len(stage3), n3)
    return header, stage1, stage2, stage3


def rust_field(name, val):
    if name == "size":
        assert val.startswith(".")
        return "size: ConstraintSize::%s" % SIZE[val[1:]]
    if name in ("align_horizontal", "align_vertical"):
        assert val.startswith(".")
        return "%s: Align::%s" % (name, ALIGN[val[1:]])
    if name == "height":
        assert val.startswith(".")
        return "height: Height::%s" % HEIGHT[val[1:]]
    if name == "max_constraint_width":
        assert re.match(r"^[0-9]+$", val)
        return "max_constraint_width: %s" % val
    if name == "max_xy_ratio":
        assert FLOAT_RE.match(val), val
        return "max_xy_ratio: Some(%s)" % val
    if name in FLOAT_FIELDS:
        assert FLOAT_RE.match(val), val
        return "%s: %s" % (name, val)
    raise SystemExit("unknown Constraint field: %s" % name)


def emit(header, stage1, stage2, stage3):
    out = []
    out.extend(header)
    out.append("//")
    out.append("// PORT NOTE: ghostty src/font/nerd_font_tables.zig (%s), transcribed" % PIN)
    out.append("// value for value by `.port/phase-7/w8-t1-transcribe-nerd-font-tables.py`")
    out.append("// (R1: generated artifacts are reproduced bit-for-bit). The reference wraps")
    out.append("// the three arrays in `pub fn Tables(comptime Elem: type) type`, instantiated")
    out.append("// once, as `Tables(?Constraint)`; Rust const data cannot be generic over the")
    out.append("// element type of struct literals, so the arrays are the `?Constraint`")
    out.append("// instantiation directly. Each stage3 struct literal becomes `Some(Constraint")
    out.append("// { .., ..Constraint::none })` (the reference's `Constraint.none` is `.{}`, so")
    out.append("// `none` carries exactly the Zig field defaults), `null` becomes `None`.")
    out.append("")
    out.append("// PORT NOTE: the float literals are copied verbatim from the generated")
    out.append("// reference; some carry more digits than the shortest f64 round-trip.")
    out.append("#![allow(clippy::excessive_precision)]")
    out.append("// PARITY: the reference emits these as container-level `pub const stageN`")
    out.append("// arrays, which Zig materialises as ONE global object each. A Rust `const`")
    out.append("// array is instead inlined into every use, so they are `static`, as")
    out.append("// rustty-unicode's `props_data` is (P7-W22, N520).")
    out.append("")
    out.append("use crate::glyph::{Align, Constraint, ConstraintSize, Height};")
    out.append("")
    out.append("// pub fn Tables(comptime Elem: type) type {")
    out.append("//     return struct {")
    out.append("//         pub const stage1: [8192]u16 = .{ ... };")
    out.append("pub static STAGE1: [u16; %d] = [%s];" % (len(stage1), ", ".join(map(str, stage1))))
    out.append("")
    out.append("//         pub const stage2: [6400]u16 = .{ ... };")
    out.append("pub static STAGE2: [u16; %d] = [%s];" % (len(stage2), ", ".join(map(str, stage2))))
    out.append("")
    out.append("//         pub const stage3: [277]Elem = .{ ... };")
    out.append("pub static STAGE3: [Option<Constraint>; %d] = [" % len(stage3))
    for e in stage3:
        if e is None:
            out.append("    None,")
        else:
            out.append("    Some(Constraint {")
            for name, val in e:
                out.append("        %s," % rust_field(name, val))
            out.append("        ..Constraint::none")
            out.append("    }),")
    out.append("];")
    out.append("//     };")
    out.append("// }")
    out.append("")
    return "\n".join(out)


def reparse_rust(rs):
    """Independent check: re-read the Rust output and rebuild the Zig values."""
    inv = {"ConstraintSize": {v: k for k, v in SIZE.items()},
           "Align": {v: k for k, v in ALIGN.items()},
           "Height": {v: k for k, v in HEIGHT.items()}}

    def arr(name):
        m = re.search(r"pub (?:const|static) %s: \[u16; (\d+)\] = \[([0-9,\s]+)\];" % name, rs)
        vals = [int(v) for v in m.group(2).replace("\n", " ").split(",") if v.strip()]
        assert len(vals) == int(m.group(1))
        return vals

    s3 = []
    m = re.search(r"pub (?:const|static) STAGE3: \[Option<Constraint>; (\d+)\] = \[\n(.*?)\n\];", rs, re.S)
    cur = None
    for ln in m.group(2).split("\n"):
        s = ln.strip()
        if s == "None,":
            s3.append(None)
        elif s == "Some(Constraint {":
            cur = []
        elif s == "..Constraint::none":
            pass
        elif s == "}),":
            s3.append(cur)
            cur = None
        else:
            fm = re.match(r"^([a-z_]+): (.+),$", s)
            name, val = fm.group(1), fm.group(2)
            em = re.match(r"^(ConstraintSize|Align|Height)::(\w+)$", val)
            if em:
                val = "." + inv[em.group(1)][em.group(2)]
            sm = re.match(r"^Some\((.+)\)$", val)
            if sm:
                val = sm.group(1)
            cur.append((name, val))
    return arr("STAGE1"), arr("STAGE2"), s3


def main():
    zig = open(sys.argv[1]).read()
    header, s1, s2, s3 = parse(zig)
    rs = emit(header, s1, s2, s3)
    r1, r2, r3 = reparse_rust(rs)
    assert (r1, r2, r3) == (s1, s2, s3), "round-trip mismatch"
    open(sys.argv[2], "w").write(rs)
    print("stage1=%d stage2=%d stage3=%d (non-null %d); round-trip OK"
          % (len(s1), len(s2), len(s3), sum(e is not None for e in s3)))


if __name__ == "__main__":
    main()
