#!/usr/bin/env python3
"""G-P2: the Phase 2 CLOSURE gate.

WHAT IT ENFORCES
----------------
Phase 2 is a COMPLETION roadmap, so "deferred to a later phase" is not an acceptable
disposition. Every open item in the Phase-2 ledgers must end as exactly one of:

  * FIXED / struck (`~~...~~`, RESOLVED, DONE, ...);
  * a permanent **Justified R1 Divergence** (the `# Justified R1 Divergences` section);
  * **verification debt** with a runnable recipe (`VERIFICATION_DEBT.md`);
  * OPEN, but with a named **OWNER** that is an OPEN workstream (or `Phase 3`).

It ALSO asserts a second, stronger property: that **WS19's own closure tasks are all
executed**. An audit-index row marked `NOT DONE` is a failure even though it is properly
owned, because otherwise GREEN would mean only "no orphans" and would be read — wrongly
— as "Phase 2 is closed". With both properties, GREEN means exactly one thing: Phase 2
is closed.

This gate FAILS on the last case when the owner is missing, or names a workstream that
is CLOSED. Filing work into a closed workstream is how 21 of 38 genuinely-open items
became ORPHANED (see the `# PHASE 2 CLOSURE AUDIT (WS19)` section of DEFERRALS.md);
this gate exists so it cannot happen silently again.

WHY THE OWNER SET IS DERIVED, NOT HARDCODED
-------------------------------------------
OPEN_WORKSTREAMS comes from the ROADMAP's own plan-authoring checklist — the same text
a human reads. A hardcoded list would rot exactly when it matters: the day a workstream
merges, every row still filed against it becomes an orphan, and a stale hardcoded list
would keep passing them. Marking a WS `merged`/`DONE` in the ROADMAP must IMMEDIATELY
make filing new work into it a gate failure. That property is itself checked (see
`--check-derivation`).

ANTI-VACUITY
------------
A parser that silently matches nothing is the classic vacuous green, and this program has
already been bitten by it twice (an `OUT_DIR/locale` nothing read; a `_vt_header_baseline.txt`
no gate read; a guard test scoped to a substring). So:
  * a non-empty ledger file in which ZERO items are found is a LOUD FAILURE;
  * an empty OPEN_WORKSTREAMS set is a LOUD FAILURE (it would make every owner invalid,
    or — worse, if the logic were inverted — every owner acceptable);
  * a ledger file that is missing entirely is exit 3 BLOCKED, never a silent pass
    (the `ci.sh` convention).

EXIT CODES
----------
  0  clean
  1  at least one open item is unowned, or owned by a CLOSED workstream
  3  BLOCKED — a ledger file or the ROADMAP is missing, or the derivation is vacuous

USAGE
-----
  python3 check_phase2_closure.py
  python3 check_phase2_closure.py --fixture <path.md>   # run against one file only
  python3 check_phase2_closure.py --check-derivation    # print the derived WS sets
  python3 check_phase2_closure.py -v                    # also list every owned item
"""

import os
import re
import subprocess
import sys

# The RUST root follows the checkout this script lives in — never a hardcoded absolute
# path. (WS19 T1 fixed exactly that bug in check_test_counts.py, where hardcoded roots
# made the gate silently measure `main` when run from a worktree.)
ROOT = subprocess.run(
    ["git", "rev-parse", "--show-toplevel"],
    cwd=os.path.dirname(os.path.abspath(__file__)),
    capture_output=True,
    text=True,
    check=True,
).stdout.strip()

LEDGERS = [
    ".port/phase-2/DEFERRALS.md",
    ".port/phase-2/VERIFICATION_DEBT.md",
    ".port/phase-2/MIRI_EXCEPTIONS.md",
    ".port/phase-2/INTEGRATION_PARITY.md",
]
ROADMAP = "docs/superpowers/plans/2026-07-23-rustty-completion-ROADMAP.md"

# An "open claim": a line asserting that something is not done.
OPEN_MARKERS = [
    re.compile(r"\bSTILL DEFERRED\b"),
    re.compile(r"\*\*OPEN\b"),
    re.compile(r"\bUn-defer trigger\b", re.I),
    # `Target:` as a DISPOSITION LABEL. The negative lookahead for a second colon is
    # load-bearing: without it this matched Rust code like `Target::Surface` and
    # `Target::<CoreSurface>::App`, which produced two confident FALSE ORPHANS
    # (DEFERRALS.md:2286 and :2458) on entries that were not open at all. A gate that
    # invents violations is as harmful as one that misses them — both destroy trust in
    # the output.
    re.compile(r"\bTarget:(?!:)\s*\*?\*?(?!.*LANDED)", re.I),
    re.compile(r"\bWhat's deferred\b", re.I),
    re.compile(r"\bstays deferred\b", re.I),
    re.compile(r"\bremains deferred\b", re.I),
    re.compile(r"\bDEFERRED\.\*\*"),
]

# An owner: a workstream id, or Phase 3 (the upstream-resync phase). `WS-n` in prose
# ("fix in WS-n") is a placeholder, not an owner, and is deliberately not matched.
OWNER_RE = re.compile(r"\b(WS(?:[1-9][0-9]|[1-9]))\b|\b(Phase[ -]3)\b")

# Text that marks an entry as no longer open.
# STRONG disposition markers only. Deliberately NOT included: bare "DONE", "PORTED",
# "LANDED", "corrected" — those appear in ordinary prose throughout these ledgers, so
# accepting them would let an orphan pass merely because a SIBLING sentence mentioned
# something being finished, degrading the gate into a keyword lottery. A disposition must
# be written as such.
CLOSED_RE = re.compile(
    r"\bRESOLVED\s*/\s*STALE\b"
    r"|\bRESOLVED\b\s*(?:—|-|\(|\[|\.|,|$)"
    r"|\bDISCHARGED\b|\bSUPERSEDED\b|\bRECLASSIFIED\b|\bMOOT\b"
    r"|\bROW DELETED\b|\bSPLIT AND OWNED\b|\bOWNER \[|\*\*OWNER\b"
    # `[RESOLVED <date>, WS<n>]` and `✅ DONE (WS<n>, <date>)` are the two
    # disposition FORMS these ledgers actually use; both are unambiguous (a date or a
    # workstream attribution follows), unlike a bare "done" in prose.
    r"|\[RESOLVED\b|✅|\bDONE \(WS"
    # NOTE: `OUT-OF-SCOPE-BY-PLATFORM` was accepted here and has been REMOVED.
    # It let unported work close by asserting a platform is not a target. The
    # standing rule is the opposite: everything the reference supports gets
    # PORTED; inability to build or verify on this host is VERIFICATION DEBT with
    # a runnable recipe, never a scope exclusion. Removing it re-opened the
    # libxev iocp/wasi_poll (2376 + 1648 reference lines) and termio Windows rows.
    r"|\bNO ACTION\b|\bVERIFICATION DEBT\b|\bJUSTIFIED R1 DIVERGENCE\b",
    re.I,
)

DIVERGENCE_HEADING = re.compile(r"^#\s+Justified R1 Divergences")
AUDIT_HEADING = re.compile(r"^#\s+PHASE 2 CLOSURE AUDIT")
HEADING = re.compile(r"^(#{1,6})\s+(.*)$")
NUMBERED = re.compile(r"^\s{0,3}(\d+)\.\s+(.*)$")


def derive_workstreams(roadmap_text):
    """Derive (open, closed) workstream ids from the ROADMAP's own checklist.

    A checklist line naming a WS is CLOSED if it says merged/DONE, else OPEN. This is
    read from the same text a human would read, on purpose — see the module docstring.
    """
    open_ws, closed_ws = set(), set()
    # Checklist rows WRAP. A line-at-a-time reader sees only the first line and would
    # mis-classify any row whose status words ("not executed. Open.") sit on a
    # continuation line — which is how WS11 first read as CLOSED here. Join each row
    # with its continuation lines before deciding.
    raw = roadmap_text.splitlines()
    rows = []
    cur = None
    for line in raw:
        if re.match(r"^\s*-\s*~?~?\[", line):
            if cur is not None:
                rows.append(cur)
            cur = line
        elif cur is not None and line.startswith("  ") and line.strip():
            cur += " " + line.strip()
        else:
            if cur is not None:
                rows.append(cur)
            cur = None
    if cur is not None:
        rows.append(cur)

    for line in rows:
        ids = {m.group(1) for m in re.finditer(r"\b(WS(?:[1-9][0-9]|[1-9]))\b", line)}
        if not ids:
            continue
        # Only the row's OWN id is decided by the row; ids mentioned in prose
        # ("inherits WS8's row") must not be classified from here.
        first = re.search(r"\b(WS(?:[1-9][0-9]|[1-9]))\b", line).group(1)
        # Precedence matters, and it is deliberately biased towards CLOSED:
        #   1. an explicit "NOT executed" / "not executed" / "open" -> OPEN, even when
        #      the box is ticked (the box tracks PLAN AUTHORING, not execution — WS12,
        #      WS13 and WS17 are `[x]` authored but unexecuted);
        #   2. otherwise a ticked box, or an explicit merged/DONE -> CLOSED. The older
        #      Wave 0/1 rows (WS1-WS5) are ticked without the word "merged"; they ARE
        #      merged, and treating a ticked box as closed is the safe default here
        #      because the DANGEROUS error is calling a closed WS open (which lets an
        #      orphan pass), not the reverse (which produces a loud, fixable RED).
        ticked = re.match(r"^\s*-\s*~?~?\[x\]", line, re.I) is not None
        says_open = re.search(r"\bNOT executed\b|\bnot executed\b|\bopen\b", line) is not None
        says_closed = re.search(r"\bmerged\b|\bDONE\b", line, re.I) is not None
        if says_open and not says_closed:
            open_ws.add(first)
        elif ticked or says_closed:
            closed_ws.add(first)
        else:
            open_ws.add(first)
    # "Phase 3" is NO LONGER a blanket acceptable owner.
    #
    # It was, and 12 rows were green ONLY on that basis — which made "deferred to
    # the next phase" a passing disposition, i.e. exactly the deferral the closure
    # rule forbids. Phase 3 is legitimate for ONE case: work whose source is the
    # beyond-pin upstream range, which by definition cannot be done until the pin
    # advances (e.g. the BL-5 render_state split, `cursor_scroll_region_up` from
    # `77190bd02`). Such a row must say so explicitly by carrying the marker
    # BEYOND-PIN, which `row_is_beyond_pin()` checks at the call site. Phase-2
    # implementation work may NOT be parked here.
    return open_ws, closed_ws


BEYOND_PIN = re.compile(r"\bBEYOND-PIN\b")


def row_is_beyond_pin(text):
    """True when a row's own text proves its source is the beyond-pin upstream
    range, which is the only case where 'Phase 3' is an acceptable owner."""
    return bool(BEYOND_PIN.search(text))


def entries_for(path, text):
    """Split a ledger into ENTRIES: (start_line, end_line, title, body_text).

    An entry is the region governed by a heading or a numbered list item. Open claims
    are attributed to the entry that encloses them, because that is where a disposition
    (owner, strike, reclassification) is actually written.
    """
    lines = text.splitlines()
    starts = []
    for i, line in enumerate(lines):
        m = HEADING.match(line)
        if m:
            starts.append((i, m.group(2)))
            continue
        m = NUMBERED.match(line)
        if m:
            starts.append((i, m.group(2)))
    out = []
    for k, (i, title) in enumerate(starts):
        j = starts[k + 1][0] if k + 1 < len(starts) else len(lines)
        out.append((i, j, title, "\n".join(lines[i:j])))
    return out


def section_flags(text):
    """Line -> (in_divergences, in_audit) for the whole file."""
    flags = []
    in_div = in_audit = False
    for line in text.splitlines():
        if DIVERGENCE_HEADING.match(line):
            in_div, in_audit = True, False
        elif AUDIT_HEADING.match(line):
            in_div, in_audit = False, True
        elif re.match(r"^#\s+", line):
            in_div = in_audit = False
        flags.append((in_div, in_audit))
    return flags


VD_HEADING = re.compile(r"^##\s+~?~?(V\d+[a-z]?)\.")


def check_verification_debt(path, rel, open_ws, closed_ws, verbose):
    """VERIFICATION_DEBT.md has its OWN contract, because being in this file is
    itself an accepted disposition (see the module docstring). What must be true of a
    row here is not "who will implement it" but:

      1. it names an OWNER in an OPEN workstream (or is explicitly CLOSED/DISCHARGED),
         because environment-gated work still needs someone to run it — V5 sat here for
         days with no owner precisely because nobody checks this; and
      2. it carries a RUNNABLE RECIPE (a fenced code block), because this file's own
         header promises "the exact recipe so whoever has the environment can run it
         without archaeology". A row with no recipe is not verification debt, it is a
         wish.
    """
    text = open(path, errors="ignore").read()
    lines = text.splitlines()
    starts = [(i, m.group(1)) for i, line in enumerate(lines)
              for m in [VD_HEADING.match(line)] if m]
    violations = []
    examined = 0
    for k, (i, vid) in enumerate(starts):
        j = starts[k + 1][0] if k + 1 < len(starts) else len(lines)
        body = "\n".join(lines[i:j])
        title = lines[i].lstrip("# ").strip()
        # An explicitly closed row (DISCHARGED / SIGNED OFF / CLOSED / DELETED) is done.
        if re.search(r"\bDISCHARGED\b|\bSIGNED OFF\b|\bCLOSED\b|\bROW DELETED\b", body):
            if verbose:
                print(f"  OK   {rel}:{i + 1} | {vid} | closed")
            continue
        examined += 1
        owners = {g for mm in OWNER_RE.finditer(body) for g in mm.groups() if g}
        live = owners & open_ws
        dead = owners & closed_ws
        has_recipe = "```" in body
        # 'Phase 3' is an acceptable owner ONLY for beyond-pin upstream work,
        # which cannot be done until the pin advances. The row must prove that
        # about itself by carrying the BEYOND-PIN marker.
        if not live and {"Phase 3", "Phase-3"} & owners and row_is_beyond_pin(body):
            live = {"Phase 3 (BEYOND-PIN)"}
        if not live:
            why = (f"owner is a CLOSED workstream ({','.join(sorted(dead))})"
                   if dead else "NO owner named")
            violations.append((rel, i + 1, title, ",".join(sorted(dead)) or "NONE",
                               f"open verification debt: {why}", (i + 1, lines[i].strip())))
            continue
        if not has_recipe:
            violations.append((rel, i + 1, title, ",".join(sorted(live)),
                               "open verification debt with NO RUNNABLE RECIPE (no fenced "
                               "code block); this file's header promises one",
                               (i + 1, lines[i].strip())))
            continue
        if verbose:
            print(f"  OK   {rel}:{i + 1} | {vid} | {sorted(live)} + recipe")
    return violations, examined


IP_SECTION = re.compile(r"^###\s+~?~?(\d+\.\d+)\b")
IP_FIXROW = re.compile(r"^\s{0,3}(\d+)\.\s+(.*)$")


def check_integration_parity(path, rel, open_ws, closed_ws, verbose):
    """INTEGRATION_PARITY.md also needs its own contract.

    Its shape is different from DEFERRALS.md: numbered `### N.N` divergence sections
    plus a numbered FIX LIST. Nothing in it uses the DEFERRALS marker vocabulary
    ("Target:", "Un-defer trigger:"), so the generic reader found ZERO items and the
    anti-vacuity guard fired — correctly. The rule here is: every `### N.N` section and
    every fix-list row must be DISPOSED (resolved / reclassified / deleted / done) or
    name an owner in an OPEN workstream. That is what caught §2.8's "(Optional)" row,
    which had no owner, no trigger and no schedule, yet read as open work.
    """
    text = open(path, errors="ignore").read()
    lines = text.splitlines()
    # Which lines are inside a SANCTIONED-DIVERGENCE section (permanent by
    # construction, the analogue of DEFERRALS.md's `# Justified R1 Divergences`)?
    sanctioned = []
    in_sanct = False
    for line in lines:
        m = re.match(r"^##\s+\d+\.\s+(.*)$", line)
        if m:
            in_sanct = re.search(r"Sanctioned Divergence", m.group(1), re.I) is not None
        sanctioned.append(in_sanct)

    marks = []
    for i, line in enumerate(lines):
        if sanctioned[i]:
            continue
        if IP_SECTION.match(line):
            marks.append((i, IP_SECTION.match(line).group(1), line.strip()))
        elif IP_FIXROW.match(line) and i > 0:
            m = IP_FIXROW.match(line)
            marks.append((i, f"fix-{m.group(1)}", m.group(2)))
    violations = []
    examined = 0
    for k, (i, ident, title) in enumerate(marks):
        j = marks[k + 1][0] if k + 1 < len(marks) else len(lines)
        body = "\n".join(lines[i:j])
        examined += 1
        if CLOSED_RE.search(body):
            if verbose:
                print(f"  OK   {rel}:{i + 1} | {ident} | disposed")
            continue
        owners = {g for mm in OWNER_RE.finditer(body) for g in mm.groups() if g}
        live = owners & open_ws
        dead = owners & closed_ws
        if live:
            if verbose:
                print(f"  OK   {rel}:{i + 1} | {ident} | {sorted(live)}")
            continue
        why = (f"owner is a CLOSED workstream ({','.join(sorted(dead))})"
               if dead else "no disposition and NO owner")
        violations.append((rel, i + 1, f"{ident} {title}", ",".join(sorted(dead)) or "NONE",
                           f"integration-parity row: {why}", (i + 1, title)))
    return violations, examined


AUDIT_ROW = re.compile(r"^\|\s*\**(O\d+[a-z]?|N\d+)\**\s*\|")


def check_audit_index(path, rel, open_ws, closed_ws, verbose):
    """Check the closure audit's OWN index table (`# PHASE 2 CLOSURE AUDIT (WS19)`).

    WHY THIS READER EXISTS. The generic DEFERRALS reader keys off a marker vocabulary
    ("**OPEN**", "Target:", "Un-defer trigger:"). Several genuinely-open rows are phrased
    differently — "coupled follow-up: `Window` still snapshots the decoration mode at
    init", "Follow-up (open, low)" nested INSIDE a RESOLVED entry — and were therefore
    INVISIBLE to it. That invisibility is not a curiosity: it is the exact structural
    defect the audit itself named for row O9 ("filed as 'Follow-up (open, low)' inside a
    RESOLVED entry — invisible to any grep for open items"). A gate that reported 2
    violations while 6 items were open would be a more sophisticated version of the same
    lie this workstream exists to end.

    So the audit's §B/§D index is checked DIRECTLY, and it is the authoritative list: it
    enumerates every item the audit found (O1-O21, N1-N11) with a disposition. A row is
    satisfied when its disposition is a REAL one — struck, reclassified into the
    divergences, moved to verification debt, routed to an OPEN workstream, or recorded as
    out-of-scope-by-platform. A row still reading "fix in WS19 T<n>" once WS19 is marked
    merged is an ORPHAN by exactly the audit's own definition, and says so.
    """
    text = open(path, errors="ignore").read()
    lines = text.splitlines()
    audit_start = None
    for i, line in enumerate(lines):
        if AUDIT_HEADING.match(line):
            audit_start = i
            break
    if audit_start is None:
        print("  BLOCKED: no `# PHASE 2 CLOSURE AUDIT` section found in DEFERRALS.md.")
        print("  That section is the authoritative index of every Phase-2 item; without")
        print("  it this gate cannot bind. Never a silent pass.")
        sys.exit(3)

    violations = []
    examined = 0
    for i in range(audit_start, len(lines)):
        line = lines[i]
        m = AUDIT_ROW.match(line)
        if not m:
            continue
        ident = m.group(1)
        examined += 1
        # PROPERTY (b): an audit row explicitly marked NOT DONE means WS19 — the
        # workstream whose whole purpose is closing Phase 2 — has an unexecuted task.
        # Without this check, GREEN would mean only "no orphans", and a reader would
        # reasonably but wrongly take it to mean "Phase 2 is closed". Checked BEFORE the
        # disposition match, so annotating a row cannot accidentally suppress it.
        if re.search(r"\bNOT DONE\b", line):
            task = re.search(r"WS19 T\d+", line)
            violations.append((
                rel, i + 1, f"audit row {ident}", "WS19",
                "WS19's OWN task is UNEXECUTED"
                + (f" ({task.group(0)})" if task else "")
                + " — Phase 2 cannot be declared closed while this row stands",
                (i + 1, line[:140]),
            ))
            continue
        # A row is DONE when it records an actual disposition.
        done = re.search(
            r"~~|\bRESOLVED\b|\bRECLASSIFIED\b|\bSTALE\b|\bMOOT\b|\bSHIPPED\b"
            r"|\bDELETED\b|\bSUPERSEDED\b|\bpermanent divergence\b"
            r"|\bJustified R1 Divergence\b|\bverification debt\b"
            r"|\bout-of-scope-by-platform\b|\bOUT-OF-SCOPE\b|✅"
            r"|\broute to WS(?:[1-9][0-9]|[1-9])\b|\bRECORDED\b|\bfixed in-line\b"
            r"|\bre-vet\b|\bpin-sequenced\b|\bWAS WRONG\b",
            line,
            re.I,
        )
        if done:
            if verbose:
                print(f"  OK   {rel}:{i + 1} | audit row {ident} | disposed")
            continue
        owners = {g for mm in OWNER_RE.finditer(line) for g in mm.groups() if g}
        live = owners & open_ws
        dead = owners & closed_ws
        if live:
            if verbose:
                print(f"  OK   {rel}:{i + 1} | audit row {ident} | {sorted(live)}")
            continue
        why = (f"still assigned to a CLOSED workstream ({','.join(sorted(dead))}) with no "
               f"recorded disposition — ORPHANED by the audit's own definition"
               if dead else "no owner and no recorded disposition")
        violations.append((rel, i + 1, f"audit row {ident}", ",".join(sorted(dead)) or "NONE",
                           f"closure-audit index: {why}", (i + 1, line[:140])))
    return violations, examined


def check_file(path, rel, open_ws, closed_ws, verbose):
    """Returns (violations, n_items_examined)."""
    text = open(path, errors="ignore").read()
    flags = section_flags(text)
    entries = entries_for(rel, text)
    violations = []
    examined = 0

    for start, end, title, body in entries:
        in_div, in_audit = flags[start] if start < len(flags) else (False, False)
        # Justified R1 Divergences are permanent by construction, and the closure-audit
        # section is the INDEX of dispositions — it quotes open markers in order to
        # describe them, so treating its rows as items would make the gate assert on its
        # own bookkeeping.
        if in_div or in_audit:
            continue

        # Does this entry make an open claim?
        claim_lines = []
        for off, line in enumerate(body.splitlines()):
            if line.lstrip().startswith(">"):
                # A blockquote is a disposition note added ON TOP of an entry
                # (that is the convention used throughout these ledgers), not a claim.
                continue
            if "~~" in line:
                # A struck line is historical text kept for provenance (this repo's
                # strike-and-annotate convention). It is not a live claim.
                continue
            if any(rx.search(line) for rx in OPEN_MARKERS):
                claim_lines.append((start + off + 1, line.strip()))
        if not claim_lines:
            continue
        examined += 1

        owners = {g for m in OWNER_RE.finditer(body) for g in m.groups() if g}
        disposed = CLOSED_RE.search(body) is not None

        live_owners = owners & open_ws
        dead_owners = owners & closed_ws

        if live_owners:
            if verbose:
                print(f"  OK   {rel}:{start + 1} | {title[:60]} | {sorted(live_owners)}")
            continue
        if disposed:
            # Struck / reclassified / superseded in place: the open marker is historical
            # text kept for provenance (this repo's strike-and-annotate convention), so
            # the entry is not open and its owner is irrelevant. This check MUST come
            # before the closed-owner check: a disposition that mentions a now-closed
            # workstream ("closes the orphaned WS8 item") is a RECORD of the fix, not a
            # new filing into a dead workstream.
            if verbose:
                print(f"  OK   {rel}:{start + 1} | {title[:60]} | disposed in place")
            continue
        if dead_owners and not live_owners:
            violations.append(
                (
                    rel,
                    start + 1,
                    title,
                    ",".join(sorted(dead_owners)),
                    f"owner is a CLOSED workstream ({','.join(sorted(dead_owners))}); "
                    f"the item is ORPHANED",
                    claim_lines[0],
                )
            )
            continue
        violations.append(
            (
                rel,
                start + 1,
                title,
                "NONE",
                "open claim with NO owner and no disposition",
                claim_lines[0],
            )
        )
    return violations, examined


def main():
    argv = sys.argv[1:]
    verbose = "-v" in argv
    argv = [a for a in argv if a != "-v"]

    roadmap_path = os.path.join(ROOT, ROADMAP)
    if not os.path.exists(roadmap_path):
        print(f"PHASE-2 CLOSURE GATE: BLOCKED — ROADMAP not found at {ROADMAP}.")
        print("The OPEN_WORKSTREAMS set is DERIVED from it (never hardcoded), so it")
        print("cannot bind without it. This is not a pass.")
        sys.exit(3)
    open_ws, closed_ws = derive_workstreams(open(roadmap_path, errors="ignore").read())

    if "--check-derivation" in argv:
        print("derived from", ROADMAP)
        print("  OPEN  :", ", ".join(sorted(open_ws)))
        print("  CLOSED:", ", ".join(sorted(closed_ws)))
        sys.exit(0)

    # Anti-vacuity on the derivation itself.
    real_open = {w for w in open_ws if w.startswith("WS")}
    if not real_open or not closed_ws:
        print("PHASE-2 CLOSURE GATE: BLOCKED — the workstream derivation is VACUOUS.")
        print(f"  open(WS only)={sorted(real_open)}  closed={sorted(closed_ws)}")
        print("Both sets must be non-empty: an empty OPEN set would reject every owner,")
        print("and an empty CLOSED set would mean the gate can never catch the exact")
        print("failure it exists for (an item filed into a merged workstream).")
        print(f"Check the plan-authoring checklist in {ROADMAP}.")
        sys.exit(3)

    if "--fixture" in argv:
        i = argv.index("--fixture")
        fixture = argv[i + 1]
        targets = [(fixture, fixture)]
    else:
        targets = [(os.path.join(ROOT, r), r) for r in LEDGERS]

    print("PHASE-2 CLOSURE GATE (G-P2)")
    print(f"  open workstreams : {', '.join(sorted(real_open))} (+ Phase 3)")
    print(f"  closed workstreams: {', '.join(sorted(closed_ws))}")

    all_violations = []
    total_examined = 0
    for path, rel in targets:
        if not os.path.exists(path):
            print(f"  BLOCKED: ledger file missing: {rel}")
            print("  A missing ledger cannot be read as clean — that is the classic")
            print("  vacuous green. Exit 3 (ci.sh renders it as a WARNING, never a pass).")
            sys.exit(3)
        text = open(path, errors="ignore").read()
        if rel.endswith("VERIFICATION_DEBT.md"):
            v, examined = check_verification_debt(path, rel, open_ws, closed_ws, verbose)
        elif rel.endswith("INTEGRATION_PARITY.md"):
            v, examined = check_integration_parity(path, rel, open_ws, closed_ws, verbose)
        else:
            v, examined = check_file(path, rel, open_ws, closed_ws, verbose)
            if rel.endswith("DEFERRALS.md"):
                av, aex = check_audit_index(path, rel, open_ws, closed_ws, verbose)
                v += av
                examined += aex
        # ANTI-VACUITY: a non-trivial file in which we found nothing means the parser
        # stopped matching, not that the file is clean.
        if examined == 0 and len(text.strip()) > 400:
            print(f"  RED (VACUOUS): found ZERO items in {rel}, which is {len(text)} bytes.")
            print("  A parser that matches nothing reports every file as clean. Fix the")
            print("  markers/entry model — do NOT accept this as a pass.")
            sys.exit(1)
        print(f"  {rel}: {examined} open-claim entr{'y' if examined == 1 else 'ies'} examined, "
              f"{len(v)} violation(s)")
        total_examined += examined
        all_violations += v

    if all_violations:
        print()
        print("VIOLATIONS — file:line | item | owner-or-NONE | reason")
        for rel, ln, title, owner, reason, claim in all_violations:
            print(f"  {rel}:{ln} | {title[:70]} | {owner} | {reason}")
            print(f"      claim: {rel}:{claim[0]}: {claim[1][:100]}")
        print()
        print(f"PHASE-2 CLOSURE GATE: RED — {len(all_violations)} open item(s) without an")
        print("owner in an OPEN workstream. Phase 2 is NOT closed.")
        print("Close them, reclassify them (Justified R1 Divergence / verification debt")
        print("with a runnable recipe), or route them to an OPEN workstream IN THAT")
        print("WORKSTREAM'S OWN PLAN. Do NOT relax this gate (R5).")
        sys.exit(1)

    print()
    print(f"PHASE-2 CLOSURE GATE: GREEN — {total_examined} open-claim entries examined; "
          "every one is struck, reclassified, or owned by an OPEN workstream.")


if __name__ == "__main__":
    main()
