# Timeline dossier — Rustty port (Ghostty Zig → Rust)

Scope: chronological reconstruction from git (`git -C /Users/gael/projects/rustty log --all`), `.port/*` records and
`docs/superpowers/plans/*`. All dates are author dates, timezone -0700 (2,984 of 2,994 commits; the 10 +0000 ones are
dependabot). Snapshot taken at `1d22ba80d` (HEAD of `phase-8-upstream-resync`, 2026-10-08).

Provenance tags: **[M]** MEASURED (command given or in the "Method" section), **[D]** DOCUMENTED (path given),
**[E]** ESTIMATED.

---

## 1. Headline numbers

| Fact | Value | Prov. | Evidence |
|---|---|---|---|
| First commit | 2026-06-21 15:36, `15c1a9bd7` "phase-1 workspace scaffold + phase-0 artifacts" | M | `git log --all --reverse` |
| Span to snapshot | 110 calendar days (2026-06-21 → 2026-10-08) | M | same |
| Commits, all refs | 2,994 (2,968 reachable from HEAD) | M | `git rev-list --all --count` |
| Non-merge commits | 2,728 | M | `git log --all --no-merges` |
| Authors | Gael Hatchue 2,983; dependabot 10; "rustty port" 1 (the very first commit) | M | `git log --all --format=%an \| sort \| uniq -c` |
| Days with ≥1 commit | 58 of 110 | M | `data/timeline-commits_by_day.csv` |
| Busiest day | 2026-09-23, 249 commits (parity closure) | M | same |
| Rust LOC at day 1 end | 33,040 (crates + sys + xtask + .port harnesses) | M | `git grep -c '' <rev> -- '*.rs'` |
| Rust LOC at snapshot | 857,867 total; 818,802 under `crates/` | M | `data/timeline-loc_by_day_wide.csv` |
| Swift LOC at snapshot | 38,180 (`macos/`) | M | same |
| `rustty-vt` LOC | 6,996 (06-21) → 135,431 (06-25) → 263,312 (10-08) | M | same |
| `#[test]` functions | 314 (06-21) → 3,005 (06-28) → 7,284 (10-08) | M | `data/timeline-tests_by_week.csv` |
| Rust lines added / deleted (non-merge, all refs) | +1,171,999 / −313,961 | M | `git log --all --no-merges --numstat` |
| Crates under `crates/` | 44 (29 created by the first commit) + 17 `crates-sys/*` | M | `data/timeline-crate_birth.csv` |
| Upstream pin advances | 7 (Phases 3–8 + one 2-commit drift absorb) | D | `.port/ORACLE_SYNC.md` §History |
| Upstream commits absorbed | 1,716 incl. merges / 1,157 non-merge | D | sum of `.port/phase-{3..8}/UPSTREAM_COMMITS*.txt` + ORACLE_SYNC |
| Upstream diff absorbed | +185,221 / −33,790 lines over 1,436 file-touches | D | `.port/phase-*/UPSTREAM_DIFF_STAT.txt` (sum) |
| Ghostty `*.zig` LOC, baseline → pin | 273,827 (`4749c4e93`) → 366,687 (`a60e9e2a5`), +34% | M | `git -C ../ghostty grep -c '' <pin> -- '*.zig'` (all .zig in the tree, incl. pkg/) |

Caveat on commit counts: commit cadence is **not** a proxy for work. Phases 4 and 5 (2026-08-17 → 08-25) were landed as
23 large squashed wave commits (e.g. `3d6fd16e9` 68 files +30,829; `47b2dbbb7` 76 files +20,099), and the Codex vetting
was a single commit (`8120dd10d`, 532 files, +87,270/−5,513). Use `timeline-churn_by_day.csv` alongside commit counts.

---

## 2. Eras (gantt)

Data: `data/timeline-eras.json` (dates, key commits, evidence). Per-commit era labels from a subject/date classifier:
`data/timeline-commit_era_assignment.csv`.

| Era | Dates | Classified commits | Key commits | Evidence |
|---|---|---|---|---|
| Phase 0 — reference baseline | 06-21 | (artifacts in first commit) | `15c1a9bd7` | `.port/phase-0/REPORT.md`: `zig build test` green, 3,234-test contract, 28 benchmarks, oracle pinned `4749c4e93` [D] |
| Phase 1 — scaffold + gates | 06-21 | 6 | `15c1a9bd7`, `5540e9bae`, `d9fd637ce` (Miri as G6) | `.port/phase-1/REPORT.md`: 43 workspace members, 29 crates, G7 acyclic, 93 edges [D] |
| Phase 2a — vt crown jewel | 06-21 → 06-26 | — | `dc1083ff6` (util), `97b2e30da` (Terminal skeleton), `e9e325358` (first Miri UB fix) | vt `#[test]` 113 → 2,032 in 4 days [M] |
| Phase 2b — upper tiers | 06-26 → 07-08 | — | config C2, font-shaper F3, termio, core C14, embedded E0–E10, GPU R-d chunks 1–4 | subjects; `docs/superpowers/plans/2026-07-08-macos-windowing-sub1.md` |
| macOS windowing + Swift app | 07-08 → 07-12 | 37 | `74c57204f` FIRST PIXELS, `1395249e5`, `d1b4df097` RUNNABLE | |
| Strategy realignment (post-Bun) | 07-10 → 07-12 | — | `2c03357ce`, `ccd14200e` (R7–R12), `8f6d781dc` (LIFETIMES), `3277f2f78` (full vt Miri sweep) | `.port/phase-2/BUN_PORT_LEARNINGS.md`, `.port/PORT_RULES.md`, `.port/audits/GHOSTTY_LIFETIMES.tsv` |
| Rebrand + inspector | 07-13 → 07-16 | 63 | `558107b33` plan, `ae70b8214` done | `docs/superpowers/plans/2026-07-13-full-rustty-rebrand.md` |
| Perf head-to-head round 1 | 07-15 → 07-16 | 37 | `349e5d101` "14/14 WINS vs Zig, suite −21.7%" | `.port/phase-2/PERF_LOG.md` |
| Linux: xev / OpenGL / GTK | 07-16 → 07-22 | 151 | `3dad6d7a2` GTK FIRST LIGHT, `9bc62e574` live terminal in window | `docs/superpowers/plans/2026-07-16-linux-gtk-port-plan.md` |
| Completion program WS1–WS20 | 07-23 → 07-30 | 322 | `b914feff3` roadmap, `c94051557` WS14 wasm/WebGL, `507dcced2` iocp/wasi | `docs/superpowers/plans/2026-07-23-rustty-completion-ROADMAP.md` |
| Phase 3 resync | 07-29 → 07-31 | 224 | `328460511` P3-W0, `0efe06a1c` pin advanced, `18da9ba5b` | `.port/phase-3/README.md` |
| (idle) | 08-01 → 08-09 | 0 | | |
| Codex audit vetting | 08-10 → 08-11 | 2 (squashed) | `d34cc1549`, `8120dd10d` | `.port/codex-vet/README.md`: 147 vet groups, 140 remediated [D] |
| Phase 4 resync | 08-17 → 08-25 | 13 (squashed waves) | `c8e76c6eb` advance, `27c784ab2` "fix 10 live bugs", `d63dd9fc7` finale | `.port/phase-4/README.md` |
| Phase 5 resync | 08-25 | 10 | `cc8c65309`, `29da15fe9` finale | `.port/phase-5/README.md` |
| Gap close + build system | 08-26 | 57 | `7e1f0989d`, `bb4eb315e` (16 build-system defects) | `.port/gap-close/README.md` |
| Perf wave 2 | 08-28 → 09-01 | ~3 | `1d50073f6` "restore comptime/inline semantics", `4348771dc` | subjects |
| Phase 6 resync | 09-01 → 09-08 | 35 | `a964e142c`, `7419838b3`, `d328db1f7` (Linux lane found 3 macOS-invisible defects) | `.port/phase-6/README.md` |
| Parity audit + closure | 09-08 → 09-28 | 1,038 | `079cc21e7` | `.port/parity-closure/README.md`: 497 findings, 404 closed / 93 waived / 0 open [D] |
| Phase 7 resync + memory-safety lanes | 09-28 → 10-07 | 327 | `9076dfefd` cross-platform build GREEN, `2a24cf6c5` file split, `1e84e3ff5` Miri-nextest, `63de65bc2` G-TSAN, `401b95b0e` | `.port/phase-7/README.md`; `docs/superpowers/specs/2026-10-01-memory-safety-lanes-design.md` |
| Phase 8 resync | 10-07 → 10-08 (finale gates GREEN) | 81 | `5e969a4bb` plan, `3b6e760f2` finale, `1d22ba80d` cross-host GREEN | `.port/phase-8/README.md` |
| Ferris icon rebrand | 10-08 | 15 | `432f73e8e` spec, `c17ca9c01` | subjects `icons:*` |

Classifier counts are approximate [E]: rules are subject-regex first (e.g. `P7-`, `parity task`, `WS\d+`), then date
window. The 1,038 "parity-audit-closure" includes a handful of early `parity:` subjects from July.

Shape of the project, in one line: **~5 weeks of greenfield port (06-21 → 07-30), then ~10 weeks of resync/audit/hardening
cycles**, with idle gaps between cycles (08-01→08-09, 08-12→08-16, 08-19→08-24, 09-12→09-21; 52 zero-commit days total).

---

## 3. Commit cadence

`data/timeline-commits_by_week.csv` [M]:

| Week of | Commits | Merges |
|---|---|---|
| 06-15 (06-21 only) | 34 | 0 |
| 06-22 | 259 | 0 |
| 06-29 | 125 | 0 |
| 07-06 | 138 | 0 |
| 07-13 | 297 | 5 |
| 07-20 | 63 | 4 |
| 07-27 | 507 | 91 |
| 08-03 | 0 | 0 |
| 08-10 | 2 | 0 |
| 08-17 | 10 | 0 |
| 08-24 | 73 | 7 |
| 08-31 | 36 | 6 |
| 09-07 | 260 | 1 |
| 09-14 | 4 | 0 |
| 09-21 | 670 | 42 |
| 09-28 | 389 | 83 |
| 10-05 | 127 | 27 |

Observations [M]:
- History is **linear until 2026-07-13** (0 merges); merges appear with parallel worktree agents (WS program, Phase 3+).
  Merges peak in the weeks of 07-27 (91) and 09-28 (83): the workstream-per-branch model of Phases 3 and 7.
- `data/timeline-commits_weekday_hour.csv` gives the weekday × hour heatmap.

### Line churn
`data/timeline-churn_by_day.csv` [M] (non-merge, all refs; buckets: rs / swift / port (.port + docs) / other).
Largest Rust-add days: 2026-06-25 (+113,219 rs; vt Terminal/Screen/formatter/kitty landing), 2026-06-24 (+51,154),
2026-08-25 (+52,262; Phase 4 par-5 + Phase 5 waves), 2026-07-02 (+46,779; termio/core/cli). 2026-10-01 shows
+212,173/−210,491: **mechanical file splits** in P7-W13 (`2a24cf6c5` terminal.rs → 19 files; `ad443e7cd`;
`edae273f4`), not new code. 2026-07-17 "other" +235,240 is vendored GTK/resource content.

---

## 4. Rust LOC over time

Files: `data/timeline-loc_by_day_wide.csv` (one row per calendar day, last HEAD-reachable commit by committer date
≤ that day; one column per bucket), `data/timeline-loc_by_day_long.csv` (tidy: date, rev, bucket, lines, files),
`data/timeline-loc_by_week.csv` (Sundays + final). Lines = raw line count (`git grep -c ''`), including comments,
blank lines and in-file `#[cfg(test)]` modules [M].

Milestones [M]:

| Date | Rev | Rust total | crates/ | Swift | rustty-vt |
|---|---|---|---|---|---|
| 06-21 | `4f02cca87` | 33,040 | 32,124 | 0 | 6,996 |
| 06-22 | `ab11a0add` | 63,892 | 62,976 | 0 | 36,172 |
| 06-25 | `a7156db18` | 228,026 | 226,872 | 0 | 135,431 |
| 07-01 | `c4df312da` | 307,072 | 301,980 | 0 | 135,615 |
| 07-12 | `df7fe998d` | 422,813 | 412,970 | 35,733 | 138,468 |
| 07-23 | `74226dd30` | 522,064 | 507,545 | 35,733 | 142,165 |
| 07-31 | `dc054bbc8` | 593,590 | 575,140 | 36,185 | 166,662 |
| 08-11 | `8120dd10d` | 621,329 | 600,800 | 36,185 | 170,758 |
| 08-25 | `29da15fe9` | 703,502 | 681,689 | 37,611 | 222,999 |
| 09-01 | `7419838b3` | 709,886 | 687,654 | 37,873 | 225,092 |
| 09-28 | `fe7de163e` | 815,868 | 784,923 | 37,936 | 251,183 |
| 10-08 | `1d22ba80d` | 857,867 | 818,802 | 38,180 | 263,312 |

Per-crate at snapshot (top 12) [M]: rustty-vt 263,312; rustty-vt-c 64,328; rustty-apprt 56,047; rustty-renderer
52,275; rustty-xev 46,873; rustty-config 35,802; rustty-font-shaper 35,580; rustty-termio 27,848; rustty-core 27,362;
rustty-font 26,484; rustty-cli 19,744; rustty-vaxis 19,467.

Swift note [M]: Swift LOC on 07-12 is **35,733 — identical to Ghostty's `macos/*.swift` at the baseline pin
`4749c4e93` (35,733)**, consistent with `1395249e5` "import the reused ghostty Swift app (Branch A, byte-identical)".
Swift growth afterwards (→ 38,180) tracks upstream Swift changes absorbed by resyncs (Ghostty at `a60e9e2a5`: 37,965)
plus the rebrand/port-side additions. The Swift app was **reused, not ported**; the Rust side provides the C ABI
(`rustty-apprt-c` / xcframework).

Ratio context [M, rough]: Ghostty has 366,687 lines of `*.zig` at `a60e9e2a5` (whole tree, incl. vendored `pkg/`);
Rustty has 857,867 lines of `*.rs`. This is not like-for-like (Rust count includes ported vendored libs like libxev,
z2d, vaxis, zf, zigimg, a `rustty-zigstd` shim, test harnesses under `.port/`, and xtask/gates). A per-module ratio
belongs to the codebase dossier (`data/codebase-zig-vs-rust-per-module.csv`).

### Tests and placeholders over time
`data/timeline-tests_by_week.csv` [M] (`git grep -E '^[[:space:]]*#\[(tokio::)?test\]'`):
314 (06-21) → 3,005 (06-28) → 3,745 (07-12) → 4,761 (08-02) → 5,795 (08-30) → 6,846 (09-27) → 7,284 (10-08).
vt alone: 113 → 2,032 (06-25) → 3,474. vt-c: 0 → 308 (06-28) → 606.
`unimplemented!(` occurrences under `crates/` (incl. comments; ESTIMATE of placeholder leaves): 8 → 140 (07-05, peak of
"placeholder-leaf" porting) → 113 (07-19) → 139 (10-08). Lines containing `unsafe {`: 264 → 20,418 (crude; the
codebase dossier has a proper unsafe census).

Cross-check: the first full vt Miri sweep on 07-11 (`3277f2f78`) says "all 2037 tests" [D], matching the measured
2,032–2,072 vt `#[test]` count around that date.

---

## 5. Crate creation order

`data/timeline-crate_birth.csv` [M] (`git log --all --reverse --diff-filter=A -- crates/<c>`), plus the first date the
crate reached ≥1,000 lines of Rust.

- **2026-06-21, first commit `15c1a9bd7`** — 29 crates scaffolded from the Phase-0 plan (leaves-first DAG): util,
  build-config, cfg-fmt, identity, datastruct, lib, os, simd, unicode, vt, render-types, args, config, font, input,
  vt-c, apprt-api, crash, renderer, termio, apprt, cli, core, bench, synthetic, inspector, rustty (bin), global,
  terminfo; plus 14 of the 17 `-sys` crates.
- 06-22 `rustty-zigstd` (Zig std shims)
- 06-30 `rustty-font-shaper` (font 3-tier split, decision D1), `rustty-z2d` (ported 2D rasterizer)
- 07-01 `rustty-oniguruma`
- 07-02 `rustty-xev` (libxev port), `rustty-apprt-c` (embedded C ABI)
- 07-03 `rustty-helpgen`
- 07-08 `rustty-wuffs`
- 07-09 `rustty-vaxis`, `rustty-zf`, `rustty-extra`
- 07-15 `rustty-wasm`, `rustty-web-canvas`
- 07-29 `rustty-zigimg`
- 09-23 `rustty-version` (split out during parity closure, UD150)
- `-sys`: 14 on 06-21, 1 each on 07-15, 07-17, 08-11.

"Reached 1k lines" shows the porting order: datastruct/lib/os/simd/unicode/vt on day 1; render-types 06-22;
args/config/font/input/vt-c 06-25; apprt-api/crash/renderer/termio 07-01; apprt/cli/core 07-02; bench/synthetic 07-09;
inspector 07-14; rustty (bin) 07-17; global 07-30; terminfo 08-17; build-config 09-11.

---

## 6. Upstream pin history

`data/timeline-upstream_pins.csv` [D] — from `.port/ORACLE_SYNC.md` §"History of this pin" and each phase README.

| Resync | From → To | Upstream date of target | Advanced in Rustty | Commits (all / non-merge) | Files | +/− | Ghostty .zig LOC at target [M] |
|---|---|---|---|---|---|---|---|
| Baseline | — → `4749c4e93` | 2026-06-20 | 06-21 | — | — | — | 273,827 |
| Phase 3 | `4749c4e93` → `6ad1fe7d8` | 07-29 | 07-31 | 365 / 250 | 507 | +37,683 / −11,323 | 297,110 |
| Phase 4 | → `12967b68f` | 08-17 | 08-17 | 543 / 359 | 381 | +72,554 / −8,106 | 331,362 |
| Phase 5 | → `683d8db64` | 08-25 | 08-25 | 245 / 169 | 155 | +25,534 / −3,360 | 351,780 |
| Gap-close drift | → `88f57ee66` | 08-25 | 08-26 | 2 / 2 | — | — | 351,976 |
| Phase 6 | → `20abdb50a` | 08-31 | 09-01 | 121 / 74 | 75 | +6,261 / −1,269 | 353,734 |
| Phase 7 | → `12752b2ac` | 09-28 | 09-28 | 333 / 234 | 233 | +35,723 / −9,235 | 360,991 |
| Phase 8 | → `a60e9e2a5` | 10-07 | 10-07 | 107 / 69 | 85 | +7,466 / −497 | 366,687 |

Doctrine change [D]: Phase 3 ported the range then advanced the pin at the end. From Phase 4 on, **advance-first**
(explicit user direction 2026-08-17): move the oracle, record the gates going RED against the target
(`.port/phase-4/w0/g1b-after-at-12967b68f.txt`: 10 modules, ~36 dropped tests; Phase 5: 295 dropped tests in 5 modules),
and use the RED as the progress meter until the finale re-greens it. Lag between upstream target date and pin advance
went from 2 days (Phase 3) to same-day (Phases 4, 5, 7, 8).

Resync turnaround [D/M]: Phase 4 08-17 → 08-25 (8 days, 543 commits); Phase 5 same day 08-25 (245 commits); Phase 6
09-01 → 09-08 (121); Phase 7 09-28 → 09-30 finale, long runs to 10-07 (333); Phase 8 10-07 → 10-08 (107).

---

## 7. Notable firsts

`data/timeline-firsts.csv` [M, from subjects; claims in subjects are DOCUMENTED by the commit]:

| Date | Commit | Event |
|---|---|---|
| 06-21 | `15c1a9bd7` | First commit (scaffold + Phase-0 artifacts) |
| 06-21 | `d9fd637ce` | Miri adopted as primary memory/UB/leak gate (G6), tracking allocator dropped |
| 06-23 | `161178d6a` | Sanitizer matrix gate script |
| 06-25 | `e9e325358` | First Miri-found UB fixed (vt-c `tracked_grid_ref`, SB+TB) |
| 06-25 | `a7156db18` | rustty-vt reaches 2,032 tests |
| 07-07 | `b0df56c29` | Offscreen golden-image renderer gate |
| 07-09 | `74c57204f` | FIRST PIXELS — Metal on-screen present |
| 07-10 | `2c03357ce` | Bun-rewrite learnings + realignment plan |
| 07-10 | `8f6d781dc` | LIFETIMES.tsv pointer-field ownership ledger (Bun heritage) |
| 07-11 | `3277f2f78` | First full vt Miri sweep (2,037 tests), real SB UB in search |
| 07-12 | `d1b4df097` | RUNNABLE Rustty terminal on macOS |
| 07-14 | `ae70b8214` | Rebrand complete |
| 07-15 | `992f91f30` | WASM target |
| 07-16 | `349e5d101` | Perf: 14/14 wins vs Zig, suite −21.7% |
| 07-17 | `3dad6d7a2` | GTK FIRST LIGHT on Linux |
| 07-29 | `2690b7139` | Live verification on the Linux VM (rvm, GNOME 50) |
| 07-30 | `507dcced2` | libxev iocp (Windows) + wasi_poll backends |
| 07-31 | `0efe06a1c` | First oracle pin advance |
| 08-26 | `b8e6a9eb8` | Flatpak built, installed and run |
| 09-28 | `079cc21e7` | Parity closure merged (497 findings, 0 open) |
| 09-30 | `9076dfefd` | Cross-platform build check GREEN: Linux, Windows x64/ARM64/GNU, macOS |
| 10-06 | `1e84e3ff5` | Miri lanes on nextest GREEN (G6/G6c/G6t) |
| 10-07 | `63de65bc2` | G-TSAN GREEN on Mac (4,380 tests); Highway target-mask race (UD377) fixed |
| 10-08 | `1d22ba80d` | Phase 8 cross-host build GREEN on rvm, rwin (msvc + gnu) and macOS |

Time-to-milestone [M]: first pixels on day 19; runnable macOS terminal on day 22; Linux GTK first light day 27;
14/14 perf wins day 26; first upstream resync day 41; zero-open parity ledger day 100.

---

## 8. Proposed visualizations

1. **Cumulative commits + daily bars, era bands behind** — combo (bars + line) over a shaded gantt band.
   Data: `timeline-commits_by_day.csv` + `timeline-eras.json`. Why: shows the bursty "sprint / idle / sprint" rhythm
   and that the port proper was ~5 weeks; the rest is resync/audit cycles.
2. **Era gantt** — horizontal bars by group (foundation/port/platform/strategy/perf/resync/audit), key commits as
   ticks. Data: `timeline-eras.json`. Why: one-glance map of the journey; overlapping eras show parallelism.
3. **Rust LOC stacked area by crate (top 10 + other), Swift line overlaid** — data: `timeline-loc_by_day_long.csv`.
   Why: shows the vt crown jewel landing in 4 days (7k → 135k), then upper tiers, then slow growth from resyncs;
   Swift flat at 35,733 makes the "reused Swift app" point visually.
4. **Upstream absorbed vs Rustty growth** — paired bars per resync (upstream non-merge commits, +/− lines) with a
   line of Ghostty .zig LOC and Rustty .rs LOC at each pin. Data: `timeline-upstream_pins.csv` +
   `timeline-loc_by_day_wide.csv`. Why: answers "how hard is it to keep a port in sync" — 1,157 upstream commits
   absorbed in 7 advances, with shrinking turnaround.
5. **Test count step chart** (total, vt, vt-c) with Ghostty's 3,234-test baseline contract as a reference line.
   Data: `timeline-tests_by_week.csv`. Why: the port exceeded the reference test count within a week.
6. **Crate birth swimlane / dot plot** — x = date, y = crate, dot at creation, bar to first ≥1k LOC.
   Data: `timeline-crate_birth.csv`. Why: shows leaves-first planning (29 crates scaffolded day 1) vs crates
   discovered later (vendored Zig deps ported as crates: z2d, xev, vaxis, zf, zigimg, wuffs).
7. **Weekday × hour heatmap** — data: `timeline-commits_weekday_hour.csv`. Why: shows agent-driven workflows
   committing around the clock (merge waves) versus human-paced commits; pairs with usage-stats data.
8. **Churn diverging bars (adds up / deletes down) per day, by file class** — data: `timeline-churn_by_day.csv`.
   Why: exposes squashed-wave eras (Phase 4/5) invisible in commit counts, and the 10-01 mechanical split
   (+212k/−210k) as a refactor signature.
9. **Milestone timeline (firsts)** — annotated horizontal timeline. Data: `timeline-firsts.csv`.

---

## 9. Method (commands)

- Commits by day/week: `git log --all --format='%ad|%an|%P' --date=format:%Y-%m-%d`, merges = >1 parent.
- LOC: for each day D, `rev=$(git rev-list -1 --before=D+1 HEAD)`; `git grep -c '' $rev -- '*.rs' '*.swift'`, bucketed
  by `crates/<name>`, `crates-sys`, `xtask`, `.port`, `macos` (script: scratchpad `loc.py`, output in data/).
- Churn: `git log --all --no-merges --format=@%ad --date=short --numstat`.
- Tests: `git grep -h -E '^[[:space:]]*#\[(tokio::)?test\]' <rev> -- '*.rs' | wc -l` at weekly revs.
- Crate birth: `git log --all --reverse --diff-filter=A --format='%h|%ad|%s' -- crates/<c> | head -1`.
- Ghostty LOC: `git -C ../ghostty grep -c '' <pin> -- '*.zig'` and `'macos/*.swift'`.
- Era classifier: Python regex over subjects + date windows (rules listed in this dossier §2); output
  `timeline-commit_era_assignment.csv`.

## 10. Gaps / caveats

- Author dates are used; some branches were rebased so author date ≠ committer date in places (LOC series uses
  committer-date `--before`). Differences are within a day.
- Deleted branches: the repo only keeps reachable refs (`main`, `phase-8-upstream-resync`, two `phase-8/w9t2*`,
  dependabot remotes). Branches deleted without merge are not recoverable; 26 commits are on refs other than HEAD.
- Phases 4/5 and codex-vet were squashed, so commit-level era detail inside them is lost; the phase READMEs and
  `assessments/` carry the wave structure.
- Codex vetting work happened outside this repo (`d34cc1549` "untrack codex stuff" removed 23,637 lines of Codex
  artifacts); only its result commit is in history.
- LOC counts include comments, tests and blank lines; the Rust total includes `.port/` harness code (18,992 lines at
  snapshot) and xtask (10,207).
- `unimplemented!` and `unsafe {` counts are grep-level estimates.

## Fact-check

Independent re-derivation, 2026-10-08. Methods differed from the dossier where possible: Python date bucketing over `git log --all`, `git grep -h -c` summed with awk at `rev-list -1 --first-parent --before=<date> HEAD`, a direct `git -C ../ghostty rev-list` over the full pin range, and `git log --diff-filter=A` on each crate's `Cargo.toml`.

| # | Fact | Status | Notes / evidence |
|---|---|---|---|
| 1 | Span and size of history | confirmed | 2,994 total and 2,728 non-merge; 110 calendar days with commits on 58 of them; busiest day 2026-09-23 with 249 (next are 07-29 with 179 and 07-30 with 174). |
| 2 | Authors | confirmed | Gael Hatchue 2,983, dependabot[bot] 10, "rustty port" 1 (15c1a9bd7, the root commit). |
| 3 | Rust LOC growth | confirmed | 33,040 / 228,026 / 422,813 / 593,590 / 703,502 / 857,867 at the daily first-parent revs. `crates/` alone: 818,802. |
| 4 | rustty-vt core in 4 days | confirmed | vt LOC 6,996 -> 135,431 and vt `#[test]` 113 -> 2,032 (06-21 -> 06-25). Snapshot: 263,312 LOC and 3,474 tests. Caveat: by 07-12 vt was only 138,468, so "largely ported" is fair, but vt grew another ~125k later through resyncs and audits. |
| 5 | Total #[test] over time | confirmed | 314 / 3,005 / 4,761 / 6,846 / 7,284. The 3,234 contract appears in .port/phase-0/REPORT.md (its 3,189 figure was a recorded blocker). Side note: 06-25 already had 2,894. |
| 6 | Swift app reused | confirmed | Rustty `*.swift` on 07-12 is 35,733. Ghostty `macos/*.swift` at 4749c4e93 is also 35,733. Snapshot is 38,180. 1395249e5 (07-12) says "import the reused ghostty Swift app (... byte-identical)". |
| 7 | Upstream pin advances | confirmed | ORACLE_SYNC.md history has 7 rows, and the chain matches. |
| 8 | Upstream absorbed | confirmed with nuance | Per-phase files for Phases 3-8 sum to 1,714 commits with merges and 1,155 non-merge. Adding the 2 gap-close drift commits gives 1,716 and 1,157. The diff stats (+185,221/-33,790 over 1,436 file-touches) are the Phase 3-8 sums only and exclude the drift commits. Phase 4: 543 commits with merges and +72,554 lines. Cross-check: `git -C ghostty rev-list --count 4749c4e93..a60e9e2a5` = 1,718 (1,157 non-merge). The single end-to-end diff is +178,226/-26,599 across 901 files; this is not a sum of touches, so it is lower. |
| 9 | Ghostty Zig growth | confirmed | 273,827 -> 366,687 `*.zig` lines at 4749c4e93 / a60e9e2a5, which is +33.9%. |
| 10 | Time to milestones | confirmed with nuance | All commits, dates and day numbers check out (day 1 = 06-21). 079cc21e7's subject is "parity-closure: keep the S7 prototype harness in-tree..." and does not say "0 open". Same-day ae6c429a5 says "closure record lists N11 as still open", and ab1987e1a says "N11 scoped, open". So "0 open" overstates it: one item (N11) was carried forward. Suggested wording: "parity-closure merged (day 100, 079cc21e7)". |
| 11 | Bun-post strategy realignment | confirmed | 2c03357ce, ccd14200e and 8f6d781dc are all on 07-10, with matching subjects. 3277f2f78 (07-11): "first full Miri sweep (all 2037 tests) ... the real search/screen.rs SB UB finding". All three cited files exist. |
| 12 | Project rhythm | confirmed | 52 zero-commit days out of 110, including all of 08-01..08-09. The "5 weeks / 10 weeks" split is approximate (06-21..07-30 is about 5.7 weeks). |
| 13 | Linear history until mid-July | confirmed with nuance | The first merge is 2026-07-16 by both author and committer date, so the more precise statement is "0 merges before 07-16". Merge peaks: week of 07-27 = 91 and week of 09-28 = 83. |
| 14 | Crate creation | corrected (sys count) | 44 `crates/*` total, and 29 in 15c1a9bd7. The later-birth dates all match. However, there are **16** -sys crates (`crates-sys/`), not 17. The dossier's crate_birth.csv counted `crates-sys/README.md` as a crate. Of the 16, 14 date from day 1 (13 in 15c1a9bd7 plus gettext-sys in f7eba28e9). The later two are libxml2-sys (07-17) and breakpad-sys (08-11). |
| 15 | Squashed eras | confirmed | 23 commits (all non-merge) dated 08-17..08-25. 8120dd10d: 532 files, +87,270/-5,513 (dated 08-11). 10-01: +213,681/-210,756 across all non-merge commits; the P7-W13 commits alone are +212,048/-210,628. |
| 16 | Advance-first doctrine | confirmed | The ORACLE_SYNC.md Phase-4 note says the REDs are "the honest progress meter for Phase 4". Lags from upstream target date to advance: P3 2 days (07-29 -> 07-31), P4/P5/P7/P8 0 days, Phase 6 1 day (08-31 -> 09-01), and drift-absorb 1 day. The claim correctly excludes Phase 6. |

**Corrections to apply:** fact 14 should read 16 -sys crates, not 17. **Wording tweaks:** fact 10's "0 open" (N11 stayed open), fact 13's "before 07-16", and fact 8 should note that the +2 drift-absorb commits are counted while their lines are not.
