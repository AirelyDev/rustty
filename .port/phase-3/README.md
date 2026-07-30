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
