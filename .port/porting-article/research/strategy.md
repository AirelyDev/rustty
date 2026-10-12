# Strategy dossier: how the port was done and how the method changed

Area: **strategy**. Scope: the porting method, verification stack, orchestration, and what
changed after Bun published "Bun in Rust" (2026-07-08). Repo `/Users/gael/projects/rustty`
at `1d22ba80d` (branch `phase-8-upstream-resync`), measured 2026-10-08.

Provenance tags: **MEASURED** (computed here; command given), **DOCUMENTED** (read from a repo
doc; path given), **ESTIMATED** (heuristic; method given). Scratch outputs are in
`research/tmp/`; chart data in `research/data/strategy-*.csv`.

---

## 0. Headline facts

| # | Fact | Value | Prov. | Evidence |
|---|---|---|---|---|
| 1 | Bun's "Bun in Rust" post date | 2026-07-08 (Jarred Sumner) | DOCUMENTED | https://bun.com/blog/bun-in-rust (fetched) |
| 2 | Day Rustty researched the Bun port and adopted its ideas | 2026-07-10, day 20 of the port (2 days after the post) | MEASURED | `git log --diff-filter=A -- .port/phase-2/BUN_PORT_LEARNINGS.md` → `2c03357ce`; rules ratified `ccd14200e` |
| 3 | Bun-derived binding rules | 6 (R7–R12), ratified the same day | DOCUMENTED | `.port/PORT_RULES.md` §"R7–R12 — Bun-port adoptions" |
| 4 | GHOSTTY_LIFETIMES.tsv rows | 216 at creation (2026-07-10), 315 now | MEASURED | `git show <c>:.port/audits/GHOSTTY_LIFETIMES.tsv \| grep -v '^#' \| grep -vc '^file'` per commit |
| 5 | Gate registry entries in `ci.sh` | 11 (2026-06-21) → 12 (07-10) → 101 (10-08); 75 HARD, 26 PROGRESS | MEASURED | `grep -cE '^\s*"[A-Za-z0-9_.-]+\|(HARD\|PROGRESS)\|' ci.sh` at dated commits |
| 6 | Gate scripts / of which self-testing | 12/10 → 14/10 (07-09) → 90/57 now | MEASURED | `git ls-tree` of `.port/tools/gates` + `grep -q 'self-test\|self_test'` |
| 7 | Dedicated `-SELF` registry entries (gate tests itself in CI) | 0 before 07-31 → 24 now | MEASURED | same grep with `-SELF\|` |
| 8 | Workflow-tool scripts in repo | 212 (`wf-*.js`; 191 in phase-2, 11 phase-4, 9 phase-5 + 1 untracked), 15,576 lines, 955 agent call sites | MEASURED | `glob .port/phase-*/wf-*.js` (python, `data/strategy-workflow-scripts.csv`) |
| 9 | Bun-class semantic-mismatch detectors | 8 (D1–D6, D8, D9), wired 2026-07-12 | MEASURED/DOCUMENTED | `94ca0243e`; `.port/tools/CI.md` §"Bun semantic-mismatch detectors" |
| 10 | Upstream commits absorbed by pin advances | 1,718 commits (1,157 non-merge) over 7 advances | MEASURED | `git -C ghostty rev-list --count <from>..<to>` for each row of `.port/ORACLE_SYNC.md` |
| 11 | Ghostty defects recorded (R13) | 66 entries on day one (2026-09-11), 205 headings now (UD ids to UD378) | MEASURED | `grep -cE '^#+ *UD[0-9]+' UPSTREAM-DEFECTS.md` per commit |
| 12 | Parity-audit ledger closure | 497 findings: 404 CLOSED, 93 WAIVED, 0 OPEN | DOCUMENTED | `.port/parity-closure/README.md` |
| 13 | Codex audit vetted by Claude | 659 registry rows → 147 groups vetted: 144 confirmed, 3 refuted, 140 remediated | DOCUMENTED | commit `8120dd10d` message; `.port/codex-vet/README.md` |
| 14 | Merge commits | 0 before 2026-07-16; 266 total (worktree branch-and-merge model) | MEASURED | `git log --merges` |
| 15 | Share of Claude API calls made by subagents (recovered window) | 88.6% in Sep, 82.8% in Oct | MEASURED | `~/projects/rustty-stats/data/messages.csv.gz`, `is_subagent` |
| 16 | Distinct subagents / worktrees, Sep 2026 | 1,142 agents, 214 worktrees, 1 Workflow run | MEASURED | same file, distinct `agent_id`, `worktree`, `workflow_id` |
| 17 | Isolated Miri proof harnesses (`mirihar-*`) | 27 (13 Jul, 12 Sep, 2 Oct) | MEASURED | `ls -d .port/phase-2/mirihar-*` + first-add dates |
| 18 | MIRI_EXCEPTIONS table rows | 462 | MEASURED | `grep -cE '^\|' .port/phase-2/MIRI_EXCEPTIONS.md` |

---

## 1. The starting method (2026-06-21 → 2026-07-09, "pre-Bun")

**Constitution.** The kickoff commit `15c1a9bd7` (2026-06-21) brings `PORT_RULES.md` R1–R5
and `.port/phase-0/plan/DEFINITION_OF_PARITY.md` (gates G1–G9) (DOCUMENTED):

- R1 100% fidelity: every line, every comment, every `test {}` block assertion for assertion.
  The phase-0 inventory count is a contract (G1 contract: 3,234 test blocks; G2 floor 11,975
  assertions; G3 comment census 42,069 lines; G9 677 surface ids — `.port/tools/CI.md` table).
- R2 identity rename with an `identity-oracle` feature so the differential harness compares
  byte-for-byte.
- R3 self-contained: Ghostty is an **external oracle** (separate process / goldens), never a
  Cargo dependency. Same C libraries via `-sys` crates.
- R4 acyclic crate graph enforced by `cycle_check.py` / G7.
- R5 gates are invariant: never weaken, skip or xfail; a gate that cannot be made green
  honestly stops the work.

**Gates from day one.** `5540e9bae` "phase-1 parity-gate infrastructure + self-test": 12
gate scripts, 10 of them with a `--self-test` arm (MEASURED). The convention that a gate must
prove it can go RED (inject a cycle, corrupt one byte of a stand-in port, flip a crate to
`done` with zero tests) is present from the first day — `.port/tools/CI.md` §"Why this is
honest". Exit-code contract: 0 pass, 3 BLOCKED (surfaced as a warning with its reason, never a
silent green), else RED.

**Memory safety (R6).** Same day, `d9fd637ce`: Miri becomes the primary UB/leak gate (G6),
replacing a custom tracking allocator chosen only because Miri appeared uninstalled. FFI crates
go to ASan; leak detection to LSan/valgrind. `161178d6a` (06-23) adds the sanitizer matrix
(ASan/LSan/MSan/TSan/HWASan); `fd9a3c8a2` documents MIRI_EXCEPTIONS with a 4-criterion routing
protocol (compensating coverage, never a skip).

**Raw-pointer doctrine, learned from Miri.** `3ee9fd06d` (06-24): Miri caught Stacked-Borrows UB
in `Screen::clear_cells`. Cause: Zig `*Page`/`*Row`/`[]Cell` parameters are unprotected; the
port's `&mut` arguments are "strongly protected" for the call, so re-reading the same memory
through another path (e.g. `assert_integrity`) is UB. Rule: port those as raw pointers and take
a short-lived `&mut` only for the mutation (memory `rust-mut-args-strongly-protected`).
`e9e325358` (06-25) fixes a second instance in vt-c. From 07-07 (`f046d41a6`) such seams are
proven on **isolated non-FFI Miri harnesses** (`.port/phase-2/mirihar-*`, 27 today) rather than
argued by hand (memory `shaper-hook-sb-clean-interleave`).

**Placeholder-leaf doctrine** (07-02, `67579b58c`, `c6041368b`): for subsystems not yet ported
(GPU renderer, xev event loop, ImGui inspector) the port defines *inhabited* placeholder structs
whose methods are `unimplemented!()`. Callers port as live compiling Rust; Surface.zig (6,036
lines) and App.zig ported ~95% live (memory `placeholder-leaf-doctrine`). A dated endgame audit
greps every `unimplemented!()`/`todo!()`. Bun's "ZERO todo!()" rule was later weighed and
**adapted**, not adopted (`BUN_PORT_LEARNINGS.md` §3.1).

**Orchestration: Workflow scripts.** Each milestone ran as a Claude Code Workflow script
(`design → adversary → port → verify`). 156 of the 212 scripts were first committed before
2026-07-10 (61 in one collection commit `f7411131f` on 07-03 after scratch dirs proved
ephemeral — so commit dates are upper bounds on when those ran). Pattern shares in that era
(ESTIMATED by regex over script text, `data/strategy-workflow-patterns-by-era.csv`): 96% emit
JSON-schema-validated results; 74% invoke Miri; 62% loop/retry until green; 49% contain an
adversarial refute stage; 33% a design/blueprint doc; 24% multi-voter panels. Blueprints were
"adversarially reviewed, ACCEPT-WITH-FIXES folded in" (`8c3bef888`, `bee8be0fc`, `3dd1fe9d8`,
`8484c007e`, `41278a493`). Representative shape, `wf-alloc-stack-audit` (`.port/phase-2/`): a
parallel Map stage of one agent per component, a Refute stage where each significant finding is
attacked by a skeptic told "default to REFUTED/OVERSTATED unless the code clearly proves it",
then a Synthesize stage.

Pre-Bun commit cadence: 452 commits in 19 days (23.8/day), linear history, no merges
(MEASURED, `data/strategy-period-commit-metrics.csv`).

---

## 2. What changed after "Bun in Rust" (2026-07-10 onward)

### 2.1 The adoption event

Two days after the post, `wf-bun-port-learnings.js` ran a research sweep over Bun PR #30412,
its deleted internal docs (`docs/PORTING.md`, `LIFETIMES.tsv`, `CYCLEBREAK.md`,
`rust-rewrite-plan.md`), its committed `.claude/workflows/*.workflow.js`, five post-release
regression post-mortems, the 548-comment HN thread and the Bun unsafe audit
(`BUN_PORT_LEARNINGS.md` §2, DOCUMENTED). Every learning is tagged ADOPT / ADAPT / COVERED /
REJECT against R1–R6. The same day (all `2026-07-10`, MEASURED via git):

| Commit | Change |
|---|---|
| `2c03357ce` | BUN_PORT_LEARNINGS.md + validation sweep #1 report + realignment plan |
| `ccd14200e` | PORT_RULES R7–R12 ratified (all six) + pre-port tier checklist |
| `8f6d781dc` | `GHOSTTY_LIFETIMES.tsv` retroactive ledger |
| `b4845f427` | LIFETIMES audit of ported code: 20 findings (0 high, 2 medium filed, 18 low) |
| `d8ee18a75` | unsafe census `count-unsafe.sh` + `UNSOUND_SAFE.md` |
| `3f5a226f0` | R10 marker taxonomy migration (`TODO(port)`, `PERF(port)`, `PORT NOTE`) |
| `10ade21c8` | `wf-realign-phase3.js` — the workflow that executed the above |

The user's locked decision (memory `adopt-lifetimes-tsv`, dated 2026-07-10): adopt LIFETIMES
both **retroactively** (raw-pointer structs already ported) and **prospectively** (no
pointer-heavy tier — GTK ~23k LOC, Swift glue, inspector, platform — ports before its rows land
with 2-reviewer consensus). Rationale recorded there: Rustty had already hit SB/TB UB twice from
raw-pointer aliasing, the exact class a field-level ownership ledger front-loads.

### 2.2 The new rules (R7–R12), each tied to a Bun source

Full mapping in `data/strategy-bun-adoptions.csv` (18 rows). Summary:

- **R7 replacement-primitive fidelity** — Bun's integer/assert map: `@intCast` →
  `T::try_from(x).unwrap()`, bare `as` only for `@truncate`; `+%` → `wrapping_*`; `for (a, b)`
  ports with `debug_assert_eq!` on lengths because `.zip()` truncates; `unwrap_unchecked`
  banned until proven on a Miri harness.
- **R8 comptime and build-switch preservation** — Zig `comptime` → generic/const/macro; a
  runtime port is a flagged deviation; build switches port as `cfg`, never folded.
- **R9 unsafe accounting** — one committed `rg` command as ground truth; `SAFETY(origin/dest)`
  tags; separate ledger of unsound *safe* wrappers; per-crate density monotone non-increasing.
- **R10 markers** — machine-greppable `TODO(port)` / `PERF(port)` / `PORT NOTE`.
- **R11 adversarial verification** — implementer, verifier and fixer are separate agents;
  verifier defaults to not-ok; bulk classifications need "file:line — what you saw"; low
  confidence claims plus a 10–15% sample of confident ones get 2–3 default-refute votes.
- **R12 upstream parity** — a standing main-parity ritual; "a merge does NOT port the fix;
  only an explicit .rs change does"; port the upstream commit's test hunks too.

### 2.3 New detectors (2026-07-12)

`94ca0243e` wires D1–D9 into `ci.sh`, one per Bun regression class that had escaped all of
Bun's gates (DOCUMENTED, `.port/tools/CI.md`): D1 slice reinterpret truncate-vs-panic (#31188),
D2 capacity off-by-one (#31503), D3 constants ported as folded literals, D4 comptime→runtime
escape corruption (#30693) via golden VT bytes, D5 paired invariants (#30678), D6 sanitizer
blinded by allocator switch (#30875) via a canary that must fail, D8 clone injection, D9 stale
comments. They entered as PROGRESS gates (fail-closed against hand-kept ledgers, but a RED
still fails CI) with self-tests.

### 2.4 Immediate payoff

- 2026-07-11 `3277f2f78`: first full `rustty-vt` Miri sweep (2,037 tests) finds a real
  Stacked-Borrows UB cluster in the search subsystem (9 tests); fixed in `9e3fe1c25` by restoring
  raw `*mut Screen` semantics; 74 Miri-intractable tests routed as 4-criterion exceptions
  (`90fa4ff71`).
- 2026-07-12 `dcae8d61e`: latent SB/TB UB in the xev MPSC completion queue, found by the
  LIFETIMES audit (M1+M2).
- 2026-07-10 `c4758b431`: the widened G6 lane finds vt-c row-iterator provenance, a Handler built
  from an autoref temporary, and kitty storage re-derive issues.

### 2.5 Before vs after, measured

| Metric | Pre-Bun (06-21..07-09) | Post-Bun (07-10..09-10) | Post-R13 (09-11..10-08) | Source |
|---|---|---|---|---|
| Days | 19 | 63 | 28 | — |
| Commits (commits/day) | 452 (23.8) | 1,257 (20.0) | 1,259 (45.0) | `strategy-period-commit-metrics.csv` |
| Merge commits | 0 | 108 | 109 | same |
| Gate registry entries at period end | 11 | 69 | 101 | `strategy-gate-registry-growth.csv` |
| Gate scripts added (excl. helpers) | 16 | 68 | 14 | `strategy-gate-scripts-added.csv` |
| Workflow scripts first committed | 156 | 56 | 0 | `strategy-workflow-scripts-by-date.csv` |
| Workflow scripts using worktrees | 2% | 46% (all post-Bun) | — | `strategy-workflow-patterns-by-era.csv` |
| Workflow scripts comparing against the oracle | 7% | 98% | — | same |
| UPSTREAM-DEFECTS entries at period end | — | — | 205 | `strategy-upstream-defects-cumulative.csv` |

Reading (ESTIMATED interpretation): post-Bun workflows shift from per-crate
port-and-verify loops to oracle-anchored parity sweeps run in worktree-isolated lanes; the
adversarial stage moves from inline script prompts into the separate-context review gate of
each worktree merge (commits saying "reviewed" go from 9 and 5 to 69 in the post-R13 era).
Commits per day roughly double after R13 because closure work is many small, test-first,
one-finding commits (`.port/parity-closure/PROCEDURE.md`).

Caveat: the dip in commits/day in the post-Bun window includes W32–W34 (2, 10 commits) — the
Codex audit period and a pause — so cadence alone is a weak signal.

---

## 3. Later evolutions (not Bun-derived)

1. **Workstreams + review gate (07-23).** The completion ROADMAP
   (`docs/superpowers/plans/2026-07-23-rustty-completion-ROADMAP.md`) splits work into WS1–WS20,
   each run by a worktree-isolated executor with a per-workstream review (spec review + quality
   review, opus), merged serially by a controller, then a Linux build gate on `ssh rvm`
   (`.superpowers/sdd/progress.md`, untracked; DOCUMENTED). Example: WS5 review found
   `clipboard::WriteResult` closed-enum-from-C UB, fixed as `repr(transparent) i32` (`86cce9d`).
2. **Oracle as a moving pin (07-29).** `ORACLE_SYNC.md`: one `ORACLE_PIN` file read by every
   oracle gate; advance-first doctrine from Phase 4 (move pin, record before/after gate state,
   then port). Seven advances absorbed 1,718 upstream commits (`strategy-oracle-pin-advances.csv`).
3. **Gates that audit their own inputs (07-30/31).** G-P2 read GREEN for days because its
   verdict derived from a hand-kept checklist; struck rows were voting. Rule: audit what a green
   rests on; self-test every acceptance arm on real rows (memory
   `derived-gate-green-can-be-bookkeeping`; `aeb05fbdc` G-CENSUS staleness).
4. **Cross-vendor audit (08-11).** Codex (GPT-5.5) ran an independent line audit (659 rows);
   Claude workflows (`wf-codexvet-*.js`) vetted every severity-bearing finding from primary
   sources (144 confirmed, 3 refuted) and remediated 140, each with a proving test. The
   remediation itself found a reachable `RGB::parse` panic on a non-char-boundary byte, fixed by
   moving parsing to `&[u8]` (`8120dd10d`). Codex activity in the stats data: Jul 16–Aug 2 only,
   28,462 calls / $2,351 list-price of a $11,032 recovered total (DOCUMENTED,
   `~/projects/rustty-stats/REPORT.md`, "Codex logs are intact").
5. **From Workflow scripts to subagent-driven development (Sep).** No new `wf-*.js` after
   2026-08-25. Phase 6–8 used hand-made worktrees plus Agent workers following
   `superpowers:subagent-driven-development` plans (`docs/superpowers/plans/2026-10-07-phase8-
   upstream-resync.md` line 3). September: 1,142 distinct subagents, 214 worktrees, 1 workflow id
   in the transcripts (MEASURED). Constraint learned the hard way: ~7 building worktree agents
   filled the disk to 100% on 2026-09-23; cap at 3 building worktrees, delete `target/` after each
   merge (memory `parallel-worktree-disk-budget`; phase-8 plan line 51).
6. **Parity audit closure (09-08 → 09-28).** 497-row ledger closed row by row: write the failing
   test first, confirm it fails for the stated reason, minimal fix, then a **negative control**
   (revert the fix, watch the test fail) (`.port/parity-closure/PROCEDURE.md`). `G-PARITY` is HARD.
7. **R13 — never port a Ghostty bug (09-11, extended 09-23).** A reference defect reachable from
   real input (crash, UB, leak, deadlock) is not reproduced: port the evident intent, prove it with
   a negative control, mark `PORT NOTE: reference defect (R13)`, record in `UPSTREAM-DEFECTS.md`.
   Extended on 09-23 to non-crash bugs that break the reference's own evident intent, plus a
   user-facing `docs/ghostty-bug-fixes.md`. Invariant asserts (conditions guaranteed by
   construction) still port under R7. Day one: 66 entries from a retroactive sweep; 205 now.
   Kinds by `kind:` field: crash 60, latent 59, functional 17, non-crash 16, leak 15, UB 19,
   test-weakness 11, deadlock 7, hang 2 (MEASURED grep; overlaps the bugs area). Two crash
   entries are marked for private report; no trigger is given here.
8. **R14 — no silent skips (09-23).** A Rust test that returns early reports `ok`; Zig reports
   SKIPPED. Every skip is a `cfg`/`ignore` attribute or a `require_<x>_or_skip` pivot that panics
   under `RUSTTY_REQUIRE_<X>=1` on lanes that have the resource. Enforced by G-SKIP
   (`check_silent_skips.py`, HARD, self-tested; allow-list capped at 6 sites).
9. **Comment fidelity ledger (09-24).** `COMMENT-NOT-CARRIED.tsv` (118 lines) records every
   reference comment G3b cannot find in the port, with a reason (`f1c1639b0`).
10. **Cross-host lanes.** macOS host; `rvm` (aarch64 Fedora UTM VM) for Linux/GTK/OpenGL from
    mid-July (Claude transcripts on rvm start 07-18; first commit naming rvm 07-29 `846bc1259`);
    `rwin` (Windows 11 ARM64 VM) from 09-27 (`a8c4fea8f`); the Phase-8 finale records a cross-host
    build check green on rvm, rwin (msvc + gnu) and macOS (`fe0f9e39d`, `1d22ba80d`).
11. **Memory-safety lanes (10-01).** `docs/superpowers/specs/2026-10-01-memory-safety-lanes-
    design.md`: Miri on `cargo miri nextest` (one process per test), Tree Borrows lane G6t, Miri
    with integrity asserts off for compression tests (G6c), G-LSAN on macOS via rustc's ASan
    runtime (R6 amended), G-SAN-SELF proving each sanitizer catches a synthetic overflow, leak and
    uninit read.
12. **Upstream test suites as differential (10-07).** G-ESCTEST runs esctest2 against both
    librustty-vt and libghostty-vt (built from a `git archive` of the pin) and is RED on any
    outcome or log-block difference not named in `esctest_known_diffs.tsv`.

---

## 4. The verification stack, as of 2026-10-08

| Layer | Tooling | Gate ids |
|---|---|---|
| Build hygiene | fmt, clippy `-D warnings`, `cargo build --workspace` (also proves acyclicity), wasm32 check | G-LINT, G7 |
| Ported tests | `cargo test --workspace`; test-block contract and per-module counts vs Zig | G1, G1b, G2, G-TESTS |
| Comment fidelity | per-file comment census vs reference after identity rename; not-carried ledger | G3, G3b |
| Differential vs oracle | byte-exact VT driver (same `vt_driver.c` against both libs), c-vt example goldens, unicode diff, fuzz smoke, esctest2, LZ4 byte-exact, reference struct geometry, EOL | G4, G4-uni, G5, G-ESCTEST, G-LZ4, G-REFGEOM, G-EOL |
| ABI | header/symbol/schema/enum-discriminant/closed-enum checks, by-value struct size (AAPCS64 sret) | A3, A4, A5, G-SYMS, G-ABISCHEMA |
| Memory safety | Miri SB (G6), Tree Borrows (G6t), asserts-off lane (G6c), ASan/LSan/MSan/TSan/HWASan + valgrind, mirihar proofs | G6*, G-ASAN, G-LSAN, G-MSAN, G-TSAN, G-HWASAN, G-VALGRIND, G-SAN-SELF |
| Bun-class detectors | D1–D6, D8, D9 (+ D10, D11 later) | D* |
| Process integrity | no silent skips, parity ledger state, UD docs, census staleness, phase closure | G-SKIP, G-PARITY, G-UDDOCS, G-CENSUS, G-P2 |
| Cross-host | rvm Linux, rwin Windows, cross-target check matrix | G-XTARGET, G-WINCHK |

101 registry entries; 24 are dedicated self-test entries; 57 of 90 gate scripts carry a
self-test arm (MEASURED). Oracle-dependent gates report BLOCKED (exit 3) on GitHub CI where the
oracle is absent, never green (`.port/tools/CI.md` "Oracle-arch note").

---

## 5. Claude Code vs Codex, models, and leverage

- **Authorship trailers** (MEASURED, `data/strategy-model-by-week.csv`): every commit is authored
  by the user; `Co-Authored-By` trailers name Claude Opus 4.8 (W25–W30), Fable 5 (W27–W36,
  bursts), Opus 5 (W31–W38), Opus 5.5 (W39+), Sonnet 5.5 (W40+). 317 commits carry no Claude
  trailer (52 of them merges; 157 fall in W31).
- **Codex** was used as an independent auditor (Jul 16–Aug 2 in logs), never as the porter of
  record; its findings were re-verified by Claude before any fix (§3.4).
- **User prompts vs commits** (`data/strategy-leverage-by-week.csv`): peaks of 13.4 commits per
  prompt (W39, Phase-7/parity tail) vs ~1.0 in W29 (Linux bring-up on the VM, hands-on).
  Prompt counts come from `history.jsonl` (lengths only) and are complete; API-call data before
  2026-09-08 was lost to transcript cleanup (`rustty-stats/README.md` "Lost data"), so cost and
  subagent shares are reported only for the recovered window.

---

## 6. Proposed visualizations

| # | Title | Chart type | Data | Why it matters to a technical reader |
|---|---|---|---|---|
| V1 | Strategy timeline | annotated horizontal timeline, eras shaded (pre-Bun / post-Bun / post-R13), Bun post as a vertical marker on 07-08 | `strategy-timeline.csv` | One view of how rules accreted and what triggered each (a Miri finding, a false green, a user ruling, a published post-mortem). |
| V2 | Gates over time | step line: registry entries + gate scripts + self-testing scripts, Bun marker | `strategy-gate-registry-growth.csv`, `strategy-gate-selftest-growth.csv` | Shows the gate count went 11→101; flat until 07-10 then stepwise. The self-test line shows the "gates must prove they can fail" discipline scaling. |
| V3 | Gate scripts added by category and era | stacked bar (era × category) | `strategy-gate-scripts-added.csv` | Contrast what kind of checking was added before vs after Bun (memory safety and fidelity first; detectors, ABI, oracle differentials, process gates later). |
| V4 | Bun idea → Rustty artifact | Sankey or two-column mapping table with ADOPT/ADAPT/REJECT chips | `strategy-bun-adoptions.csv` | The direct answer to "what did you take from Bun": 18 ideas, incl. the 3 rejections and why. |
| V5 | Workflow-script anatomy by era | grouped bar of pattern prevalence (%), pre vs post | `strategy-workflow-patterns-by-era.csv` | Shows orchestration shifting from Miri-driven port loops to worktree, oracle-anchored sweeps. Label as regex-estimated. |
| V6 | Orchestration modes over time | strip/timeline: wf-*.js scripts added per day vs merge commits per week vs subagents per month | `strategy-workflow-scripts-by-date.csv`, `strategy-commit-keywords-by-week.csv` (merge col) | Shows the hand-off from Workflow scripts (Jun–Aug) to worktree + subagent-driven development (Sep–Oct). |
| V7 | LIFETIMES ledger growth | step line of rows, with prospective tiers annotated | `strategy-lifetimes-rows.csv` | Retroactive 216 rows on day one, then prospective rows as tiers (GTK, Swift glue, Phase-7 additions) land. Pair with Rustty's class histogram (`bun-article-rustty-lifetimes-classes.csv` from the Bun area). |
| V8 | Ghostty defects recorded after R13 | cumulative step line | `strategy-upstream-defects-cumulative.csv` | Jump to 66 on the day of the rule (retroactive sweep), then steady growth; shows how a policy change turned "faithfully ported crash" into tracked upstream defects. Coordinate with `bugs-upstream-cumulative.csv`. |
| V9 | Upstream absorbed per pin advance | bar per advance (commits, non-merge) | `strategy-oracle-pin-advances.csv` | Port tracked a moving target: 1,718 upstream commits in 7 advances. |
| V10 | Commits per user prompt | bar by ISO week, with model co-author as color band | `strategy-leverage-by-week.csv`, `strategy-model-by-week.csv` | Autonomy grows with orchestration maturity; mark data-loss caveats. |
| V11 | Verification stack | layered diagram (not a chart) | §4 table | Gives readers the whole defense-in-depth in one picture. |
| V12 | Message-keyword heat strip | heatmap weeks × {miri, UB, adversarial, reviewed, R13/UD, upstream} | `strategy-commit-keywords-by-week.csv` | Shows where effort went: Miri/UB early, review/R13 late. |

---

## 7. Gaps and caveats

- Workflow-script dates are **first commit dates**; 61 scripts were collected into git on
  2026-07-03 after running earlier, so pre-07-03 run dates are approximated by that bound.
- Pattern classification of scripts and gate categories is regex-based (ESTIMATED); a hand
  audit of ~20 scripts would firm it up.
- `.superpowers/sdd/` (SDD ledgers, review diffs) is gitignored; facts quoted from it are not
  reproducible from git alone.
- Claude transcript/API data before 2026-09-08 is lost; subagent counts and costs cover the
  recovered window only. Codex prompts are not in `prompts.csv`.
- The commit-message keyword counts depend on message conventions that changed over time
  (e.g. "reviewed" in merge subjects is a Phase-6+ convention), so they show practice, not rates.
- D7 has no detector (`.port/tools/CI.md`); D10/D11 exist in the registry but were not traced.
- Bun's blog date comes from the fetched page; the Bun PR merge date (2026-05-14) is from
  `BUN_PORT_LEARNINGS.md`.

---

## Fact-check

Independent re-derivation, 2026-10-08, repo at `1d22ba80d`. Commands differ from the
dossier's where possible (whole-range counts instead of per-row sums, awk over `%P` for merges,
TSV field parsing instead of line greps, a direct fetch of the Bun post).

| # | Fact | Verdict | Correct value / evidence |
|---|---|---|---|
| 1 | Bun "Bun in Rust" post date | **confirmed** | WebFetch of https://bun.com/blog/bun-in-rust: date line "July 8, 2026", byline "Jarred Sumner". |
| 2 | Bun research + R7–R12 + LIFETIMES on 2026-07-10, day 20 | **confirmed** (detail) | `2c03357ce` 07-10 10:55 (BUN_PORT_LEARNINGS.md; `wf-bun-port-learnings.js` is added in this same commit, not separately), `ccd14200e` 07-10 17:14 (R7–R12), `8f6d781dc` 07-10 17:51 (LIFETIMES). First commit 2026-06-21 = day 1, so 07-10 = day 20. |
| 3 | LIFETIMES rows 216 → 315; audit found 20 | **corrected** | 216 at creation confirmed (`8f6d781dc`, data rows excl. header/comments, no blank lines). At HEAD the file is **314** data rows: the dossier's `grep -v '^#' \| grep -vc '^file'` counts one blank line (HEAD has 1 blank, creation had 0). Last change `daee56664` 2026-10-08. Audit: 20 findings confirmed (`b4845f427`: "0 high, 2 medium filed, 18 low"; subject says "1 fixed, 4 filed"). |
| 4 | ci.sh registry 11 → 12 → 69 → 101; 75 HARD / 26 PROGRESS | **confirmed** | `grep -oE '"[A-Za-z0-9_.-]+\|(HARD\|PROGRESS)\|'` on the last ci.sh commit at end of 06-21 / 07-10 / 09-11 / 10-08 → 11, 12, 69, 101 (09-10 is also 69). HEAD: 75 HARD, 26 PROGRESS. |
| 5 | Self-testing gates 10/12 → 57/90; -SELF entries 0 → 24 | **confirmed** (detail) | Day one `.port/tools/gates`: 12 `.sh/.py` files, 10 grep `self-test\|self_test`. HEAD: 89 `.sh/.py` + 1 `.mjs` (`ws14_webgl_browser_gate.mjs`, no self-test) = 90; 57 self-testing. `-SELF` registry entries: 0 at 07-30, 2 at 07-31, 6 at 09-10, 24 at HEAD. |
| 6 | 212 wf-*.js, 15,576 lines, 955 agent call sites; 156 before 07-10, 56 after, none after 08-25 | **corrected** | 212 files and 15,576 lines confirmed (192 in phase-2 incl. the untracked one, 11 phase-4, 9 phase-5; 211 tracked). **Before 07-10: 155**, after: 56 (the dossier's own CSV gives 155/56/1 undated). The 1 untracked script is `wf-journey-article-research.js`, written 2026-10-08 for this article, so "none after 08-25" holds only for committed scripts. **Agent call sites ≈ 691, not 955**: 955 = raw `agent(`+`robust(` tokens (608+2+345), which counts the 88 copies of the `robust()` retry-wrapper definition (each contributes 2 `agent(` + 1 `robust(`). Stripping those definitions leaves 691 call sites. |
| 7 | 8 Bun detectors (D1–D6, D8, D9) wired 2026-07-12 | **confirmed** | `94ca0243e` 2026-07-12 02:23; ci.sh at that commit has exactly D1–D6, D8, D9, all PROGRESS. HEAD adds D10 (PROGRESS), D11 and D11r (HARD). CI.md: "D7 is intentionally absent". |
| 8 | Full vt Miri sweep (2,037 tests) found 9-test SB UB; "fixed next day" | **corrected** | Sweep `3277f2f78` 2026-07-11 01:14 ("all 2037 tests"); fix `9e3fe1c25` 2026-07-11 08:20 ("9 tests"). Fixed **the same day, ~7 hours later**, not the next day. (The sweep is the day after the 07-10 realignment.) |
| 9 | 1,718 upstream commits (1,157 non-merge), 7 advances 07-31..10-07 | **confirmed** | Single range in `../ghostty`: `git rev-list --count 4749c4e93..a60e9e2a5` = 1718, `--no-merges` = 1157 (equals the per-advance sum). ORACLE_SYNC.md has 7 rows dated 07-31 → 10-07; `ORACLE_PIN` = `a60e9e2a5`. |
| 10 | R13 ledger 66 on day one, 205 now | **confirmed** (detail) | `7dd991582` (09-11) creates the file with 0 UD headings; last commit of 09-11 (`987c6417a`) has 66; HEAD has 205 headings, 205 distinct ids, max id UD378. "Day one" = end of 2026-09-11. |
| 11 | Codex audit: 659 rows → 147 groups; 144 confirmed, 3 refuted, 140 remediated | **confirmed** | `8120dd10d` message (2026-08-11) and `.port/codex-vet/README.md:4,11`. Note the 147 groups cover 179 rows; the 3 refuted ids are listed in the commit. |
| 12 | Sep 2026: 1,142 subagents, 214 worktrees, 1 workflow id; 88.6% subagent calls | **confirmed** | `messages.csv.gz`, tool=claude, is_project=1, by month: Sep 67,917 calls, 88.6% subagent, 1,142 distinct agent_id, 214 worktrees, 1 workflow_id; Oct 82.8%, 167 agents, 48 worktrees. Caveat: July (recovered remnant, 5,011 calls) shows 9 workflow ids and 0 worktrees, consistent with the shift. |
| 13 | 0 merges before 2026-07-16; 266 total | **confirmed**; §2.5 per-era merge row **corrected** | `git log --merges` = 266, first merge `d211a84a6` 2026-07-16. But §2.5 says 108 post-Bun / 109 post-R13 (sum 217 ≠ 266). Recount by `%P` parent count and author date: **0 / 114 / 152**. |
| 14 | Cadence 23.8 / 20.0 / 45.0 per day | **confirmed** | awk over `git log --date=format:%Y-%m-%d`: 452/19, 1,257/63, 1,259/28 (same with committer date). |
| 15 | ≤3 building worktrees after ~7 agents filled disk on 2026-09-23 | **confirmed** | Memory `parallel-worktree-disk-budget.md` ("Running ~7 worktree-isolated agents at once filled the Mac's data volume to 100%", modified 2026-09-23); `docs/superpowers/plans/2026-10-07-phase8-upstream-resync.md:51` "at most 3 building worktrees on the Mac". |

**Corrections to apply in the body:** §0 row 4 and §1/§6 V7: 315 → 314. §0 row 8: 955 → ~691
agent call sites (or relabel 955 as "agent/robust tokens"). §1: 156 → 155 scripts before
07-10. §2.4 / headline: Miri fix was the same day (07-11), not the next. §2.5 merge row:
108/109 → 114/152.
