# /porting: author review checklist

Everything below needs your eyes before publication. Each item says what to check, where it lives, and what "done" looks like. Paths are relative to the website repository; research paths are under `rustty/.port/journey-article/research/`.

Tick an item by changing `[ ]` to `[x]`. If you change a number, change it in `scripts/porting/facts.csv` (or the named builder), run the build from `scripts/porting/build_data.py`, then `npm run verify:porting`.

---

> Status 2026-10-11: rustty `main` (to 3a59e7edc) and `journey-article-research` pushed to the private AirelyDev/rustty; every article link to Rustty resolves for signed-in viewers. The author ACCEPTED the five redacted values already in main's history (Windows VM LAN IP, Linux VM LAN user@host, two packaging emails, the other project's name) — none is a secret; revisit only if the repo goes public with a different policy. Advisories on hold (author will return to them); credit line decided: the author by name, Claude credited for the writing.

## 1. Gates before publication (blocking)

- [ ] **Confirm both advisories are filed before this page goes live (details are published on the page).** Two defect-map entries are published here in full by author decision: **UD131** (the row-copy retry loops `@panic` when a destination page's storage is exhausted; the entry is also the ReleaseFast reclassification below) and **UD351** (an OSC 7 report whose host percent-decodes to more than 255 bytes). They have not been reported yet: the widget says each is "Not reported yet. It is to go to Ghostty privately, as a security advisory…", and the prose says two crashes "are to be reported privately". Blocking: file both advisories with Ghostty before deploy, then update that text (`PRIVATE_US` and `STATUS` in `scripts/porting/widgets/w08_defect_map.py`, and the "Two crashes reachable…" sentence in `content.mdx`, section "Never port a bug"), or hold the page. — **Owned by the author (handled separately).**
- [x] **Merge and push the Rustty commits the page links to.** Done 2026-10-11: Rustty `main` is pushed to the private AirelyDev/rustty at `3e43a1d18` (local `main` = `origin/main`), so every commit the page links resolves for signed-in viewers. Re-run before publication: `for h in $(grep -oh 'hash="[0-9a-f]*"' src/app/porting/content.mdx | cut -d'"' -f2 | sort -u); do git -C ../rustty branch -r --contains $h | grep -q . || echo "unpushed $h"; done` prints nothing.
- [x] **Push Rustty `main` and the research branch (private).** Done 2026-10-11: `main` at `3e43a1d18` and `journey-article-research` (data commit `f153b7014`, then the article source snapshot) are pushed to the private AirelyDev/rustty. Making the repository public is a separate step that still waits for the two advisories (item above and the advisories item below); every Rustty link 404s for readers until then.
- [ ] **Approve or remove the short prompt quotes left in the committed research before pushing it.** The privacy scan kept a few short prompt quotes the article does not use: one in `BRIEF.md` (an extension of the "Lets call our port rustty…" quote), three short bug-report prompts in `r2-platforms.md`, short limit prompts in `r2-cost.md` and in `data/r2-human-story-quiet-stretches.csv`, and one or two one-line prompts in `data/mock-cost-embed.json` and `data/r2-codex-audit-timeline.csv` (see the privacy-scan section of `links-2026-10-10.md`). Approve them, or remove them on `journey-article-research` before it is pushed.
- [ ] **Data still to land after Task 21 (re-snapshot at Rustty `58aa62a81` / Ghostty `246f70287`, 2026-10-10).**
  - The Windows full-suite figure (6,418 passed) cites an uncommitted log, `rustty/.port/phase-8/final/deferred/v26-rwin-at-58aa62a81.log.gz`: commit it (still uncommitted on 2026-10-10; until it is, the `windows_full_tests` fact links to its fallback, the re-snapshot row in `refresh-2026-10-10.md`, and should be re-pointed at the committed log).
  - The Linux sanitizer lanes and the Linux differential-driver check have no result at `246f70287` yet (the matrix shows them at `a60e9e2a5`). When they land, add rows to `research/data/verification-lanes-2026-10-10.csv`, regenerate, and update the verification paragraph and the limitations bullet in `content.mdx`, or accept the current wording.
- [ ] **Every prompt: the website-folder prompts are out of the log (2026-10-12).** The section is about the port, so the 163 prompts typed in the website folders, and the 15 sessions made only of them (the 14 website sessions and one command-only session), are no longer built (`WEBSITE_MARKER` in `scripts/porting/widgets/w12_every_prompt.py`; the builder refuses a session that mixes website-folder prompts with others). The page now shows 1,066 prompts in 97 sessions in ten areas. The facts read from the new `prompts-counts-2026-10-12.json`, and the log is published with it as `prompts-redacted-2026-10-12.json` on `journey-article-research` at research commit `1b0cf0bdf`, on `main` of the Rustty repository; the 2026-10-11f files stay there unchanged. The items below that quote 1,229 prompts, 112 sessions, eleven areas or the 14 website sessions describe the earlier state. Re-read the redaction decisions once more (23 of the 24 redact decisions still apply; the other one was for a website-folder prompt) and run the push scan over the new research commit as well.
- [ ] **Every prompt: re-read the redaction decisions and the rendered log before pushing anything.** The section publishes all 1,229 prompts verbatim. Re-read `research/data/prompts-redactions-approved-2026-10-11e.csv` (24 redact, 6 keep) and then the rendered log on `/porting#every-prompt` (open "Read every prompt", turn on the hidden commands, search for names, hosts, paths, addresses and anything you would not post). A new redaction goes into the approved CSV, then `python3 scripts/porting/build_data.py --only every-prompt` and `--only facts`, and the research copy below must be re-committed.
- [ ] **Every prompt: the prompt log is under the same embargo.** The log ships as `public/porting/every-prompt-log.json` and on `journey-article-research` at research commit `f153b7014` (`.port/porting-article/research/data/prompts-redacted-2026-10-11f.json`, the counts-only `prompts-manifest-2026-10-11e.json`, `prompts-counts-2026-10-11f.json`, the approved founding sessions (2026-10-11), the approved session titles and areas without reviewer notes, the article-work overrides and the approved redaction decisions as id, reason and decision only, all for the 2026-10-11e extract; the fact and widget-source links point there). The research branch was rewritten again on 2026-10-11 so that no commit on it holds a value redacted in either review; check again before pushing with the scan below. Push that branch and deploy the page only after both advisories above are filed. Never commit `prompts-raw-*`, `prompts-sessions-*.jsonl`, a proposed CSV (redactions, titles, sessions or founding sessions) or `prompts-manifest-private-*`.
- [x] **Every prompt: redacted values already in Rustty's history.** Some values the log redacts were committed to Rustty long before the article. The author ACCEPTED all of them on 2026-10-11 (the redaction protects the prompt log, not the repository). Pre-push scan of `main journey-article-research` on 2026-10-11, by approved-CSV file and line:
  - 2026-10-10 line 18 (IP address): 4 hits in 4 commits, 3 files at `main`. **Accepted.**
  - 2026-10-10 line 10 (a VM login; lines 15 and 16 are the same value): 17 hits in 10 commits, 3 files at `main`. **Accepted.**
  - 2026-10-10 line 4 (a packaging email, also in upstream Ghostty): 8 hits in 7 commits, 3 files at `main`. **Accepted.**
  - 2026-10-10 line 5 (email): 5 hits in 2 commits, no file at either tip. **Accepted.**
  - 2026-10-10 line 7 (the unrelated project's name): 2 whole-word hits, 1 file at `main`; its substring count (35) is mostly the name inside longer, unrelated words. **Accepted.**
  - 2026-10-11 line 6 (the author's own email address): 183 hits in 21 commits, 46 files. **Accepted**: it is already public as the git author and committer address of every Rustty commit.
  Every other row is 0, and the branch-only range `main..journey-article-research` is 0 for every row.
- [ ] **Every prompt: scan before any push.** Run the redacted-value scan over everything about to be pushed, not just the research branch. From the website repository: `python3 -I scripts/porting/scan_redacted_values.py --repo ~/projects/rustty main journey-article-research`. By default it reads the values from every untracked approved-redactions CSV (`prompts-redactions-approved-` 2026-10-10, 2026-10-11, 2026-10-11b, 2026-10-11c, 2026-10-11d and 2026-10-11e; pass `--approved <csv>` to add another), prints counts per CSV file and line number only, and exits 1 when anything is found. It matches whole words on decoded JSON plus a case-insensitive substring pass; a person-reason value (the author's username) is counted only in the prompt-log dataset files. Every count must be 0, or the value must be one accepted in the item above; add `--repo .` on the squashed website branch before it is merged.
- [ ] **Every prompt: re-extract if later port prompts should be included.** The extract stops at `2026-10-11T01:51:58Z` (the manifest's `cutoff`). To include later prompts, re-run the extractor in the research worktree (`.port/porting-article/tools/README.md`: fetch the VM history, then `extract_prompts.py`), then the builder. A re-extract needs a fresh review of everything the builder gates: redactions (the builder refuses to run while any proposal lacks a decision, or when an approved span no longer matches the re-extracted text), session titles and areas (every session needs an approved title and area, and titles pass the redaction scan) and the founding-session list. Afterwards re-commit the redacted log, manifest, counts file and approved CSVs on the research branch and re-point the `prompts_*` and `sessions_*` facts in `scripts/porting/facts.csv`, `SOURCES_HREF` in `scripts/porting/widgets/w12_every_prompt.py` and the registry `sourceHref` at the new commit (line anchors move with the counts).
- [x] **Every prompt: review the session titles and areas on the rendered page.** Each of the 112 sessions shows a title (Claude Code's own, or one written by Claude, marked "title by Claude") and one of eleven areas. Read them in the session timeline and cards on `/porting#every-prompt`, with the command-only sessions turned on, as a reader would: wrong area, a title that misdescribes the session, or a title that says more than the prompts should. Fix in `research/data/prompts-sessions-approved-2026-10-11e.csv` (`edited_title`, `edited_area`), rebuild, and re-commit the research copy. — **Signed off by the author, 2026-10-11.**
- [x] **Every prompt: review the 14 website sessions.** The website area holds the sessions typed in the website folders (162 of the 163 website-folder prompts): mostly the late-June and early-July build of the site, in Codex and Claude Code. Their titles were written or kept at review; check each title and its area on the rendered page (filter by the website area), and that none is article work by mistake (the 7 October rebrand session is website work by the author's rule, `prompts-article-overrides-2026-10-11f.csv`). — **Signed off by the author, 2026-10-11.**
- [x] **Every prompt: the founding-session list.** Three sessions typed outside the repository are included as founding prompts (`research/data/prompts-founding-sessions-approved-2026-10-11.csv`): on the night of 20 June, before the repository existed, the planning session in the projects folder (where the port was weighed, planned and named) and a one-prompt session in the early `rusty` folder whose only prompt is an `/effort` command (00:37 Pacific on 21 June); and a short session in the Ghostty checkout on 23 June, after the repository existed. One other candidate (a question about the workflows feature) is excluded. Confirm the list, in particular whether the `/effort`-only session belongs in it. — **Signed off by the author, 2026-10-11.**
- [ ] **Squash the website branch on merge.** Earlier commits of `journey-article` hold older versions of the prompt log with values that the 2026-10-11 review redacts (your username in paths, your own email, two VM logins). Merge the branch as one squashed commit so those versions never reach the public history, and run the scan above with `--repo .` on the squashed result.
- [ ] **Every prompt: glossary entries.** Internal labels in the prompts get a hover gloss from `jargon.py` plus your entries in `research/data/prompts-glossary-extra-2026-10-10.tsv` (A2, P3-W0). Check the hovers read well, and add an entry for any label you see without one.
- [x] **Every prompt: check the timeline in Safari.** Done 2026-10-11 in WebKit 18.0 (Playwright 1.48.2) at 1280x900 and 390x844 (touch): slider drag + 150 arrow presses with no SecurityError or crash; shared `#every-prompt?from=30&to=70&co=1` and `?c=all` links land with the filter applied; log, session card and mode toggle work; no page-level horizontal scroll. Only console output: a dev-server CSS preload warning. A check on a real iPhone remains optional.
- [x] **Sign off the elapsed-time estimate.** The page says that without the usage limits the same work would have taken about 46 days (41–51), from `research/elapsed-without-limits-v2-2026-10-11.md`. The estimate rests on a medium-confidence assumption: the idle stretches between work were driven by the limits. If they were not, the estimate grows by about 10 days. Confirm the wording ("I estimate") and the range, or widen it. — **Signed off by the author, 2026-10-11.**
- [ ] **Advisories still open.** Two Ghostty crashes the page publishes in full are not yet reported to Ghostty: UD131 (a terminal row copy retries forever and then panics when the destination page has no storage left) and UD351 (an OSC 7 working-directory report whose host part, once percent-decoded, is longer than 255 bytes crashes the parser). File both privately as security advisories with Ghostty before the repository or the page goes public, then update the page's "not reported yet" text (see the first item of this section). — **Owned by the author (handled separately).**
- [x] **Plan tiers, prices and credits.** The cost widget and prose state: Claude Max 20x for the whole port, ChatGPT Pro for one month (18 July to 18 August, $200; the Codex audit ran 07-31 → 08-02) and Plus otherwise; `/usage-credits` run twice on 07-13 and credits ran out on 09-08, "unknown extra-usage credits, not included". Confirm the tiers, the $200 price, the dates, and that leaving the extra-usage credits out is what you want (`src/components/porting/cost/Paid.tsx`, `Header.tsx`; `scripts/porting/widgets/w11_cost.py`). — **Signed off by the author, 2026-10-11.**

## 2. Hand reviews of machine rewrites

### 2.1 `scripts/porting/jargon-review.txt` (368 lines)

Every rewrite the builders made to visible text, one per line: `slug<TAB>before<TAB>after`. Counts by widget: agents-vs-human 191, defect-map 56, perf-replay 47, verification-matrix 27, memory-safety 18, three-reviewers 12, cost 9, translation-lens 7, moving-target 1.

How to review: read the _after_ column as a reader would. Look for (a) a stripped id that left a sentence reading oddly ("…triage the advance's gate flips"), (b) a rewrite that changed meaning, (c) quoted commit subjects that should stay verbatim. Fix a bad rewrite in the builder's substitution list (`PLAIN`, `TEXT_FIXES`, `SOURCE_NAMES`, `NO_BUN`), not in the JSON.

Notes:

- The file is append-only, so it holds a few stale lines whose _after_ text was later improved (for example the line around 327, and `Miri (P3 vt sweep)` → `…during the first resync`, now `…during the first upstream catch-up`). The JSON is the truth; stale lines can be ignored or pruned.
- The last ~30 lines are this pass's plain-word rewrites: "resync" → "upstream catch-up", "pin advance" → "reference move", "Pin +365" → "Upstream +365", "oracle harness" → "reference-comparison harness".
- Commit subjects shown in agents-vs-human tooltips are quoted git history and keep words like "advance the oracle pin"; only internal ids were stripped from them. Decide whether that is acceptable.

### 2.2 `scripts/porting/mapping-review.txt` (138 mappings)

The translation-lens widget's hand-drawn Zig → Rust line mappings: `slug<TAB>pattern/excerpt<TAB>Zig file: lines<TAB>Rust file: lines<TAB>first Zig line → first Rust line`.

How to review: for each pattern excerpt, open both files at the named lines (Zig at the reference commit, Rust at the snapshot commit) and check that each Zig range really maps to the Rust range shown, in particular for `defer` blocks, `comptime` branches and PORT NOTE comments. A wrong mapping draws a wrong ribbon in the widget.

### 2.3 Renamed Bun annotations in agents-vs-human

Bun is credited once, in §6 only. The agents-vs-human era and event labels came from annotations that named Bun; the builder rewrites them (`NO_BUN` in `scripts/porting/widgets/w02_agents_vs_human.py`):

| original annotation          | shown as                           |
| ---------------------------- | ---------------------------------- |
| Bun learnings + realignment: | Realignment:                       |
| Bun ideas, macOS app, perf   | New porting rules, macOS app, perf |
| Bun ideas pivot              | Validation pivot                   |
| Bun learnings committed      | Porting lessons committed          |
| Bun-rewrite learnings        | porting lessons                    |
| (Bun lifetime-c…             | (lifetime-c…                       |

Check the renamed labels still describe what happened. The strategy-timeline keeps one trigger marker that names Bun's write-up in its tooltip (ruling R5); confirm that single mention is acceptable.

## 3. Quotes

### 3.1 Rule quotes (approved 2026-10-08, ruling R16: listed for a final read)

From the port's agent memory and porting rules, shown in three-reviewers ("the rule that came out of it"):

1. clear_cells: "the Zig reference passes `*Page` / `*Row` / `[]Cell` — Zig pointers/slices are UNPROTECTED — so it's sound there. The Rust `&mut` is a _Rust-only over-constraint_." (agent memory note on protected `&mut` arguments)
2. Shaper hook: "do NOT hand-reason SB, TEST it" (agent memory note on the shaper-hook interleave)
3. Search: "BORROW_FIELD — longer-lived non-owning alias stored in a struct field; owner is another structure -> raw ptr, never &mut (SB discipline)" (pointer-lifetime ledger header)
4. Event-loop queue: "no pointer-heavy tier ports without its GHOSTTY_LIFETIMES.tsv rows landing first (2-reviewer consensus)." (porting rules)

Rule quotes in the prose (`src/app/porting/content.mdx`): line 58 (never weaken a gate), 165 (lifetime table "trust it over local guessing"), 171 ("Fixed" bar), 323 ("a merge does NOT port the fix"), 345 (early return reports `ok`), 408 ("the reference also traps"), 446 (split contexts). The memory-safety widget's eight rule summaries are paraphrases, not quotes (`src/app/porting/data/memory-safety.json`, `rules`).

### 3.2 Your own prompts quoted in the prose

Lines 23–29, 143, 145, 159, 300, 382, 436, 460, 518 of `content.mdx` quote your prompts with times. Confirm you are happy publishing each verbatim, typos included ("adverserial", "Is works exactly as Ghostty to me").

### 3.3 Regenerated Miri outputs (three-reviewers)

Miri's output for the four replayed bugs was **regenerated on 2026-10-08** against the pre-fix commits (or the in-repo proof harnesses), not copied from the original runs (`research/data/mock-three-reviewers-bugs.json`; builder note in `scripts/porting/widgets/w01_three_reviewers.py`). Check: the caption/terminal title make clear these are re-runs; the Stacked Borrows and Tree Borrows commands shown are the ones that produced the text; bug B's implementer excerpt is the proof-harness model of the first pass (never committed).

## 4. Estimated values

### 4.1 Every `ESTIMATED` fact (33, in `scripts/porting/facts.csv`)

| id                       | shown as       | source            |
| ------------------------ | -------------- | ----------------- |
| cost_paid                | ~$420–$600     | r2-cost.md        |
| cost_paid_central        | about $420     | r2-cost.md        |
| cost_claude_paid         | $361           | r2-cost.md        |
| cost_codex_paid          | $61            | r2-cost.md        |
| cost_api_equiv           | ~$23.8k        | r2-cost.md        |
| cost_api_range           | $19.8k–$31.4k  | r2-cost.md        |
| cost_lost_modeled        | ~$12.8k        | r2-cost.md        |
| cost_lost_range          | $8.7k–$20.3k   | r2-cost.md        |
| leverage                 | about 56×      | r2-cost.md        |
| codex_per_point          | about $22      | r2-codex-audit.md |
| engaged_hours            | about 79 hours | r2-human-story.md |
| rf_ub                    | 37 of 60       | w08_defect_map.py |
| rf_wrong                 | 13             | w08_defect_map.py |
| rf_term                  | 10             | w08_defect_map.py |
| method_read              | 92             | r2-data-fixes.md  |
| method_arith             | 30             | r2-data-fixes.md  |
| method_arith_crash       | 29             | r2-data-fixes.md  |
| method_xplat             | 25             | r2-data-fixes.md  |
| method_xplat_crash       | 6              | r2-data-fixes.md  |
| method_compile           | 17             | r2-data-fixes.md  |
| method_fault             | 12             | r2-data-fixes.md  |
| method_testport          | 11             | r2-data-fixes.md  |
| method_san               | 3              | r2-data-fixes.md  |
| method_types             | 5              | r2-data-fixes.md  |
| method_build             | 4              | r2-data-fixes.md  |
| method_live              | 4              | r2-data-fixes.md  |
| method_withheld          | 2              | r2-data-fixes.md  |
| unsafe_test_share        | about 58%      | codebase.md       |
| unsafe_test_share_strict | 52%            | codebase.md       |
| vt_unsafe_test           | 6,270          | codebase.md       |
| vt_unsafe_nontest        | 1,714          | codebase.md       |
| inspector_unsafe_nontest | 1,147 of 1,155 | codebase.md       |
| vm2_commits              | about 75       | r2-platforms.md   |

Check each against its source and that the page labels it as an estimate where it appears.

### 4.2 Estimated classifications

- [ ] **Defect discovery method** (defect-map "found by (estimated)", `method_*` facts): a hand pass over the 205 ledger entries with a confidence per entry (`research/data/r2-bugs-upstream-method.csv`). Spot-check a sample, especially low-confidence rows.
- [ ] **ReleaseFast consequence 37 / 13 / 10** (of 60 safety-checked crashes: undefined behaviour / wrong result / terminates anyway). Read from each entry's ReleaseFast field. Includes the **UD131 reclassification** (ruling R19): its published ledger text says it still aborts in ReleaseFast, so it moved from UB to "terminates", changing the split from 38/13/9 to 37/13/10. Confirm.
- [ ] **Era classifier** (agents-vs-human eras, moving-target "Rustty commits during the catch-up", strategy-timeline eras): commits are assigned to eras by a classifier over subjects and dates (`research/data/timeline-commit_era_assignment.csv`). The moving-target matrix shows classifier counts next to git-derived counts (they differ for moves 5–7); confirm you accept both.

## 5. Hand-written plain-language texts

- [ ] **Strategy-timeline events** (31 events, 3 eras): each label and sentence was written by hand from the timeline dossier (`EVENTS` in `scripts/porting/widgets/static_charts.py`). Check facts and dates; this pass renamed "Standing upstream resync" → "Regular upstream catch-up", "Advance first" → "Move first", "Seventh reference advance" → "Seventh reference move".
- [ ] **Platform table cells** (`PLATFORM_ROWS` in the same file): yes / partial / no per platform and check, with hand-written tooltip sentences from `research/r2-platforms.md`.
- [ ] **Cut wording for cuts 10 and 12** (cutting-the-cycle, `CUTS` in `scripts/porting/widgets/w04_cutting_the_cycle.py`): cut 10 "renderer → apprt, core … now go through a surface trait"; cut 12 "inspector → core … receives the surface data it reads through a narrow seam, without naming any core type". Confirm both describe the real seams.
- [ ] **"1 unplanned seam" vs five extra seams** (cutting-the-cycle Insights): the insight says one unplanned seam (crash/sentry.zig → Surface.zig became a trait), while the data lists five seams outside the 14 planned cuts (`EXTRA`). Decide the wording: "1 seam to break the last knot" or "5 extra seams, 1 load-bearing".
- [ ] **Moving-target "in plain words" quotes**: each move's check results are restated from the commit message (`ADVANCES[].q` in `w06_moving_target.py`). Moves 1 and 4 were rewritten in this pass from raw commit subjects.

## 6. Figures from mockups not covered by a fact-check

- [ ] **HWASan 9,987** test executions (`san_hwasan`) and **Linux TSan 4,428** (`san_tsan_linux`): taken from the mockup and `r2-platforms.md`, not in BRIEF or bugs.md. Verify against the lane logs at `12752b2ac`.
- [ ] **Codex run-2 figures** (cost widget, `src/components/porting/cost/Codex.tsx`): "about 9.8k calls at high effort, then about 13.2k at medium", "stopped at 99%, with 0.35% of the 3.1M-line scope credited", and the 07-23 date; typed in, not read from data.
- [ ] Other mockup-file sources cited by `facts.csv` (allowed by ruling R23, listed for awareness): zig_tests_first_count, contract_week1_pct, contract_precovered, zig_src_files_first, scc_edges, quirks_importers, scc_files_now, rawptr_allow_fns, port_memsafety_aliasing, port_memsafety_sanitizers, perf_session_effect (+14.9%), churn_terminal_share, and the move2/3/5/7 comment and test counts (from `mockups/06-moving-target.md`).

## 7. Unverifiable prose details

- [ ] Line 147: "On 12 July the Swift app was imported unchanged. **Twenty minutes later**, d1b4df097 …": check the two commit times.
- [ ] Line 60: "On day 3 I asked for AddressSanitizer and LeakSanitizer to complement it, and a sanitizer-matrix gate followed the same day (161178d6a)". The order (request before gate) comes from transcripts; confirm.

## 8. Chart details to eyeball

- [ ] **Codex meter spike to ~90% at 08-01T09** (audit-funnel and cost meters, shared treatment, ruling R21): the hourly min–max band jumps to about 90% in one hour. Confirm it is real (two Codex sessions reading the meter) and not a parsing artefact in `research/data/mock-cost-data.json`.
- [ ] The perf-replay LTO insight no longer gives a whole-suite saving: "2,732 ms" was in no data file. It now says terminal-parser 531 → 322 ms on identical code, and that the fat-LTO build's 14-benchmark suite took 2,948 ms (computed from `perf-r1-stage-chain-lto-profile.csv`). If you have the shipped-profile suite total for `d211a84`, add it to a research data file and cite it.

## 9. Deferred minor findings still open (summary)

From the per-task reviews; none blocks publication, all are polish.

- **Pipeline**: `strip_internal_ids` reports a change on whitespace-only tidies; no full `facts.json` ↔ `facts.csv` sync test; builders write partial output if a later builder fails; `jargon-review.txt` gets appended during test runs; the `find_internal_ids` vs strip round-trip deviates in code-like strings.
- **Verification scripts**: `verify-porting-build.mjs` uses `unified`/`unist-util-visit` transitively (not in `package.json`); prose checks do not strip frontmatter or inline code.
- **Shared UI**: two pinned tooltips can show at once; the tooltip's edge nudge is not recomputed on resize; a few format edge cases (`$1000.0k`, `100.0 ms`, ASCII minus in ratios); `InstrumentCard` drops a numeric 0 headline; the pending placeholder lacks `role="status"`.
- **Widgets**: several files exceed ~300 lines (test-contract model 682, static_charts.py ~550, StaticCharts.module.css ~600, OrchChart 319); some numbers still typed in (perf hazard figures, Codex run-2, 110.6%, "15–18"); tiny tap targets in the defect-map, scatter and phone svg text; no table view for the defect-map arrow navigation; the memory-safety SB/TB toggle is not driven by the models field; the verification-matrix "418 tests" under a fixed mark and the pin-7 Windows wording are judgement calls.
- **Prose**: ~3,660 words in §11–§18 vs the ~3k target (reviewer listed optional cuts); mixed date formats; "the ledger of unsafe safe wrappers" first mention. (Fixed in the final review pass: the 4,277 vs 4,423 scope, "under Stacked Borrows", "(from `&mut`)", the 45.0 commits/day band, now stated in "Never port a bug".)
- **Data notes**: vt detail lists 37 edges (incl. dev) vs the tile's 15 used-by; the max-lag tile date lacks a month name; the binary-name normalisation in perf terminal titles is not disclosed in a method note; the translation-lens ledger text still says "matching the oracle" (quoted ledger row).
