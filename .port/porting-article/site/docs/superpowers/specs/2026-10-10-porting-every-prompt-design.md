# "Every prompt" — prompt transparency section for "Porting Ghostty to Rust"

Date: 2026-10-10 · Status: design approved in chat; spec pending author review
Parent spec: `2026-10-08-journey-article-design.md` (all its author decisions and quality bar apply).

## Goal

Show every human prompt that went into the port, verbatim, with the settings each ran under where
known, and flag openly what was lost or redacted. Readers should be able to see the actual steering
behind any stretch of the story.

## Author decisions (2026-10-10, binding)

- Scope: Claude Code prompts on the Mac and the Linux VM in the rustty project; the author's Codex
  app prompts (automated Codex auto-review sessions excluded); this article's own prompts, included
  and tagged "article work". Prompts the orchestrator sent to subagents are not included.
- Verbatim: exactly as typed. Internal ids (R7, UD131, G3b, …) keep their spelling and get a
  dotted underline with a plain-language hover from the glossary. Redactions only for secrets,
  other people's private information and unrelated projects, each rendered as
  `[redacted: <reason>]` and counted on the page. The author approves a redaction list before any
  text ships.
- Placement: approach A — in-article section, full log loaded on demand.
- Embargo: same as the rest of the article (nothing pushed before the advisories are filed).

## What the data holds (checked 2026-10-10)

| Source | Prompts | Settings |
|---|---|---|
| Mac `~/.claude/history.jsonl` (project path contains `rustty`) | 808 with text, 2026-06-22 → 2026-10-10 | model/effort/mode from transcripts only from 2026-09-08; before that purged (Claude Code ~30-day cleanup) |
| Linux VM Claude history | ~165, 2026-07-18 → 2026-08-14 | complete |
| Codex app session files (Mac) | human turns inside the non-auto-review sessions among 904 | complete (model, effort, sandbox, approval policy) |

Pasted content survives for only 14 Mac prompts; elsewhere the history shows `[Pasted text #N]`.
Session-level settings: `~/projects/rustty-stats/data/sessions.csv` (`models`, `efforts`, `mode`,
`permission_mode(s)`), message-level: `messages.csv.gz` (`model`, `effort`, `speed`).
Before 2026-09-08 the Mac model can only be inferred from commit trailers (`Co-Authored-By`,
`Claude-Session`) and is labeled "inferred from commits".

## Section and prose

New `## Every prompt` after `## How the orchestration changed`, before `## What it cost`.
One to two short paragraphs in the author's voice: what the log is, where it comes from, what was
lost (transcripts before 8 September → settings unknown; pasted text), what was redacted (count via
`<Fact>`), and that article-work prompts are included and tagged. The agents-vs-human widget's
caption in section 5 links to this section. All numbers are `<Fact>`s linked to the dataset.

## The instrument (flagship widget `every-prompt`)

1. **Timeline strip** 2026-06-21 → 2026-10-10: three lanes (Mac Claude Code, VM Claude Code,
   Codex); one tick per prompt, height = length (log scale); commits-per-day band beneath; the
   window before 2026-09-08 hatched and labeled "transcripts purged — model and effort unknown";
   phase bands from the strategy-timeline data. Brushing a range filters tiles and log; keyboard
   alternative: range inputs.
2. **Tiles**, live for the current filter: prompts; median length; share under ten words; decision
   answers (prompts that are only a choice/ack such as A, B, ok, yes, go, continue — rule defined
   in the builder and documented); commits per prompt; the longest prompt (jump link); the most
   repeated prompts (top 5); redactions.
3. **Settings view**: per-week stacked bars of model × effort (and mode as a toggle); weeks without
   known settings drawn as hatched "unknown", never dropped; pre-09-08 Mac model from commit
   trailers marked "inferred from commits".
4. **Full log**, fetched when the reader opens it or scrolls it into view; virtualized rows. Row:
   date-time (Pacific), tool + machine, model, effort, mode, length, verbatim text (long prompts
   expand). Badges: "settings unknown", "pasted text not kept", "article work", "redacted".
   Commits: links to commits made between this prompt and the session's next prompt (session id +
   time window; plus `Claude-Session` trailer matches), rendered with CommitLink. Filters: search,
   tool, model, effort, phase, decisions-only, hide article work; filter state in the URL fragment
   (`#every-prompt?…`) so a view can be linked.

Data budget: the timeline summary ships with the page chunk (target < 60 KB); the log text is a
separate static JSON fetched on demand (no limit beyond the site's static size sanity, target
< 600 KB gzipped).

## Pipeline

- **Extractor** (rustty side, `.port/porting-article/tools/extract_prompts.py`, stdlib): reads the
  three sources, keeps human turns only (no tool results, no system/command-output turns, no
  subagent sessions, no auto-review sessions), assigns stable ids (`<tool>-<machine>-<ts>-<n>`),
  joins session settings and commit windows, tags article-work sessions (sessions after
  2026-10-08 whose subject is the article — rule documented), and writes
  `research/data/prompts-2026-10-10.jsonl` plus `prompts-manifest-2026-10-10.json` (counts per
  source, per badge, dropped rows by reason).
- **Redaction**: a candidate scanner (secrets/token patterns, emails, phone numbers, personal names
  not in an allowlist, paths outside the rustty/ghostty projects, mentions of unrelated projects)
  writes `prompts-redactions-proposed.csv` (id, span, reason). The author edits/approves it into
  `prompts-redactions-approved.csv`; only approved redactions are applied; the builder refuses to
  emit text while any proposed row is unreviewed.
- **Glossary**: ids found in prompts are matched against `research/r2-glossary.md`; unknown ids are
  listed in `prompts-glossary-missing.txt` for the author.
- **Website builder** `scripts/porting/widgets/w12_every_prompt.py`: writes
  `src/app/porting/data/every-prompt.json` (summary) and `public/porting/every-prompt-log.json`
  (log); facts rows for the prose.
- The redacted dataset is committed to the research branch (`journey-article-research`), never the
  raw extract.

## Checks

- Counts reconcile: every source prompt appears in the output or in the manifest's dropped list
  with a reason; per-source totals match the sources.
- No unreviewed redaction; every applied redaction renders as `[redacted: reason]` and is counted.
- Every badge is backed by data; no row claims settings in the purged window unless inferred and
  marked so.
- Existing article figures windowed "to 8 October" stay unchanged; this section states its own
  window (to 10 October, article work included and tagged).
- Quality bar of the parent spec: lint/build/verify scripts, link guard (facts have hrefs), a11y
  (keyboard brushing alternative, tooltips, focus, reduced motion), 390 px stacked rows, no
  page-level horizontal scroll, no console errors.

## Non-goals

No subagent prompts, no model responses, no rewriting of prompts beyond approved redactions, no
sentiment analysis or scoring of prompts.
