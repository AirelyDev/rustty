#!/usr/bin/env python3
"""Per-TEST oracle->port coverage audit (pass 3), against the LIVE oracle.

G1 (`check_tests_ported.py`) already compares test blocks — but it reads
`.port/phase-0/inventory/` **frozen at 4749c4e93**. Every test upstream added
after that pin is in neither its numerator nor its denominator, so G1 can be
green while a whole commit's worth of new tests is unported. That blind spot is
the same one G-FILES found at file level, and it is worth its own gate rather
than a note.

This reads `test "name" {` blocks out of the LIVE oracle tree and asks whether a
counterpart exists in `crates/`. Matching is by NAME, normalized:

    test "cursorScrollRegionUp simple"   ->  cursor_scroll_region_up_simple
    test "PageList: pageIterator"        ->  page_list_page_iterator / pagelist_...

A test counts as PRESENT if any normalized fragment of its name appears as a
substring of some Rust `fn` name in the tree. Deliberately generous, for the
reason stated in `audit_symbol_coverage.py`: a false ABSENT costs a reader's
time and tempts an audit "fix" rather than a code fix.

Accepted absences are argued in `.port/phase-3/TEST_AUDIT.md`, same standard as
G-FILES/G-SYMS: an absence is acceptable when someone wrote down why.
"""
import os
import re
import subprocess
import sys

ROOT = subprocess.run(["git", "rev-parse", "--show-toplevel"],
                      cwd=os.path.dirname(os.path.abspath(__file__)),
                      capture_output=True, text=True, check=True).stdout.strip()
ORACLE = os.path.abspath(os.path.join(ROOT, "..", "ghostty"))
SKIP_PREFIXES = ("src/build/", "src/bench/")

TEST_BLOCK = re.compile(r'^\s*test\s+"([^"]+)"\s*\{', re.M)
# NOTE the leading char class: this port KEEPS the oracle's camelCase type
# prefix on ported test names (`fn CircBuf_first_last`, `fn SplitTree_zoom`) for
# traceability. A `[a-z_]`-anchored pattern silently skipped every one of them
# and reported 54 tests as absent that were present and passing — the audit was
# measuring its own regex, not the port.
RS_FN = re.compile(r"\bfn\s+([A-Za-z_][A-Za-z0-9_]*)")


def norm(s):
    """Lowercase, split camelCase, collapse non-alphanumerics to underscores."""
    s = re.sub(r"(.)([A-Z][a-z]+)", r"\1_\2", s)
    s = re.sub(r"([a-z0-9])([A-Z])", r"\1_\2", s)
    # Split LETTER<->DIGIT too. Without this, "generate256Color" normalized to
    # the single token "generate256", which matches nothing in a port that spells
    # it `generate_256_color` — and all nine of that function's tests were
    # reported absent while present and passing. Third matcher bug of this audit;
    # every one produced FALSE gaps, which is why each reported gap here is
    # confirmed against the code before it is believed.
    s = re.sub(r"([A-Za-z])([0-9])", r"\1_\2", s)
    s = re.sub(r"([0-9])([A-Za-z])", r"\1_\2", s)
    s = re.sub(r"[^A-Za-z0-9]+", "_", s).lower()
    return [p for p in s.split("_") if len(p) > 2]


def main():
    argv = sys.argv[1:]
    limit = int(argv[argv.index("--limit") + 1]) if "--limit" in argv else 40

    acc_path = os.path.join(ROOT, ".port/phase-3/TEST_AUDIT.md")
    accepted = set()
    if os.path.exists(acc_path):
        accepted = set(re.findall(r'`(src/[\w/]+\.zig::[^`]+)`',
                                  open(acc_path, errors="ignore").read()))

    # Every Rust fn name in the tree, as one blob for substring containment.
    fns = set()
    for base, dirs, files in os.walk(os.path.join(ROOT, "crates")):
        dirs[:] = [d for d in dirs if d != "target"]
        for f in files:
            if f.endswith(".rs"):
                try:
                    fns.update(RS_FN.findall(
                        open(os.path.join(base, f), errors="ignore").read()))
                except OSError:
                    pass
    blob = "\n".join(sorted(fns)).lower()

    files = subprocess.run(["git", "ls-files", "src"], cwd=ORACLE,
                           capture_output=True, text=True, check=True).stdout.split()
    nonports_path = os.path.join(ROOT, ".port/phase-3/NON_PORTS.md")
    nonported = set()
    if os.path.exists(nonports_path):
        nonported = set(re.findall(r"`(src/[\w/]+\.zig)`",
                                   open(nonports_path, errors="ignore").read()))
    zig = [f for f in files if f.endswith(".zig")
           and not any(f.startswith(p) for p in SKIP_PREFIXES)
           and f not in nonported]

    total, gaps, acked = 0, [], []
    for rel in sorted(zig):
        try:
            src = open(os.path.join(ORACLE, rel), errors="ignore").read()
        except OSError:
            continue
        stem = os.path.basename(rel)[:-4].lower()
        for name in TEST_BLOCK.findall(src):
            total += 1
            # Strip a leading module qualifier that merely repeats the file name:
            # `test "color: parse valid"` in color.zig. In Rust the module IS the
            # file, so the ported fn is `parse_valid` and the qualifier can never
            # appear in its name — matching on it produces a guaranteed false
            # absence.
            if ":" in name:
                head, _, tail = name.partition(":")
                if norm(head) and norm(head)[0] in (stem, stem.replace("_", "")):
                    name = tail.strip() or name
            parts = norm(name)
            if not parts:
                continue
            # PRESENT if the distinctive parts of the name co-occur in some fn.
            need = parts[:4]
            if any(all(p in fn.lower() for p in need) for fn in fns):
                continue
            # A RENAMED test still counts, but only if TWO distinctive fragments
            # co-occur in ONE fn name.
            #
            # THIS ARM WAS VACUOUS AND THE CONTROL CAUGHT IT. It used to accept
            # "the longest fragment appears anywhere in the tree", which any long
            # word satisfies — renaming a ported test away left the gate GREEN,
            # i.e. it could not detect the exact thing it exists to detect. Two
            # fragments in the same identifier is a real signal; one fragment
            # somewhere in 800 files is not.
            long_parts = [p for p in parts if len(p) >= 6]
            if len(long_parts) >= 2 and any(
                sum(p in fn.lower() for p in long_parts) >= 2 for fn in fns
            ):
                continue
            key = f"{rel}::{name}"
            (acked if key in accepted else gaps).append((rel, name))

    for rel, name in gaps[:limit]:
        print(f'ABSENT  {rel:46} test "{name}"')
    if len(gaps) > limit:
        print(f"… and {len(gaps) - limit} more (raise --limit)")

    print(f"\nTEST AUDIT: {total} live-oracle test blocks across {len(zig)} files; "
          f"{len(gaps)} unexplained, {len(acked)} accepted with a written reason")
    if total < 1000:
        print(f"RED (VACUOUS): only {total} test blocks found; expected thousands.")
        return 1
    if gaps:
        print("RED: an oracle test has no counterpart and no TEST_AUDIT.md entry.")
        return 1
    print("GREEN: every live-oracle test has a counterpart or a written reason.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
