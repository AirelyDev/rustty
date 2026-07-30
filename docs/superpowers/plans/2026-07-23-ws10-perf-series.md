# WS10 — Perf Series (BL-4) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Port the whole of upstream backlog series **BL-4** (`.port/phase-2/MAIN_PARITY.md` §2) — the stream/terminal/termio performance family: SIMD ESC/ASCII inlining, bulk CSI parse + direct final dispatch, printed-run batching, bulk APC slice dispatch, scroll-region optimizations, render-state lock-hold reduction, the two-stage pty read pipeline, once-per-value logging, and the four benchmark-corpus commits that instrument all of it — **and, before any of that, build a perf gate that can actually fail**, because today the repo's perf gate (G8) is both unarmed *and* silently comparing against the oracle's **Debug** numbers. WS10 also closes the one ledger row orphaned into BL-4 (`Screen::cursor_scroll_region_up`, `DEFERRALS.md:5011`/O3) and fixes a **correctness** divergence that happens to live in a perf file (`rustty-simd/src/vt.rs` scalar UTF-8 decode). The standing goal — "hold or extend the 14/14 wins" — is re-expressed as a falsifiable, same-session **paired** fence against the pinned oracle binary, with a demonstrated FAIL arm.

**Architecture:** WS10 is six sequential layers, and the order is load-bearing.

- **Layer 0 — measurement substrate (T1–T4).** Nothing downstream is verifiable until the perf gate is real. T1 re-baselines the 14-config suite; T2 fixes the G8 baseline-index bug that collapses ReleaseFast onto Debug rows; T3 lands the paired A/B harness `.port/tools/perf/ws10_suite.py`, arms G8, and proves the FAIL arm on a *real* pessimized binary; T4 puts `rustty-bench` into a gate for the first time (`B-compression.md` §4.5: "`rustty-bench` is in NO gate").
- **Layer 1 — correctness and independent smalls (T5–T9).** `cb4c49fbf` is a behavioural divergence, not a perf change, and it is currently **untestable in-repo** because the scalar decoder is dead code on every non-wasm target; it goes first, with the oracle's own differential fuzz test as the negative control. Then the four self-contained items that touch nothing else: tabstop realloc, once-per-value logging, macOS QoS on the read thread, renderer link no-alloc.
- **Layer 2 — bench corpus (T10–T12).** Instrumentation before optimization. `terminal-stream` currently measures a *print-only* handler over 4 KiB reads; every "terminal-stream improved" claim made against the current bench is measuring less work than upstream's. Swap the handler and the chunk size, add `HyperlinkMap`, add `--loops` to `ScreenClone`, **before** touching the hot paths those benches watch.
- **Layer 3 — the stream fast-path family (T13–T17).** `253e4f9c3` → `300f42c7a` → `1a88f3622` → `083d9709b` → `47e26df60`+`cb2d78587`+`f2a7652ab` → `f6f79acce`+`8c523ed03`+`ApcParser`. `D-terminal-subsystems.md` §6 is explicit: "Do not cherry-pick: the fast paths interlock." Each commit is one task-step with its own oracle tests and its own suite re-measure.
- **Layer 4 — terminal/render (T18–T19).** `77190bd02` scroll-region optimizations (which is where O3 lives) and `446f80f4e` render-state lock-hold reduction (the largest single commit in the series: +1348/−209 across 9 files including the vt C header).
- **Layer 5 — pty pipeline + closeout (T20–T22).** `2f0e6659d` + `bb0ac4c72` last, because it is the only HIGH-risk item: its documented invariant ("buffer contents need no locking, only ring metadata is guarded") is precisely a Stacked-Borrows violation in Rust and needs raw-pointer slot handoff plus a dedicated Miri harness. Then the final paired re-measure, the ledger sweep, and the ROADMAP row repair.

**Tech Stack:** Rust 1.93.1 (pinned, `rust-toolchain.toml`); `cargo` profiles `release-fast` (the ReleaseFast-parity profile, `overflow-checks = false`) for all timing and `dev` for tests; `rustty-bench` built with `rustty-vt/simd` (already declared in `crates/rustty-bench/Cargo.toml` as a `PERF(port)` fairness fix); the vendored `highway` C++ kernel in `crates-sys/highway-sys` for `crates/rustty-simd/cbits/simd/vt.cpp`; `cargo +nightly miri` for the pipeline and scroll-region harnesses; Python 3 for the perf harness and gates (`.port/tools/gates/_gatelib.py` conventions). Oracle: `/Users/gael/projects/ghostty`, working tree pinned at `4749c4e93`, `origin/main` at `6ad1fe7d8` — **all** of BL-4 is beyond the pin, read via `git -C ../ghostty show <hash>:<path>`. Host: Apple M4 Pro (`Mac16,7`), 14 logical cores, macOS 25.5.0.

## Global Constraints (inherited from `2026-07-23-rustty-completion-ROADMAP.md`)

- **R1 full fidelity — but WS10 is the first workstream whose ENTIRE scope is beyond the pin.** Every BL-4 commit postdates `4749c4e93`. The fidelity target for each hunk is therefore the *named upstream commit*, read with `git -C ../ghostty show <hash>`, not the pinned working tree. Do not advance `.port/ORACLE_SYNC.md`'s pin; record each ported commit in `.port/phase-2/MAIN_PARITY.md` §3 as the existing beyond-pin series (BL-2/3/5/6/7) did.
- **Oracle protocol: NEVER `git checkout` in `../ghostty`.** Four other lanes depend on its working tree staying at the pin. Read beyond-pin state with `git -C ../ghostty show`. If a whole tree is needed (T22's optional oracle rebuild), use `git -C ../ghostty worktree add`, never a checkout.
- R2 product identity: rustty naming with per-token casing preservation. R3: no build-time dependency on the oracle. R4/R5: acyclic crate graph; gates are invariant — never weakened to pass. R5: no macOS/Linux behavioural divergence introduced except where the oracle itself is `comptime`-gated (the `setQosClass` hunk is macOS-only in the oracle too).
- DG5: C-ABI panic policy is ABORT. T19 (`446f80f4e`) touches `include/rustty/vt/render.h` and the vt C render surface, so the **A4 ABI gate** (`check_embed_h_abi.sh`) and `check_rustty_vt_headers.sh` are in scope and must stay green.
- **LTO is banned** (standing decision). No task may propose enabling it, and no measured win may depend on it.
- **Do not add the `libc` crate anywhere it is not already a dependency.** `crates/rustty-termio/Cargo.toml:79` already declares `libc = "0.2"`, so T20 may use it there; new syscall surfaces elsewhere are hand-rolled `extern "C"` per the `mem.rs`/`page.rs` precedent.
- Zig byte-slices → `Vec<u8>`/`&[u8]`, never lossy `String`.
- `&mut Page`/`&mut Row`/`&mut [Cell]` arguments re-read across `assert_integrity` are Stacked-Borrows UB — use raw-pointer args (memory: `rust-mut-args-strongly-protected`). This binds T18 (scroll region) and T20 (pipeline) hardest.
- Rustty-side performance divergences from the oracle are marked with the established `// PERF(port):` comment convention (43 existing sites, e.g. `crates/rustty-vt/src/parser.rs:592`, `crates/rustty-vt/src/style.rs:120`). **WS10 must not silently delete an existing `PERF(port)` divergence to make an upstream hunk land** — see PREMISE CORRECTION §5.
- Ledger discipline: strike/repair `.port/phase-2/DEFERRALS.md`, `MAIN_PARITY.md`, and `VERIFICATION_DEBT.md` rows in the same PR (T22). No new deferrals: anything this hardware cannot verify is filed as **verification debt with a runnable recipe**, not deferred work.
- Commit style: `subsystem: summary (WS10 T<n>)`. Branch `ws10-perf-series`; merge to main only with the full gate set green *and* the paired oracle fence green.

---

## PREMISE CORRECTION (read before Task 1)

The ROADMAP row is one line — "SIMD scans, bulk CSI/print, pty pipelining. Guarded by
the benchmark suite (goal: hold or extend the 14/14 wins)" — and **every clause of it
needs qualification**. Six findings, each with evidence gathered at plan-authoring
time (2026-07-29).

### 1. The 14/14 baseline is comparable to the PINNED oracle only — and that is the only oracle this host can build

`.port/phase-3/assessments/E-os-termio-platform.md` row **E34** records that upstream
added `src/quirks_memset.zig` (338 lines, new in the pin→head range) which made the
oracle **~2.8× faster on `+terminal-stream`**, and warns that a future
rustty-vs-`origin/main` run "will show a smaller rustty margin … and that is the
oracle getting faster, **not** rustty regressing." That warning is correct and stands.
`quirks_memset` must **not** be ported: `rustc` lowers `@llvm.memset` to platform
libc `memset` (Apple `libSystem` on macOS, glibc IFUNC on Linux), both already
SIMD + `dc zva`; exporting a strong `memset` from a Rust dylib would pessimize
rustty and risk ODR/interposition breakage.

**But the phantom-regression scenario cannot occur on this host, for a harder reason:**

```
$ git -C ../ghostty show 4749c4e93:build.zig.zon | grep minimum_zig_version
    .minimum_zig_version = "0.15.2",
$ git -C ../ghostty show origin/main:build.zig.zon | grep minimum_zig_version
    .minimum_zig_version = "0.16.0",
$ zig version
0.15.2
```

The installed Zig cannot build `origin/main` at all. The only oracle bench binary in
existence here is the **pinned** one, on disk in the shared checkout and byte-stable
since 2026-06-21:

```
/Users/gael/projects/rustty/.port/phase-0/baseline/bench-releasefast/bin/ghostty-bench
  1 553 824 bytes, mtime 2026-06-21 15:09
  sha256 0fb47d268c7b9a7f9f93ed14834ca221383ba189404b69d15f750ff76abf5cdb
```

So: **all WS10 fences are against that binary**, and every number in this plan and in
`PERF_LOG.md` is comparable to it and to nothing else. The `origin/main` oracle
comparison is filed as verification debt (**V6**, T22) with a runnable recipe and its
*expected* outcome pre-stated, so that whoever runs it does not read a 2.8×-faster
oracle as a rustty regression.

Note this binary is **untracked** (`.gitignore`: "Reference-oracle BUILD outputs under
`baseline/` (regenerable; large)"), so it is absent from a fresh worktree. WS10 tasks
reference it by absolute path in the shared checkout. Same for the four corpora
(`ascii.bin`, `utf8.bin`, `osc-parser.bin` at 67 108 864 B each; `ascii-small.bin` at
262 144 B) — present in the shared checkout, absent from worktrees.

### 2. The 14/14 wins DO still hold on `main` — measured — but three rustty medians have drifted 6–20% slower than the record, and the oracle column has not drifted at all

Measured at plan-authoring time on this host with `rustty-bench` built from `main`
(`4069662`) at `--profile release-fast`, N=7 + 1 discarded warmup, both binaries run
back-to-back per config, single-process `time.monotonic()`, stdout/stderr to
`DEVNULL` — i.e. the `run_benchmarks.py` methodology:

| bench | zig ms | rust ms | Δ | recorded rust (`PERF_LOG.md`, r5-SHIP @ `a3a2b23`) | rust drift |
|---|--:|--:|--:|--:|--:|
| terminal-parser | 265.5 ‡ | 105.8 ‡ | **−60.2%** | 92.5 | **+14.4%** |
| terminal-stream | 553.9 ‡ | 363.0 ‡ | **−34.5%** | 343.3 | +5.7% |
| screen-clone.clone | 54.9 ‡ | 25.1 ‡ | **−54.3%** | 20.9 | **+20.1%** |
| screen-clone.render | 16.1 | 14.7 | −8.9% | 13.8 | +6.5% |
| grapheme-break.table | 256.3 | 218.4 | −14.8% | 211.4 | +3.3% |
| is-symbol.table | 240.4 | 201.4 | −16.2% | 200.5 | +0.4% |
| screen-clone.noop | 6.0 | 5.2 | −12.1% | 4.6 | +13.0% |
| osc-parser | 58.0 | 50.3 | −13.2% | 49.4 | +1.8% |
| codepoint-width.table | 233.1 | 209.1 | −10.3% | 207.9 | +0.6% |
| is-symbol.uucode | 228.3 | 207.2 | −9.2% | 206.1 | +0.5% |
| codepoint-width.wcwidth | 260.1 | 238.2 | −8.4% | 237.6 | +0.3% |
| grapheme-break.noop | 231.4 | 214.2 | −7.5% | 211.8 | +1.1% |
| codepoint-width.simd | 502.0 | 460.5 | −8.3% | 466.4 | −1.3% |
| codepoint-width.noop | 4.0 | 3.7 | −8.7% | 3.0 | +23% (3.7 ms — floor noise) |

‡ = second-pass figure at N=9; the first pass had zig-side `max/min` spreads of 1.224
(terminal-parser) and 1.540 (terminal-stream) under contention and is discarded for
those two.

**Suite (first pass, N=7): zig 3307.6 ms vs rust 2308.3 ms, −30.2%, wins 14/14.**

**Measured-vs-provisional status — read before quoting any of the above.** These
numbers were taken on a **busy** box: four other agent lanes were running and
`uptime` reported 1-minute load averages of **7.1 → 15.3** across the session, with
four `yes` processes each pinned at ~99% CPU. They are therefore **PROVISIONAL**.
What makes them still worth recording:

- The **oracle column reproduces the record almost exactly** — 10 of 14 configs within
  ±2% of `PERF_LOG.md`'s zig column (cw.table 233.1 vs 234.3; cw.simd 502.0 vs 501.1;
  gb.noop 231.4 vs 231.0; gb.table 256.3 vs 256.4; is.table 240.4 vs 238.6;
  is.uucode 228.3 vs 229.9; osc 58.0 vs 57.1; t-parser 265.5 vs 263.8; t-stream 553.9
  vs 551.5; sc.render 16.1 vs 15.4). The 14-core M4 Pro absorbs four single-threaded
  burners without disturbing a single-threaded benchmark much.
- Per-config intra-session `max/min` spread on the ten large-corpus configs was
  **≤1.05** even at load 15 (e.g. cw.simd 1.008, cw.wcwidth 1.010, gb.noop 1.014).
- **The direction is unambiguous: 14/14 wins, with the thinnest margin at −7.5%**,
  which is ~5× that config's measured spread.

What is *not* safe to conclude: the exact suite delta (−30.2% here vs −21.7% recorded)
— it is inflated because the two long benches' zig medians were contention-blown in
the pass that produced the suite total. **T1 re-measures on a quiet box and records the
authoritative figure.**

The **asymmetry** is the real finding and T1 must resolve it: the oracle column
reproduces to ≤2% while three rustty configs are 6–20% above the recorded r5-SHIP
figures with tight spreads (t-parser rspread 1.068, sc.clone 1.215). That pattern is
consistent with genuine rustty-side drift since `a3a2b23` — WS5, WS7 (BL-1
compression, which touched `Page`/`PageList`/`Screen` hard), and WS8 all landed after
the record was taken. **T1 owns confirming or refuting this and, if confirmed,
bisecting it.** It is not a WS10 port regression; it is a pre-existing condition WS10
must establish before it can attribute anything to its own changes.

### 3. G8, the repo's only perf gate, is vacuous TWICE OVER — and one of the two failures survives arming it

`.port/tools/ci.sh:169` runs `check_baseline_regression.py` with **no `--metrics`**, so
it is permanently in "progress mode" and returns GREEN by construction. That much is
known (it is spelled `PROGRESS` in the gate table). The second failure is not known
and is worse:

`BASELINE.json` carries **28** benchmark rows — the same 14 ids twice, once
`"optimize": "ReleaseFast"` and once `"optimize": "Debug"` — and
`check_baseline_regression.py`'s `bench_index()` keys **on `id` alone**, so the Debug
rows (which come second in the array) silently overwrite the ReleaseFast ones.
Demonstrated:

```
$ python3 -c '<import bench_index from the gate>; print(bench_index(BASELINE))'
indexed ids: 14
terminal-stream/ascii     -> ('throughput_MiB_per_s',   2.073, 'higher')   # Debug!
terminal-parser/utf8      -> ('throughput_MiB_per_s', 258.156, 'higher')   # Debug!
screen-clone.clone/small  -> ('median_s',              5.4863, 'lower')    # Debug!
```

The ReleaseFast values are 118.8 MiB/s, 249.8 MiB/s and 0.0493 s respectively. So if
G8 were armed today it would let `terminal-stream` pass at **1/57th** of the real
oracle throughput and `screen-clone.clone` pass at **111× the real oracle's time**.
Arming the gate without fixing the index would produce a *green gate that cannot
fail*. **T2 fixes this and adds a self-test arm that pins the ReleaseFast row.**

Third, softer, issue: `BASELINE.json` is a **frozen June measurement**. Across four
independent sessions the *same unchanged* oracle binary measured terminal-parser at
256.2 (BASELINE.json) / 286.4 (`PERF_COMPARISON.md`) / 263.8 (`PERF_LOG.md` r5) /
265.5 (this probe) ms — an 11.8% max/min span. A frozen-JSON comparison therefore
cannot be tightened below ~12% without producing false REDs. That is why the WS10
fence is **same-session paired** (re-measure the oracle binary alongside the candidate
in the same process) rather than JSON-vs-JSON. See "Non-regression gate design".

### 4. `cb4c49fbf` is a correctness bug, and it is currently unreachable by any test

`D-terminal-subsystems.md` correctly flags `crates/rustty-simd/src/vt.rs` as diverging
from upstream: the truncated-multi-byte branch returns `DecodeResult { consumed:
decode_offset, decoded: decode_count }` unconditionally, where upstream `cb4c49fbf`
only does so when `decode.len() == input.len()` and otherwise `break`s so the prefix
becomes one maximal subpart (one U+FFFD, consumed through the ESC). What the assessment
does **not** say is that **the divergent function is dead code on every non-wasm
target**:

```rust
// crates/rustty-simd/src/vt.rs:31
const SIMD: bool = true;
// :34-49
#[cfg(not(target_arch = "wasm32"))]
if SIMD { /* … FFI to rustty_simd_decode_utf8_until_control_seq … */ return …; }
utf8_decode_until_control_seq_scalar(input, output)
```

`grep -rn utf8_decode_until_control_seq_scalar crates/` → exactly **two** hits: the
definition and that one unreachable call. All 19 existing tests in that module
(`grep -c '#\[test\]' crates/rustty-simd/src/vt.rs` → 19) call the public
`utf8DecodeUntilControlSeq`, i.e. they all exercise the **highway C++** path. So the scalar decoder — the reference implementation the SIMD kernel is supposed
to match — has **zero test coverage on macOS/Linux**, and its divergence is live only
in `wasm32` builds (WS14's target). This makes upstream's own new test
`test "decode simd matches scalar"` (a 10 000-iteration differential fuzz) both the
fix's negative control *and* the first coverage that path has ever had. T5.

### 5. Rustty already carries its own perf divergences in exactly the files BL-4 rewrites — porting upstream's fast paths is a MERGE, not an overlay

`PERF_LOG.md`'s rounds 1–5 landed six accepted rustty-side optimizations, four of them
in BL-4's blast radius:

| rustty optimization | site | BL-4 commit that rewrites the same code |
|---|---|---|
| r5-parser-fastpath (state-equal fast path) | `crates/rustty-vt/src/parser.rs:592` `// PERF(port): state-equal fast path` | `300f42c7a` `csiEntryByte`, `1a88f3622` `csiDispatchFinal` |
| r5-print-outline (cold grapheme/zero-width/wide arms outlined) | `crates/rustty-vt/src/terminal.rs:884` | `47e26df60` `printSlice` family (+675 lines in `Terminal.zig`) |
| r5-clone-style-run (same-style run batching, by-pointer set adds) | `crates/rustty-vt/src/page_impl.rs:1423,1441,1467,1622`, `ref_counted_set.rs:528,652,723` | `446f80f4e` (`page.zig` +274, `render.zig` +919) |
| r5-stream-simd (bench binary built with `simd`) | `crates/rustty-bench/Cargo.toml:45` | `4c5b1d5d5`/`258de36d1` bench changes |

Each of those is worth −6% to −65% *today*. The naive port — paste the upstream hunk,
delete whatever conflicts — can silently drop a rustty win and still look like a
faithful port. **Rule for every Layer-3/4 task: an existing `PERF(port)` marker in a
hunk you are rewriting is a stop-and-decide point.** Either the upstream structure
subsumes it (prove with a measurement), or it must be preserved *inside* the ported
structure with its `PERF(port)` note updated to say which upstream commit it now lives
under. The self fence (below) is what makes "silently dropped" detectable.

### 6. Four smaller factual corrections to the assessments this plan builds on

- **`300b42f42a` is not a commit.** `D-terminal-subsystems.md` rows 135 and 506 cite
  `1a88f3622 + 300b42f42a`. `git -C ../ghostty rev-parse 300b42f42a` → *unknown
  revision*. The real commit is **`300f42c7a`** "terminal: handle CSI entry bytes
  inline in consumeUntilGround" (2026-07-06, 1 file, +63/−34), which is what
  `MAIN_PARITY.md:161` names. Use `300f42c7a` everywhere.
- **`258de36d1` is NOT blocked on area A.** `B-compression.md` §3.4 says the
  `TerminalStream` bench swap needs "`terminal.TerminalStream` (the readonly full
  handler)" from area A. It already exists:
  `crates/rustty-vt/src/stream_terminal.rs:66` `pub type Stream<'a> =
  stream::Stream<'a, Handler<'a>>` with all dispatch arms filled (the file's
  `PORT STATUS: stub` header line is stale — the PORT NOTEs immediately below it
  record the arms landing). T10 is unblocked today.
- **`7e14347c1` is not just a benchmark commit.** `B-compression.md` presents it as
  "`HyperlinkMap.zig` (+155, NEW) + `cli.zig` + `main.zig` registration". The commit
  is 7 files / +631/−41 and its *substance* is `src/terminal/hash_map.zig` **+472**
  ("terminal: bound page map probe lengths") plus `page.zig` +37, `Screen.zig` +3,
  `hyperlink.zig` +2. The benchmark exists to measure that change. Porting the
  benchmark without the probe bounding measures nothing. T11 ports both.
- **`f2a7652ab` "mitchell's touchups" is a 35-file commit**, not a bench commit.
  Only six of its files are in WS10's scope (`benchmark/TerminalStream.zig` +6,
  `benchmark/cli.zig` +2/−1, `simd/main.zig` +12 — the `pub fn lanes(T)` helper,
  `terminal/Terminal.zig` +155/−…, `terminal/hash_map.zig` +18, `terminal/stream.zig`
  +24). Port those hunks only; the other 29 files belong to other areas and must not
  be dragged in.

### 7. O3 — the orphaned ledger row — is WS10's, and it is portable now

`DEFERRALS.md:5011` files WS7 Phase C divergence #11 (`Screen::cursor_scroll_region_up`
unported, and therefore `0bad91adb`'s 4th `invalidateNodeLayout` site and the test
`Screen: cursorScrollRegionUp renews page generation` have nothing to land on) to "the
**WS8** upstream-sweep backlog (**BL-4**)". Both halves are wrong: WS8 is merged and
closed, and BL-4 is the perf series — **WS10**. `WS19`'s closeout audit (row O3) routed
it here and rewrote the disposition; WS19 explicitly did **not** port it. WS10 owns it.

Verified state: `grep -rn 'cursor_scroll_region_up\|cursorScrollRegionUp' crates/` →
**zero hits**. The three *other* Screen `invalidate_node_layout` sites from `0bad91adb`
ARE ported (`crates/rustty-vt/src/screen.rs:1536,1618,1651`) and so is the Terminal one
(`crates/rustty-vt/src/terminal.rs:3004`) — 15 sites total repo-wide. The single
missing site is the one inside the unported function. WS19 noted the port is "cheaper
AFTER Phase 3 advances the pin"; Phase 3 has produced its assessments but has **not**
advanced the pin, and the user's instruction is that Phase 2 closes fully with no
further deferrals. So WS10 ports `77190bd02` from `origin/main` the same way WS5/WS7/WS8
ported their beyond-pin series. T18.

---

## Measurement environment (the plan is written against these)

- **Host:** Apple M4 Pro (`Mac16,7`), arm64, 14 logical cores, macOS 25.5.0.
- **`hyperfine` is NOT installed** (confirmed during WS7 D8 and unchanged). All timing
  uses the in-repo single-process harness, which is what every recorded number in
  `PERF_COMPARISON.md` / `PERF_LOG.md` used, so the methodology is continuous.
- **Oracle bench binary:** pinned, on disk, untracked — see PREMISE CORRECTION §1 for
  path and sha256. **Corpora:** four files in
  `/Users/gael/projects/rustty/.port/phase-0/baseline/corpora/`, also untracked.
  Both live in the **shared checkout**, not in worktrees.
- **Build cost:** `cargo build -p rustty-bench --profile release-fast` completed in
  **12.5 s** in a fresh worktree at plan-authoring time (warm cargo cache), so
  re-measuring after each task is cheap and there is no excuse for skipping it.
- **The box is shared.** Four other agent lanes were live during authoring (load 7–15).
  Every timing task in this plan therefore begins with an **idle gate** (see T1 Step 1)
  and refuses to record numbers taken above the threshold.

---

## Non-regression gate design

The ROADMAP says "guarded by the benchmark suite". Today that guard is
`check_baseline_regression.py` in progress mode over a Debug-collapsed index — it
cannot fail. WS10 replaces it with **two same-session paired fences plus three
demonstrated FAIL arms**.

### Fence 1 — ORACLE FENCE (the "hold the 14/14 wins" fence)

`.port/tools/perf/ws10_suite.py` runs all 14 configs against **both** binaries
alternately within one process (`zig` block then `rust` block per config, 1 discarded
warmup each, N=9), takes medians, and asserts per config:

| arm | configs | rule | basis |
|---|---|---|---|
| **STRICT** | the 10 large-corpus configs (`terminal-parser`, `terminal-stream`, `codepoint-width.{table,simd,wcwidth}`, `grapheme-break.{noop,table}`, `is-symbol.{table,uucode}`, `osc-parser`) | `rust_median < zig_median`, 0% tolerance | measured intra-session `max/min` spread ≤1.05 on all ten; the thinnest measured margin is grapheme-break.noop at −7.5%, ~5× the spread |
| **BANDED** | the 4 short/small-corpus configs (`codepoint-width.noop` 3.7 ms, `screen-clone.{noop,clone,render}` 5.2/25.1/14.7 ms) | `rust_median < zig_median * 1.15` | these are process-spawn-floor dominated (`PERF_COMPARISON.md` §"Process-spawn / setup-floor caveat"); measured spreads 1.53 / 1.20 / 1.22 / 1.13 |

**Two-strike rule:** a config that fails is automatically re-run once at N=15; only a
second failure is RED. Recorded in the output either way, so a flake is visible.

**Idle gate:** the harness reads `os.getloadavg()[0]` before starting and **refuses to
emit an authoritative metrics file** above the threshold recorded in T1 (it still runs,
but stamps `"provisional": true` and exits 2). This is what stops a contended run
becoming a baseline — the exact mistake this plan's own §2 numbers are labelled with.

### Fence 2 — SELF FENCE (no rustty-side self-regression across WS10's own tasks)

The failure mode Fence 1 misses: a task that makes rustty 20% slower on a config where
it was winning by 60% still passes Fence 1. So T1 stashes the **entry binary** —
`.perf-bins/rustty-bench-ws10-entry-RF` (untracked, sha256 recorded in `PERF_LOG.md`) —
and each subsequent perf task runs `ws10_suite.py --self <entry-binary>`, which
measures entry and candidate **in the same session, alternately**, and requires:

- `candidate_median ≤ entry_median * 1.05` for the 10 large-corpus configs;
- `candidate_median ≤ entry_median * 1.15` for the 4 short ones.

**Why +5% is defensible here and would not be against a frozen JSON:** the same
unchanged oracle binary spans 11.8% max/min across four historical sessions
(PREMISE CORRECTION §3), so any cross-session frozen comparison needs ≥12%. Re-measuring
the *stored binary* in the same session removes session variance entirely, leaving only
the ≤1.05 intra-session spread — so +5% is ~5× the residual noise. This is the whole
reason the fence stores a binary rather than a JSON.

An intentional pessimization that is a *net* win (e.g. upstream's `printSlice` trades a
slower single-`print` path for a much faster bulk path) is not silently allowed: the
task must add its config to `ws10_suite.py`'s `ACCEPTED_TRADEOFFS` table with the
measured before/after and a one-line justification, and that table is printed in the
gate output. No silent waivers.

### FAIL arms — three, all demonstrated, not asserted

1. **`check_baseline_regression.py --self-test`** already ships a 5-arm flip proof and
   prints `SELF-TEST: PASS`; arm `[c]` (throughput −50%) must exit non-zero. T2 adds a
   **sixth** arm: assert `bench_index()` on the real `BASELINE.json` returns
   `terminal-stream/ascii` at the **ReleaseFast** 118.8 MiB/s, not the Debug 2.073 —
   the arm that would have caught the bug in §3. This arm must FAIL on the pre-fix
   code (verify by running it before applying the fix).
2. **`ws10_suite.py --self-test`** (new, T3): three synthetic metric pairs —
   (a) identical → exit 0; (b) `grapheme-break.noop` rust median ×1.10 → exit 1 with
   `LOST bench grapheme-break.noop/utf8` (Fence 1 STRICT arm); (c) candidate vs entry
   with `codepoint-width.table` ×1.08 → exit 1 with
   `SELF-REGRESS bench codepoint-width.table/utf8 (+8.0%)` (Fence 2). Prints
   `SELF-TEST: PASS` only if all three arms behave.
3. **A LIVE flip proof on a real binary** (T3, run once and recorded). `PERF_LOG.md`
   records that building `rustty-bench` **without** the `rustty-vt/simd` feature is a
   measured ~24% terminal-stream pessimization (that was the r5 "FAIRNESS FIX"). So:
   temporarily drop `features = ["simd"]` from `crates/rustty-bench/Cargo.toml:45`,
   rebuild, run the armed gate, and confirm it goes **RED on
   `terminal-stream/ascii`** with roughly the expected magnitude; then revert. This is
   the arm that proves the gate catches a real regression of known size on a real
   binary — not a doctored JSON. Its output is pasted into `PERF_LOG.md`.

### What the fences do NOT prove (stated, and filed as debt in T22)

- Nothing about the `origin/main` oracle (no Zig 0.16 here) → **V6**.
- Nothing about Linux. The oracle bench binary is a macOS arm64 build and the corpora
  paths are macOS. The two Linux-relevant items in scope — `2f0e6659d`'s POSIX pipeline
  and `896aca499`'s `MADV_DONTNEED` twin — are unmeasured on Linux → **V7**.
- Nothing about real interactive latency. The suite is throughput over static corpora;
  `2f0e6659d`'s stated motivation is a *latency* fix (macOS tty output queue capping
  master reads at ~1 KiB). T20 adds its own latency assertion rather than leaning on
  the suite.
- Nothing about RSS. `BASELINE.json`'s `metrics_unmeasured_TODO` already records that
  peak RSS per benchmark was never measured. WS10 does not add it (out of scope) but
  T22 must not claim a memory fence exists.

---

## Files to create / modify

**New:**
- `.port/tools/perf/ws10_suite.py` — the paired A/B harness + both fences + `--self-test` + metrics emitter (T3).
- `.port/tools/perf/README.md` — how to run a fence, the idle gate, the two-strike rule, where the untracked oracle binary and corpora live (T3).
- `.port/tools/gates/check_perf_fence.sh` — the ci.sh-facing wrapper (gate id **G8b**) (T3).
- `crates/rustty-bench/src/hyperlink_map.rs` — port of `benchmark/HyperlinkMap.zig` (T11).
- `crates/rustty-bench/src/apc_parser.rs` — port of `benchmark/ApcParser.zig` (T17).
- `crates/rustty-synthetic/src/kitty.rs` + `crates/rustty-synthetic/src/cli/kitty.rs` — port of `synthetic/Kitty.zig` + `synthetic/cli/Kitty.zig`, the generator `ApcParser` needs (T17).
- `.port/phase-2/mirihar-ws10-pipeline/` — isolated non-FFI Miri harness for the pty ring's raw-pointer slot handoff (T20).
- `.port/phase-2/mirihar-ws10-scrollregion/` — Miri harness for `cursor_scroll_region_up`'s raw-pointer row shifts (T18).

**Modified (grouped by task layer):**
- `.port/tools/gates/check_baseline_regression.py` — `(id, optimize)` keying, ReleaseFast preference, 6th self-test arm (T2).
- `.port/tools/ci.sh` — arm G8 with `--metrics`; register G8b; add `cargo test -p rustty-bench --lib` (T3, T4).
- `crates/rustty-simd/src/vt.rs` — the `cb4c49fbf` guard + `break`; the two new tests; the differential fuzz test (T5).
- `crates/rustty-simd/cbits/simd/vt.cpp` — `083d9709b` `WidenAsciiStore` + `DecodeNonAsciiUntilControlSeq`, incl. the `HWY_SCALAR` arm (T15).
- `crates/rustty-simd/src/lib.rs` — `pub fn lanes<T>()` (the `f2a7652ab` `simd/main.zig` hunk) (T16).
- `crates/rustty-vt/src/tabstops.rs` — `5caf20e3e` `<` → `<=` (T6).
- `crates/rustty-vt/src/stream.rs` — `b5053153f` `log_unsupported_once` (T7); `253e4f9c3` `consume_csi_params` (T13); `300f42c7a` `csi_entry_byte` (T14); `1a88f3622` `csi_dispatch_final` (T14); `47e26df60` `Action::PrintSlice` + the lane-wise printable-run scan in `next_slice` (T16); `f6f79acce` `Action::ApcPutSlice` + `consume_apc_string` (T17); `8c523ed03` its vectorization (T17).
- `crates/rustty-vt/src/stream_terminal.rs` — the `.print_slice` and `.apc_put_slice` dispatch arms (T16, T17); stale `PORT STATUS: stub` header line (T10).
- `crates/rustty-vt/src/terminal.rs` — `47e26df60` `print_slice`/`print_slice_fast`/`print_slice_eligible`/`print_slice_fill`/`print_slice_check_expected`/`field_mask` + `cb2d78587`'s bulk style-only fill + `f2a7652ab`'s +155 (T16); `77190bd02`'s Terminal-side hunks + 4 tests (T18); `446f80f4e`'s Terminal hunks (T19).
- `crates/rustty-vt/src/screen.rs` — `77190bd02` `cursor_scroll_region_up` + `cursor_scroll_region_up_slow` + 5 tests; `0bad91adb`'s 4th `invalidate_node_layout` site + `screen_cursor_scroll_region_up_renews_page_generation` (T18).
- `crates/rustty-vt/src/hash_map.rs`, `page.rs`/`page_impl.rs`, `hyperlink.rs` — `7e14347c1` probe bounding (T11).
- `crates/rustty-vt/src/render.rs`, `page_impl.rs` — `446f80f4e` render-state rewrite (T19).
- `crates/rustty-vt/src/apc/mod.rs` + `apc/glyph/request.rs` + `kitty/graphics_command.rs` — `f6f79acce` `feed_slice` (T17).
- `include/rustty/vt/render.h` (exists, 729 lines), `crates/rustty-vt-c/src/render.rs` + `crates/rustty-vt-c/src/render/` (exist), `crates/rustty-vt-c/src/lib.rs:56` (`pub mod render`) — `446f80f4e`'s C render surface additions (T19); A4 + `check_rustty_vt_headers.sh` re-freeze.
- `.gitignore` — add `/.perf-bins/` (T1; it is absent today).
- `crates/rustty-renderer/src/generic.rs`, `crates/rustty-renderer/src/link.rs` — `446f80f4e` renderer hunk (T19); `9c9cf3e82` no-alloc-when-no-links (T9).
- `crates/rustty-termio/src/exec.rs` — `E4` `setQosClass` (T8); `2f0e6659d` + `bb0ac4c72` pipeline (T20).
- `crates/rustty-termio/src/stream_handler.rs` — the `.print_slice` / `.apc_put_slice` arms (T16, T17).
- `crates/rustty-bench/src/{cli.rs,lib.rs,terminal_stream.rs,screen_clone.rs}` — action registration, the `258de36d1`+`4c5b1d5d5` handler/chunk swap, `446f80f4e`'s `--loops` + 3 render modes (T10, T11, T12, T17).
- `crates/rustty-synthetic/src/{cli.rs,main_gen.rs}` — Kitty generator registration (T17).
- `.port/phase-2/PERF_LOG.md` — the WS10 section: re-baseline, per-task measurements, negative results, the live flip proof (every task).
- `.port/phase-2/MAIN_PARITY.md` — strike each ported BL-4 row; rewrite the BL-4 §2 bullet on close (T22).
- `.port/phase-2/DEFERRALS.md` — strike `D:4981`/`:5011` and the O3 row (T18/T22).
- `.port/phase-2/VERIFICATION_DEBT.md` — new rows **V6** (origin/main oracle unbuildable) and **V7** (Linux perf unmeasured) (T22).
- `.port/phase-2/MIRI_EXCEPTIONS.md` — the pipeline's `poll`/`read` arms (T20).
- `docs/superpowers/plans/2026-07-23-rustty-completion-ROADMAP.md` — correct the WS10 row (T22).

---

### Task 1: Branch, idle-gate the box, re-baseline the suite, and resolve the drift question

**Files:**
- Doc: `.port/phase-2/PERF_LOG.md` (new "# WS10 — re-baseline" section)

**Interfaces:**
- Consumes: the untracked pinned oracle binary + corpora in the shared checkout.
- Produces: (a) branch `ws10-perf-series`; (b) the authoritative idle 14-config paired
  table; (c) `.perf-bins/rustty-bench-ws10-entry-RF` + its sha256 (Fence 2's anchor);
  (d) a verdict on the §2 drift.

- [ ] **Step 1: Branch, and establish the idle gate**

```bash
cd /Users/gael/projects/rustty
git checkout main && git pull && git log --oneline -1
git checkout -b ws10-perf-series
uptime
ps -Ao pcpu,comm -r | head -8
ls /Users/gael/projects/rustty/.claude/worktrees/
```

Record the 1-minute load average and the worktree count. **Threshold for an
authoritative timing run: 1-minute load average ≤ 2.0 and no other agent worktree
actively building.** If the box is busier, do the non-timing steps and come back;
do NOT record contended numbers as a baseline (this plan's PREMISE CORRECTION §2 table
is exactly what that mistake produces, and it is labelled provisional for that reason).

- [ ] **Step 2: Verify the measurement substrate still exists and is unchanged**

```bash
shasum -a 256 /Users/gael/projects/rustty/.port/phase-0/baseline/bench-releasefast/bin/ghostty-bench
ls -l /Users/gael/projects/rustty/.port/phase-0/baseline/corpora/
```

Expected: sha256 `0fb47d268c7b9a7f9f93ed14834ca221383ba189404b69d15f750ff76abf5cdb`;
four corpora at 67108864 / 67108864 / 67108864 / 262144 bytes. If the sha differs,
STOP — the oracle binary was rebuilt and every historical number is incomparable; that
is a report-and-stop condition, not something to work around.

- [ ] **Step 3: Build the entry binary and stash it**

```bash
cd /Users/gael/projects/rustty
cargo build -p rustty-bench --profile release-fast
mkdir -p .perf-bins
cp target/release-fast/rustty-bench .perf-bins/rustty-bench-ws10-entry-RF
shasum -a 256 .perf-bins/rustty-bench-ws10-entry-RF
```

`.perf-bins/` is **not** currently gitignored — verified at plan-authoring time:
`git check-ignore -v .perf-bins` returns nothing and `grep -n perf-bins .gitignore` has
no match, even though `PERF_LOG.md` already refers to
`.perf-bins/rustty-bench-r5-clone-style-run-RF` as an "untracked stash". **Add
`/.perf-bins/` to `.gitignore` in this step** (a 1.5 MB binary must never be committed),
and confirm with `git check-ignore -v .perf-bins/rustty-bench-ws10-entry-RF`.

- [ ] **Step 4: Run the 14-config paired suite, N=9, idle**

Until T3 lands the real harness, use the phase-0 harness twice and pair by hand:

```bash
cd /Users/gael/projects/rustty
python3 .port/phase-0/baseline/run_benchmarks.py \
  .port/phase-0/baseline/bench-releasefast/bin/ghostty-bench /tmp/ws10-zig.jsonl 9
python3 .port/phase-0/baseline/run_benchmarks.py \
  target/release-fast/rustty-bench /tmp/ws10-rust.jsonl 9
uptime
```

Expected: 14 records in each file, all `max_s/min_s` ≤ 1.06 for the ten large-corpus
ids. Any id above 1.06 is re-run before its number is recorded.

- [ ] **Step 5: Compare against BOTH historical records and decide the drift question**

Build the comparison table with three columns per config: this run's zig median, this
run's rust median, and `PERF_LOG.md`'s r5-SHIP rust median. Then answer, in writing, in
`PERF_LOG.md`:

1. **Does 14/14 still hold?** (Provisional answer from authoring: yes, thinnest margin
   −7.5% on `grapheme-break.noop`.) If any config has LOST, that is the highest-priority
   finding in WS10 and it is investigated before any port work.
2. **Does the oracle column reproduce?** Expected within ±2% on ≥10 of 14 (authoring
   measured exactly that). If it does not, the host has changed and the historical
   record is void — say so.
3. **Is the rustty drift real?** Authoring measured terminal-parser **+14.4%**,
   screen-clone.clone **+20.1%**, screen-clone.noop +13.0%, terminal-stream +5.7% vs
   the r5-SHIP record, with the oracle column flat. On an idle box, if any config is
   still ≥+8% with `max/min` ≤ 1.06, **bisect it**: the candidate commits are the
   merges that landed after `a3a2b23` — WS5 (`74226dd` lib-vt C-API), WS7 Phase A–E
   (BL-1 compression: `Page`/`PageList`/`Screen` are all in `screen-clone`'s and
   `terminal-parser`'s hot paths), WS8, WS19. Build `rustty-bench` at each merge point
   and time only the drifted configs (12 s per build; ~6 builds).

- [ ] **Step 6: Record**

Append to `.port/phase-2/PERF_LOG.md`:

```markdown
# WS10 T1 — re-baseline (2026-07-__)

Machine, load average at run, oracle binary sha256, entry binary sha256,
harness + N, the 14-config paired table, the three answers from Step 5, and an
explicit statement that every number in this section is comparable to oracle
`4749c4e93` / binary sha256 0fb47d26… and to NO other oracle revision
(see the WS10 plan's PREMISE CORRECTION §1: `origin/main` needs Zig 0.16.0,
this host has 0.15.2).
```

If Step 5 found real drift, `.port/phase-2/PERF_LOG.md` gets a named sub-section with
the bisect result, and — if the drift is a fixable rustty regression — it becomes
**T1b**, done before Layer 1, because Fence 2's anchor must not be a degraded binary.

- [ ] **Step 7: Commit**

```bash
git add .port/phase-2/PERF_LOG.md .gitignore
git commit -m "perf: WS10 re-baseline — 14-config paired suite vs pinned oracle (WS10 T1)"
```

---

### Task 2: Fix the G8 baseline-index bug (ReleaseFast rows silently overwritten by Debug)

**Files:**
- Modify: `.port/tools/gates/check_baseline_regression.py`

**Interfaces:**
- Consumes: `.port/phase-0/baseline/BASELINE.json` (28 rows, 14 ids × 2 optimize modes).
- Produces: a `bench_index` keyed on `(id, optimize)` with a documented ReleaseFast
  preference, plus a self-test arm that pins it.

- [ ] **Step 1: Reproduce the bug (negative control FIRST)**

```bash
cd /Users/gael/projects/rustty
python3 - <<'PY'
import importlib.util, json
spec = importlib.util.spec_from_file_location("g8", ".port/tools/gates/check_baseline_regression.py")
m = importlib.util.module_from_spec(spec); spec.loader.exec_module(m)
idx = m.bench_index(json.load(open(".port/phase-0/baseline/BASELINE.json")))
print("indexed ids:", len(idx))
for k in ("terminal-stream/ascii", "terminal-parser/utf8", "screen-clone.clone/small"):
    print(k, "->", idx[k])
PY
```

Expected (the bug): `indexed ids: 14`, `terminal-stream/ascii -> (…, 2.0730996…,
'higher')`, `screen-clone.clone/small -> ('median_s', 5.4863…, 'lower')` — the **Debug**
values. The ReleaseFast truths are 118.8 MiB/s and 0.0493 s. Paste this output into the
commit message; it is the evidence the fix is not cosmetic.

- [ ] **Step 2: Write the failing self-test arm**

Add arm `[f]` to `--self-test`, before touching `bench_index`:

```python
# [f] BASELINE.json's ReleaseFast rows must win over the Debug rows of the same id.
b = load_baseline_file()
idx = bench_index(b)
ts = idx[("terminal-stream/ascii", "ReleaseFast")]
ok_f = abs(ts[1] - 118.8) / 118.8 < 0.01
print("[f] ReleaseFast row preferred -> %s (expect True)" % ok_f)
```

Run `python3 .port/tools/gates/check_baseline_regression.py --self-test`.
**Expected: `SELF-TEST: FAIL` on arm `[f]`.** If it passes before the fix, the arm is
wrong — rewrite it until it fails.

- [ ] **Step 3: Fix `bench_index`**

Key on `(id, optimize)` where `optimize` defaults to `"ReleaseFast"` when the field is
absent (candidate metrics files, which only ever measure the release-fast binary, must
still match). Keep the same `(metric_key, value, direction)` value shape. Add a
docstring paragraph stating the June-2026 bug and why the key is a tuple. `--strict`
must compare only the `ReleaseFast` half against a candidate and must **not** report
the 14 Debug rows as "missing-from-candidate".

- [ ] **Step 4: Prove the flip**

```bash
python3 .port/tools/gates/check_baseline_regression.py --self-test
```

Expected: `SELF-TEST: PASS` with all six arms, including `[c] throughput -50% -> exit 1`
(the pre-existing FAIL arm must still fail) and `[f] ReleaseFast row preferred -> True`.
Re-run Step 1's snippet: `indexed ids: 28`, and the three probed ids now report
118.8 / 249.8 MiB/s and 0.0493 s.

- [ ] **Step 5: Commit**

```bash
git add .port/tools/gates/check_baseline_regression.py
git commit -m "gate(G8): key baseline benchmarks by (id, optimize) — Debug rows were silently overwriting ReleaseFast (WS10 T2)"
```

---

### Task 3: Land the paired A/B harness, arm G8/G8b, and prove the FAIL arm on a real binary

**Files:**
- Create: `.port/tools/perf/ws10_suite.py`, `.port/tools/perf/README.md`, `.port/tools/gates/check_perf_fence.sh`
- Modify: `.port/tools/ci.sh`, `.port/phase-2/PERF_LOG.md`

**Interfaces:**
- Consumes: the oracle binary, the four corpora, a candidate `rustty-bench`, and
  (for Fence 2) an entry binary.
- Produces: gate **G8b** (`check_perf_fence.sh`), an emitted METRICS JSON in
  `BASELINE.json`'s shape that arms **G8**, and a recorded live flip proof.

- [ ] **Step 1: Write `ws10_suite.py`**

Required surface (no more):

```
ws10_suite.py --rust PATH [--oracle PATH] [--self PATH] [-n N]
              [--emit-metrics OUT.json] [--json OUT.json] [--self-test]
```

- Same 14 configs, same ids, same corpora, same argv shape as
  `.port/phase-0/baseline/run_benchmarks.py` (ids MUST match `BASELINE.json`'s:
  `terminal-parser/utf8`, `terminal-stream/ascii`, `codepoint-width.noop/utf8`, …,
  `screen-clone.render/small` — G8 joins on them).
- Per config: warmup+N reps of binary A, then warmup+N reps of binary B, **alternating
  which binary goes first between configs** so a monotonic thermal trend cannot bias
  one side systematically.
- `STRICT_IDS` (10) and `BANDED_IDS` (4, band 1.15) exactly as tabled in
  "Non-regression gate design". Constants named, with the measured spread that justifies
  each in a comment.
- Two-strike: any failing config re-runs once at `N*2` before being reported RED.
- Idle gate: `os.getloadavg()[0] > IDLE_MAX` (2.0) ⇒ stamp `"provisional": true`, print
  a loud warning, exit 2 (not 0, not 1 — a distinct "unusable measurement" code).
- `--emit-metrics` writes `{"benchmarks":[{id, optimize:"ReleaseFast", median_s,
  throughput_MiB_per_s | clones_per_s, metric, n_reps, reps_s}, …]}` so
  `check_baseline_regression.py --metrics --strict` consumes it directly.
- `ACCEPTED_TRADEOFFS: dict[str, str]` printed in every run's output; empty at T3.

- [ ] **Step 2: Write `--self-test` (Fence FAIL arm #2) and prove it fails correctly**

Three synthetic arms, computed in-process from fabricated median tables — no
subprocesses:

| arm | input | expect |
|---|---|---|
| (a) | rust == 0.9 × zig on all 14 | exit 0 |
| (b) | as (a) but `grapheme-break.noop/utf8` rust = 1.10 × zig | exit 1, output contains `LOST bench grapheme-break.noop/utf8` |
| (c) | candidate vs entry, `codepoint-width.table/utf8` candidate = 1.08 × entry | exit 1, output contains `SELF-REGRESS bench codepoint-width.table/utf8 (+8.0%)` |

```bash
python3 .port/tools/perf/ws10_suite.py --self-test
```

Expected: `SELF-TEST: PASS`. Then deliberately break arm (b)'s threshold to 1.20 and
confirm the self-test reports FAIL — a self-test that cannot fail is worth nothing.
Restore.

- [ ] **Step 3: Run the real fence on the entry binary**

```bash
cd /Users/gael/projects/rustty
uptime
python3 .port/tools/perf/ws10_suite.py \
  --rust target/release-fast/rustty-bench -n 9 \
  --emit-metrics /tmp/ws10-metrics.json
echo "exit=$?"
python3 .port/tools/gates/check_baseline_regression.py \
  --metrics /tmp/ws10-metrics.json --strict
echo "exit=$?"
```

Expected: `exit=0` from both; the fence prints 14 PASS lines with the margin per config;
G8 prints `compared 14 metrics; regressions 0; missing-from-candidate 0` and GREEN.
Note the tolerance asymmetry in the README: **G8 is a loose +7% fence against the frozen
June oracle JSON; G8b is the real fence.** G8 stays armed because it additionally
guards binary sizes at 0% tolerance, which G8b does not measure.

- [ ] **Step 4: FAIL arm #3 — the LIVE flip proof**

```bash
cd /Users/gael/projects/rustty
# Temporarily remove the simd feature (PERF_LOG r5 measured this as ~24% slower
# on terminal-stream — a real regression of known magnitude).
python3 - <<'PY'
import re, pathlib
p = pathlib.Path("crates/rustty-bench/Cargo.toml"); s = p.read_text()
p.write_text(s.replace('rustty-vt = { workspace = true, features = ["simd"] }',
                       'rustty-vt = { workspace = true }'))
PY
cargo build -p rustty-bench --profile release-fast
python3 .port/tools/perf/ws10_suite.py --rust target/release-fast/rustty-bench -n 9 \
  --self .perf-bins/rustty-bench-ws10-entry-RF
echo "exit=$? (expect 1)"
git checkout crates/rustty-bench/Cargo.toml
cargo build -p rustty-bench --profile release-fast
```

Expected: **exit 1**, with a `SELF-REGRESS bench terminal-stream/ascii (+2_%)` line in
the ~+20–25% range. If it exits 0, the fence is too loose and the band must be tightened
until this real regression is caught — the fence's entire justification is catching this
class of change. Paste the full output into `PERF_LOG.md`.

- [ ] **Step 5: Wire ci.sh and write the README**

Register in `.port/tools/ci.sh` alongside the existing G8 line (`:169`):

```
"G8|ARMED|$PY $GATES/check_baseline_regression.py --metrics $PERF_METRICS --strict"
"G8b|PERF-FENCE|$GATES/check_perf_fence.sh"
```

`check_perf_fence.sh` must **BLOCK (exit 3), not fail**, when the untracked oracle
binary or corpora are absent, or when the load average exceeds the idle threshold —
a missing measurement substrate is not a green pass and not a regression. It emits the
metrics file G8 then consumes, so G8b runs first.

`.port/tools/perf/README.md` documents: the untracked oracle binary + corpora and how to
rebuild them (`zig build -Doptimize=ReleaseFast`, oracle **pin only**, and the
memory-note that zigbench needs `-Doptimize=ReleaseFast`); the idle gate and why; the
two fences and their thresholds with the measured spreads behind each; the two-strike
rule; `ACCEPTED_TRADEOFFS`; and a bold statement that all numbers are comparable to
oracle `4749c4e93` only.

- [ ] **Step 6: Commit**

```bash
git add .port/tools/perf .port/tools/gates/check_perf_fence.sh .port/tools/ci.sh .port/phase-2/PERF_LOG.md
git commit -m "gate(G8b): paired same-session perf fence + armed G8; live flip proof via simd-off bench (WS10 T3)"
```

---

### Task 4: Put `rustty-bench` into a gate for the first time

**Files:**
- Modify: `.port/tools/ci.sh`

**Interfaces:** Consumes nothing new. Produces a test lane for the 16 existing
`rustty-bench` unit tests, so the benchmark-corpus work in Layer 2 has a guard.

- [ ] **Step 1: Establish the current count (real names, not guesses)**

```bash
cd /Users/gael/projects/rustty
grep -rn "rustty-bench" .port/tools/gates/ .port/tools/ci.sh
cargo test -p rustty-bench --lib 2>&1 | tail -5
cargo test -p rustty-bench --lib --release 2>&1 | tail -5
```

Expected: the grep finds **nothing** in `gates/` (this is `B-compression.md` §4.5's
finding, re-verified) — `rustty-bench` is in no gate at all. The test run reports
**16** tests across 12 files (`benchmark.rs` 1, `cli.rs` 1 —
`bench_registers_compression_actions`, `codepoint_width.rs` 1, `grapheme_break.rs` 1,
`is_symbol.rs` 1, `lib.rs` 2 — incl. `ref_all_decls`, `osc_parser.rs` 1,
`page_compression.rs` 2, `screen_clone.rs` 1, `scrollback_compression.rs` 3,
`terminal_parser.rs` 1, `terminal_stream.rs` 1). Record the exact number and names.

- [ ] **Step 2: Add the lane**

Register a `rustty-bench` unit-test lane in `.port/tools/ci.sh` next to the other
per-crate test lanes, in **both** debug and `--release` form (WS18 made the release lane
a real gate; a new crate must not be the thing that re-breaks it).

- [ ] **Step 3: Verify, including the negative control**

```bash
bash .port/tools/ci.sh 2>&1 | grep -i "rustty-bench"
```

Expected: the new lane appears and is GREEN with the count from Step 1. Then prove the
lane is not vacuous: temporarily make `cli::tests::bench_registers_compression_actions`
assert a 10th action, re-run, confirm the lane goes RED, revert.

- [ ] **Step 4: Commit**

```bash
git add .port/tools/ci.sh
git commit -m "gate: add rustty-bench unit-test lane (debug + release) — the crate was in no gate (WS10 T4)"
```

---

### Task 5: `cb4c49fbf` — scalar UTF-8 decode consumes partial sequences cut off by ESC

**Files:**
- Modify: `crates/rustty-simd/src/vt.rs`

**Interfaces:**
- Consumes: `crate::index_of::indexOf`, the existing highway FFI
  `rustty_simd_decode_utf8_until_control_seq`.
- Produces: a scalar decoder that agrees with the SIMD kernel on all inputs, and the
  first test coverage that function has ever had.

This is Layer 1's first task because it is a **correctness** fix (PREMISE CORRECTION §4),
and because its negative control is unusually clean.

- [ ] **Step 1: Write the failing differential test FIRST**

Port oracle `cb4c49fbf`'s `test "decode simd matches scalar"` into
`crates/rustty-simd/src/vt.rs`'s test module as
`fn decode_simd_matches_scalar()`. Faithfulness requirements:

- 10 000 iterations, seed `0xf00dface`, `input: [u8; 257]`, three byte-style mixtures
  exactly as the oracle (mostly-ASCII with 1-in-21 specials; heavy multi-byte/invalid;
  fully random), `len` uniform in `0..=257`.
- Zig's `std.Random.DefaultPrng` is xoshiro256++; do **not** try to reproduce its
  exact stream. Use a small deterministic PRNG defined in the test module (a 3-line
  xorshift64* is fine) seeded with the same constant, and say so in a PORT NOTE. The
  test's value is the differential property over a large deterministic input set, not
  byte-identical inputs to Zig's.
- Compare `utf8DecodeUntilControlSeq` (SIMD) against
  `utf8_decode_until_control_seq_scalar` **called directly** — the scalar path is
  unreachable through the public entry point on this target, which is precisely why it
  was never covered. Assert `consumed`, `decoded`, and the decoded prefix slice.
- Gate it `#[cfg(not(target_arch = "wasm32"))]` (on wasm both calls are the same
  function and the test is vacuous), mirroring the oracle's
  `if (comptime !options.simd) return error.SkipZigTest`.

```bash
cargo test -p rustty-simd --lib decode_simd_matches_scalar 2>&1 | tail -20
```

**Expected: FAIL**, with a printed input containing a truncated multi-byte prefix
immediately before a `0x1b`. If it passes, the test is not exercising the divergence —
fix the test before touching the implementation. Record the first failing input in the
commit message.

- [ ] **Step 2: Port the two targeted regression tests**

`test "decode partial UTF-8 before escape"` becomes
`fn decode_partial_utf_8_before_escape()`, both cases verbatim:
`b"hi\xc2\x1b[0m"` ⇒ `consumed == 3`, `decoded == 3`, `output[2] == 0xFFFD`;
`b"\xe0\xa0\x1bX"` ⇒ `consumed == 2`, `decoded == 1`, `output[0] == 0xFFFD`.
These must run against the **public** entry point (SIMD) *and* against the scalar
function directly — two assertions per case — because the whole point of the commit is
that the two must agree. Expected before the fix: the SIMD assertions PASS, the scalar
assertions FAIL.

- [ ] **Step 3: Apply the fix**

In the continuation-byte loop, replace the unconditional early return with the oracle's
shape: return `DecodeResult { consumed: decode_offset, decoded: decode_count }` **only**
when `decode.len() == input.len()`; otherwise `break` so the valid-so-far prefix falls
through to the maximal-subpart arm below. Port the oracle's replacement comment verbatim
(neutral prose; no snark to strip in this one).

- [ ] **Step 4: Verify — the whole module, both arms**

```bash
cargo test -p rustty-simd --lib 2>&1 | tail -8
```

Expected: all pre-existing tests in the module still pass (**19** before this task; **21**
after — the differential fuzz plus `decode_partial_utf_8_before_escape`) and the two new
ones pass. Then re-run Step 1's test with the fix **reverted** one more
time to confirm the FAIL/PASS flip is caused by this hunk and nothing else.

- [ ] **Step 5: wasm — the only target where the divergence was live**

```bash
bash .port/tools/build-vt-wasm.sh 2>&1 | tail -5
node .port/tools/wasm-vt-roundtrip.mjs 2>&1 | tail -5
```

Expected: unchanged results. This is the target whose *behaviour* just changed, so a
green wasm round-trip is the R5 no-divergence evidence.

- [ ] **Step 6: Commit**

```bash
git add crates/rustty-simd/src/vt.rs
git commit -m "simd/vt: scalar UTF-8 decode consumes partial sequences cut off by ESC + differential fuzz vs SIMD (WS10 T5)"
```

Strike `cb4c49fbf` in `MAIN_PARITY.md` §3 in T22's sweep, noting it was a behavioural
divergence, not a perf item.

---

### Task 6: `5caf20e3e` — do not reallocate exactly-sized tabstop storage

**Files:**
- Modify: `crates/rustty-vt/src/tabstops.rs`

- [ ] **Step 1: Failing test first.** Add `fn tabstops_resize_to_exact_capacity_keeps_buffer()`
      to `tabstops.rs`'s test module: build a `Tabstops` whose `dynamic_stops` has a
      known capacity, record the backing pointer, `resize` to exactly that size, and
      assert the pointer is unchanged. Expected: FAIL at `tabstops.rs:252`'s
      `if size < self.dynamic_stops.len()`.
- [ ] **Step 2: Apply** `<` → `<=`, ported from `5caf20e3e` with its comment.
- [ ] **Step 3: Verify** `cargo test -p rustty-vt --lib tabstops 2>&1 | tail -5` — new
      test passes, existing tabstops tests unchanged.
- [ ] **Step 4: Commit** `vt/tabstops: avoid reallocating exactly-sized dynamic stop storage (WS10 T6)`

---

### Task 7: `b5053153f` — log unsupported input once per distinct value

**Files:**
- Modify: `crates/rustty-vt/src/stream.rs`

- [ ] **Step 1: Confirm the gap.** `grep -n "log_unsupported_once" crates/rustty-vt/src/stream.rs`
      → 0 hits. Count the current `log::warn!` sites in `stream.rs` that the oracle
      converts (upstream converts **8**); list them by line so the port is auditable.
- [ ] **Step 2: Port** `logUnsupportedOnce(fmt, args, key)` as `log_unsupported_once`:
      a 16-slot dedup table of `AtomicU32`. Faithfulness notes: the oracle's table is a
      module-level `var`; in Rust use a `static [AtomicU32; 16]` with `Ordering` chosen
      to match the oracle's atomic semantics (document the choice — Zig's default
      `.monotonic` on `@atomicRmw` maps to `Ordering::Relaxed`). Convert all 8 call
      sites.
- [ ] **Step 3: Test.** Add `fn stream_logs_unsupported_once_per_value()`: drive the
      same unsupported sequence twice and assert the dedup slot transitions exactly once
      (the table is the observable — expose a `#[cfg(test)]` reader rather than
      capturing log output).
- [ ] **Step 4: Verify** `cargo test -p rustty-vt --lib stream 2>&1 | tail -5` green, and
      `cargo test -p rustty-vt --lib 2>&1 | tail -3` still reports its full count.
- [ ] **Step 5: Commit** `vt/stream: log unsupported-input messages once per distinct value (WS10 T7)`

---

### Task 8: `2f0e6659d`'s separable QoS hunk — `setQosClass` on the termio read thread (macOS)

**Files:**
- Modify: `crates/rustty-termio/src/exec.rs`

`E-os-termio-platform.md` row **E4** measured this at **15% throughput on an M4 Max**
and calls it separable from the pipeline — it applies to the *current* single read
thread. Landing it here de-risks T20 by removing one variable.

- [ ] **Step 1: Confirm.** `grep -rn "setQosClass" crates/rustty-termio/` → 0 hits.
      `grep -n "pub fn setQosClass" crates/rustty-os/src/macos.rs` →
      `:85 pub fn setQosClass(class: QosClass) -> Result<(), SetQosClassError>`.
      Only caller today: `crates/rustty-core/src/renderer_seam.rs:986`.
- [ ] **Step 2: Port.** Call `rustty_os::macos::setQosClass(QosClass::UserInitiated)`
      at the top of `ReadThread::threadMainPosix` (`exec.rs:3323`), `#[cfg(target_os =
      "macos")]`, matching the oracle's `comptime` gate and its error handling (the
      oracle logs and continues; do the same — `SetQosClassError::ThreadIncompatible`
      must not kill the read thread).
- [ ] **Step 3: Test.** Add `fn readthread_sets_user_initiated_qos()`
      (`#[cfg(target_os = "macos")]`): spawn the read thread against a pty pair and
      assert from *inside* it, via a hand-rolled `extern "C" { fn
      pthread_get_qos_class_np(...) }` (no new `libc` surface needed — `libc` is already
      a `rustty-termio` dependency at `Cargo.toml:79`, so use `libc`'s binding if it
      exposes one, else the local `extern` per the `mem.rs` convention), that the class
      is `QOS_CLASS_USER_INITIATED`. Expected: FAIL before Step 2.
- [ ] **Step 4: Verify.** `cargo test -p rustty-termio --lib readthread 2>&1 | tail -5` —
      the new test plus the pre-existing `readthread_pty_bytes_land_in_screen`
      (`exec.rs:4925`) both green.
- [ ] **Step 5: Commit** `termio/exec: set user-initiated QoS on the pty read thread (macOS) (WS10 T8)`

---

### Task 9: `9c9cf3e82` — renderer avoids allocating when there are no active links

**Files:**
- Modify: `crates/rustty-renderer/src/link.rs`

- [ ] **Step 1: Read the oracle hunk** (`git -C ../ghostty show 9c9cf3e82`, 1 file,
      +82/−14) and map it onto `crates/rustty-renderer/src/link.rs` (501 lines).
- [ ] **Step 2: Failing test.** Add a test that drives the link-collection path with a
      config carrying zero active links and asserts no allocation occurs. Use the
      established allocation-counting harness (the `alloc-bench` feature that
      `PERF_LOG.md` records `cargo test -p rustty-vt --features alloc-bench` running
      against) if it reaches the renderer; otherwise assert the returned collection is
      the empty/borrowed variant the oracle introduces, which is the structural
      observable. State which it is — do not claim an allocation assertion you did not
      make.
- [ ] **Step 3: Port, verify** (`cargo test -p rustty-renderer --lib link 2>&1 | tail -5`),
      **and run the fence** (`ws10_suite.py --rust … --self …`) even though no suite
      config exercises the renderer — a green fence here documents that the change is
      suite-neutral rather than leaving it unstated.
- [ ] **Step 4: Commit** `renderer/link: avoid allocating when there are no active links (WS10 T9)`

---

### Task 10: `258de36d1` + `4c5b1d5d5` — `terminal-stream` bench uses the full handler and 64 KiB reads

**Files:**
- Modify: `crates/rustty-bench/src/terminal_stream.rs`, `crates/rustty-vt/src/stream_terminal.rs` (stale header line only)

**This task makes the suite measure more work, so it will move the
`terminal-stream/ascii` number — by design.** It must therefore land BEFORE any Layer-3
task that claims a terminal-stream improvement, and its own suite delta must be recorded
as an `ACCEPTED_TRADEOFFS` entry with the explanation "the bench now measures the same
work as upstream's", not as a regression.

- [ ] **Step 1: Confirm the premise correction.** `grep -n "pub type Stream"
      crates/rustty-vt/src/stream_terminal.rs` → `:66 pub type Stream<'a> =
      stream::Stream<'a, Handler<'a>>`. So the full handler exists and
      `B-compression.md` §3.4's "blocked on area A" is stale. Also confirm the current
      bench state: `terminal_stream.rs:262` defines a local print-only `Handler`,
      `:31` imports `rustty_vt::stream::Stream`, `:223` uses `let mut buf = [0u8; 4096]`.
- [ ] **Step 2: Record the pre-swap number.** Run the fence and note the current
      `terminal-stream/ascii` medians for both binaries. This is the only chance to
      measure the old bench.
- [ ] **Step 3: Port `258de36d1`** — delete the local `Handler`, construct
      `rustty_vt::stream_terminal::Stream` over a real `Terminal`, exactly as the oracle
      sets `const Stream = terminalpkg.TerminalStream`. Port `4c5b1d5d5` — unbuffered
      reader, `[0u8; 65536]` to match termio's `buffer_capacity`.
- [ ] **Step 4: Fix the stale header.** `crates/rustty-vt/src/stream_terminal.rs:5`
      says `PORT STATUS: stub`; the PORT NOTEs below it record every arm landing and
      this task now depends on it. Rewrite that line to the truth.
- [ ] **Step 5: Verify.** `cargo test -p rustty-bench --lib terminal_stream` green;
      then run the bench for real over `ascii.bin` and confirm it exits 0 and that the
      medians moved (they must — otherwise the handler swap did not take effect, which
      is the vacuous-verification trap here).
- [ ] **Step 6: Record + fence.** Add `ACCEPTED_TRADEOFFS["terminal-stream/ascii"]` with
      before/after and the justification. Re-run the ORACLE fence: the oracle binary is
      the **pinned** one whose own `TerminalStream.zig` is *also* the pre-swap
      print-only form, so **after this task rustty measures more work than the oracle
      binary does on this config** and the comparison is no longer apples-to-apples.
      **State this explicitly in `PERF_LOG.md` and move `terminal-stream/ascii` from
      Fence 1's STRICT arm to a documented `INCOMPARABLE` set** until/unless a
      pin-era-equivalent oracle bench exists. Do not fake a win by keeping the old bench.
- [ ] **Step 7: Commit** `bench/terminal-stream: full terminal handler + 64 KiB chunks (WS10 T10)`

---

### Task 11: `7e14347c1` — bounded page-map probe lengths + the `HyperlinkMap` benchmark

**Files:**
- Modify: `crates/rustty-vt/src/hash_map.rs`, `page.rs`/`page_impl.rs`, `hyperlink.rs`, `screen.rs`
- Create: `crates/rustty-bench/src/hyperlink_map.rs`
- Modify: `crates/rustty-bench/src/{cli.rs,lib.rs}`

Per PREMISE CORRECTION §6, this commit's substance is `hash_map.zig` **+472**, not the
benchmark. Port the substance first, then the instrument.

- [ ] **Step 1** Read the full commit (`git -C ../ghostty show 7e14347c1`) and enumerate
      its hunks per file against `crates/rustty-vt/src/hash_map.rs`.
- [ ] **Step 2** Port the probe-length bounding with its oracle tests. This touches
      `Page` capacity math, so `check_capacity_saturation.sh` and
      `check_paired_invariants.sh` are in scope.
- [ ] **Step 3** Port `benchmark/HyperlinkMap.zig` (155 lines, 2 modes, 4 options,
      1 test) as `hyperlink_map.rs`, and register: `cli.rs:22` `Action::HyperlinkMap`
      (10th variant), `cli.rs:47` `from_name` `b"hyperlink-map"`, `cli.rs:209`
      `main_action` dispatch, `lib.rs` module + re-export.
- [ ] **Step 4** Extend `cli::tests::bench_registers_compression_actions` (or add a
      sibling named for what it checks) to assert all **11** actions round-trip through
      `from_name`. Expected FAIL before Step 3.
- [ ] **Step 5** Verify: `cargo test -p rustty-vt --lib hash_map`,
      `cargo test -p rustty-bench --lib`, `bash .port/tools/ci.sh` for the capacity
      gates, and the SELF fence (`hash_map` is on the print/hyperlink hot path — this is
      a task where a self-regression is genuinely plausible).
- [ ] **Step 6** Commit as two commits: `vt/hash_map: bound page map probe lengths (WS10 T11)` then `bench: HyperlinkMap benchmark + hyperlink-map action (WS10 T11)`

---

### Task 12: `446f80f4e`'s bench hunk — `ScreenClone` `--loops` and the three render modes

**Files:** Modify `crates/rustty-bench/src/screen_clone.rs`, `cli.rs`

Split out of T19 deliberately: `--loops` alone makes `screen-clone` long enough to
profile, and `screen-clone.clone` is one of the three configs flagged as possibly
drifted in T1 — a `--loops` knob is the tool that resolves that question.

- [ ] **Step 1** Port the `loops: u32` option and the iteration-count change
      (1 000 → 50 000 × loops) from `git -C ../ghostty show 446f80f4e -- src/benchmark/ScreenClone.zig`.
- [ ] **Step 2** Add the three modes `render-locked`, `render-clean`, `render-partial`
      to `Mode` (currently only `Noop`/`Clone`/`Render` at `screen_clone.rs:80-99`),
      `from_name`, and the `stepFn` switch. The three new modes need `446f80f4e`'s
      `RenderState` API, which arrives in T19 — so **land them here as
      `unimplemented!()`-free stubs only if the API exists; otherwise land `--loops`
      now and the three modes as part of T19.** Decide by
      `grep -n "fn begin_update" crates/rustty-vt/src/render.rs`; record which branch
      was taken and why.
- [ ] **Step 3** Verify + fence; use `--loops` to re-check T1's `screen-clone.clone`
      drift finding with the per-pass slope method
      (`(t(loops=N) − t(loops=1)) / (N−1)`, the technique WS7 D8 used to strip spawn
      cost) and record the result.
- [ ] **Step 4** Commit `bench/screen-clone: --loops option and 50k iteration count (WS10 T12)`

---

### Task 13: `253e4f9c3` — bulk-parse CSI parameter bytes at slice level

**Files:** Modify `crates/rustty-vt/src/stream.rs`

- [ ] **Step 1** `grep -n "consume_csi_params" crates/rustty-vt/src/stream.rs` → 0 hits
      (confirm the gap). Read `git -C ../ghostty show 253e4f9c3` (1 file, +76, the new
      fn is `fn consumeCsiParams(self: *Self, input: []const u8) usize`).
- [ ] **Step 2 — the `PERF(port)` stop-and-decide.** `crates/rustty-vt/src/parser.rs:592`
      carries rustty's r5 state-equal fast path, worth −55% on `terminal-parser`. Read it
      and state, in a PORT NOTE, whether `consume_csi_params` subsumes it, coexists with
      it, or conflicts. If conflict: keep the rustty win, port the upstream structure
      around it, and record the decision with the measurement that justifies it.
      **Do not silently delete a `PERF(port)` site.**
- [ ] **Step 3** Port `consume_csi_params` into the `Stream` impl, plus its call site in
      `consume_until_ground` (`stream.rs:821`). Upstream ships this commit without new
      tests; the existing `stream.rs` CSI tests are the guard — count them first
      (`grep -c "#\[test\]" crates/rustty-vt/src/stream.rs`) so a silent drop is visible.
- [ ] **Step 4** Add the differential test the oracle omitted:
      `fn stream_csi_bulk_params_match_per_byte()` — drive a corpus of CSI sequences
      (params, sub-params, intermediates, `>`/`?` private markers, overflow-length
      params) through `next_slice` and through per-byte `next`, and assert identical
      handler action sequences. This is the falsifiable check that the bulk path did not
      change semantics; it must FAIL if the bulk path mis-handles any of those forms
      (verify by seeding a deliberate off-by-one, observing RED, reverting).
- [ ] **Step 5** Verify: full `cargo test -p rustty-vt --lib` (record the count before
      and after; it must only grow), then the ORACLE + SELF fences with the
      `terminal-parser` and `terminal-stream` deltas recorded in `PERF_LOG.md`.
- [ ] **Step 6** Commit `vt/stream: bulk-parse CSI parameter bytes at the slice level (WS10 T13)`

---

### Task 14: `300f42c7a` + `1a88f3622` — CSI entry bytes inline, CSI finals dispatched directly

**Files:** Modify `crates/rustty-vt/src/stream.rs`

Two commits, two sub-steps, one task because `1a88f3622` builds directly on
`300f42c7a`'s inline entry handling. **Note the SHA correction: `300f42c7a`, not the
non-existent `300b42f42a` that `D-terminal-subsystems.md` cites.**

- [ ] **Step 1** Port `300f42c7a` (`inline fn csiEntryByte(self: *Self, c: u8) bool`,
      +63/−34) as `csi_entry_byte`, wired into `consume_until_ground`.
- [ ] **Step 2** Port `1a88f3622` (`fn csiDispatchFinal(self: *Self, c: u8) void`, +89)
      as `csi_dispatch_final`.
- [ ] **Step 3 — the `vtRaw` interlock.** `D-terminal-subsystems.md:506` flags that this
      pair's `@hasDecl(T, "vtRaw")` gate is **load-bearing for the inspector**: the
      inspector must stay on the general path so it still sees every raw action. rustty
      already models `vtRaw` as the `vt_raw` trait method (`stream.rs:256-260`, dispatch
      at `:1019`). Both fast paths must be gated on `vt_raw` exactly as the oracle gates
      on `@hasDecl`. Add `fn stream_fast_paths_bypassed_when_vt_raw_hooked()`: a handler
      whose `vt_raw` returns `true` must observe the identical raw-action sequence
      before and after this task. Expected: this test FAILS if the gate is omitted —
      prove that by omitting it once.
- [ ] **Step 4** Re-run T13's differential test — it must still pass — plus the full
      `rustty-vt` lib suite and the inspector's own tests
      (`cargo test -p rustty-inspector 2>&1 | tail -3`).
- [ ] **Step 5** Fences; record the `terminal-parser` delta.
- [ ] **Step 6** Two commits: `vt/stream: handle CSI entry bytes inline in consume_until_ground (WS10 T14)` and `vt/stream: dispatch CSI finals directly from the stream fast paths (WS10 T14)`

---

### Task 15: `083d9709b` — decode ASCII inline in the SIMD ESC scan

**Files:** Modify `crates/rustty-simd/cbits/simd/vt.cpp`

- [ ] **Step 1** `grep -n "WidenAsciiStore\|DecodeNonAsciiUntilControlSeq"
      crates/rustty-simd/cbits/simd/vt.cpp` → 0 hits. Read
      `git -C ../ghostty show 083d9709b` (1 file, +125/−36).
- [ ] **Step 2** Port `WidenAsciiStore` (byte→u32 widening via `PromoteTo` on quarter
      vectors) **including the `HWY_TARGET == HWY_SCALAR` arm** — without it, wasm and
      scalar highway targets break, per `D-terminal-subsystems.md`. Port
      `DecodeNonAsciiUntilControlSeq(d, input, count, base, …)` as the non-ASCII
      continuation.
- [ ] **Step 3** T5's `decode_simd_matches_scalar` is the guard, and it is now doing real
      work: it compares the *changed* SIMD kernel against the *fixed* scalar reference
      over 10 000 inputs. Run it first; it must pass. Then run the whole
      `rustty-simd` lib suite and `decode_tight_buffer_no_overread`
      (`vt.rs:218`) **under AddressSanitizer** — the new widening store is exactly the
      class of change that over-reads:
      `bash .port/tools/gates/sanitizers.sh` (and `check_sanitizer_canary.sh` to prove
      ASan is actually armed, not silently no-op).
- [ ] **Step 4** wasm: `bash .port/tools/build-vt-wasm.sh` + `node
      .port/tools/wasm-vt-roundtrip.mjs` — the `HWY_SCALAR` arm's proof.
- [ ] **Step 5** Fences; the expected movers are `terminal-parser` and
      `terminal-stream`.
- [ ] **Step 6** Commit `simd/vt: decode ASCII inline in the SIMD scan for ESC (WS10 T15)`

---

### Task 16: `47e26df60` + `cb2d78587` + `f2a7652ab` — batch printed codepoint runs into direct row fills

**Files:** Modify `crates/rustty-vt/src/terminal.rs`, `stream.rs`, `stream_terminal.rs`, `crates/rustty-simd/src/lib.rs`, `crates/rustty-termio/src/stream_handler.rs`

The largest Layer-3 item: `47e26df60` alone is +675 lines in `Terminal.zig`.

- [ ] **Step 1** Confirm the gaps: `grep -n "print_slice\|PrintSlice"
      crates/rustty-vt/src/{stream.rs,terminal.rs}` → 0 hits;
      `grep -n "fn lanes" crates/rustty-simd/src/lib.rs` → 0 hits (the file is 54 lines).
- [ ] **Step 2** Port `f2a7652ab`'s `simd/main.zig` hunk first: `pub fn lanes<T>()`
      (= 2× native vector width), which the printable-run scan needs.
- [ ] **Step 3** Port `47e26df60`'s `Terminal` surface with the oracle's exact names:
      `print_slice` (pub), `print_slice_fast`, `print_slice_eligible` (inline, generic
      over `PrintSliceWidth`), `print_slice_fill`, `print_slice_check_expected`,
      `field_mask`. **`PERF(port)` stop-and-decide:** `terminal.rs:884` carries rustty's
      r5-print-outline (cold grapheme/zero-width/wide arms outlined via
      `#[inline(never)]`). Decide and document whether the upstream eligibility check
      subsumes the outlining or whether both stay.
- [ ] **Step 4** Port the four oracle tests verbatim as
      `terminal_print_slice_simple_ascii`, `terminal_print_slice_wraps_and_scrolls`,
      `terminal_print_slice_pending_wrap_state`, and the fuzz
      `terminal_print_slice_differential_fuzz_vs_print` (with its
      `test_print_slice_differential` helper). The differential-vs-`print` fuzz is the
      real guard and the negative control: run it against a deliberately wrong
      `print_slice_eligible` (e.g. admit a wide codepoint) and confirm RED.
- [ ] **Step 5** Port `cb2d78587` (bulk style-only cell-run fill, +112/−5) and
      `f2a7652ab`'s `Terminal.zig` +155 vectorization on top, in that order.
- [ ] **Step 6** Port the dispatch: `stream.rs` `Action::PrintSlice` + the lane-wise
      printable-run scan inside `next_slice` (`:662`); `stream_terminal.rs`
      `.print_slice => terminal.print_slice(cps)`; `stream_handler.rs`'s arm.
- [ ] **Step 7 — Stacked Borrows.** `print_slice_fill` writes cell runs through row
      pointers. Per the `rust-mut-args-strongly-protected` rule, the fill helpers take
      **raw-pointer** row/cell args, never `&mut`, and must not hold an alias across an
      `assert_integrity` call. Run
      `cargo +nightly miri test -p rustty-vt --lib print_slice 2>&1 | tail -20`.
      Expected: clean. If Miri times out on the fuzz test, add
      `#[cfg_attr(miri, ignore)]` **to the fuzz test only** and register it in
      `MIRI_EXCEPTIONS.md` with the reason — never blanket-ignore the whole module.
- [ ] **Step 8** Verify: full `cargo test -p rustty-vt --lib` (count before/after);
      `cargo test -p rustty-termio --lib`; T13/T14's differential tests still green.
- [ ] **Step 9** Fences. This is the task most likely to produce a *net* win with a
      local loss (single-`print` may slow while bulk speeds up); any such trade goes in
      `ACCEPTED_TRADEOFFS` with numbers.
- [ ] **Step 10** Commits, one per upstream commit:
      `simd: lanes(T) helper (WS10 T16)`,
      `vt/terminal: batch printed codepoint runs into direct row fills (WS10 T16)`,
      `vt/terminal: fill style-only cell runs in bulk in print_slice_fill (WS10 T16)`,
      `vt/terminal: vectorize the print-slice fill (WS10 T16)`

---

### Task 17: `f6f79acce` + `8c523ed03` — bulk APC slice dispatch, its vectorization, and the `ApcParser` benchmark

**Files:** Modify `crates/rustty-vt/src/{stream.rs,stream_terminal.rs}`, `apc/mod.rs`, `apc/glyph/request.rs`, `kitty/graphics_command.rs`, `crates/rustty-termio/src/stream_handler.rs`; create `crates/rustty-bench/src/apc_parser.rs`, `crates/rustty-synthetic/src/kitty.rs`, `crates/rustty-synthetic/src/cli/kitty.rs`

- [ ] **Step 1** Confirm gaps: `grep -rn "apc_put_slice\|feed_slice" crates/rustty-vt/src/`
      → 0 hits; `crates/rustty-vt/src/apc/mod.rs:103` has `pub fn feed(&mut self, alloc:
      Allocator, byte: u8)` and no slice form.
- [ ] **Step 2 — the union-payload borrow hazard.** `D-terminal-subsystems.md` §4.6
      flags `apc.Handler.feedSlice` as "borrow of a union payload while reassigning the
      union" — legal in Zig, UB-adjacent in Rust. Port `feed_slice` so the payload is
      read out (or accessed by raw pointer) **before** the enum discriminant is
      reassigned. Write the Miri check for this specific shape into the module's tests.
- [ ] **Step 3** Port `feed_slice` on `apc::Handler`, `apc/glyph/request.rs`'s
      `CommandParser::feed_slice` (+ the `ReadFailed` error arm), and
      `kitty/graphics_command.rs`'s `Parser::feed_slice`.
- [ ] **Step 4** Port `stream.rs`'s `Action::ApcPutSlice` (+ its `cval` C form),
      `consume_apc_string`, and the `stream_terminal.rs` / `stream_handler.rs` arms.
      Port the five oracle tests: `stream_apc_bulk_slice`,
      `stream_apc_bulk_slice_split_across_inputs`,
      `stream_apc_bulk_slice_keeps_c0_bytes_as_data`, `stream_apc_aborted_by_can`,
      `stream_apc_scalar_path_matches`.
- [ ] **Step 5** Port `8c523ed03` (vectorized `invalid`-mask loop inside
      `consume_apc_string`, +43/−2) and its test
      `stream_apc_vector_boundaries_match_scalar_path`. That test is the negative
      control for the vectorization: it must FAIL if the lane boundary handling is off
      by one (verify).
- [ ] **Step 6** Port `synthetic/Kitty.zig` (+88) and `synthetic/cli/Kitty.zig` (+70) as
      `crates/rustty-synthetic/src/kitty.rs` + `cli/kitty.rs`, register in
      `synthetic/src/cli.rs` and `main_gen.rs`, then port
      `benchmark/ApcParser.zig` (143 lines) as `crates/rustty-bench/src/apc_parser.rs`
      with `Action::ApcParser` / `b"apc-parser"` registration (12th action; extend the
      registration test again).
- [ ] **Step 7** Verify: `cargo test -p rustty-vt --lib apc`,
      `cargo test -p rustty-synthetic`, `cargo test -p rustty-bench --lib`;
      `cargo +nightly miri test -p rustty-vt --lib apc 2>&1 | tail -20` clean (Step 2's
      hazard); generator determinism — run `rustty-synthetic +kitty --seed=42` twice and
      `cmp` the outputs.
- [ ] **Step 8** Fences (the new `apc-parser` config is **not** part of the 14 — the
      oracle bench binary predates it, so it has no paired oracle. Add it to the
      harness as a **rustty-only self-fenced** config and say so explicitly; do not
      count it toward "14/14").
- [ ] **Step 9** Commits, one per upstream commit plus one for the bench/generator pair.

---

### Task 18: `77190bd02` — scroll-region optimizations, and the closure of O3

**Files:** Modify `crates/rustty-vt/src/screen.rs`, `terminal.rs`, `.port/phase-2/DEFERRALS.md`; create `.port/phase-2/mirihar-ws10-scrollregion/`

This is the task that discharges the orphaned ledger row (PREMISE CORRECTION §7).
`A-terminal-core.md` records that `cursor_scroll_region_up` carries the **heaviest
Stacked-Borrows risk in area A**, so the Miri harness is not optional.

- [ ] **Step 1** Confirm: `grep -rn 'cursor_scroll_region_up\|cursorScrollRegionUp'
      crates/` → **0 hits**. Confirm the 15 existing `invalidate_node_layout` sites
      (4 in `screen.rs`/`terminal.rs` from `0bad91adb`, 11 in `pagelist/`) so the 16th
      is attributable.
- [ ] **Step 2 — pre-port LIFETIMES.tsv step.** Per the adopted `LIFETIMES.tsv` ledger
      convention, record the field lifetimes of every raw-pointer alias
      `cursor_scroll_region_up`/`_slow` will hold (row pins, page pointers, the cursor
      pin) BEFORE writing the port. This is the prospective half of that ledger and it
      is what makes Step 5's Miri result interpretable.
- [ ] **Step 3** Port `pub fn cursorScrollRegionUp(self: *Screen, limit: usize) !void`
      and `fn cursorScrollRegionUpSlow` as `cursor_scroll_region_up` /
      `cursor_scroll_region_up_slow`, plus `77190bd02`'s `Terminal.zig` hunks
      (+184/−…). Raw-pointer row args throughout, per the constraint.
- [ ] **Step 4** Port all nine oracle tests verbatim, renamed to Rust slugs:
      `screen_cursor_scroll_region_up_simple`,
      `screen_cursor_scroll_region_up_moves_selection`,
      `screen_cursor_scroll_region_up_region_spans_pages`,
      `screen_cursor_scroll_region_up_region_spans_pages_with_background_sgr`,
      `screen_cursor_scroll_region_up_with_styled_erased_row`,
      `terminal_index_bottom_of_scroll_region_with_top_margin_and_background_sgr`,
      `terminal_index_bottom_of_alt_screen_full_region`,
      `terminal_index_bottom_of_alt_screen_top_region`,
      `terminal_scroll_up_top_region_no_scrollback`.
- [ ] **Step 5 — close O3.** Add `0bad91adb`'s 4th `invalidate_node_layout` call site
      inside `cursor_scroll_region_up`, and its test
      `screen_cursor_scroll_region_up_renews_page_generation` — the exact test name
      `DEFERRALS.md:4981` names as missing. Verify `grep -rn invalidate_node_layout
      crates/ | wc -l` → 16.
- [ ] **Step 6 — Miri.** Build `.port/phase-2/mirihar-ws10-scrollregion/` as an isolated
      non-FFI harness (the `mirihar-chunk3b` precedent: prove SB on a standalone
      harness, do not hand-reason it) driving the multi-page and selection-spanning
      cases. Run `cargo +nightly miri run` there **and**
      `cargo +nightly miri test -p rustty-vt --lib cursor_scroll_region 2>&1 | tail -20`.
      Both clean. Negative control: introduce a `&mut Row` where the port uses a raw
      pointer, confirm Miri reports the SB violation, revert — that is the proof the
      harness has teeth.
- [ ] **Step 7** Strike the ledger, in this commit: `DEFERRALS.md`'s
      "## Deferred to WS8 upstream sweep — WS7 Phase C divergence #11" section
      (`:4981`–`:5015`) becomes a RESOLVED entry naming WS10 T18, the ported function,
      the added call site and the test; and the O3 row in the WS19 audit table
      (`:5402`) is marked closed with the commit hash.
- [ ] **Step 8** Verify + fences. `77190bd02` touches `Screen`'s scroll paths, which
      `screen-clone.render` and `terminal-stream` both exercise — record both.
- [ ] **Step 9** Commit `vt/screen+terminal: scroll region optimizations, cursor_scroll_region_up + generation renewal (WS10 T18, closes DEFERRALS O3)`

---

### Task 19: `446f80f4e` — render-state update optimizations (~2.7×–11× less lock hold)

**Files:** Modify `crates/rustty-vt/src/render.rs`, `page_impl.rs`, `terminal.rs`; `include/rustty/vt/render.h`; `crates/rustty-vt-c/src/{render.rs,lib.rs}`; `crates/rustty-renderer/src/generic.rs`; `crates/rustty-bench/src/screen_clone.rs`

The single largest commit in the series (+1348/−209 across 9 files) and the only one
that moves the **C ABI**.

- [ ] **Step 1** Read the whole commit. Map its 9 files onto rustty:
      `render.zig` +919/−168 → `crates/rustty-vt/src/render.rs` (2243 lines);
      `page.zig` +274 → `page_impl.rs`; `Terminal.zig` +91/−…; `renderer/generic.zig`
      +16/−… → `crates/rustty-renderer/src/generic.rs`; `terminal/c/render.zig` +74 and
      `include/ghostty/vt/render.h` +66 → the rustty vt-c render surface;
      `lib_vt.zig` +2; `benchmark/ScreenClone.zig` (already partly in T12).
- [ ] **Step 2 — `PERF(port)` stop-and-decide, the biggest one.** rustty's r5
      clone-style-run win lives across `page_impl.rs:1423,1441,1467,1622` and
      `ref_counted_set.rs:528,652,723` and is worth **−58.7%** on
      `screen-clone.clone`. `446f80f4e` rewrites the same region. Enumerate every
      `PERF(port)` marker inside the rewritten hunks, and for each: subsumed (with a
      measurement) or preserved (with an updated note). Write this enumeration into the
      commit message.
- [ ] **Step 3** Port the Rust-side render-state rewrite with its oracle tests.
- [ ] **Step 4 — the C ABI.** Port `render.h`'s +66 and the `c/render.zig` +74 into
      `include/rustty/vt/render.h` and `crates/rustty-vt-c/src/render.rs`, then:
      `bash .port/tools/gates/check_rustty_vt_headers.sh`,
      `bash .port/tools/gates/check_embed_h_abi.sh` (A4),
      `bash .port/tools/gates/check_symbol_manifest.sh`,
      `node .port/tools/check-vt-struct-layouts.mjs`.
      Any by-value struct return added here must byte-match the header (memory:
      `embed-abi-byvalue-sret-gate` — an AAPCS64 sret mismatch is a crash, not a
      warning). Re-freeze the ABI baselines in the same commit.
- [ ] **Step 5** Land T12's three deferred `ScreenClone` render modes now that
      `RenderState` exists; the `render-locked`/`render-clean`/`render-partial` modes are
      the instrument for this commit's headline claim.
- [ ] **Step 6 — verify the headline claim, falsifiably.** The commit's subject asserts
      "~2.7x to ~11x less lock hold". Measure it: use the new `render-locked` mode with
      `--loops` to compare lock-hold time before/after. If the measured reduction is far
      from that range, **say so** — the port is faithful either way, but an unverified
      quoted number is exactly the vacuous verification this plan forbids.
- [ ] **Step 7** Verify: `cargo test -p rustty-vt --lib render`, `-p rustty-vt-c`,
      `-p rustty-renderer`, `-p rustty-bench`; `cargo build --workspace`;
      `cargo +nightly miri test -p rustty-vt --lib render 2>&1 | tail -20`;
      `bash .port/tools/gates/check_acyclic.sh`.
- [ ] **Step 8** Fences — `screen-clone.{clone,render}` are the expected movers.
- [ ] **Step 9** Commit `vt/render: render state update optimizations + c/render surface (WS10 T19)`

---

### Task 20: `2f0e6659d` + `bb0ac4c72` — the two-stage pty read pipeline

**Files:** Modify `crates/rustty-termio/src/exec.rs`, `.port/phase-2/MIRI_EXCEPTIONS.md`; create `.port/phase-2/mirihar-ws10-pipeline/`

Last, because it is the only **HIGH**-risk item in the series (`E-os-termio-platform.md`
row E1): ~350 lines of new concurrent code whose stated invariant is a Stacked-Borrows
violation in Rust.

- [ ] **Step 1 — the reverted-commit trap.** `E-os-termio-platform.md` row **E2**:
      `bed47168c` ("termio: bound POSIX read-ahead on non-Darwin") was **reverted by**
      `60121a039` inside the same range; net diff empty. **Port from the range diff, not
      commit-by-commit, and do NOT port `bed47168c`.** Verify the ordering yourself:
      `git -C ../ghostty log --oneline --ancestry-path bed47168c..60121a039 | tail -3`.
- [ ] **Step 2** Confirm the gap: `grep -rn "io-gather\|bridge_threshold\|batch_ready"
      crates/` → 0 hits. Current shape: `exec.rs:3323` `ReadThread::threadMainPosix`,
      `let mut buf = [0u8; 1024];` at `:3411`, one `'tight:` loop + one 2-fd `poll` at
      `:3413-3510`.
- [ ] **Step 3 — design the slot handoff BEFORE writing it.** The oracle's `Pipeline` is
      a 4-slot × 64 KiB ring with a mutex guarding only `lens`/`head`/`tail`/`count`/
      `bridging`/`done`, two condvars (`batch_ready`, `slot_free`), an idle self-pipe,
      and the constants `bridge_spin_max = 16`, `bridge_poll_timeout_ms = 1`,
      `bridge_threshold = 1024`, `gather_budget_ns = 3_000_000`. The "buffer contents
      need no locking" invariant means the gather thread writes slot *i* while the parse
      thread reads slot *j* — two live aliases into one allocation. Design: the ring
      buffer is a single heap allocation owned by neither thread; each side receives a
      `*mut u8` + len for its slot only, derived per-handoff from the base pointer
      (never a `&mut [u8]` that outlives the handoff), with slot ownership transferred
      under the mutex. Write this into a module-level PORT NOTE and into
      `LIFETIMES.tsv` before coding.
- [ ] **Step 4 — build the Miri harness first.** `.port/phase-2/mirihar-ws10-pipeline/`
      is an isolated **non-FFI** model of the ring: two real threads, the same mutex +
      condvar + raw-slot protocol, a fake producer/consumer, no `poll`/`read`. Precedent:
      `mirihar-chunk3b`, `mirihar-xev-mpsc`. Run
      `cargo +nightly miri run` there. **Negative control: implement one handoff with
      `&mut [u8]` instead of a raw pointer and confirm Miri reports the SB violation.**
      Only proceed once the harness is clean AND the negative control fails as expected.
- [ ] **Step 5** Port the pipeline into `exec.rs`: `gatherMainPosix` on a new `io-gather`
      thread (named per the oracle), `threadMainPosix` as the parse stage, the 3-fd poll
      set, the `bridging` flag and idle self-pipe, and the buffer growth 1 KiB → 64 KiB.
      Apply T8's `setQosClass` to **both** threads (the oracle does). Port `bb0ac4c72`
      (+110/−7, "don't bridge pty reads while the parser is idle") on top, as a second
      commit.
- [ ] **Step 6 — the two guarding tests E1 says do not exist.** The existing
      `readthread_pty_bytes_land_in_screen` (`exec.rs:4925`) passes with *either*
      shape, so it is not a guard. Add:
      - `fn readthread_pipeline_sustains_saturated_producer()` — a writer blasting
        ≥8 MiB into the pty master must be fully drained into the screen with no lost or
        reordered bytes; assert total bytes and a checksum, and assert the throughput
        exceeds the pre-pipeline shape's (measure both by keeping the old loop behind a
        `#[cfg(test)]` switch for the duration of the task, then remove it).
      - `fn readthread_pipeline_keeps_interactive_latency()` — a single sub-1 KiB write
        must land in the screen within a bounded time (assert against
        `gather_budget_ns` × a safety factor, and state the factor). This is the
        *latency* property `2f0e6659d` exists for, and the throughput suite cannot see
        it.
      Both must FAIL against the pre-pipeline shape — run them before Step 5 to prove it.
- [ ] **Step 7 — `threadMainPosix`'s two new early returns.** E1's §"return early in two
      new places" warning: verify the quit-pipe `QuitFdCloser` Drop guard
      (`exec.rs:3331-3341`) still fires on **every** path in both threads, and that the
      gather thread's exit cannot leave the parse thread blocked on `batch_ready`
      forever. Add `fn readthread_pipeline_shuts_down_cleanly_from_both_stages()`
      covering quit-during-gather and quit-during-parse.
- [ ] **Step 8** Verify: `cargo test -p rustty-termio` (record the count);
      `cargo test -p rustty-termio --release` (the WS18 lane);
      the real-`poll`/`read` arms go into `MIRI_EXCEPTIONS.md` as Category 2 with the
      justification, while the pure ring logic stays Miri-run;
      run the termio suite 20× to smoke out the flakes concurrency ports produce
      (`for i in $(seq 20); do cargo test -p rustty-termio --lib readthread || break; done`).
- [ ] **Step 9** Fences. The suite does **not** exercise the pty path at all, so state
      plainly that the fence is neutral here and the pipeline's evidence is Step 6's two
      tests — not the benchmark suite. Do not imply suite coverage that does not exist.
- [ ] **Step 10** Commits: `termio/exec: pipeline pty reads to overlap parsing with draining (WS10 T20)` then `termio/exec: don't bridge pty reads while the parser is idle (WS10 T20)`

---

### Task 21: Final paired re-measure and the WS10 perf report

**Files:** Modify `.port/phase-2/PERF_LOG.md`, `.port/phase-2/PERF_COMPARISON.md`

- [ ] **Step 1 — idle gate again.** `uptime`; proceed only at 1-min load ≤ 2.0. This is
      the number that goes into the ROADMAP; a contended run here undoes the whole plan.
- [ ] **Step 2** `cargo build -p rustty-bench --profile release-fast`; then
      `python3 .port/tools/perf/ws10_suite.py --rust target/release-fast/rustty-bench
      --self .perf-bins/rustty-bench-ws10-entry-RF -n 9
      --emit-metrics .port/phase-2/perf-ws10/final.json`, and
      `python3 .port/tools/gates/check_baseline_regression.py --metrics
      .port/phase-2/perf-ws10/final.json --strict`.
- [ ] **Step 3** Write the WS10 final table: per config, T1 entry median → final median,
      the oracle median, the verdict per fence arm, and the `INCOMPARABLE` set
      (`terminal-stream/ascii` after T10) with its reason. State the win count honestly
      in the form "**N/13 paired configs win against the pinned oracle; 1 config
      (`terminal-stream/ascii`) is no longer comparable because T10 made rustty's bench
      measure strictly more work than the pinned oracle binary does**" — not "14/14"
      unless it is literally true after the T10 change.
- [ ] **Step 4** Record every **negative result** (a ported upstream optimization that
      measured neutral or worse on this host), the same way `PERF_LOG.md`'s "Round 5
      honest outcomes" section does. A perf series with no recorded negative results is
      an under-reported one.
- [ ] **Step 5** Append a dated addendum to `PERF_COMPARISON.md` pointing at the WS10
      table, and add the PREMISE CORRECTION §1 warning (pinned-oracle-only comparability;
      `origin/main` needs Zig 0.16) at the top of that file so the next reader cannot
      miss it.
- [ ] **Step 6** Commit `perf: WS10 final paired measurement and report (WS10 T21)`

---

### Task 22: Ledger sweep, verification debt, ROADMAP repair

**Files:** Modify `.port/phase-2/{MAIN_PARITY.md,DEFERRALS.md,VERIFICATION_DEBT.md}`, `docs/superpowers/plans/2026-07-23-rustty-completion-ROADMAP.md`, `.port/phase-3/assessments/{B-compression.md,D-terminal-subsystems.md}`

- [ ] **Step 1 — `MAIN_PARITY.md`.** Strike each ported row in §3 with its rustty commit:
      `253e4f9c3`, `1a88f3622`, `300f42c7a`, `f6f79acce`, `8c523ed03`, `47e26df60`,
      `cb2d78587`, `083d9709b`, `cb4c49fbf`, `b5053153f`, `77190bd02`, `446f80f4e`,
      `9c9cf3e82`, `2f0e6659d`, `bb0ac4c72`, `4c5b1d5d5`, `258de36d1`, `7e14347c1`,
      `5caf20e3e`, and the in-scope hunks of `f2a7652ab`. Rewrite the §2 **BL-4** bullet
      to CLOSED-by-WS10 form (the shape the BL-5/BL-6/BL-7 bullets already use), listing
      what landed and naming anything deliberately not ported with its basis. Record
      `bed47168c`/`60121a039` as **N/A — net zero, deliberately not ported** so the trap
      is documented, not just avoided once. Record `quirks_memset` (`20c3eae04`,
      `e8525c0fd`) as **N/A — Zig-toolchain-specific, deliberate non-port**, with the
      libc-`memset` justification.
- [ ] **Step 2 — `DEFERRALS.md`.** Confirm T18 struck `:4981`/`:5011` and the O3 row.
      Then `grep -n "BL-4" .port/phase-2/DEFERRALS.md` and repair every remaining
      reference — a closed backlog must not still be named as a future owner anywhere.
- [ ] **Step 3 — `VERIFICATION_DEBT.md`.** Add two rows in the established V-numbered
      format (next free is **V6**):
      - **V6 — the `origin/main` oracle cannot be built on this host.** Evidence:
        `origin/main:build.zig.zon` `.minimum_zig_version = "0.16.0"` vs
        `zig version` → `0.15.2`. Consequence: the `quirks_memset`-accelerated oracle
        (~2.8× faster on `+terminal-stream`) is unmeasurable, so every WS10 fence is
        against the pinned binary. **Recipe:** install Zig 0.16.0; `git -C ../ghostty
        worktree add /tmp/oracle-main origin/main` (**a worktree, never a checkout** —
        four lanes depend on the pinned tree); `zig build -Doptimize=ReleaseFast` in the
        worktree; `python3 .port/tools/perf/ws10_suite.py --oracle
        /tmp/oracle-main/zig-out/bin/ghostty-bench --rust target/release-fast/rustty-bench`.
        **Pre-stated expected outcome:** `terminal-stream`'s oracle median drops toward
        ~195 ms and rustty likely LOSES that config. **That is the oracle getting
        faster, not rustty regressing** — do not treat it as a WS10 regression.
      - **V7 — no Linux perf measurement.** The oracle bench binary is macOS arm64 and
        no Linux oracle exists here. Unmeasured on Linux: `2f0e6659d`'s POSIX pipeline
        (which E1 notes "is POSIX and runs on Linux too") and the `MADV_DONTNEED` twin
        of BL-2's `madvise`. Recipe: build both binaries on the Fedora VM (`ssh rvm`),
        regenerate the corpora with `rustty-synthetic`/`ghostty-gen` there, run the
        fence.
- [ ] **Step 4 — assessment repairs.** These are the source documents the next reader
      will trust, so fix them in place with a dated note: `D-terminal-subsystems.md`
      rows 135/506 (`300b42f42a` → `300f42c7a`); `B-compression.md` §3.4 (the
      `TerminalStream` swap is not blocked — `stream_terminal.rs:66` exists) and its
      `7e14347c1` row (the commit's substance is `hash_map.zig` +472, not the
      benchmark); `B-compression.md` §4.5 (`rustty-bench` is now in a gate — T4).
- [ ] **Step 5 — ROADMAP.** Rewrite the WS10 row: what landed, the honest win count from
      T21 Step 3, the `INCOMPARABLE` config and why, the two new V-rows, and the
      pinned-oracle comparability statement. Also correct the Wave-3 framing if it still
      implies the 14/14 figure is comparable to any oracle other than `4749c4e93`.
- [ ] **Step 6 — final gate sweep.**

```bash
cd /Users/gael/projects/rustty
cargo build --workspace 2>&1 | tail -3
cargo test --workspace 2>&1 | tail -5
cargo test --workspace --release 2>&1 | tail -5
cargo clippy --workspace --all-targets --no-deps -- -D warnings 2>&1 | tail -3
cargo fmt --check
bash .port/tools/ci.sh 2>&1 | tail -40
```

All green, **including the newly armed G8 and G8b**. Do NOT run
`no_homebrew_gettext_gate.sh` (WS9's lane owns it).

- [ ] **Step 7** Commit `docs: WS10 ledger sweep, V6/V7 verification debt, ROADMAP repair (WS10 T22)`

---

## Success criteria

1. Every BL-4 commit is either ported (with its oracle tests, at their real names) or
   recorded in `MAIN_PARITY.md` as a deliberate non-port with a written basis.
   `bed47168c` is not ported; `quirks_memset` is not ported; both facts are in the ledger.
2. `cb4c49fbf`'s divergence is fixed and the scalar decoder has test coverage for the
   first time; `decode_simd_matches_scalar` is green and was **proven to fail** before
   the fix.
3. `DEFERRALS.md`'s O3 row is closed by real code: `cursor_scroll_region_up` exists, the
   16th `invalidate_node_layout` site is inside it, and
   `screen_cursor_scroll_region_up_renews_page_generation` passes.
4. G8's `(id, optimize)` bug is fixed with a self-test arm that was **proven to fail**
   before the fix; G8 is armed with a real metrics file; G8b (the paired fence) is in
   `ci.sh`; `rustty-bench` is in a test gate.
5. The perf fence has **three demonstrated FAIL arms** — the gate self-test, the
   harness self-test, and the live `simd`-off flip proof — all recorded in `PERF_LOG.md`
   with output.
6. `cargo build --workspace`, `cargo test --workspace` (debug **and** `--release`),
   clippy `-D warnings`, `fmt --check`, `check_acyclic.sh`, the A4/vt-header ABI gates,
   and both Miri harnesses are green.
7. T21's final table states the win count **honestly**, names the `INCOMPARABLE` config
   and why, and records the negative results.
8. `VERIFICATION_DEBT.md` carries V6 and V7 with runnable recipes and — for V6 — the
   pre-stated expected outcome, so the `origin/main` oracle comparison cannot be
   misread as a rustty regression.
9. No new deferrals. Nothing in `DEFERRALS.md` names BL-4 or WS10 as a future owner.
