# "Every prompt" Sessions and Founding Prompts Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add the founding prompts and a session layer (titles, areas, session timeline, session cards) to the "Every prompt" section of `/porting`.

**Architecture:** The Rustty-side extractor gains an explicit founding-session list and a per-session aggregate; a separate proposal step writes a title and area per session into a review CSV; after the author's review, the website builder emits session records (bundled summary) and session ids on log rows; the widget adds a session timeline, session tiles and a session-card log mode on top of the existing views.

**Tech Stack:** Python 3 stdlib (`unittest`), Next.js 16 App Router (static), React 19, CSS Modules, inline SVG; existing `src/components/porting/ui/*` and `every-prompt/*` modules.

**Spec:** `docs/superpowers/specs/2026-10-11-porting-every-prompt-sessions-design.md` (extends `2026-10-10-porting-every-prompt-design.md`, parent `2026-10-08-journey-article-design.md`).

## Global Constraints

- Website worktree `/Users/gael/projects/ghostty-website-journey`, branch `journey-article`. **Never push** (origin is ghostty-org/website). Commit locally.
- Rustty side: tools in the research worktree `/Users/gael/projects/rustty-journey-research` (branch `journey-article-research`, tip `064f0f66e`, unpushed) under `.port/porting-article/tools/`; untracked outputs in `/Users/gael/projects/rustty/.port/journey-article/research/data/` with suffix `-2026-10-11`. Raw extracts, proposal CSVs with text, and private manifests are never committed. Never push. A redacted value must never enter any commit on either branch — tests use synthetic values only (TEST-NET IPs, `hunter2-example`).
- Existing rules stay: verbatim prompts; only approved redactions; builder refuses unreviewed or stale reviews (match text must equal the raw text at the span; title review rows likewise must match the proposed title); counts reconcile across prose facts, registry caption/summary (guarded by `verify-porting-helpers.mjs`), tiles and log.
- Word choice: "session" everywhere; never "workstream". Areas (exact labels): engine, renderer and fonts, macOS app, GTK/Linux, performance, upstream catch-up, verification/CI, defects, tooling, article.
- Titles: Claude Code's own (`title`/`ai_title` in `~/projects/rustty-stats/data/sessions.csv`) kept verbatim where present (`title_source=claude-code`), else written (`claude-written`) — short, plain words, no internal ids, no names/paths/values from the redaction categories.
- Article rules (`.superpowers/sdd/2026-10-08-journey-article/prose-context.md`, `global-constraints.md`): first person singular sparingly; every number a `<Fact>` with an href; no internal ids in prose; Bun only in "The method matures"; no "pin" for the reference commit.
- Pacific time for days. Commit trailers: `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>` and `Claude-Session: https://claude.ai/code/session_018sK49nnxTgneVXAfHw9Mp3`.
- Quality bar: `python3 -m unittest scripts/porting/test_build_data.py`; extractor tests `python3 -m unittest discover -s .port/porting-article/tools -p 'test_*.py'`; `npm run verify:porting`, `npm run lint`, `npm run build`, `npm run verify:porting:build`, `npm run verify:animation`; Playwright at 1100 and 390 px on `next build` + `next start -p 3142` (port 3141 is the author's dev server — never kill processes by pattern; kill only PIDs you start); Playwright files under `/Users/gael/projects/rustty/.playwright-mcp/`, deleted after.
- Embargo: same as the article; the pre-push scan `scripts/porting/scan_redacted_values.py` must cover any new redactions (it reads the approved CSV).

## Review Focus

1. **A founding prompt from an unrelated project slips in** (another session in `~/projects` the same day) — expected: only sessions on the approved founding list are included. Test: Task 1 fixture with a same-day unrelated session.
2. **A Claude-written title leaks a redacted value or private detail** — expected: titles go through the redaction scanner and the author's review; builder refuses unreviewed titles. Test: Task 1 scanner on titles; Task 4 refusal test.
3. **Session totals don't add up** (a prompt in no session, or two) — expected: Σ session prompt counts = prompt total; every log row has exactly one session id that exists. Test: Task 4.
4. **Hundreds of overlapping bars on a busy day** — expected: packed rows stay readable; phone layout scrolls inside its container; no page-level horizontal scroll. Test: Task 5 Playwright at 390 px on the busiest day.
5. **A session link to a session hidden by other filters** — expected: the session filter wins (other filters relaxed with a visible note), never an empty view without explanation. Test: Task 5/6 model tests.

---

### Task 1: Extractor — founding sessions and session aggregates (Rustty side)

**Files (research worktree):**
- Modify: `.port/porting-article/tools/extract_prompts.py`
- Modify: `.port/porting-article/tools/test_extract_prompts.py`
- Modify: `.port/porting-article/tools/README.md`
- Outputs (untracked, `$RESEARCH/data/`): `prompts-raw-2026-10-11.jsonl`, `prompts-manifest-2026-10-11.json` (public, counts only), `prompts-manifest-private-2026-10-11.json`, `prompts-redactions-proposed-2026-10-11.csv`, `prompts-founding-sessions-proposed-2026-10-11.csv`, `prompts-sessions-2026-10-11.jsonl`, `prompts-glossary-missing-2026-10-11.txt`, `prompts-article-sessions-2026-10-11.csv`

**Interfaces — Produces:**
- `prompts-founding-sessions-proposed-2026-10-11.csv`: `session,tool,machine,project,first_ts,last_ts,prompts,reason,decision` (decision empty).
- Raw JSONL rows gain `"founding": bool`.
- `prompts-sessions-2026-10-11.jsonl`, one object per session:
  ```json
  {"session": "<full id>", "key": "claude-mac-<first8>", "tool": "claude", "machine": "mac",
   "first_ts": "…Z", "last_ts": "…Z", "prompts": 12, "commands_only": false, "founding": false,
   "article_work": false, "branches": ["…"], "models": ["…"], "commits": ["<sha>"], "commits_ambiguous": 3,
   "cc_title": "<Claude Code title or null>"}
  ```
- Redaction proposals keep the existing columns (`id,start,end,match,reason,decision`), with ids that may now include founding prompts.

- [ ] **Step 1: Failing tests.**
  - `test_founding_sessions_explicit_list` — with `--founding approved.csv` listing session A (decision `include`), prompts from A in `~/projects` are kept with `founding=True`; same-day session B in `~/projects` not on the list is dropped with reason `other-project`; a list row with decision `exclude` keeps it out.
  - `test_founding_candidates_proposed` — without an approved list, sessions in `~/projects`, `~/projects/rusty*` or `~/projects/ghostty*` between 2026-06-20T00:00Z and the first prompt in a `rustty` path are written to the proposed CSV with a `reason` (`port-decision`, `naming`, `ghostty-checkout`, `other`), and nothing is included.
  - `test_session_aggregate` — fixture with three sessions: counts, first/last, branches/models union, commits union (ambiguous counted, not listed), `commands_only` true only when every prompt is `kind=command`; Σ prompts per session = total kept.
  - `test_cc_title` — Claude Code title from the stats `sessions.csv` (`ai_title` preferred, else `title`) attached when present; null otherwise.
- [ ] **Step 2:** Run the discover command → FAIL.
- [ ] **Step 3: Implement.** New CLI flag `--founding <approved.csv>`; candidate detection as tested; session aggregation after `dedupe_links`; write the outputs above. Existing behaviour unchanged for rustty-path prompts. Keep the public-manifest privacy check (no paths/IPs).
- [ ] **Step 4:** Tests PASS. Real run (VM history: reuse the copy fetched on 2026-10-11 under `$RESEARCH/tmp/` if present; else fetch with rsync if `ssh rvm` answers; never start/stop the VM; if neither, mark VM "reused-from-2026-10-10 extract" and say so) **without** `--founding` → proposals written. Report counts: sessions, founding candidates (ids + reason + prompt count only, no text), new redaction proposals by reason.
- [ ] **Step 5: Commit** tools only: `tools(porting): founding sessions and session aggregates`.

### Task 2: Session title and area proposals (agent judgment; Rustty side, no commit)

**Files:** Output only: `$RESEARCH/data/prompts-sessions-proposed-2026-10-11.csv` with columns `session,key,title,title_source,area,decision,edited_title,edited_area,notes`.

- [ ] **Step 1:** For every session in `prompts-sessions-2026-10-11.jsonl` (including founding candidates, marked `founding-candidate` in notes): if `cc_title` exists, `title = cc_title`, `title_source = claude-code`; else write a title (≤ 60 characters, sentence case, plain words, no internal ids, no paths, no names, no values from redaction categories) from that session's prompts and linked commit subjects, `title_source = claude-written`. Assign exactly one area from the fixed list (Global Constraints). Command-only sessions: title "Command-only session", area = the area of the nearest substantive session of the same machine and day (note it). Leave `decision`, `edited_title`, `edited_area` empty.
- [ ] **Step 2:** Run the extractor's redaction scanner over every title (import its scanner; write any hit as a row in the redaction proposals with id `title:<key>`), and a check that no title contains an internal id (`find_internal_ids`).
- [ ] **Step 3:** Report: rows by area, titles by source, scanner hits (counts), and 10 sample rows (key, area, title) for the controller.

### Task 3: AUTHOR GATE — founding sessions, redactions, titles and areas (controller + author)

- [ ] **Step 1:** Controller presents: founding candidates (time, folder, prompt count, first prompt truncated to 60 characters) with a recommendation per row; new redaction proposals with context (secrets masked) and recommendations; the session table grouped by area (title + source + prompts + dates) with recommendations for any title to edit.
- [ ] **Step 2:** Author decides. Controller writes `prompts-founding-sessions-approved-2026-10-11.csv`, `prompts-redactions-approved-2026-10-11.csv` (carrying over the 2026-10-10 decisions for unchanged spans; the builder's stale-review check proves they still match), and `prompts-sessions-approved-2026-10-11.csv` (`decision` = `accept` | `edit`; edited fields filled when `edit`).
- [ ] **Step 3:** Re-run the extractor with `--founding` (controller dispatches a short subagent run of Task 1's CLI) so the raw extract includes approved founding prompts and the redaction proposals cover them; if new proposals appear, they come back to the author before Task 4.

### Task 4: Website builder — sessions in the data

**Files:**
- Modify: `scripts/porting/widgets/w12_every_prompt.py`, `scripts/porting/test_build_data.py`, `scripts/porting/facts.csv`, `scripts/porting/scan_redacted_values.py` (read the 2026-10-11 approved CSV too)
- Outputs: `src/app/porting/data/every-prompt.json`, `public/porting/every-prompt-log.json`

**Interfaces — Produces:**
- Summary gains `"sessions": [{"key","lane","firstDay","firstMin","lastDay","lastMin","prompts","promptsBucket","area","title","titleSource","founding","articleWork","commandsOnly","commits","row"}]` (`row` = packed timeline row index computed in the builder, greedy interval packing by start time), `"areas": [labels in fixed order]`, and per tick a `sessionIndex`.
- Log rows gain `"session": "<key>"`; log gains `"sessions": {"<key>": {"title","titleSource","area","branches","models","commits"}}`.
- `totals` gains `sessions`, `sessionsCommandOnly`, `foundingPrompts`, `foundingSessions`, `sessionsByArea`, `busiestParallelDay` (`{"day","open"}`), `longestSessionPrompts` (`{"key","prompts","spannedDays"}`: the session with the most prompts and the days its first and last prompt spanned), `medianPromptsPerSession`.
- New facts: `sessions_total`, `sessions_command_only`, `sessions_founding_prompts`, `sessions_busiest_parallel`, `sessions_longest_prompts`, `sessions_longest_spanned_days`, `sessions_median_prompts`, plus per-area counts only if the prose cites them. Hrefs: interim → the tools README at the Task 1 commit; final in Task 7.

- [ ] **Step 1: Failing tests:** refuses when any session lacks an approved decision, or an approved title differs from the proposal it reviewed (stale); `edit` rows use the edited title/area; titles pass the redaction-value grep (approved redact matches absent from both outputs, raw + JSON-escaped); every tick/log row has exactly one existing session; Σ session prompts = prompts total; founding prompts only from approved founding sessions; packing never overlaps two sessions on one row; the summary stays < 60 KB.
- [ ] **Step 2:** Run → FAIL. **Step 3:** Implement. **Step 4:** PASS; regenerate; `npm run verify:porting`. **Step 5: Commit** `porting: every-prompt sessions data`.

### Task 5: Widget — session timeline and session tiles

**Files:**
- Create: `src/components/porting/every-prompt/{SessionTimeline.tsx,SessionTiles.tsx,session-model.ts,session-model.test.ts}`
- Modify: `Body.tsx`, `model.ts` (Filter gains `area?: string; session?: string`; encode/decode `ar`, `s` with allowed lists), `EveryPrompt.module.css`, `src/components/porting/lib/registry.ts` (caption/summary/label: window from 21 June, sessions, "title by Claude", founding; re-measure `minHeight`), `scripts/verify-porting-helpers.mjs` (caption/summary guard covers the new numbers)

**Interfaces — Produces:** `session-model.ts`:
```ts
export function sessionsInRange(sessions: Session[], f: Filter): Session[];
export function sessionTiles(sessions: Session[]): { sessions: number; byArea: Array<{ area: string; n: number }>; longestHours: Session | null; longestPrompts: Session | null; busiestDay: { day: number; open: number } | null; medianPrompts: number | null };
export function revealSession(f: Filter, s: Session): Filter; // relax only filters hiding s
```

- [ ] **Step 1: Failing tests** (`session-model.test.ts`): range/area filtering; tiles on empty input (nulls, no crash); busiest day counting sessions open across midnight; `revealSession` relaxes only what hides the session; encode/decode round-trip with `ar`/`s` and rejection of unknown values.
- [ ] **Step 2:** FAIL. **Step 3: Implement.** Session timeline: SVG above the prompt timeline, same x scale (21 Jun → cutoff), packed rows, bar = session span (min width 2 px), fill by area colour + text label in the hover card and a legend with direct labels; thickness by `promptsBucket`; founding sessions with a distinct marker; purged-window hatch; roving focus over bars (`useRovingFocus`), Enter selects (sets `session` filter) and opens the card (Task 6 renders it; until then select only). Phone: scroll inside its own container (tabIndex 0, labelled). Session tiles row above existing tiles. Existing prompt timeline, tiles and settings view respect `session`/`area`.
- [ ] **Step 4:** PASS; lint, build; Playwright 1100/390 including the busiest day and keyboard selection. **Step 5: Commit** `porting: every-prompt session timeline and tiles`.

### Task 6: Widget — session-card log mode

**Files:**
- Create: `src/components/porting/every-prompt/{SessionCards.tsx,SessionCard.tsx}`
- Modify: `Log.tsx` (mode toggle "Sessions" | "All prompts", default Sessions), `log-model.ts` (+ tests), `EveryPrompt.module.css`

- [ ] **Step 1: Failing tests:** grouping rows by session preserves prompt order; filters apply inside cards (a card shows "N of M prompts match"); cards with zero matching prompts hidden; command-only sessions grouped in one collapsed group with a count; jump-to-session from the timeline/URL opens and focuses the card.
- [ ] **Step 2:** FAIL. **Step 3: Implement.** Virtualized list of cards (reuse the existing measured virtualization); card header: title (+ "title by Claude" marker when `claude-written`), area, Pacific dates, duration, prompts, commits (CommitLink, max 5 + more), branch/models where known, founding/article badges; expanding shows the session's prompts using the existing `LogRow`. Plain text only (existing grep test covers new files). Keyboard: cards as list items with a disclosure button (`aria-expanded`), focus kept on scroll (existing focused-row logic).
- [ ] **Step 4:** PASS; Playwright 1100/390: open a card, toggle modes, deep link `#every-prompt?s=<key>`. **Step 5: Commit** `porting: every-prompt session cards`.

### Task 7: Prose, dataset commit, links, checklist

**Files:**
- Modify: `src/app/porting/content.mdx` (section "Every prompt"), `scripts/porting/facts.csv`, `docs/superpowers/plans/journey-review-checklist.md`
- Research worktree: commit under `.port/porting-article/research/data/`: the redacted log (copy of `public/porting/every-prompt-log.json` as `prompts-redacted-2026-10-11.json`), public manifest and counts file for 2026-10-11, approved founding-sessions CSV and approved sessions CSV (titles + areas + decisions; no raw text), approved redactions CSV with `id,reason,decision` only.

- [ ] **Step 1:** Prose: open the section with what the sessions show — how the work split into sessions and areas, parallel sessions (busiest day), the founding session — then the existing paragraphs updated (window from 21 June; founding prompts "typed before the repository existed", no longer "left out"; titles "by Claude unless Claude Code kept its own"). Every number a `<Fact>`.
- [ ] **Step 2:** Commit the research files (`docs(porting): sessions, founding prompts and redacted prompt log`); verify with the scan (`python3 -I scripts/porting/scan_redacted_values.py --repo /Users/gael/projects/rustty main..journey-article-research` semantics: 0 per row for branch-only history) and that no proposal CSV with text was committed. Re-point every every-prompt href (facts, registry `sourceHref`, builder `SOURCES_HREF`) to the new commit; no stale research SHA in tracked files.
- [ ] **Step 3:** Checklist: review session titles/areas on the rendered page before push; the founding-session list; re-extraction requires a fresh title and redaction review.
- [ ] **Step 4:** Full quality bar + Playwright pass of the section at 1100/390. **Step 5: Commit** `porting: Every prompt sessions and founding prompts`.

---

## Self-review notes

- Spec coverage: founding list (T1/T3), sessions data (T1/T4), titles/areas + gate (T2/T3/T4), session timeline/tiles (T5), session cards + toggle (T6), filters/links (T5/T6), prose (T7), checks (T4–T7). No gaps.
- Placeholders: interim fact hrefs in T4 are replaced in T7 (explicit).
- Type consistency: `Filter.area/session`, `Session`, `sessionIndex`, `key` used consistently across T4–T6.
