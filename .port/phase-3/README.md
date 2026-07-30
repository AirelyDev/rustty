# Phase 3 — upstream resync (pin → oracle head)

Phase 3 exists to close the gap between the port's **pinned baseline** and upstream
ghostty's `main`, and to leave `.port/ORACLE_SYNC.md` able to state a single honest
sync point instead of "baseline + a list of beyond-pin series".

## Range

```
from  4749c4e93731067049bfbf2e4572061cef2bdd17   2026-06-20  i18n(kk): translate missing "Close Split" string (#13052)
to    6ad1fe7d8cbda36c77b337a96c9bea8a77883699   2026-07-29  url: exclude trailing spaces from path matches (#13505)
```

365 commits (250 non-merge). **507 files, +37,683 / −11,323.**

## Artifacts (generated, do not hand-edit)

| File | What |
|---|---|
| `UPSTREAM_DIFF.patch` | the complete diff, 91,948 lines / 3.5 MB |
| `UPSTREAM_DIFF_STAT.txt` | per-file `--stat` (508 lines) |
| `UPSTREAM_COMMITS.txt` | 250 non-merge commits, oldest-last |
| `UPSTREAM_COMMITS_WITH_MERGES.txt` | all 365, for PR-boundary context |
| `assessments/*.md` | per-area re-vetting verdicts (see below) |

Regenerate with:

```sh
cd ../ghostty && git fetch origin main
git diff  4749c4e93..origin/main            > ../rustty/.port/phase-3/UPSTREAM_DIFF.patch
git diff  --stat 4749c4e93..origin/main     > ../rustty/.port/phase-3/UPSTREAM_DIFF_STAT.txt
git log   --oneline --no-merges 4749c4e93..origin/main > ../rustty/.port/phase-3/UPSTREAM_COMMITS.txt
git log   --oneline            4749c4e93..origin/main > ../rustty/.port/phase-3/UPSTREAM_COMMITS_WITH_MERGES.txt
```

**The oracle working tree stays at the pin.** Read beyond-pin state with
`git -C ../ghostty show origin/main:<path>`; never `git checkout` there — gates and
concurrent lanes depend on the tree staying put (see `.port/ORACLE_SYNC.md`).

## Binding rule for Phase 3: re-vet, do not assume

Several series in this range were already ported opportunistically before Phase 3
existed (BL-1 scrollback compression incl. LZ4, BL-2 `madvise` reclamation, BL-3
runtime-safety fixes, BL-5 lib-vt C-API, BL-6 kitty/gfx generation stamps,
BL-7 OSC 72). **Phase 3 treats every one of those as POSSIBLY PARTIAL and re-vets it
against this diff.** The reasons this is not paranoia:

- They were ported from *selected commits*, not from a diff of the full range, so a
  later upstream commit touching the same file may have been silently missed.
- `MAIN_PARITY.md` was triaged at `53bd14fec` (2026-07-10) and scoped to `src/` +
  `include/` only — it cannot speak for this range or for `pkg/`, `po/`, `macos/`.
- This program has repeatedly found ledger rows that were stale in **both**
  directions: claiming work open that had shipped (WS12 uucode, WS16 x11 arm,
  `boo`), and claiming work done that was vacuous (an `OUT_DIR/locale` nothing
  reads; a guard test that passes on unmodified code).

A verdict of "already ported" is only acceptable with **evidence**: the rustty
symbol, its file:line, and a test or gate that would fail if it regressed.

### Named re-vet assertions inherited from Phase 2 (WS19 T2, 2026-07-29)

These are **not** deferrals: they are items with nothing to port in the pinned tree,
handed to Phase 3 with the exact assertion to make. Each was orphaned in
`.port/phase-2/DEFERRALS.md` and re-owned by the WS19 Phase 2 closure audit (gate
`G-P2`, `.port/tools/gates/check_phase2_closure.py`, enforces that no ledger row is
open without an owner in an open workstream).

| # | Assertion Phase 3 must make | Ledger row |
|---|---|---|
| P3-RV1 | **If** the resync introduces a `PageList::Limits` struct, **then** `Limits::enforce` must call `mark_activity` on entry, taking rustty's `mark_activity` site count from **10 to 11** (`crates/rustty-vt/src/pagelist/compress.rs`, PORT NOTE at `mark_activity`). If the resync does not introduce it, the row closes with no work. | `DEFERRALS.md` "Deferred to WS7 D6" item 2 (audit row **O5**) |
| P3-RV2 | Advancing the oracle pin past `4749c4e93` is what resolves G3b's beyond-pin comment-coverage deltas **wholesale** (a sentence upstream DELETED beyond the pin still exists at the pin and is reported `missing`). After the resync, re-run every G3b lane and confirm the beyond-pin `missing` counts drop by construction. **A per-file `GHOSTTY_SRC` override is forbidden** — it is the spoof hole `.port/tools/ci.sh:114-129` killed for G1b. | audit row **N6**; `.port/tools/gates/README.md` G3b subsection |
| P3-RV3 | `Screen::cursor_scroll_region_up` is defined by beyond-pin commit `77190bd02` (BL-4). It is owned by **WS10**, sequenced to run **after** the pin advances. When the resync lands `77190bd02`, its `invalidateNodeLayout` call site and the `screen_cursor_scroll_region_up_renews_page_generation` test must land alongside it. Phase 3 must surface `77190bd02` to WS10 rather than absorbing it silently. | audit row **O3** |
| P3-RV5 | **BL-5 is PARTIAL — proven, not suspected.** The oracle's `src/lib_vt.zig` at `origin/main` exports 185 `ghostty_*` symbols; rustty's published artifacts (native dylib ∪ wasm module) carry 183 of the R2-renamed counterparts. The two absent are **`rustty_render_state_begin_update`** and **`rustty_render_state_end_update`**: upstream split `RenderState.update` into `beginUpdate` + `endUpdate` (`origin/main:src/terminal/render.zig:356`/`:399`), adding a `pending_styles` field and deferring per-cell style denormalization so a locking caller can shorten its lock hold. rustty's `RenderState` has **no `pending_styles`** (repo-wide grep: 0 hits), so this is a FEATURE port, not a signature split. Phase 3 must port it as part of the BL-5 re-vet and then DELETE the two entries from the documented-residual list in `.port/tools/gates/check_rustty_vt_headers.sh` arm (d) — the gate's stale-allowlist guard FAILS if they are ported but left listed, so the list cannot rot. | found by A5 arm (d) on the day it landed, WS19 T3 |
| P3-RV4 | The `rustty-termio` ReadThread Linux `POLLHUP` quirk is preserved **verbatim** as a Justified R1 Divergence; its only correct trigger is an upstream fix to `threadMainPosix`. If the resync range contains such a fix, port it and retire the divergence entry. | `DEFERRALS.md` `# Justified R1 Divergences` (audit §E.2) |

## Area breakdown (drives the assessment fan-out)

| Area | +ins | −del | files |
|---|---|---|---|
| `src/terminal` | 22,466 | 5,137 | 77 |
| `po` | 1,454 | 1,062 | 32 |
| `src/benchmark` | 1,360 | 79 | 16 |
| `include` | 1,275 | 61 | 11 |
| `src/apprt` | 1,117 | 376 | 30 |
| `src/build` | 905 | 319 | 29 |
| `pkg` | 859 | 399 | 46 |
| `src/termio` | 795 | 387 | 7 |
| `src/os` | 636 | 927 | 22 |
| `src/datastruct` | 587 | 38 | 4 |
| `src/font` | 568 | 355 | 60 |
| `src/config` | 540 | 286 | 13 |
| `.github` | 480 | 224 | 13 |
| `src/renderer` | 433 | 166 | 13 |
| `src/cli` | 418 | 294 | 24 |
| `src/lib` | 389 | 64 | 9 |
| `src/global.zig` | 357 | 151 | 1 |
| `macos` | 356 | 50 | 11 |
| `example` | 350 | 137 | 29 |
| `src/quirks_memset.zig` | 338 | 0 | 1 (NEW) |
| `src/unicode` | 271 | 21 | 5 |
| `src/inspector` | 254 | 71 | 7 |
| `src/simd` | 227 | 40 | 3 |
| `src/Command.zig` | 215 | 92 | 1 |
| `src/Surface.zig` | 174 | 144 | 1 |
| `src/synthetic` | 174 | 16 | 6 |

Largest single files: `PageList.zig` (+5,088/−1,430), `Terminal.zig` (+3,286/−833),
`Screen.zig` (+2,060/−566), `c/terminal.zig` (+1,234/−458),
`stream_terminal.zig` (+1,103/−130), `render.zig` (+913/−168).

### Files upstream ADDED (new subsystems — highest risk of being wholly absent)

`src/terminal/compress/{lz4,lz4_differential,Page}.zig`, `src/terminal/mem.zig`,
`src/datastruct/segmented_list.zig` (533), `src/quirks_memset.zig` (338),
`src/terminal/osc/parsers/kitty_dnd_protocol.zig` (363),
`src/benchmark/{PageCompression,ScrollbackCompression,HyperlinkMap,ApcParser}.zig`,
`src/apprt/gtk/winproto/wayland/{Hotkeys.zig,protocols/vicinae-hotkey-v1.xml}`,
`src/config/limit.zig` (115), `src/terminal/c/{unicode,color_scheme}.zig`,
`src/lib/compat/{fd,thread,reader,file}.zig`, `src/synthetic/{Kitty,cli/Kitty}.zig`,
`include/ghostty/vt/{unicode,color_scheme}.h`, `pkg/apple-sdk/native_link.zig`,
`src/build/libsystem_override.sh`, `macos/Tests/QuickTerminalScreenStateCacheTests.swift`.

### Files upstream DELETED (port must delete too, or justify keeping)

`pkg/harfbuzz/c.zig`, `pkg/wuffs/src/c.zig`, `src/os/args.zig`, `src/os/env.zig`.
