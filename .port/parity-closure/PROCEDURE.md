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
