# WS19 — Phase 2 Closeout Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close Phase 2 **completely**, with **no additional deferrals**. Concretely: (1) repair the **vacuous and broken gates** — G1b silently gates `main` when run from a worktree, G1b is BLOCKED because the zig reference corpus was evicted, the debug test lane fail-fasts and masks later crates, and `include/rustty.h` has never compiled as standalone C because gate A5 only checks the curated header; (2) take ownership of every **ORPHANED** ledger item (items filed into WS8, which is closed and merged, plus items that never had an owner at all); (3) reclassify what is genuinely permanent into **Justified R1 Divergences** with a 4-criterion basis and what is genuinely environment-blocked into **verification debt** with a runnable recipe; (4) route what belongs to an open workstream to WS10–WS14/WS17 explicitly, in-ledger; and (5) land a **Phase 2 closure gate** that FAILS if any ledger item is open without an owner in an open workstream — with its FAIL arm proven reachable by a synthetic fixture, because a gate that cannot fail is worthless.

**Architecture:** WS19 has four layers, and they must execute in that order. **Layer 0 (gates, T1–T4)** repairs the verification substrate. It comes first because every "green" claim made by a vacuous gate is unproven, so no later task's evidence can be trusted until the instruments are honest — and specifically because N4 currently *masks* N1 (the gate exits `3 BLOCKED` before it counts anything, so restoring the zig corpus without fixing the hardcoded path would turn a loudly-blocked gate into a silently-wrong one). **Layer 1 (ledger truth, T2)** rewrites `.port/phase-2/DEFERRALS.md`, `VERIFICATION_DEBT.md` and `INTEGRATION_PARITY.md` so that the closure gate in T19 has truthful input; the audit that establishes what is wrong is already committed as the `# PHASE 2 CLOSURE AUDIT (WS19)` section of `DEFERRALS.md` and is this plan's factual basis. **Layer 2 (product fixes, T5–T18)** closes the orphaned code items WS19 owns itself. **Layer 3 (T19–T20)** lands the closure gate and the both-platform finale. Nothing in WS19 ports beyond-pin oracle state: where an item would be cheaper after the pin advances, it is routed to WS10 or Phase 3 **with the reasoning written into the ledger row**, never silently deferred.

**Tech Stack:** Rust 1.93.1 (pinned, `rust-toolchain.toml`); Python 3 for the gate scripts (`.port/tools/gates/*.py`, `_gatelib.py`); Bash for the shell gates; `cargo +nightly miri` for the vt Miri lanes; `clang`/`clang++` from Xcode 26.5 for the header compile checks; macOS M4 Pro host for the Metal/embedded half; Fedora 44 aarch64 VM over `ssh rvm` for the GTK/Wayland/X11 half; oracle git repo at `/Users/gael/projects/ghostty`, working tree pinned at `4749c4e93`.

## Global Constraints (inherited from `2026-07-23-rustty-completion-ROADMAP.md`)

- R1 full fidelity vs the ghostty oracle at `../ghostty` (pinned `4749c4e93`); the oracle is reference-only — no build-time dependency on it (R3).
- R2 product identity: rustty naming with per-token casing preservation (Ghostty→Rustty, ghostty→rustty). **CORRECTION to the ROADMAP's own wording** — see PREMISE CORRECTION §4: the `ghostty_*` C-ABI alias layer was **deleted upstream** at `e4cc852`; rustty exports `rustty_*` only, in every profile. Do not re-add aliases.
- R4/R5: acyclic crate graph; gates are invariant — **never weakened to pass**. WS19 is the workstream that *strengthens* four of them; every strengthening must be additive (a gate that binds more, never less) and must ship a proof that its FAIL arm is reachable.
- DG5: C-ABI panic policy is ABORT — no unwind across the ABI. T3 touches a generated header only; T17 touches `posix_waitpid`, which is reachable from `Command::wait` but not from a C-ABI entry — confirm before changing its signature.
- Toolchain: pinned Rust 1.93.1. LTO stays banned.
- Ledger discipline: every task that closes a row strikes that row **in the same commit**. The `# PHASE 2 CLOSURE AUDIT (WS19)` section of `DEFERRALS.md` is the index; when a row closes, mark it in both places.
- Zig byte-slices → `Vec<u8>`/`&[u8]`, never lossy `String` (memory: zig-byteslice-to-rust-rawbytes). T15 is literally a violation of this rule.
- `&mut Page`/`Row`/`[Cell]` args re-read across `assert_integrity` are Stacked-Borrows UB — use raw-ptr args (memory: rust-mut-args-strongly-protected). T5 and T10 both live in this hazard class.
- A long-lived raw alias into a field interleaved with `&mut` to that field is SB/TB UB, and must be **proven on an isolated non-FFI Miri harness**, not hand-reasoned (memory: shaper-hook-sb-clean-interleave). T10 uses `.port/phase-2/mirihar-xev-mpsc`.
- Commit style: `subsystem: summary (WS19 T<n>)`. Branch `ws19-phase2-closeout`; merge to main only with BOTH platforms green (`cargo build --workspace` on macOS AND `ssh rvm 'cd ~/projects/rustty && cargo build --workspace'`).
- **DO NOT run `.port/tools/gates/no_homebrew_gettext_gate.sh`** — it mutates Homebrew state and has broken two concurrent lanes.
- **NEVER `git checkout` in the oracle repo.** Read beyond-pin state with `git -C /Users/gael/projects/ghostty show origin/main:<path>`. Concurrent lanes depend on that tree staying at the pin.

---

## PREMISE CORRECTION (read before Task 1)

The premise this plan was created from — "`DEFERRALS.md` contains items filed to WS8,
which is closed" — is **correct, and is worse than stated**. The audit committed as the
`# PHASE 2 CLOSURE AUDIT (WS19)` section of `.port/phase-2/DEFERRALS.md` establishes
the following, each verified against code rather than transcribed from the ledger. Read
that section before starting; it is this plan's factual basis and carries the evidence
for every claim below.

**1. Four orphans, not three.** The three named in the task brief (`D:5080`
`style::Color`, `D:5357` the Miri lane, `D:4981`/`:5011` `cursor_scroll_region_up`) are
real. A **fourth** was found: `D:4960`, the inspector `pagelist.rs` CELL-detail restore
path, was filed to **WS7 D6** — and WS7 D6 **merged** at `b8c40f9` without closing it.
`Node::page(cell.node)` still appears at 6 sites, and the guard test at
`pagelist.rs:2414` only asserts the property over the page-tree loop source, exactly the
weaker form the deferral itself predicted. In total **21 of 38 genuinely-open items were
orphaned** (closed-WS owner, or no owner at all).

**2. `D:5011` is DOUBLY mis-filed, and the fix belongs to WS10 — after the pin moves.**
The row says `cursor_scroll_region_up` "belongs to the WS8 upstream-sweep backlog
(BL-4)". WS8 is closed **and** BL-4 is the perf series, which is **WS10**'s scope. The
function is defined by unported upstream commit `77190bd02`, which is **beyond the pin**.
WS19 therefore does NOT port it: it rewrites the row to name WS10 as the owner and to
state that the port is cheaper once Phase 3 advances the pin (porting a beyond-pin
function into a pinned tree means hand-transcribing a commit that Phase 3 will re-vet
anyway). Same reasoning for `D:4973` (`PageList::Limits`), which is routed to Phase 3 as
a re-vet row with the exact assertion Phase 3 must make.

**3. Eleven ledger rows were factually WRONG, in BOTH directions.** Claimed-open but
long since shipped: `renderer_seam.rs`'s "15 `unimplemented!()` Phase-3 placeholders"
(all 15 are `#[cfg(test)]` fakes; the production bodies are live), apprt-c's "exactly 4
`unimplemented!()` leaves" (zero remain), `boo` "STILL DEFERRED" (a 525-line live port —
and the same file says `RESOLVED` 240 lines earlier), Linux `Timer::cancel` ("core will
not build on Linux" — it does, the arm is ported), the xev `ThreadPool` body (1630 lines,
0 `unimplemented!()`), the xcframework xtask ("no such xtask exists" — `xtask
xcframework` is registered), the `ghostty_*` alias layer (deleted upstream; the premise
is moot), the `+version` STUB lines (WS3-B replaced all three), the GUI launch leaf (the
GTK arm is live and diverges first), and `VERIFICATION_DEBT.md` V1 (ROADMAP records it
DISCHARGED 2026-07-23). Plus ~40 "Target: Phase-3" labels that mean the *pre-WS codename
for the GPU/xev/inspector tiers* — all landed — while "Phase 3" today means upstream
resync, so grepping this file for a Phase-3 worklist yields ~40 false positives.
**Consequence for WS19:** T2 is not cosmetic. The closure gate in T19 parses these
ledgers; feeding it 11 wrong rows and 40 mis-labels would make it authoritative about
fiction.

**4. The ROADMAP's own global constraint is false.** `ROADMAP:25` says "`ghostty_*`
C-ABI aliases stay". `grep -rn 'export_name' crates/rustty-apprt-c/src/` → 0 hits, and
`crates/rustty-apprt-c/Cargo.toml:31-33` states the crate "exports ONLY `rustty_*` (R2),
always, in every profile" because upstream deleted the layer at `e4cc852`. T2 corrects
the ROADMAP row.

**5. The G1b vacuity and the G1b blockage interact, and order matters.**
`check_test_counts.py` hardcodes `/Users/gael/projects/rustty/crates/...` for the RUST
side (`:25,:29,:39,:43,:46,:52,:56,:64,:67,:76,:91,:92`), so from a worktree it counts
`main`'s tests — G1b is vacuous for every worktree change, the identical bug
`miri-run.sh` already fixed with `git rev-parse --show-toplevel`. It is currently
**masked**: `~/.cache/zig/p/` was evicted, so the script exits `3 BLOCKED` before
counting. **Fix the path first, the corpus second** — restoring the corpus alone converts
a loudly-blocked gate into a silently-wrong one. The **oracle**-side hardcodes must stay:
R3 makes the oracle external, and `ci.sh:114-129` documents why an env-overridable oracle
path was deliberately killed as a spoof hole.

**6. The G3b beyond-pin artifact resolves wholesale when the pin advances — and must
not be papered over.** `check_comment_coverage.py:34` reads the oracle working tree at
the pin. A sentence upstream DELETED beyond the pin still exists at the pin, so porting
the beyond-pin state makes G3b report it "missing" (WS7 D2's config lane went 5→9 for
exactly this reason). Advancing `GHOSTTY_SRC` to the resynced tree removes the reference
side of every such row **by construction** — no exception list, no gate change, no R5
exposure. A per-file `GHOSTTY_SRC` override would be the same spoof hole `ci.sh` killed
for G1b. **WS19 records this verdict (T2); Phase 3 realizes it.** No code change here.

### What WS19 does itself vs what it routes

| Item | Disposition |
|---|---|
| N1 G1b worktree vacuity, N4 zig corpus, N5 debug-lane fail-fast | **WS19 T1** |
| Ledger truth-up: W1–W11, E.1–E.4, N6 verdict, O3/O5 restatement | **WS19 T2** |
| N2 `include/rustty.h` standalone C + widen A5 | **WS19 T3** |
| N3 two `gtk4::init()` tests in one binary | **WS19 T4** |
| O1 `style::Color` padding-free | **WS19 T5** |
| O2 `-C debug-assertions=off` vt Miri lane | **WS19 T6** |
| O4 inspector cell-detail restore + whole-file guard | **WS19 T7** |
| O6 GTK portal `OpenURI` | **WS19 T8** |
| O7 wayland decoration-mode re-read; O8 duplicate-global (→ divergence) | **WS19 T9** |
| O9 kqueue `thread_perform` `&mut Loop` + `task_loop` lineage | **WS19 T10** |
| O10 macOS File/TCP/UDP watchers; O11 xev `c_api.zig` (→ divergences) | **WS19 T11** |
| O12 `+version` OSC-8 commit hyperlink | **WS19 T12** |
| O18 synthetic binary entries; O19 bench binary entry + local `detectArgs` | **WS19 T13** |
| O19a `os_signpost`; O20 terminfo source/termcap (→ divergences); V1 delete; V5 → WS17 | **WS19 T14** |
| O21 `shell_integration.rs` lossy path | **WS19 T15** |
| N7 23 × `example/*/build.sh` stale dylib | **WS19 T16** |
| N9 `threadEnter` drain ordering + `posix_waitpid` `ECHILD` | **WS19 T17** |
| N10 `Terminal::compress(.incremental)` restart documentation | **WS19 T18** |
| **Phase 2 closure gate (G-P2)** | **WS19 T19** |
| O3 `cursor_scroll_region_up` (BL-4, beyond-pin) | **→ WS10** |
| O17 `stb_image_resize` glyph resize | **→ WS11** — routing was right, the STATUS was wrong: the port shipped 2026-06-30 (`2e82776`); WS11 closed the *vacuous-coverage* residual instead, 2026-07-29 |
| O13a `main_wasm.zig`, O14 `Atlas.Wasm` test, O15 `App.zig` Wasm block, O16 WebGL backend | **→ WS14** |
| O13b `main_build_data.zig` + `+gen-*` emit; N8 3 `.blp` msgids + 25 unwrapped call sites | **→ WS13** (already in its plan) |
| O5 `PageList::Limits` | **→ Phase 3** (re-vet row) |
| V2/V3/V4/V1b, and now **V5** | **→ WS17** (V5 was orphaned; T14 adds it to WS17's scope) |
| N6 G3b pin artifact | **→ Phase 3** (resolved by advancing the pin) |

### Dependency order

```
T1 ──┬──> T2 ──> (T5..T18 in any order, parallel-safe) ──> T19 ──> T20
     └──> T3, T4        (independent of T2; gate repairs)

T19 REQUIRES T2 (truthful ledgers) and every T5..T18 row it will assert on.
T5  BEFORE T6  (T6 re-measures the Miri inventory that T5 changes).
T7  BEFORE T19 (its guard test is one of the gate's assertions).
T9, T8  → VM-only verification; may run in parallel with the macOS tasks.
```

---

## Task 1: Repair the vacuous gates (N1, N4, N5) — **do this first**

**Rationale.** G1b is vacuous for worktree changes AND currently blocked; the debug test
lane masks failures. Until these are honest, no evidence produced by any later task can
be trusted. Order inside the task is load-bearing: path fix, then corpus, then verify the
gate actually binds.

- [ ] **Step 1: RED — prove the worktree vacuity, don't assume it**

```sh
cd /Users/gael/projects/rustty          # main checkout
git rev-parse --show-toplevel
python3 .port/tools/gates/check_test_counts.py; echo "main rc=$?"
cd "$(git -C /Users/gael/projects/rustty rev-parse --git-common-dir)/.."  # back to repo root
# Now from THIS worktree:
cd <ws19 worktree>
grep -n '"/Users/gael/projects/rustty' .port/tools/gates/check_test_counts.py
```

Expected: the grep prints the ~12 RUST-side hardcodes (`:25,:29,:39,:43,:46,:52,:56,:64,:67,:76,:91,:92`).
Record the output in the commit body — this is the vacuity proof.

- [ ] **Step 2: Fix the RUST-side root only; leave the ORACLE root hardcoded**

Edit `.port/tools/gates/check_test_counts.py`. Add near the top, above the path
constants:

```python
import subprocess

# WS19 T1 (N1): the RUST side must follow the CHECKOUT THIS SCRIPT LIVES IN, not a
# hardcoded absolute path. Run from a git worktree, the old hardcodes made the gate
# count `main`'s tests while reporting on the worktree's changes — G1b was VACUOUS for
# every worktree change. Same bug `.port/tools/miri-run.sh` already fixed this way.
RS_ROOT = subprocess.run(
    ["git", "rev-parse", "--show-toplevel"],
    cwd=os.path.dirname(os.path.abspath(__file__)),
    capture_output=True, text=True, check=True,
).stdout.strip()

# The ORACLE root stays hardcoded ON PURPOSE (R3: the oracle is an EXTERNAL reference,
# never shipped to CI) and stays NON-overridable: `.port/tools/ci.sh:114-129` records
# why an env-redirectable oracle probe was killed as a spoof hole (probe passes against
# a fake dir while the script counts zig=0 everywhere -> vacuous green, R5).
GH_ROOT = "/Users/gael/projects/ghostty"
```

Then rewrite every RUST-side constant as `os.path.join(RS_ROOT, "crates", ...)` and
every oracle-side one as `os.path.join(GH_ROOT, "src", ...)`. Do **not** introduce an
env override for either.

- [ ] **Step 3: GREEN — the gate now follows the worktree**

```sh
python3 - <<'PY'
import subprocess, sys
sys.path.insert(0, ".port/tools/gates")
import check_test_counts as m
print("RS_ROOT =", m.RS_ROOT)
PY
```

Expected: the WS19 worktree path, not `/Users/gael/projects/rustty`.

- [ ] **Step 4: Write the hash-pinned zig-dep restore script (N4)**

Create `.port/tools/restore_zig_deps.sh`. It must fetch the three vendored-dep sources
at the EXACT hashes `check_test_counts.py` names (`:162` libxev
`libxev-0.0.0-86vtc4IcEwCqEYxEYoN_3KXmc6A9VLcm22aVImfvecYs`, `:183` zf
`zf-0.10.3-OIRy8RuJAACKA3Lohoumrt85nRbHwbpMcUaLES8vxDnh`, `:200` vaxis
`vaxis-0.5.1-BWNV_LosCQAGmCCNOLljCIw6j6-yt53tji6n6rwJ2BhS`) into `~/.cache/zig/p/`,
verify each tarball's digest, and **fail loudly on a digest mismatch** — never fall back
to an unverified fetch. Take the URLs from `../ghostty/build.zig.zon.json` (read-only).

Requirements, each of which must be visible in the script:
- idempotent: a present, digest-matching directory is left alone;
- `set -euo pipefail`;
- prints the exact list of paths `check_test_counts.py` will read, then re-runs
  `python3 .port/tools/gates/check_test_counts.py` and reports its exit code;
- a `--verify-only` mode that checks presence + digests and exits non-zero without
  fetching (this is what CI calls).

- [ ] **Step 5: Prove the restore script unblocks G1b**

```sh
bash .port/tools/restore_zig_deps.sh
python3 .port/tools/gates/check_test_counts.py; echo "rc=$?"
```

Expected: `rc=0` (or a REAL per-module RED naming specific modules — which would be a
genuine finding, not a blockage). **A second `rc=3` means the script did not fetch what
the gate reads; fix the paths, do not relax the gate.**

- [ ] **Step 6: Register `--verify-only` as a BLOCKED-reporting pre-flight in `ci.sh`**

In `.port/tools/ci.sh`, `run_test_counts()` currently probes only the oracle. Extend it
to also call `bash "$ROOT/.port/tools/restore_zig_deps.sh" --verify-only` and `return 3`
with a message naming the restore command when the corpus is absent. Keep the existing
oracle probe unchanged.

- [ ] **Step 7: Fix the debug test lane's fail-fast masking (N5)**

`.port/tools/ci.sh:289` is bare `cargo test --workspace`. Change it to:

```sh
run_cargo "cargo test --workspace" cargo test --workspace --no-fail-fast
```

Add the comment explaining why (`check_release_lane.sh:155` is the precedent; the
masking is what hid the `rustty-extra` golden failures behind
`rustty-apprt-c --test smoke`). **Note in the comment that `ci.sh:288` already runs
`cargo build --workspace` first, so the cold-`target/` uplift half of that symptom does
not bite inside ci.sh** — only outside it, which is what T16 addresses for the examples.

- [ ] **Step 8: Prove the lane change is not cosmetic**

```sh
# Count targets reported. With fail-fast, a single early failure truncates the run.
cargo test --workspace --no-fail-fast 2>&1 | grep -c '^test result:'
```

Record the count. It must be >= the number of test targets in the workspace.

- [ ] **Step 9: Commit**

```sh
git commit -m "gates: G1b follows the worktree, not a hardcoded root; hash-pinned zig-dep restore; --no-fail-fast on the debug lane (WS19 T1)"
```

---

## Task 2: Ledger truth-up — the 11 wrong rows, the 4 structural defects, the routings

**Rationale.** T19's closure gate parses these files. Wrong rows would make it
authoritative about fiction. Nothing in this task changes code.

- [ ] **Step 1: Strike each of the 11 wrong rows in place, citing the verified evidence**

For each of W1–W11 in the `# PHASE 2 CLOSURE AUDIT (WS19)` section of
`.port/phase-2/DEFERRALS.md`, edit the ORIGINAL row (at its own line) to a struck form
carrying (a) `~~…~~` on the false text, (b) `**RESOLVED / STALE [WS19 T2 audit,
2026-07-29]**`, (c) the one-line code evidence, (d) a pointer to the audit row id. Do
NOT delete the historical text — this repo's convention is strike-and-annotate.

Rows and files:

| Row | File:line to edit |
|---|---|
| W1 | `.port/phase-2/DEFERRALS.md:1839`, `:1869-1878`; **and** `crates/rustty-core/src/renderer_seam.rs:35` (`PORT STATUS: placeholder` → the live status) |
| W2 | `.port/phase-2/DEFERRALS.md:2609-2614` |
| W3 | `.port/phase-2/DEFERRALS.md:2954`, `:2989-2996`; **and** `crates/rustty-cli/src/lib.rs:12` |
| W4 | `.port/phase-2/DEFERRALS.md:3409-3421` |
| W5 | `.port/phase-2/DEFERRALS.md:3258-3266` (keep the narrow macOS residual, now O10) |
| W6 | `.port/phase-2/DEFERRALS.md:3771` |
| W7 | `.port/phase-2/DEFERRALS.md:3756` |
| W8 | `.port/phase-2/DEFERRALS.md:3089-3093` |
| W9 | `.port/phase-2/DEFERRALS.md:3038-3047` |
| W10 | every `Target: Phase-3` / `Phase-3 R-d` / `Phase-3 dcimgui` label (`grep -n 'Phase-3\|Phase 3' .port/phase-2/DEFERRALS.md`) |
| W11 | `.port/phase-2/VERIFICATION_DEBT.md` V1 (already marked by the audit; delete the row here per T14) |

For W10 specifically: replace each label with the tier that actually owns it and its
landing evidence (`W0–W5` for windowing, `xev Milestone A/B` for D6, `WS6` for the
inspector), or delete the label where the residual is gone. **Add a one-line note at the
top of `DEFERRALS.md`** stating that "Phase 3" in historical entries is the pre-WS
codename for the GPU/xev/inspector tiers and is NOT
`.port/phase-3/` (upstream resync), so future greps are not misled.

- [ ] **Step 2: Fix the two mis-filed numbered items (audit §E.1)**

Move `DEFERRALS.md:5282` (item 3) and `:5301` (item 4) out from inside the `killPid`
divergence entry and back under their own `## Deferred to WS7 Phase E (Miri / gates)`
heading, restoring the 1–2–3–4 numbering. Verify afterwards that `D:5357` (the O2 Miri
lane item) is no longer nested inside a permanent-divergence entry:

```sh
awk '/^# Justified R1 Divergences/{d=1} d && /debug-assertions=off/{print NR": still inside divergences"}' .port/phase-2/DEFERRALS.md
```

Expected: no output.

- [ ] **Step 3: Move the two permanent items into the divergence section (audit §E.2)**

Relocate `D:4713` (the ReadThread POLLHUP quirk — "preserved verbatim, NOT fixed …
reference behavior") and `D:4910` (the xev ThreadPool `Event` shutdown-wake
"DELIBERATE DEVIATION (controller-approved)") under `# Justified R1 Divergences`. Both
already carry full rationale; add the 4-criterion framing header each divergence entry
in that section uses.

- [ ] **Step 4: Restate the two beyond-pin rows with their real owner and reasoning**

- `D:4981`/`:5011` (`cursor_scroll_region_up`): rewrite the **Disposition** paragraph to
  name **WS10 (BL-4 perf series)** as owner, state that WS8 is closed and that BL-4 was
  never WS8's scope, and state explicitly: *"`77190bd02` is beyond the pin. Porting it
  into the pinned tree means hand-transcribing a commit that Phase 3's re-vet will
  revisit; it is therefore cheaper AFTER Phase 3 advances the pin, and WS10 should
  sequence it that way. This is a stated sequencing decision, not a deferral — the owner
  and the trigger are both named."*
- `D:4973` (`PageList::Limits`): rewrite as a **Phase 3 re-vet row** with the exact
  assertion Phase 3 must make: *"if the resync introduces a `Limits` struct,
  `Limits::enforce` must call `mark_activity` on entry, taking rustty's
  `mark_activity` site count from 10 to 11."* Add the same line to
  `.port/phase-3/README.md`'s re-vet list so Phase 3 cannot miss it.

- [ ] **Step 5: Record the N6 G3b verdict where a lane owner will read it**

Add a subsection to `.port/tools/gates/README.md` under G3b titled **"Beyond-pin
comment deltas are EXPECTED (WS19 T2 verdict)"** stating: the gate reads the oracle
working tree at the pin; porting a beyond-pin state where upstream DELETED prose makes
that prose show up as `missing`; **advancing the pin in Phase 3 resolves it wholesale
and is the only correct fix**; a per-file `GHOSTTY_SRC` override is forbidden (the same
spoof hole `ci.sh:114-129` killed for G1b); until the pin advances, each lane records
its beyond-pin delta in its own report.

- [ ] **Step 6: Correct the ROADMAP (audit §E.3, §E.4) and its stale wave status**

In `docs/superpowers/plans/2026-07-23-rustty-completion-ROADMAP.md`:
- `:25` — replace "`ghostty_*` C-ABI aliases stay" with "`ghostty_*` C-ABI aliases were
  **deleted upstream at `e4cc852`**; rustty exports `rustty_*` only, in every profile
  (`crates/rustty-apprt-c/Cargo.toml:31-33`)".
- The "Wave status" block's last line — "**Round 2 next**: WS7 Phases B-E" — is stale:
  Phases B (`098fc63`), C (`ab2c08c`), D1–D8, and E (`bbecaf7`) are all merged. Replace
  with the true remaining scope (see the table in Step 7).
- The plan-authoring checklist line "`[ ] WS10–WS17: authored at wave start`" is stale:
  WS12, WS13, WS15, WS16, WS17 are authored (WS15/WS16 also executed+merged). Replace
  with per-WS rows.

- [ ] **Step 7: Write the true remaining-scope table into the ROADMAP**

| WS | Plan | Executed | True state |
|---|---|---|---|
| WS1–WS9 | authored | **merged** | DONE |
| WS10 perf (BL-4) | **NOT authored** | no | open; inherits O3 |
| WS11 font glyph resize | **authored + LANDED** (`docs/superpowers/plans/2026-07-23-ws11-font-glyph-resize.md`) | yes | **CLOSED 2026-07-29** — O17's port shipped 2026-06-30; WS11 closed the vacuous-coverage residual (5 tests + gate D10) |
| WS12 vaxis completion | authored (`c7c93f7`) | **no** | open; consumes T1's restore script |
| WS13 i18n & docs | authored (`39355ff`) | **no** | open; owns N8, O13b |
| WS14 wasm/WebGL | **NOT authored** | no | open; inherits O13a/O14/O15/O16 |
| WS15 GTK GlobalShortcuts | authored | **merged `e8a9f59`** | DONE |
| WS16 X11 winproto | authored | **merged `07b64db`** | DONE |
| WS17 verification debt | authored (`ef7621a`) | **no** | open; scope now V2/V3/V4/V1b **+ V5** |
| WS18 release lane | authored in-ROADMAP | **merged `eeec83f`** | DONE |
| **WS19 Phase 2 closeout** | this plan | in flight | closes Phase 2 |

- [ ] **Step 8: Commit**

```sh
git commit -m "port-docs: truth-up 11 wrong ledger rows, un-nest 2 mis-filed items, reclassify 2 permanent divergences, restate the beyond-pin owners; correct the ROADMAP (WS19 T2)"
```

---

## Task 3: `include/rustty.h` must compile standalone as C, and A5 must check it (N2)

**Rationale.** The full cbindgen header has never compiled as C. Gate A5
(`check_rustty_vt_headers.sh`) only compiles the **curated** `rustty/vt.h`, which is why
nobody noticed. A header we publish but never compile is an unproven artifact.

- [ ] **Step 1: RED — reproduce, and confirm it is pre-existing**

```sh
printf '#include "rustty.h"\nint main(void){return 0;}\n' > /tmp/ws19_hcheck.c
clang -fsyntax-only -I include /tmp/ws19_hcheck.c; echo "rc=$?"
grep -n 'Opaque' include/rustty.h
```

Expected: `rc=1` with `unknown type name 'Opaque'` at `include/rustty.h:2455` and `:2457`;
the grep shows the only `typedef`s are `Node` (`:90`) and `Command` (`:93`) — `Opaque` is
never declared. Confirm pre-existence with `git stash && <re-run> && git stash pop`.

- [ ] **Step 2: Find the cbindgen root cause, do not hand-patch the generated header**

```sh
grep -rn 'wasm_alloc_opaque\|wasm_free_opaque\|Opaque' crates/rustty-vt-c/src/ | head
grep -n 'parse_deps\|after_includes\|\[export\]' crates/rustty-vt-c/cbindgen.toml
```

The exports return `*mut Opaque` where `Opaque` is a Rust type cbindgen cannot resolve
(it emits `WARN: Can't find Opaque`). Fix at the source, in this order of preference:
(a) name a concrete `#[repr(C)] pub struct` cbindgen CAN see; (b) if the Rust type is
deliberately opaque, add the `typedef struct Opaque Opaque;` to `cbindgen.toml`'s
`after_includes` — the same hand-pinning mechanism E11 already uses for cross-crate C
types (`parse_deps=false`). **Do not edit `include/rustty.h` by hand** — it is generated
and would regress on the next `xtask header-gen`.

- [ ] **Step 3: Regenerate and prove GREEN in C and C++**

```sh
cargo run -p xtask -- header-gen
clang   -std=c11    -fsyntax-only -Wall -Wextra -I include /tmp/ws19_hcheck.c; echo "c   rc=$?"
printf '#include "rustty.h"\nint main(){return 0;}\n' > /tmp/ws19_hcheck.cpp
clang++ -std=c++17  -fsyntax-only -Wall -Wextra -I include /tmp/ws19_hcheck.cpp; echo "cpp rc=$?"
```

Both `rc=0`. Also confirm the cbindgen run emits **no** `Can't find` warning.

- [ ] **Step 4: Widen gate A5 so this can never regress**

In `.port/tools/gates/check_rustty_vt_headers.sh`, add a second probe alongside the
existing `#include <rustty/vt.h>` one (`:13`): compile `#include "rustty.h"` from
`include/` as **both** C11 and C++17 with `-Wall -Wextra`, and fail the gate on either.
Add the same for `include/rustty-apprt.h` if it is not already covered — E11 claims it
compiles standalone; **verify that claim rather than trusting it**, and record the result.

- [ ] **Step 5: Prove A5's new arm can FAIL (non-vacuity)**

```sh
cp include/rustty.h /tmp/rustty.h.bak
printf '\nBogusUndeclaredType *ws19_selftest_probe(void);\n' >> include/rustty.h
bash .port/tools/gates/check_rustty_vt_headers.sh; echo "must be non-zero: rc=$?"
cp /tmp/rustty.h.bak include/rustty.h
bash .port/tools/gates/check_rustty_vt_headers.sh; echo "must be zero: rc=$?"
```

- [ ] **Step 6: Commit**

```sh
git commit -m "vt-c: declare the wasm Opaque handle so include/rustty.h compiles standalone; A5 now compiles the FULL header as C and C++ (WS19 T3)"
```

---

## Task 4: One `gtk4::init()` per test binary (N3) — VM

**Rationale.** `crates/rustty-apprt/tests/gtk_wayland_winproto.rs` calls `gtk4::init()`
in two `#[test]`s (`:26`, `:81`). `gtk4::init()` pins GTK to the calling thread and
libtest gives each `#[test]` its own thread even at `--test-threads=1`, so the second
panics *"Attempted to initialize GTK from two different threads"*. WS16 T2 already
established the fix pattern for the x11 KATs (`a7fa3b7`); the wayland file was missed.

- [ ] **Step 1: RED — reproduce on the VM**

```sh
ssh rvm 'cd ~/projects/rustty && git fetch && git checkout ws19-phase2-closeout && \
  cargo test -p rustty-apprt --features gtk,wayland --test gtk_wayland_winproto -- --test-threads=1 2>&1 | tail -30'
```

Expected: the second test panics with the two-different-threads message. **Record the
verbatim panic** — if it does NOT reproduce, stop and re-derive the premise before
changing anything.

- [ ] **Step 2: Split, following the WS16 precedent exactly**

Look at how WS16 split its four x11 KATs (`crates/rustty-apprt/tests/gtk_x11_winproto.rs`,
`gtk_x11_xkb_init.rs`, `gtk_x11_event_mods.rs`, `gtk_x11_windowid.rs`) and mirror it:

- `crates/rustty-apprt/tests/gtk_wayland_winproto.rs` — keeps
  `wayland_app_init_selects_wayland_arm` only.
- `crates/rustty-apprt/tests/gtk_wayland_set_urgent.rs` — **create**; carries
  `wayland_set_urgent_dispatches_activation_token` plus the helpers it needs
  (`activation_done_count`, `drain_main_context`).

Add a module header to each stating the one-init-per-binary constraint and citing
`a7fa3b7`, so a future author does not merge them back.

- [ ] **Step 3: GREEN — both binaries pass**

```sh
ssh rvm 'cd ~/projects/rustty && \
  cargo test -p rustty-apprt --features gtk,wayland --test gtk_wayland_winproto -- --nocapture; \
  cargo test -p rustty-apprt --features gtk,wayland --test gtk_wayland_set_urgent -- --ignored --nocapture'
```

- [ ] **Step 4: Sweep for any other multi-`gtk4::init()` binary**

```sh
for f in crates/rustty-apprt/tests/*.rs; do
  n=$(grep -c 'gtk4::init()' "$f"); [ "${n:-0}" -gt 1 ] && echo "$f: $n";
done
```

Expected: no output after the split. Fix any other hit the same way.

- [ ] **Step 5: Update `VERIFICATION_DEBT.md` V4's recipe**

V4's recipe (`:160-162`) names `--test gtk_wayland_winproto`. Retarget it to
`--test gtk_wayland_set_urgent`, or WS17 will run the wrong binary.

- [ ] **Step 6: Commit**

```sh
git commit -m "rustty-apprt/gtk: split the wayland KATs one-per-binary (GTK is thread-pinned) (WS19 T4)"
```

---

## Task 5: `style::Color` padding-free (O1) — the orphaned WS8 item

**Rationale.** `crates/rustty-vt/src/style.rs:143` is a plain Rust enum, so a typed write
of a `Style` leaves the inactive `Color` payload uninitialized, and
`CompressedPage::init` compresses those bytes. Reproduction is the committed
`mapinit_styled` scenario. Owner was "WS8", which is closed.

- [ ] **Step 1: RED — reproduce with the committed harness scenario**

```sh
cd .port/phase-2/mirihar-pagelist-compress && cargo +nightly miri run -- mapinit_styled 2>&1 | tail -20
```

Expected: `reading memory at alloc…[0x17c..0x17d], but memory is uninitialized`.
Record it.

- [ ] **Step 2: Measure the true blast radius before touching anything**

```sh
grep -rn 'Color::None\|Color::Palette\|Color::Rgb' crates/ --include='*.rs' | wc -l
grep -rln 'Color::None\|Color::Palette\|Color::Rgb' crates/ --include='*.rs'
```

The audit measured 160 occurrences across 18 files (the old ledger said 163/17). Update
the ledger row with whatever you measure — the number is load-bearing for review.

- [ ] **Step 3: Design decision, recorded in-file before the edit**

The required shape is a byte-complete `#[repr(C)]` value with accessors, replacing the
Rust enum whose `None`/`Palette` variants do not cover their whole slot. The reference
is `union(Tag)` with `Tag = enum(u8) { none, palette, rgb }` and its own `PackedStyle`
already models the byte layout (`style.rs:677`+, `data_from_color`). Write the chosen
representation as a PORT NOTE at the type, arguing fidelity against the reference union
(a byte-complete value is *closer* to the Zig union than a Rust enum is), and keep
`ColorTag`'s explicit `u8` values.

**Constraint:** `Style` is embedded in `RefCountedSet` items that live inside the page
mapping, and `Page::assert_integrity` re-reads that memory — so any helper taking
`&mut Page`/`&mut [Cell]` across such a call is SB UB. Use raw-ptr args (memory:
rust-mut-args-strongly-protected).

- [ ] **Step 4: Convert the type and every site, in one atomic change**

Keep the public API shape (constructors + a `tag()` + accessors) so the 160 sites become
mechanical. Preserve `color_key`'s injective normalization exactly — it is on
`Style::eql`'s hot path and its PERF note must survive.

- [ ] **Step 5: GREEN — the harness, then the real production test**

```sh
cd .port/phase-2/mirihar-pagelist-compress && cargo +nightly miri run -- mapinit_styled
cd - && cargo test -p rustty-vt --lib 2>&1 | tail -3
RUSTFLAGS="-C debug-assertions=off" cargo +nightly miri test -p rustty-vt --lib \
  pagelist_compress_page_round_trips_a_cold_history_page -- --include-ignored 2>&1 | tail -5
```

Expected: `mapinit_styled` reports the mapping fully initialized; vt suite count
unchanged (2157 passed / 0 failed at audit time); the production Miri test passes. **The
production test is the acceptance evidence, not the harness** — that is the standard WS7
Phase E set.

- [ ] **Step 6: Update the ledgers**

Strike `DEFERRALS.md:5080` and the `KNOWN RESIDUAL` block in
`MIRI_EXCEPTIONS.md` ("WS7 Phase E — storing a non-default `style::Style`"), replacing
its 4-criterion acceptance basis with the resolution + the before/after Miri output.

- [ ] **Step 7: Commit**

```sh
git commit -m "vt/style: make Color padding-free so page mappings are fully initialized (WS19 T5, closes the orphaned WS8 item)"
```

---

## Task 6: A `-C debug-assertions=off` vt Miri lane (O2) — the orphaned WS8 item

**Rationale.** `check_leaks.sh` runs the vt Miri lane with debug assertions ON, where
`Page::assert_integrity` (O(page), on nearly every mutation) dominates and forces 10
`cfg_attr(miri, ignore)` annotations. WS7 Phase E measured that
`pagelist_compress_page_round_trips_a_cold_history_page` goes TIMEOUT(360s) →
**ok in 194s** with assertions off. Run T5 first: it changes what the inventory measures.

- [ ] **Step 1: Add the lane as an ADDITIVE second invocation, never a replacement**

In `.port/tools/gates/check_leaks.sh`, add a lane (call it **G6c**) that runs
`RUSTFLAGS="-C debug-assertions=off" cargo +nightly miri test -p rustty-vt --lib
-- --include-ignored` over the `pagelist_compress`/`compress` family only, with a hard
time bound. R5: the existing debug-assertions-ON lane stays exactly as it is — without it
the vt G6 lane cannot complete at all, and it is what upholds the integrity-gate
coverage. Document in-file that the new lane trades integrity checking (which adds no
SB/TB or leak coverage — the `mirihar-pagelist-compress` harness already relies on this
accommodation) for reach.

- [ ] **Step 2: Re-measure the whole `cfg_attr(miri, ignore)` inventory against the new lane**

```sh
grep -rn 'cfg_attr(miri, ignore)' crates/rustty-vt/src/ | tee /tmp/ws19-miri-inventory.txt
wc -l /tmp/ws19-miri-inventory.txt
```

For each entry, run it in **single-worker isolation** under the new lane with a 6-minute
bound (a parallel TIMEOUT is a candidate, never a verdict — WS7 Phase E's own rule).
Record a per-test table: name | ON-lane verdict | OFF-lane verdict | seconds.

- [ ] **Step 3: Un-annotate every test the new lane makes tractable**

Delete the `cfg_attr(miri, ignore)` from each newly-tractable test and let it run in the
G6c lane. **Do not blanket-annotate or blanket-un-annotate**: WS7 Phase E explicitly
found one tractable test (`pagelist_page_preserving_state_leaves_a_compressed_node_compressed`,
77s) that a blanket pass would have hidden.

- [ ] **Step 4: Prove G6c's FAIL arm is reachable**

Add `--self-test` to the lane: inject a deliberate UB (e.g. a stale-slice read) behind a
`ws19_selftest` cfg in a throwaway test, assert the gate exits non-zero and names it,
then remove the injection. Record the exit code.

- [ ] **Step 5: Register G6c in `ci.sh`**

Add to `GATE_REGISTRY` as `"G6c|PROGRESS|bash $GATES/check_leaks.sh --debug-assertions-off"`.
PROGRESS, not HARD, on first landing — the same bedding-in convention the D1–D9
detectors use — with an in-file note to promote it once it has ridden clean.

- [ ] **Step 6: Rewrite the `MIRI_EXCEPTIONS.md` WS7-Phase-E table against BOTH lanes**

The "4-criterion basis for the 10 annotations" section is now measured against a
two-lane world. Rewrite criterion 1 (empirical intractability) per lane and strike
`DEFERRALS.md:5357`.

- [ ] **Step 7: Commit**

```sh
git commit -m "gates: G6c — a -C debug-assertions=off vt Miri lane; re-measure the whole cfg_attr(miri,ignore) inventory (WS19 T6, closes the orphaned WS8 item)"
```

---

## Task 7: Inspector cell-detail path must not restore compressed pages (O4)

**Rationale.** WS7 D6 merged (`b8c40f9`) without closing the CELL-detail half of its own
deferral. `crates/rustty-inspector/src/widgets/pagelist.rs` still calls
`Node::page(cell.node)` at `:1701, :1703, :1759, :1889, :2163, :2379`, and the guard at
`:2414` only asserts the property over the page-tree loop source — exactly the weaker
form `D:4960` predicted.

- [ ] **Step 1: RED — tighten the guard FIRST, and watch it fail**

Edit the guard test at `pagelist.rs:2414` from the loop-scoped assertion to the whole-file
form the plan asked for:

```rust
assert!(
    !std::include_str!("pagelist.rs").contains("Node::page("),
    "WS19 T7: the cell-detail path must reach pages through page_preserving_state, \
     never Node::page (which RESTORES a compressed page just to inspect a cell)"
);
```

```sh
cargo test -p rustty-inspector 2>&1 | tail -20
```

Expected: **FAILS**, naming the 6 sites. This is the RED step; do not proceed until it
fails for the right reason.

- [ ] **Step 2: Read the oracle's end state (read-only, beyond-pin)**

```sh
git -C /Users/gael/projects/ghostty show origin/main:src/inspector/widgets/pagelist.zig | \
  sed -n '/InspectedCell/,/^}/p'
```

The oracle replaced `pages.getCell(pt)` with `pages.pin(pt)` +
`pin.node.pagePreservingState(...)` + an `InspectedCell` struct, and threads the
resulting `page: *const Page` through `cell_info.draw`, `hyperlinkTable` and
`graphemeTable`. Port that structure.

- [ ] **Step 3: Port it, threading `page` instead of re-deriving it**

Introduce the `InspectedCell` struct, take the pin once via `page_preserving_state`, and
pass the borrowed `page` to the three consumers. **Aliasing:** the returned `*const Page`
must not be held across any `&mut` to the owning `PageList` — use a raw-ptr arg and a
narrow access window (memory: shaper-hook-sb-clean-interleave).

- [ ] **Step 4: GREEN**

```sh
cargo test -p rustty-inspector 2>&1 | tail -5
grep -c 'Node::page(' crates/rustty-inspector/src/widgets/pagelist.rs
```

Expected: tests pass; the grep prints `0` (or only the guard-test literal — if so, move
the guard into a sibling file so the assertion is not self-satisfied, which is exactly
the "guard test that passes on unmodified code" failure this program has hit before).

- [ ] **Step 5: Strike `DEFERRALS.md:4955-4971` item 1 and commit**

```sh
git commit -m "inspector/pagelist: inspect cells through page_preserving_state, never Node::page; guard the whole file (WS19 T7, closes the orphaned WS7-D6 item)"
```

---

## Task 8: GTK portal `OpenURI` (O6) — VM

**Rationale.** `D:4046` is marked **OPEN** with no owner: WS15 split it out of the
GlobalShortcuts entry and did not take it. Consequence: opening a URL/file through the
desktop portal (the sandbox-correct path) is unavailable on GTK; only the non-portal
fallback exists.

- [ ] **Step 1: Read the oracle at the pin**

```sh
sed -n '1,200p' /Users/gael/projects/ghostty/src/apprt/gtk/portal.zig
sed -n '1,200p' /Users/gael/projects/ghostty/src/apprt/gtk/portal/OpenURI.zig
grep -n 'portal' /Users/gael/projects/ghostty/src/apprt/gtk/class/application.zig
```

- [ ] **Step 2: RED — a failing KAT first**

Create `crates/rustty-apprt/tests/gtk_portal_openuri.rs` asserting that the portal
`OpenURI` path is reached (request object path computed, `Response` pre-subscribed before
the call is issued). Model it on WS15's `global_shortcuts` tests. It must fail today.

- [ ] **Step 3: Port `portal.zig` + `portal/OpenURI.zig`**

Onto the same `gio::DBusConnection` + self-computed-`Request`-object-path pattern
`crates/rustty-apprt/src/gtk/class/global_shortcuts.rs` establishes: pre-subscribe to
`Response` on a self-computed request path **before** issuing the call (this ordering is
load-bearing — WS15 found the race). New file
`crates/rustty-apprt/src/gtk/portal/{mod.rs,open_uri.rs}`; wire the consumer at
`application.rs`'s `startup`, which already fetches the connection.

- [ ] **Step 4: Live-verify on the VM, reusing WS15's environment setup**

`.port/tools/gates/ws15_globalshortcuts_portal_gate.sh` already encodes the two
environment facts any portal call needs (the app-id / `app.slice` / `.desktop`
requirement; the per-app-id grant store). Reuse that setup. Assert with a
`dbus-monitor` capture that the emitted traffic matches the installed
`ghostty 1.3.1-2.fc44` doing the same handshake — same methods, same order, same payload
shapes, differing only in the R2 token prefix. **Include a negative control** (no portal
running → the fallback path is taken, no hang).

- [ ] **Step 5: Strike `DEFERRALS.md:4046` and commit**

```sh
git commit -m "rustty-apprt/gtk: port the XDG portal OpenURI call site (WS19 T8, closes the unowned L6 portal deferral)"
```

---

## Task 9: Wayland decoration-mode re-read (O7) + duplicate-global divergence (O8) — VM

- [ ] **Step 1: RED — pin the current snapshot behavior**

Add a KAT asserting that a decoration-mode change delivered on the private queue
(now cranked by `WlQueueSource`) is reflected in `Window`'s effective mode. It must fail
today: `Window` snapshots the mode at init and never re-reads `Globals`.

- [ ] **Step 2: Fix — re-read `Globals` at sync time**

In `crates/rustty-apprt/src/gtk/winproto/wayland.rs`, make `Window::sync_appearance` /
`client_side_decoration_enabled` read the live `Globals` default decoration mode rather
than the init snapshot, matching the oracle's own read point. Keep the config override
precedence unchanged.

- [ ] **Step 3: GREEN, on a real compositor**

Run the KAT under isolated headless sway (the WS15/WS16 precedent — **not** the primary
GNOME session; two hard host crashes correlate with primary-session KAT activity, see
`VERIFICATION_DEBT.md` V4).

- [ ] **Step 4: O8 — reclassify the duplicate-global nit as a permanent divergence**

Move `DEFERRALS.md:4905` into `# Justified R1 Divergences` with this 4-criterion basis:
(1) **bounded input** — it requires a compositor to re-advertise a *singleton* interface
under a new `name`, which no compositor does; (2) **the oracle has the same shape** —
`Globals.zig`'s bind path likewise overwrites the typed proxy; (3) **failure mode is
safe** — a `global_remove(old_name)` would unbind a live proxy, degrading to the
graceful-`None` arm the code already handles for absent globals, never UB; (4) **cost of
fixing exceeds the benefit** — tracking `tag→name` bidirectionally to defend against an
input no implementation produces adds state the reference does not have.

- [ ] **Step 5: Commit**

```sh
git commit -m "rustty-apprt/gtk: re-read the live wayland decoration mode instead of the init snapshot; ledger the duplicate-global nit as a justified divergence (WS19 T9)"
```

---

## Task 10: kqueue `thread_perform` must not form `&mut Loop` on a pool worker (O9)

**Rationale.** `D:4565` is filed as "Follow-up (open, low)" **inside a RESOLVED entry**,
so it is invisible to any grep for open work — and it is no longer dead: the ThreadPool
body has landed (audit W5), so `thread_pool` can be `Some`. `epoll.rs`'s twin was already
fixed to notify through a by-value `Copy` eventfd, forming no `&mut Loop`; `kqueue.rs`
still carries the `self.wakeup()` idiom.

- [ ] **Step 1: RED on the isolated harness, not by hand-reasoning**

Extend `.port/phase-2/mirihar-xev-mpsc` with a scenario reproducing a pool worker
forming `&mut Loop` concurrently with the loop thread's accesses. It must be RED under
**both** Stacked and Tree Borrows before the fix. Per doctrine
(shaper-hook-sb-clean-interleave), SB conclusions come from a harness, never from
argument.

- [ ] **Step 2: Fix — mirror epoll's copied-fd notify**

In `crates/rustty-xev/src/kqueue.rs`, replace the pool-worker `self.wakeup()` autoref
with a by-value `Copy` handle notify, exactly as `epoll.rs` does.

- [ ] **Step 3: Audit the full `task_loop` pointer lineage**

Trace every `(*c).task_loop` deref and confirm none forms a `&mut Loop` on a worker.
Record the site list in the commit body.

- [ ] **Step 4: GREEN under both models**

```sh
cd .port/phase-2/mirihar-xev-mpsc
MIRIFLAGS="-Zmiri-strict-provenance" cargo +nightly miri run -- <scenario>
MIRIFLAGS="-Zmiri-tree-borrows"      cargo +nightly miri run -- <scenario>
cd - && cargo test -p rustty-xev 2>&1 | tail -3
```

- [ ] **Step 5: Strike `DEFERRALS.md:4565` and commit**

```sh
git commit -m "xev/kqueue: notify the pool worker through a copied fd handle, forming no &mut Loop; task_loop lineage audited (WS19 T10)"
```

---

## Task 11: Reclassify O10 and O11 as justified divergences

**Rationale.** Both are permanent by their own logic but sit in "DEFERRED" lists, so a
closure gate would read them as open. Neither is a deferral of choice.

- [ ] **Step 1: O10 — macOS `xev.File`/`TCP`/`UDP` + the `kqueue: file IO on thread pool` skip-stub**

Move `DEFERRALS.md:3273` (and the narrow residual left by W5's correction of `:3258`)
into `# Justified R1 Divergences`, with this basis: (1) **zero consumers** — ghostty makes
no use of `xev.File`/`TCP`/`UDP` on macOS, and rustty's only pool consumer is the Linux
epoll `File` watcher; (2) **the divergence is gate-pinned, not hidden** —
`crates/rustty-xev/tests/surface_parity.rs` asserts each target's actual exported
surface, so neither side can drift silently; (3) **the Linux side IS ported**, so the
capability exists in the tree and the macOS gap is an export decision, not missing logic;
(4) **the skip-stub is honest** — `kqueue_file_io_on_thread_pool` is a SkipZigTest-style
`#[test]` that keeps the count at 12 and defers genuinely-unused functionality.

- [ ] **Step 2: O11 — xev `c_api.zig`**

Move `DEFERRALS.md:3274`'s `c_api.zig` clause into the divergence section: (1) libxev's
own C ABI is outside the port's consumption surface — every rustty consumer uses the Rust
API; (2) ghostty does not consume it either, so there is no fidelity obligation to a
behavior any caller observes; (3) porting it would add an unexercised C surface that no
gate could bind; (4) if a consumer ever appears, the row names the trigger.

- [ ] **Step 3: Commit**

```sh
git commit -m "port-docs: reclassify the macOS xev watcher surface and libxev c_api as justified R1 divergences (WS19 T11)"
```

---

## Task 12: `+version` OSC-8 commit hyperlink (O12)

- [ ] **Step 1: RED — a KAT asserting the hyperlink is emitted**

Extend `crates/rustty/tests/cli_smoke.rs`'s `version_plus` to assert the OSC-8 sequence
appears with a real commit hash. It must fail today (`commit_hash()` is always `None`).

- [ ] **Step 2: Add the repo-URL identity constant**

Add to `crates/rustty-identity` the rustty repo URL (R2: the reference's
`github.com/ghostty-org/ghostty/commit/…` is a product-identity value, so the rustty
constant is the rustty repo, with the oracle spelling under `identity-oracle`).

- [ ] **Step 3: Thread a build-time commit hash**

Capture it in a `build.rs` (`git rev-parse --short HEAD`), degrading gracefully to `None`
outside a git checkout so a source-tarball build does not fail. Wire it into
`crates/rustty-cli/src/version.rs`'s hyperlink branch.

- [ ] **Step 4: GREEN + strike `DEFERRALS.md:2643`; commit**

```sh
git commit -m "cli/version: emit the OSC-8 commit hyperlink with a real hash + rustty repo-URL identity constant (WS19 T12)"
```

---

## Task 13: The binary-entry seams (O18, O19)

**Rationale.** `rustty-synthetic`'s 4 `cli.rs` seams and `rustty-bench`'s `main_bench`
entry share one un-defer trigger ("the `rustty +synthetic` / bench binary entry") that no
workstream owns. **Decide in-plan; do not defer.**

- [ ] **Step 1: Establish whether the acyclic split actually forbids the edge**

```sh
grep -n 'rustty-cli\|rustty-synthetic\|rustty-bench' crates/rustty/Cargo.toml
bash .port/tools/gates/check_acyclic.sh
```

`cli.rs:56`'s stated blocker is that `detectArgs` lives ABOVE `synthetic`. The `rustty`
bin already deps `rustty-cli` and is the top sink (C6, ADV-FIX 2), so the entry can live
**in the bin**, not in the leaf — the same shape C6 used for action detection.

- [ ] **Step 2: Implement, or record as permanent — with the reason**

If the bin-hosted entry works: wire `main` + `main_action_impl_{ascii,osc,utf8}` and the
bench entry there, and delete the 4+1 `unimplemented!()` seams. If it provably cannot
(G7 RED), record **permanent divergence** with the exact cycle as the basis — not a
deferral. Do not leave a third option.

- [ ] **Step 3: O19a — `os_signpost` as a justified divergence**

Move the `Benchmark.zig:44-65` macOS `os_signpost` Instruments interval into the
divergence section: (1) it is **pure instrumentation** — an Instruments trace interval
with zero effect on measured behavior or output; (2) the verbatim Zig body is preserved
at the site (R1 no-loss); (3) no test in the reference or the port observes it; (4) the
binding tier it needs (`macos.os.Log` family) is the same log-facade seam the tree defers
tree-wide, so filling it here would be an isolated one-off.

- [ ] **Step 4: GREEN + strike `DEFERRALS.md:3668`/`:3697`; commit**

```sh
git commit -m "synthetic+bench: host the binary entries in the rustty bin; ledger os_signpost as a justified divergence (WS19 T13)"
```

---

## Task 14: The reclassification sweep — divergences, verification debt, WS17 scope

- [ ] **Step 1: O20 — terminfo SOURCE + termcap as a justified divergence**

Move `INTEGRATION_PARITY.md §2.8` into `DEFERRALS.md`'s divergence section and **delete
fix-list item 8** (a row marked "(Optional)" is not a disposition). Basis: (1) the
reference's `emit_terminfo`/`emit_termcap` default **FALSE for Release**
(`Config.zig:410-429`) and the shipping app IS Release, so **rustty matches the oracle
exactly in the shipped configuration**; (2) the divergence is confined to a Debug build
and to *informational* files — the compiled DB, which is what TERM resolution actually
uses, IS produced (`xtask/src/res_terminfo.rs:50-90`); (3) no test observes it; (4) if a
Debug-parity need appears, the row names the two `GhosttyResources.zig` hunks to mirror.

- [ ] **Step 2: W11/V1 — delete the discharged verification-debt row**

Delete `VERIFICATION_DEBT.md` V1 (the audit already marked it). That file's header says
verified items are deleted, not left marked; the audit trail lives in the
`# PHASE 2 CLOSURE AUDIT (WS19)` section.

- [ ] **Step 3: V5 — add it to WS17's scope, in WS17's own plan**

Edit `docs/superpowers/plans/2026-07-29-ws17-verification-debt.md`: add V5 to the Goal
sentence and to the scope table. Note the leverage the audit found: **V5's items 2 and 3
are discharged by the very KDE-Plasma-on-X11 box V3 already requires** (a native X11
session AND KWin), so V5 costs WS17 almost nothing beyond V3; only item 1 (a physical
keypress) needs real hardware input. Also update WS17's ROADMAP row scope cell.

- [ ] **Step 4: Route the remaining items to their workstreams, in the ledger**

For each, rewrite the ledger row's target line to name the WS and add a bullet to that
WS's plan (or, where no plan exists, to the ROADMAP row so the plan author inherits it):

| Row | New owner |
|---|---|
| `D:3741` `stb_image_resize` (O17) | **WS11** — plan `2026-07-23-ws11-font-glyph-resize.md`, **LANDED 2026-07-29**; the port itself shipped 2026-06-30, WS11 closed the vacuous-coverage residual |
| `D:571` WebGL backend (O16) | **WS14** — ROADMAP row (no plan yet) |
| `D:1894` `App.zig` Wasm block (O15) | **WS14** |
| `D:4209` `Atlas.Wasm` test (O14) | **WS14** |
| `D:3065` `main_wasm.zig` (O13a) | **WS14** |
| `D:3725`/`:3065` `main_build_data` + `+gen-*` emit (O13b) | **WS13** — add to its plan |
| `D:4981` `cursor_scroll_region_up` (O3) | **WS10** — ROADMAP row; pin-sequenced (see T2 Step 4) |
| `D:4973` `PageList::Limits` (O5) | **Phase 3** re-vet (see T2 Step 4) |

- [ ] **Step 5: Commit**

```sh
git commit -m "port-docs: reclassify terminfo-source as a divergence, delete the discharged V1, hand V5 to WS17, route O3/O5/O13-O17 to their workstreams (WS19 T14)"
```

---

## Task 15: `shell_integration.rs` lossy path bytes (O21)

- [ ] **Step 1: Fix the two sites**

`crates/rustty-termio/src/shell_integration.rs:1708` and the `realpath_bytes` helper
(`:1753-1763`) build owned `Vec<u8>` paths via
`to_string_lossy().into_owned().into_bytes()`. Replace with
`std::os::unix::ffi::OsStrExt::as_bytes` (memory: zig-byteslice-to-rust-rawbytes).

- [ ] **Step 2: Prove it with a non-UTF-8 path**

Add a `#[test]` creating a temp dir whose name contains a non-UTF-8 byte (e.g.
`b"\xff"`), and assert the helper round-trips the exact bytes. Under the old code the
byte becomes U+FFFD — so this test is the RED/GREEN discriminator, not a smoke test.

- [ ] **Step 3: GREEN + strike `DEFERRALS.md:4595`; commit**

```sh
git commit -m "termio/shell-integration: raw OsStr bytes for tmp paths, never lossy String (WS19 T15)"
```

---

## Task 16: `example/*/build.sh` must not link a stale dylib (N7)

- [ ] **Step 1: RED — prove the stale-link hazard**

```sh
cargo build -p rustty-vt-c --release --features "identity-oracle simd"
touch -t 200001010000 target/release/librustty_vt_c.dylib   # make it obviously stale
grep -n 'librustty_vt_c' example/c-vt-colors/build.sh
```

The `[ -f "$lib" ] || (cargo build …)` guard at `:11` means the stale file is used as-is.

- [ ] **Step 2: Fix all 23 identically**

Replace the `[ -f "$lib" ] || …` guard with an unconditional
`cargo build -p rustty-vt-c --release --features "identity-oracle simd"` (cargo is
already incremental, so the "avoid rebuilding" motive is void). Apply to every
`example/*/build.sh` — `ls example/*/build.sh | wc -l` → 23. Keep each script's existing
flags and profile.

- [ ] **Step 3: Verify all 23 build+run**

```sh
for d in example/*/; do
  [ -f "$d/build.sh" ] || continue
  sh "$d/build.sh" >/dev/null 2>&1 && echo "ok   $d" || echo "FAIL $d"
done
```

- [ ] **Step 4: Sweep for the same guard elsewhere**

```sh
grep -rn '\[ -f .*\.dylib" \] ||' example/ macos/ .port/tools/ xtask/ 2>/dev/null
```

- [ ] **Step 5: Commit**

```sh
git commit -m "example: always rebuild librustty_vt_c instead of accepting a stale release dylib (all 23) (WS19 T16)"
```

---

## Task 17: termio teardown — `threadEnter` drain ordering + `posix_waitpid` `ECHILD` (N9)

**Rationale.** `D:5090` and `D:5130` were both filed 2026-07-29 with no owner. The first
is a real (narrow) unrecoverable-hang path; the second is a reachable panic.

- [ ] **Step 1: RED — reproduce the no-drainer wedge with the committed harness**

```sh
bash .port/tools/repro_subprocess_hang.sh 2>&1 | tail -20
bash .port/tools/capture_subprocess_hang.sh 2>&1 | tail -30
```

Expected: the child in macOS state `?Es`, the parent spinning inside `killPid`. Record it.

- [ ] **Step 2: Restructure `threadEnter`'s error arms to drain before `stop()`**

In `crates/rustty-termio/src/exec.rs`, the `pipe()`, `xev::Timer::init`,
`xev::Process::init` and read-thread `spawn` failure arms each call
`self.subprocess.stop()` **before the io-reader thread exists**. Add a "drain the master
once, synchronously" helper shared with the read thread, and call it on every such arm
before `stop()` — the invariant `threadExit` gets for free. **This is a deliberate
divergence from upstream, which has the same ordering and the same latent defect:** record
it as `[R1-DIVERGENCE]` next to the bounded `killPid` entry, with the reasoning (liveness
> R1, the same trade the bounded wait already made) and the intent to upstream it.

- [ ] **Step 3: Give `posix_waitpid` an `ECHILD`-aware variant for `killPid` only**

`crates/rustty-termio/src/command.rs`'s `posix_waitpid` treats every non-`EINTR` errno as
`unreachable!()`. `ECHILD` is reachable because `crates/rustty-xev/src/process.rs`
`waitpid`s in the `.proc` trampoline. Add `posix_waitpid_checked` returning
`Option<WaitPidResult>` (or an errno-carrying variant) used **only** by `killPid`, where
`ECHILD` means "reaped by someone else" → break the loop, matching the loop's actual
intent. Leave the verbatim shim on the `Command::wait` path untouched so the Zig-std
contract the port reproduces elsewhere is preserved. Record as `[R1-DIVERGENCE]` too.

- [ ] **Step 4: GREEN**

```sh
bash .port/tools/repro_subprocess_hang.sh 2>&1 | tail -5    # expect 0 hangs
cargo test -p rustty-termio 2>&1 | tail -3
bash .port/tools/gates/check_release_lane.sh --crate rustty-termio
```

**Do not add an in-suite guard that reproduces the wedge** — the `killPid` divergence
entry records why the previous such test was deleted (it asserted nothing that could
fail, its only failure mode was hanging, and it stranded `?Es` children). Deterministic
evidence stays out-of-suite in `repro_subprocess_hang.sh`.

- [ ] **Step 5: Strike `DEFERRALS.md:5090`/`:5130`; commit**

```sh
git commit -m "fix(termio): drain the pty master before stop() on threadEnter error arms; ECHILD-aware waitpid in killPid [2 R1-divergences] (WS19 T17)"
```

---

## Task 18: Document the `Terminal::compress(.incremental)` restart (N10)

**Rationale.** A documentation finding, not a defect. A fresh
`Terminal::compress(.incremental)` right after a COMPLETED pass returns `Pending`, not
`Complete`, because the incremental traversal restarts — so the obvious "work is done"
probe is invalid. `renderer_seam.rs:1990-1997` already depends on the distinction
(schedules another tick on `Pending`, stops on `Complete`/`Unsupported`), but nothing
states the restart, and the next test author will read `Pending` as "not finished".

- [ ] **Step 1: Confirm the behavior, do not assume it**

```sh
cargo test -p rustty-vt --lib pagelist_compress_incremental_converges_to_complete -- --nocapture 2>&1 | tail
```

Then write a scratch probe: drive `compress(Incremental)` to `Complete`, call it once
more, and print the result. Record the observed value.

- [ ] **Step 2: Extend the doc comment where a caller will read it**

`crates/rustty-vt/src/terminal.rs:2847-2860` currently says only "the return value tells
callers whether they should continue calling it in the future". Add, as a `///` block
below it (this is rustty-only clarification of ported behavior, so mark it `PORT NOTE`,
not reference prose):

```
/// PORT NOTE (WS19 T18) — `Pending` does NOT mean "unfinished work remains".
/// A fresh `compress(.incremental)` issued right after a pass that returned
/// `Complete` returns `Pending` again, because the incremental traversal
/// RESTARTS from the beginning rather than remembering that it converged.
/// So `Pending` means "call me again", never "there is work outstanding", and
/// `result == Complete` is NOT a durable "compression is done" probe — it is
/// only true of the pass that just ran. Callers that want an idle-detection
/// signal must debounce on activity (see the renderer's compression scheduler,
/// `rustty-core/src/renderer_seam.rs`), not on this return value.
```

- [ ] **Step 3: Pin the behavior with a test, so the doc cannot rot**

Add `terminal_compress_incremental_restarts_after_complete` to `terminal.rs`'s test
module: drive to `Complete`, assert the next call returns `Pending`. Name it so a future
reader finds it from the doc comment, and reference the test from the PORT NOTE.

- [ ] **Step 4: Cross-reference from the compression docs**

Add the same one-liner to the WS7 section of `.port/phase-2/MIRI_EXCEPTIONS.md`'s
compression narrative (where the `compress(Drain)`/`compress(Incremental)` coverage
argument lives) so the fact is discoverable from the compression side too.

- [ ] **Step 5: Commit**

```sh
git commit -m "vt/terminal: document that compress(.incremental) restarts after Complete, and pin it with a test (WS19 T18)"
```

---

## Task 19: The Phase 2 closure gate (**G-P2**) — with a proven-reachable FAIL arm

**Rationale.** The deliverable that makes Phase 2 *stay* closed. It parses the ledgers
and FAILS if any item is open without an owner in an open workstream. **A gate that
cannot fail is worthless**, so its FAIL arm is proven with a synthetic fixture, not
asserted.

- [ ] **Step 1: Define the contract precisely, in the script's header**

Create `.port/tools/gates/check_phase2_closure.py`. Contract:

1. Parse `.port/phase-2/DEFERRALS.md`, `VERIFICATION_DEBT.md`, `MIRI_EXCEPTIONS.md`,
   `INTEGRATION_PARITY.md`.
2. An **item** is a heading or numbered row carrying an open marker
   (`OPEN`, `STILL DEFERRED`, `DEFERRED`, `Target:`, `Un-defer trigger:`) and **not**
   struck (`~~…~~`), and not inside the `# Justified R1 Divergences` section.
3. Every such item **must** name an owner matching `WS(1[0-9]|[1-9])\b` or `Phase 3`,
   and that owner must appear in the gate's **OPEN_WORKSTREAMS** set.
4. **OPEN_WORKSTREAMS** is derived from the ROADMAP checklist, **not hardcoded**: a WS
   whose row is marked `merged`/`DONE` is CLOSED. Filing into a closed WS is the exact
   failure this gate exists to catch, so it must be derived from the same file a human
   would read.
5. Exit `0` clean; `1` on any unowned or closed-WS-owned item, printing
   `file:line | item | owner-or-NONE | reason`; `3` BLOCKED if a ledger file is missing
   (never a silent green — the `ci.sh` convention).

Add the **anti-vacuity guard** the other gates use: if the parser finds **zero** items in
a file that is non-empty, FAIL LOUDLY. A parser that silently matches nothing is the
classic vacuous green (this program has already been bitten by an `OUT_DIR/locale`
nothing reads and a `_vt_header_baseline.txt` no gate reads).

- [ ] **Step 2: Run it against the real tree; it must be GREEN only after T2–T18**

```sh
python3 .port/tools/gates/check_phase2_closure.py; echo "rc=$?"
```

If RED, the output IS the remaining Phase 2 worklist. Do not relax the gate to make it
green — close the rows or route them (which is what T2/T14 exist for).

- [ ] **Step 3: Prove the FAIL arm with a SYNTHETIC FIXTURE (the non-vacuity proof)**

Create `.port/tools/gates/fixtures/phase2-closure-orphan.md` — a minimal ledger fragment
reproducing the exact defect this whole audit was created by:

```markdown
# Synthetic fixture — NOT a real ledger. Proves check_phase2_closure.py's FAIL arm.

## Deferred to WS8 upstream sweep — synthetic orphan
**OPEN.** A synthetic item filed to WS8, which is merged and closed.
**Target: WS8.**

## Synthetic item with no owner at all
**OPEN.** No target line anywhere in this entry.
```

Add `--fixture <path>` to the gate. Then prove all three arms:

```sh
# 1. FAIL on the orphan fixture — must name BOTH rows and exit 1
python3 .port/tools/gates/check_phase2_closure.py \
  --fixture .port/tools/gates/fixtures/phase2-closure-orphan.md; echo "must be 1: rc=$?"

# 2. PASS on a fixture whose item names an OPEN workstream — proves PASS is reachable
python3 .port/tools/gates/check_phase2_closure.py \
  --fixture .port/tools/gates/fixtures/phase2-closure-owned.md;  echo "must be 0: rc=$?"

# 3. BLOCKED on a missing file — proves absence is never a silent green
python3 .port/tools/gates/check_phase2_closure.py \
  --fixture /nonexistent.md; echo "must be 3: rc=$?"
```

Create `phase2-closure-owned.md` as the positive control (same shape, `**Target: WS14.**`).
**All three exit codes must be observed and recorded in the commit body.** A gate with
only a demonstrated PASS is exactly the failure mode this task exists to prevent.

- [ ] **Step 4: Prove the closed-WS derivation is live, not hardcoded**

Temporarily edit a ROADMAP row to mark an open WS as merged, re-run the gate against
the owned fixture, and confirm it flips to FAIL. Revert. Record both runs.

- [ ] **Step 5: Register G-P2 HARD in `ci.sh`**

```sh
"G-P2|HARD|$PY $GATES/check_phase2_closure.py"
```

HARD is correct here (unlike a bedding-in detector): it is green at landing, fast, and
its FAIL arm has a demonstrated fixture proof — the same bar `G6b` and `G-REL` met.
Document it in `.port/tools/gates/README.md` with the fixture commands.

- [ ] **Step 6: Commit**

```sh
git commit -m "gates: G-P2 — Phase 2 closure gate; FAILS on any open item without an owner in an OPEN workstream, FAIL arm proven by synthetic fixture (WS19 T19)"
```

---

## Task 20: Finale — both platforms green, full gate sweep, merge

- [ ] **Step 1: macOS full sweep**

```sh
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo build --workspace
cargo test --workspace --no-fail-fast 2>&1 | tail -40
bash .port/tools/gates/check_acyclic.sh
bash .port/tools/gates/check_release_lane.sh
bash .port/tools/gates/check_pagelist_compress_leaks.sh
bash .port/tools/gates/check_rustty_vt_headers.sh
python3 .port/tools/gates/check_test_counts.py
python3 .port/tools/gates/check_phase2_closure.py
```

**Do NOT run `no_homebrew_gettext_gate.sh`** (mutates Homebrew state; has broken two
concurrent lanes).

- [ ] **Step 2: Linux/VM sweep**

```sh
ssh rvm 'cd ~/projects/rustty && git fetch && git checkout ws19-phase2-closeout && \
  cargo build --workspace && cargo test --workspace --no-fail-fast 2>&1 | tail -40'
ssh rvm 'cd ~/projects/rustty && cargo build --features gtk,wayland,x11 -p rustty'
```

- [ ] **Step 3: Full CI entry point on both hosts**

```sh
bash .port/tools/ci.sh 2>&1 | tail -60
ssh rvm 'cd ~/projects/rustty && bash .port/tools/ci.sh 2>&1 | tail -60'
```

- [ ] **Step 4: Final ledger consistency check**

```sh
# every audit row O1..O21 / N1..N10 must be either struck, reclassified, or routed
grep -n '^| O[0-9]\|^| \*\*N[0-9]' .port/phase-2/DEFERRALS.md
python3 .port/tools/gates/check_phase2_closure.py; echo "must be 0: rc=$?"
```

- [ ] **Step 5: Mark Phase 2 CLOSED in the ROADMAP and record the gate**

Add a "Phase 2 closure" section to the ROADMAP: G-P2 is the standing invariant; the
remaining open workstreams are WS10–WS14 and WS17; Phase 3 may now execute.

- [ ] **Step 6: Merge**

```sh
git checkout main && git merge --no-ff ws19-phase2-closeout \
  -m "merge: WS19 Phase 2 closeout — 4 vacuous/broken gates repaired, 21 orphaned items owned, 11 wrong ledger rows corrected, G-P2 closure gate landed"
```

---

## Acceptance criteria

- [ ] `python3 .port/tools/gates/check_phase2_closure.py` exits **0** on the real tree, and its FAIL, PASS and BLOCKED arms are each demonstrated on fixtures with recorded exit codes.
- [ ] `check_test_counts.py` follows the checkout it lives in (worktree-correct) and exits **0**, not **3**, after `restore_zig_deps.sh`.
- [ ] `.port/tools/ci.sh`'s debug test lane runs with `--no-fail-fast`.
- [ ] `include/rustty.h` compiles standalone as C11 **and** C++17 with `-Wall -Wextra`, and gate A5 enforces it with a proven-reachable FAIL arm.
- [ ] No test binary in `crates/rustty-apprt/tests/` calls `gtk4::init()` more than once.
- [ ] `mapinit_styled` reports the page mapping fully initialized; the production Miri test that reproduced the uninit read passes.
- [ ] `grep -c 'Node::page(' crates/rustty-inspector/src/widgets/pagelist.rs` → **0** outside the guard.
- [ ] Every one of audit rows O1–O21 and N1–N10 is struck, reclassified into `# Justified R1 Divergences` with a 4-criterion basis, moved to `VERIFICATION_DEBT.md` with a runnable recipe, or routed to a named OPEN workstream **in that workstream's own plan or ROADMAP row**.
- [ ] Zero rows anywhere say "deferred to a later phase" without a named owner and trigger.
- [ ] Both platforms: `cargo build --workspace` + `cargo test --workspace --no-fail-fast` green; `.port/tools/ci.sh` green on both.
