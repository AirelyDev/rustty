# Research dossier: "Rewriting Bun in Rust" (bun.com/blog/bun-in-rust)

Area: `bun-article`. Researched 2026-10-08. Read-only on both repos; outputs only under
`.port/journey-article/research/`.

Sources used:

- The live page, fetched with curl (`tmp/bun/page.html`, 507 KB) and rendered in Playwright
  (computed styles, DOM, animation frames). Text extract: `tmp/bun/article.txt`, local
  research cache only. Do not republish it, because it is copyrighted text.
- Screenshots of each interactive widget, at rest and mid-replay: `tmp/bun/viz*.png`.
- Rustty's July capture of the article and its ecosystem:
  `.port/phase-2/wf-bun-port-learnings.js` (5 parallel research agents + 1 synthesis agent) and
  its output `.port/phase-2/BUN_PORT_LEARNINGS.md` (839 lines, dated 2026-07-10).

Provenance tags: **MEASURED** = computed by me (command or method given). **DOCUMENTED** = stated
in the article or in a repo file (path given). **ESTIMATED** = my judgement.

---

## 0. Key facts

| Fact | Value | Provenance |
|---|---|---|
| Article | "Rewriting Bun in Rust", Jarred Sumner, July 8, 2026 | DOCUMENTED (page header) |
| Port duration | 11 days, from first commit May 4 to merge May 14, 2026 | DOCUMENTED |
| Concurrency | about 50 dynamic workflows; peak 4 worktrees x 16 Claudes = 64 | DOCUMENTED |
| Commits | 6,502 on the port branch (merges excluded); 6,778 in the stats table | DOCUMENTED; 6,502 also MEASURED by summing the 207 heatmap tooltips (`data/bun-article-commits-per-hour.csv`) |
| Landed diff | +1,009,272 lines; 535,496 Zig LOC (excluding comments) became about 780,000 Rust LOC | DOCUMENTED |
| Cost | 5.9B uncached input, 690M output and 72B cached-read tokens, about $165,000 | DOCUMENTED |
| Regressions | 19 known, all fixed; v1.4.0 fixes 128 bugs that reproduce in v1.3.14 | DOCUMENTED |
| Rustty adoption date | Bun learnings committed 2026-07-10, 2 days after the article (`2c03357ce`); R7-R12 ratified the same day (`ccd14200e`) | MEASURED (`git log -- .port/phase-2/BUN_PORT_LEARNINGS.md`) |
| Rustty commits before adoption | 452 of 2,968 (15.2%). Rustty started 2026-06-21 (`15c1a9bd7`) | MEASURED (`git rev-list --count --before='2026-07-10 00:00' HEAD`) |

---

## 1. Structure (section by section)

The page has 1 H1, 11 H2, 18 H3 and 9 H4 headings, plus 13 `<pre>` code blocks, 6 tables,
5 custom interactive widgets, 2 images, 1 embedded tweet and 29 external links. **MEASURED**
by regex over the `<article>` element of `page.html`. The full ordered list, with a summary
and word count for each section, is in **`data/bun-article-sections.csv`**. The arc:

1. **Problem** (intro to "Why Rust?", about 1,600 words). Bun's scope, then a concrete list of
   13 memory-safety bugs fixed in v1.3.14: use-after-free (UAF), double-free, error-path leaks
   and one GC race. Next comes what Bun already did: ASan via a patched Zig, ReleaseSafe on
   Windows, Fuzzilli and leak tests. The core argument is that `defer`-based cleanup plus
   style guides cannot be enforced, while `Drop` and the borrow checker turn the bug class
   into compile errors. C++ is considered and rejected for the same reason.
2. **Strategy** ("Claude, rewrite Bun in Rust." and "Loops that write & review code"). Two
   decisions: all-at-once, not incremental; and a port that "looks like we transpiled our
   Zig", with idiomatic refactoring deferred until after v1.4. Engineering is framed as a
   `task -> 2x review -> apply` loop. The aim is to fix the process that generates the code,
   not the code itself.
3. **Adversarial review**, with split context windows: 1 implementer and at least 2
   reviewers. The implementer never reviews and the reviewer never implements. This section
   contains widget V1.
4. **Execution chronology** ("What does this look like?"):
   1. Prep work: PORTING.md and LIFETIMES.tsv.
   2. Trial run on 3 files.
   3. False starts: git races, then worktree sharding.
   4. Writing the code: V2 heatmap.
   5. Compiler errors as a work queue: about 100 crates, cycle-breaking and about 16k
      errors; V3 replay.
   6. Another false start: stubbing and long justification comments.
   7. Smoke tests.
   8. Local test suite.
   9. Even more false starts: cgroup isolation.
   10. CI: V4 race to green.
   11. Merge.
   12. Stats: V5 git-log replay, plus the test-count, token and cost table.
   13. The work continues: security review, fuzzing, unsafe share.
5. **Porting mistakes.** Four regression classes, each with a Zig/Rust snippet pair and an
   issue link.
6. **Outcomes** ("Bun is better in Rust"): memory, binary size, stack usage and speed, each
   with a table or test output.
7. **Production, Shipping, Maintainability, What's next.** A side-by-side Zig/Rust function
   shows comments carried over verbatim and `comptime bool` becoming a const generic.

---

## 2. Visualization inventory

Every figure is listed in **`data/bun-article-visualizations.csv`**. Visual tokens are in
**`data/bun-article-style-tokens.json`**. All measurements in this section are **MEASURED**
from Playwright computed styles and the SVG DOM.

### Shared design language of the 5 custom widgets

- **Dark "terminal card" on a white page.** Background `#0b0c10`, 1px border `#282a36`,
  rounded-2xl corners. On mobile the card bleeds to the screen edge. On wide screens it is
  wider than the 736px prose column (800px, from `lg:-mx-8 xl:-mx-24`).
- **Header block.** An orange outlined pill reading "✻ claude code · dynamic workflow"
  (Martian Mono 9px, `#e8946f`). Next to it is an uppercase tracked label (Martian Mono 11px,
  letter-spacing about 2px, `#6b7280`). Below sits a large monospace headline number
  (Martian Mono 30px, weight 700).
- **Caption** in a footer strip separated by a rule (12px sans, `#6b7280`). Every caption
  states exactly what is plotted and stresses that the data is real, for example "Error
  lines are real commit subjects" and "The log is real commit messages".
- **Replay animations.** A `▶ play` or `↻ replay` button compresses days into seconds. A live
  timestamp in PDT is shown next to it, and counters tick during playback. The animation
  runs on `requestAnimationFrame`, with scroll-triggered staggered reveals.
- **Implementation.** SSR React islands with Tailwind and inline SVG. No chart library was
  found in the shipped chunks (`tmp/bun/chunk-*.js`). Marks carry SVG `<title>` tooltips
  with exact values.
- **Typography.** Archivo for the H1, the system sans for prose (17px), and Martian Mono for
  all code and data labels.

### The five widgets

**V1. Adversarial review replay** (section "Split context windows")

- A three-step chat stepper.
- Columns: "claude implementer" on the left, with its context described as "the .zig
  original, the port plan, its own reasoning". "adversarial reviewer claude" is on the right,
  with its context described as "only the diff. told to assume the code is wrong."
- Each step shows the implementer's code card: file path, a "compiles clean" badge, and the
  buggy lines tinted red. The reviewer's critique appears as a bubble with a red tint. The
  fixed code card follows, tinted green, with the real commit hash and subject.
- The three bugs:
  - async `uv_close` vs `Box` drop, giving UAF then double-free (`f0a454376c7`);
  - `trunc` vs `floor` producing a negative `nsec` (`7cc88f00141`);
  - eager `unwrap_or` vs lazy `unwrap_or_else` causing a panic (`90111846a14`).
- The button relabels itself to "next bug · the eager default".

**V2. Commit heatmap** (section "Finally writing the code")

- A grid of 11 days (rows, May 4 to May 14) by 24 hours (columns, PDT).
- 264 cells, each a 19x19 rect with rx=4, in a 556x257 viewBox.
- Colour ramp from 1 to 695 commits per hour: deep purple, then magenta, orange and yellow
  (plasma-like). The scale looks logarithmic, since 1-commit hours are clearly purple.
  Empty hours are `#16181f`.
- Tooltip example: "May 6, 10am–11am PDT — 695 commits, +44,000 lines".
- Headline: "6,502 commits". A gradient legend bar sits top right.
- The caption notes uneven timing caused by EC2 IOPS stalls.
- Data re-extracted to `data/bun-article-commits-per-hour.csv`: 207 non-empty hours, summing
  to 6,502 commits and +1,780,453 lines. **MEASURED.**

**V3. Phase-D compiler-error drain** (section "Compiler errors as a work queue")

- Replays 1,610 real commits from May 6.
- The headline counter "≈16,000 errors left" counts down: about 10,097 after 594 fix commits
  and about 7,801 after 825.
- Left panel: an `errors.txt` list of real commit subjects with red `error:` prefixes.
- Centre: "divvied up · 64 claudes", shown as 4 worktree boxes of 4 loops each. Chips flow
  from fixer (yellow `#fbbf24`) to 2 reviewers (violet `#a78bfa`) to applier (green
  `#4ade80`).
- Right: "commits land per crate", a horizontal bar race with categorical colours
  (bun_runtime 379, bun_bundler 252, bun_sql 67, and so on). A commit-subject ticker runs
  underneath.

**V4. CI race to green** (section "Get the test suite passing in CI")

- A swimlane strip chart with 6 platform lanes, labelled with shard counts: macOS x64 · 2,
  Linux arm64 · 60, Linux x64 · 60, macOS arm64 · 4, Windows x64 · 8, Windows arm64 · 8.
- X axis is time from May 8 to May 14. Each tick is one Buildkite build: red `#ef4444` means
  any shard failed, green `#22c55e` means all passed, and dim green `#166534` means a partial
  run with no failures.
- On replay a playhead sweeps across, an "N / 6 platforms green" counter updates, and the
  current build number and time are shown. A lane label turns green with a check mark once
  that platform first passes fully.
- It covers 135 builds that ran tests, out of 420 mined. The final all-green build is #54202.
- Data re-extracted to `data/bun-article-ci-race.csv`: 574 lane-build rows across 135
  builds. **MEASURED.**

**V5. git-log replay** (section "Stats")

- Replays all 6,502 commits in 30 seconds.
- Two big counters: COMMITS, and "+LINES WRITTEN, REWRITES INCLUDED" in pink. The lines
  counter passes +1,211,368 by May 6, 1:32 PM.
- 243 time-bucket bars in a 760x130 viewBox. Bar height is lines changed. Colour is
  interpolated from pink `#f472b6` (mostly new code) through violet to cyan `#22d3ee` (mostly
  deletion).
- Dashed annotation lines mark "first 100-file draft batch", "PR #30412 opened" and "merged".
- A log ticker shows real commit subjects with green `+N` and red `−N`. A header stat reads
  "peak: 58 commits in one minute".
- The caption separates churn from the landed diff (+1,009,272).

### Static figures

- **Tables (6).** Cleanup mechanism per language. Per-platform tests (expect() calls, tests,
  files). `Bun.build` RSS at 500 to 2,000 builds. Binary size. HTTP req/s measured with oha.
  App wall time measured with hyperfine. All are plain prose tables with no styling beyond
  the prose defaults. Values are in `data/bun-article-tables.csv`.
- **Images (2).** A terminal screenshot of the `oxfmtr` corruption, and a webp chart of
  Claude Code startup p50 (517 to 464 ms).
- **Embed (1).** An X/Twitter embed about binary size.
- **Code blocks (13).** Zig and Rust side by side, workflow pseudocode, and bun test output,
  each with a copy button.

### What makes these visualizations work (ESTIMATED, for our design brief)

1. Every chart is built from **raw process exhaust**: commits, CI builds and commit
   subjects. Nothing is illustrative, and each caption says so.
2. They **animate time**. A process that took days replays in seconds, with a live
   timestamp. This conveys how continuous and parallel the agent work was.
3. Each widget shows **one mechanism**: review, throughput, error drain, convergence or churn.
4. The visual language is consistent: a dark card, mono numerals, and a small palette with
   roles fixed across widgets (pink = new code, red = fail, green = pass or apply,
   violet = review).
5. Each widget pairs a headline number with a chart.

---

## 3. Every metric reported

The full list is in **`data/bun-article-metrics.csv`** (63 rows, all DOCUMENTED except the
word-count rows). Grouped:

**Scale**

- 535,496 Zig LOC, excluding comments.
- 20% of the code is C++.
- 1,448 .zig files.
- About 100 target crates.
- About 780,000 Rust LOC.
- +1,009,272 lines landed.

**Process**

- About 3 hours of PORTING.md dialogue; a 3-file trial.
- About 50 workflows over 11 days.
- 4 worktrees x 16 Claudes = 64.
- Peak generation of about 1,300 LOC per minute.
- 6,502 commits, merges excluded (6,778 in the stats table).
- Peaks of 695 commits per hour and 58 commits per minute.
- About 16,000 compiler errors, with 1,610 phase-D commits.
- CI failing files went from 972 to 23 in 2 days; Linux was green 1.5 days later.
- 135 of 420 CI builds ran tests; the final build was #54202.
- 6 platforms.

**Verification**

- 0 tests skipped or deleted.
- About 1.39M, 1.26M and 1.01M `expect()` calls on Debian, macOS and Windows.
- About 57k to 61k tests and about 4,174 files per platform.

**Cost**

- 5.9B uncached input, 690M output and 72B cached-read tokens.
- About $165,000.
- The by-hand counterfactual is 3 engineers for about a year.

**Quality**

- 19 known regressions, all fixed.
- 128 v1.3.14 bugs fixed in v1.4.0.
- 11 post-merge security-review rounds.
- Fuzzing: 100B parser executions and about 15 PRs.
- unsafe: about 4% of code (about 13,000 keywords across about 27,000 lines out of about
  780k); 78% of unsafe blocks are a single line.

**Performance and resources**

- `Bun.build` x2000 RSS: 6,745 MB in v1.3.14 vs 609 MB in v1.4.0 (about 3 MB leaked per build
  before).
- Binary size: -3.8, -5.5 and -6.8 MB (Windows, macOS, Linux) from the port alone. With ICF
  and ICU trimming, about 20% overall (94 to 76 MB on Windows, 88 to 70 MB on Linux).
- Stack: the TOML test at 25,000 nesting levels no longer overflowed.
- HTTP throughput +2.8% to +4.8%; app wall time +2.2% to +4.7%.
- Claude Code startup on Linux p50: 517 to 464 ms (-10%).

---

## 4. Methodology and strategy

### 4a. As described in the article (DOCUMENTED)

- **Mechanical, all-at-once port.** Keep the architecture, behaviour and performance.
  Accept `unsafe` and refactor after v1.4. The oracle is Bun's unchanged TypeScript test
  suite, which is language-independent.
- **Dynamic-workflow loops.** About 50 Claude Code workflow scripts, each a loop of
  `task -> implement -> 2 adversarial reviews (in parallel) -> apply`. Workflow kinds:
  - porting-guide generation;
  - per-file port matching PORTING.md and LIFETIMES.tsv;
  - per-crate compile fixing;
  - per-subcommand smoke tests;
  - per-test-file fixing;
  - large refactors and cleanup.

  When something goes wrong, the operator edits the loop rather than the code.
- **Split-context adversarial review.** The reviewer sees only the diff and is told to assume
  the code is wrong. Ratio: 1 implementer, at least 2 reviewers, 1 fixer.
- **PORTING.md.** About 3 hours of dialogue serialized into a Zig-to-Rust pattern and type
  map.
- **LIFETIMES.tsv.** One workflow read every struct field in every file and traced control
  flow. For each field with a complex lifetime it proposed a Rust lifetime, had it reviewed by
  2 adversarial agents, applied the feedback, and serialized the result into a TSV that other
  agents consult. A joint adversarial review of PORTING.md and LIFETIMES.tsv followed, plus a
  manual read.
- **De-risking.** A trial on 3 files before all 1,448.
- **Agent hygiene learned from failures:**
  - No `git stash`, `git reset`, `cargo` or other slow commands inside agents; commit
    specific files only.
  - 4 worktrees, because one worktree per agent would exhaust disk.
  - `cargo check` runs once at the start of each round, and git only at the end.
  - Reviewers reject stubbing, and reject "paragraph-long comments justifying a workaround".
  - Heavy tests run under `systemd-run` cgroups for memory, CPU and pid isolation.
- **Compiler errors as a work queue.** Cyclic dependencies were classified by a workflow,
  then refactored by another. Per crate: `cargo check`, then errors grouped by file, then fix,
  then 2 reviewers, then apply.
- **Merge criterion.** 100% of the suite green on all 6 platforms, a manual check that tests
  really ran and were not skipped, and local manual testing. Merge is not release.
- **Review of the mega-PR.** The author checked that the reviewers were catching Zig/Rust
  discrepancies and enforcing both guides, and read the code side by side.
- **Regression taxonomy.** Most regressions were syntactically identical but semantically
  different:
  1. a side effect inside `debug_assert!` (#30678);
  2. slice reinterpret that truncates in Zig but panics in Rust (#31188);
  3. ReleaseFast without bounds checks, plus a placeholder constant that shrank capacity
     (#31503);
  4. a `comptime` format string turned into runtime parsing (#30693).
- **Post-merge.** Security review, coverage-guided fuzzing whose crashes are auto-routed to
  Claude to open PRs, LSan tracking every native allocation, Miri on a growing subset in CI,
  and reduction of `unsafe`.

### 4b. Beyond the article, captured by Rustty on 2026-07-10 (DOCUMENTED in `.port/phase-2/BUN_PORT_LEARNINGS.md`)

The July research script `wf-bun-port-learnings.js` ran 5 parallel source clusters:

1. `porting-md`: the deleted `docs/PORTING.md` at `f6ad291ea032`, 756 lines.
2. `port-pr-and-crates`: PR #30412 and PR #30224, plus the committed
   `.claude/workflows/*.workflow.js` at `ed1a70f81708`.
3. `regressions`: the 4 issues above plus LSan PR #30875.
4. `hn-critique`: HN item 48016880, 548 comments.
5. `unsafe-and-style`: bun.com/bun-unsafe-audit and TigerStyle.

A synthesis agent then wrote BUN_PORT_LEARNINGS.md with a ranked top-10, a catalog of
learnings tagged ADOPT, ADAPT, COVERED or REJECT, detector specs D1-D9, and proposed rules
R7-R12. Details the article itself does not give:

- **Bun's LIFETIMES.tsv schema.** Columns `file/struct/field/zig_type/class/rust_type/evidence`
  over 2,252 rows. An 11-class taxonomy: OWNED, SHARED, BORROW_PARAM, BORROW_FIELD, STATIC,
  BACKREF, INTRUSIVE, FFI, ARENA, UNKNOWN and JSC_BORROW. Evidence is written as
  "file:line — what you saw". Guidance: "trust it over local guessing". Verification used
  3 default-refute votes on every UNKNOWN plus a 12% sample of confident claims.
- **Workflow shapes** (phase-a-port, phase-g-test-swarm, phase-h-main-parity):
  - The orchestrator computes paths ("do NOT let agents pick").
  - Verifiers "default to ok=false"; fixers make "surgical edits only" and may skip a
    hallucinated finding.
  - Disputed findings in the 2-vote intersection go to a "default confirmed=false" tiebreak.
  - One rebuild per round; fixer agents are forbidden to build.
  - Test parity is pass-count equality against the reference binary, recorded in `.diag`
    files.
  - "A merge does NOT port the fix."
- **Unsafe audit method.** One committed `rg` pattern; an origin x destination taxonomy;
  13,365 sites; 13.7 sites per kLOC; a separate ledger of unsound *safe* functions.
- **The fifth regression class**, not in the article: a hard-coded mimalloc
  `#[global_allocator]` blinded ASan and LSan (#30875).

### 4c. What Rustty borrowed, precisely

The full mapping is in **`data/bun-article-adoption-map.csv`**. All dates and hashes below
are MEASURED from git.

- **2026-07-10, `2c03357ce`.** Learnings document, validation sweep #1 and realignment plan
  committed.
- **2026-07-10 17:14, `ccd14200e`.** `PORT_RULES.md` ratifies **R7-R12**. Headings are at
  `.port/PORT_RULES.md` lines 192-345.
  - R7: replacement-primitive semantic fidelity. Covers `@intCast`, wrapping and saturating
    arithmetic, slice reinterprets, zip-length asserts, assert/unreachable mapping, and
    rewriting stale comments.
  - R8: comptime-phase and build-switch preservation, including constants ported by
    expression and sanitizer canaries.
  - R9: unsafe accounting and the unsound-safe ledger.
  - R10: markers `TODO(port)`, `PERF(port)` and `PORT NOTE`.
  - R11: the adversarial verification protocol, the LIFETIMES ledger and swarm hygiene.
  - R12: upstream (main-)parity tracking.
  - Added in the same commit: a binding pre-port tier checklist whose item 1 is "LIFETIMES
    rows land first".
- **2026-07-10 17:51, `8f6d781dc`.** `.port/audits/GHOSTTY_LIFETIMES.tsv` created. It uses
  Bun's exact schema: `file struct field zig_type class rust_type evidence`.
  - It now has **315 data rows in 18 classes**: BORROW_FIELD 68, BORROW_PARAM 49, OWNED 34,
    FFI 31, SHARED 27, INTRUSIVE 25, ARENA 19, GHOSTTY_C 17, STATIC 11, BORROWED_PTR 10,
    BACKREF 9, and 7 smaller classes. **MEASURED** with an `awk` field-5 histogram; data in
    `data/bun-article-rustty-lifetimes-classes.csv`.
  - GHOSTTY_C replaces Bun's JSC_BORROW.
  - The file has been touched by 67 commits through 2026-10-08 (`daee56664`), so it is a
    living ledger. **MEASURED.**
- **2026-07-10, `b4845f427`.** LIFETIMES retroactive audit against code already ported:
  20 findings, 1 fixed and 4 filed. **DOCUMENTED**: the commit subject and
  `.port/audits/LIFETIMES_AUDIT_2026-07-10.md`.
  - **2026-07-12, `dcae8d61e`**: "fix xev MPSC completion-queue latent SB/TB UB
    (LIFETIMES-audit M1+M2)". This is a concrete bug found by the borrowed technique.
- **2026-07-10, `d8ee18a75`.** Unsafe census v1 (`.port/audits/count-unsafe.sh`, with the
  same rg pattern as Bun) and the `UNSOUND_SAFE.md` ledger seed.
- **Detector artifacts** in `.port/audits/` (DOCUMENTED, files present):
  `slice-reinterpret.tsv` (D1), `capacity-saturation.tsv` (D2),
  `D9_STALE_COMMENT_CHECKLIST.md` and `stale-comments-D9.tsv` (D9), and `MARKERS.md` (R10).
- **Explicitly rejected:** Bun's non-compiling "Phase A draft, then compiler-errors-as-queue"
  main loop. Rustty gates every sub-batch compiled and tested under R5/R6. Also rejected:
  Bun's crates.io-for-pure-Zig exception, because R1 requires hand-porting (z2d, zf).
  **Kept over Bun's "ZERO todo!()" rule:** Rustty's placeholder-leaf doctrine.
- **Unsafe density** (`data/bun-article-unsafe-compare.csv`, DOCUMENTED from the census
  JSONs). Rustty went from 27.0 sites per kLOC at 2026-07-10 (11,145 sites over 412k LOC) to
  27.9 at 2026-08-25 (18,826 over 676k). Bun reports 13.7 per kLOC.
  - **Comparability caveat (ESTIMATED):** Rustty's LOC counts all `.rs` lines in
    crates, crates-sys and xtask, including comments and tests. It also includes `-sys` FFI
    crates. Bun's figures are approximate and their LOC basis is not stated. Do not present
    the two numbers as like for like without that caveat.

---

## 5. Tone and length calibration

- **Length.** About 7,700 visible words including code and widget text, and roughly 5,000 to
  6,000 words of prose. That is about 25 to 30 minutes of reading. **ESTIMATED**: the
  extracted text measured 7,728 words after dropping chart-tooltip blobs; prose is my
  estimate after removing code and widget text. Per-section counts are in
  `data/bun-article-sections.csv`, but the CI-race section count includes some lane labels.
- **Voice.** First person and candid. Admits false starts at three separate points ("False
  starts", "Another false start", "Even more false starts") and states the regressions
  openly. Uses short, punchy section titles. There is some humour, such as "Models love
  saying 'smoke tests'", but every claim is backed by a commit hash, issue number, table or
  replay of real data.
- **Technical density.** High. Real file paths, commit hashes and short code pairs carry the
  argument. Benchmarks state the hardware and tools (EC2 Xeon 8488C, oha, hyperfine).
- **Structure pattern.** Problem, then decision, then mechanism (loops and review), then
  chronology with numbers and widgets, then mistakes, then outcomes, then production and
  maintainability.
- **For Rustty's article.** Match the evidence density and the candour about false starts
  and regressions. Skip the product-promotion material (download counts, customer quote,
  canary call to action), because our brief is technical only. Note that the Ghostty
  references must stay fact-only, per the user rule.

---

## 6. Proposed visualizations for the Rustty article

Each item is modelled on a Bun widget and fed by Rustty data. Files marked `(other areas)`
will be produced by the git-history and stats research areas.

1. **Commit heatmap, days x 24h** (Bun V2 analogue).
   - Data: Rustty commits per hour over the whole project, 2026-06-21 to now. About 110 rows
     (days) x 24 columns, or a GitHub-style weekly calendar because Rustty spans months, not
     11 days. `(other areas: git-history commits-per-hour CSV)`.
   - Reference: Bun's data is in `data/bun-article-commits-per-hour.csv` for a side-by-side
     or caption comparison (11 days vs about 3.5 months; burst vs sustained).
   - Why: it shows the rhythm and the phases. Phase boundaries can be annotated, including
     2026-07-10, the Bun-learnings pivot.
2. **Strategy-pivot timeline.** An annotated cumulative-commits line with a vertical marker
   at `2c03357ce` and `ccd14200e`, plus markers for the R7-R12 rule births and later R13 and
   R14.
   - Data: `data/bun-article-adoption-map.csv` plus a cumulative commits series (other areas).
   - Why: it shows the article-driven strategy change precisely. 15.2% of commits come before
     it.
3. **LIFETIMES ledger class distribution.** A horizontal bar chart of the 18 classes, with
   Bun-taxonomy classes and Rustty-specific ones (GHOSTTY_C, BORROWED_PTR, INLINE_OWNED and
   others) in different colours.
   - Data: `data/bun-article-rustty-lifetimes-classes.csv`.
   - Why: it makes the borrowed artifact concrete and shows how it was adapted.
4. **Adversarial review replay** (Bun V1 analogue). A three-bug stepper built from real
   Rustty review-fix commits: implementer card, reviewer critique, fixed card with hash.
   - Candidates: the LIFETIMES-audit xev MPSC SB/TB UB fix (`dcae8d61e`), the search-screen
     SB UB (Rustty memory notes), and the embed ABI by-value sret crash.
   - Data: hand-picked commits (other areas: bugs and review).
5. **Gate convergence swimlanes** (Bun V4 analogue). Lanes for macOS, Linux (rvm), Windows
   msvc and gnu (rwin), Miri and the sanitizer matrix, over time. Ticks are gate runs
   recorded in `.port/phase-*/` records. Phase-8 T1b cross-host GREEN is the end stamp.
   - Data: other areas (CI/gates).
6. **Upstream-resync drain** (Bun V3 analogue). For each pin advance (Phases 3 to 8), upstream
   Ghostty commits triaged into ported, missing, partial or N/A, drained as a counter with a
   per-crate bar race.
   - Data: `.port/ORACLE_SYNC.md` and the MAIN_PARITY ledgers (other areas).
7. **Unsafe density over time.** Line or dot chart of sites per kLOC per census snapshot, by
   tier (pure-miri, ffi-asan, sys).
   - Data: `data/bun-article-unsafe-compare.csv` for the totals; the `by_tier` blocks of each
     census JSON can be expanded.
   - Show Bun's 13.7 only as an annotated reference, with the caveat above.
8. **Regression-class matrix.** A table or heat-grid of Bun's 5 semantic-mismatch classes
   against Rustty's detector (D1-D9), the rule (R7/R8) and the instances caught.
   - Data: `data/bun-article-adoption-map.csv` plus detector TSV row counts (other areas).
9. **git-log replay** (Bun V5 analogue). Pink/cyan add/delete bars over the full history,
   with phase annotations and counters for commits and lines.
   - Data: other areas (numstat per commit).

Style guidance, if we emulate Bun: a dark card on the light page, monospace numerals, a
replay button with a live timestamp, captions that say "real commits", and fixed role colours
throughout. The concrete tokens are in `data/bun-article-style-tokens.json`. Use the Rustty
website's own palette rather than Bun's pink and violet.

---

## 7. Gaps and caveats

- WebFetch refuses verbatim reproduction. The full text was obtained from the raw HTML
  instead. It is copyrighted, so quote it sparingly.
- The two raster images loaded lazily and were not downloaded. Their content is known from
  alt text and captions only.
- The V3 per-crate commit totals were seen only mid-animation. The final per-crate numbers
  are not in the static HTML; they are computed inside the bundled component, which I did not
  fully decompile.
- Bun's 6,502 vs 6,778 commits: the article does not explain the difference. Presumably the
  6,778 includes merges.
- Prose word count is an estimate.

---

## Fact-check

Independent re-derivation, 2026-10-08. Methods deliberately differ from the dossier's where
possible: Python `html.parser` and a fresh regex over `<title>` tooltips instead of the original
script; Python CSV histogram instead of `awk`; plain-path `git log` plus `git show` on each hash.

| # | Fact | Verdict | Evidence / correction |
|---|---|---|---|
| 1 | Bun: 11 days, about 50 workflows, 4 worktrees x 16 = 64 Claudes | **Confirmed**, one nuance | article.txt l.119 "about 50 dynamic workflows ... 11 days"; l.221 and l.391 "4 worktrees ... 16 claudes ... About 64". Heatmap rows run May 4 to May 14 (11 days). Nuance: the stats-table caption says "11 days (May 3 → merged May 14)", so the article itself gives both May 3 and May 4 as the start. Say "May 3/4 to May 14" or just "11 days". |
| 2 | 6,502 commits (merges excluded); +1,780,453 churn; landed +1,009,272 | **Confirmed** | Fresh regex over `<title>` tooltips: 207 unique hours, 6,502 commits, +1,780,453 lines, peak 695/hour. Covers 11 distinct days (May 4-14). "+1,009,272" appears once in the V5 caption. 6,778 is the stats-table figure; the article does not explain the gap. |
| 3 | 5.9B uncached input, 690M output, 72B cached reads, about $165,000 | **Confirmed** | article.txt l.412, word for word: "5.9 billion uncached input tokens, 690 million output tokens, and 72 billion cached input token reads — around $165,000 at API pricing". The article spells out "billion"/"million", so a grep for "5.9B" finds nothing. |
| 4 | 19 regressions, all fixed; 4 classes shown; 128 v1.3.14 bugs fixed | **Confirmed** | l.422 "19 known regressions, each of which has been fixed"; 4 H4 subsections under it (debug_assert #30678, odd-length slices #31188, bounds checks #31503, comptime format strings #30693); l.465 "fixes 128 bugs that reproduce in v1.3.14". |
| 5 | Composition: 1 H1/11 H2/18 H3/9 H4; 13 code blocks; 6 tables; 5 widgets; 2 images; 1 tweet | **Confirmed**, one clarification | `html.parser` over `<article>`: h1 1, h2 11, h3 18, h4 9, pre 13, table 6. There are 3 `<img>` tags, but one is the 28px author avatar, so 2 content images is correct. There are 5 `bg-[#0b0c10]` widget cards. There is 1 `twitter-tweet` blockquote. |
| 6 | About 7,700 visible words; 5,000-6,000 prose words | **Confirmed as an estimate**, range widened | My parse of `<article>`, skipping script, style and `<title>` tooltips, gives **7,042 words**. The dossier's article.txt method gives 7,728, and raw `wc -w article.txt` with tooltip blobs gives 10,864. Use "about 7,000-7,700 words". The prose-only figure stays an estimate, not measured. |
| 7 | Adoption 2026-07-10 (`2c03357ce`), R7-R12 `ccd14200e`, 452 of 2,968 commits (15.2%) before | **Confirmed** | `git show`: 2c03357ce 2026-07-10 10:55 -0700, ccd14200e 2026-07-10 17:14 -0700. That is 2 days after the July 8 article. `git rev-list --count HEAD` = 2,968, and with `--before='2026-07-10 00:00'` it is 452 (15.23%). First commit is 15c1a9bd7, 2026-06-21. Caveat: the total is for HEAD = `phase-8-upstream-resync` as of 2026-10-08 and will drift. |
| 8 | GHOSTTY_LIFETIMES.tsv: 315 rows, 18 classes, 67 commits, created `8f6d781dc` | **Corrected**: 314 data rows | Python parse, skipping `#` comment lines: header + **314** data rows, all with 7 columns. 18 classes. The counts match the dossier exactly (BORROW_FIELD 68, BORROW_PARAM 49, OWNED 34, FFI 31, SHARED 27, INTRUSIVE 25, ARENA 19, GHOSTTY_C 17, STATIC 11, BORROWED_PTR 10, BACKREF 9, INLINE_OWNED 5, OWNER_TRANSFER 3, OWNED_FD 2, and 1 each for OWNED_HEAP, SHARED_CELL, BORROWED_HANDLE and BORROWED_SLICE), and they sum to 314. "315" counted the column-header line. Commits: plain `git log -- path` gives 67 (with `--follow`, 53). Last touch is daee56664 on 2026-10-08. `--diff-filter=A` confirms creation by 8f6d781dc at 2026-07-10 17:51. The schema is identical to Bun's 7 columns. |
| 9 | LIFETIMES audit: 20 findings (1 fixed, 4 filed); xev MPSC latent SB/TB UB fixed `dcae8d61e` | **Confirmed**, with a precision note | b4845f427 subject and `LIFETIMES_AUDIT_2026-07-10.md`: "20 findings: 0 high, 2 medium, 18 low". The 1 fix was a **comment-only** PORT NOTE correction. The 4 filed findings are M1, M2, L7 and L18, in 3 DEFERRALS entries. dcae8d61e (2026-07-12) fixed M1+M2, proven on the `mirihar-xev-mpsc` harness under SB and TB. Wording for the article: the UB was **real but latent**. It sat in a dead thread-pool branch that was unreachable at the time, so do not call it a reachable crash. The fix needed a separate heap allocation for the queue, because the one-line `addr_of_mut!` fix was not enough. |
| 10 | Unsafe density: Rustty 27.0 -> 27.9/kLOC; Bun 13.7/kLOC, about 4% | **Confirmed**, two notes | Census JSON `totals`: b4845f4 (added 2026-07-10) 11,145 sites / 412,464 LOC = 27.02. b0685c0d (2026-08-25, phase-5, the later of the two August snapshots) 18,826 / 675,517 = 27.87. There is a third snapshot, 47b2dbbb (2026-08-25, phase-4) at **28.15** (18,645 / 662,381), so the series is not monotonic. No census is newer than 2026-08-25, so the "current" density is unmeasured. Bun's 13.7/kLOC is from BUN_PORT_LEARNINGS.md l.97/440 (the bun-unsafe-audit page, 13,365 sites), **not** the article. The article gives "about 4% ... ~13,000 keywords across ~27,000 lines / ~780,000 lines" (l.418). 13,365 / 13.7 implies about 975 kLOC, not 780k, so even Bun's two figures use different LOC bases. The comparability caveat is stronger than the dossier states. |

Net: 9 of 10 confirmed, 1 corrected (LIFETIMES rows 315 -> 314). Clarifications to carry into
the article: Bun start date (May 3 vs May 4), word count (about 7,000-7,700), that the audit's
"1 fixed" was comment-only and its UB latent, and the non-monotonic unsafe series with Bun's
mismatched LOC bases.
