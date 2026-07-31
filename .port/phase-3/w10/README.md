# `P3-W10` — advancing the oracle sync point `4749c4e93 -> 6ad1fe7d8`

The oracle working tree at `/Users/gael/projects/ghostty` was advanced (detached checkout) on
2026-07-31 under a lane freeze. This directory is the recorded before/after evidence.

| file | what |
|---|---|
| `g3b_before.txt` | `check_comment_coverage.py` at the OLD pin (`4749c4e93`), re-verified byte-identical immediately before the checkout |
| `g3b_after.txt` | the same gate at `6ad1fe7d8`, with the per-crate MISSING print cap temporarily lifted so all 334 rows are listed (the gate file was restored in place afterwards) |
| `g3b_delta_table.txt` | per-crate `ref` and `missing` before -> after |
| `g3b_after_attribution.txt` | every one of the 334 MISSING sentences attributed to the oracle file that contains it |
| `attribute_g3b.py` | the attribution script (re-runnable) |
| `testcounts_before.txt` | `check_test_counts.py` (G1b) at the old pin |

## Headline finding: an advance is BIDIRECTIONAL

The plan modelled the advance as one-directional — "the reference side of every beyond-pin G3b
artifact disappears by construction, the config row goes to 0". **That model is wrong, and the
measurement shows it.** G3b reads a *directory* (`/Users/gael/projects/ghostty/src`) but resolves
its file list from the FIXED phase-0 `crate_map`. (G3, `check_comments_fidelity.py`, does **not**
read the oracle tree at all — it compares against the phase-0 comment census; see "Which gates the
advance actually moved" below.) Advancing therefore re-reads the *same*
files at a newer SHA, which simultaneously:

* **removes** reference sentences upstream deleted in `4749c4e93..6ad1fe7d8`, and
* **adds** reference sentences upstream *wrote* in that range — every one of which is MISSING unless
  a workstream ported it.

Net: G3b's total went **218 -> 334**. G3b is a `PROGRESS` gate in `.port/tools/ci.sh` and was
already RED at 218 before the advance, so this is a change in a tracked number, not a gate flip.

## Per-crate accounting (missing, before -> after)

| crate | ref b->a | missing b->a | direction |
|---|---|---|---|
| config | 2777 -> 2814 | **30 -> 4** | −26 |
| os | 688 -> 652 | **35 -> 12** | −23 |
| termio | 995 -> 1148 | **23 -> 14** | −9 |
| bench | 112 -> 133 | **8 -> 5** | −3 |
| vt | 9609 -> 11515 | **116 -> 218** | +102 |
| global | 58 -> 104 | **0 -> 31** | +31 |
| unicode | 64 -> 124 | **0 -> 20** | +20 |
| cli | 591 -> 599 | **2 -> 11** | +9 |
| vt-c | 334 -> 407 | **4 -> 10** | +6 |
| lib | 89 -> 90 | **0 -> 5** | +5 |
| simd | 79 -> 102 | **0 -> 4** | +4 |
| args, build-config, core, crash, datastruct, extra, inspector, render-types, synthetic, terminfo, util | — | 0 -> 0 | unchanged |
| **TOTAL** | | **218 -> 334** | |

The `missing` counts are exact. The `resolved`/`new` split in `g3b_delta_table.txt` is a **lower
bound** on both sides: `g3b_before.txt` was captured with the gate's default per-crate print cap of
25 rows, so for `config`, `os`, `termio` and `vt` the before-side sentence list is truncated. The
after-side list is complete (334 of 334) because the cap was lifted for `g3b_after.txt`. Every
crate moved in *both* directions — e.g. `config` is ~26 resolved against 4 new, not a clean −26.

### Negative control (plan `T3 Step 5`) — PASSED

The failure mode being guarded against is "the gate goes quiet" (the `_vt_header_baseline.txt`
pattern: an artifact that reports nothing reads as success). It did not happen:

* no crate's `distinctive ref comments` went to 0; no crate dropped out of the report;
* the whole-corpus reference count went **UP** (e.g. `vt` 9609 -> 11515), i.e. the gate is
  demonstrably reading a *different, newer* tree;
* the sentences that vanished from `config` are exactly the predicted class — the KDE/Hyprland
  `background-blur` doc block, the `scrollback-limit` doc rewrite (`65c48213b`), and the
  `Color.fromHex` / `Palette.parseCLI` lines (`2970e9a2a`) — verbatim text the oracle itself deleted.

## The `config` residue: 4 survivors, NOT zero

The plan required `G3b [config]: missing=0` and said any survivor is a real drop. There are **4**,
and they are one contiguous comment block in one function:

```
todo: this is not available on wasi without libc due to the memory
allocation requirement. this restricts this function to said platforms.
to be fair, the legacy getenv path had more restrictive issues (no wasi
period, or windows for that matter).
```

`/Users/gael/projects/ghostty/src/config/Config.zig:5176-5182`, inside `probableCliEnvironment`.
Introduced by **`e8525c0fd` "Update to Zig 0.16.0"** (confirmed:
`git log -S containsUnemptyConstant -- src/config/Config.zig` -> `e8525c0fd`), which replaced
`std.posix.getenv("TERM_PROGRAM")` with `global.environ().containsUnemptyConstant("TERM_PROGRAM")`.
rustty's `crates/rustty-config/src/config_struct.rs:5416` `probable_cli_environment` is still at the
pin shape (`std::env::var("TERM_PROGRAM")`).

**Verdict: a real, un-ported upstream change — but a documented NON-PORT, not an unaccounted drop.**
`e8525c0fd` is a Zig-toolchain upgrade and is triaged across
`.port/phase-3/NON_PORTS.md` (11 separate sections) and `.port/phase-3/SAMPLING_CLOSURE.md` §1. The
same commit's `global.zig` environ/args rework is what produces the `global` crate's 31 new misses.
The comment text is Zig-specific (`std.process.environ`, WASI-without-libc, "the legacy getenv
path"), so porting it verbatim into Rust would be R1-faithful only in the sense of copying an
untrue statement.

**Left OPEN, owner: next resync.** It is not closed by this workstream and must not be recorded as
closed.

## Where the other 116 new misses come from (file attribution)

Full per-sentence attribution is in `g3b_after_attribution.txt`. Top buckets of the *after* state:

* `vt` 218 — `terminal/PageList.zig` 104, `terminal/render.zig` 28, `terminal/page.zig` 27,
  `terminal/Terminal.zig` 14, `kitty/graphics_storage.zig` 12, `Screen.zig` 8, `hash_map.zig` 6,
  `SelectionGesture.zig` 4, `search/viewport.zig` 4, rest ≤3. This is the BL-1
  compression/generation subsystem's commentary; WS7 ported the code, and the comment coverage
  lags it.
* `global` 31 — 27 in `global.zig` (`e8525c0fd` environ/args/state rework), 4 in the tool-binary
  CLI-action-namespace block.
* `unicode` 20 — `unicode/grapheme.zig` 12, `unicode/props.zig` 7, `unicode/main.zig` 1 (the
  `uucode` revision bump, `.port/phase-3/NON_PORTS.md` §G1, already classified DEFERRED).
* `cli` 11 — `cli/list_themes.zig` 7, `cli/boo.zig` 2, `edit_config.zig` 1, `new_window.zig` 1.
* `os` 12 — `os/windows.zig` 8 (`e8525c0fd`, `NON_PORTS.md` §"os/windows.zig"), `os/open.zig` 3,
  `os/mach.zig` 1.
* `termio` 14 — `Command.zig` 12, `termio/Exec.zig` 2.
* `vt-c` 10 — `terminal/c/terminal.zig` 9, `terminal/c/color.zig` 1.
* `lib` 5 — all `lib/enum.zig`. `simd` 4 — all `simd/vt.zig`. `bench` 5 —
  `benchmark/TerminalStream.zig` 3, `main_bench.zig` 2.

None of these is closed by `P3-W10`. They are the measured, attributed comment-coverage debt of the
resync and the seed of the next pass.

## Which gates the advance actually moved — and which it did NOT

This is the second thing the plan did not model, and it matters for reading any post-advance CI run.
The gates split into two families:

* **live-tree readers** — they open files under `/Users/gael/projects/ghostty/src` at run time, so
  they moved with the advance: **G3b**, **G1b** (`check_test_counts.py` greps `test{}` blocks),
  **A5** (`check_rustty_vt_headers.sh`, incl. the oracle `lib_vt.zig` export arm), **G-LZ4**
  (differential vs `origin/main:src/terminal/compress/lz4.zig`), `check-vt-struct-layouts.mjs`.
* **frozen-inventory readers** — **G1** (`check_tests_ported.py`) and **G2**
  (`check_assertions.py`) never touch the oracle tree at all (`grep -n ghostty` in either file
  returns nothing). They read `.port/phase-0/inventory/{tests-summary.json,tests.jsonl}`, a census
  taken at `4749c4e93` and last written in `15c1a9b`. G3 (`check_comments_fidelity.py`) likewise
  compares against the phase-0 comment census.

**Consequence: advancing the working tree does NOT advance G1/G2/G3's contract.** Their verdicts
are bit-for-bit identical before and after the checkout. Regenerating the phase-0 census at
`6ad1fe7d8` is a real follow-up and is **not** done here — it would move the global assertion floor
(11,975) and the test contract (3,234), which is a decision, not a chore. **Owner: next resync.**

## G1 — RED, 12 named misses (PRE-EXISTING, not caused by the advance)

`PROGRESS` in ci.sh. Because G1 reads the frozen inventory, these 12 are **pin-era** contract tests
that were never ported, not newly-surfaced head tests:

```
config::fromHex
os::appendEnv empty            os::appendEnv existing
os::prependEnv empty           os::prependEnv existing        os::args
vt::HashMap ensureUnusedCapacity with tombstones
vt::PageList compact insufficient savings returns null
vt::PageList compact std_size page returns null
vt::Terminal: scrollUp with max_scrollback zero
vt::Terminal: scrollUp with max_scrollback zero and left/right margin
vt::Terminal: scrollUp with max_scrollback zero and top margin
```

The `os::*Env` / `os::args` five are the environ/args helpers; the remaining seven are genuine
pin-era test gaps. Left OPEN, owner: next resync.

## G2 — RED on `os::desktop environment` (PRE-EXISTING, and a false alarm)

`WEAKENED: os::desktop environment ref=10 rust=9`. This is the frozen-inventory effect in its purest
form. `P3-W7 T3` correctly ported upstream `f8e13f31e` ("Fix desktop detection tests when running
from Gnome"), which **restructured** the test into per-case blank `Environ.Map`s and in doing so
**reduced** it from 10 assertions to 8 upstream. `crates/rustty-os/src/desktop.rs:232` matches that
new shape with 9. G2 still compares against the pin-era count of 10, so a faithful port reads as a
weakening. **Not a regression, and not fixable without regenerating the phase-0 census** — see the
follow-up above. Do NOT "fix" this by padding assertions into `desktop.rs`.

## Two HARD ci.sh failures, neither caused by the advance

* **G6b** (`check_pagelist_compress_leaks.sh`) — **FIXED HERE.** The out-of-tree harness
  `.port/phase-2/mirihar-pagelist-compress` still called `PageList::init(20, 6, Some(1 << 30))`;
  `P3-W3`'s runtime scrollback limits replaced that signature with a single `Options`. The harness
  therefore failed to **compile**, and the gate printed `RED: PageList leaked compressed-page
  encodings` — *a build break wearing a leak's clothes*, which is exactly the class of misleading
  gate output this project keeps finding. Call site migrated to
  `Options { max_size: Some(1 << 30), ..Options::new(20, 6) }`; gate now GREEN and its FAIL arm
  re-proven reachable (`--self-test`: "PageList leaked 27374 global-allocator bytes", exit 101).
  **Lesson for the gate itself (not fixed here): `check_pagelist_compress_leaks.sh` should
  distinguish a non-zero `cargo` exit from a non-zero leak delta.**
* **G4-uni** (`.port/phase-0/harness/unicode/run_unicode_diff.sh`) — **still RED, environmental.**
  `zig build uucode-oracle` fails at
  `zig-pkg/uucode-0.2.0-…/build.zig:211: member function expected 3 argument(s), found 2`
  (`b.modules.put`). The harness pins uucode 0.2.0 in its **own** `build.zig.zon` — it does not read
  the oracle tree — so this is not advance-related. The host `zig` is **0.16.0** (installed
  2026-07-29, mid-Phase-3; the plan's stated local toolchain was 0.15.2) and uucode 0.2.0 predates
  the 0.16 stdlib change. The fix is the deferred **uucode revision bump** already owned by
  `.port/phase-3/NON_PORTS.md` §G1. **Not weakened, not skipped, left RED.**

## Upstream moved during Phase 3

`git fetch origin main` at advance time put `origin/main` at **`4d605bf0d`**, ten commits past the
new pin (`src/terminal/PageList.zig` +835, `src/lib/enum.zig`, `src/terminal/ref_counted_set.zig`,
`.github/VOUCHED.td`). Per the plan this is recorded as the seed of the next resync and is
deliberately not chased here.
