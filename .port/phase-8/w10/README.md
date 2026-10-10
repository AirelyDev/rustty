# Phase-8 second pin advance — gate evidence (2026-10-09)

Oracle advanced `a60e9e2a5` → `246f70287` (= `origin/main`) by `P8-W10 T0`, on the same branch
(`phase-8-upstream-resync`). Range: 18 commits, 12 non-merge, 10 files, +401 / −72
(`UPSTREAM_COMMITS_WITH_MERGES.txt`, `UPSTREAM_COMMITS.txt` (non-merge), `UPSTREAM_DIFF.patch`,
`UPSTREAM_DIFF_STAT.txt`). Four of the 12 are VOUCHED-list updates (N/A: `7551c5bad`,
`0f171f650`, `bad5854f3`, `9d479dcb1`).

Preconditions: worktree `p8-w10t0` (branch `phase-8/w10t0`) clean at `ea0cad0ed`; the other
worktrees were the main checkout and the three W10 task worktrees (T1–T3), none of which moves the
oracle. Oracle clean at `a60e9e2a57f73e1eef2bd1cf2995a467f69e7fb0` with `origin/main` already
fetched; `zig version` 0.16.0; 72 GiB free.

After the move:

- the oracle `zig-out` was rebuilt with `zig build -Demit-lib-vt=true`; the installed headers are
  byte-equal to the oracle's `include/ghostty/`;
- `.port/phase-0/harness/build_ref_driver.sh` relinked this worktree's `bin/vt_driver_ref`;
- `.port/phase-0/build-examples.sh` rebuilt all 23 c-vt examples at the pin (N525 method) and
  `.port/phase-0/run-cvt-corpora.sh` re-captured the goldens: **no golden changed** (all 23 equal,
  `c-vt-paste` under the N395 mask; the determinism re-run is OK for all 23). The `terminal.h`
  change in the range is documentation only (DECSTR wording on the full-reset callback, mode
  reset-default/current-value options and the XTCHECKSUM default option), so no example's output
  moves. No golden finding;
- `_oracle_root.sh --self-test` GREEN (`g-oracle-sh-self-after-at-246f70287.txt`);
- rvm: `/home/gael/projects/ghostty` fetched (`a60e9e2a5..246f70287 main -> origin/main`) and
  detached at `246f702876b924a1cb7cade1e99274d1470302fc`; HEAD = `origin/main`, tree clean (N486).

## Before / after

Script names: G-TESTS `audit_test_coverage.py --limit 500`, G-FILES `audit_file_coverage.py
--limit 500`, G-SYMS `audit_symbol_coverage.py --limit 500`, A5 `sh check_rustty_vt_headers.sh`.

| gate | before at `a60e9e2a5` | after at `246f70287` | new red owned by |
|---|---|---|---|
| G1b | GREEN (see F1) | GREEN. The 7 new `Terminal.zig` tests and the new `stream.zig` test fit inside the existing Rust slack of their modules, so G1b does not see them; G-TESTS does | — |
| G3b (default) | GREEN | RED, 19 missing, all in `vt`: `Terminal.zig` 10 (`softReset` doc block, the DECSTR palette/saved-cursor/allocation comments), `stream.zig` 1 (`DECRQM - Request Mode, DECSTR - Soft Terminal Reset`), `formatter.zig` 8 (origin-mode cursor comments and their test comments). No NOT-CARRIED-INVALID | T1 (11), T2 (8) |
| G3b `--strict` | GREEN | RED, the same 19 (no strict-only rows) | T1 (11), T2 (8) |
| G-TESTS | GREEN, 4277 blocks, 0 unexplained | RED, 4287 blocks, 8 unexplained: `Terminal.zig` `softReset modes`, `softReset margins`, `softReset keeps the cursor and screen`, `softReset saved cursor`, `softReset pen, protection, and charsets`, `softReset status display and checksum`, `softReset palette`; `stream.zig` `DECSTR dispatch` | T1 (8) |
| G-FILES | GREEN, 562 | GREEN, 562 (no new file) | — |
| G-SYMS | GREEN, 2908 | RED, 2911, 1 unexplained: `Terminal.softReset` (see F3 for `ModeState.getDefault`) | T1 |
| G-CENSUS | GREEN (4487 / 18258 / 360) | RED at arrival (`g-census-arrival-at-246f70287.txt`: live 4497 / 18291 / 360); GREEN after `CENSUS_DELTA.md` moved (+10 blocks: `Terminal.zig` 7, `formatter.zig` 2, `stream.zig` 1) | — (T0, done) |
| G-LOGFILE | GREEN (1415 sites) | RED, 2 arms (1416 sites): 2 `LOG-ABSENT-SITES.tsv` rows off-line (`:68 stream.zig:1337` now at 1339, `:101 stream_handler.zig:1581` now at 1582: line drift from the `soft_reset` action and dispatch arm); 2 files short by 1 (`stream.zig` 134/133, which includes the new `invalid DECSTR command` warn; `stream_handler.zig` 42/41, from its off-line ABSENT row) | T1 |
| G-REFGEOM | GREEN | GREEN (36 lines, 3 targets) | — |
| A3 | GREEN | GREEN (`include/ghostty.h` unchanged; `oracle-ghostty-h.sha256` not touched) | — |
| A5 | GREEN | GREEN (the `terminal.h` change is doc comments only; no enum, struct or function moved) | T1 still ports the doc wording into `include/rustty/vt/terminal.h` via `header-gen` |
| G9 | GREEN 777/777 | against the frozen surface: GREEN 777/777 (`g9-before-regen-at-246f70287.txt`). Regenerated: GREEN 777/777 (see F2) | — |

## Regenerated `parity-surface.json` (T2 generator)

`build_parity_surface.py` at `246f70287`; `parse_csi.py --self-test` and `parse_esc.py --self-test`
PASS on the oracle first. `_meta.reference_rev` is now `246f70287…`. No id appears or vanishes in
any kind. The only content change is the CSI `p` arm's comment (`DECRQM - Request Mode` →
`DECRQM - Request Mode, DECSTR - Soft Terminal Reset`); the other 106 changed lines are
`stream.zig` line numbers moved by the new `Action` member.

## Findings

- **F1 — the zig package cache was empty, so G1b was BLOCKED before anything moved.** The
  2026-10-09 disk cleanup (progress.md) deleted `~/.cache/zig`, which holds the zf, vaxis and
  libxev (`86vtc4Ic…`) corpora G1b reads. The first "before" run exited 3 (BLOCKED, R3 guard, not
  a silent green). `bash .port/tools/restore_zig_deps.sh` (the documented route; both pins checked:
  tarball sha256 and zig's content hash) restored all three, then G1b was GREEN; the recorded
  `g1b-before-at-a60e9e2a5.txt` is that run. Only the zig cache was written. Not a gate defect.
- **F2 — G9 cannot see DECSTR.** DECSTR (`CSI ! p`) shares the CSI final `p` with DECRQM, and G9's
  CSI ids are the dispatch arms' final bytes (N523), so the regenerated surface has no new id and
  G9 stays GREEN while `softReset` is unported. G-TESTS and G-SYMS do carry it (T1). Same
  arm-granularity limit as N523 records; not fixed here.
  Follow-up: P8-W10 T4 fixed it (N545): a CSI id is now intermediates plus final. Four plain ids
  were replaced rather than kept, because upstream has no plain-final command for them: `p` → `!p`,
  `$p`, `?$p`; `q` → ` q`, `"q`, `>q`; `y` → `#y`, `*y`; `}` → `$}`. Every old final still has at
  least one id.
- **F3 — G-SYMS counts `ModeState.getDefault` (new in `modes.zig`) as covered by a name match.**
  The only Rust `get_default` is `rustty-font-shaper/src/hb.rs` `Language::get_default`, unrelated
  to modes. Same common-name class as N524. T1 ports `getDefault` regardless (it is on the DECSTR
  path); the audit simply will not go red if it is skipped.
  Follow-up: P8-W10 T4 fixed it (N546, container-aware G-SYMS); P8-W10 FX10 narrowed the
  module-stem fallback to the counterpart files.

No other gate changed. No cvt golden changed.

## Perf oracle (second commit)

Rebuilt with Zig 0.16.0 `-Doptimize=ReleaseFast` from `git archive 246f70287` in the scratchpad
(the oracle's `zig-pkg` copied in), never inside the oracle, as `reports/W9-perfprep-report.md`
did. Stored in `~/rustty-oracle-artifacts/` (older artifacts untouched):

| file | bytes | sha256 |
|---|---|---|
| `ghostty-bench-246f70287` | 4110400 | `37c6317668c020e6be5b62ca2e4d2d45c9afee41ff9e60d2920c4171490fee49` |
| `ghostty-gen-246f70287` | 464728 | `ec5e5ee17da6a8542d6741545f570aec971f5a806b4b854a786e35028fa64755` |
| `libghostty-vt.0.1.0.dylib-246f70287` | 1964784 | `52d5f8bf3da1a29c4be388c13987e117a3dd6329189329c22d9da29b19c79231` |
| `libghostty-vt.a-246f70287` | 11167952 | `1a5094b44904372023899c7ec33332d16e56239cfbd52d32c32520195b27b692` |

`ws10_suite.py`: `ORACLE_SHA256` = the bench sha256 above, `ORACLE_SHA256_PIN_DIGEST` =
`ef1a9f5859c2ce05`; `--self-test` PASS and `oracle_identity_blocker` → None. No measurement was
run. The G8 `BASELINE.json` re-base is the final validation's perf step. The scratch dir was deleted.

## Files

| file | what |
|---|---|
| `*-before-at-a60e9e2a5.txt` | G1b, G3b, G3b `--strict`, G-TESTS, G-FILES, G-SYMS, G-CENSUS, G-LOGFILE, G-REFGEOM, A3, A5, G9 |
| `*-after-at-246f70287.txt` | the same set (G9 against the regenerated surface), plus G-ORACLE-SH-SELF |
| `g-census-arrival-at-246f70287.txt` | G-CENSUS at the new pin before `CENSUS_DELTA.md` moved (RED by design) |
| `g9-before-regen-at-246f70287.txt` | G9 at the new pin against the frozen surface (GREEN 777/777) |
