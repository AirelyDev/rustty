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

## Concurrency: one implementer at a time (ruled 2026-09-10)

Three times on this plan, one worker's commit carried another worker's changes.
Diagnosed properly, the cause is not careless staging:

- `git add <path>` adds to the INDEX; a bare `git commit` then commits the WHOLE
  index, including anything another worker staged earlier. Putting the paths on the
  commit (`git commit <paths> -m ...`) fixes that half.
- It does not fix the other half. **Two workers editing the SAME FILE in one working
  tree contaminate each other no matter how the commit is scoped**, because a commit
  takes the file's whole content. And this plan's workers all edit the same three
  files: `LEDGER.tsv`, `ADJUDICATION.md`, and `NEW-FINDINGS.md`. Two of them also
  independently claimed the same new finding id.

**So: run ONE implementer at a time on this plan.** Reviewers are read-only and may
run in parallel with anything. The shared ledger has to serialize regardless, so
parallel implementers do not buy throughput — they move the conflict out of git,
where it is loud, and into the documents, where it is silent.

If parallel implementation is ever genuinely needed, give each worker its own git
worktree and merge, rather than sharing one tree.

## The Linux VM: two corrections learned the hard way (2026-09-10)

- **`cargo clean` on the VM destroys the long-running Miri sweep.** Its `--target-dir` is
  `~/projects/rustty/target/miri` — **inside** the directory `cargo clean` removes. This
  controller told two workers the sweep was safe from cleaning because it "uses a separate
  build directory"; that is true only in the `--target-dir` sense and is **wrong** about
  `cargo clean`. If the VM runs out of space, remove **only** `target/debug/incremental`
  (large, regenerable, on no Miri path) and confirm the sweep with `pgrep -c miri` before
  and after.
- **`--features gtk` is not enough.** Use `--features gtk,x11,wayland`. A real defect — a
  `LOG_SCOPE` inserted between a `#[derive(Debug)]` and its enum, killing the `Debug` impl
  five restored sites depend on — was caught **only** by `cargo build --workspace`, because
  the narrower feature set never compiles the affected file. `cargo fmt --check` stayed
  green through all five rounds of breakage: it parses, it does not type-check.

## Testing: an absence needs a positive assertion (2026-09-10)

A test asserting that something is **not** emitted must also assert, positively, that it
reached the code the absence is about. Found when a negative control did not fire: the
assertion was vacuous for two stacked reasons — a dispatch thunk returned early on an unbound
field, and the binding step **snapshotted** that field into a closure, so binding it
afterwards changed nothing. The existing counter could not catch either, because it was
bumped one frame above the guard. The fix added a counter bumped **inside** the thunk and
asserted it advances by one.

On this plan a "negative control" has three times turned out not to exercise the thing it
claimed. Prove the control fails, then prove it fails *for the stated reason*.

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

## Gate blind spots (1-3 recorded 2026-09-09 from the 14b review; 4 added 2026-09-10)

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

3. **The ledger's cited anchors are approximate — enumerate the oracle yourself.** Found
   2026-09-10, while closing the font-shaper rows. Three defects, all in the audit's own
   citations, all verified against the pin:

   - **Line drift.** `S84-F1` cites `CodepointResolver.zig:138-140, 176, 195-199, 218,
     265-268, 290-293`; the pin's actual calls are at `136, 171, 186, 203, 217, 261, 290`.
     A consistent 6-9 line drift, and the same drift runs through all thirteen of
     `Collection.zig`'s anchors. The audit was written against a slightly different tree.
   - **Under-count.** That same row lists **six** anchors for **seven** real calls — the
     `found codepoint 0x{X} in fallback face={s}` site at `:203` belongs to no row at all.
     `S91-F7` under-counts the same way: four anchors, five live calls in
     `face/web_canvas.zig`, with `:317` unclaimed.
   - **Comment counts are not site counts.** A grep for commented `log.` lines counts both
     the multi-line block comment reproducing a reference call AND its one-line stub, so it
     reports roughly double. Three tasks were briefed with such a count; all three had to
     correct it, and one would have emitted ~15 duplicate log lines had it obeyed.

   **So: the anchors locate a region, they do not enumerate the work.** For any row citing a
   range, enumerate the ORACLE's own calls in that region and confirm each has a live
   counterpart. Expect sites the port dropped leaving no comment; expect a reference call
   inlined into a different Rust function than the file layout implies. Report the inventory
   as oracle-sites-found versus live-sites-after, per file, and explain every difference. A
   site you find that no row claims still gets restored — say so under its own heading.

2. **The gate counts rows, not sites.** A row citing twenty sites closes on the same single
   `state` cell as a row citing one. Closing a multi-site row therefore requires checking every
   cited anchor is actually live, not just that the row's *theme* was addressed. The 14b review
   found three rows closed on reports asserting consolidation that had not happened — ~120
   commented sites in `stream.rs` alone. Verify anchors; do not summarise them.

4. **A reference file's HEAD can be unaudited, and no row will ever claim it — now
   machine-checked.** Recorded 2026-09-10 (`NEW-FINDINGS.md` N12). Every row cites a RANGE
   of reference line numbers; anything outside every row's range is outside the audit.
   Several large reference files have their lowest cited anchor partway down: `Surface.zig`
   had 38 live `log.*` sites above line 2193, `gtk/class/application.zig` 20 above 812,
   `Config.zig` 14 above 4680 — **101 unclaimed head sites over 14 files**. N4, N6, N7 and
   N9 are all instances of this class found the hard way, one file at a time.

   Unlike blind spots 1-3 this one is caught by machine.
   **`.port/tools/gates/check_log_reconcile.py` (`G-LOGFILE`, HARD in `ci.sh`)** reconciles
   per ORACLE FILE, not per row: the reference's live `log.*` call sites against live port
   emissions in the Rust file(s) declared to port it, per platform arm. Because it counts
   the whole file, a row's anchors are irrelevant to it and an unaudited head cannot hide.
   `--report` prints the full per-file deficit table.

   **What it still needs from you, because no gate can supply it:**

   - **The mapping is DECLARED, in `.port/parity-closure/LOG-PORTMAP.tsv`, never guessed.**
     Filename inference gets at least five entries wrong — `global.zig` ports to
     `rustty-apprt/src/lib.rs`, `renderer/Thread.zig` to `rustty-core/src/renderer_seam.rs`
     (a different crate), `OpenGL.zig`'s head to `opengl/gl/sys.rs`, `main_ghostty.zig` to
     `rustty-core/src/logging.rs`, and both renderer twin pairs split two ways. **Adding a
     LOG row for a reference file with no portmap entry turns the gate RED**; add the entry
     in the same commit, and take that as the prompt to say where the file really ports to.
   - **Platform twins are separate arms (`|`), never summed.** A file's score is the
     MINIMUM over arms, because only one arm compiles on a given target. Summing them is
     precisely how N9 hid: a fully restored macOS twin covering a Linux twin that emitted
     almost none of it, with three rows reading as done.
   - **The allowances are a RATCHET.** An allowance LARGER than the measured deficit is
     RED, so restoring a site must tighten the number in the same commit; slack cannot be
     left behind for a future regression to hide in. A non-zero allowance with no `reason`
     is RED, the same rule G-PARITY applies to a `WAIVED` row's `owner`.
   - **It counts, it does not attribute.** A green means "no diagnostic mass was lost from
     this file", not "every individual site is live". Where two reference files port into
     one Rust file the arm is credited with that file's whole count; the portmap's `reason`
     column records every such overlap. Blind spot 3 still applies inside a row.
   - **A gate seeded from the audit inherits the audit's blind spots.** `LOG-PORTMAP.tsv`
     was built from the reference files that `LOG`-class rows cite, so `G-LOGFILE` could
     only ever see what the audit had already looked at: 112 reference files holding 523
     live diagnostics — 37% of the reference's total, including its single largest
     log-bearing file — were outside the gate, and nothing in its output distinguished
     "reconciles" from "never looked at". Fixed 2026-09-10 (N15): the gate enumerates the
     reference tree itself and goes RED on any log-bearing file that is neither declared nor
     EXCLUDED-with-a-category. **Whenever you build a gate to catch an audit's blind spots,
     derive its universe from the reference, never from the audit.**
   - **A count check cannot see a message that is PRESENT but WRONG**, and a count check is
     what most gates are. One funnel emitted half of `"error handling VT action action={}
     err={}"` and reconciled perfectly; the stage that found it then reintroduced the class
     four times in its own fix commit, dropping the space before a placeholder. `G-LOGFILE`
     now compares each emission's text against the reference call preserved as a comment
     above it (N21), with placeholder CONTENTS normalised and spacing left alone, and
     declares exceptions in `LOG-TEXT-DIVERGENCES.tsv`. **It reaches 72% of emissions** — it
     can only compare what the port wrote down, so preserving the reference call at a site
     is what makes that site checkable. The same scan detects the inverse (N20): a preserved
     comment quoting a call the reference does NOT have, which a future stage would
     faithfully restore.
   - **`LEDGER.tsv`'s rustty-side line anchors have drifted, and are NOT being refreshed.**
     Up to 70 lines on rows closed earlier in this plan, because restoration moves code.
     Ruled 2026-09-11: **leave them and treat them as historical citations, not as
     locations.** Three reasons. They are the AUDIT's record of where it looked, and
     rewriting them rewrites that record. A mechanical refresh needs a matching heuristic,
     and every heuristic in this plan that was trusted without a mutation test turned out to
     be wrong. And the authority on where a diagnostic lives now is the gate, which is
     regenerated from the reference on every run, not a frozen anchor. **Treat a ledger
     rustty anchor as "this region, at the time of the audit"; locate the code by symbol.**
   - **Ratchet EVERY number a stage can move, including the escape hatch.** Completing the
     portmap (N15) created a new lever: an EXCLUSION retires a reference file instead of
     reconciling it, so it erases that file's whole deficit. Reclassifying
     `src/termio/Exec.zig` as `no-port:` wiped 58 sites and left the gate green, because
     allowances and bypass caps were ratcheted and the exclusion list was not. It now has a
     frozen `#!exclusion-cap` budget on the same two-way ratchet, and the gate BLOCKS if the
     directive is missing. When you add a category to let people record an honest exception,
     cap the category in the same commit — otherwise the exception becomes the exit.
   - **A count ratchet needs a SHAPE ratchet beside it.** `G-LOGFILE` counts a
     `eprintln!("warn(gtk): …")` as an emission, because the reference's diagnostic does
     reach a human — so without a second constraint a stage could close a count deficit by
     adding MORE facade-bypassing direct prints and the gate would call it an improvement.
     `LOG-PORTMAP.tsv`'s `bypass_cap` column is that constraint (finding N14). Whenever a
     gate counts two things that are not equally good, cap the worse one separately.
   - **Count through the instrument, not beside it.** N14 was first filed with numbers from
     a line-oriented `grep` — 199, where the gate's own regex says 244 — because a
     line-oriented grep cannot see a multi-line `eprintln!(`. A finding written to document
     a measurement problem was itself measured informally. If a gate can produce the number,
     the finding quotes the gate.
   - **Validate the scanner against the live tree, not only against fixtures.** The gate's
     own Rust stripper shipped without a char-literal branch: one `b'"'` in
     `crates/rustty-vt/src/stream.rs` opened a phantom string, blanked ~1200 lines below it,
     hid 59 live emissions and froze a 49-site deficit that did not exist — while all 16
     synthetic self-test arms stayed green, because a desync depends on real file content.
     `--self-test` now carries a LIVE-TREE arm that checks the scanner against a naive grep
     over every mapped port file, plus a LITERAL-SHAPE arm pinning every Rust literal that
     can desync it (char, byte, raw, raw-byte, lifetime, doc-comment apostrophe). Two of
     those are regression pins for bugs that shipped: `b'"'` and `br"…\\"`. Keep both arms,
     and add them for any new scanner.
   - **Four emission shapes exist in this port** and the gate knows all four:
     `log::warn!`, a `macro_rules! log_warn!` facade (`opengl/gl.rs`), a `fn log_warn(msg)`
     facade (`config_struct.rs`, most of `rustty-os`), and `eprintln!("warn(scope): …")`
     reproducing Zig's default `logFn` output inline (the whole GTK tree). If you add a
     fifth, teach `discover_emitters` about it — a shape the gate cannot see reads as a
     dropped diagnostic, and a shape it sees wrongly reads as a false green. A `log_*`
     helper with an EMPTY body is a dropped diagnostic, not a facade, and the gate lists
     those under its `NOTE:` line.
