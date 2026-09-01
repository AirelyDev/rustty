# Phase 6 — upstream resync (pin → oracle head, 2026-09-01)

Phase 6 closes `88f57ee66..20abdb50a` with **no deferrals**, advance-first (the phase-4/5
doctrine; both tree-reading gates were GREEN at `88f57ee66` minutes before this advance —
`w0/*-before-at-88f57ee66.txt`).

## Range

```
from  88f57ee66eeaad4da77b414b245f7b6693348985   2026-08-25  terminal: ignore UTF-8-decoded C1 controls in the ground state (#14023)
to    20abdb50a6216c450d6d4d010c41c7edf5ab15b2   2026-08-31  Update VOUCHED list (#14101)
```

121 commits (74 non-merge). **75 files, +6,261 / −1,269.** Upstream deleted
`.github/workflows/freestanding.yml` inside the range (created by `10b7158e8`, folded into
`test.yml` by `fc7bfa60b`) — END STATE is: no freestanding.yml, freestanding job in test.yml.

## Gate arrival state (advance-first)

- G1b **GREEN** at arrival (`w0/g1b-after-at-20abdb50a.txt`) — but ONLY because the gate's
  pinned file list predates the two NEW oracle files. Hand count of the range's `+test "`
  hunks: `c/search.zig` **17**, `search/terminal.zig` **6**, `kitty/graphics_image.zig` 2,
  `ref_counted_set.zig` 1, `Terminal.zig` 1; `search/Thread.zig` −1 (moved to
  `search/terminal.zig`). The gate's `search` row must gain `search/terminal.zig` and the vt-c
  scope must gain `c/search.zig` as part of this phase (a gate that cannot see a new file is a
  bookkeeping green — memory `derived-gate-green-can-be-bookkeeping`).
- G3b **RED** at arrival (`w0/g3b-after-at-20abdb50a.txt`): **51 missing** — vt 38, config 6,
  vt-c 4, lib 2, core 1. MUST be GREEN at the finale.

## Artifacts (generated, do not hand-edit)

| File | What |
|---|---|
| `UPSTREAM_DIFF.patch` | the complete range diff, 10,967 lines |
| `UPSTREAM_DIFF_STAT.txt` | per-file `--stat` |
| `UPSTREAM_COMMITS.txt` | 74 non-merge commits, oldest-last |
| `UPSTREAM_COMMITS_WITH_MERGES.txt` | all 121, for PR-boundary context |
| `w0/` | before/after gate states for the pin advance |
| `assessments/*.md` | per-workstream assess+port records (verdict per commit, evidence, negative control) |

Regenerate with (oracle working tree is AT the pin = origin/main = 20abdb50a):

```sh
cd ../ghostty
git diff  88f57ee66..20abdb50a            > ../rustty/.port/phase-6/UPSTREAM_DIFF.patch
git diff  --stat 88f57ee66..20abdb50a     > ../rustty/.port/phase-6/UPSTREAM_DIFF_STAT.txt
git log   --oneline --no-merges 88f57ee66..20abdb50a > ../rustty/.port/phase-6/UPSTREAM_COMMITS.txt
git log   --oneline            88f57ee66..20abdb50a > ../rustty/.port/phase-6/UPSTREAM_COMMITS_WITH_MERGES.txt
```

## Binding rules

`.port/phase-4/README.md` §"Binding rules" applies verbatim: port from the RANGE DIFF (end
state), re-vet don't assume, every task names its NEGATIVE CONTROL, R1/R5/R7/R9/DG5, raw-ptr
args for Page/Row/Cell/Screen re-read seams, the oracle tree IS the target.

## Area breakdown → workstreams (plan: `docs/superpowers/plans/2026-09-01-phase6-upstream-resync.md`)

| WS | oracle files | rustty home |
|---|---|---|
| W1 vt-core | ref_counted_set.zig, Screen.zig, Terminal.zig, PageList.zig, snapshot/snapshot.zig, kitty/graphics_image.zig, terminal/build_options.zig, lib/allocator.zig, lib_vt.zig (std_options/freestanding half) | rustty-vt, rustty-lib, rustty-vt-c/lib.rs |
| W2 search-vt | search.zig, search/Thread.zig, search/screen.zig, search/terminal.zig (NEW) | rustty-vt/src/search |
| W3 search-capi (after W2) | c/search.zig (NEW), c/selection.zig, c/types.zig, c/terminal.zig, c/main.zig, lib_vt.zig exports, include/ghostty/vt/{search,selection,types}.h + vt.h, example/c-vt-search | rustty-vt-c, include/rustty/vt, example/c-vt-search, A5 manifests |
| W4 core-tier | renderer/generic.zig, Surface.zig, config/Config.zig, main_ghostty.zig, apprt/gtk/class/{surface,surface_scrolled_window}.zig, os/i18n_locales.zig | rustty-renderer, rustty-core, rustty-config, rustty bin, rustty-apprt, rustty-os |
| W5 macos | 17 Swift files + Rustty.sdef + 3 test files (KeyEventSynthesizeTests NEW, URLTests NEW) + Sparkle 2.9.6 pin | macos/ |
| W6 platform-tail | po/{fr,ru,uk}.po + NEW sr, sr@latin; font/res Noto emoji (binary); iTerm2 themes tarball; .github (milestone bump, freestanding CI lane); CODEOWNERS; build.zig.zon* (N/A except the themes hash) | po/, rustty-font/res, xtask res_themes, .github/workflows, CODEOWNERS |
| W7 finale | G1b (file-list extension) + G3b GREEN, A5/A3/A4/G7/G-VTFEAT/G-THEMES/G-I18N, macOS full + xcodebuild, ledgers, ORACLE_SYNC finale note | — |
