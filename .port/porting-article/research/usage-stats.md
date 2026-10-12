# Usage stats: what the port cost in tokens and agent-hours

Area: usage-stats. Sources are `/Users/gael/projects/rustty-stats` (snapshot generated 2026-10-08T19:50Z, `data/meta.json`) and git history of `/Users/gael/projects/rustty` (HEAD = `phase-8-upstream-resync` @ 1d22ba80d). Everything is restricted to `is_project=1`, meaning the path contains "rustty", worktrees included. Every number can be regenerated with:

```
python3 /Users/gael/projects/rustty/.port/journey-article/research/tmp/usage_stats.py
```

That script is stdlib only and read-only on both repos. Its outputs are `data/usage-stats-*.csv|json` plus `usage-stats-headline.json`. `usage-stats-eras.csv` and `usage-stats-cost-estimate.csv` were produced by a one-off inline python snippet that reads `usage-stats-git-cost-daily.csv` and `prompts.csv`. The snippet's logic is described in the "Eras" section.

Provenance tags:
- MEASURED: computed here; the script is the command.
- DOCUMENTED: read from a repo doc; the path is given.
- ESTIMATED: a modelled value with stated assumptions.

---

## 0. The caveat that frames everything (read first)

- **Mac Claude Code transcripts before 2026-09-08 were lost.** Claude Code deletes `~/.claude/projects` transcripts after about 30 days. DOCUMENTED in `rustty-stats/README.md` under "Lost data".
- **The port started 2026-06-21**, at commit 15c1a9bd7.
- **What survived for 2026-06-21 to 2026-09-07:**
  - Codex on the Mac, which is complete. Its Rustty activity runs 2026-07-15 to 2026-08-02.
  - Claude on the Linux VM `rvm`, which is complete for 2026-07-18 to 2026-08-14.
  - A few Mac Claude calls dated 2026-08-18 and 2026-09-01 to 09-07, found inside files that were touched later.
- **Prompt counts survived for the whole period.** `prompts.csv` (from `history.jsonl`) has timestamps and lengths but no text. There are 511 Mac prompts on Rustty before 2026-09-08 with no recoverable API calls. MEASURED (`prompts_mac_before_2026-09-08`).
- **Every dollar and token total below is a LOWER BOUND.** Charts must show coverage explicitly. Use `usage-stats-coverage-daily.csv`, whose `mac_claude_transcripts` column is `recovered`, `lost` or `none`.
- **Costs are API-equivalent list prices,** not what was billed. The prices are LiteLLM rates in `rustty-stats/rates.json`. All Claude cache writes are priced at the 5-minute rate. Pricing the 1-hour cache-write TTL at its own rate would add $159. DOCUMENTED (README, "Cost method") and MEASURED (`cost_1h_premium_usd`).
- **Timestamps are UTC.** Git commit days are converted to UTC before joining with usage days.

## 1. Headline totals (recovered data, lower bounds)

| metric | value | provenance |
|---|---:|---|
| API calls (Claude, de-duplicated) plus Codex token deltas | 112,324 | MEASURED |
| API-equivalent cost, all sources | **$11,032** | MEASURED; matches `meta.json` `project_cost_usd` 11032.43 (DOCUMENTED) |
| Mac Claude | $7,957 | MEASURED |
| Mac Codex | $2,351 | MEASURED |
| rvm Claude | $724 | MEASURED |
| Estimated lost Mac Claude spend, 06-21 to 08-30 | ~$5.8k (range $4.1k to $8.7k) | ESTIMATED: 1,024 non-merge commits with an AI co-author trailer in the lost window, times $5.67/commit from the recovered window (weekly range $4.0 to $8.5). Low confidence. Independently agrees with README's $4k to $9k, central $6k (DOCUMENTED). |
| Estimated total API-equivalent | **~$17k (range $15k to $20k)** | ESTIMATED (`usage-stats-cost-estimate.csv`) |
| Billable tokens (input + cache read + cache write + output) | 20.43 B | MEASURED |
| Cache-read tokens | 19.71 B (96.5% of tokens) | MEASURED |
| Root sessions | 1,098 (Mac Claude 1,017; rvm Claude 76; Codex 5 root threads) | MEASURED |
| Subagent threads (distinct agent_ids) | 2,344 | MEASURED |
| Claude prompts on Rustty, all time (history.jsonl) | 890 (Mac 725, rvm 165) | MEASURED |
| Median prompt length | 46 characters | MEASURED |

**Prompt counts are small next to API calls.** There are 890 human prompts against more than 112k recovered API calls, and the full history has 2,702 non-merge commits on HEAD.

**Interaction model.** Short human steering ("continue", slash commands, decisions) drives long autonomous orchestrator runs. In the recovered window, each prompt is worth about **$34 of API-equivalent spend** (212 prompts, $7,258) and **6.0 commits** (1,280 non-merge commits).

## 2. Spend by model, and how model choice tracks strategy

Cost by model, all recovered data (MEASURED, `usage-stats-weekly-by-model.csv` / `usage-stats-headline.json`):

| model | cost | first to last day seen (role) |
|---|---:|---|
| claude-opus-5-5 | $4,567 | Mac 09-23 to 10-08. Main orchestrator of phases 6 to 8. Effort "medium". |
| gpt-5.5 (Codex) | $2,014 | 08-01 to 08-02. Effort "high" then "medium". |
| claude-opus-5 | $1,788 | Mac 09-08 to 09-11. Effort "medium". |
| claude-fable-5 | $711 | rvm 07-19 to 23; Mac 09-01 at effort "high" |
| claude-opus-4-8 | $603 | rvm 07-18 to 23 (effort unset); Mac 09-23 |
| claude-opus-4-7 | $510 | Almost entirely `sdk-py` sessions: 1,073 one-prompt **automated security-review** sessions at effort **xhigh** (titles like "exec.rs security review"). Mac sdk-py root sessions cost $495, rvm sdk-py $33. |
| gpt-5.6-sol (Codex) | $336 | 07-15 to 08-01 |
| claude-sonnet-5 | $259 | 09-09 to 09-30 |
| claude-fable-5-1 | $138 | 09-07 |
| claude-sonnet-5-5 | $104 | 09-30 to 10-08 |
| haiku-4-5 / haiku-5-5 / gpt-5.6-terra | about $1 each | |

**Usage data alone cannot test whether model changes line up with strategy shifts.** The Bun-inspired realignment happened on 2026-07-10, inside the lost window. The evidence is:
- commit 2c03357ce: "Bun-rewrite learnings + validation sweep #1 reports, realignment plan"
- commit ccd14200e: PORT_RULES R7 to R12, covering adversarial verification and LIFETIMES
- commit 8f6d781dc: the `GHOSTTY_LIFETIMES.tsv` retroactive pointer-field ownership ledger, "Bun lifetime-classify heritage"

**Git `Co-Authored-By` trailers give a model timeline for the whole port.** MEASURED, `usage-stats-coauthor-model-weekly.csv`:
- **Weeks 06-15 to 07-20: Claude Opus 4.8 (1M context).** It is the dominant model, with 6, 179, 58, 57, 238 and 22 commits per week. Claude Fable 5 appears as a secondary model in weeks 06-29 to 07-20 (7, 67, 2, 1 commits). Fable's peak, 67 commits in week 07-06, is the week of the Bun-learnings realignment and the LIFETIMES ledger.
- **Week 07-27: switch to Claude Opus 5** (326 commits). This is cloud session `session_0185a6...`, which made 334 commits between 07-28 and 07-31 (DOCUMENTED, `rustty-stats/data/commit_sessions.csv`). Codex ran in parallel: gpt-5.6-sol and gpt-5.5 cost $2,347 that week.
- **Week 08-31: Fable 5** (29 commits). Mac usage shows claude-fable-5 at effort "high" on 09-01, costing $561 in one day.
- **Week 09-07: Opus 5** (258 commits).
- **Week 09-21 onward: Claude Opus 5.5**, with 606, 325 and 62 commits per week. Sonnet 5.5 appears in the last two weeks (3 and 22 commits).

**Interpretation, with explicit limits:**
- The post-Bun strategy (realignment on 07-10; Phase 3 orchestration script `wf-realign-phase3`, commit 10ade21c8) did not coincide with a main-model change. Opus 4.8 stayed the main model until 07-27, and Fable 5 was used alongside it during the realignment week.
- The model upgrades (Opus 4.8 to Opus 5 on about 07-28; Opus 5 to Opus 5.5 by 09-23) coincide with execution-heavy bursts rather than with planning or strategy changes.
- Effort settings are only observable from 07-15 onward. "medium" is the default for Opus 5 and 5.5 orchestration. Explicit "xhigh" appears only on the automated security reviewer (Opus 4.7) and "high" on Fable 5 and Codex.
- No effort change is visible at any strategy marker that has data.

## 3. Token mix: why cache reads dominate (MEASURED, `usage-stats-token-mix.csv`)

All sources:

| component | tokens | share of tokens | cost | share of cost |
|---|---:|---:|---:|---:|
| cache read | 19.71 B | 96.5% | ~$6.07k | **55.0%** |
| cache write | 0.518 B | 2.5% | ~$2.90k | 26.3% |
| uncached input | 0.140 B | 0.7% | ~$0.66k | ~6% |
| output | 0.061 B | 0.3% | ~$1.40k | 12.7% |
| thinking or reasoning (subset of output) | 16.8 M | n/a | n/a | n/a |

**Cache hit rates** are computed as cache_read / (cache_read + cache_write + uncached input):

| source | cache hit rate |
|---|---:|
| Mac Claude | 97.0% |
| rvm Claude | 96.8% |
| Codex | 95.3% |

**Thinking share of output:**

| source | thinking / output |
|---|---:|
| Mac Claude | 0.33 |
| Codex reasoning | 0.17 |
| rvm Claude | 0, because the older CLI does not log thinking |

**Implication for the article.** The cost is dominated by long-context agents repeatedly re-reading cached state, which is where 55% of the dollars go. Generated tokens are a small part of the bill.

**Context size per call** (`usage-stats-context-size-weekly.csv`, Claude):

| week | median context | p90 context |
|---|---:|---:|
| 07-13 | 77k | 425k |
| 09-21 (orchestrator era) | 204k | 523k |

The maximum is about 967k, which approaches the 1M-context window.

## 4. Who spends it: main thread vs subagents vs Workflow runs (MEASURED, `usage-stats-agent-share.csv`)

| tool / kind | calls | cost | share of tool cost |
|---|---:|---:|---:|
| Claude main | 11,199 | $1,760 | 20.3% |
| Claude subagents (Agent tool) | 63,268 | $5,993 | **69.0%** |
| Claude Workflow-script runs (`wf_*`) | 9,395 | $929 | 10.7% |
| Codex main | 9,124 | $930 | 39.5% |
| Codex subagents | 19,338 | $1,421 | 60.5% |
| **All: non-main (subagents + workflows)** | | | **75.6%** |

Notes:
- Only 10 distinct Workflow runs are tagged with a `workflow_id`. The field only exists in recent Claude Code transcripts, so this is a lower bound.
- The repo has committed Workflow scripts since 2026-06-22 (57e999739, "vt-3c methods-complete recovery workflow script"; DOCUMENTED in git).
- Spend concentrates in a few orchestrator sessions. The top two Mac Claude sessions account for **67.3% of all recovered spend** (`usage-stats-top-sessions.csv`):

| session | cost | active hours | calls | subagent files | compactions | span |
|---|---:|---:|---:|---:|---:|---|
| "Rustty port upstream changes" | $4,524 | 70.2 | 43,014 | 1,636 | 3 | 09-01 to 09-26 |
| "... (2)" | $2,904 | 65.4 | 30,045 | 354 | 7 | 09-24 to 10-08 |

## 5. Time: sessions, active hours, parallelism (MEASURED, `usage-stats-activity-daily.csv`, `usage-stats-sessions-daily.csv`)

- **Busy wall-clock hours** are 196 h. This is the union of all Rustty API-call timestamps per source and day, counting gaps of 5 minutes or less.
- **Agent-thread hours** are 391 h, the same measure summed per thread (session plus agent_id).
- **Mean concurrency is about 2.0.** Peaks:

| date | source | concurrency | threads |
|---|---|---:|---:|
| 2026-09-07 | Mac Claude | 8.5 | 75 |
| 2026-09-08 | Mac Claude | 7.4 | 452 |
| 2026-09-01 | Mac Claude | 4.4 | n/a |
| 2026-08-01 | Codex | 3.0 | 773 |

- **Per-root-session active hours** sum to 228 h, and context compactions total 15 (`usage-stats-headline.json`).
- **Busiest recovered days by busy hours:**

| date | source | busy hours |
|---|---|---:|
| 2026-08-01 | Codex | 23.1 |
| 2026-07-19 | rvm | 17.3 |
| 2026-09-24 | Mac Claude | 15.4 |
| 2026-09-11 | Mac Claude | 14.4 |

- **Prompt heatmap:** `usage-stats-prompts-heatmap-pt.csv` gives weekday × hour in America/Los_Angeles for all 890 prompts.

## 6. Cost per unit of output (recovered window only, 2026-09-08 to 2026-10-08)

MEASURED from `usage-stats-git-cost-daily.csv` and `usage-stats-weekly.csv`. Commits are non-merge commits on HEAD. "Code lines" means `.rs` plus other code (Swift, C, Python, shell, JS and similar), excluding `*/vendor/*`, `.port/`, `docs/` and `records/`.

| measure | value | notes |
|---|---:|---|
| Cost | $7,258 | |
| Non-merge commits | 1,280 | |
| $ per commit | **$5.67** | weekly: 09-07 $8.47, 09-21 $4.01, 09-28 $7.03, 10-05 $5.63 |
| Code lines added | 400,813 | |
| $ per 1k code lines added | **$18.1** | weekly range $8.7 to $62.9 |
| Prompts | 212 | |
| $ per prompt | **$34.2** | |

**Caveat on lines added.** Week 09-28 adds 264k code lines but deletes 229k. That week is mostly P7-W13 mechanical file splits. For example, "split: rustty-vt terminal.rs into 19 topic files (P7-W13 A)" shows +22,695/-22,663. Lines added per dollar is therefore not a productivity measure, so use $/commit as the primary unit.

Whole-history git volume on HEAD (MEASURED):
- 2,702 non-merge commits and 266 merge commits.
- Rust: +1,151,213 / -313,599 lines.
- Other code: +75,872 lines.
- Vendored C/C++ under `crates-sys/*/vendor`: +1,558,561 lines.
- `.port/` and docs records: +690,599 lines, including UPSTREAM_DIFF.patch files of 38k to 101k lines each.

## 7. Eras (MEASURED except cost, which is a lower bound; `usage-stats-eras.csv`)

| era | dates | non-merge commits | merges | code lines added | records/docs lines added | prompts | commits per prompt | recovered cost |
|---|---|---:|---:|---:|---:|---:|---:|---:|
| E1 pre-realignment | 06-21 to 07-09 | 445 | 0 | 417k | 80k | 113 | 3.9 | $0 (lost) |
| E2 post-Bun rules + LIFETIMES, phases 3 to 5, incl. VM + Codex | 07-10 to 08-31 | 947 | 107 | 403k | 417k | 551 | 1.7 | $3,076 (VM + Codex + one Sep-1 Fable day; Mac Claude lost) |
| E3 upstream resyncs, Opus 5/5.5 orchestrator, phases 6 to 8 | 09-01 to 10-08 | 1,310 | 159 | 407k | 193k | 226 | 5.8 | $7,957 |

**What changes between eras:**
- **The records-to-code ratio rises from 0.19 (E1) to 1.04 (E2).** That is consistent with the post-Bun process adding ledgers, audits, adversarial verification and LIFETIMES. It is partly inflated by UPSTREAM_DIFF.patch records.
- **The ratio drops to 0.47 in E3.**
- **Merge commits** appear from E2 on (worktree-per-agent integration) and keep growing in E3.
- **Commits per human prompt** go from 1.7 in E2 to 5.8 in E3. That reflects longer autonomous orchestrator runs.
- **Caveat:** E2 prompts include the 165 rvm prompts. Codex prompts are not in `prompts.csv`.

## 8. Strategy markers used for annotations (`usage-stats-strategy-markers.json`)

| date | commit | marker |
|---|---|---|
| 06-21 | 15c1a9bd7 | Port starts |
| 06-21 | d9fd637ce | Miri adopted as the primary memory, UB and leak gate (G6) |
| 06-22 | 57e999739 | First Workflow script |
| 07-10 | 2c03357ce | Bun learnings |
| 07-10 | ccd14200e | PORT_RULES R7 to R12 |
| 07-10 | 8f6d781dc | LIFETIMES ledger |
| 07-15 to 08-02 | (usage data) | Codex burst |
| 07-18 to 08-14 | (usage data) | VM sessions |
| ~07-28 | (trailers) | Opus 4.8 to Opus 5 |
| 09-01 | 62c1690d9 | Phase 6 pin advance 88f57ee66 to 20abdb50a |
| 09-28 | 9f007b10e | Phase 7 pin advance to 12752b2ac |
| 10-07 | 7719ebb4d | Phase 8 pin advance to a60e9e2a5 |

## 9. Proposed visualizations

1. **Coverage strip plus prompts per day**
   - Chart: stacked bar of prompts per day (Mac, rvm), with a background band coloured by Mac Claude transcript status (lost, recovered).
   - Data: `usage-stats-coverage-daily.csv`.
   - Why: it is the honesty chart. It shows exactly where dollar figures are missing while human activity is still visible. It should come first.
2. **Weekly API-equivalent cost by source**
   - Chart: stacked bars for Mac Claude, Codex and rvm Claude, with hatched "estimated lost" bars in the lost weeks, plus strategy-marker annotations.
   - Data: `usage-stats-weekly.csv`, `usage-stats-cost-estimate.csv`, `usage-stats-strategy-markers.json`.
3. **Model timeline**
   - Chart: stacked area or stream of weekly commits by `Co-Authored-By` model, across the full port.
   - Data: `usage-stats-coauthor-model-weekly.csv`, with Bun and LIFETIMES markers.
   - Why: it is the only model signal that spans the lost window. It shows Opus 4.8 to Opus 5 to Opus 5.5, with Fable as a side model.
4. **Where the tokens vs the dollars go**
   - Chart: two 100% bars (tokens and cost) by component.
   - Data: `usage-stats-token-mix.csv`.
   - Why: 96.5% of tokens are cache reads but they are 55% of the cost. Output is 0.3% of tokens and 12.7% of the cost. This tells technical readers that long-context re-reading is the bill.
5. **Main vs subagent vs Workflow share**
   - Chart: 100% bars per tool; weekly variant from `usage-stats-agent-share-weekly.csv`.
   - Data: `usage-stats-agent-share.csv`.
   - Why: about three quarters of spend is delegated work.
6. **Concurrency**
   - Chart: per-day dumbbell or bar of busy wall-clock hours vs agent-thread hours, with the ratio as a line.
   - Data: `usage-stats-activity-daily.csv`.
   - Why: it shows the parallel-agent fan-out, up to 8.5×.
7. **Context-size creep**
   - Chart: weekly median and p90 context tokens per call (line with band).
   - Data: `usage-stats-context-size-weekly.csv`.
   - Why: it shows the median doubling into the orchestrator era and p90 near 0.5M.
8. **Cost efficiency**
   - Chart: $/commit and $/prompt per recovered week (dot plot), with the file-split week flagged.
   - Data: `usage-stats-weekly.csv`.
9. **Eras comparison**
   - Chart: small multiples of commits per prompt, records-to-code ratio and merges.
   - Data: `usage-stats-eras.csv`.
   - Why: it is the quantitative footprint of the Bun-inspired process change.
10. **Prompt heatmap**
    - Chart: weekday × hour (PT).
    - Data: `usage-stats-prompts-heatmap-pt.csv`.
    - Why: it shows when the human was in the loop. Optional.
11. **Top sessions table or bar**
    - Data: `usage-stats-top-sessions.csv`.
    - Why: two orchestrator sessions are 67% of recovered spend.

## 10. Gaps

- **Mac Claude tokens and cost for 06-21 to 09-07 are lost,** apart from 08-18 and 08-31 to 09-07 remnants. They cannot be recovered; only commit-based estimates are possible.
- **There is no effort or model telemetry around the 07-10 Bun realignment.** The commit trailer model is the only signal, and it carries no effort.
- **`workflow_id` tagging only exists in recent transcripts.** Workflow runs before that are counted as subagents or main.
- **Codex prompts are not in `prompts.csv`,** so era prompt counts are Claude-only.
- **rwin (Windows VM) has no usage data** (DOCUMENTED, README).
- **Cloud sessions (claude.ai/code) carry no token data.** The 14 `Claude-Session` trailers only identify sessions (DOCUMENTED, `rustty-stats/data/commit_sessions.csv`).
- **Commit-to-cost joins are by UTC day.** Agents commit in bursts after work, so daily $/commit is noisy; weekly or window aggregates are more reliable.
- **Merged-LOC weighting is noisy.** Mechanical file splits (P7-W13) and vendored or patch records inflate line counts. The line categories are a path heuristic.

## Fact-check

Independent re-derivation on 2026-10-08. I did not run `usage_stats.py`. I recomputed from `rustty-stats/data/messages.csv.gz`, `sessions.csv` and `prompts.csv` (is_project=1) and from `git log 1d22ba80d --no-merges`, parsing trailers with `%(trailers:key=Co-Authored-By,valueonly)` instead of the dossier's regex.

| # | fact | verdict | evidence / correction |
|---|---|---|---|
| 1 | Recovered cost $11,032 (Mac Claude $7,957; Codex $2,351; rvm $724) | confirmed | Sum of `cost_usd` over 112,324 rows is $11,032.43; per source 7,957.09 / 2,350.99 / 724.35. |
| 2 | Lost window ~$5.8k, total ~$17k | **corrected** | The dossier's trailer regex requires `<email>` after the name. It misses 180 trailers with no email, such as `Co-Authored-By: Claude Opus 4.8 (1M context)` on claude.ai/code session commits (e.g. efb4daeda). The true count of non-merge commits with an AI trailer before 2026-08-31 (author date, UTC) is **1,195**, not 1,024. At $5.67 per commit (range $4.0 to $8.5) that gives **~$6.8k (range $4.8k to $10.2k)**, for a total of **~$17.8k (range ~$15.8k to ~$21.2k)**. The estimate is still rough. It double-counts rvm-made commits whose cost is already recovered (07-18 to 08-14), and it treats cloud-session commits as Mac spend. |
| 3 | Cache reads 96.5% of 20.43B tokens, 55.0% of cost; output 0.3% of tokens, 12.7% of cost | confirmed | 20.433B tokens. cache_read 96.48%. Cost shares from the per-row `cost_*_usd` columns: cache read 55.03%, output 12.73%, cache write 26.26%, input 5.98%. |
| 4 | Non-main share 75.6% (Claude sub 69.0% + wf 10.7%; Codex sub 60.5%) | confirmed | Classified by `workflow_id` non-empty → wf, else `is_subagent=1` → sub: 75.62%. Claude main/sub/wf = 20.3/69.0/10.7%; Codex sub 60.5%. |
| 5 | Top two sessions 67.3% ($4,524, 70.2 h, 1,636 subagent files; $2,904, 65.4 h) | confirmed | `sessions.csv` roots: $4,523.63 / 70.20 h / 43,014 calls / 1,636 files (09-01 to 09-26); $2,904.08 / 65.42 h (09-24 to 10-08); share 67.33%. Nuance: the first session also used Fable 5, 5.1, Opus 4.8 and Sonnet 5, so "Opus 5/5.5" describes its main model only. |
| 6 | $5.67/commit, $34.2/prompt, $18.1 per 1k code lines | confirmed (prompt figure minor) | $7,258.00 / 1,280 commits = $5.67. Code lines added are 400,813, giving $18.1 per 1k lines. `prompts.csv` has **214** prompts dated 09-08 to 10-08, not 212, which gives **$33.9 per prompt**. The dossier's daily join drops prompts on days that have neither commits nor cost. |
| 7 | 890 prompts, median 46 chars; 112,324 calls; 511 Mac prompts in lost window | confirmed | 890 (Mac 725, rvm 165), median 46.0. 511 Mac prompts are dated before 09-08. 112,324 rows. |
| 8 | Commits per prompt E1 3.9 / E2 1.7 / E3 5.8 | confirmed | Using author date UTC: 445/113 = 3.94, 947/551 = 1.72, 1310/226 = 5.80. |
| 9 | Records/code ratio 0.19 → 1.04 → 0.47 | confirmed, with a sharpened caveat | 0.192 / 1.036 / 0.474. Excluding `UPSTREAM_DIFF*` patch files (232k lines in E2, 81k in E3), the ratios are **0.19 → 0.46 → 0.28**. The "jump" is therefore mostly from patch records, though a real rise remains. |
| 10 | Trailer model timeline | confirmed, but the weekly counts in §2 are corrected | The phases hold: Opus 4.8 through week 07-20, Fable 5 at 67 in week 07-06, Opus 5 from week 07-27, Opus 5.5 from week 09-21, and Sonnet 5.5 in the last two weeks (3 and 22). The regex bug undercounts Opus 4.8. True weekly counts are **6, 287, 125, 69, 238, 22**, not 6/179/58/57/238/22. In week 07-06, Opus 4.8 (69) slightly outnumbers Fable 5 (67). Week 07-27 Opus 5 is 317 by `%(trailers)`, against the dossier's 326. Fable 5 is also the only trailer model in weeks 08-17 (10) and 08-31 (29). |
| 11 | 391 h thread vs 196 h busy; peak 8.5× on 09-07; 452 threads on 09-08 | **corrected (peak)** | Recomputed with the 5-minute gap rule: 390.8 h vs 196.1 h. The 8.45× on 09-07 rests on only 0.25 busy hours and 75 threads, so it is a small-denominator artifact. The meaningful peak is **7.4× on 09-08** (1.15 busy h, 8.5 thread h, 452 threads). Next are 4.4× on 09-01 and 3.0× for Codex on 08-01 (773 threads). |
| 12 | Cache hit rates 97.0 / 96.8 / 95.3% | confirmed | 97.03 / 96.81 / 95.33%. |
| 13 | Median context 77k (wk 07-13) → 204k (wk 09-21); p90 0.4-0.5M; max ~967k | **corrected (max)** | Medians are 76.9k and 204.0k; p90 is 426k and 523k. The overall maximum is **~999k, in week 07-13**. The ~967k maximum applies only to the orchestrator weeks 09-07 to 09-28. |
| 14 | 1,073 one-prompt sdk-py sessions, Opus 4.7 xhigh, ~$528 | **corrected (composition)** | There are 1,073 sdk-py root sessions costing $528.0 in total. Of these, 977 are Opus 4.7 at xhigh (Mac 911, rvm 66; $509.9) and **96 are Opus 5.5 at medium ($18.1)**. 1,063 have one prompt and 10 have two. |

Required text changes:
- Use 1,195 and ~$6.8k / ~$17.8k for the lost-window estimate.
- Fix the Opus 4.8 weekly counts in §2.
- Quote the concurrency peak as 7.4× on 09-08.
- Give the context maximum as ~999k.
- Describe the security reviewer as mostly Opus 4.7 xhigh.
- Use $33.9 per prompt with 214 prompts.
