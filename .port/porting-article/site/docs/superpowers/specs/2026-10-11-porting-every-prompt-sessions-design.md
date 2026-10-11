# "Every prompt" — sessions and founding prompts (addendum)

Date: 2026-10-11 · Status: design approved in chat; spec pending author review
Extends: `2026-10-10-porting-every-prompt-design.md` (all its rules still apply: verbatim text,
approved redactions only, refusal on unreviewed or stale reviews, counts that reconcile, embargo).

## Goal

Show the prompts in the context of the sessions they were typed in, so a reader sees how the port
split into sessions, which areas they covered and when sessions ran in parallel — not only how many
prompts there were. Include the founding prompts typed before the repository existed.

## Author decisions (2026-10-11, binding)

- Include the founding prompts via an explicit list of founding sessions the author confirms at the
  review gate — not by loosening the project-path filter.
- Use the word **session** throughout (UI, prose, data). Never "workstream".
- Each session carries a **title** and an **area**:
  - title: Claude Code's own session title where it survived (16 sessions), otherwise written by
    Claude in the same short style, shown with a "title by Claude" marker;
  - area: one of a fixed list — engine, renderer and fonts, macOS app, GTK/Linux, performance,
    upstream catch-up, verification/CI, defects, tooling, article;
  - the author reviews every title and area at the same gate as the redactions.

## Data (checked 2026-10-11)

- 93 sessions in the current extract: 66 Mac Claude Code, 21 VM, 6 Codex; median 2 prompts,
  maximum 168; 39 single-prompt sessions, mostly `/resume`, `/model`, `/clear`.
- Founding candidates (Mac `~/.claude/history.jsonl`, 21 June UTC): session `1662fcc7…` in
  `~/projects` (repository checkout, line count, architecture read, the port decision, naming
  Rustty); session `3afc661c…` in `~/projects/rusty`; the two prompts typed in the Ghostty checkout
  the research notes mention (to be identified by the extractor and confirmed by the author).
  Other sessions in `~/projects` that day (e.g. a question about Claude's workflows) are not port
  work unless the author adds them.

## Sessions

- Session key: `(tool, machine, session id)`.
- Per session: first and last prompt time, duration, prompt count, commits linked (existing rules,
  including "ambiguous"), branch/worktree and models where known, source lane, founding flag,
  article-work flag (existing overrides apply per session), title, title source
  (`claude-code` | `claude-written`), area.
- Command-only sessions (every prompt is a slash command) are grouped as "command-only sessions":
  counted and listed, collapsed by default, never dropped.

## Review gate (author)

One stop after extraction, covering: the founding-session list; redaction proposals for any new
prompts and for every proposed title; the title and area for every session. Files (untracked
research data): `prompts-founding-sessions-proposed-2026-10-11.csv`,
`prompts-sessions-proposed-2026-10-11.csv` (`session, title, title_source, area, decision,
edited_title, edited_area`), and the redaction CSVs. The builder refuses while any row lacks a
decision, and refuses stale reviews (proposal text changed since review), as for redactions.

## Widget

1. **Session timeline** (new top view): one bar per session from first to last prompt; rows packed
   so overlapping sessions stack; colour + text label = area (colour never the only encoding);
   bar thickness = prompt count (bucketed); founding sessions marked; hatched purged window kept.
   Hover/focus: title, area, dates, duration, prompts, commits. Click/Enter: selects the session
   (filters everything below) and opens its card. Keyboard: roving focus across bars.
2. **Session tiles**: sessions; sessions per area (small bar list); longest session by prompts, with the days its first and last prompt
   spanned (resumed sessions; no hours of work); busiest parallel day (most sessions open the same Pacific day); median prompts per
   session. Existing prompt tiles stay, filtered.
3. **Log**: default mode = session cards (title, "title by Claude" marker where applicable, area,
   dates, duration, counts, commits), expanding to the session's prompts in order; toggle to the
   existing flat prompt list. Command-only sessions collapsed in one group.
4. **Filters and links**: area filter and a session filter join the existing filters; the URL
   fragment can name a session (`#every-prompt?s=<short id>`); bare `#every-prompt` resets.
5. Existing prompt timeline, settings view and flat log keep working, filtered by the session
   selection.

## Prose

The section opens with what the sessions show (how the work split up, parallel sessions, the
founding session), then the existing paragraphs. The window becomes 21 June → cutoff; the
"left out" wording for founding prompts is replaced by "typed before the repository existed".
Every number is a `<Fact>` with an href.

## Checks

- Every prompt belongs to exactly one session; session prompt counts sum to the prompt total.
- Founding prompts appear only from approved founding sessions.
- No title ships unreviewed; titles pass the redaction scan; no internal ids in titles unless the
  glossary covers them (titles are written text, so prefer plain words).
- Numbers agree across prose facts, registry caption/summary (guarded), tiles and the log.
- Accessibility: roving focus on bars, cards as a list with expandable regions, single live region.
- Page weight: session summary data in the bundled summary (< 60 KB total), prompts stay in the
  lazy log.
