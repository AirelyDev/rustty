# How long without usage limits? (2026-10-11)

Question from the author: the port ran 2026-06-21 → 2026-10-10 (the article's "112 days"). How long
would it have taken in elapsed time if subscription limits had not paced it?

**Answer: about 73–78 days (central), within an outer range of 60–97 days.** That is a calendar end
around **1–6 September** (central), with an outer range of **19 August – 25 September**, instead of
10 October. Only the conservative end (94–97 days, end ~22–25 Sept) rests on idle time that the
author tied to a limit in writing. The central estimate also counts stretches whose timing points to
the weekly reset but whose prompts do not mention a limit.

Per-day data: `data/elapsed-without-limits-2026-10-11.csv` (113 rows, 06-20 → 10-10; 06-20 is the
pre-repository founding night). Scripts were run from `/tmp/elapsed_scripts/` (not kept in the repo).

## 1. Sources and how each column was built

| column | source | notes |
|---|---|---|
| `commits` | `git log --all` (3,135 unique commits), author date converted to Pacific | includes agent and human commits |
| `prompts` | `data/prompts-raw-2026-10-11.jsonl` (1,023 prompts: Mac + VM Claude, Codex) | includes commands |
| `api_calls` | `rustty-stats/data/messages.csv.gz`, rows with `is_project=1` or a Rustty cwd | **Mac Claude before 09-08 is lost**: only VM Claude (07-18 → 08-14) and Codex (07-15 → 08-02) are present before 09-08 |
| `active_hours` | same rows: count of 5-minute buckets with ≥1 Rustty model call × 5/60 | measures machine activity, not human time. Same loss window |
| `limit_evidence` | `r2-human-story-limit-mentions.csv`, `r2-cost-limit-events-2026-10-10.csv`, Codex `rate_limit_used_pct`, plus resume timing (below) | Rustty-only codes. Other projects' limit mentions appear only as a flag, with no names |
| `other_project_prompts_count` | `~/.claude/history.jsonl` (project ≠ Rustty) + `~/.codex/history.jsonl` mapped to session cwd | counts only. No names or text were read into the outputs |
| `class` | hand classification per stretch (section 3) | |

**Weekly-reset anchor.** The Claude Max weekly window reset on **Tuesdays at 15:00 Pacific**
(22:00 UTC). This rests on the 10-02 limit notice ("resets Oct 6 at 3pm", a Tuesday) and the
quota-week starts in `r2-cost-limit-events`. The resumes on 06-30, 07-07 and 07-14 confirm it held
in June and July too.

## 2. Day-by-day picture

- 112 days from first commit (06-21 15:36) to snapshot (10-10). Commits landed on **59** of them,
  and on **47** of them with ≥10 commits (median 51 commits on those days).
- Commit gaps of ≥12 h add up to **74.9 days** of the 111.3-day span. Commits were flowing (no gap
  over 12 h) for about **36 days' worth** of wall-clock time.
- The idle stretches stop and start sharply. Commits stop within minutes of a pause or limit prompt,
  and restart within minutes of the resume prompt (for example 07-23: last commit 14:41, pause
  prompt 14:53; 09-28: last commit 21:47, pause prompt 21:48).

### The weekly-reset signature (strongest single piece of evidence)

There are 22 commit gaps of ≥12 h. In **7 of them, the first commit after the gap lands 0–5 h
after a Tuesday 15:00 reset**, and an eighth resume prompt lands 4 h after one:

| gap (Pacific) | first commit after | hours after Tue 15:00 reset |
|---|---|---|
| 06-26 09:30 → 06-30 | 06-30 19:02 | 4.0 |
| 07-04 08:43 → 07-07 | 07-07 18:46 | 3.8 |
| 07-12 16:41 → 07-14 | 07-14 15:02 | 0.03 |
| 07-23 14:41 → 07-28 | 07-28 20:02 | 5.0 |
| 09-06 19:37 → 09-08 | 09-08 16:42 | 1.7 |
| 09-17 21:24 → 09-22 | 09-22 18:43 | 3.7 |
| 09-28 21:47 → 09-29 | 09-29 15:45 | 0.75 |
| 10-02 15:48 → 10-06 | 10-06 23:15 (prompt 19:03) | 8.3 (prompt 4.0) |

A 5-hour window is 3% of a week, so chance alone would put about 0.7 of 22 resumes in it, not 7–8.
These gaps ended because the weekly quota came back.

## 3. Stretches and classification

Classes:
- **(a) limit**: the author's own prompt ties the stop to a limit, or a limit notice fires, and the
  resume timing agrees.
- **(a?) probable limit**: the resume falls right after a weekly reset, or the author paused Rustty
  to spend the shared quota elsewhere, but no prompt names a limit as the reason for that stretch.
- **(b) other project**: heavy other-project prompting with no limit link. Never removed.
- **(c) unknown**.

"Lost" is measured on commit timestamps, from the moment work was blocked to the first commit after.
Each gap is shown both raw and minus a **12 h allowance**, because gaps of up to 12 h occur inside
normal active runs (nights, waiting for review). The two values bound each estimate.

| # | stretch (Pacific) | gap h | class | evidence summary | other-proj prompts | lost days (−12 h … raw) |
|---|---|---|---|---|---|---|
| 1 | 06-26 09:30 → 06-30 19:02 | 105.5 | a | pause prompt, "approaching my token limit" (06-26). Resume "token limit is reset" 4 h after Tue reset | 20 | 3.9 … 4.4 |
| 2 | 07-04 08:43 → 07-07 18:46 | 82.0 | a | "I am out of token usage" (07-04). Resume 3.8 h after reset. No other activity | 0 | 2.9 … 3.4 |
| 3 | 07-12 16:41 → 07-14 15:02 | 46.3 | a | 07-13 `/rate-limit-options`, `/usage-credits` ×2, plan writing only. Resume 2 min after reset | 0 | 1.4 … 1.9 |
| 4 | 07-19 16:04 → 07-22 21:48 | 77.7 | c | VM GTK work. "my computer crashed" (07-22), Cursor/IDE crashes. 1 commit 07-20. No limit language | 7 | (2.7 … 3.2) |
| 5 | 07-23 14:41 → 07-28 20:02 | 125.3 | a? | Pause prompt with no limit language ("resume when I say so"). Resume 5 h after reset. Other-project limit mention on 07-28 | 22 | 4.7 … 5.2 |
| — | 07-31 → 08-02 07:00 | — | active (Codex) | Codex audit ran (21k calls on 08-01), ended as the Codex weekly meter reached 99% (08-02) | — | 0 |
| 6 | 08-02 → 08-10 13:13 | ~198 | b | 152 other-project prompts. Rustty resumed by handing Codex findings to Claude | 152 | — |
| 7 | 08-10 13:13 → 08-11 22:03 | 32.8 | a? | Fable allotment ran out ("ran out of Fable usage", 08-11), switch to Opus 5. Commit 7 h after reset | 16 | 0.9 … 1.4 |
| 8 | 08-11 22:03 → 08-17 22:27 | 144.4 | b | 165 other-project prompts. Rustty only disk cleanup | 165 | — |
| 9 | 08-18 14:21 → 08-25 11:25 | 165.1 | mixed a?/b | 08-25 resume says the workflow was "interrupted when I ran our of usage credits". Commits stopped 39 min before the 08-18 reset, yet 112 other-project prompts followed that week. **Half counted as a?** | 112 | 3.2 … 3.4 (half) |
| 10 | 08-26 21:40 → 08-28 22:17 | 48.6 | c | no limit language, 0 other-project prompts on 08-27 | 1 | (1.5 … 2.0) |
| 11 | 08-28 22:17 → 09-01 01:35 | 75.3 | c | local GitHub Actions run, light other-project use | 23 | (2.6 … 3.1) |
| 12a | 09-01 07:51 → ~09-05 00:00 | ~88 | c | 09-01 12:07 "interrupted due to my session limits expiring" (a 5 h limit, and the weekly reset was 15:00 that day, so it explains hours, not days) | 36 | (3.2 … 3.7) |
| 12b | 09-05 → 09-07 07:53 | ~56 | b | 328 other-project prompts (new app started 09-05) | 328 | — |
| 13 | 09-07 07:53 → 09-08 16:42 | 32.8 | a | Rustty resume attempts 09-07 and 09-08 both cite session limits, `usage_credits_exhausted` 09-08, resume 1.7 h after reset | (in 12b) | 0.9 … 1.4 |
| 14 | 09-11 15:58 → 09-22 18:43 | 261 (4 commits 09-17) | a? | "I want to focus tokens in something else" (09-11 pause prompt). Other project hit its own limit 09-14/15. Resume 3.7 h after reset | 501 | 10.4 … 10.9 |
| 15 | 09-24 08:36 → 09-26 11:28 | 50.9 | a | "we're nearing limits. I'll continue post-reset" | 52 | 1.6 … 2.1 |
| 16 | 09-28 21:47 → 09-29 15:45 | 18.0 | a | "pause ... since we're approching usage limits". Resume 45 min after reset | 34 | 0.25 … 0.75 |
| 17 | 10-02 15:48 → 10-06 23:15 | 103.4 | a | weekly-limit notice "resets Oct 6 at 3pm". 0 Claude prompts in any project 10-03..05. "previously hit the session limit" (10-06) | 17 | 3.8 … 4.3 |

Normal gaps (<24 h, inside active runs: 07-11/12, 09-09, 09-26/27) are not counted.

Totals (lost days, −12 h allowance … raw):
- **a: 14.8 … 18.3 days** (stretches 1, 2, 3, 13, 15, 16, 17)
- **a?: 19.2 … 20.9 days** (5, 7, 14, half of 9). Stretch 14 alone is 10.4–10.9.
- **c: 10.0 … 12.0 days** (4, 10, 11, 12a)
- **b: ~21 days** (6, 8, 12b, half of 9). Never removed.

## 4. Estimates

| scenario | removes | elapsed days | calendar end (from 06-21) |
|---|---|---|---|
| Conservative | a | **94–97** | ~22–25 Sept |
| Central | a + a? | **73–78** | ~1–6 Sept |
| Upper (fastest) | a + a? + c (+ ~1 day limit-paced slowdown 09-29..10-02) | **60–68** | ~19–27 Aug |
| Floor, for context only | every gap ≥12 h, other projects included | ~36 commits-flowing days, ~59 commit days | not a counterfactual |

Assumption, in one reader-facing sentence: *the estimate removes the idle stretches that usage
limits caused and keeps everything else, including the time I spent on other projects, the speed of
the active days, and the order of the work.*

Further assumptions:
- Throughput on active days is unchanged. Limits probably also capped concurrency on active days
  (09-28 "dont start new work/sugagents"; model switches when an allotment ran out). That is not
  measurable, so it is left out and the estimates lean long. Throughput had other ceilings too: on
  09-23 seven building worktrees filled the disk (now ≤3 at once), long CI and Miri runs, and the
  author's own review time. Large further compression is unlikely.
- A counterfactual end date does not move the upstream target. Two upstream catch-ups (Phase 6,
  09-01; Phase 7, 09-28) landed late in the real timeline. In a shorter calendar there would be less
  upstream drift but the same catch-up work, so this is ignored.
- Partial days at stretch boundaries are handled by the hour-level gaps, not by whole days.

### Critical-path / intensity view

- The port was running (≥1 commit) on 59 of 112 days (53%), about 3.7 days a week.
- In the recovered window (09-08 → 10-08, the only window with complete Mac Claude telemetry),
  Rustty's list-price usage was **~$7,260 over 31 days: ~$234 per calendar day, ~$403 per active
  day** (18 active days). This is my attribution from `messages.csv.gz` (is_project or Rustty cwd).
  r2-cost measured $6,539 of Rustty Mac Claude over its four complete quota weeks, so the two agree
  in size but need reconciling before either is published.
- Reading against r2-cost: a maxed-out week was ~$2,769 at list price across all projects (central;
  range $1,974–$4,262), and Rustty's share was ~54%. Rustty's half of a week therefore buys about
  **3.4 active days** at the port's active-day burn of ~$403. The port actually ran ~3.7–4 days a
  week. **The subscription's weekly allowance and the port's running days match closely**, which is
  the mechanism behind the estimate.
- Whole port: ~$23.8k API-equivalent (r2-cost) over 112 calendar days = ~$212/day. Over 59 commit
  days = ~$400/day, the same order as the recovered active-day rate. Treat this as consistent, not
  as an independent measurement, since the lost-window figure was modelled from the same ceilings.

## 5. Sensitivity and confidence

- **The estimate turns on stretch 14 (09-11 → 09-22, ~10.5 days).** The author paused Rustty "to
  focus tokens in something else", which is a limit-driven reallocation of a shared quota. It is
  also a choice to do other work, and 501 other-project prompts were sent in it. Moving it from a?
  to b shifts the central estimate from 73–78 to 84–89 days. Its well-documented counterpart is
  stretch 9 (half counted).
- **Shared quota.** Several b stretches used the same weekly quota (08-18 → 08-25, 09-05 → 09-07).
  With no limits, Rustty workflows could have run alongside the other projects. The brief says b is
  never removed, so this is not counted. If it were, the fastest case nears the ~59 commit-day
  floor.
- **12 h allowance vs raw gaps**: ±2–5 days per scenario. This sets the width of each range.
- **Lost transcripts before 09-08.** They reduce confidence in the June–August classification only
  a little. That classification rests on prompt history (intact), git (intact), Codex logs (intact)
  and the VM transcripts (intact). The loss removes per-call usage and Claude's own limit
  notices (the `session`/`weekly` events exist only from 09-01). So June–August (a) calls rest on
  the author's typed prompts plus the reset timing, and there could be more limit stretches hidden
  in the c class (stretches 10, 11, 12a are all in or near the lost window). Confidence:
  conservative **high**, central **medium**, upper **low**.
- The "112 days" headline counts the founding commit day. Using the 111.3-day span changes nothing
  material.

## 6. Placement in the article

`content.mdx` opens with "done over <Fact days_total/>", and the stats-card "days" tile carries the
note "Every day counted, including the quiet ones." The reader takes that as the port's duration.
The analysis qualifies it, so the short version should sit **near the headline** and the method in
**"What it cost"**, where limits and the maxed-out week are already explained.

1. **Stats-card tile (recommended, short).** Keep `days_total` as the value. Change the note to:
   "Every day counted. Commits landed on 59 of them. Without usage limits, an estimated 73–97 days."
   (facts: `commit_days`, `days_no_limits_range`). A tile note has no room for the assumption, so
   link it to the cost section.
2. **"At a glance" (one sentence, recommended)**, after the scope paragraph:
   > The port ran on one Claude subscription, alongside other projects, and its pace was set by the
   > subscription's weekly limit. Agents committed on <Fact id="commit_days"/> of the
   > <Fact id="days_total"/>. Without the limits, the same work would have taken an estimated
   > <Fact id="days_no_limits_range"/> (see [What it cost](#what-it-cost)).
   The opening line can stay as is. Changing it to "done over 112 days, about 59 of them active"
   is the alternative if the author wants the qualification in the first sentence.
3. **"What it cost" (fuller paragraph, recommended)**, after the API-equivalent paragraph and
   before the cost widget. It explains the price of the 56× leverage: the subscription bought list
   price tokens cheaply by spreading them over the calendar. Draft:

   > The subscription was cheap partly because it set the pace. When the port was running, it used
   > about <Fact id="api_per_active_day"/> a day at list price. My share of a maxed-out week
   > covered about <Fact id="active_days_per_week_budget"/> such days, and the port ran on about
   > <Fact id="commit_days_per_week"/> days a week. The commit history shows the seam. Of the
   > <Fact id="gaps_12h"/> pauses longer than 12 hours, <Fact id="resumes_after_reset"/> ended
   > within five hours of the weekly reset, Tuesday at 3 p.m. Pacific. Several began with a prompt
   > such as "Pause the work for now, we're nearing limits." Removing the stretches that limits
   > caused, and keeping everything else, including the weeks I spent on other projects, gives an
   > estimate of <Fact id="days_no_limits_central"/> instead of <Fact id="days_total"/>. Counting
   > only the stretches I tied to a limit in writing gives <Fact id="days_no_limits_conservative"/>.
   > The other stretches might have been limits too or might not, so the fastest case,
   > <Fact id="days_no_limits_upper"/>, is a bound, not a claim.

   (Quote is 9 words, verbatim, not on the redaction list.)
4. **Not recommended as the main home:** "The first three weeks" (only stretches 1–3 are there; it
   already mentions limits), "How the orchestration changed" (about structure, not pace). The
   agents-vs-human widget's `humanEras` would be the wrong frame, but see the visual below.

**Visual (worth it, small).** `agents-vs-human.json` already carries a `quiet` array of bands with
notes. Colour those bands by class (limit / probable limit / other project / unknown) from the
`class` column of the CSV, and add a one-line legend: "limit-paced: X days · other projects: Y
days". That is a data and colour change on an existing widget, not a new chart. A separate
"counterfactual end date" band on a timeline is not recommended, because it reads as more precise
than a range.

## 7. Proposed facts

| id | value | status | source |
|---|---|---|---|
| `commit_days` | 59 | MEASURED | git log --all, Pacific dates, 06-21..10-10 (this CSV) |
| `commit_days_10plus` | 47 | MEASURED | same |
| `commit_days_per_week` | about 3.7 | MEASURED | 59 / 16 weeks |
| `gaps_12h` | 22 | MEASURED | git commit gaps ≥12 h |
| `resumes_after_reset` | 7 (8 counting the 10-06 resume prompt) | MEASURED | gap table, section 2; reset Tue 15:00 PT from r2-cost-limit-events |
| `flowing_days` | about 36 days | MEASURED | span minus gaps ≥12 h |
| `api_per_active_day` | about $400 | ESTIMATED | messages.csv.gz 09-08..10-08, Rustty attribution, $7,258 / 18 days; reconcile with r2-cost $6,539/4 weeks |
| `api_per_calendar_day` | about $230 (recovered window) / about $210 (whole port) | ESTIMATED | same; cost_api_equiv / 112 |
| `active_days_per_week_budget` | about 3½ | ESTIMATED | cost_ceiling_central × rustty_share_measured / api_per_active_day |
| `limit_days_explicit` | 15–18 days | ESTIMATED | section 3, class a |
| `limit_days_probable` | 19–21 days | ESTIMATED | section 3, class a? |
| `days_no_limits_conservative` | 94–97 days | ESTIMATED | section 4 |
| `days_no_limits_central` | 73–78 days | ESTIMATED | section 4 |
| `days_no_limits_upper` | 60–68 days | ESTIMATED | section 4 |
| `days_no_limits_range` | 73–97 days (or "about 75–95") | ESTIMATED | central low to conservative high, the defensible headline span |
| `other_project_days` | about 21 days | ESTIMATED | section 3, class b |

## 8. Concerns

- The central estimate depends on reading the 09-11 → 09-22 pause as limit-driven. The author
  should confirm, since only the author knows whether they would have kept Rustty running there
  without a cap.
- The `api_per_active_day` attribution differs in method from r2-cost's. Reconcile before
  publishing a dollar figure. The day counts and the reset signature do not depend on it.
- Stretch 4 (07-19 → 07-22) was computer and IDE crashes, not limits or other work. It sits in c
  only because the brief has no "other cause" class. It adds about 3 days to the upper case.
- Extra-usage credits were bought or used at least on 07-13/14 and 08-18 → 08-25 (r2-cost cannot
  size them). Where credits kept work going, limits cost less time than the reset signature
  suggests. This is already reflected, since those days show as active.
