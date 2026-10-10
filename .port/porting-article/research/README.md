# Research behind "Porting Ghostty to Rust"

This directory holds the research dossiers and datasets behind the article "Porting Ghostty to
Rust" (the `/porting` page of the Ghostty website fork). Every number in the article comes from
the article's fact table, and each fact names its source: a file in this directory or a record
elsewhere in this repository. The article's snapshot is Rustty `58aa62a81` and Ghostty `246f70287`,
taken on 2026-10-10. Figures from the Claude Code and Codex session records stop at 2026-10-08.

Provenance tags used throughout: MEASURED (a command or script is given), DOCUMENTED (a record path
is given), ESTIMATED (a model or a hand classification is described).

## Dossiers and the article sections they feed

| File | Article sections |
|---|---|
| `BRIEF.md` | The synthesis of the round-1 and round-2 dossiers: outline, fact-checked headline numbers, widget inventory, contradictions resolved. Hardware in the methodology appendix. |
| `refresh-2026-10-10.md` | The 2026-10-10 re-snapshot: every fact whose value changed from the 2026-10-08 snapshot, with the exact command or record. Most current-snapshot figures cite it. |
| `timeline.md` | At a glance; The reference as a contract; The first three weeks (commit cadence, lines and tests over time, crate creation, reference moves, notable firsts). |
| `strategy.md` | The reference as a contract (gates on day 1); The method matures (the Bun realignment, rules, ledger, detectors); Checks as the specification; How the orchestration changed. |
| `codebase.md` | Cutting a 345-file cycle into crates (the import cycle and its cuts); Unsafe, measured (the census history); the z2d subset. |
| `bugs.md` | Zig pointers in Rust's aliasing model; Never port a bug (the defect ledger); The verification matrix (lane results, Miri timings). |
| `perf.md` | Performance (the July campaign, method, wall-clock versus per-operation slopes, LTO, the parity bug that profiling found, the August and October rounds). |
| `bun-article.md` | The method matures (what Rustty took from "Rewriting Bun in Rust"). |
| `usage-stats.md` | What it cost (token mix, delegated share, cost per commit); How the orchestration changed (context per call, commits per prompt). |
| `r2-cost.md` | What it cost (plans, the maxed-out-week ceiling, the lost-window model, paid and list-price totals). |
| `r2-data-fixes.md` | Never port a bug (discovery-method hand pass); Checks as the specification (silent skips). |
| `r2-platforms.md` | Limitations and what's left (per-platform claims, the Linux GTK history). |
| `r2-glossary.md` | Plain-language names for internal ids, and the quotable rule texts. |
| `r2-outline-plain.md`, `website.md` | Article outline and website integration notes. |

`data/` holds the datasets the dossiers and the article's widgets read. File names start with the
dossier that produced them (`timeline-`, `strategy-`, `bugs-`, `perf-`, `codebase-`, `usage-stats-`,
`r2-…`) or with `mock-` for the widget datasets. A `-2026-10-10` suffix marks the dataset regenerated
for the 2026-10-10 re-snapshot; the file without the suffix is the 2026-10-08 version, kept as it was.

`links-2026-10-10.csv` maps every fact, commit hash, source-file mention, external reference and
widget in the article to the URL it links to, and `links-2026-10-10.md` explains the choices (both added in a follow-up commit).

## What was left out

- `tmp/` is excluded: scratch scripts and raw exports of usage and message data.
- Files whose content is raw prompt or message text were excluded after a privacy scan:
  `r2-human-story.md` and `r2-codex-audit.md` (the human-story and Codex-audit dossiers, which
  quote prompts at length), and the datasets `data/r2-human-story-quotable-prompts.csv`,
  `data/r2-human-story-codex-root-prompts.csv`, `data/r2-human-story-prompt-gaps.csv`,
  `data/r2-human-story-limit-mentions.csv`, `data/r2-cost-limit-events.csv` and
  `data/r2-cost-limit-events-2026-10-10.csv`. Their aggregate datasets (counts per day, hour, era
  and week) are included.
