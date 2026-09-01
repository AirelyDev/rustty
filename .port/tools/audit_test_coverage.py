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
# The oracle sits BESIDE THE MAIN CHECKOUT, which is NOT `ROOT` when this runs
# inside a git worktree: `ROOT/../ghostty` resolves to
# `.claude/worktrees/ghostty`, which does not exist. Measured on a scratch
# worktree, the old form died with
# `FileNotFoundError: ... '/…/tmp/ghostty'` — loud, at least, but it made these
# tools unrunnable from every agent worktree, and one lane worked around it by
# creating a symlink. A tool that needs a symlink to be correct is one that will
# be wrong the next time nobody makes one. `--git-common-dir` points at the MAIN
# repo's .git from inside any worktree, so its parent is always the real
# checkout; falling back to ROOT keeps the non-worktree case identical.
def _oracle_root():
    common = subprocess.run(
        ["git", "rev-parse", "--path-format=absolute", "--git-common-dir"],
        cwd=ROOT, capture_output=True, text=True).stdout.strip()
    main_checkout = os.path.dirname(common) if common else ROOT
    return os.path.abspath(os.path.join(main_checkout, "..", "ghostty"))


ORACLE = _oracle_root()
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


def analyze(zig, fns, cites, accepted):
    """The whole decision procedure, as a pure function of its four inputs.

    Split out from `main` for ONE reason: so `--self-test` can feed it doctored
    inputs and prove each acceptance arm can actually fail. An earlier version of
    this gate had a vacuous arm and my control DID NOT FIRE — renaming a ported
    test away left it GREEN. A gate whose FAIL path has never been demonstrated
    on real rows is indistinguishable from one that cannot fail.
    """
    total, gaps, acked = 0, [], []
    for rel in sorted(zig):
        try:
            src = open(os.path.join(ORACLE, rel), errors="ignore").read()
        except OSError:
            continue
        stem = os.path.basename(rel)[:-4].lower()
        for name in TEST_BLOCK.findall(src):
            total += 1
            # A verbatim citation of the oracle test name in a PORT NOTE settles
            # it before any name heuristic runs.
            if re.sub(r"\s+", " ", name) in cites:
                continue
            # Strip a leading module qualifier that merely repeats the file name:
            # `test "color: parse valid"` in color.zig. In Rust the module IS the
            # file, so the ported fn is `parse_valid` and the qualifier can never
            # appear in its name — matching on it produces a guaranteed false
            # absence.
            if ":" in name:
                head, _, tail = name.partition(":")
                # Compare the JOINED normalization, not just its first token. The
                # old `norm(head)[0]` form compared "glyf" against the stem
                # "glyf_rasterize", never matched, left the qualifier on the
                # name, and reported a present-and-passing test as absent — the
                # FOURTH false absence this matcher has produced.
                hn = "_".join(norm(head))
                if hn and hn in (stem, stem.replace("_", "")):
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
            # ...AND that fn must cover most of the name, not just two words of
            # it. Two fragments alone is maskable by a SIBLING IN THE SAME TEST
            # FAMILY: `PageList cold compression continues after an
            # incompressible page` was accepted by
            # `pagelist_compression_leaves_incompressible_pages_resident`, a
            # different test that merely shares the family's vocabulary. The
            # self-test arm below caught exactly that. Requiring 70% of the
            # name's distinct tokens surfaced 16 further absences, all in the
            # same compression series.
            long_parts = [p for p in parts if len(p) >= 6]
            uniq = list(dict.fromkeys(parts))
            if len(long_parts) >= 2 and any(
                sum(p in fn.lower() for p in long_parts) >= 2
                and sum(p in fn.lower() for p in uniq) >= 0.70 * len(uniq)
                for fn in fns
            ):
                continue
            key = f"{rel}::{name}"
            (acked if key in accepted else gaps).append((rel, name))
    return total, gaps, acked


def self_test(zig, fns, cites, accepted):
    """Break each acceptance arm on a REAL row and require the gate to notice.

    Every arm below is checked against a specific test that lives in the tree
    today, so these controls fail loudly if someone deletes the row rather than
    silently passing on an empty set.
    """
    failures = []

    def arm(label, z, f, c, a, must_contain):
        _t, gaps, _k = analyze(z, f, c, a)
        found = any(must_contain in f"{rel}::{name}" for rel, name in gaps)
        print(f"  {'ok  ' if found else 'FAIL'} {label}")
        if not found:
            failures.append(label)

    # 1. NAME arm: a test matched only by fn-name similarity must be reported
    #    when that fn is renamed away. This is the exact control that did not
    #    fire against the old vacuous fallback.
    victim = "pagelist_cold_compression_continues_after_an_incompressible_page"
    assert victim in fns, "self-test anchor gone: re-point arm 1"
    arm("name arm fires when the ported fn is renamed away",
        [z for z in zig if z.endswith("terminal/PageList.zig")],
        {f for f in fns if f != victim}, "",
        set(), "cold compression continues after an incompressible page")

    # 2. CITATION arm: a test matched ONLY by a verbatim PORT NOTE citation must
    #    be reported when the citation is removed. Anchored on a gsettings test,
    #    whose Rust name (`gsettings_key_tag_names`) shares no fragment with the
    #    oracle name — so nothing but the citation can be carrying it.
    cite = "@tagName returns correct GTK property names"
    assert cite in cites, "self-test anchor gone: re-point arm 2"
    arm("citation arm fires when the PORT NOTE citation is removed",
        [z for z in zig if z.endswith("gtk/gsettings.zig")],
        fns, cites.replace(cite, ""), set(), cite)

    # 3. ACCEPTED arm: an absence recorded in TEST_AUDIT.md must come back as a
    #    GAP when the entry is withdrawn — otherwise the ledger is decorative.
    key = "src/terminal/c/unicode.zig::codepoint_width wide"
    assert key in accepted, "self-test anchor gone: re-point arm 3"
    arm("accepted arm fires when the TEST_AUDIT.md entry is withdrawn",
        [z for z in zig if z.endswith("terminal/c/unicode.zig")],
        fns, cites, accepted - {key}, key)

    # 4. VACUITY arm: an empty oracle read must be RED, not a clean 0/0 pass.
    total, _g, _k = analyze([], fns, cites, accepted)
    ok = total < 1000
    print(f"  {'ok  ' if ok else 'FAIL'} vacuity arm trips on an empty oracle read")
    if not ok:
        failures.append("vacuity")

    if failures:
        print(f"SELF-TEST RED: {len(failures)} arm(s) could not fail: {failures}")
        return 1
    print("SELF-TEST GREEN: all 4 acceptance arms demonstrated reachable FAIL.")
    return 0


def main():
    argv = sys.argv[1:]
    limit = int(argv[argv.index("--limit") + 1]) if "--limit" in argv else 40

    acc_path = os.path.join(ROOT, ".port/phase-3/TEST_AUDIT.md")
    accepted = set()
    if os.path.exists(acc_path):
        accepted = set(re.findall(r'`(src/[\w/]+\.zig::[^`]+)`',
                                  open(acc_path, errors="ignore").read()))

    # Every Rust fn name in the tree, plus the raw source text — the latter so a
    # PORT NOTE that CITES the oracle test verbatim counts as a match. That
    # citation is the port's own claim of provenance, the same signal G-FILES
    # relies on at file level, and it is the only thing that survives a rename:
    # `test "Key.Type returns correct types"` is ported as
    # `fn gsettings_key_type`, which no name-similarity heuristic will ever link
    # back. Requiring the LITERAL oracle name keeps this from going vacuous.
    fns = set()
    text = []
    for base, dirs, files in os.walk(os.path.join(ROOT, "crates")):
        dirs[:] = [d for d in dirs if d != "target"]
        for f in files:
            if f.endswith(".rs"):
                try:
                    src = open(os.path.join(base, f), errors="ignore").read()
                except OSError:
                    continue
                fns.update(RS_FN.findall(src))
                text.append(src)
    # Rewrap doc comments before matching: rustfmt splits a long PORT NOTE mid
    # citation, so `test "Key.GValueType returns correct GObject types"` is
    # stored as `... GObject\n    /// types"` and a literal search misses it. Two
    # of these were reported absent while cited correctly.
    cites = "\n".join(text)
    cites = re.sub(r"\n\s*(?:///|//!|//|\*)?[ \t]*", " ", cites)
    cites = re.sub(r"\s+", " ", cites)

    files = subprocess.run(["git", "ls-files", "src"], cwd=ORACLE,
                           capture_output=True, text=True, check=True).stdout.split()
    # EVERY phase's non-port ledger, not just phase 3's. The ledgers are written
    # per phase (`.port/phase-N/NON_PORTS.md`) and reading one of them was an
    # accident of the phase this tool was written in: `src/lib/TinyIo.zig` — a
    # CONFIRMED non-port with a written rationale and a reopen trigger in
    # `.port/phase-4/NON_PORTS.md` — had its 19 tests reported as unexplained
    # gaps, which is the opposite of what the ledger says about them. The
    # standard is unchanged (an absence is acceptable when someone WROTE DOWN
    # why); this just stops the tool from ignoring where they wrote it.
    nonported = set()
    for phase in sorted(os.listdir(os.path.join(ROOT, ".port"))):
        p = os.path.join(ROOT, ".port", phase, "NON_PORTS.md")
        if os.path.exists(p):
            nonported |= set(re.findall(r"`(src/[\w/]+\.zig)`",
                                        open(p, errors="ignore").read()))
    zig = [f for f in files if f.endswith(".zig")
           and not any(f.startswith(p) for p in SKIP_PREFIXES)
           and f not in nonported]

    if "--self-test" in argv:
        return self_test(zig, fns, cites, accepted)

    total, gaps, acked = analyze(zig, fns, cites, accepted)

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
