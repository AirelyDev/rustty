# Porting article tools

## `extract_prompts.py`: every human prompt, for the "Every prompt" section

Collects every prompt the author typed into Claude Code (Mac and the Linux VM) and the Codex app
for the Rustty port, joins each one to the settings it ran under and the commits made after it,
tags article work, and proposes redactions. Standard library only.

```sh
# 1. fetch the VM's prompt history (the VM must be running; its IP can move between restarts)
rsync -a rvm:.claude/history.jsonl ~/projects/rustty/.port/journey-article/research/tmp/vm-history.jsonl
# 2. extract (cutoff = now unless --cutoff is given)
python3 .port/porting-article/tools/extract_prompts.py --vm-status "fetched <when> from rvm"
# tests (in-memory fixtures only)
python3 -m unittest discover -s .port/porting-article/tools -p 'test_*.py' -v
```

`python3 -m unittest .port/porting-article/tools/test_extract_prompts.py` does not work: the
leading `.port` makes unittest derive an empty module name. Use `discover` as above.

Options: `--research` (research dir, default `~/projects/rustty/.port/journey-article/research`),
`--rustty-repo`, `--stats` (rustty-stats `data/`), `--vm-history`, `--vm-status`, `--codex-dir`,
`--claude-dir`, `--people` (one personal name per line; default `<research>/data/prompts-people.txt`),
`--allowlist` (default `redaction-allowlist.txt` here), `--cutoff` (ISO UTC).

### Outputs (in `<research>/data/`, untracked, UNREDACTED: never commit them)

| File | Content |
|---|---|
| `prompts-raw-2026-10-10.jsonl` | one prompt per line: `id, ts, tool, machine, session, text, chars, kind, pasted, model, effort, mode, settings_source, approval, sandbox, article_work, commits, commits_basis, ids, phase, decision, branch, project` |
| `prompts-manifest-2026-10-10.json` | per-source `in_source / kept / dropped{reason}`, cutoff, VM status, rules, counts per badge |
| `prompts-redactions-proposed-2026-10-10.csv` | `id,start,end,match,reason,decision` (decision empty = unreviewed; the author writes `redact` or `keep`) |
| `prompts-glossary-missing-2026-10-10.txt` | `id<TAB>count<TAB>first prompt id<TAB>family` for ids not in `r2-glossary.md` |
| `prompts-article-sessions-2026-10-10.csv` | the sessions tagged as article work, with the reason |

### Rules

- **Sources and units.** Claude: one unit per row of `history.jsonl` (typed prompts and slash
  commands; subagent prompts never reach the history). Codex: one unit per user-role `message`
  item of a rollout file (rollouts present in both `sessions/` and `archived_sessions/` are read
  once). Every unit is either kept or dropped with a reason, and `kept + Σ dropped == in_source`
  is checked before anything is written.
- **Project filter.** History `project` / Codex `cwd` contains `rustty` (worktrees included, as in
  rustty-stats). Paths containing `ghostty-website` are kept from 2026-10-08 00:00 Pacific on
  (article work). Window: 2026-06-21 00:00 Pacific to the cutoff.
- **Codex human turns.** Subagent rollouts (`drop: subagent`) and auto-review rollouts (model
  `codex-auto-review` or `guardian_review` threads; `drop: auto-review`) are dropped whole. Inside
  a root session, injected context (`# AGENTS.md instructions`, `<environment_context>`,
  `<recommended_plugins>`, `<codex_internal_context>` goal nudges, `<subagent_notification>` …)
  is dropped as `injected-context`; replays of the same turn are `duplicate`.
- **Kind.** `command` when the text starts with a slash command (`/clear`, `/goal …`); a path
  such as `/Users/…` is a prompt.
- **Pasted text.** `none` without a `[Pasted text #N]` placeholder; `kept` when every referenced
  paste body survives (inline in the history row or in `~/.claude/paste-cache`); `lost` otherwise.
  The text is always the verbatim display; pastes are never expanded.
- **Settings** (`settings_source`): `transcript`, the model call answering the prompt (live
  transcript, else the rustty-stats per-call rows; for Codex the turn context, with approval policy
  and sandbox) > `session`, the session's only model in rustty-stats > `commit-trailer`, the
  `Co-Authored-By` model of the prompt's commits (or of every commit in its window when they all
  agree; effort unknown) > `unknown`. Mode is the transcript's `permissionMode` at the prompt, else
  the rustty-stats session mode (`bypassPermissions` → `bypass`). Codex mode: `plan` for plan
  collaboration mode, `bypass` for approval `never` + `danger-full-access`, else `default`.
- **Commits.** See `COMMITS_RULE` in the script (also written to the manifest): a window from the
  prompt to the session's next prompt (capped at 12 h); `session-window` when the session's branch
  is known, `time-window-branch` when it is not but only one branch was active, else `none`.
  Claude-Session trailers and the `[branch sha]` lines git prints into surviving transcripts keep
  a commit away from sessions that did not make it.
- **Decision** (`is_decision`): after lowercasing and stripping punctuation, a single choice letter,
  or 1-3 words all from `{ok, yes, y, no, go, ahead, continue, proceed, sure, agreed, lgtm, a, b, c, d}`.
- **Article work.** A session with a prompt on/after 2026-10-08 00:00 Pacific AND (a project
  path containing `ghostty-website` OR a prompt matching
  `\b(article|journey|website|porting page|disclosure)\b`, case-insensitive). Only its prompts on
  or after the cutoff are tagged.
- **Ids.** `ID_PATTERN` and `PROCESS_LABELS` are copied from the website repo's
  `scripts/porting/jargon.py` (`find_internal_ids`), plus the `FX<n>` family.
- **Phase.** The strategy timeline's era (latest dated `strategy-timeline-*.csv`) in force on the
  prompt's Pacific date; the one-day `external` marker is skipped.
- **Redaction candidates** (`scan_redactions`): `secret` (sk-/ghp_/xox…/AWS keys, `-----BEGIN`,
  "password is …"), `email` (except `noreply@anthropic.com`), `phone`, `unrelated-path` (home paths
  outside `projects/rustty*` / `projects/ghostty*`), `person` (names from `--people`),
  `unrelated-project` (leaf names of the other projects in the prompt history). Spans inside an
  allowlisted phrase (`redaction-allowlist.txt`) are not proposed. Only the author's `redact`
  rows are ever applied, downstream.
