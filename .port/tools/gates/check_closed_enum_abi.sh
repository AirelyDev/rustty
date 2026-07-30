#!/usr/bin/env bash
# D11 — no CLOSED `#[repr(C)]` enum may be received BY VALUE across the C ABI.
#
# (The plan text for P3-W0 T3 calls this gate "D10". That id was already taken
# by the vendored stb_image_resize parity gate registered in ci.sh, so this one
# is D11. Same contract, different letter.)
#
# THE RULE IT ENFORCES
#   Inbound C integers narrow through an explicit `match`; a value with no
#   narrowing point gets `#[repr(transparent)]`. Declaring the Rust enum ITSELF
#   as the C parameter type breaks that: the invalid value is materialized at
#   the ABI boundary, BEFORE any `match` can run. Constructing a `#[repr(C)]`
#   enum from an out-of-range tag is instant UB — there is no defensive code
#   that can run "first", because the callee's prologue already did it.
#
#   This is not theoretical for these types. Every one of the enums flagged
#   today is declared in the published header with a
#   `…_MAX_VALUE = RUSTTY_ENUM_MAX_VALUE` enumerator, i.e. the header itself
#   DOCUMENTS an out-of-range value as writable by a conforming C caller
#   (include/rustty/vt/terminal.h:206,223,255,297,317,368,445,890,1200).
#
#   `rustty_mode_report_encode` already does it correctly, via
#   `pub struct ReportState(pub c_int)` (crates/rustty-vt-c/src/modes.rs:26,
#   with the PORT NOTE at :19 explaining the oracle's open `enum(c_int) { _ }`).
#   That is the proof the remaining sites are oversights, not policy.
#
# WHAT IT FLAGS
#   A parameter of an exported `extern "C" fn` in crates/rustty-*-c/src whose
#   BY-VALUE type resolves to a fieldless `#[repr(C)]` enum — either directly,
#   or as a field of a by-value `#[repr(C)]` struct parameter. Pointer and
#   reference parameters are NOT flagged: the value is not materialized by the
#   callee's prologue, and reading through the pointer is a narrowing point the
#   callee controls.
#
#   `#[repr(transparent)]` newtypes are exempt by construction — that IS the
#   sanctioned open-enum shape.
#
# NON-VACUITY
#   * this gate is RED on the tree it lands on, naming 33 sites across 27
#     exports — a gate that lands green is unproven;
#
#   PLAN CORRECTION: P3-W0 T3 predicted exactly FOUR offending exports
#   (rustty_terminal_scroll_viewport, rustty_focus_encode, rustty_sys_log_stderr,
#   rustty_size_report_encode). All four are here, but the real population is
#   33 sites / 27 exports. The plan's list was derived by reading rustty-vt-c
#   only, at one level of struct nesting; this scanner also covers
#   rustty-apprt-c, follows `pub type` aliases (which is the only way
#   rustty_terminal_scroll_viewport's `behavior.tag` is reachable at all), and
#   recurses through nested #[repr(C)] fields (which is how
#   rustty_surface_read_text's four Point tags surface). Do not misattribute
#   the pre-existing ones to the Phase 3 range — only ScrollViewportTag's
#   `Row = 3` (3a2e28329) is in it.
#   * a stale allowlist row (an entry that no longer matches any site) is itself
#     a FAIL, so the list can only shrink;
#   * if the scanner finds ZERO `extern "C" fn` signatures it FAILS LOUDLY
#     rather than reporting a clean run.
set -uo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
exec "${PYTHON:-python3}" - "$ROOT" <<'PYEOF'
import os, re, sys

ROOT = sys.argv[1]

# ---------------------------------------------------------------------------
# Allowlist. Every row needs a written justification. Format:
#   (export, parameter) -> reason
# A row that matches no live site is a FAIL (stale allowlist rot).
# ---------------------------------------------------------------------------
ALLOW = {}

CRATE_DIRS = []
crates = os.path.join(ROOT, "crates")
for name in sorted(os.listdir(crates)):
    if name.startswith("rustty-") and name.endswith("-c"):
        d = os.path.join(crates, name, "src")
        if os.path.isdir(d):
            CRATE_DIRS.append(d)
# The types those crates name often live in the pure-VT crate.
TYPE_DIRS = CRATE_DIRS + [os.path.join(crates, "rustty-vt", "src")]


def rs_files(dirs):
    for d in dirs:
        for root, _, files in os.walk(d):
            for f in sorted(files):
                if f.endswith(".rs"):
                    yield os.path.join(root, f)


# --- 1. type inventory ------------------------------------------------------
closed_enums = {}        # name -> ["crate/path.rs:line", ...] (bare names collide)
transparent = set()      # repr(transparent) types: the sanctioned open shape
repr_c_structs = {}      # name -> [(field, type_name, "file:line")]
aliases = {}             # `pub type A = B;` — the C API names several
                         # parameter types through an alias (e.g.
                         # `terminal::ScrollViewport` -> `ScrollViewportC`),
                         # and not following them hides the deepest hits.

attr_re = re.compile(r"^\s*#\[")
for path in rs_files(TYPE_DIRS):
    rel = os.path.relpath(path, ROOT)
    lines = open(path, encoding="utf-8").read().split("\n")
    for i, line in enumerate(lines):
        am = re.match(r"^\s*pub type\s+([A-Za-z0-9_]+)\s*=\s*([A-Za-z0-9_:<>, ]+?)\s*;", line)
        if am:
            aliases.setdefault(am.group(1), am.group(2).split("::")[-1].strip())
            continue
        m = re.match(r"^\s*pub (enum|struct) ([A-Za-z0-9_]+)", line)
        if not m:
            continue
        kind, name = m.group(1), m.group(2)
        j, reprs = i - 1, []
        while j >= 0 and (attr_re.match(lines[j]) or lines[j].lstrip().startswith("//")
                          or lines[j].strip() == ""):
            if lines[j].strip() == "":
                break
            r = re.search(r"#\[repr\(([^)]*)\)\]", lines[j])
            if r:
                reprs.append(r.group(1))
            j -= 1
        flat = ",".join(reprs)
        if "transparent" in flat:
            transparent.add(name)
            continue
        if "C" not in [t.strip() for t in flat.split(",")]:
            continue
        if kind == "enum":
            # fieldless only: a data-carrying repr(C) enum is a tagged union and
            # is not what this rule is about.
            body, depth, fieldless = [], 1, True
            for k in range(i + 1, len(lines)):
                depth += lines[k].count("{") - lines[k].count("}")
                if depth <= 0:
                    break
                t = lines[k].strip()
                if t.startswith("//") or t.startswith("#[") or not t:
                    continue
                if re.match(r"^(?:r#)?[A-Za-z_][A-Za-z0-9_]*\s*[({]", t):
                    fieldless = False
            if fieldless:
                closed_enums.setdefault(name, []).append(f"{rel}:{i+1}")
        else:
            fields, depth = [], 1
            for k in range(i + 1, len(lines)):
                depth += lines[k].count("{") - lines[k].count("}")
                if depth <= 0:
                    break
                t = lines[k].strip()
                if t.startswith("//") or t.startswith("#[") or not t:
                    continue
                fm = re.match(r"^(?:pub\s+)?([A-Za-z0-9_]+)\s*:\s*(.+?),?$", t)
                if fm:
                    fields.append((fm.group(1), fm.group(2), f"{rel}:{k+1}"))
            repr_c_structs.setdefault(name, []).extend(fields)


def base_type(t):
    """Return (by_value, last path segment) for a Rust type spelling."""
    t = t.strip().rstrip(",").strip()
    if t.startswith("*") or t.startswith("&") or t.startswith("Option<"):
        return (False, None)
    t = re.sub(r"<.*>", "", t)
    return (True, t.split("::")[-1].strip())


def walk_byvalue(type_name, expr, seen=None, depth=0):
    """Yield (expr, enum, how) for every closed enum reachable BY VALUE from a
    by-value parameter of type `type_name`. Recurses through nested #[repr(C)]
    struct fields — `rustty_terminal_scroll_viewport`'s offending tag is two
    levels down (`behavior.tag` inside a by-value ScrollViewportC), which a
    one-level scan misses entirely."""
    if seen is None:
        seen = set()
    if depth > 6 or type_name in seen:
        return
    for _ in range(4):
        if type_name in aliases and type_name not in closed_enums \
                and type_name not in repr_c_structs:
            type_name = aliases[type_name]
        else:
            break
    seen = seen | {type_name}
    if type_name in transparent:
        return
    if type_name in closed_enums:
        yield (expr, type_name, "parameter" if depth == 0 else "nested by-value field")
        return
    for (fname, ftype, _floc) in repr_c_structs.get(type_name, []):
        fbyval, fbase = base_type(ftype)
        if fbyval and fbase:
            yield from walk_byvalue(fbase, f"{expr}.{fname}", seen, depth + 1)


# --- 2. exported signatures -------------------------------------------------
hits = []
sig_count = 0
sig_re = re.compile(r'extern\s+"C"\s+fn\s+([A-Za-z0-9_]+)\s*\(', re.S)
for path in rs_files(CRATE_DIRS):
    rel = os.path.relpath(path, ROOT)
    src = open(path, encoding="utf-8").read()
    for m in sig_re.finditer(src):
        name = m.group(1)
        if not name.startswith("rustty_"):
            continue
        sig_count += 1
        # balanced-paren scan for the parameter list
        depth, k = 1, m.end()
        while k < len(src) and depth:
            if src[k] == "(":
                depth += 1
            elif src[k] == ")":
                depth -= 1
            k += 1
        params = src[m.end():k - 1]
        line = src[:m.start()].count("\n") + 1
        for p in re.split(r",(?![^<(]*[>)])", params):
            p = p.strip()
            if not p or ":" not in p:
                continue
            pname, ptype = p.split(":", 1)
            byval, base = base_type(ptype)
            if not byval or not base:
                continue
            if base in transparent:
                continue
            for path_expr, ebase, how in walk_byvalue(base, pname.strip()):
                hits.append((name, path_expr, ebase, how, f"{rel}:{line}",
                             " | ".join(closed_enums[ebase])))

if sig_count == 0:
    print("[D11] FAIL (VACUOUS): found ZERO `extern \"C\" fn rustty_*` "
          "signatures. A scanner that matches nothing must be a loud failure.")
    sys.exit(1)

unlisted = [h for h in hits if (h[0], h[1]) not in ALLOW]
matched = {(h[0], h[1]) for h in hits}
stale = [k for k in ALLOW if k not in matched]

print(f"[D11] scanned {sig_count} exported extern \"C\" fn signature(s); "
      f"{len(closed_enums)} closed #[repr(C)] enum type(s); "
      f"{len(transparent)} #[repr(transparent)] type(s) exempt.")

if stale:
    print("[D11] STALE ALLOWLIST — these rows match no live site; delete them:")
    for k in stale:
        print(f"        {k[0]}({k[1]})")

if unlisted:
    print("[D11] FAIL — closed #[repr(C)] enums received BY VALUE across the C ABI:")
    for (fn, param, enum, how, site, decl) in sorted(set(unlisted)):
        print(f"        {fn}({param}) : {enum}   [{how}]")
        print(f"          export {site}")
        n = decl.count("|") + 1
        if n > 1:
            print(f"          enum   AMBIGUOUS bare name, {n} declarations: {decl}")
        else:
            print(f"          enum   {decl}")
    print("[D11] Fix: take the value as the C integer type and narrow it with an")
    print("[D11] explicit `match`, or give the type #[repr(transparent)] the way")
    print("[D11] crates/rustty-vt-c/src/modes.rs:26 `ReportState(pub c_int)` does.")
    print("[D11] Or add a row to ALLOW in this gate WITH a written justification.")

sys.exit(1 if (unlisted or stale) else 0)
PYEOF
