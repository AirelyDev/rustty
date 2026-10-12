# "Every prompt" Section Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a prompt-transparency section to `/porting` that shows every human prompt behind the port, verbatim (approved redactions only), with known settings, commit links and explicit flags for lost or redacted data.

**Architecture:** A stdlib-Python extractor on the Rustty side reads the three prompt sources, joins settings and commits, proposes redactions and writes an unredacted JSONL that stays untracked. After the author approves the redaction list, a website builder applies it and writes a small summary JSON (bundled) and a log JSON (static file fetched on demand). A new flagship React widget renders a timeline strip, live tiles, a settings view and a virtualized, filterable log.

**Tech Stack:** Python 3 stdlib (`unittest`), Next.js 16 App Router (static), React 19, MDX, CSS Modules, inline SVG; existing `src/components/porting/ui/*` primitives.

**Spec:** `docs/superpowers/specs/2026-10-10-porting-every-prompt-design.md` (parent: `docs/superpowers/specs/2026-10-08-journey-article-design.md`).

## Global Constraints

- Worktree `/Users/gael/projects/ghostty-website-journey`, branch `journey-article`. **Never push** (origin is ghostty-org/website). Commit locally.
- Rustty side: tools live in the research worktree `/Users/gael/projects/rustty-journey-research` (branch `journey-article-research`, unpushed) under `.port/porting-article/tools/`; outputs go to the untracked research dir `/Users/gael/projects/rustty/.port/journey-article/research/data/` (`$RESEARCH/data`). The **unredacted** extract (`prompts-raw-2026-10-10.jsonl`) is never committed anywhere. Never push.
- Sources: Mac `~/.claude/history.jsonl` and `~/.claude/projects/**/*.jsonl`; VM history fetched with `rsync -a rvm:.claude/history.jsonl` (the VM may need starting; the author allows it; IP may move — see memory `windows-vm-rwin`); Codex `~/.codex/sessions/**` and `~/.codex/archived_sessions/**`; settings from `~/projects/rustty-stats/data/sessions.csv` and `messages.csv.gz`. Project filter: path/cwd contains `rustty` (worktrees included), same as rustty-stats.
- Scope: Claude Code Mac + VM human prompts; Codex app human turns (exclude sessions whose model or kind is `codex-auto-review`); article-work prompts included and tagged; no subagent prompts; no model responses.
- Verbatim text; internal ids keep their spelling and get a glossary hover; redactions only for secrets, other people's private information and unrelated projects, rendered `[redacted: <reason>]`, counted; **no text ships while any proposed redaction is unreviewed**.
- Article rules (from `.superpowers/sdd/2026-10-08-journey-article/global-constraints.md` and `prose-context.md`): first person singular, sparingly; every number in prose is `<Fact>` with an `href`; commit hashes via `CommitLink`; no internal ids in prose (ids inside quoted prompts are allowed — they are the data); Bun only in "The method matures"; no "pin" for the reference commit.
- Times shown in Pacific time (as the rest of the article). The section's window is 2026-06-21 → 2026-10-10; other sections' "to 8 October" figures stay unchanged.
- Commit trailers: `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>` and `Claude-Session: https://claude.ai/code/session_018sK49nnxTgneVXAfHw9Mp3`.
- Quality bar: `python3 -m unittest scripts/porting/test_build_data.py`, `npm run verify:porting`, `npm run lint && npm run build && npm run verify:porting:build`, `npm run verify:animation`; Playwright at 1100 and 390 px — no console errors, no page-level horizontal scroll; keyboard reachable controls; reduced motion honoured. The author's dev server on port 3141 stays running; use `next start -p 3142` on a production build for your own checks (two `next dev` servers share a lock).

## Review Focus

1. **A prompt that leaks something** (a token, someone's name, another project) — expected: it appears in the proposed-redactions CSV and the builder refuses to emit text until reviewed. Test: Task 1 scanner fixtures; Task 3 refusal test.
2. **Prompts in the purged window shown with settings** — expected: model/effort "unknown" (or "inferred from commits", marked) for Mac prompts before 2026-09-08. Test: Task 1 settings-join test; Task 3 invariant test.
3. **Counts that don't reconcile** — expected: per-source totals in the manifest equal the sources; every dropped row has a reason. Test: Task 1 reconcile test; Task 3 manifest test.
4. **Huge or odd prompts** (multi-kilobyte pastes, newlines, markdown, `<`/`>` characters, emoji, `[Pasted text #1]`) — expected: render as plain text (no HTML/MDX interpretation), collapsed with an expand control, no layout overflow at 390 px. Test: Task 5 render test with a fixture row.
5. **Filters that empty the log** (search with no hits, brushing a day with no prompts) — expected: an explicit "no prompts match" row, tiles show 0/—, no crash. Test: Task 4/5 model tests.

---

### Task 1: Prompt extractor, settings and commit joins, redaction proposals (Rustty side)

**Files (in `/Users/gael/projects/rustty-journey-research`):**
- Create: `.port/porting-article/tools/extract_prompts.py`
- Create: `.port/porting-article/tools/test_extract_prompts.py`
- Create: `.port/porting-article/tools/README.md`
- Outputs (untracked, `$RESEARCH/data/`): `prompts-raw-2026-10-10.jsonl`, `prompts-manifest-2026-10-10.json`, `prompts-redactions-proposed-2026-10-10.csv`, `prompts-glossary-missing-2026-10-10.txt`, `prompts-article-sessions-2026-10-10.csv`

**Interfaces — Produces** (consumed by Task 3): one JSON object per line in `prompts-raw-2026-10-10.jsonl`:

```json
{"id": "claude-mac-20260713T091502Z-0", "ts": "2026-07-13T09:15:02Z", "tool": "claude", "machine": "mac",
 "session": "0b1c…", "text": "continue", "chars": 8, "kind": "prompt|command",
 "pasted": "none|kept|lost", "model": "claude-opus-4-7|null", "effort": "xhigh|null", "mode": "bypass|plan|default|null",
 "settings_source": "transcript|session|commit-trailer|unknown", "article_work": false,
 "commits": ["<full sha>", "…"], "commits_basis": "session-window|time-window-branch|none",
 "ids": ["UD131", "R7"], "phase": "<phase key from strategy timeline or null>"}
```

Manifest: `{"sources": {"claude-mac": {"in_source": N, "kept": N, "dropped": {"<reason>": N}}, "claude-vm": {…}, "codex": {…}}, "generated": "<iso>", "window": ["2026-06-21", "2026-10-10"], "rules": {"article_work": "<text>", "decision": "<text>", "commits": "<text>"}}`.
Proposed redactions CSV columns: `id,start,end,match,reason,decision` (`decision` empty = unreviewed; the author fills `redact` or `keep`).

- [ ] **Step 1: Write failing tests** in `test_extract_prompts.py` with in-memory fixtures (no real files):
  - `test_history_rows_filtered_by_project` — rows whose `project` lacks `rustty` are dropped with reason `other-project`.
  - `test_slash_commands_kind_command` — `display="/clear"` → `kind="command"`.
  - `test_pasted_flag` — `"[Pasted text #1]"` with empty `pastedContents` → `pasted="lost"`; with contents → `"kept"` and the text is NOT expanded (verbatim display only); no placeholder → `"none"`.
  - `test_codex_human_turns_only` — a Codex session fixture with an `environment_context`/AGENTS.md injection, a tool result and two real user turns → exactly 2 prompts; a session whose model is `codex-auto-review` → 0 prompts, dropped reason `auto-review`.
  - `test_settings_purged_window` — a Mac prompt at 2026-08-01 with no transcript → `model=None`, `settings_source="unknown"` unless a commit trailer in its window names a model → `settings_source="commit-trailer"`; a 2026-09-20 prompt with a transcript call after it → model/effort from that call, `settings_source="transcript"`.
  - `test_commit_window` — commits between this prompt and the next prompt of the same session, on the session's branch when known (`commits_basis="session-window"`); without a branch → `time-window-branch` only if a single branch was active, else `none`.
  - `test_decision_rule` — `A`, `b`, `ok`, `yes`, `go`, `continue`, `ok.`, `B, go ahead` classify as decisions; `ok but change X` does not. Rule: after lowercasing and stripping punctuation, the text is a single choice letter, or ≤ 3 words all from `{ok, yes, y, no, go, ahead, continue, proceed, sure, agreed, lgtm, a, b, c, d}`. Export as `is_decision(text) -> bool` (Task 3 reuses the same rule — copy it there verbatim, tested on the same cases).
  - `test_article_work_rule` — a session is article work if any of its prompts is on/after 2026-10-08T00:00-07:00 AND (its cwd/project path contains `ghostty-website`, OR any of its prompts matches `\b(article|journey|website|porting page|disclosure)\b` case-insensitively). Prompts before 2026-10-08 are never article work.
  - `test_redaction_scanner` — finds: `sk-…`/`ghp_…`/`xox…`/AWS-style keys/`-----BEGIN` → `secret`; emails other than `noreply@anthropic.com` → `email`; phone numbers → `phone`; paths under `/Users/<name>/` outside `projects/rustty*`, `projects/ghostty*`, `projects/rustty-stats` → `unrelated-path`; names from a configurable `PEOPLE` list → `person`; an allowlist suppresses known-safe terms (`Ghostty`, `Mitchell` only when part of `Mitchell Hashimoto` crediting Ghostty, etc. — allowlist file `tools/redaction-allowlist.txt`).
  - `test_ids_extracted` — `UD131`, `R7`, `G3b`, `N550`, `P8-W10`, `FX13` found by the shared id regex (import `find_internal_ids` semantics from `scripts/porting/jargon.py` — copy the pattern list into the tool and note the source; keep them in sync via a test in Task 3).
  - `test_manifest_reconciles` — kept + Σdropped == in_source per source.
- [ ] **Step 2:** Run `python3 -m unittest .port/porting-article/tools/test_extract_prompts.py -v` → FAIL (module missing).
- [ ] **Step 3: Implement** `extract_prompts.py` (pure functions for each rule above + `main()` with `--research`, `--rustty-repo`, `--stats`, `--vm-history`, `--codex-dir`, `--claude-dir`, `--people`). Timestamps: history `timestamp` is epoch ms → ISO UTC. Commit list from `git -C <rustty> log --all --no-merges --format=%H%x09%aI%x09%D%x09%(trailers:key=Claude-Session,valueonly)%x09%(trailers:key=Co-Authored-By,valueonly)`. Phases: read `$RESEARCH/data/strategy-timeline*.csv` (latest dated) and map by date. Glossary: ids not found in `$RESEARCH/r2-glossary.md` → `prompts-glossary-missing-2026-10-10.txt` (one id per line with count and first prompt id).
- [ ] **Step 4:** Tests PASS. Then run for real: fetch the VM history (`rsync -a rvm:.claude/history.jsonl /Users/gael/projects/rustty/.port/journey-article/research/tmp/vm-history.jsonl`; start the VM if needed and stop it after), run `main()`, and print the manifest. Sanity: Mac Claude prompts ≈ 808, VM ≈ 165 (rustty-stats `prompts.csv` counts), Codex human turns > 0.
- [ ] **Step 5: Commit** (research worktree; tools only — never the outputs): `git add .port/porting-article/tools && git commit -m "tools(porting): prompt extractor for the Every prompt section"` + trailers.
- [ ] **Step 6:** Report: manifest counts, number of proposed redactions by reason, glossary-missing ids count, article-work sessions list path.

### Task 2: AUTHOR GATE — redaction and glossary review (controller + author, no subagent)

- [ ] **Step 1:** Controller summarizes for the author: counts by reason, every `secret`/`person`/`email` row in full (id, ±40 chars context), `unrelated-path`/other rows grouped, the article-work session list, and the missing glossary ids. Present it as a file the author can edit: `$RESEARCH/data/prompts-redactions-proposed-2026-10-10.csv` (`decision` column) plus `prompts-glossary-missing-2026-10-10.txt` (author adds `ID<TAB>plain words`).
- [ ] **Step 2:** Author fills decisions. Controller copies the result to `prompts-redactions-approved-2026-10-10.csv` and `prompts-glossary-extra-2026-10-10.tsv`, and verifies: every row has `redact` or `keep`.
- [ ] **Step 3:** Ledger the approval (date, counts). If the author adds manual redactions (whole prompts or spans), they go into the approved CSV with reason `author` and the same columns.

### Task 3: Website data builder for `every-prompt`

**Files:**
- Create: `scripts/porting/widgets/w12_every_prompt.py`
- Modify: `scripts/porting/widgets/__init__.py` (register `"every-prompt": w12_every_prompt.build`)
- Modify: `scripts/porting/build_data.py` only if a builder needs to write a second file (add a documented `extra_outputs` hook: a builder may return `{"__extra__": {"public/porting/every-prompt-log.json": {...}}}`; `build_data.py` writes those paths relative to the repo root and strips the key)
- Modify: `scripts/porting/test_build_data.py`
- Modify: `scripts/porting/facts.csv` (new rows, below)
- Outputs: `src/app/porting/data/every-prompt.json`, `public/porting/every-prompt-log.json`

**Interfaces — Consumes:** Task 1 JSONL/manifest; Task 2 approved CSV + glossary TSV; `$RESEARCH/r2-glossary.md`.
**Produces:**

`every-prompt.json` (bundled, target < 60 KB):
```json
{"_meta": {...}, "window": ["2026-06-21", "2026-10-10"], "purgedBefore": "2026-09-08",
 "lanes": ["claude-mac", "claude-vm", "codex"],
 "ticks": [[dayIndex, minuteOfDay, laneIndex, log2chars, flags], ...],
 "commitsPerDay": [n, ...], "phases": [{"key": "...", "label": "...", "from": d, "to": d}],
 "weeks": [{"week": "2026-06-22", "settings": {"<model>|<effort>": n, "unknown": n}, "modes": {"bypass": n, "plan": n, "default": n, "unknown": n}}],
 "totals": {"prompts": n, "bySource": {...}, "redactions": n, "redactedPrompts": n, "pastedLost": n, "settingsUnknown": n, "articleWork": n, "commands": n},
 "logUrl": "/porting/every-prompt-log.json", "logBytes": n,
 "rules": {"decision": "...", "articleWork": "...", "commits": "..."},
 "sourcesHref": "<research permalink to manifest>"}
```
`flags` bitmask: 1 decision, 2 command, 4 article work, 8 settings unknown, 16 pasted lost, 32 redacted.

`every-prompt-log.json` (static, fetched): `{"glossary": {"UD131": "plain words", ...}, "rows": [{"id","ts","tool","machine","model","effort","mode","settingsSource","chars","kind","pasted","articleWork","decision","phase","commits": ["<full sha>"],"commitsBasis","text"}]}` — `text` already redacted (`[redacted: <reason>]`), plain string.

New facts (all `MEASURED`, source `research/data/prompts-manifest-2026-10-10.json`, `as_of` `2026-10-10`, `href` = research permalink — add after Task 6 commits the dataset; use a placeholder href to the tools README until then and replace in Task 6): `prompts_total`, `prompts_claude_mac`, `prompts_claude_vm`, `prompts_codex`, `prompts_median_chars`, `prompts_decisions_share`, `prompts_redactions`, `prompts_pasted_lost`, `prompts_settings_unknown`, `prompts_article_work`.

- [ ] **Step 1: Failing tests** in `test_build_data.py` (`EveryPromptTest`), using a temp research dir fixture:
  - refuses (raises `SystemExit` with a message naming the file) when a proposed redaction id is absent from the approved CSV or has an empty decision;
  - applies only `redact` rows; the output text contains `[redacted: secret]`, and the original span is absent from BOTH JSON outputs (grep the serialized strings);
  - `totals` reconcile with the manifest (`prompts == Σ kept`);
  - no row before `purgedBefore` from `claude-mac` has `settingsSource == "transcript"`;
  - `is_decision` cases from Task 1 (same list) pass;
  - the id regex matches the same samples as `scripts/porting/jargon.py`'s `find_internal_ids` (sync test);
  - `every-prompt.json` serialized < 60 KB on the real data (skip if research dir absent, like existing tests).
- [ ] **Step 2:** Run → FAIL.
- [ ] **Step 3:** Implement the builder (+ `extra_outputs` hook if used) and facts rows.
- [ ] **Step 4:** Tests PASS; `python3 scripts/porting/build_data.py` regenerates; `npm run verify:porting` passes (registry entry comes in Task 4 — if the registry check requires one now, add the registry entry in this task with `requiredKeys: ["ticks","weeks","totals","lanes"]`).
- [ ] **Step 5: Commit** `porting: every-prompt data builder`.

### Task 4: Widget part 1 — timeline strip, tiles, settings view

**Files:**
- Create: `src/components/porting/every-prompt/{index.ts,Body.tsx,Timeline.tsx,Tiles.tsx,SettingsView.tsx,model.ts,model.test.ts,EveryPrompt.module.css}`
- Modify: `src/components/porting/ui/WidgetMount.tsx` (LOADERS `"every-prompt"`), `src/components/porting/lib/registry.ts` (entry: slug `every-prompt`, title "Every prompt", label "EVERY HUMAN PROMPT, 21 JUNE TO 10 OCTOBER", source "prompt history · Codex sessions · usage transcripts", `sourceHref` = manifest permalink, caption + summary written from the data (state purged window, pasted text, redaction count, article-work tag), `flagship: true`, `frame: "card"`, `dataFile: "every-prompt.json"`, `requiredKeys` as Task 3, `minHeight` measured after render)

**Interfaces — Consumes:** `every-prompt.json`. **Produces** (for Task 5): `model.ts` exports
```ts
export type Filter = { from: number; to: number; lanes: Set<number>; decisionsOnly: boolean; hideArticle: boolean; model?: string; effort?: string; phase?: string; query?: string };
export function applyFilter(ticks: Tick[], f: Filter): Tick[];
export function tiles(ticks: Tick[]): { prompts: number; medianChars: number | null; underTenWordsShare: number | null; decisions: number; commitsPerPrompt: number | null };
export function encodeFilter(f: Filter): string;   // URL fragment "every-prompt?from=…&to=…&lanes=0,2&d=1&a=0&m=…&e=…&p=…&q=…"
export function decodeFilter(s: string, defaults: Filter): Filter;
```
`Body.tsx` owns the `Filter` state, syncs it with `location.hash` (replaceState, no scroll), and passes it to Task 5's `Log` (render a placeholder slot `<div data-log-slot />` now).

- [ ] **Step 1: Failing tests** `model.test.ts` (run by `npm run verify:porting` helpers like existing `*.test.ts`): filter by range/lanes/flags; empty result → tiles `{prompts: 0, medianChars: null, …}`; encode/decode round-trip; decode ignores garbage.
- [ ] **Step 2:** Run → FAIL.
- [ ] **Step 3: Implement.**
  - Timeline: SVG, x = day (21 Jun → 10 Oct), three lane rows; tick per prompt at `minuteOfDay` within its day column, height ∝ `log2chars`; colours from site tokens (Mac Claude copper `#d97934`, VM gold `#ffbc7a`, Codex teal `#64d8c6`) plus lane labels (colour never the only encoding); commits-per-day bars beneath; phase bands; hatched region before `purgedBefore` with label "transcripts purged — model and effort unknown"; brush with pointer (drag) and keyboard alternative (two `<input type="range">` for from/to, labelled); hover/focus tooltip per day (prompts per lane, commits) via existing `useChartTip`.
  - Tiles: prompts, median length, share under ten words, decisions, commits per prompt, redactions (global, from `totals`), longest prompt (button → sets query to its id, Task 5 scrolls to it), most repeated (top 5 from Task 5's loaded log; before load, show "open the log to compute").
  - Settings view: per-week stacked bars model×effort; hatched "unknown" segment; toggle to modes; legend with direct labels.
  - Phone (390 px): timeline scrolls inside its own container; tiles 2 columns; settings bars stacked.
- [ ] **Step 4:** Tests PASS; lint/build; Playwright check at 1100/390 px.
- [ ] **Step 5: Commit** `porting: every-prompt timeline, tiles and settings view`.

### Task 5: Widget part 2 — the full log

**Files:**
- Create: `src/components/porting/every-prompt/{Log.tsx,LogRow.tsx,Glossed.tsx,log-model.ts,log-model.test.ts}`
- Modify: `Body.tsx` (replace the slot), `EveryPrompt.module.css`

**Interfaces — Consumes:** Task 4 `Filter` + `applyFilter` semantics; `logUrl`. **Produces:** `log-model.ts`
```ts
export function filterRows(rows: Row[], f: Filter): Row[];
export function topRepeated(rows: Row[], n: number): { text: string; count: number }[];
export function splitGlossed(text: string, glossary: Record<string,string>): Array<{ t: string; id?: string }>;
```

- [ ] **Step 1: Failing tests** `log-model.test.ts`: `filterRows` agrees with `applyFilter` on the same fixture (same ids); query matches case-insensitively on text and id; `topRepeated` ignores commands and counts exact normalized text; `splitGlossed` splits `"fix UD131 then R7"` into 4 parts with ids; text with `<script>`/`{}`/`**` stays literal (renderer uses text nodes only — assert no `dangerouslySetInnerHTML` in the component sources via a grep test).
- [ ] **Step 2:** Run → FAIL.
- [ ] **Step 3: Implement.**
  - Fetch `logUrl` when the log section enters view or the reader presses "Read every prompt (N)"; show size and a loading row; on error show a retry button (no crash).
  - Virtualized list (fixed estimate + measured rows; no new dependency unless needed — if one is added, pin an exact version ≥ 2 weeks old and justify in the report).
  - Row: date-time (Pacific, `YYYY-MM-DD HH:mm`), lane label, model, effort, mode (or "unknown" / "inferred from commits"), length; text as plain text with `white-space: pre-wrap`, collapsed beyond 6 lines with an expand button; ids rendered by `Glossed` as `<abbr>`-like focusable spans with the plain words as tooltip (existing `Tooltip`); badges (settings unknown, pasted text not kept, article work, redacted, command, decision); commits as `CommitLink`s (max 5 shown + "+N more" expanding).
  - Filters UI: search box, lane toggles, model/effort/phase selects, decisions-only, hide article work; "no prompts match" row; result count; all keyboard reachable; filter state shared with Task 4 via the URL fragment.
  - Longest-prompt and timeline brushing scroll/filter the log.
- [ ] **Step 4:** Tests PASS; lint/build; Playwright at 1100/390 px including: open log, search "continue", toggle decisions-only, expand a long prompt, click a commit link (opens Rustty URL), keyboard-only pass over filters; no console errors; no page-level horizontal scroll; render a fixture row with markdown/HTML characters literally.
- [ ] **Step 5: Commit** `porting: every-prompt log`.

### Task 6: Prose, placement, dataset commit, links and final checks

**Files:**
- Modify: `src/app/porting/content.mdx` (new `## Every prompt` between `## How the orchestration changed` and `## What it cost`, with `<PortingWidget slug="every-prompt" />`; agents-vs-human caption/summary or adjacent prose links to `#every-prompt`)
- Modify: `scripts/porting/facts.csv` (final `href`s)
- Modify: `docs/superpowers/plans/journey-review-checklist.md` (new items below)
- Research worktree: commit the **redacted** log JSON and manifest under `.port/porting-article/research/data/` (copy `public/porting/every-prompt-log.json` as `prompts-redacted-2026-10-10.json` + `prompts-manifest-2026-10-10.json` + the approved redactions CSV **with the `match` column removed**), never the raw JSONL.

- [ ] **Step 1:** Write the prose (1–2 short paragraphs, author voice, every number a `<Fact>`): what the log is and its three sources; that the text is verbatim and internal ids carry a glossary hover; what was lost (transcripts before 8 September, so model/effort/mode unknown there — the model where shown is inferred from commit trailers; pasted text kept for only a few prompts); redactions (count, reasons, nothing else changed); article-work prompts included and tagged; the section's window differs from the "to 8 October" figures elsewhere.
- [ ] **Step 2:** Commit the redacted dataset in the research worktree (`docs(porting): redacted prompt log and manifest`); set the new facts' `href` to permalinks at that commit (manifest lines for counts); point the registry `sourceHref` there.
- [ ] **Step 3:** Checklist items: (a) re-read the redaction decisions and the rendered log before push; (b) the prompt log is under the same embargo — push only after the advisories; (c) glossary-extra entries added by the author.
- [ ] **Step 4:** Full quality bar (Global Constraints) + `verify:porting` link/quote guards; Playwright pass of the whole page at 1100/390 px.
- [ ] **Step 5: Commit** `porting: Every prompt section`.

---

## Self-review notes

- Spec coverage: scope (T1), verbatim + glossary (T1/T3/T5), redaction gate (T1/T2/T3), purged-window flag (T1/T3/T4/T5), pasted-lost flag (T1/T5), article-work tag (T1/T5/T6), timeline/tiles/settings/log (T4/T5), URL-fragment filters (T4/T5), lazy load (T5), pipeline outputs (T1/T3), dataset committed redacted (T6), checks (T3–T6), prose + heatmap link (T6). No gaps found.
- Placeholders: the facts' interim href (T3) is explicitly replaced in T6.
- Type consistency: `Filter`, `applyFilter`, `filterRows`, `is_decision`, flags bitmask used consistently across T1/T3/T4/T5.
