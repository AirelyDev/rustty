#!/usr/bin/env python3
"""Per-SYMBOL oracle->port coverage audit (pass 2).

`audit_file_coverage.py` answers "does a counterpart file exist". This answers
the next question down: **does every public function in that oracle file have a
counterpart symbol somewhere in the port?** A file can exist, be the right size,
cite its oracle source, and still be missing individual `pub fn`s — that is
precisely the shape of a partial port, and nothing else here looks for it.

MATCHING. Zig `pub fn fooBar` becomes Rust `fn foo_bar` by R2 convention, but
this port also keeps oracle spellings in PORT NOTEs and sometimes keeps camelCase
for fidelity. So a symbol counts as PRESENT if any of these appears anywhere in
the crates tree:
    fooBar        (verbatim, e.g. in a PORT NOTE or a kept name)
    foo_bar       (the R2 snake_case form)
This is deliberately generous. A false PRESENT is a missed gap, but a false
MISSING wastes a human's time and — worse, on this project's evidence — invites
someone to "fix" the audit rather than the code. Generous matching plus a written
justification for every accepted absence is the safer bias.

Usage:
  python3 .port/tools/audit_symbol_coverage.py                 # gaps only
  python3 .port/tools/audit_symbol_coverage.py --file src/terminal/page.zig
  python3 .port/tools/audit_symbol_coverage.py --limit 40      # cap output
"""
import os
import re
import subprocess
import sys

# The checkout this tool belongs to, from the script's own location, and the
# oracle beside the MAIN checkout, from the shared `_gatelib.oracle_root`.
# The reasoning, and the R5 history (ROOT from `--show-toplevel` and a private
# resolver, neither stripping `GIT_*`), is in `audit_file_coverage.py`.
# G-TESTS-SELF (`audit_test_coverage.py --self-test`) proves that GIT_DIR +
# GIT_WORK_TREE move none of ROOT, ORACLE or `oracle_files()` here.
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "gates"))
import _gatelib as G  # noqa: E402

ROOT = G.RUSTTY_ROOT
ORACLE = G.oracle_root(ROOT)


def oracle_files():
    """`git ls-files src` in the oracle, with `GIT_*` removed (R5)."""
    return subprocess.run(["git", "ls-files", "src"], cwd=ORACLE, env=G.git_env(),
                          capture_output=True, text=True, check=True).stdout.split()
SKIP_PREFIXES = ("src/build/", "src/bench/")

PUB_FN = re.compile(r"^\s*pub fn ([A-Za-z_][A-Za-z0-9_]*)\s*\(", re.M)


def camel_to_snake(name):
    s = re.sub(r"(.)([A-Z][a-z]+)", r"\1_\2", name)
    return re.sub(r"([a-z0-9])([A-Z])", r"\1_\2", s).lower()


def build_symbol_index():
    """One pass over the whole crates tree; every identifier-ish token."""
    idx = set()
    tok = re.compile(r"[A-Za-z_][A-Za-z0-9_]*")
    for base, dirs, files in os.walk(os.path.join(ROOT, "crates")):
        dirs[:] = [d for d in dirs if d != "target"]
        for f in files:
            if not f.endswith(".rs"):
                continue
            try:
                with open(os.path.join(base, f), errors="ignore") as fh:
                    idx.update(tok.findall(fh.read()))
            except OSError:
                pass
    return idx


def main():
    argv = sys.argv[1:]
    only = argv[argv.index("--file") + 1] if "--file" in argv else None
    limit = int(argv[argv.index("--limit") + 1]) if "--limit" in argv else 60

    nonports_path = os.path.join(ROOT, ".port/phase-3/NON_PORTS.md")
    nonported = set()
    if os.path.exists(nonports_path):
        nonported = set(re.findall(r"`(src/[\w/]+\.zig)`",
                                   open(nonports_path, errors="ignore").read()))

    files = oracle_files()
    zig = [f for f in files if f.endswith(".zig")
           and not any(f.startswith(p) for p in SKIP_PREFIXES)
           and f not in nonported]
    if only:
        zig = [f for f in zig if f == only]

    print(f"indexing port symbols…", file=sys.stderr)
    idx = build_symbol_index()

    # Absences ARGUED in writing. Same standard as G-FILES: an absence is
    # acceptable when someone wrote down why, never merely because it is old.
    acc_path = os.path.join(ROOT, ".port/phase-3/SYMBOL_AUDIT.md")
    accepted = set()
    if os.path.exists(acc_path):
        accepted = set(re.findall(r"`(src/[\w/]+\.zig::[A-Za-z_][A-Za-z0-9_]*)`",
                                  open(acc_path, errors="ignore").read()))

    total = 0
    gaps = []
    acked = []
    for rel in sorted(zig):
        try:
            src = open(os.path.join(ORACLE, rel), errors="ignore").read()
        except OSError:
            continue
        for name in PUB_FN.findall(src):
            if name in ("main", "deinit", "init"):     # too generic to be evidence
                continue
            total += 1
            snake = camel_to_snake(name)
            # PREFIX match, not exact. This port routinely SUFFIXES a ported
            # name — `putAssumeCapacityNoClobber` lands as
            # `put_assume_capacity_no_clobber_context` — and exact token
            # matching reported three such functions as absent when all three
            # were present and in use. A suffixed name is still the port of that
            # function; a prefix of a longer identifier is not a coincidence at
            # these lengths.
            if name in idx or snake in idx:
                continue
            if len(snake) >= 8 and any(t.startswith(snake) for t in idx):
                continue
            if f"{rel}::{name}" in accepted:
                acked.append((rel, name))
                continue
            gaps.append((rel, name))

    for rel, name in gaps[:limit]:
        print(f"ABSENT  {rel:52} pub fn {name}")
    if len(gaps) > limit:
        print(f"… and {len(gaps) - limit} more (raise --limit)")

    print(f"\nSYMBOL AUDIT: {total} oracle `pub fn` checked across {len(zig)} files; "
          f"{len(gaps)} unexplained, {len(acked)} accepted with a written reason")
    if total < 500:
        print(f"RED (VACUOUS): only {total} symbols checked; expected thousands.")
        return 1
    if gaps:
        print("RED: an oracle `pub fn` has no counterpart and no SYMBOL_AUDIT.md entry.")
        return 1
    print("GREEN: every oracle `pub fn` has a counterpart or a written reason.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
