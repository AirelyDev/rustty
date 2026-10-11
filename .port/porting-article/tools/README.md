# Porting article tools

## `extract_prompts.py`: every human prompt, for the "Every prompt" section

Collects every prompt the author typed into Claude Code (Mac and the Linux VM) and the Codex app
for the Rustty port, joins each one to the settings it ran under and the commits made after it,
tags article work, and proposes redactions. Standard library only.

```sh
# 1. fetch the VM's prompt history (the VM must be running; its IP can move between restarts)
rsync -a rvm:.claude/history.jsonl ~/projects/rustty/.port/journey-article/research/tmp/vm-history.jsonl
# 2. extract (cutoff = now unless --cutoff is given); proposes founding sessions, keeps none
python3 .port/porting-article/tools/extract_prompts.py --vm-status "fetched <when> from rvm"
# 3. after the author's review of the founding proposals: keep the approved founding sessions
python3 .port/porting-article/tools/extract_prompts.py --vm-status "..." \
  --founding ~/projects/rustty/.port/journey-article/research/data/prompts-founding-sessions-approved-<date>.csv
# tests (in-memory fixtures only)
python3 -m unittest discover -s .port/porting-article/tools -p 'test_*.py' -v
```

Always run the tests with the `discover` form above (a dotted file path starting with `.port`
gives unittest an empty module name).

Options: `--research` (research dir, default `~/projects/rustty/.port/journey-article/research`),
`--rustty-repo`, `--stats` (rustty-stats `data/`), `--vm-history`, `--vm-status`, `--codex-dir`,
`--claude-dir`, `--people` (one personal name per line; default `<research>/data/prompts-people.txt`),
`--allowlist` (default `redaction-allowlist.txt` here), `--cutoff` (ISO UTC), `--founding` (approved
founding-sessions CSV: the proposed file's columns with `decision` = `include` or `exclude`; a row
without a valid decision is refused; without the flag no founding prompt is kept).

The output date suffix is `DATE_TAG` in the script (now `2026-10-11`); earlier runs' files are
never overwritten.

### Outputs (in `<research>/data/`, untracked, UNREDACTED: never commit them)

| File | Content |
|---|---|
| `prompts-raw-<date>.jsonl` | one prompt per line: `id, ts, tool, machine, session, text, chars, kind, pasted, model, effort, mode, settings_source, approval, sandbox, article_work, commits, commits_basis, ids, phase, decision, branch, project, founding` |
| `prompts-manifest-<date>.json` | publishable: per-source `in_source / kept / dropped{reason}`, cutoff, VM status, rules, counts only (no home paths, IPs or project names; checked before writing) |
| `prompts-manifest-private-<date>.json` | debugging only, never published: input paths, the free-text `--vm-status` note, the `--founding` path, other-project names |
| `prompts-redactions-proposed-<date>.csv` | `id,start,end,match,reason,decision` (decision empty = unreviewed; the author writes `redact` or `keep`); covers founding prompts once they are kept |
| `prompts-glossary-missing-<date>.txt` | `id<TAB>count<TAB>first prompt id<TAB>family` for ids not in `r2-glossary.md` |
| `prompts-article-sessions-<date>.csv` | the sessions tagged as article work, with the reason |
| `prompts-founding-sessions-proposed-<date>.csv` | `session,tool,machine,project,first_ts,last_ts,prompts,reason,decision` (decision empty; the author writes `include` or `exclude` into the approved copy) |
| `prompts-sessions-<date>.jsonl` | one session per line: `session, key, tool, machine, first_ts, last_ts, prompts, commands_only, founding, article_work, branches, models, commits, commits_ambiguous, cc_title` |

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
  `Co-Authored-By` model of the prompt's own linked commits after the dedupe (effort unknown) >
  `unknown`. Mode is the transcript's `permissionMode` at the prompt, else
  the rustty-stats session mode (`bypassPermissions` → `bypass`). Codex mode: `plan` for plan
  collaboration mode, `bypass` for approval `never` + `danger-full-access`, else `default`.
- **Commits.** See `COMMITS_RULE` in the script (also written to the manifest): a window from the
  prompt to the session's next prompt (capped at 12 h and at the extraction cutoff); `session-window` when the session's branch
  is known, `time-window-branch` when it is not but only one branch was active, else `none`.
  Claude-Session trailers and the `[branch sha]` lines git prints into surviving transcripts keep
  a commit away from sessions that did not make it. After linking, a commit claimed by two or more
  human sessions keeps only links backed by that evidence; prompts left with no commits get basis
  `ambiguous` (`dedupe_links`).
- **Decision** (`is_decision`): after lowercasing and stripping punctuation, a single choice letter,
  or 1-3 words all from `{ok, yes, y, no, go, ahead, continue, proceed, sure, agreed, lgtm, a, b, c, d}`.
- **Article work.** A session with a prompt on/after 2026-10-08 00:00 Pacific AND (a project
  path containing `ghostty-website` OR a prompt matching
  `\b(article|journey|website|porting page|disclosure)\b`, case-insensitive). Only its prompts on
  or after the cutoff are tagged.
- **Ids.** `ID_PATTERN` and `PROCESS_LABELS` are copied from the website repo's
  `scripts/porting/jargon.py` (`find_internal_ids`), plus the `FX<n>` family.
- **Phase.** The strategy timeline's era (latest dated `strategy-timeline-*.csv`) in force on the
  prompt's Pacific date; the one-day `external` marker is skipped. A founding prompt dated before
  the first phase starts takes the first phase.
- **Founding sessions** (`FOUNDING_RULE`). Candidates: Claude history sessions with prompts on or
  after 2026-06-20T00:00Z in `~/projects` itself or `~/projects/rusty*` before the first prompt in a
  `rustty` path (any machine), or in the reference checkout `~/projects/ghostty*` (not
  `ghostty-website*`) up to the cutoff. The checkout is searched past the first `rustty` prompt
  because its two prompts came two days later. One reason each, first match wins:
  `ghostty-checkout`, `port-decision` (a prompt matching `port|porting|ported` and
  `zig|rust|ghostty`), `naming` (`rustty|rusty|name it|naming|call it`, or the `rusty` folder),
  `other`. Candidates are only proposed. With `--founding`, every prompt of an `include`d
  `(tool, machine, session)` from 2026-06-20T00:00Z to the cutoff is kept with `founding: true`,
  whatever its folder; everything else follows the project and window rules unchanged. A session
  mixing founding and non-founding prompts stops the run. Founding detection covers Claude history
  (Mac and VM) only; Codex was checked (rustty-stats `sessions.csv`, 2026-06-20 to 06-24) and has
  no session in a qualifying folder.
- **Sessions** (`SESSION_RULE`, after `dedupe_links`). Key `<tool>-<machine>-<first 8 of the id>`
  (longer only if two ids of the same tool and machine share 8 characters). `commits` is the union
  of the session's prompt links; `commits_ambiguous` counts its prompts left `ambiguous` (their
  commits are not listed). `commands_only` when every prompt is `kind=command`. `cc_title`:
  rustty-stats `sessions.csv` `ai_title`, else `title` (Claude sessions; `vm` = stats machine
  `rvm`), else null. The run refuses to write when Σ session prompts ≠ prompts.
- **Redaction candidates** (`scan_redactions`): `secret` (sk-/ghp_/xox…/AWS keys, `-----BEGIN`,
  Bearer tokens, `sshpass -p`, credentials in any phrasing: "password is X", "pwd X", "pw: X",
  "passw X", "pass: X", "login user/pass", "api_key = …"; a common word after the credential word
  does not count), `email` (except `noreply@anthropic.com`), `user-host` (user@host or user@IP that
  is not an email, e.g. ssh targets and shell prompts), `ip` (IPv4, private ranges included),
  `phone`, `unrelated-path` (home paths
  outside `projects/rustty*` / `projects/ghostty*`), `person` (names from `--people`),
  `unrelated-project` (leaf names of the other projects in the prompt history). Spans inside an
  allowlisted phrase (`redaction-allowlist.txt`) are not proposed. Only the author's `redact`
  rows are ever applied, downstream.

### Runs

- **2026-10-11** (`DATE_TAG` 2026-10-11), `--cutoff 2026-10-11T01:51:58Z` (the 2026-10-10 run's
  cutoff, so its counts and redaction reviews carry over), no `--founding`. VM history: the copy
  rsynced on 2026-10-11T01:28Z (its last prompt is 2026-08-22T17:27:58Z), reused; the VM was not contacted.
  1,010 prompts (823 Mac, 165 VM, 22 Codex), identical to 2026-10-10 apart from the new `founding`
  field (commit windows now stop at the cutoff, so later research commits are not linked). 93 sessions (66 Mac,
  21 VM, 6 Codex; 44 command-only; 16 with a Claude Code title). Founding candidates: 4 sessions,
  16 prompts (one each `port-decision`, `naming`, `ghostty-checkout`, `other`). Redaction proposals
  unchanged (20 spans in 15 prompts).
