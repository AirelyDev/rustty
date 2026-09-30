# Oracle sync point

**Reference oracle:** Ghostty, checked out locally at `../ghostty` (read-only reference only —
R3 forbids any build-time dependency on it).

## Pinned baseline

```
commit  12752b2ac1bb05ce53402ed8c853ed1f96eef0b1   (short: 12752b2ac / 12752b2a)
date    2026-09-28
subject Update VOUCHED list (#14445)
```

There is **one** sync point. The oracle working tree is checked out (detached) at this commit, and
it is the SHA referenced by `.port/PORT_RULES.md` (R12 main-parity tracking) and
`.port/cycle-analysis.md`.

The gates read this SHA from **one** checked-in file, `.port/tools/gates/ORACLE_PIN`: a single line
holding the full 40-hex commit, lower-case, with no other content. The shell gates read it through
`rustty_oracle_pin` (`.port/tools/gates/_oracle_root.sh`) and the Python gates through
`_gatelib.oracle_pin()`. G-ORACLE-SH-SELF checks that the two readers agree, that no other tracked
file under `.port/tools` spells the SHA (in full, or as any 7-hex piece of it), and that no other
tracked file in `.port/tools/gates` holds a 40-hex constant. Untracked and ignored files (a stale
`__pycache__/*.pyc`, say) are not scanned: they are not the repository's content. The SHA in the block above is the human record;
`ORACLE_PIN` is what the gates enforce, and the two must always be equal.

- **G-EOL arm 3, G4's staleness guard, G-LOGFILE and G-REFGEOM** require the oracle's HEAD
  to equal `ORACLE_PIN`; G-EOL also requires a clean `test/fuzz-libghostty/corpus`, and
  G-REFGEOM a clean oracle (no modified tracked file). On a mismatch they
  report BLOCKED (exit 3), which ci.sh shows as a WARNING, never GREEN.
- **D10** reads `src/stb/*` at the `ORACLE_PIN` commit (`git show "$PIN:…"`), whatever the oracle's
  HEAD is. It reports BLOCKED only when the oracle does not have that commit, so it does **not**
  flag an `ORACLE_PIN` that was left behind after an advance.
- A missing or malformed `ORACLE_PIN` is BLOCKED, never a pass. G4's `--self-test-oracle` runs a
  copy of G4 against a missing, empty, 39-hex, upper-case and CRLF file to prove it.
- **G-BENCH, G-LZ4 and A5** read `origin/main` and ignore the pin.

Verify with:

```sh
git -C ../ghostty rev-parse HEAD          # 12752b2ac1bb05ce53402ed8c853ed1f96eef0b1
git -C ../ghostty status --porcelain      # empty
```

**This matters mechanically, not just editorially.** `.port/tools/gates/check_comment_coverage.py`
(G3b) and `check_comments_fidelity.py` (G3) read a **directory**, not a SHA: `/Users/gael/projects/ghostty/src`.
Editing this file without moving the working tree changes nothing. Neither gate accepts a
`GHOSTTY_SRC` env override — that override was deleted by `P3-W0 T2 Step 6` because a redirectable
oracle path is the spoof hole `.port/tools/ci.sh:114-129` documents for the test-count gate (point it
at an empty tree and every gate finds zero reference sentences, i.e. vacuous green).

## History of this pin

| advanced | from | to | by |
|---|---|---|---|
| 2026-07-31 | `4749c4e93` (2026-06-20) | `6ad1fe7d8` (2026-07-29) | `P3-W10 T3`, Phase 3 upstream resync |
| 2026-08-17 | `6ad1fe7d8` (2026-07-29) | `12967b68f` (2026-08-17) | Phase 4 upstream resync (advance-first by explicit user direction; before/after gate states in `.port/phase-4/w0/`) |
| 2026-08-25 | `12967b68f` (2026-08-17) | `683d8db64` (2026-08-25) | Phase 5 upstream resync (advance-first, same doctrine; before/after gate states in `.port/phase-5/w0/`) |
| 2026-08-26 | `683d8db64` (2026-08-25) | `88f57ee66` (2026-08-25) | Gap-close drift absorb (advance-first): the 2-commit `stream.zig` ground-state tail — `0f35043c9` (execute C0 0x10-0x1F) + `40a40f848` (ignore UTF-8-decoded C1). See `.port/gap-close/`. |
| 2026-09-01 | `88f57ee66` (2026-08-25) | `20abdb50a` (2026-08-31) | Phase 6 upstream resync (advance-first, same doctrine; before/after gate states in `.port/phase-6/w0/`). 121 commits, 74 non-merge, 75 files, +6,261/−1,269. |
| 2026-09-28 | `20abdb50a` (2026-08-31) | `12752b2ac` (2026-09-28) | Phase 7 upstream resync (`P7-W0 T3`, advance-first, same doctrine; before/after gate states in `.port/phase-7/w0/`). 333 commits, 234 non-merge, 233 files, +35,723/−9,235. |

**Phase-4 note on ordering.** Unlike `P3-W10`, this advance happened at the *start* of the
resync (explicit user direction: update the oracle, then port the whole range). The
tree-reading gates therefore measure against the TARGET during Phase-4 porting:
G1b recorded RED with 10 modules / ~36 dropped tests (`w0/g1b-after-at-12967b68f.txt`) and
G3b's missing counts grew by the range's new reference text (`w0/g3b-after-at-12967b68f.txt`).
Those REDs are the honest progress meter for Phase 4, not regressions; both were GREEN at
`6ad1fe7d8` immediately before the advance (`w0/*-before-at-6ad1fe7d8.txt`). They MUST be
green again at the Phase-4 finale.

**FINALE (2026-08-25): re-greened.** G1b GREEN (every module rust ≥ zig; the 10 RED rows
closed by their owning waves). A5 all arms GREEN (194 oracle exports both directions, 0
residuals). A3 re-pinned GREEN. G-BENCH/G-THEMES/G7/D11 GREEN. G3b finale sweep recorded at
`w0/g3b-finale-at-12967b68f.txt`. The absorbed range's execution record is
`docs/superpowers/plans/2026-08-17-phase4-upstream-resync.md` + the `phase-4 *` commit series.

**PHASE-5 FINALE (2026-08-25): re-greened.** G1b GREEN (295 dropped tests closed by their
waves). A5 all arms GREEN (195 exports, 613 discriminants). A3/A4/D11/D11r GREEN;
menu-shortcut parity 51 actions byte-identical. Both platforms: macOS workspace 5,343 tests
+ xcodebuild RusttyTests 356/0; Linux (`rvm`) full lane 0 FAILED. G3b finale sweep recorded
beside the advance evidence in `.port/phase-5/w0/`.

**PHASE-6 FINALE (2026-09-01): re-greened.** G1b GREEN with the `search` row extended to the
NEW `search/terminal.zig` (search zig 72 / rust 77; vt_c 433 / 487 — the NEW `c/search.zig`
was already globbed). G3b GREEN (0 missing in every crate; arrival was 51). Live per-test audit
(`audit_test_coverage.py`) GREEN: 4,090 live-oracle test blocks, 0 unexplained — the range's
27 tests (c/search 17, search/terminal 6, graphics_image 2, ref_counted_set 1, Terminal 1) are
all present by name. A5 headers GREEN (203 exports both directions, 0 residuals); G-VTFEAT
GREEN with `search` as the 10th oracle feature; symbol manifest 91 == 91; menu-shortcut parity
51 actions byte-identical. macOS xcodebuild RusttyTests **403 passed / 0 failed** (356 + 47
new: KeyEventSynthesize 42, KeyboardShortcutTranslation 1, URLTests 4). Census ledger moved to
`20abdb50a` (G-CENSUS was RED by design after the advance). **Both platforms
exercised:** the Linux (`rvm`) lane ran `cargo test --workspace` GREEN and
G-GTKFEAT GREEN (the GTK gate macOS can only report BLOCKED), with a post-fix
G-REL of 5,477 passed / 0 failed; it also exposed three PRE-EXISTING
Linux-only defects that macOS structurally cannot see — the 25 examples'
`xcrun`/`set -e` abort, a `cfg(not(macos))` clippy break, and a G-REL
false-RED on a passing test's stderr — all fixed and re-verified
(`d38b014b`, `d591a985`; see `.port/phase-6/README.md`
§"Linux-only defects found by this lane"). Execution record:
`docs/superpowers/plans/2026-09-01-phase6-upstream-resync.md` + `.port/phase-6/assessments/W1..W6`
+ the `phase-6/*` merge series. CI-lane results are recorded in `.port/phase-6/README.md`
§"Finale".

**PHASE-7 FINALE (2026-09-30): re-greened at `12752b2ac`.** Every workstream (W0–W11, W12 T0/T1b)
is merged into `phase-7-upstream-resync`. The live per-test audit (`audit_test_coverage.py`) is
GREEN: 4,188 live-oracle test blocks, 0 unexplained. The R5 checklist
(`.port/phase-7/w0/tests-checklist.txt`) has the 112 added upstream tests present by name, the 40
changed bodies re-aligned and the 2 removals dropped. G-FILES (557 files) and G-SYMS (2,890
`pub fn`) have 0 unexplained. G1b is GREEN (the arrival RED on `input_key_encode` is closed).
G3b is GREEN, and `--strict` is GREEN too, for the first time since the advance. G-LOGFILE (with
its re-anchored `LOG-*.tsv`), G-CENSUS (ledger at 4,397 / 17,717 / 357 since P7-W0 T4), A3, A4,
A5, G4 (drivers rebuilt at the pin), G-REFGEOM, G-EOL, G-I18N, G-THEMES, G-UDDOCS, G-PARITY, G7,
G9 (693 / 693) and the whole post-merge set are GREEN.

The phase fixed and documented 21 reference defects (UD352–UD372) and opened 25 port findings
(N435–N465), all closed except the record-only N438. The targeted cross-host checks passed:

- rvm: the GTK-feature workspace clippy, `cargo test -p` for the 29 touched crates, and the live
  GTK KATs;
- rwin: kitty/windows, memory reclaim, xev, and fontconfig GNU with the UD360 child-process test;
- macOS: xcodebuild RusttyTests, 403 passed; menu-shortcut parity, 51 actions.

The full lanes and the benches are P7-W12 T2. Records: `.port/phase-7/README.md` §"Finale table"
and the `phase-7/*` merge series.

The previous pin, `4749c4e93`, was a **stale baseline**: work from beyond it had been ported by
WS5–WS8 and tracked in an ad-hoc "beyond-pin series" table (BL-1…BL-7) that existed only because the
pin had not moved. That table is **gone**. Its contents were either absorbed by the advance (the
series are ancestors of `6ad1fe7d8`) or, where genuinely still open, live as normal ledger rows with
a named owner.

## Advancing the pin

Advancing the pin means `git checkout` in the oracle repo, and that is the one irreversible act in
the port. The standing rule is **never `git checkout` the oracle**: every gate and every concurrent
agent depends on that tree standing still. Advancing therefore requires:

1. **No other lane is active.** Verify with `git worktree list` and a per-worktree `git status`.
2. The oracle tree has **no modified files** (`git -C ../ghostty status --porcelain`).
3. A recorded **before** state for the tree-reading gates — at minimum G3b
   (`check_comment_coverage.py`) and G1b (`check_test_counts.py`).
4. A recorded **after** state and a per-crate delta in **both** directions.
   After the advance also run G-REFGEOM (`.port/tools/gates/check_ref_item_geometry.py`):
   RED means upstream changed the set-item geometry the port's page layout is tied to
   (NEW-FINDINGS N88); update the shadows in `style.rs` / `hyperlink.rs` and the golden
   together.
5. `.port/tools/gates/ORACLE_PIN` (the full 40-hex SHA, one line), `.port/ORACLE_SYNC.md`,
   `.port/PORT_RULES.md` (R12) and `.port/cycle-analysis.md` are updated **together in one
   commit**, and the commit message states which range the advance absorbed. Until `ORACLE_PIN`
   names the new HEAD, G-EOL, G4, G-LOGFILE and G-REFGEOM report BLOCKED on every local run. D10 does not,
   and keeps reading the old pin's `src/stb`. Then:
   - rebuild the oracle's `zig-out/include` and re-run `.port/phase-0/harness/build_ref_driver.sh`
     at the new pin. G4's guard checks only HEAD == `ORACLE_PIN`, and cannot tell that
     `zig-out/include` was built at an older commit;
   - run `bash .port/tools/gates/_oracle_root.sh --self-test` (G-ORACLE-SH-SELF). Its pin arms
     fail if the new SHA, whole or as a 7-hex piece, was copied into another tracked `.port/tools` file,
     or if any 40-hex constant other than `ORACLE_PIN`'s (the old SHA, say) sits in
     `.port/tools/gates`. They look only for the current pin, so the **old** SHA, abbreviated
     anywhere or whole in a `.port/tools` file outside `gates/`, is not caught by them.

**Advancing is bidirectional — do not model it as "the stale reference sentences disappear".**
`P3-W10 T3` measured this. The gates re-read the *same pinned file list* (from the phase-0
`crate_map`) at the new SHA, so an advance simultaneously:

* **removes** reference text upstream deleted in the range (G3b `config` went `missing=30 -> 4`;
  `os` `35 -> 12`; `termio` `23 -> 14`; `bench` `8 -> 5`), and
* **adds** reference text upstream *wrote* in the range, which is missing unless it was ported
  (`vt` `116 -> 218`, `global` `0 -> 31`, `unicode` `0 -> 20`, `cli` `2 -> 11`, `vt-c` `4 -> 10`,
  `lib` `0 -> 5`, `simd` `0 -> 4`).

G3b's total therefore went **218 -> 334**, not to zero. See `.port/phase-3/w10/` for the full
before/after reports, the per-crate delta table, and the per-file attribution of all 334.

## Upstream beyond this pin

At the time of the Phase-7 advance (2026-09-28) the pin IS `origin/main` — there is no
beyond-pin gap. Phase 7 absorbs `20abdb50a..12752b2ac` (333 commits, 234 non-merge, 233 files,
+35,723/−9,235; artifacts in `.port/phase-7/`, delta reviews in `.port/phase-7/delta-review/`);
Phase 6 absorbed `88f57ee66..20abdb50a` (121 commits, 74 non-merge, 75 files,
+6,261/−1,269; artifacts in `.port/phase-6/`); the gap-close absorbed `683d8db64..88f57ee66`
(`.port/gap-close/`); Phase 5 absorbed `12967b68f..683d8db64` (`.port/phase-5/`); Phase 4
absorbed `6ad1fe7d8..12967b68f` (`.port/phase-4/`). Any future upstream movement is the seed
of the next resync.

```sh
git -C ../ghostty fetch origin main          # once
git -C ../ghostty show origin/main:<path>    # current upstream state of a file
git -C ../ghostty show <hash>                # a specific beyond-pin commit
```
