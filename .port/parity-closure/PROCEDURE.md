# Per-row closure procedure

This is the binding procedure for every row in a `.port/parity-closure/<area>.md`
checklist. It is a standalone file so that a task brief can name it without the
executor needing to read the whole plan.

The row's full evidence — oracle file:line, rustty file:line, description,
evidence, and a suggested fix — is in `docs/parity-audit/<area>.md` under the
row's finding id. The checklist carries the same locations in short form.

## For each row

1. **Read both sides yourself.** Open the row's entry in
   `docs/parity-audit/<area>.md`, then read the **oracle** region at
   `/Users/gael/projects/ghostty/<oracle path>` and the **rustty** region
   directly. Do not fix from the audit's summary — it is a finding report, not a
   patch, and a finder can misread.
2. **Write the failing test first**, at the smallest scope that observes the
   difference. If the reference has a test for this behaviour, port that test
   verbatim — same name, same assertions — rather than inventing one.
3. **Run it and confirm it fails for the stated reason.** A test that fails for
   a different reason is not yet the right test.
4. **Make the minimal change.**
5. **Run the test, then the crate's suite.**
6. **Negative control:** revert the fix, confirm the test fails, re-apply the
   fix. Do this once per row — an assertion that cannot fail is not a fix.
7. **Record it:** tick the checklist box, set the row's `state` to `CLOSED` in
   `.port/parity-closure/LEDGER.tsv`, and commit naming the finding id
   (e.g. `closes S151-F1`).

## If a row is not a defect

Set `state=WAIVED` and put the justification in the `owner` column (a file:line
or a doc anchor). `G-PARITY` rejects a `WAIVED` row with an empty `owner`, so a
waiver is always accompanied by a written reason. **Never tick a box silently.**

## Ledger mechanics

```bash
# rows still open for an area
awk -F'\t' -v a="src/terminal" '$5==a && $11=="OPEN"' .port/parity-closure/LEDGER.tsv

# close a row
#
# Edit the `state` cell IN PLACE by splitting each line on tab, rather than
# round-tripping through csv.DictReader/DictWriter — the round-trip re-quotes
# any untouched row whose fields contain a literal `"` (its own `csv.writer`
# escaping the quote char), rewriting rows you did not touch even though the
# checker reads them as unchanged. Only the `state` cell of the named row(s)
# changes; every other byte in every other row is preserved.
python3 - <<'EOF'
import io
p=".port/parity-closure/LEDGER.tsv"
CLOSE={"S151-F1"}
out=[]
for i,line in enumerate(io.open(p, newline="")):
    if i==0: out.append(line); continue
    f=line.rstrip("\n").split("\t")
    if f[0] in CLOSE: f[10]="CLOSED"      # state is the 11th column
    out.append("\t".join(f)+"\n")
io.open(p,"w",newline="").writelines(out)
EOF

# verify only the intended row(s) changed
git diff --stat .port/parity-closure/LEDGER.tsv

# verify
python3 .port/tools/gates/check_parity_closure.py
```

## Constraints that bind every row

- **R1 full fidelity** — comments, tests assertion-for-assertion, error paths.
- **R3** — the ghostty oracle at `/Users/gael/projects/ghostty` (pin `20abdb50a`)
  is READ-ONLY. Never `git checkout`/`reset`/`stash`/edit there.
- **R5** — gates are never weakened. A gate change must be net-new coverage with
  a self-test arm, argued in the commit message.
- **Raw-pointer discipline** — `&mut Page/Row/Cell/Screen/PageList` arguments
  re-read across integrity checks are Stacked-Borrows UB here. Use raw pointers
  where the reference uses raw pointers, and re-run
  `.port/tools/miri-run.sh rustty-vt <filter>` after touching
  `rustty-vt/src/{search,pagelist,page}`.
- **DG5** — abort at the C-ABI boundary; follow `rustty-vt-c`'s existing
  `cwrap`/`result.rs` pattern.
- **Naming** — Ghostty→Rustty per token (`.port/tools/PARITY_NAMING.md`); C
  symbols stay `ghostty_*` and are aliased `rustty_*` in headers.
- **Linux rows** — anything under `src/apprt/gtk` compiles only on Linux. Verify
  on the VM: push with
  `git push ssh://rvm/~/projects/rustty +HEAD:refs/heads/p6-incoming`, then
  `ssh rvm 'cd ~/projects/rustty && git reset --hard p6-incoming && <command>'`.
  Pushing to the VM's checked-out branch is refused, hence the side ref.
- **wasm rows** — anything that can change struct layout, Cargo features, or
  linkage must also build for `wasm32-unknown-unknown`, which the macOS and
  Linux lanes both miss. Three artifacts, three separate build graphs:
  `.port/tools/build-vt-wasm.sh`, `.port/tools/build-web-canvas-wasm.sh`,
  `.port/tools/build-main-wasm.sh`. A `log` feature or an `offset_of!` freeze
  that is right on LP64 can be wrong on `wasm32` — both have already happened
  (`DECISION-logging.md` §8r).

## Two gate blind spots (recorded 2026-09-09, from the 14b review)

`G-PARITY` enforces less than this procedure requires. Know the gaps:

1. **A `CLOSED` row with a known unrestored site slips through.** The gate rejects a `WAIVED`
   row with an empty `owner`, but places no constraint on `CLOSED`. So "close it and mention
   the hole in the report" passes the gate while violating this procedure. If a site cannot be
   done, the row is `WAIVED` **with the reason in `owner`** — never `CLOSED`. This is discipline,
   not a check; the gate will not catch you.

   **The one carve-out** (ruled 2026-09-09, from the 14r-b review; see `NEW-FINDINGS.md` N2).
   A row may stay `CLOSED` when the unrestorable site falls **outside the row's own cited oracle
   anchors** — the audit scoped the row deliberately, and a site it never claimed is not that
   row's hole. Three conditions, all required, none of them optional:

   - every one of the row's **cited anchors** is verified live, site by site;
   - the exception is written into the tracked `owner` cell **and** into a PORT NOTE at the
     code site, so neither survives only in a gitignored report;
   - the leftover gets its **own entry in `NEW-FINDINGS.md`**, so the finale re-audit meets it
     as a tracked item rather than as a footnote inside a closed row.

   This is not a softening. The rule exists to stop a hole living only in a report; the carve-out
   demands the hole be recorded in three tracked places instead. A site **inside** the cited
   anchors still forces `WAIVED`, no matter how well documented. If you are unsure which side of
   the line a site falls on, it is inside — `WAIVED` is the safe answer, and understating
   completion costs nothing.

2. **The gate counts rows, not sites.** A row citing twenty sites closes on the same single
   `state` cell as a row citing one. Closing a multi-site row therefore requires checking every
   cited anchor is actually live, not just that the row's *theme* was addressed. The 14b review
   found three rows closed on reports asserting consolidation that had not happened — ~120
   commented sites in `stream.rs` alone. Verify anchors; do not summarise them.
