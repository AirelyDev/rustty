#!/usr/bin/env python3
"""G-UDDOCS — the UD <-> docs check (copy of task S-6's checker, gated in ci.sh).

WHY: `.port/parity-closure/UPSTREAM-DEFECTS.md` (the UD ledger) and
`docs/ghostty-bug-fixes.md` (the user-facing log) must stay in lockstep: every
fix-type UD is a row in the docs log, and every id the docs log cites is a
fix-type UD. Task S-6 wrote a one-shot script for this; DECISIONS.md R2 tracks
it here as a standing ci.sh gate instead of a script nobody re-runs.

Checks:
1. Every `### UD` entry (`## Entries` section, template heading and the
   `## Candidates pending verification` section excluded) has EXACTLY ONE
   `- upstream status:` line. Zero or two-or-more is RED.
2. Every fix-type UD id appears in the Record column of
   `docs/ghostty-bug-fixes.md` exactly once, and every id the docs log cites is
   a UD entry of fix type — EXCEPT an entry whose `- upstream status:` line
   starts with `fixed upstream in`: Ghostty has already fixed it upstream, so
   there is no Rustty-vs-Ghostty behavioural difference left to document, and
   the entry is exempt from needing a docs row.
3. A privately-reported UD (`Report PRIVATELY`) never has its trigger recipe
   in the public docs log.

Fix type is derived from each entry, not hand-listed: an entry is a fix when its
`port handling` does not start with a faithful / no-change phrase and its `kind` is
a crash, UB, leak, deadlock, hang or functional/non-crash (evident-intent) defect.
The few entries whose port handling changed code but whose Ghostty behaviour is
invisible or unbuilt are excluded by name below, each with its reason; the script
prints the derived lists so the classification can be reviewed.
"""
import os
import re
import sys

PORT_ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
REPO_ROOT = os.path.dirname(PORT_ROOT)
UD_PATH = os.path.join(PORT_ROOT, "parity-closure", "UPSTREAM-DEFECTS.md")
DOCS_PATH = os.path.join(REPO_ROOT, "docs", "ghostty-bug-fixes.md")

NOT_A_FIX_PREFIX = ('faithful', 'none possible', 'not ported', 'collapsed')
FIX_KIND = re.compile(r'crash|\bub\b|leak|deadlock|hang|functional|non-crash|panic|resource', re.I)
# Entries whose port handling changes code but which have no Ghostty behaviour a
# user can see, so the docs log (a list of user-visible differences) omits them.
EXCLUDED = {
    'UD4': 'treated as an invariant: no verified trigger (the entry says so)',
    'UD26': 'unobservable: the two infos always hold the same value',
    'UD28': 'dead in product: no map has a zero-sized key',
    'UD106': 'unbuilt code: the wasm font tree is not compiled',
    'UD109': 'test code only (kind test-weakness)',
    'UD181': 'unobservable: no production caller compares two loaded maps',
}
# An entry whose upstream status says Ghostty already fixed it needs no docs
# row: there is nothing left for Rustty to differ on (see module docstring #2).
FIXED_UPSTREAM = re.compile(r'^- upstream status:\s*fixed upstream in', re.M | re.I)
# A UD entry whose upstream status says to report it privately (a security
# advisory) must not have its trigger in the public docs log: its row must carry
# the marker phrase, and must hold no recipe (a size, a count of colours, or an
# escape sequence).
MARKER = 'details withheld until reported to Ghostty privately'
RECIPE = re.compile(r'\d[\d.,]*\s*(?:[KMG]i?B|bytes?)\b|\d+\s+(?:specific\s+)?colou?rs|ESC\s*\[|\\x1b|`[^`]*\[[0-9;?]*[A-Za-z]`', re.I)


def parse_entries(ud):
    """Split the '## Entries' section (template + Candidates excluded) into
    {id: block} keyed by UD id, same split points as the original checker."""
    entries_part = ud.split('## Entries', 1)[1].split('## Candidates pending verification', 1)[0]
    blocks = re.split(r'\n(?=### UD\d+ )', entries_part)
    entries = {}
    for b in blocks:
        m = re.match(r'### (UD\d+) ', b)
        if not m:
            continue
        entries[m.group(1)] = b
    return entries


def classify(entries):
    """Return (fix_ids, not_fix) — fix_ids is the ordered list of fix-type UD
    ids; not_fix is [(id, reason)] for everything else."""
    fix, not_fix = [], []
    for i, b in entries.items():
        kind = re.search(r'^- kind:\s*(.*)$', b, re.M).group(1)
        ph = re.search(r'^- port handling:\s*(.*)$', b, re.M).group(1).strip()
        reason = None
        if i in EXCLUDED:
            reason = 'excluded: ' + EXCLUDED[i]
        elif ph.lower().startswith('fixed'):
            pass
        elif ph.lower().startswith(NOT_A_FIX_PREFIX):
            reason = 'port handling: ' + ph.split('.')[0][:60]
        elif not FIX_KIND.search(kind):
            reason = 'kind: ' + kind[:60]
        (not_fix if reason else fix).append((i, reason))
    return [i for i, _ in fix], not_fix


def docs_citations(docs):
    """Return {UD id: citation count} over the docs log's Record column."""
    cited = re.findall(r'\bUD\d+\b', ''.join(
        l.rsplit('|', 2)[-2] for l in docs.splitlines()
        if l.startswith('| ') and not l.startswith('| Area')))
    counts = {}
    for c in cited:
        counts[c] = counts.get(c, 0) + 1
    return counts


def verdict(ud, docs):
    """Return (rc, lines). rc 0 GREEN, 1 RED."""
    out = []
    ok = True

    headings_all = len(re.findall(r'^### UD', ud, re.M))
    status_all = len(re.findall(r'^- upstream status:', ud, re.M))
    entries = parse_entries(ud)
    status_entries = sum(1 for b in entries.values() if re.search(r'^- upstream status:', b, re.M))

    out.append(f'### UD headings (whole file, template included): {headings_all}')
    out.append(f'- upstream status: lines (whole file, template included): {status_all}')
    out.append(f'UD entries: {len(entries)}; entries with an upstream status line: {status_entries}')
    if headings_all != status_all or status_entries != len(entries):
        ok = False
    for i, b in entries.items():
        n = len(re.findall(r'^- upstream status:', b, re.M))
        if n != 1:
            out.append(f'  FAIL {i}: {n} upstream status lines (want exactly 1)')
            ok = False

    fix_ids, not_fix = classify(entries)
    exempt = {i for i, b in entries.items() if FIXED_UPSTREAM.search(b)}

    out.append(f'\nfix-type UD entries ({len(fix_ids)}): {", ".join(fix_ids)}')
    out.append(f'\nnot fix-type ({len(not_fix)}):')
    for i, r in not_fix:
        out.append(f'  {i}: {r}')
    if exempt:
        out.append(f'\nfixed-upstream, exempt from a docs row ({len(exempt)}): {", ".join(sorted(exempt, key=lambda x: int(x[2:])))}')

    counts = docs_citations(docs)
    for i in fix_ids:
        if i in exempt:
            continue
        n = counts.get(i, 0)
        if n != 1:
            out.append(f'FAIL {i}: fix-type, cited {n} times in docs/ghostty-bug-fixes.md')
            ok = False
    for c, n in sorted(counts.items(), key=lambda kv: int(kv[0][2:])):
        if c not in entries:
            out.append(f'FAIL {c}: cited in the docs log but not a UD entry')
            ok = False
        elif c not in fix_ids:
            out.append(f'FAIL {c}: cited in the docs log but not fix-type')
            ok = False

    private = [i for i, b in entries.items()
               if re.search(r'^- upstream status:.*Report PRIVATELY', b, re.M)]
    out.append(f'\nprivately-reported UD entries: {", ".join(private) or "none"}')
    for l in docs.splitlines():
        if not l.startswith('| ') or l.startswith('| Area'):
            continue
        ids = re.findall(r'\bUD\d+\b', l.rsplit('|', 2)[-2])
        for i in ids:
            if i in private:
                if MARKER not in l:
                    out.append(f'FAIL {i}: privately-reported, but its docs row lacks the marker "{MARKER}"')
                    ok = False
                m = RECIPE.search(l)
                if m:
                    out.append(f'FAIL {i}: privately-reported, but its docs row describes the trigger ({m.group(0)!r})')
                    ok = False

    rows = sum(1 for l in docs.splitlines() if l.startswith('| ') and not l.startswith('| Area'))
    out.append(f'\ndocs rows: {rows}; UD ids cited: {len(counts)}')
    out.append('CHECK ' + ('PASS' if ok else 'FAIL'))
    return (0 if ok else 1), out


def _entry(id_, kind, port_handling, upstream_status, extra=''):
    return (f'### {id_} — synthetic\n'
            f'- kind:            {kind}\n'
            f'- port handling:   {port_handling}\n'
            f'{extra}'
            f'- upstream status: {upstream_status}\n')


def _ud_doc(entries_md):
    return ('# Upstream defects\n\n## Entries\n\n' + entries_md +
            '\n## Candidates pending verification\n\nnone\n')


def self_test():
    print("=== G-UDDOCS SELF-TEST ===")
    ok = True

    # Case 1: a UD with no status line -> RED.
    ud1 = _ud_doc(
        f'### UD1 — synthetic\n- kind:            crash\n- port handling:   fixed: safe handling\n')
    docs1 = ('# docs\n\n| Area | Ghostty | Rustty | Record |\n'
             '| ---- | ------- | ------ | ------ |\n'
             '| x | crashes | does not | UD1 |\n')
    rc1, lines1 = verdict(ud1, docs1)
    case1_ok = rc1 == 1 and any('upstream status lines' in l for l in lines1)
    print(f'  case 1 (no status line)      -> rc={rc1} (expect 1) {"ok" if case1_ok else "FAIL"}')
    ok = ok and case1_ok

    # Case 2: a UD with two status lines -> RED.
    ud2 = _ud_doc(
        '### UD1 — synthetic\n- kind:            crash\n- port handling:   fixed: safe handling\n'
        '- upstream status: not reported\n- upstream status: not reported\n')
    rc2, lines2 = verdict(ud2, docs1)
    case2_ok = rc2 == 1 and any('upstream status lines' in l for l in lines2)
    print(f'  case 2 (two status lines)    -> rc={rc2} (expect 1) {"ok" if case2_ok else "FAIL"}')
    ok = ok and case2_ok

    # Case 3: a UD without a docs row and not fixed upstream -> RED.
    ud3 = _ud_doc(_entry('UD1', 'crash', 'fixed: safe handling', 'not reported'))
    docs3 = '# docs\n\n| Area | Ghostty | Rustty | Record |\n| ---- | ------- | ------ | ------ |\n'
    rc3, lines3 = verdict(ud3, docs3)
    case3_ok = rc3 == 1 and any('cited 0 times' in l for l in lines3)
    print(f'  case 3 (fix-type, no docs row, not fixed upstream) -> rc={rc3} (expect 1) {"ok" if case3_ok else "FAIL"}')
    ok = ok and case3_ok

    # Case 4: a fixed-upstream UD without a docs row -> GREEN (exemption).
    ud4 = _ud_doc(_entry('UD1', 'crash', 'fixed: safe handling', 'fixed upstream in 1234567890'))
    rc4, lines4 = verdict(ud4, docs3)
    case4_ok = rc4 == 0
    print(f'  case 4 (fixed upstream, no docs row) -> rc={rc4} (expect 0) {"ok" if case4_ok else "FAIL"}')
    ok = ok and case4_ok

    # Negative control on the case-4 exemption itself: strip the exemption text
    # (so the status line no longer starts with "fixed upstream in") and confirm
    # the SAME entry, still with no docs row, turns RED again.
    ud4_no_exemption = _ud_doc(_entry('UD1', 'crash', 'fixed: safe handling', 'not reported'))
    rc4b, _ = verdict(ud4_no_exemption, docs3)
    case4b_ok = rc4b == 1
    print(f'  case 4 control (exemption removed)   -> rc={rc4b} (expect 1) {"ok" if case4b_ok else "FAIL"}')
    ok = ok and case4b_ok

    print("SELF-TEST GREEN: the gate flips RED on a missing/duplicated status line and on an "
          "undocumented fix, stays GREEN when a fixed-upstream entry has no docs row, and flips "
          "back RED when that exemption text is removed." if ok else "SELF-TEST RED: the gate did not flip as expected.")
    return 0 if ok else 1


if __name__ == "__main__":
    if "--self-test" in sys.argv:
        sys.exit(self_test())
    if not os.path.exists(UD_PATH):
        print(f"BLOCKED: UD ledger not found at {UD_PATH}", file=sys.stderr)
        sys.exit(3)
    if not os.path.exists(DOCS_PATH):
        print(f"BLOCKED: docs log not found at {DOCS_PATH}", file=sys.stderr)
        sys.exit(3)
    ud_text = open(UD_PATH).read()
    docs_text = open(DOCS_PATH).read()
    rc, lines = verdict(ud_text, docs_text)
    print("\n".join(lines))
    sys.exit(rc)
