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
"""
import argparse
import os
import re
import shutil
import subprocess
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import _gatelib as G  # noqa: E402

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
    return out


def crate_token_corpus(crates):
    """Concatenate all ported Rust source of the given crates, comment-stripped
    but with STRING LITERALS KEPT, for token presence checks. Cap/action ids
    live in `b"..."` literals (id_variants' documented quoted-string form);
    blanking strings alongside comments made every literal-only id a false
    MISSING (the terminfo 13/268 false RED once the crate claimed done).
    Comments stay stripped so a verbatim-commented Zig block can never fake
    coverage (tightening preserved)."""
    blobs = []
    for crate in crates:
        for path in G.iter_rust_files(crate):
            try:
                with open(path, encoding="utf-8", errors="replace") as fh:
                    text = fh.read()
            except Exception:  # noqa: BLE001
                continue
            clean, _ = G._strip_rust(text, keep_strings=True)
            blobs.append(clean)
    return "\n".join(blobs)


def crate_test_corpus(crates):
    """Same as crate_token_corpus but only the bodies of #[test] fns (so a
    surface id is 'test-covered' if it appears inside a ported test).

    Structure (attrs/fn names/brace spans) is computed on the fully-stripped
    text so braces inside string literals cannot desync the block finder; the
    emitted body slice comes from the strings-kept text (both strippers are
    offset-preserving, so the spans map 1:1)."""
    blobs = []
    for crate in crates:
        for path in G.iter_rust_files(crate):
            try:
                with open(path, encoding="utf-8", errors="replace") as fh:
                    text = fh.read()
            except Exception:  # noqa: BLE001
                continue
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


def check_terminfo_infocmp(surface):
    """If `infocmp rustty` is available, diff its cap set against the surface
    (R2-terminfo-cap-parity). Returns (available, missing_caps_list, note)."""
    if shutil.which("infocmp") is None:
        return False, [], "infocmp not on PATH"
    try:
        r = subprocess.run(["infocmp", "-1", "rustty"], capture_output=True, text=True)
    except Exception as e:  # noqa: BLE001
        return False, [], f"infocmp error: {e}"
    if r.returncode != 0:
        return False, [], "rustty terminfo entry not compiled/installed yet"
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
        blob = crate_token_corpus(owners)
        test_blob = crate_test_corpus(owners) if require_tests else ""

        uncovered = []
        covered = 0
        for name in kind_ids:
            is_present = present(name, blob)
            is_tested = (not require_tests) or present(name, test_blob)
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
        print(f"{kind:<10}: covered {covered}/{len(kind_ids)}  owners={','.join(owners)} "
              f"({'done' if owners_done else 'stub'})  -> {state}")
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

    shutil.rmtree(tmp, ignore_errors=True)
    print("SELF-TEST:", "PASS" if ok else "FAIL")
    return 0 if ok else 1


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
