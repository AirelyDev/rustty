# How long without usage limits? v2 (2026-10-11)

Supersedes the estimate in `elapsed-without-limits-2026-10-11.md` (v1, kept for reference, including
its stretch table and weekly-reset evidence). Per-day data: `data/elapsed-without-limits-v2-2026-10-11.csv`.
Scripts ran from `/tmp/elapsed_scripts/` and are not kept in the repo.

## Answer

**Central estimate: about 46 days, with a range of 41–51 days.** That puts the calendar end around
**6 August** (range 1–11 August) instead of 10 October. This counts all the work actually done,
every upstream catch-up included.

If the port had pinned one reference commit and made a single final catch-up (rule in §5), the
estimate is **about 43 days (38–48)**, ending around 3 August (29 July – 8 August).

Reader-facing assumption, in one sentence: *the estimate keeps every piece of work and every hour I
spent reading and answering the agents, and removes the stretches the usage limits left idle,
counting the eleven days I split between a limit and other work as half chosen.*

### Why v1 was too long

v1 never removed other-project time (~21 days) or the unknown stretches (~10 days). The author's
recorded statement is that quiet stretches were project switches *due to* token limits. v2 therefore
treats all idle time as limit-paced by default. The one exception is 11–22 September, which the
author called "both" and v2 splits 50/50.

## 1. Decomposition (central, all catch-ups included)

| component | days | how measured |
|---|---|---|
| Agent working time | 27.8 | busy time (below) minus author waits |
| Author waits inside active periods | 14.4 | gaps of ≥30 min that end in an author prompt, inside busy time. Includes nights shorter than 12 h |
| **= Busy time, as it happened** | **42.2** | union of all Rustty activity (commits, model calls, prompts), counting every gap shorter than 12 h |
| − throttling recovered | −2.3 (range 0 … −4.6) | §3 |
| + idle time that limits don't explain | +0.6 | the one non-limit gap of ≥12 h: overnight 09-26 17:15 → 09-27 07:50 |
| + the chosen half of 11–22 Sept | +5.4 (range 2.7 … 8.0 at a 25–75% split) | §2 |
| **= Central estimate** | **≈ 46 (41–51)** | |
| catch-up variant (b): − intermediate catch-up work | −2.6 | §5 |
| **= Variant (b)** | **≈ 43 (38–48)** | |

The upstream floor is **zero**, per the coordinator's correction. A faster port would have pinned
earlier and needed fewer catch-ups, so no upstream-imposed wait is added.

**Busy time.** All Rustty events from the first to the last commit (06-21 15:36 → 10-10 23:16), sorted.
Every gap shorter than 12 h counts as busy, and every gap of 12 h or more counts as idle. Events are
3,135 commits (`git log --all`), Rustty model calls from `messages.csv.gz`, and the 1,023 prompts.
Results: 42.2 busy days at a 12 h threshold, 36.0 at 6 h, 48.7 at 24 h. The 12 h threshold is used
because the gaps of 12–24 h that are not limit-tied come down to the one overnight gap added above;
the others are limit pauses (for example 09-28 21:47 → 09-29 15:45, right after "dont start new
work/sugagents"). The 6 h threshold would remove real nights and review time, which the estimate
should keep.

Busy blocks at 12 h, with commits per busy hour:

| block | busy h | commits | /h | note |
|---|---|---|---|---|
| 06-21 → 06-26 | 114 | 293 | 2.6 | terminal engine sprint |
| 06-30 → 07-04 | 86 | 125 | 1.5 | ends "out of token usage" |
| 07-07 → 07-12 | 122 | 138 | 1.1 | realignment, macOS bring-up |
| 07-14 → 07-19 | 121 | 297 | 2.5 | VM GTK, perf |
| 07-22 → 07-23 | 18 | 62 | 3.5 | |
| 07-28 → 08-02 | 108 | 507 | 4.7 | catch-up 1, perf, Codex audit (~40 h, 0 commits) |
| 08-10 → 08-18 (3 blocks) | 45 | 12 | — | Codex-findings remediation, upstream sync start |
| 08-25 → 08-26 | 38 | 71 | 1.8 | catch-ups 2–4 |
| 09-01 | 11 | 36 | 3.4 | catch-up 5, parity audit |
| 09-08 → 09-11 | 79 | 259 | 3.3 | parity closure |
| 09-22 → 09-24 | 38 | 486 | **12.8** | fresh quota, 180-commit overnight run |
| 09-26, 09-27 → 09-28 | 46 | 326 | 7.1 | |
| 09-29 → 10-02 | 73 | 247 | 3.4 | Phase 7 resync, sanitizer/Miri runs |
| 10-06 → 10-10 | 100 | 268 | 2.7 | verification matrix, catch-ups 7–8 |

## 2. Other-project and unknown stretches (point 1)

- **Default rule:** idle stretches with other-project prompts, and stretches with no explanation,
  count as limit-paced, following the author's statement. Section 3 of v1 lists them (6, 8, 9, 10,
  11, 12a, 12b).
- **Exceptions where evidence shows a voluntary choice:** I found none outside 11–22 September. No
  prompt in those stretches says Rustty was set aside by choice. The closest is 08-12 ("ensure we're
  not losing anything ... Then cleanup"), which wraps up a finished task, not a decision to stop.
- **11–22 September split rule:** the author answered "both". The idle part of the window
  (09-11 19:25 → 09-22 18:35, minus 5.7 busy hours on 09-17) is 257.5 h. 50% of it counts as
  chosen: **+5.4 days**. The range uses a 25–75% split.
- **Sensitivity:** 09-05 → 09-07 went to the same other project. Splitting it the same way adds
  about 1.2 days. Treating all of v1's unknown stretches (10–12 days) as irreducible instead of
  limit-paced would give ~56 days. That includes the 07-19 → 07-22 VM-crash stretch.

## 3. Throttling inside active time (point 2)

**Prompt evidence (Rustty only):**
- Effort lowered on 07-08 ("dropped the effort level to high") and 08-01 ("use medium effort from
  now on"). The recorded effort is `medium` on every prompt from 09-07.
- Model switched when an allotment ran out, on 08-10 and 08-11 (Fable → Opus 5).
- New work stopped on 09-28 ("dont start new work/sugagents").
- Against that, the author asked to parallelize "as much as possible" on 07-30, 07-31, 09-28 and
  09-22. No prompt asks to run fewer agents to save tokens. Concurrency was mostly not throttled by
  instruction: the limits stopped the work rather than slowing it.

**Measured pacing within the quota week** (busy hours bucketed by days since the Tuesday 15:00 PT
reset):

| | days 0–2 after reset | days 3–6 |
|---|---|---|
| Jun–Aug busy hours | 413 | 246 (206 excluding the Codex audit) |
| Jun–Aug commits per busy hour | **2.97** | **1.15** (1.37 excluding the Codex audit) |
| Sep–Oct busy hours | 256 | 101 |
| Sep–Oct commits per busy hour | 4.5 | 4.6 |
| Sep–Oct Mac Claude list price per busy hour | $18–23 | $10–60 (noisy) |

- **June–August:** the port both ran fewer hours late in the quota week (busy hours by day: 154,
  144, 115, 93, 64, 31, 58) and committed at under half the rate. Running the late-week work (283
  commits) at the early-week rate would save at most **4.6 days**, Codex audit excluded. The
  central estimate takes half, **2.3 days**, because part of the slow late-week hours was slow work
  (the 07-09 → 07-12 realignment and macOS planning).
- **September–October:** no rate drop late in the week. Throttling took the form of stopping (the
  pauses), not slowing. Gain: 0.
- **Bound check against the September peak:** the fresh-quota block 09-22 → 09-24 ran a median of
  20 concurrent agents and 6 worktrees per hour, at $45 list price per busy hour and 12.8 commits
  per hour. The block after the 09-28 warning ran 4 agents, $17/h and 3.4/h. That block was bound
  by sanitizer and Miri runs (the author: "Why is the sanitizer work slow?"), so its gap from the
  peak is **not** counted as throttling. The 12.8/h peak, and the 0/14 → 14/14 perf night, are
  shown for context. Neither is applied as a rate anywhere, because they are peaks of one kind of
  work, not a sustained rate for the whole port.

## 4. Work-content cross-check (point 3)

Each era's commits divided by its own sustained, unthrottled rate:
- **Jun–Aug:** 1,508 commits in busy blocks at the early-quota-week rate of 2.97/h = 21.2 days.
- **Sep–Oct:** 1,627 commits at 4.5/h = 15.1 days.
- **Total ≈ 36 days** of rate-limited work, against the model's 42.2 − 2.3 = 39.9 days. The 4-day
  difference is busy time with no commits (the Codex audit, CI and VM lanes, review). It belongs in
  the estimate as an irreducible wait, so the central value stands. Applying the September peak
  (12.8/h) to the whole port would give about 10 days. That is the theoretical figure the brief
  rules out, and it is not used.

Irreducible waits, and where they sit:
- Author review and decision time (14.4 days) is inside busy time and kept. Before 09-08, Mac Claude
  calls are lost, so some of these "waits" may have been agents working without committing. That
  shifts days between the two busy-time components, not the total.
- Long Miri, sanitizer and Valgrind runs and the VM lanes are inside busy blocks (09-29 → 10-02,
  10-06 → 10-10, 07-14 → 07-19) and kept in full. No long run continued through an idle stretch,
  because commits and model calls stop together in every gap.

## 5. Upstream catch-ups (corrected rule)

- **(a) All work done:** every catch-up's busy hours are kept (headline estimate).
- **(b) Rule:** keep the port of the first reference commit (4749c4e93) and **one** final catch-up,
  and drop the busy hours of the others. The kept final catch-up is catch-up 1 (07-29 → 07-31,
  upstream through 07-29). Its range ends just before the counterfactual end date of early August,
  so it is the catch-up that port would have needed. Busy hours inside catch-up windows 2–8:
  20.7 + 9.7 + 3.5 (−1.1 overlap) + 6.0 + 13.7 + 9.7 + 1.2 = **63.4 h ≈ 2.6 days**. Other work ran
  in parallel inside some windows, so 2.6 is an upper bound on the saving.

## 6. Cost link (usage intensity)

- The port ran on 59 of the 112 calendar days, for **42 days' worth of busy hours**.
- Whole-port API-equivalent value is ~$23.8k (`cost_api_equiv`, r2-cost). That is ~$212 per
  calendar day, ~$403 per commit day, ~$565 per busy day.
- At the no-limit pace, the same tokens over ~46 days come to **~$3,600 a week at list price for
  Rustty alone**. A maxed-out week across all projects was ~$2,769 (`cost_ceiling_central`), and
  Rustty's measured share ~54%. So the no-limit pace needs about **2½ times the share of the
  subscription Rustty actually had**, or a little over one whole maxed-out week every week.
- Caveat: the lost-window part of $23.8k was modelled from the same maxed-week ceiling, so this
  ratio is partly circular. Use "about 2½×" rather than more digits.
- In the recovered window, Rustty used ~$7,260 over 31 days at list price, ~$403 per active day. My
  attribution differs from r2-cost's $6,539 for its four quota weeks and should be reconciled
  before a dollar figure is published.

## 7. Confidence

- **Busy time (42.2 d): high.** Git, prompts and the surviving telemetry agree, and idle gaps start
  and end within minutes of pause and resume prompts.
- **Treating idle as limit-paced: medium.** It rests on the author's statement plus the weekly-reset
  signature (7–8 of 22 resumes within 5 h of the Tuesday reset; v1 §2). Few stretches show it
  directly. The swing is +10 days if the unknowns are not limit-paced.
- **Throttling (−2.3 d): medium-low.** The quota-week effect is large in Jun–Aug, but commits per
  hour mix work types.
- **Lost Mac transcripts before 09-08** do not move the busy-time total much, because commits and
  prompts are intact. They do hide Mac agent activity, so some 12 h+ gaps before September could
  have had agents running without committing. That would make busy time, and the estimate,
  slightly longer: the 07-31 → 08-02 Codex audit shows the effect exists, and it is already
  counted. Likely effect: under 2 days.

## 8. Drafts for the approved placements

**"At a glance" sentence** (after the scope paragraph):

> Rustty ran on one Claude subscription that I shared with other projects, and the subscription's
> weekly limit set its pace. Agents worked on <Fact id="commit_days"/> of the
> <Fact id="days_total"/>. Without the limits, I estimate the same work would have taken about
> <Fact id="days_no_limits_central"/> (<Fact id="days_no_limits_range"/>); [What it cost](#what-it-cost)
> explains how.

**Stats-card note** (tile `days`):

> Every day counted. Agents worked on 59 of them. Without usage limits, an estimated 41–51 days.

**"What it cost" paragraph** (after the API-equivalent paragraph, before the widget):

> The flat price came with a pace. The subscription's limit reset every Tuesday afternoon, and
> the port ran to it: of the <Fact id="gaps_12h"/> pauses in the commit history longer than
> 12 hours, <Fact id="resumes_after_reset"/> ended within five hours of a reset. Between June and
> August, agents committed <Fact id="pace_early_week"/> times an hour in the first three days of a
> limit week and <Fact id="pace_late_week"/> in the rest. Add up the hours when the port was
> actually running, including my own time reading and answering the agents, and they come to
> <Fact id="busy_days"/>. The rest was idle, mostly because the limit was reached or because I
> moved to other projects until it reset. The estimate keeps every piece of work and every hour
> I spent on it, and removes the stretches the limits left idle, counting the eleven days I
> split between a limit and other work as half chosen. That gives about
> <Fact id="days_no_limits_central"/> (<Fact id="days_no_limits_range"/>) instead of
> <Fact id="days_total"/>. Running at that pace would have used about
> <Fact id="usage_multiple_no_limits"/> the share of the subscription the port had.

## 9. Proposed facts

| id | value | status | source |
|---|---|---|---|
| `commit_days` | 59 | MEASURED | git log --all, Pacific dates (v2 CSV) |
| `busy_days` | about 42 days | MEASURED | v2 §1, busy time at a 12 h gap threshold |
| `author_wait_days` | about 14 days | ESTIMATED | v2 §1 (prompt-ended gaps ≥30 min) |
| `gaps_12h` | 22 | MEASURED | v1 §2 |
| `resumes_after_reset` | 7 | MEASURED | v1 §2 (8 counting the 10-06 resume prompt) |
| `pace_early_week` | about 3 | MEASURED | v2 §3, Jun–Aug commits per busy hour, days 0–2 |
| `pace_late_week` | about 1.2 (1.4 excluding the Codex audit) | MEASURED | v2 §3 |
| `days_no_limits_central` | about 46 days | ESTIMATED | v2 §1 |
| `days_no_limits_range` | 41–51 days | ESTIMATED | v2 §1 |
| `days_no_limits_single_catchup` | about 43 days (38–48) | ESTIMATED | v2 §5 |
| `end_no_limits` | early August (about 6 August) | ESTIMATED | 06-21 + 46 days |
| `usage_multiple_no_limits` | about 2½× | ESTIMATED | v2 §6; cost_api_equiv, cost_ceiling_central, rustty_share_measured |
| `peak_block_rate` | 486 commits in 38 busy hours (09-22 → 09-24) | MEASURED | v2 §1 (context only) |

## 10. Concerns

- The estimate rests on the author's statement that idle stretches were limit-driven. If the
  unknown stretches were not, the central value rises to ~56 days.
- The throttling term uses commits per hour, which mix work types. It is kept small (half of the
  measured gap) and has no effect in September–October.
- "About 2½×" is partly circular. Do not publish it to more precision.
- The headline uses (a), all catch-ups, because that is the work that was actually done. (b) is a
  footnote-level variant.
- The busy-time split is uncertain before 09-08 (agent working vs. waiting for me). Only the total is
  meant for publication.
