#!/usr/bin/env python3
"""Per-file oracle->port coverage audit.

WHY: every existing gate answers a DIFFERENT question — G1 counts ported test
blocks, G3b counts comment sentences, G1b counts per-module tests. None of them
answers the blunt one: "is there a Rust counterpart for this oracle file at all,
and if so how does its size compare?" A file can be absent entirely and still be
invisible to all three (G1/G2/G3 read a FROZEN phase-0 inventory; a file added
upstream after the pin is in none of them).

This walks the LIVE oracle tree and reports, per file:
  MISSING   no plausible Rust counterpart found
  THIN      counterpart exists but is <40% of the oracle's non-blank lines
  OK        counterpart exists and is plausibly complete

THIN is a heuristic and deliberately loud rather than precise: a Rust port is
usually LONGER than its Zig source (PORT NOTE comments, explicit error types), so
a counterpart at under 40% is a strong signal of a stub or a partial port and
worth a human look. It is a triage tool, not a gate.

Usage:
  python3 .port/tools/audit_file_coverage.py            # summary + gaps
  python3 .port/tools/audit_file_coverage.py --all      # every row
  python3 .port/tools/audit_file_coverage.py --crate vt # one crate's files
"""
import json
import os
import re
import subprocess
import sys

# The checkout this tool belongs to, from the script's own location, and the
# oracle beside the MAIN checkout, from the shared `_gatelib.oracle_root`.
#
# `ROOT/../ghostty` is wrong inside a git worktree: it resolves to
# `.claude/worktrees/ghostty`, which does not exist. Measured on a scratch
# worktree, that form died with `FileNotFoundError: ... '/…/tmp/ghostty'` — loud,
# at least, but it made these tools unrunnable from every agent worktree, and one
# lane worked around it by creating a symlink. `oracle_root` asks git for
# `--git-common-dir`, whose parent is the real checkout from inside any worktree.
#
# R5: nothing here is caller-settable. `ROOT` used to come from
# `git rev-parse --show-toplevel` and this tool kept its own copy of the
# resolver; neither stripped `GIT_*`, so `GIT_DIR` + `GIT_WORK_TREE` moved ROOT
# and the oracle onto a decoy tree. `oracle_root` runs git with `GIT_*` removed
# and uses its answer only on exit 0, and `oracle_files` lists the oracle the
# same way. G-TESTS-SELF (`audit_test_coverage.py --self-test`) proves it for
# all three audit tools.
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "gates"))
import _gatelib as G  # noqa: E402

ROOT = G.RUSTTY_ROOT
ORACLE = G.oracle_root(ROOT)


def oracle_files():
    """`git ls-files src` in the oracle, with `GIT_*` removed (R5)."""
    return subprocess.run(["git", "ls-files", "src"], cwd=ORACLE, env=G.git_env(),
                          capture_output=True, text=True, check=True).stdout.split()


# Oracle subtrees that have no Rust counterpart by design.
SKIP_PREFIXES = ("src/build/", "src/bench/")


def nonblank(path):
    try:
        with open(path, errors="ignore") as fh:
            return sum(1 for ln in fh if ln.strip())
    except OSError:
        return 0


def camel_to_snake(name):
    s = re.sub(r"(.)([A-Z][a-z]+)", r"\1_\2", name)
    return re.sub(r"([a-z0-9])([A-Z])", r"\1_\2", s).lower()


def cited_by(rel):
    """Rust files that CITE this oracle path.

    This is the reliable signal, and filename matching is the fallback. Every
    ported file in this repo carries a PORT NOTE naming its oracle source, so a
    citation is the port's own claim of provenance — whereas a filename guess
    misses every renamed, split or flattened port. The first run of this audit
    reported 21 MISSING of which 17 were false positives (Config.zig ->
    config_struct.rs, PageList.zig -> pagelist/*.rs, ipc/DBus.zig flattened into
    app.rs, ...), which is what motivated this.
    """
    base = os.path.basename(rel)
    pats = [rel[4:], base]          # "terminal/page.zig" and "page.zig"
    hits = []
    for pat in pats:
        r = subprocess.run(["grep", "-rl", "--include=*.rs", "-F", pat, "crates"],
                           cwd=ROOT, capture_output=True, text=True)
        hits += [h for h in r.stdout.split() if "/target/" not in h]
        if hits:
            break
    return hits


def rust_candidates(rel):
    """Plausible Rust paths for an oracle src/-relative path."""
    parts = rel.split("/")            # e.g. src/terminal/page.zig
    stem = parts[-1][:-4]             # page
    snake = camel_to_snake(stem)
    names = {snake, snake + "_impl", stem.lower()}
    # `Foo.zig` (a type file) often becomes foo.rs or foo/mod.rs
    out = []
    for base, _dirs, files in os.walk(os.path.join(ROOT, "crates")):
        if "/target/" in base or base.endswith("/target"):
            continue
        for f in files:
            if not f.endswith(".rs"):
                continue
            if f[:-3] in names or (f == "mod.rs" and os.path.basename(base) in names):
                out.append(os.path.join(base, f))
    return out


def main():
    argv = sys.argv[1:]
    show_all = "--all" in argv
    only = None
    if "--crate" in argv:
        only = argv[argv.index("--crate") + 1]

    files = oracle_files()
    zig = [f for f in files if f.endswith(".zig")
           and not any(f.startswith(p) for p in SKIP_PREFIXES)]

    # Files argued as deliberate non-ports in the ledger.
    nonports_path = os.path.join(ROOT, ".port/phase-3/NON_PORTS.md")
    nonported = set()
    if os.path.exists(nonports_path):
        txt = open(nonports_path, errors="ignore").read()
        nonported = set(re.findall(r"`(src/[\w/]+\.zig)`", txt))

    rows, missing, thin, recorded = [], [], [], []
    for rel in sorted(zig):
        if only and f"/{only}/" not in rel and not rel.startswith(f"src/{only}"):
            continue
        zlines = nonblank(os.path.join(ORACLE, rel))
        # An ARGUED non-port is settled before any size heuristic runs.
        #
        # This used to be checked only in the `not cands` branch, which made the
        # disposition depend on whether anything happened to MENTION the file.
        # `quirks_memset.zig` is recorded as a deliberate non-port at NON_PORTS.md
        # E34; when a PORT NOTE explaining *why it is not ported* was added to
        # `rustty-bench/src/main_bench.rs`, `cited_by()` matched it, `cands` went
        # non-empty, the NON-PORT branch was skipped, and the file flipped to THIN
        # (49 vs 304 lines) — turning the gate RED for documenting the decision it
        # already accepted. Writing down a reason must never make a gate angrier.
        if rel in nonported:
            rows.append(("NON-PORT", rel, zlines, 0))
            recorded.append(rel)
            continue
        # UNION, not fallback. A filename match short-circuiting the citation
        # lookup is what kept Config.zig reading THIN at 463 lines: `config.rs`
        # exists and matched, so the 27k-line config_struct.rs that actually
        # cites it was never consulted.
        cands = set(rust_candidates(rel))
        cands |= {os.path.join(ROOT, c) for c in cited_by(rel)}
        # A split port's counterpart is the SUM of its parts, not the largest —
        # PageList.zig's 15934 lines map to 22045 across pagelist/*.rs, and a
        # max() over those files reported it as THIN at 2692.
        rlines = sum(nonblank(c) for c in cands)
        if not cands:
            # A file with no counterpart is only acceptable if it is RECORDED as
            # a deliberate non-port. That is the same standard the rest of this
            # port holds: an absence has to be argued in writing, not inferred
            # from silence. Unrecorded absence is the failure this gate exists
            # to catch.
            if rel in nonported:
                status = "NON-PORT"
                recorded.append(rel)
            else:
                status = "MISSING"
                missing.append((rel, zlines))
        elif zlines >= 40 and rlines < 0.40 * zlines:
            status = "THIN"
            thin.append((rel, zlines, rlines))
        else:
            status = "OK"
        rows.append((status, rel, zlines, rlines))

    if show_all:
        for st, rel, z, r in rows:
            print(f"{st:8} {rel:60} zig={z:5} rust={r:5}")
    else:
        for rel, z in missing:
            print(f"MISSING  {rel:60} zig={z}")
        for rel, z, r in thin:
            print(f"THIN     {rel:60} zig={z:5} rust={r:5}")

    n = len(rows)
    ok = n - len(missing) - len(thin) - len(recorded)
    print(f"\nAUDIT: {n} oracle files examined ({len(missing)} MISSING, "
          f"{len(thin)} THIN, {len(recorded)} recorded NON-PORT, {ok} OK)")
    # ANTI-VACUITY: examining zero files means the oracle path is wrong or the
    # tree is empty, and a silent 0/0 would read as a clean pass.
    if n < 100:
        print(f"RED (VACUOUS): only {n} oracle files examined; expected ~500. "
              f"Is {ORACLE} the oracle checkout?")
        return 1
    if missing or thin:
        print("RED: an oracle file has no counterpart and no NON_PORTS.md entry.")
        return 1
    print("GREEN: every oracle file has a counterpart or a written non-port.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
