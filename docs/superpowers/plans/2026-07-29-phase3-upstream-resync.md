# Phase 3 — Upstream Resync Master Plan (pin `4749c4e93` → head `6ad1fe7d8`)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close the 365-commit gap between the port's pinned oracle baseline `4749c4e93` (2026-06-20) and upstream head `6ad1fe7d8` (2026-07-29), and end with `.port/ORACLE_SYNC.md` able to state **one honest sync point** instead of "baseline + a list of beyond-pin series". Concretely: (1) repair the instruments first — the LZ4 encoder byte-stream has **no** guard at all, A5's symbol check is one-directional, `lz4_differential_exhaustive` reports PASSED where the oracle reports SKIPPED, `rustty-bench` is in no gate, and `_vt_header_baseline.txt` is a dead artifact that has already been mis-cited as coverage; (2) close the coverage the assessments explicitly declared **sampled rather than exhaustive**, so no verdict rests on a pattern-match; (3) port the ≈95 absent + ≈19 partial semantic changes in dependency order, from the **range diff** rather than commit-by-commit; (4) audit the five faithfully-carried oracle bugs that upstream has since fixed — invisible to every gate; and (5) advance the pin, which means moving the oracle **working tree**, not editing a document.

**Architecture:** Eleven workstreams in four layers. **Layer 0 (`P3-W0`) is the instrument layer** and must complete before any porting: several gates are vacuous or structurally blind, so porting into them yields meaningless green — the two absent `rustty_render_state_*` exports hid for six weeks precisely because A5(b) only checks one direction, and a `HASH_LOG` 12→11 edit was measured to diverge **1,929 of 7,976** encoded LZ4 blocks from upstream with the whole suite green. **Layer 1 (`P3-W1`–`P3-W2`) is vt-core**: the live-bug wave (all S/M, no cross-area coupling) then the page-map/capacity plumbing whose pointer-stability contract change (`fedd42e8d`) is a logic bug in Zig and **UB** in rustty's raw-pointer maps. **Layer 2 (`P3-W3`–`P3-W6`) is the coupled feature mass**: runtime scrollback limits (one atomic unit spanning vt + config + termio + C-API + inspector + bench), resize/cursor/visibility state (whose signature change ripples to four crates), the perf/print/render family gated on a single shared `page::Mask`, and the C-API surface. **Layer 3 (`P3-W7`–`P3-W9`) is the platform and UI tiers**, which are largely independent of Layer 2 and can run in parallel with it. **`P3-W10` is the finale**: advance the oracle working tree, absorb the gate flips that advance causes (G1b goes RED for `rustty_vt_c` at 354 < 363; G3b's nine config false positives vanish by construction), and merge.

**Tech Stack:** Rust 1.93.1 (pinned, `rust-toolchain.toml`); Zig 0.15.2 locally (sufficient to compile the oracle's dependency-free LZ4 codec standalone — verified — despite upstream targeting 0.16); Python 3 for `.port/tools/gates/*.py`; Bash for the shell gates; `clang`/`clang++` from Xcode 26.5 for header and ABI probes; `cargo +nightly miri` for the SB/TB lanes; Node for `.port/tools/check-vt-struct-layouts.mjs`; macOS M4 Pro host for Metal/embedded/Swift (`xcodebuild test -project macos/Rustty.xcodeproj -scheme Rustty`); Fedora 44 aarch64 VM over `ssh rvm` for GTK/Wayland/X11; oracle git repo at `/Users/gael/projects/ghostty`, working tree pinned at `4749c4e93` until `P3-W10 T3`.

## Global Constraints (inherited from `2026-07-23-rustty-completion-ROADMAP.md`)

- **R1 full fidelity vs the oracle**, but the fidelity target for Phase 3 is `6ad1fe7d8`, not the pin. The oracle stays reference-only (R3) — no build-time dependency.
- **Port from the RANGE DIFF, never commit-by-commit.** Two traps in this range prove why: `bed47168c` ("bound POSIX read-ahead on non-Darwin") was **REVERTED** by `60121a039`, so a commit-by-commit porter ports discarded code; and `f2a7652ab`, whose subject reads as pure Zig-0.16 cleanup, **smuggled a real `omitFramePointer` change**. Where two commits rewrite the same function (`0ff4e41b2` then `d6e24d985`; `7e14347c1` then `fedd42e8d`), port the **end state** and carry **both** commits' regression tests.
- **R2 product identity:** rustty naming with per-token casing preservation (Ghostty→Rustty, ghostty→rustty). The `ghostty_*` C-ABI alias layer was deleted upstream; do not re-add aliases.
- **R4/R5:** acyclic crate graph (`check_acyclic.sh`); gates are invariant — **never weakened to pass**. Every gate change in `P3-W0` is additive (binds more, never less) and must ship a proof that its FAIL arm is reachable.
- **DG5:** C-ABI panic policy is ABORT via the ABI string (`extern "C"`), **not** per-function wrappers. No task in this plan proposes a wrapper.
- **Zig byte-slices → `Vec<u8>`/`&[u8]`, never lossy `String`** (memory: `zig-byteslice-to-rust-rawbytes`). `P3-W7 T2` exists because this rule was violated in `crates/rustty-os/src/env.rs`.
- **LTO stays banned.** Do **not** add the `libc` crate as a new dependency edge, and do not route `madvise` or any new syscall through it — hand-rolled `extern "C"` per the `crates/rustty-vt/src/mem.rs` precedent. ("`libc` is already a dependency of `rustty-vt`/`-os`/`-termio`" is **not** permission.)
- **`&mut Page` / `&mut Row` / `&mut [Cell]` / `&mut Screen` re-read across a call that mutates through another pointer is Stacked-Borrows UB** (memory: `rust-mut-args-strongly-protected`). The convention is raw-pointer args with narrowly-scoped `&mut` reconstruction. A long-lived raw alias into a field interleaved with `&mut` to that field must be **proven on an isolated non-FFI Miri harness**, not hand-reasoned (memory: `shaper-hook-sb-clean-interleave`; precedent `.port/phase-2/mirihar-chunk3b`).
- **Rust arithmetic is stricter than Zig in debug and laxer in release.** Zig traps only in safety builds; Rust `+=`/`-=` **panics in debug and wraps in release**. A "faithful" transcription of Zig arithmetic can therefore be genuinely *worse* in release. Every `@abs`/`+|`/`-|`/`std.math.add(...) catch` hunk becomes an explicit `unsigned_abs()`/`saturating_*`/`checked_*`. This is a standing **review class**, not a per-site judgement call — see `P3-W0 T7`.
- **Hash citations are worthless as a coverage proxy in BOTH directions.** Measured in area A: of 46 area-A commits with no hash citation in rustty source, **7 are fully ported**; and one *cited* family (`c442eb4b3`) is incomplete. **No task in this plan may use "no hash citation" as evidence of absence, or "hash cited" as evidence of presence.**
- **Every task states its NEGATIVE CONTROL** — the concrete mutation that must make its check fail. 5+ vacuous assertions have already been found in this program (`lz4_differential_exhaustive`; `compressed_page_requires_the_maximum_useful_scratch`'s first assertion; `bench_registers_compression_actions`; the `OUT_DIR/locale` nothing reads; `_vt_header_baseline.txt`). A guard that passes on unmodified code is worthless.
- **Severity requires reachability evidence.** Do **not** inherit the assessments' severity counts. Verified while authoring: area A's highest-severity item (`crates/rustty-vt/src/pagelist/pin.rs:494` `right_wrap` underflow) is **latent, not live** — all four callers (`crates/rustty-vt/src/selection_gesture.rs:910,915,925,930`) pass `n=1`, which cannot reach the `extra % cols == 0` branch. See `P3-W0 T6`.
- Ledger discipline: every task that closes a row strikes it **in the same commit**, in `.port/phase-2/MAIN_PARITY.md` and `.port/phase-2/DEFERRALS.md` both.
- Commit style: `subsystem: summary (P3-W<n> T<m>)`. Branch per workstream (`p3-w<n>-<slug>`); merge to `main` only with **both** platforms green (`cargo build --workspace` + `cargo test --workspace --no-fail-fast` on macOS AND `ssh rvm`).
- **DO NOT run `.port/tools/gates/no_homebrew_gettext_gate.sh`** — it mutates Homebrew state and has broken two concurrent lanes.
- **NEVER `git checkout` in the oracle repo** until `P3-W10 T3`, which is the *one* task licensed to do it and which requires that no other lane is active. Read beyond-pin state with `git -C /Users/gael/projects/ghostty show origin/main:<path>` / `git -C /Users/gael/projects/ghostty show <hash>`.
- Keep builds cheap: prefer `grep` and `cargo check -p <crate>` over `cargo build --workspace` inside a task; the full sweep belongs to each workstream's finale.

---

## PREMISE CORRECTION (read before Task 1)

### 1. Phase 3 must not start until Phase 2 closes, and must not inherit its debt

`docs/superpowers/plans/2026-07-29-ws19-phase2-closeout.md` is the gate. Phase 3 **executes after WS19 T20 Step 5** marks Phase 2 CLOSED in the ROADMAP. WS19 also **already owns** several repairs that a naive Phase 3 plan would duplicate; those are explicitly **out of scope here**:

| Repair | Owner | Do NOT re-plan |
|---|---|---|
| G1b (`check_test_counts.py`) counting `main`'s tests from a worktree | **WS19 T1** (N1) | ✔ |
| G1b BLOCKED because `~/.cache/zig/p/` was evicted (`restore_zig_deps.sh`) | **WS19 T1** (N4) | ✔ |
| `ci.sh` debug lane fail-fast masking later crates | **WS19 T1** (N5) | ✔ |
| `include/rustty.h` not compiling standalone (`Opaque`) + A5 compiling the FULL header as C11 **and** C++17 | **WS19 T3** (N2) | ✔ — this is area C's gate #2 and §4.6 |
| Two `gtk4::init()` calls in one test binary | **WS19 T4** (N3) | ✔ — this is area F's §5 pre-work |
| `example/*/build.sh` linking a stale `librustty_vt_c.dylib` | **WS19 T16** (N7) | ✔ — this is area C's gate #6 |
| Inspector cell-detail path restoring compressed pages | **WS19 T7** (O4) | ✔ — this is area F's `CellChooser` PARTIAL |
| `posix_waitpid` `ECHILD` + `threadEnter` drain ordering | **WS19 T17** (N9) | ✔ — this is area E §5.2/§5.3 |
| `style::Color` padding-free; `-C debug-assertions=off` vt Miri lane | **WS19 T5/T6** | ✔ |

**Phase 3's own gate-repair scope is therefore the remainder**, and it is: the missing LZ4 encoder byte-stream differential; A5's *one-directional* export check and its `type_json`-driven (rather than header-driven) layout arm; enum-discriminant parity; the closed-enum-across-ABI lint; `lz4_differential_exhaustive`'s false PASS; `rustty-bench`'s total absence from the gate suite; the two dead `_vt_*` artifacts; and the `GHOSTTY_SRC` env override that is itself the spoof-hole shape `ci.sh:114-129` killed for G1b. All of that is `P3-W0`.

### 2. The raw line count overstates the work by roughly an order of magnitude

`+37,683 / −11,323` across 507 files is **not** the size of this job. The dominant component is the Zig 0.16 stdlib migration (`e8525c0fd` plus its follow-ups `b988efcfe`, `f2a7652ab`, `a77c706a1`, `048619a6b`, `7121ab6c3`), which carries **no behaviour** and has no Rust analogue. Measured, per the assessments:

- **area E:** ~78 % of the area's churn.
- **area F:** **54 of 60** `src/font` files; 19 of 25 PNG regolds are `z2d.png_exporter` output churn (two pairs pixel-decoded and proven identical).
- **area G:** `e8525c0fd` alone is **694/394 of `pkg`'s 859/399** and **519/309 of `src/build`'s 905/319**; **2,142 of the 2,516** changed `po/` lines are `#:` reference renumbering, and the whole `po/` diff contains **exactly one** real translation (`+msgstr "关闭分屏"`).
- **wholly N/A directories:** all 46 `pkg/` files, all 13 `.github/` files, `nix/`, `flatpak/`, `build.zig.zon*`.
- **`src/quirks_memset.zig` (+338, NEW) is a deliberate non-port**: it exists because Zig 0.16 globally disabled LLVM loop auto-vectorization, turning compiler_rt's `memset` into a ~24× slower scalar loop. `rustc` lowers `@llvm.memset` to platform libc (`libSystem`/glibc IFUNC), which is already SIMD + `dc zva`. Porting it would *pessimize* rustty and risk ODR/interposition breakage. **But it makes the oracle ~2.8× faster on `+terminal-stream` inside this range** — see §6.

**Size the plan on the semantic count.** Reproduce it:

```sh
cd .port/phase-3/assessments
grep -h 'ABSENT\|PARTIAL' *.md | grep -oE '\b[0-9a-f]{9}\b' | sort -u > /tmp/ap.txt
grep -h 'PORTED'          *.md | grep -oE '\b[0-9a-f]{9}\b' | sort -u > /tmp/p.txt
comm -23 /tmp/ap.txt /tmp/p.txt | wc -l          # → 100
```

The seven assessments carry **161 ABSENT and 37 PARTIAL verdict rows**, which dedupe to **100 distinct upstream commits that appear on an ABSENT/PARTIAL line and on no PORTED line**. Five of those are context artifacts (`4749c4e93` is the pin itself; `c243d89bd`, `c9ad708ef`, `b988efcfe`, `048619a6b` appear as "rustty is at the `<hash>` shape" prose on PARTIAL rows and are themselves ported). **Net: ≈95 distinct absent semantic changes + ≈19 partials, against ~90 confirmed-ported.** The row count is ~1.7× the commit count purely because five commits (`446f80f4e`, `89b103dd5`, `6c8c07981`, `c594031d5`, `f4c68d65e`) are each verdicted independently by four or five areas — which is exactly why those five are the plan's atomic-landing units.

### 3. Ten of the seven assessments' coverage statements are SAMPLED, not exhaustive — and this plan treats that as debt, not coverage

Named by the authors themselves. Each gets a task in `P3-W0 T4` before its area's porting starts:

| Declared gap | Where | Why it must be closed first |
|---|---|---|
| `e8525c0fd`'s ~2,000 unsampled lines in `PageList/Screen/Terminal.zig` | A §1 "Coverage honesty" | A read the complete `page.zig` hunk set and **sampled** the other three |
| `src/build/SharedDeps.zig` (+293), `GhosttyLibVt.zig` (+203) not read line-by-line | G OQ-3 | `f2a7652ab` **proved** the hazard by smuggling `omitFramePointer` |
| ~50 mechanical `src/font` files classified from diff text without re-reading rustty | F §"Sampled" | 2 spot-checks clean is weak evidence |
| 21 of 25 PNG pairs not pixel-decoded | F §"Sampled" | the structural argument is good but unverified |
| `benchmark/HyperlinkMap.zig`, `ApcParser.zig` read surface-only | B §7 | hunk lists are *surface*-complete, not line-complete |
| `crates/rustty-bench/src/{screen_clone,terminal_stream}.rs` grepped, not read | B §7 | ditto |
| `src/Surface.zig`'s ~85 % mechanical residue pattern-verified, not read | E §0 | "a semantic change disguised inside a `lockUncancelable` hunk would be missed" |
| BL-1 codec re-vetted at hook/symbol level by A, line level by B | A §1, B §2 | resolved: **B's line-level verdict is authoritative** (byte-exact, 7,976 points) |
| `hash_map.zig` bodies assessed by neither A nor D at line level | A §1, D §2.1 | D owns them; the caller audit is the hard prerequisite |
| Area C's per-function body fidelity for the ~183 untouched exports | C §0 | out of range; explicitly accepted, not re-opened |

### 4. Two assessment claims did not survive verification, and one contradiction must be resolved by fiat

- **The `FAITHFULNESS QUIRK` sweep list in the task brief is wrong.** `grep -rln 'QUIRK' crates/` returns `rustty-renderer/src/metal/buffer.rs`, `metal/texture.rs`, `opengl.rs`, `opengl/buffer.rs`, `rustty-xev/src/{epoll,io_uring,dynamic}.rs`, `rustty-termio/tests/live_shell.rs` — **`crates/rustty-cli/src/help.rs` has no such comment**. And of the four renderer sites, only `src/renderer/metal/buffer.zig` appears in `.port/phase-3/UPSTREAM_DIFF_STAT.txt:402` (8 lines = `4a22eed6d`); `metal/texture.zig`, `opengl.zig` and `opengl/buffer.zig` are **not in the range at all**, so those three quirks are still faithful. `P3-W0 T5` is scoped to the grep-derived set, not the brief's list, and its expected answer is *one* fix and *three* re-affirmations.
- **Area A's highest-severity finding is latent.** See Global Constraints. `P3-W0 T6` makes per-item reachability a hard prerequisite for priority.
- **Contradiction: how to advance `GHOSTTY_SRC`.** Area G §7 recommends `git -C ../ghostty worktree add ../ghostty-head origin/main` and pointing `GHOSTTY_SRC` at it. **Rejected.** A redirectable oracle path is precisely the spoof hole `ci.sh:114-129` documents killing for G1b: the probe passes against a fake directory while the script counts `zig=0` everywhere → vacuous green. The resolution is to advance the **real** working tree (`P3-W10 T3`) and to **delete** `check_comment_coverage.py:34`'s `os.environ.get("GHOSTTY_SRC", …)` override so the tree cannot be spoofed (`P3-W0 T2 Step 6`). This is why `P3-W10 T3` is the one task that may `git checkout` in the oracle, and why it requires that **no other lane is active**.

### 5. Three bundles whose halves are vacuous — or worse than absent — alone

1. **Runtime scrollback limits: `65c48213b` + `1092204df` + `f4c68d65e` (+ `5b2d3b7df`, `86f81fb5b`, `10bc43420`, `03d5fa268`, `a27e04e8f`, `5fd2973b9`) land as ONE unit** (`P3-W3`). The config half alone ships a documented, completion-visible, C-ABI-exposed `scrollback-limit-lines` that **nothing enforces** — a shipped lie, strictly worse than absent. Note rustty's current `crates/rustty-config/src/config_struct.rs:4975` `scrollback_limit: 50_000_000` matches **neither** the pin (10 MB) nor head (`scrollback-limit-bytes`, `Limit(usize, 50_000_000)`): WS7 D2 lifted the value without the mechanism, and `config_struct.rs:2322-2323` still ships *"It is not currently possible to set an unlimited scrollback buffer. This is a future planned feature."* directly above a default that only exists because upstream added exactly that feature.
2. **`439d35e27` semantic-failure flag lands with its try/log re-partition** (`P3-W4 T5`). The flag without the `setAttribute`/`windowTitle`/`reportPwd` promotions (and *without* promoting `colorOperation`/`kittyColorOperation`/`clipboardContents`, which stay logged) is a flag that never fires.
3. **`c594031d5` cursor defaults land atomically with the code they DELETE** in `stream_terminal.zig` (−33) and `termio/stream_handler.zig` (−68), or DECSCUSR handling doubles up (`P3-W4 T4`).

### 6. Five findings that must be recorded before anyone re-measures anything

1. **The perf baseline is invalidated in the oracle's favour.** `20c3eae04`/`quirks_memset` made the Zig oracle ~2.8× faster on `+terminal-stream` inside this range. The `perf-baseline-rust-vs-zig` record (14/14 wins, suite −21.7 %) was measured against a pre-`quirks_memset` oracle. **Append the note before anyone re-benchmarks against `origin/main` and reads the shrunken margin as a rustty regression** (`P3-W0 T7 Step 4`). The oracle build still needs `-Doptimize=ReleaseFast`.
2. **G1b will flip RED for `rustty_vt_c` the moment the pin advances**: baseline 307 zig `test{}` in `src/terminal/c` → head **363**; rustty-vt-c has **354** `#[test]`. That is a genuine, useful signal. Plan for it (`P3-W10 T4`), do not be surprised by it.
3. **G3b's config row is currently 100 % false positives** — all 9 "missing" sentences are text the oracle *itself deleted* beyond the pin. Area G's own doc work pushes it 9 → ~39 **before** the finale fixes it. Advancing `.port/ORACLE_SYNC.md` alone changes nothing; the gate reads a directory.
4. **`fedd42e8d` is a sleeper.** Backward-shift deletion **moves entries on removal**, so "any key or value pointer previously returned by the map must be considered invalidated by any removal". In Zig that is a logic bug; in rustty's raw-pointer hyperlink/grapheme maps it is **UB**. A caller audit of `page.rs`/`page_impl.rs` is a **hard prerequisite**, not a follow-up (`P3-W2 T1`).
5. **`960c2cca5` must not be lost.** It fixes a `@memcpy` length mismatch in code that does not exist in rustty yet. It is `N/A`-until; whoever ports `e8525c0fd`(iii)'s kitty temp-**directory** medium must land the *fixed* form (bounds-check, then copy exactly `v.len`) with a PORT NOTE citing `960c2cca5` so a future reader cannot re-introduce it.

---

## Execution order and rationale

```
                    ┌── WS19 (Phase 2 closeout) MUST BE MERGED ──┐
                    ▼                                            │
P3-W0  instruments + evidence closure   ← BLOCKS EVERYTHING       │
   │                                                             │
   ├──► P3-W1  vt-core live-bug wave  (S/M, no coupling)          │
   │       │                                                     │
   │       ├──► P3-W2  page maps + capacity resilience            │
   │       │        │                                             │
   │       │        └──► P3-W3  runtime scrollback limits ────┐    │
   │       │                    (ONE unit, 6 crates)          │    │
   │       ├──► P3-W4  resize / cursor / visibility / DECRQSS ─┤    │
   │       └──► P3-W5  page::Mask → printSlice → render ───────┤    │
   │                    → stream fast paths → benches          │    │
   │                                                          ▼    │
   ├──► P3-W6  C-API surface + effects  (needs W3, W4, W5 heads)   │
   │                                                               │
   ├──► P3-W7  OS / termio / platform     ) independent of W2–W6,  │
   ├──► P3-W8  UI tiers (macOS, renderer,  ) run in PARALLEL       │
   │           font, apprt, hotkeys)       )                       │
   └──► P3-W9  config / CLI / build / i18n / synthetic ────────────┘
                                    │
                                    ▼
                    P3-W10  FINALE — advance the oracle WORKING TREE
                            (no other lane may be active)
```

**Why this order:**

1. **`P3-W0` first because the instruments are dishonest.** Porting a compression change into a suite with no encoder byte-stream guard, or a new export into a one-directional symbol check, produces green that means nothing. The two absent `render_state_*` exports are the existence proof.
2. **`P3-W1` before every large feature** because it is the highest value-per-line in the range: it is where the *live* bugs are, it is all S/M, it touches no other area, and it changes no signature — so it cannot conflict with the renames in `P3-W3`.
3. **`P3-W2` before `P3-W3`** because `7e14347c1` moves `Page`'s layout byte size, which moves `page.rs`'s capacity math and every size golden. Doing that after the scrollback-limit renames means redoing both.
4. **`P3-W3` is a wall.** It contains two mechanical mega-renames that must each land as a single atomic commit — `PageList::init(cols, rows, max_size)` → `init(Options)` (**256 call sites, 8 files**) and `max_scrollback` → `max_scrollback_bytes` (**204 occurrences, 45 files, 9 crates**). Anything landing concurrently in those files conflicts. `P3-W4` and `P3-W5` are sequenced *before or beside* it deliberately, not after.
5. **`P3-W5` starts with `page::Mask` alone** because it is the shared dependency of `Terminal::print_slice_fill` (area A), `render.zig`'s group scans (area D), `renderer/generic.rs` (area F) and `include/rustty/vt/render.h` (area C). Ported once in area A with a raw-base API; otherwise four areas hand-roll masked compares and diverge.
6. **`P3-W7`–`P3-W9` run in parallel** with Layer 2: their only inbound edges are call sites (`Termio.init`'s limits, `stream_handler`'s slice arms, the inspector's limit rows), each of which is an S-cost tail that can land with its blocking workstream.
7. **`P3-W10` last and alone.** Advancing the working tree is the only irreversible act in the plan and the standing "never checkout the oracle" rule exists because concurrent lanes depend on that tree.

**Task inventory:** 11 workstreams, **73 tasks**, 426 checkbox steps.

| WS | Scope | Tasks | Blocked on |
|---|---|---|---|
| `P3-W0` | Instruments + evidence closure | 7 | WS19 merged |
| `P3-W1` | vt-core live-bug wave | 6 | W0 |
| `P3-W2` | Page maps + capacity resilience | 4 | W1 |
| `P3-W3` | Runtime scrollback limits (one unit) | 6 | W2 |
| `P3-W4` | Resize / cursor / visibility / DECRQSS | 7 | W1 |
| `P3-W5` | `page::Mask` → print → render → stream | 9 | W1 |
| `P3-W6` | C-API surface + effects | 7 | W3/W4/W5 heads |
| `P3-W7` | OS / termio / platform | 8 | W0 |
| `P3-W8` | UI tiers | 8 | W0 |
| `P3-W9` | config / CLI / build / i18n / synthetic | 7 | W0 |
| `P3-W10` | Finale — advance the pin | 4 | all |

---

# `P3-W0` — Instruments and evidence closure (**do this first**)

**Branch:** `p3-w0-instruments`. Nothing in this workstream ports oracle behaviour; it makes the
subsequent 62 tasks' evidence trustworthy.

## Task 1: The LZ4 encoder byte-stream differential gate (**G-LZ4**)

**Rationale.** Area B **proved** rustty's codec is byte-exact with `origin/main` across 7,976
differential points (8 corpus families × 35 boundary sizes; 4,000 randomized triples; every match
period 1…96; 3,600 corrupted decodes) — and **also proved that byte-exactness is unprotected**.
Fault injection against `lz4_differential_light` caught 4 of 7 faults and missed 3:
`copy_match` memcpy threshold 64→8, `advance_search` 1024→512, and `HASH_LOG` 12→11. The last one
was measured to diverge **1,929 of 7,976** encoded blocks while the entire rustty suite stayed green.
Upstream's own `src/terminal/compress/AGENTS.md` mandates this coverage — *"When a change should not
alter compressor output, prove it… Ratio drift is a functional change, not noise"* — and that file
is **not ported**. This is the single highest-value gate in Phase 3 and it must land **before or
alongside any compression change**.

- [ ] **Step 1: RED — reproduce the blind spot before building the gate**

```sh
cargo test -p rustty-vt --lib --release compress::            # expect 19/19 pass
sed -i.bak 's/const HASH_LOG: u32 = 12/const HASH_LOG: u32 = 11/' crates/rustty-vt/src/compress/lz4.rs
grep -n 'HASH_LOG' crates/rustty-vt/src/compress/lz4.rs | head -3
cargo test -p rustty-vt --lib --release compress::            # MUST STILL PASS — that is the finding
cargo test -p rustty-bench --lib --release                     # MUST STILL PASS
mv crates/rustty-vt/src/compress/lz4.rs.bak crates/rustty-vt/src/compress/lz4.rs
```

Record both "still pass" results in the commit body. **If either fails, stop** — the premise has
changed and the gate design must be re-derived.

- [ ] **Step 2: Write `.port/tools/gates/check_lz4_byte_exact.sh`**

Contract, in the script header:

1. `git -C /Users/gael/projects/ghostty show origin/main:src/terminal/compress/lz4.zig` and
   `…:src/terminal/compress/lz4_differential.zig` into a temp dir (**read-only; never `checkout`**).
2. Build the Zig arm: `zig build-exe harness.zig -O ReleaseSafe`. The codec depends only on `std`
   (upstream's own `compress/AGENTS.md` documents standalone compilation as supported), and it
   **compiles under the locally installed Zig 0.15.2** — verified — so the gate does not need a 0.16
   toolchain.
3. Build the Rust arm: `rustc -O -C debug-assertions=on` over a harness that `#[path]`-includes
   `crates/rustty-vt/src/compress/lz4.rs` verbatim (it has no dependencies beyond `core`/`std`).
4. Both harnesses implement the **identical deterministic corpus generator** and print
   `family,size,seed,encoded_len,fnv1a(encoded)` per point; phase 4 prints the exact
   error-variant name or `decoded_len,fnv1a(decoded)`.
5. Compare; non-empty diff ⇒ **exit 1**, printing the diverging `(family,size,seed)` rows and the
   total count. Oracle checkout absent ⇒ **exit 3 BLOCKED**, never a silent green.
6. **Anti-vacuity guard:** if either arm emits **zero** rows, FAIL LOUDLY.

- [ ] **Step 3: NEGATIVE CONTROL — a `--self-test` arm that must reproduce 1,929**

```sh
bash .port/tools/gates/check_lz4_byte_exact.sh --self-test; echo "must be non-zero: rc=$?"
```

`--self-test` copies `lz4.rs` to the temp dir, patches `HASH_LOG` 12 → 11 in **the copy only**, and
requires the gate to exit non-zero reporting **exactly 1,929** diverging rows out of 7,976. A
different count means the corpus generator diverged from area B's and the gate's sensitivity is
unknown — fix the generator, do not adjust the expected number. Also assert the three faults
`lz4_differential_light` *does* catch still fail there, so the two guards are proven
complementary rather than overlapping.

- [ ] **Step 4: Register G-LZ4 in `.port/tools/ci.sh`**

Add to `GATE_REGISTRY` (`.port/tools/ci.sh:149`), next to D3 whose subject matter it extends from
derived constants to emitted bytes:

```sh
  # G-LZ4 — the LZ4 encoder BYTE STREAM matches the oracle's, not merely round-trips.
  # `lz4_differential_light` is a real guard for format conformance and every decoder
  # bounds/margin path (4 of 7 injected faults caught, measured) but is BLIND to
  # match-finder heuristics: HASH_LOG 12->11 diverges 1,929/7,976 encoded blocks with the
  # whole suite green. Upstream's compress/AGENTS.md mandates exactly this proof.
  # PROGRESS while it beds in; its FAIL arm is proven by --self-test (1,929 rows).
  "G-LZ4|PROGRESS|bash $GATES/check_lz4_byte_exact.sh"
```

- [ ] **Step 5: Port `src/terminal/compress/AGENTS.md`**

`git -C /Users/gael/projects/ghostty show origin/main:src/terminal/compress/AGENTS.md` → 90 lines →
`crates/rustty-vt/src/compress/AGENTS.md` (precedent: `crates/rustty-vt/src/apc/glyph/AGENTS.md`).
Token-rename per `.port/tools/PARITY_NAMING.md`. It is the document that *specifies* this gate; add
one rustty-specific paragraph naming `check_lz4_byte_exact.sh` as the realization of its
ratio-drift rule.

- [ ] **Step 6: Commit**

```sh
git commit -m "gates: G-LZ4 proves the LZ4 encoder byte stream matches the oracle, with a 1,929-row self-test FAIL arm; port compress/AGENTS.md (P3-W0 T1)"
```

---

## Task 2: A5 must bind in both directions, be header-driven, and check enum discriminants

**Rationale.** Four structural holes, each with a demonstrated consequence.
**(a) A5(b) is one-directional.** It builds `vt_decl` (lowercase `rustty_*` tokens in
`include/rustty/vt*`) ∩ `vt_cbindgen` (same grep over `include/rustty.h`) and requires
`intersection ⊆ exports`. A symbol upstream has that is missing from **both** the header and the
crate is invisible — which is exactly how `rustty_render_state_begin_update` and `_end_update`
hid. It is also **vacuous-shrink-prone**: a symbol present in `vt.h` but absent from cbindgen output
silently shrinks the intersection instead of failing. (Reproduced statically: `vt_decl` = 188,
`vt_cbindgen` = 183, intersection = **183** — non-vacuous *today*, which is precisely the property
that must be asserted rather than assumed.)
**(b) A5(c) is `type_json`-driven, not header-driven** (`.port/tools/check-vt-struct-layouts.mjs`):
a struct typedef'd in the header but never registered in `crates/rustty-vt-c/src/types.rs` gets
**zero** assertions, silently — and `P3-W6 T3` is about to add two such typedefs. Its success line
also prints only the field-offset count, so a run that produced only `sizeof` asserts reads as
`0 field offsets … OK`.
**(c) There is no analogue of upstream's `checkGhosttyHEnum`** — nothing compares a `#[repr(C)]`
enum's discriminants to the header's enumerator values, and `P3-W3`/`P3-W6` are about to add
`OPT_SCROLLBACK_MAX_BYTES = 27`, `DATA_… = 33..35`, `OPT_… = 29/30`.
**(d) `_vt_header_baseline.txt` is a dead artifact that has already been mis-cited as coverage.**

- [ ] **Step 1: RED — prove hole (a) with the two known-absent symbols**

```sh
bash .port/tools/gates/check_rustty_vt_headers.sh; echo "rc=$? (expect 0 — the hole)"
git -C /Users/gael/projects/ghostty show origin/main:src/lib_vt.zig \
  | grep -oE 'name = "ghostty_[A-Za-z0-9_]+"' | sed 's/.*"ghostty_/rustty_/;s/"//' | sort -u > /tmp/oracle_vt.txt
grep -rhoE 'extern "C" fn rustty_[A-Za-z0-9_]+' crates/rustty-vt-c/src | sed 's/.*fn //' | sort -u > /tmp/rustty_vt.txt
comm -23 /tmp/oracle_vt.txt /tmp/rustty_vt.txt
```

Expected: A5 green, and the `comm` prints exactly `rustty_render_state_begin_update` and
`rustty_render_state_end_update`. That contrast **is** the proof.

- [ ] **Step 2: Add A5(d) — oracle vt-export parity, both directions**

Extend `.port/tools/gates/check_rustty_vt_headers.sh`. The **oracle** drives the loop (so the arm
cannot go vacuous by rustty shrinking its own artifacts): extract the export set from
`$ORACLE_TREE/src/lib_vt.zig`'s `@export(&…, .{ .name = "ghostty_…" })` block, apply the
`ghostty_`→`rustty_` map from `.port/tools/parity_names.env`, and diff **both** directions against
rustty's live `nm` ∪ wasm exports. Print both missing sets; exit 1 if either is non-empty; **exit 3
BLOCKED if the oracle checkout is absent.** Do **not** add an env override for the oracle path
(`ci.sh:114-129`).

Expected today: **RED, naming the 2 `render_state_*` symbols.** Land it RED as the tracker for
`P3-W5 T7`; `run_gate`'s PROGRESS-went-RED branch escalates it to a HARD failure once green, so it
cannot silently rot afterwards.

- [ ] **Step 3: Add A5(b′) — kill the vacuous-shrink hazard**

Assert the intersection's cardinality is **≥ a floor recorded in the script** (183 today) and that
`vt_decl \ intersection` equals exactly the 5 known non-ABI tokens — `rustty_mode_new`,
`rustty_mode_ansi`, `rustty_mode_value` (function-like macros) and the two prose artifacts
`rustty_osc_` / `rustty_vt_osc_free` (`include/rustty/vt/osc.h:88,99`). **`rustty_vt_osc_free` is a
doc typo inherited verbatim from upstream `ghostty_vt_osc_free` — R1-faithful; do NOT "fix" it**,
allowlist it with that reason.

- [ ] **Step 4: Add A5(c′) — make the layout arm header-driven**

Enumerate `typedef … } Rustty…;` aggregates from `include/rustty/vt/**` and **require** each to be a
key of `rustty_type_json()`. "In the header, absent from `type_json`" becomes **FAIL**, not skip.
Print the `sizeof` assert count and the `offsetof` assert count **separately**, and FAIL if either
is zero.

- [ ] **Step 5: Add the enum-discriminant parity arm**

For every `Rustty…` enum in the curated headers, emit `_Static_assert((int)RUSTTY_X_Y == <n>, …)`
from the Rust `#[repr(C)]` discriminants and compile. This is the missing analogue of upstream's
`checkGhosttyHEnum` (which validates every Zig enum field against the matching macro translated out
of the real header — `src/lib/enum.zig`).

- [ ] **Step 6: Delete the dead artifacts and the `GHOSTTY_SRC` spoof hole**

```sh
grep -rn '_vt_header_baseline\|_vt_mismatch_report' .port/ xtask/ Makefile 2>/dev/null   # expect: no readers
git rm .port/tools/gates/_vt_header_baseline.txt .port/tools/gates/_vt_mismatch_report.txt
```

The baseline is a 156-symbol list frozen 2026-07-15 against 183 live exports, and its companion
report is its stale output. **An unread "baseline" is worse than none, because it invites a planner
to cite it as coverage — which is exactly what happened.**

In the same commit, change `.port/tools/gates/check_comment_coverage.py:34` from
`GHOSTTY_SRC = os.environ.get("GHOSTTY_SRC", "/Users/gael/projects/ghostty/src")` to a plain
non-overridable constant, with the comment citing `ci.sh:114-129`: an env-redirectable oracle path
lets the gate pass against a fake tree while it reports zero reference sentences everywhere.
`P3-W10 T3` advances the real tree instead. Do the same in
`.port/tools/gates/check_comments_fidelity.py` if it carries the same override.

- [ ] **Step 7: NEGATIVE CONTROL for each new arm**

```sh
# (b′) shrink the intersection: delete a decl from a curated header -> must FAIL, not shrink-pass
cp include/rustty/vt/color.h /tmp/color.h.bak
sed -i '' '/rustty_color_luminance/d' include/rustty/vt/color.h
bash .port/tools/gates/check_rustty_vt_headers.sh; echo "must be non-zero: rc=$?"
cp /tmp/color.h.bak include/rustty/vt/color.h

# (c′) typedef an aggregate with no types.rs row -> must FAIL
printf '\ntypedef struct { size_t size; } RusttyP3SelftestAggregate;\n' >> include/rustty/vt/terminal.h
bash .port/tools/gates/check_rustty_vt_headers.sh; echo "must be non-zero: rc=$?"
git checkout include/rustty/vt/terminal.h

# enum arm: perturb one Rust discriminant -> must FAIL
# (do it on a scratch copy of the enum in a #[cfg(test)] shim; revert immediately)
```

Record all four exit codes. Any arm that does not flip is not a gate.

- [ ] **Step 8: Commit**

```sh
git commit -m "gates: A5 gains oracle-export parity (both directions), a header-driven layout arm, enum-discriminant asserts, and a vacuous-shrink floor; delete the dead _vt_* artifacts and the GHOSTTY_SRC override (P3-W0 T2)"
```

---

## Task 3: The closed-enum-across-ABI lint (**D10**)

**Rationale.** The program's rule is: **inbound** C integers narrow via an explicit `match`; values
with no narrowing point get `#[repr(transparent)]`. Four exports violate it by declaring the *Rust
enum itself* as the C parameter type, so an invalid value is materialized at the ABI boundary
**before** any `match` can run — instant UB on an out-of-range tag. Every one of these enums is
declared in the header with a `…_MAX_VALUE = RUSTTY_ENUM_MAX_VALUE` enumerator, i.e. the header
**documents an out-of-range value as writable by a conforming C caller** (verified:
`include/rustty/vt/terminal.h:206,223,255,297,317,368,445,890,1200`).

| export | offending param | type | in this range? |
|---|---|---|---|
| `rustty_terminal_scroll_viewport` (`crates/rustty-vt-c/src/lib.rs:2494`) | `behavior.tag` in the by-value `ScrollViewportC` | `ScrollViewportTag` (`terminal.rs:1502`, 0..=3) | **YES** — `3a2e28329` added `Row = 3` |
| `rustty_focus_encode` (`lib.rs:278`) | `event` | `focus::FocusEvent` (`focus.rs:28`, 0..=1) | no (pre-existing) |
| `rustty_sys_log_stderr` (`lib.rs:346`) | `level` | `sys::LogLevel` (`sys.rs:44`, 0..=3) | no (pre-existing) |
| `rustty_size_report_encode` (`lib.rs:453`) | `style` | `size_report::Style` (`size_report.rs:29`, 0..=3) | no (pre-existing) |

`rustty_mode_report_encode` **already does it correctly** via `pub struct ReportState(pub c_int)`
(`crates/rustty-vt-c/src/modes.rs:26`, with the PORT NOTE at `:19` explaining the oracle's open
`enum(c_int) { _ }`), which proves the other four are oversights and not policy. Do **not**
misattribute the three pre-existing ones to this range.

- [ ] **Step 1: Write `.port/tools/gates/check_closed_enum_abi.sh`**

Grep exported `extern "C" fn` signatures in `crates/rustty-*-c/src` for parameter types that resolve
to a **non-`repr(transparent)` fieldless `#[repr(C)]` enum** — directly, or as a field of a by-value
`#[repr(C)]` struct parameter. Require an allowlist entry with a written justification for each hit.
Exit 1 on an unlisted hit.

- [ ] **Step 2: NEGATIVE CONTROL — it must be RED on today's tree**

```sh
bash .port/tools/gates/check_closed_enum_abi.sh; echo "must be non-zero today: rc=$?"
```

It must name all four exports. **A gate that lands green is unproven**; this one is provably
non-vacuous the day it lands. Register it PROGRESS in `ci.sh`; promote to HARD after `P3-W6 T2`
sweeps the four sites.

- [ ] **Step 3: Commit**

```sh
git commit -m "gates: D10 flags closed #[repr(C)] enums received by value across the C ABI; RED on 4 known exports (P3-W0 T3)"
```

---

## Task 4: Close the ten declared-sampled coverage gaps

**Rationale.** PREMISE CORRECTION §3. Each of these is an assessment author's own statement that
they did not have the coverage they might appear to claim. Treating sampling as coverage is how
`f2a7652ab` would have smuggled `omitFramePointer` past this program.

- [ ] **Step 1: `e8525c0fd`'s unsampled ~2,000 lines in area A's three big files**

```sh
git -C /Users/gael/projects/ghostty show e8525c0fd -- \
  src/terminal/PageList.zig src/terminal/Screen.zig src/terminal/Terminal.zig > /tmp/a_016.diff
grep -c '^[+-]' /tmp/a_016.diff
# Filter OUT the mechanical classes, then read EVERYTHING that survives:
grep '^[+-]' /tmp/a_016.diff | grep -vE 'std\.|posix\.|windows\.|@(Type|Struct|Enum|Union|Int|Fn)|ArrayList|Io\.|testing\.io|\.data\b' > /tmp/a_016_residue.txt
wc -l /tmp/a_016_residue.txt
```

Read every surviving line **in context**. Looking specifically for: changed **constants**, changed
**control flow**, changed **error-handling arms**. Record the residue line count and the verdict per
surviving hunk in `.port/phase-3/SAMPLING_CLOSURE.md` (new file). Area A read the complete `page.zig`
hunk set and found it all mechanical (`posix.PROT` bitfield form, `windows.exp.kernel32`,
`content.codepoint` → `packed struct(u24){data,_pad}`) — that is the expected shape, but it is not
yet evidence for the other three files.

- [ ] **Step 2: `src/build/SharedDeps.zig` + `GhosttyLibVt.zig` line-by-line (~1 hour)**

```sh
git -C /Users/gael/projects/ghostty show f2a7652ab -- src/build/SharedDeps.zig src/build/GhosttyLibVt.zig
git -C /Users/gael/projects/ghostty show e8525c0fd -- src/build/SharedDeps.zig
```

Justified because `f2a7652ab` **proved** the hazard. Expected outcome: nothing hiding; but "I do not
believe anything is hiding there" is not this program's evidence standard. Record in
`SAMPLING_CLOSURE.md`.

- [ ] **Step 3: Area F's ~50 mechanical font files and 21 undecoded PNG pairs**

```sh
git -C /Users/gael/projects/ghostty diff 4749c4e93..origin/main -- src/font \
  | grep '^[+-]' | grep -vE 'std\.(Thread|Io|fs|meta)|ArrayListUnmanaged|@(Type|Struct|Union|Fn|Int|Enum)|fixedBufferStream|@cImport|testing\.io' > /tmp/f_residue.txt
wc -l /tmp/f_residue.txt
```

Then pixel-decode the remaining 21 golden pairs (Pillow; the harness pattern is in area F §"Sampled").
**Expected:** only the 6 files in `d0e72a3ab`'s stat differ in pixels
(`U+2500…{11x21+2,9x17+1}`, `U+E000…{11x21+2,12x24+3,18x36+4,9x17+1}`); the other 19 are
`z2d.png_exporter` output churn. **If a seventh file differs in pixels, `d0e72a3ab` is not the only
rendering change in the range** and area F's structural argument collapses — escalate.

- [ ] **Step 4: Read `benchmark/HyperlinkMap.zig` and `ApcParser.zig` in full; read rustty's two partial benches in full**

```sh
git -C /Users/gael/projects/ghostty show origin/main:src/benchmark/HyperlinkMap.zig   # 155 lines
git -C /Users/gael/projects/ghostty show origin/main:src/benchmark/ApcParser.zig      # 144 lines
```

Then read `crates/rustty-bench/src/screen_clone.rs` and `terminal_stream.rs` end to end. Area B's
hunk lists are surface-complete; `P3-W5 T8/T9` need them line-complete.

- [ ] **Step 5: Read `src/Surface.zig`'s mechanical residue in full (~30 min)**

```sh
git -C /Users/gael/projects/ghostty diff 4749c4e93..origin/main -- src/Surface.zig > /tmp/surface.diff
grep -c '^[+-]' /tmp/surface.diff        # 904 changed lines, ~85% mechanical
```

Area E's own note: *"a semantic change disguised inside a `lockUncancelable` hunk would be missed"*,
and `src/Surface.zig` is where `6c8c07981`'s occlusion rewrite lives — a hunk that **changes existing
behaviour** (`if (self.visible == visible) return;` means repeated same-state occlusion callbacks no
longer push a renderer message). That is not additive-only, so this file's residue matters.

- [ ] **Step 6: NEGATIVE CONTROL for the sweep itself**

Insert a synthetic semantic change into the *filter input* and confirm the filter does not swallow it:

```sh
printf -- '-    const threshold = 1024;\n+    const threshold = 512;\n' >> /tmp/a_016.diff
grep '^[+-]' /tmp/a_016.diff | grep -vE 'std\.|posix\.|windows\.|@(Type|Struct|Enum|Union|Int|Fn)|ArrayList|Io\.|testing\.io|\.data\b' | tail -2
```

The two synthetic lines **must** appear in the residue. A filter that hides a constant change is
worse than no filter. Revert `/tmp/a_016.diff` afterwards.

- [ ] **Step 7: Commit**

```sh
git commit -m "port: close the ten declared-sampled coverage gaps with a per-hunk residue verdict (P3-W0 T4)"
```

---

## Task 5: The faithfully-carried-oracle-bug sweep

**Rationale.** rustty deliberately reproduces some oracle bugs, documents them as
`FAITHFULNESS QUIRK`, and **no gate can see when upstream fixes one**. This is the only failure
class in the program that is invisible in *both* directions: the quirk comment asserts
intentionality, so a reviewer reads it and moves on.

- [ ] **Step 1: Derive the site set by grep, not from any list**

```sh
grep -rn 'FAITHFULNESS QUIRK\|latent reference quirk\|NOT corrected' crates/
grep -rln 'QUIRK' crates/
```

Authoring-time result (**the task brief's list is wrong** — `crates/rustty-cli/src/help.rs` has no
such comment):

```
crates/rustty-renderer/src/metal/buffer.rs:155,224
crates/rustty-renderer/src/metal/texture.rs:222
crates/rustty-renderer/src/opengl.rs:101
crates/rustty-renderer/src/opengl/buffer.rs:82,177
crates/rustty-xev/src/{epoll,io_uring,dynamic}.rs
crates/rustty-termio/tests/live_shell.rs
```

- [ ] **Step 2: Check EACH site's oracle file against the range**

```sh
grep -nE 'renderer/(metal/(buffer|texture)|opengl(/buffer)?)\.zig' .port/phase-3/UPSTREAM_DIFF_STAT.txt
```

Authoring-time result: only `src/renderer/metal/buffer.zig` appears
(`.port/phase-3/UPSTREAM_DIFF_STAT.txt:402`, 8 lines = `4a22eed6d`). `metal/texture.zig`,
`opengl.zig`, `opengl/buffer.zig` are **not in the range**, so those three quirks are still faithful
— **re-affirm them in place with a one-line note recording that they were checked against
`4749c4e93..6ad1fe7d8` and the oracle file is untouched.** The xev quirks live in `pkg/libxev`, which
is a dependency bump, not a source file in this diff; record the same for them (and note the libxev
bump is `P3-W9 T6`'s to re-check).

- [ ] **Step 3: Fix `metal/buffer.rs` — and DELETE the comment, do not amend it**

`4a22eed6d` fixes a 2× `sizeof` over-allocation in `Buffer.sync`/`syncFromArrayLists`: the old code
computed `size = req_bytes * 2` then passed `size * @sizeOf(T)`, i.e. `data.len * sizeof(T)² * 2`
— **64× for the 32-byte `CellText` buffers, 8× for `CellBg`, per swap-chain frame** (~9.4 MB instead
of ~300 KB for a 120×40 screen). The fix tracks the length in units of `T` (`self.len = data.len * 2`),
which also repairs a **second latent bug**: `self.len` was never updated on realloc. Both halves are
required. `crates/rustty-renderer/src/metal/buffer.rs:150-161` and `:219-226`.

**The PORT NOTE at `:155-160` must be removed, not edited.** Leaving "quirk preserved deliberately"
text next to the fix would mislead the next reader into re-introducing it.

- [ ] **Step 4: NEGATIVE CONTROL — a headless Metal unit test**

`MTLCreateSystemDefaultDevice()` works in a headless test process on the macOS dev host, so this is
**not** un-gateable GPU work. `#[cfg(target_os = "macos")]`:

```
build a Buffer<CellText> at len 1
sync a 100-element slice
assert buffer.length() == 200 * size_of::<CellText>()   // NOT 200 * size_of²
assert self.len == 200
sync a 150-element slice
assert NO realloc happened (length() unchanged)          // the stale-`len` half
```

The second assertion is the one the stale-`len` bug broke, so the test fails on today's code for
**two independent reasons** — that is the non-vacuity proof.

- [ ] **Step 5: Institutionalize the check**

Add to `.port/tools/gates/check_stale_comments.sh`'s ledger (`stale-comments-D9.tsv`) one row per
quirk site recording the oracle path + the SHA range it was last reconciled against, so the next
resync sweep is a diff rather than a rediscovery.

- [ ] **Step 6: Commit**

```sh
git commit -m "renderer/metal: fix the 2x sizeof buffer over-allocation and the stale len on realloc (oracle 4a22eed6d); re-affirm the 6 remaining quirk sites against the range (P3-W0 T5)"
```

---

## Task 6: Per-item caller-reachability triage (produces `P3-W1`'s priority order)

**Rationale.** The assessments assign severity without uniformly proving reachability, and the one
item spot-checked at authoring time did not survive: `crates/rustty-vt/src/pagelist/pin.rs:494`
`right_wrap`'s `extra % cols - 1` underflow is called "the highest-severity single finding in area A"
— and it is **latent**, because all four callers pass `n=1`:

```
crates/rustty-vt/src/selection_gesture.rs:910  click_pin.left_wrap(1)
crates/rustty-vt/src/selection_gesture.rs:915  click_pin.right_wrap(1)
crates/rustty-vt/src/selection_gesture.rs:925  drag_pin.right_wrap(1)
crates/rustty-vt/src/selection_gesture.rs:930  drag_pin.left_wrap(1)
```

`n=1` cannot reach the exact-row-multiple branch. It is still a real bug and still gets fixed — but
it is **not** the item to schedule first, and shipping it as a "live crash hotfix" would misstate the
evidence.

- [ ] **Step 1: Build the reachability table**

Create `.port/phase-3/REACHABILITY.md` with one row per claimed-live defect. Required columns:
`rustty site | claimed trigger | ALL callers (file:line) | argument domain at each caller | reachable from a public API? | verdict {LIVE, LATENT, UNREACHABLE} | evidence`.

Seed set (area A §3.2 plus area D §1):

| site | claimed trigger | must verify |
|---|---|---|
| `pagelist/pin.rs:494` `right_wrap` | exact row multiple | **done: LATENT** (4 callers, all `n=1`) |
| `pagelist/pin.rs:468` `left_wrap` | symmetric | same 4 callers |
| `pagelist/iter.rs:489` reverse `.count` | `row.y == 0` → `row.y - 1` | find every `Limit::Count` reverse-direction caller; `iter.rs:476-491` |
| `pagelist/scroll.rs:253,346` | `isize::MIN` delta | is `isize::MIN` reachable from `scroll(.delta_row)` / `.delta_prompt` public callers? |
| `terminal.rs:970` `debug_assert!(!brk)` | mode 2027 off → base+U+200B → 2027 on → U+0301 | drive it through `Stream` end-to-end, not by calling `print` directly |
| `pagelist/grow.rs:495` `adjust_field!` | `increase_capacity` from a zero capacity dimension | precondition is `compact` producing exact capacities; rustty has both `compact` (`grow.rs:610`) and `exact_row_capacity` (`page_impl.rs:898`) |
| `pagelist/access.rs:1095` `screen_point` | scrollback > 65,535 physical rows | is a 65k-row scrollback reachable under the current default limit? |
| `search/sliding_window.rs:1165` empty needle | `ascii_index_of_ignore_case` returns `Some(0)` | **claimed strictly worse than the pin** — verify against Zig's `std.ascii.indexOfIgnoreCase` returning `null` |
| `search/screen.rs:822,1409` cached-highlight walk | resize between feeds frees nodes | the dangling-node window; area D calls this its highest-severity gap |

- [ ] **Step 2: For every row marked LIVE, write the reproducer FIRST**

A reproducer is a `#[test]` that fails on today's `main` (a panic in debug, or a Miri error). No
LIVE verdict without one. Add each as `#[ignore = "P3 reachability reproducer — un-ignore with the fix"]`
so the debt is visible in the test list rather than only in a document.

- [ ] **Step 3: NEGATIVE CONTROL — prove the LATENT verdicts are falsifiable, not convenient**

For each LATENT row, add a `#[test]` that calls the function **directly** with the out-of-domain
argument and asserts the panic. It must fail on today's code (proving the defect is real) while the
public-API path does not (proving the LATENT verdict). If the direct test **passes**, the defect
analysis is wrong — escalate rather than downgrading the row.

`pagelist/grow.rs:495` deserves particular care: its consequence is not a panic but an **infinite
loop allocating a page per iteration** in the unbounded-retry callers (`start_hyperlink`,
`cursor_set_hyperlink`, the reflow probes, `insert_lines`/`delete_lines`), and **silent data loss**
in the single-retry style/grapheme paths. A test for that needs a bounded iteration counter, not a
`#[should_panic]`.

- [ ] **Step 4: Reorder `P3-W1` by the table, not by the assessments**

`P3-W1`'s task order is whatever `REACHABILITY.md` says is LIVE, cheapest-first. Record the delta
against area A §4's "Wave A1" recommendation, with reasons.

- [ ] **Step 5: Commit**

```sh
git commit -m "port: per-item caller-reachability triage for the 9 claimed-live vt defects; pin.rs right_wrap is LATENT, not live (P3-W0 T6)"
```

---

## Task 7: Standing review classes — release arithmetic, `rustty-bench` gating, `cfg(test)` transitivity, perf-baseline invalidation

**Rationale.** Four cross-cutting hazards that are not any one commit's, and would otherwise be
re-litigated per site.

- [ ] **Step 1: Write `.port/phase-3/REVIEW_CLASSES.md` — the release-arithmetic class**

Content, as a binding rule for every task in this plan: Zig's arithmetic traps **only in safety
builds**; Rust's **panics in debug and wraps in release**. Therefore a byte-faithful transcription of
a Zig `+`/`-`/`*` can be *worse than the oracle in release*. Every `@abs` → `unsigned_abs()`; every
`+|`/`-|` → `saturating_*`; every `std.math.add(...) catch` → `checked_*` with the oracle's fallback.
The four `P3-W1` arithmetic fixes (`c753fe4a4`, `afbf5ba15`, `e6e4a9fdc`, `30b42f42a`) are instances,
not exceptions. Cross-reference `crates/rustty-vt/src/compress/lz4.rs`'s handling as the exemplar:
`match_end_position` replaces the oracle's trapping `limit - position` with
`limit.saturating_sub(position)` **plus** a `debug_assert!(position <= limit)`, so the saturating form
cannot silently absorb an invariant break. That pairing — saturate **and** assert — is the required
shape, not one or the other.

- [ ] **Step 2: Put `rustty-bench` into a gate**

```sh
grep -rn 'rustty-bench' .port/tools/gates/ .port/tools/ci.sh    # authoring-time result: nothing
```

`rustty-bench` is in **no gate at all**, which is why four benchmark findings (2 wholly absent files,
2 partials) produced zero automated signal. Add a `crates/rustty-bench/src/cli.rs` test asserting the
**complete** action set matches upstream's, driven by a checked-in list — not the current
`bench_registers_compression_actions`, which asserts only that `page-compression` and
`scrollback-compression` resolve and **passes with `hyperlink-map` and `apc-parser` entirely absent**.
Register `cargo test -p rustty-bench --lib` in `ci.sh`'s gate registry.

**NEGATIVE CONTROL:** the new test must be RED today (upstream has 11 `Action` variants;
`crates/rustty-bench/src/cli.rs:22-41` has 9). Land it RED as `P3-W5 T8/T9`'s tracker.

- [ ] **Step 3: Fix `lz4_differential_exhaustive`'s false PASS, and audit the `cfg(test)` class**

`crates/rustty-vt/src/compress/lz4_differential.rs:595` returns early unless `RUSTTY_LZ4_SLOW` is
set, and that variable is set **nowhere** in the repo — no gate, no `ci.sh` entry, no workflow. The
shape is faithful to upstream (`GHOSTTY_LZ4_SLOW` is likewise unset), but it is **worse in one
specific way**: Zig's `error.SkipZigTest` reports **SKIPPED** while Rust's early return reports
**PASSED**, so a reader is actively misinformed that 4 seeds × the exhaustive budget just ran. Fix:
`#[ignore = "set RUSTTY_LZ4_SLOW=1; ~minutes"]` **and** a periodic lane that sets it. Do both.

Then sweep the related class: `crates/rustty-vt/src/mem.rs:108,173` branch on `cfg!(test) || cfg!(miri)`
to port Zig's `builtin.is_test`. **`cfg(test)` is not transitive across crates**: Zig's
`zig build test` compiles the whole terminal module with `is_test = true`, so upstream's benchmark
tests see the zeroing simulation, whereas rustty's `rustty-bench` tests link `rustty-vt` with
`cfg(test)` **false** and therefore exercise the **real kernel `MADV_FREE_REUSABLE` path**. They pass
on macOS/arm64 today, but `scrollback_compression_restores_cold_terminal_pages` asserts
`compressed_pages > 0`, which now depends on a real kernel call succeeding — on any target where
`can_reclaim(Strict)` is false (non-Darwin, non-Linux, non-64-bit) it **FAILS where upstream's
passes**. Gate the simulation on something crate-external (`cfg(feature = "test-decommit-sim")`
enabled by `rustty-bench`'s dev-deps). Then
`grep -rn 'cfg!(test)' crates/*/src --include='*.rs'` and triage every hit in a **library** crate
whose *behaviour* it changes (as opposed to test-only helpers).

- [ ] **Step 4: Record the perf-baseline invalidation BEFORE anyone re-benchmarks**

Append to the `perf-baseline-rust-vs-zig` record: the oracle got ~2.8× faster on `+terminal-stream`
inside this range via `20c3eae04`/`src/quirks_memset.zig` (Zig 0.16 globally disabled LLVM loop
auto-vectorization, making compiler_rt's byte-loop `memset` ~24× slower and 63 % of executed
instructions; upstream hand-`@Vector`-ized it with an aarch64 `dc zva` path). **rustty does not port
it** — `rustc` lowers `@llvm.memset` to platform libc, exporting a strong `memset` from a Rust dylib
would pessimize rustty and risk ODR/interposition breakage. **Consequence:** any future rustty-vs-Zig
comparison against `origin/main` will show a smaller rustty margin than the recorded 14/14, and that
is the oracle getting faster, **not** rustty regressing. Re-confirm `-Doptimize=ReleaseFast`.

- [ ] **Step 5: Commit**

```sh
git commit -m "gates: register rustty-bench, un-vacuum the exhaustive LZ4 differential, gate the mem.rs decommit simulation crate-externally; record the release-arithmetic review class and the quirks_memset perf-baseline invalidation (P3-W0 T7)"
```

---

## `P3-W0` finale

- [ ] `bash .port/tools/ci.sh 2>&1 | tail -60` — G-LZ4 green, A5(d) RED (tracking the 2 known-absent exports), D10 RED (tracking the 4 known enum sites), `rustty-bench` action test RED (tracking 2 absent benches). **Three RED trackers are the intended state**; each is claimed by a named later task.
- [ ] `.port/phase-3/SAMPLING_CLOSURE.md`, `REACHABILITY.md`, `REVIEW_CLASSES.md` all exist and are referenced from `.port/phase-3/README.md`.
- [ ] Merge `p3-w0-instruments` to `main` with both platforms green.

---

# `P3-W1` — vt-core live-bug wave

**Branch:** `p3-w1-vt-safety`. **Scope:** the BL-3 safety family's **PageList half, which is 0 of 7
ported while its `Screen`/`Selection`/`Tabstops`/`Terminal` half is 12 of 13** — a half-ported family
that reads as done in `MAIN_PARITY.md` and in the in-source hash citations. Plus area D's three cheap
high-severity items. All S/M, no cross-area coupling, **no signature changes**, so this workstream
cannot conflict with `P3-W3`'s renames. Task order is set by `.port/phase-3/REACHABILITY.md`
(`P3-W0 T6`), not by the assessments.

**UB note for the whole workstream:** none of these hunks introduces a new `&mut Page`/`&mut Row`
alias, but `e727a3658` and `8d663a76e` **touch code that is already raw-pointer style**
(`crates/rustty-vt/src/screen.rs:2055-2068` carries the exemplary PORT NOTE). Do **not** "simplify"
either to `&mut [Cell]` while a `*mut Page` alias is live.

## Task 1: Width-aware pin traversal (`d6e24d985` end state, carrying `0ff4e41b2`'s tests)

- [ ] **Step 1: RED — port both commits' regression tests first**

`0ff4e41b2`'s two exact-multiple tests and `d6e24d985`'s three mixed-width tests. The mixed-width
fixture is `mixedWidthPinListForTest`, which uses `pauseIntegrityChecks(true)` + manual `createPage`;
**rustty has both primitives** (`crates/rustty-vt/src/pagelist/mod.rs`). Upstream's fixture also
builds `Node{ .data = .{ .resident = undefined } }` in one arm — **do NOT port that as `MaybeUninit`**;
build a real 1-column page via `create_page`, exactly as the mixed-width arm of upstream's own fixture
does.

- [ ] **Step 2: GREEN — port `d6e24d985`'s end state only**

`d6e24d985` rewrites the same functions `0ff4e41b2` fixes, so porting both sequentially would write
code that is immediately replaced. Port the end state: `PageList::pin` rejects `x >= p.node.cols()`;
`left_wrap`/`right_wrap` become per-row loops using each **destination** page's width;
`up_overflow`/`down_overflow` clamp `x` to `min(self.x, node.cols()-1)` at **all four** exit paths
(`crates/rustty-vt/src/pagelist/pin.rs:452,481,527,577`). Delete the verbatim pre-fix comment
*"This assumes that all pages have the same width"*. Record **both** hashes in the PORT NOTE so the
parity ledger stays honest.

- [ ] **Step 3: NEGATIVE CONTROL** — revert the `min(self.x, cols-1)` clamp at **one** of the four exit
paths and confirm a mixed-width test fails. All four sites must be individually load-bearing; if
reverting one changes nothing, that site has no test and needs one.

- [ ] **Step 4: Commit**

```sh
git commit -m "vt/pagelist: width-aware pin traversal and wrap math (oracle d6e24d985 end state, 0ff4e41b2 tests) (P3-W1 T1)"
```

## Task 2: The `PageIterator` count limit (`9b466e9c5`)

- [ ] **Step 1: RED** — `crates/rustty-vt/src/pagelist/iter.rs:382-399` (forward) and `:476-491`
(reverse) are **verbatim pre-fix**. Two defects: the forward arm's `if (len as usize) > *limit` is
dead, so a count-limited forward iteration **never crosses a page boundary** (the upstream
"impossible condition" bug, faithfully reproduced); and the reverse arm computes `row.y - 1` on a
`u16`, so a reverse count-limited iteration starting at `row.y == 0` underflows. Write the reproducer
per `REACHABILITY.md`.

- [ ] **Step 2: GREEN** — count the current row in **both** directions, consume the returned length,
advance only when rows remain; the reverse chunk becomes `start = available - len`, `end = available`.

- [ ] **Step 3: NEGATIVE CONTROL** — restore the dead `>` comparison and confirm the new
cross-page-forward test fails; separately restore `row.y - 1` and confirm the reverse test panics.
Two independent controls, because the commit fixes two independent bugs.

- [ ] **Step 4: Commit**

```sh
git commit -m "vt/pagelist: PageIterator count limit crosses pages and does not underflow at row 0 (oracle 9b466e9c5) (P3-W1 T2)"
```

## Task 3: The release-arithmetic subset (`c753fe4a4`, `afbf5ba15`, `e6e4a9fdc`, `30b42f42a`)

**Rationale.** All four are instances of `REVIEW_CLASSES.md`'s first class, and **three are strictly
worse in rustty than in Zig**.

- [ ] **Step 1: RED** — one reproducer each, per `REACHABILITY.md`:
  - `crates/rustty-vt/src/pagelist/scroll.rs:253,257,300` — `(-n) as usize` panics on `isize::MIN`.
  - `scroll.rs:346` — `scroll_prompt`'s `if delta > 0 { delta } else { -delta }`, same.
  - `crates/rustty-vt/src/pagelist/access.rs:1095` — `let mut y: size::CellCountInt = self.row_idx;`
    then `y += Node::rows(node)` in a loop: **panics in debug and wraps SILENTLY in release** once
    scrollback exceeds 65,535 physical rows.
  - `crates/rustty-vt/src/pagelist/pin.rs:143,148,153` — unchecked `coord.y += …` on `u32`.

- [ ] **Step 2: GREEN** — `n.unsigned_abs()` for the two scroll sites (`@abs`); widen the
`screen_point` accumulator to `u32` (`e6e4a9fdc`); `point_from_pin` uses **checked** adds and returns
`None` on overflow (`30b42f42a`). Per `REVIEW_CLASSES.md`, pair each saturating/checked form with a
`debug_assert!` on the invariant so the softened arithmetic cannot silently absorb a real invariant
break.

- [ ] **Step 3: NEGATIVE CONTROL** — build the tests `--release` as well as debug and assert the
**value**, not merely the absence of a panic. A release-mode wrap produces a wrong answer with no
panic, so a `#[should_panic]`-only control is blind to exactly the case that matters here. Confirm
these tests are **not** `#[cfg_attr(not(debug_assertions), ignore)]`.

- [ ] **Step 4: Commit**

```sh
git commit -m "vt/pagelist: unsigned_abs on scroll deltas, u32 screen-point accumulation, checked point_from_pin (oracle c753fe4a4 afbf5ba15 e6e4a9fdc 30b42f42a) (P3-W1 T3)"
```

## Task 4: The grapheme-break assert, stored-page widths, and the capacity-growth floor

- [ ] **Step 1: `b287f6d1a`** — `crates/rustty-vt/src/terminal.rs:968-970` still asserts
`debug_assert!(!brk)` in the stored-grapheme walk. Mode 2027 can be enabled *after* zero-width
codepoints were attached, so the break must be **fed into the state machine**, not asserted away. Fix
= delete the `debug_assert!(!brk)` and **keep the `grapheme_break(...)` call**, which mutates `state`
(the existing PORT NOTE already documents that). Drive the reproducer through `Stream`, not by calling
`print` directly. Upstream also removes the `eraseDisplay` workaround in
`testPrintSliceDifferential` — that test does not exist in rustty until `P3-W5`; note the dependency.

- [ ] **Step 2: `e727a3658` + `a55850c98`** — use the **stored page's** width, not `self.pages.cols`:
`crates/rustty-vt/src/screen.rs:2003` (`clear_rows` must take cells via `page.get_cells(row)`),
`:2124` and `:2138` (`clear_cells` compares `cells.len == page.size.cols`), and `screen.rs:1197`
(`cursor_cell_end_of_prev` uses `page_pin.node.cols() - 1`). All four trip `clear_cells`' own
`debug_assert!` bounds checks on a narrower stored page during incomplete reflow. **Keep the existing
raw-pointer style at these sites.**

- [ ] **Step 3: `8307349ec`** — `crates/rustty-vt/src/pagelist/grow.rs:495`'s `adjust_field!` does
`(old as $int).checked_mul(2)` with **no `old == 0` guard**, so growth from a zero capacity dimension
doubles 0 forever. Add the "jump to the standard default" arm. Per `REACHABILITY.md`, the consequence
is an **infinite loop allocating a page per iteration** in the unbounded-retry callers and **silent
data loss** in the single-retry style/grapheme paths — so the test needs a bounded iteration counter.

- [ ] **Step 4: `c442eb4b3`'s two missing asserts** — this is the one previously-believed-ported item
that is incomplete. Add `size_of::<Row>() >= size_of::<usize>()` as a `const _: () = assert!(…)` at
`crates/rustty-vt/src/page.rs:1225` (which today has only the unrelated
`size_of::<Row>() % align_of::<Cell>()` assert), and `assert!(cap.rows > 0)` in
`crates/rustty-vt/src/page_impl.rs:414` `init_buf`. **Why it matters in rustty specifically:**
`decommit_pool_item` (`crates/rustty-vt/src/pagelist/mod.rs:2410`) is live, so rustty already relies
on the invariant these asserts protect — pool free-list nodes occupy the first pointer-size bytes of a
recycled page buffer, and only `init_buf` rewriting row 0 erases them, and rustty skips the zeroing
memset in release exactly as Zig does. A future layout reorder or a zero-row page would corrupt pages
**in release builds only**, non-deterministically, depending on pool reuse.

- [ ] **Step 5: NEGATIVE CONTROL** — for Step 4, a `const _` assert has no runtime test; prove it by
temporarily reordering `Page`'s fields so `Row` is not first and confirming the build **fails to
compile**. Record the compiler error in the commit body. For Step 3, remove the `old == 0` arm and
confirm the bounded-iteration test hits its ceiling.

- [ ] **Step 6: Commit**

```sh
git commit -m "vt: feed grapheme breaks to the state machine, use stored page widths in clear/cursor paths, floor zero-capacity growth, restore the pool-zeroing asserts (oracle b287f6d1a e727a3658 a55850c98 8307349ec c442eb4b3) (P3-W1 T4)"
```

## Task 5: The search subsystem's two real defects (`5bc6588e4`, `627518447`)

**Rationale.** Area D's highest-severity findings, and one is **strictly worse than the pin**.

- [ ] **Step 1: `5bc6588e4` empty needles — a rustty-only live bug**

`crates/rustty-vt/src/search/sliding_window.rs` lacks `if (self.needle.len == 0) return null` in
`next()` (`:286`), **and** rustty's `ascii_index_of_ignore_case` (`:1165`) returns `Some(0)` for an
empty needle where Zig's `std.ascii.indexOfIgnoreCase` returns `null`. An empty needle therefore
produces a zero-length match that `highlight()` **cannot represent** (its end offset is inclusive).
Fix all four hunks: the two `needle.is_empty()` early returns (`sliding_window.rs:286`,
`search/active.rs:74`, the latter after `self.window.clear_and_retain_capacity()`), and the three
unsaturated `needle.len() - 1` overlap sites (`search/viewport.rs:262,315`, `search/active.rs:132`) →
`saturating_sub(1)`, hoisted to a single `overlap_len` local as upstream does. Port
`SlidingWindow empty needle has no matches`.

- [ ] **Step 2: `627518447` — the dangling-node window**

Extract `fn reset_if_dimensions_changed(&mut self) -> Result<bool, OutOfMemory>` from the inline body
at `crates/rustty-vt/src/search/screen.rs:496-533`, returning `true` when the reset happened. Then:
`feed` calls it; **`reload_active` (`:822`) calls it as its very first statement, before
`validate_selected_history()`, and returns early on `true`**; and **`select` (`:1409`) calls it before
`reload_active()`**. Until (3)/(4) land, both paths walk cached `FlattenedHighlight` node pointers
that column reflow may have freed. rustty's own PORT NOTE at `search/screen.rs:826-830` already flags
that upstream has this call and rustty does not — **that note is the smoking gun.**

**UB HAZARD (area D §4.2):** the reset does `self.deinit(); *self = new;` while `self.screen: *mut Screen`
is a live raw alias, and `ScreenSearch::init` borrows `self.needle()`. rustty's existing `feed` copy
already handles this by copying the needle to an owned `Vec<u8>` **first** — **reuse that shape
verbatim** in the extracted function. Do not let `ScreenSearch::init(self.allocator(), self.screen, self.needle())`
survive the refactor: `self.needle()` is a borrow of `self` that must not be live across
`self.deinit()`. In `select`, the new call must not sit inside any borrow of `self.history_results` /
`self.active_results` / `self.selected` — after a reset those are new allocations. This subsystem
already had a real 9-test SB UB cluster (memory: `search-screen-sb-ub-finding`); treat every
`&mut Screen` here as suspect.

- [ ] **Step 3: NEGATIVE CONTROL** — port `select after resize resets stale flattened results`
(upstream `search/screen.zig:1204`), then remove **only** the `select` call site and confirm it fails;
then restore it, remove **only** the `reload_active` call site, and confirm a second test fails. One
test that covers both sites cannot tell you a site regressed. For Step 1, assert
`ascii_index_of_ignore_case(b"", …) == None` directly — the early return alone would leave the
underlying divergence from Zig in place for the next caller.

- [ ] **Step 4: Run the vt Miri lane**

```sh
bash .port/tools/miri-run.sh --lane vt 2>&1 | tail -40
```

The vt lane is RED for a known 9-test search SB cluster (memory: `search-screen-sb-ub-finding`).
**Record the test-name set before and after** and assert it did not grow. Growing it here means the
extraction reintroduced the class.

- [ ] **Step 5: Commit**

```sh
git commit -m "vt/search: reject empty needles and reset cached results before reload_active/select (oracle 5bc6588e4 627518447) (P3-W1 T5)"
```

## Task 6: The scalar UTF-8 decode divergence, tabstop capacity, DECRQSS underline, run-grouped release

- [ ] **Step 1: `cb4c49fbf` — a behavioural divergence, not perf**

`crates/rustty-simd/src/vt.rs:94-101` returns `DecodeResult { consumed: decode_offset, decoded: decode_count }`
**unconditionally**, with no `decode.len() == input.len()` guard and no `break`. Upstream: a valid-so-far
multi-byte sequence truncated by the **decode region** is only left pending when the region ends at
the true end of input; when the region was bounded by an **ESC** it can never complete, so the prefix
is consumed as one maximal subpart → **one U+FFFD**. Consequence today: `"hi\xc2\x1b[0m"` and
`"\xe0\xa0\x1bX"` decode differently from upstream. Port the guard; port `decode partial UTF-8 before escape`
(2 cases).

- [ ] **Step 2: `5caf20e3e`** — `crates/rustty-vt/src/tabstops.rs:252` `if size < self.dynamic_stops.len()`
→ `<=`, so a resize to the exact existing capacity does not reallocate. Pure allocation behaviour; the
test needs a fixed-buffer allocator equivalent (upstream uses `FixedBufferAllocator`).

- [ ] **Step 3: `cb2fef390`** — `crates/rustty-vt/src/terminal.rs:4379` `print_attributes` emits a bare
`b'4'` with no `;4:{n}` sub-parameter branch, so DECRQSS loses the underline **style**. On `'4'` with
`pen.flags.underline != Single`, write `;4:{n}` and continue. Needs the 5-case table test.
**Keep `print_attributes`' existing `&mut [u8]` + returned `&[u8]`; do not format through `String`.**

- [ ] **Step 4: `8d663a76e`** — `crates/rustty-vt/src/screen.rs:2145-2154` releases styles per cell;
group styled cells into runs and call `styles.release_multiple(id, n)` once per run. rustty **already
has** `release_multiple` (`crates/rustty-vt/src/ref_counted_set.rs:824`), so this is a ~15-line loop
rewrite. **UB HAZARD:** it reads `cells[j].style_id` while calling `page.styles.release_multiple(...)`,
which mutates page memory — read the ids through the raw `*mut Cell` **as the existing loop does**,
never through a live `&mut [Cell]`.

- [ ] **Step 5: NEGATIVE CONTROL** — Step 1: a differential test feeding the two named byte sequences
through **both** the SIMD and scalar paths and asserting identical output; revert the guard and it must
fail. Step 2: assert **allocation count**, not just success — a test that only checks the resize
succeeds passes either way. Step 3: the 5-case table must include at least one `underline == Single`
case, or the test passes with the branch unconditionally taken. Step 4: assert the ref count reaches
exactly zero after a run release; a "no panic" test is blind to a double-release.

- [ ] **Step 6: Commit**

```sh
git commit -m "vt: resolve ESC-truncated UTF-8 prefixes to U+FFFD, keep exact-size tabstop buffers, report underline style in DECRQSS, release styles per run (oracle cb4c49fbf 5caf20e3e cb2fef390 8d663a76e) (P3-W1 T6)"
```

## `P3-W1` finale

- [ ] `cargo test -p rustty-vt --lib` and `--release` both green; every `REACHABILITY.md` LIVE row's
reproducer is un-ignored and passing.
- [ ] `bash .port/tools/miri-run.sh --lane vt` — test-name failure set **not larger** than the
recorded baseline.
- [ ] Strike the BL-3 rows in `.port/phase-2/MAIN_PARITY.md` and record that the family was
**half-ported by file** (Screen/Selection/Tabstops/Terminal 12/13; PageList 0/7) so the next reader
does not repeat the mistake.
- [ ] Merge with both platforms green.

---

# `P3-W2` — Page maps and capacity resilience

**Branch:** `p3-w2-page-maps`. **Ordering is upstream's:** `7e14347c1` → `65f953e8e` → `fedd42e8d`.
Port `fedd42e8d`'s **end state** for the hash-map internals: `7e14347c1` introduced a
tombstone-headroom counter and an in-place rehash (~470 lines) that `fedd42e8d` immediately **deletes**,
and rustty never had them — **do not introduce them.**

## Task 1: The pointer-stability caller audit (**hard prerequisite**)

**Rationale.** `fedd42e8d` replaces tombstones with backward-shift deletion (Knuth 6.4 alg. R), which
**moves other entries within a probe cluster on removal**. Upstream states the contract change
explicitly: *"insertion never moves existing entries, but removal may move other entries within a
probe cluster. Any key or value pointers previously returned by the map must be considered invalidated
by any removal."* In Zig, violating that is a logic bug. **In rustty's raw-pointer hyperlink and
grapheme maps it is UB.**

- [ ] **Step 1: Enumerate every caller that holds a key/value pointer across a removal**

```sh
grep -n 'get_ptr\|getPtr\|fetch_remove\|\.remove(' crates/rustty-vt/src/page.rs crates/rustty-vt/src/page_impl.rs
grep -n 'hyperlink_map\|grapheme_map\|GraphemeMap' crates/rustty-vt/src/page_impl.rs | head -40
```

Produce `.port/phase-3/HASHMAP_CALLER_AUDIT.md`: one row per call site, with the lifetime of every
handed-out pointer relative to the next `remove`/`fetch_remove` on the same map. Adopt the
`LIFETIMES.tsv` field-lifetime ledger convention (memory: `adopt-lifetimes-tsv`) for the raw-pointer
fields involved.

- [ ] **Step 2: Do NOT proceed to Task 3 until every row is either "no pointer survives a removal" or
has a rewrite plan.** This step's output is a gate on the workstream, not documentation.

- [ ] **Step 3: NEGATIVE CONTROL** — write a Miri test that deliberately holds a value pointer across
a removal on a `page.rs` hyperlink map and confirm it is **clean under today's tombstone map** and
**errors under a prototype backward-shift map**. That contrast is the proof that the audit is
necessary rather than precautionary; it also gives Task 3 an acceptance test.

- [ ] **Step 4: Commit**

```sh
git commit -m "vt/page: audit hash-map key/value pointer lifetimes against backward-shift deletion (oracle fedd42e8d prerequisite) (P3-W2 T1)"
```

## Task 2: Load-factor-aware map layout (`7e14347c1`, `65f953e8e`)

- [ ] **Step 1** — `crates/rustty-vt/src/hash_map.rs` has no `max_load`/`layout_for_size`
(`grep -c` = 0). Add the `max_load_percentage` comptime-equivalent parameter to
`AutoOffsetHashMap`/`OffsetHashMap`/`HashMapUnmanaged` (const generic, per the `adjust_field!`
precedent — hand-expanded consts and a macro, **not** a trait-object abstraction), plus `max_load()`,
`grow_if_needed` → `check_capacity`, `layout_for_size` (round entries up to the load factor, **then**
to a power of two), and `default_max_load_percentage = 100`.
`crates/rustty-vt/src/hyperlink.rs:57` `Map` switches to load **80**.

- [ ] **Step 2** — the `page.zig` side: `GraphemeMap` gains the explicit default load percentage;
`hyperlink_capacity()` returns `map.max_load()` **not** `capacity()`
(`crates/rustty-vt/src/page_impl.rs:2218`); `layout()` drops the pre-load-factor
`ceilPowerOfTwoAssert` on `hyperlink_map_count`; the `exact_row_capacity` derivation comment is
updated. All three `page.zig` hunks survive to head (verified at `origin/main:src/terminal/page.zig:98,1501,1769`).

- [ ] **Step 3: `65f953e8e`** — `crates/rustty-vt/src/page_impl.rs:2202` and `:2404`
(`move_hyperlink`/`move_grapheme`) call `put_assume_capacity_context`; switch to
`put_assume_capacity_no_clobber_context`, which rustty already has
(`crates/rustty-vt/src/hash_map.rs:535`). 2 lines.

- [ ] **Step 4: This moves `Page`'s layout byte size.** Regenerate every size golden and re-run D3
(`check_constants_parity.py`). Expect `page.rs`'s capacity math to move; that is the intended effect,
not a regression.

- [ ] **Step 5: NEGATIVE CONTROL** — port
`OffsetHashMap maximum load leaves probe headroom` and `HashMap no-clobber move after remove at max load`.
Then set the hyperlink map's load back to 100 and confirm the headroom test fails; and revert the
`no_clobber` switch and confirm the second fails. Also assert `hyperlink_capacity()` returns the
**max load**, not the capacity — a test that only checks "some capacity" passes either way.

- [ ] **Step 6: Commit**

```sh
git commit -m "vt/page: load-factor-aware offset-map layout and no-clobber moves (oracle 7e14347c1 65f953e8e) (P3-W2 T2)"
```

## Task 3: Backward-shift deletion (`fedd42e8d` end state)

- [ ] **Step 1** — replace `Metadata::TOMBSTONE` (`crates/rustty-vt/src/hash_map.rs:248`),
`is_tombstone` (`:278`), the `first_tombstone_idx` recycling (`:802-839`) and the single-slot-clear
`remove_by_index` (`:885`) with the backward-shift algorithm. `Metadata` becomes a `u8` with
`is_free() == (byte == 0)`; `remove()` is **deleted**; forced-inline Wyhash for
`hasUniqueRepresentation` keys.

**UB HAZARD (area D §4.5):** `remove_by_index` writes `metadata[hole] = metadata[j]`,
`keys_ptr[hole] = keys_ptr[j]`, `values_ptr[hole] = values_ptr[j]` — three overlapping raw views into
one `Page` allocation — plus `ctx.hash(keys_ptr[j])` reading a key while holes move. **Keep
`metadata`/`keys()`/`values()` as raw pointers** (rustty already does:
`metadata: Option<*mut Metadata>`); do **not** form `&mut [K]` and `&mut [Metadata]` simultaneously
from the same `*mut u8` base for indexing convenience.

- [ ] **Step 2** — port the two new asserts as real `debug_assert!`s:
`putAssumeCapacityNoClobberContext`'s `self.header().size < self.capacity()` and `getOrPut`'s
`metadata[0].is_free()`. They are what turns a violated capacity contract into a panic instead of a
silent live-entry overwrite.

- [ ] **Step 3** — port the 4 new upstream tests **and** `expectCanonical` as a **Rust
`debug_assert`-style invariant helper**, not a test-only function (area D §6). Upstream ships no
eviction/canonical-placement invariant outside tests; rustty should carry it in the type.

- [ ] **Step 4: NEGATIVE CONTROL** — the audit's Miri test from `P3-W2 T1 Step 3` must now be
**clean**, and `expectCanonical` must FAIL if the backward-shift loop's `hash(keys[j])` cluster test
is inverted. Run the full vt Miri lane and compare the failure-name set to the recorded baseline.

- [ ] **Step 5: Commit**

```sh
git commit -m "vt/hash_map: backward-shift deletion replaces tombstones, with a canonical-placement invariant (oracle fedd42e8d end state) (P3-W2 T3)"
```

## Task 4: Capacity-error resilience (`ee9d5b352` → `043326249`)

- [ ] **Step 1: `ee9d5b352`** — add `IncreaseCapacity::for_clone_error(err)`,
`PageList::clone_row_grow_capacity`, and the retry-on-capacity in `erase_row`
(`crates/rustty-vt/src/pagelist/erase.rs:24` calls `clone_row_from` once with no retry); add
`Screen::clone_partial_row_grow_capacity`. **The Zig version deliberately `@panic`s if
`increaseCapacity` fails mid-rotation — port the panic, do not invent a recovery.**

- [ ] **Step 2: `043326249`** — `cursor_scroll_above_rotate` gains `current_is_fresh` tracking, the
`assert(fresh_node == null or fresh_node == pages.last)` invariant, and an inner retry loop **sharing
`for_clone_error`**. It lands second so it can reuse Step 1's helper.

- [ ] **Step 3: UB HAZARD** — the doc comment is explicit: *"The source must NOT be on the given node
since the node's page memory may be freed on capacity increase."* Two raw `*mut Page` that must be
**proven disjoint**; `increase_capacity` frees one of them. Encode the disjointness as a
`debug_assert!` on the node pointers, not as a comment.

- [ ] **Step 4: NEGATIVE CONTROL** — drive the capacity failure through a tripwire fail point (the
established rustty substitute for `std.testing.FailingAllocator`; 19 tripwire sites already exist in
`crates/rustty-vt/src/screen.rs`) and assert the row survives the retry. Then remove the retry and
confirm the test fails; then remove the `for_clone_error` mapping and confirm a *different* assertion
fails. Two controls, because the retry and the error mapping are independently droppable.

- [ ] **Step 5: Commit**

```sh
git commit -m "vt: retry row clones on page-capacity errors in erase_row and cursor_scroll_above_rotate (oracle ee9d5b352 043326249) (P3-W2 T4)"
```

## `P3-W2` finale

- [ ] All size goldens regenerated; D3 green; `cargo test -p rustty-vt` green in debug **and** release.
- [ ] Full vt Miri lane run, failure-name set recorded and not larger than baseline.
- [ ] `.port/phase-3/HASHMAP_CALLER_AUDIT.md` has zero unresolved rows.
- [ ] Merge with both platforms green.

---

# `P3-W3` — Runtime scrollback limits (ONE unit, six crates)

**Branch:** `p3-w3-scrollback-limits`. **This workstream must land as one merge.** Area G's verdict:
the config half alone ships a documented, completion-visible, C-ABI-exposed `scrollback-limit-lines`
that **nothing enforces** — *"a shipped lie, and strictly worse than ABSENT"*. The vt half alone is
harmless but pointless. The unit is
`5b2d3b7df` → `86f81fb5b` → `10bc43420` → `f4c68d65e` (vt) + `65c48213b` + `1092204df` (config) +
`03d5fa268` + `a27e04e8f` + `5fd2973b9` + `2729996ea` (C-API) + the termio, inspector and bench tails.

**Blocked on:** `P3-W2` (Task 2 moves `Page`'s layout), and on WS7 BL-1 having merged (a concurrent
lane owns `crates/rustty-vt/src/{compress,pagelist}/`, and `f4c68d65e` rewrites the same
`erase_page`/`fixup_viewport`/`mark_activity` paths).

**Free win:** `f4c68d65e` closes a documented deferral. `crates/rustty-vt/src/pagelist/compress.rs:165-173`
records verbatim that the oracle has **eleven** `mark_activity` call sites and rustty has ten, the
missing one being inside `Limits.enforce`, *"and rustty has no `PageList::Limits` struct at all… If a
`Limits` struct is ever introduced, `enforce` must `mark_activity` on entry."* Trimming scrollback
mutates the node set under an in-flight incremental compression traversal, which is precisely what the
activity token detects. **`Limits::enforce` must `mark_activity()` on entry**, and the deferral row is
struck in the same commit.

## Task 1: `PageList::init(Options)` — the 256-call-site rename, alone

- [ ] **Step 1** — `5b2d3b7df`: `crates/rustty-vt/src/pagelist/mod.rs:1379`
`pub fn init(alloc, cols, rows, max_size: Option<usize>)` → `init(alloc, Options{cols, rows, max_size, max_lines})`.
Add `explicit_max_lines`/`min_max_lines`, `min_max_lines()`, `max_lines()`, `enforce_max_lines()`,
`IntegrityError::MaxLinesExceeded`, `fixup_viewport` clearing `viewport_pin.garbage`, and the
`enforce_max_lines` calls from `clone`/`resize`/`resize_without_reflow`/`grow`.

- [ ] **Step 2: This is ONE atomic commit.** 256 call sites across 8 files
(`crates/rustty-vt/src/pagelist/tests.rs`, `screen.rs`, `search/pagelist.rs`, `pagelist/bench.rs`,
`examples/*`, `crates/rustty-inspector/`). Mechanical but huge; the workspace does not build in a
split state. Upstream's own `659a60ae5` is the same mechanical follow-up on its side
(`PageList.init(alloc, 80, 24, 0)` → `init(alloc, .{…})`) — port its test-call-site form too.

- [ ] **Step 3: UB HAZARD** — `enforce_max_lines` iterates `tracked_pins.keys()` setting
`p.garbage = true` while `erase_page` remaps the same pins. Pins are **already** raw `*mut Pin` in
rustty; keep them that way. Do not introduce a `&mut Pin` for the flag write.

- [ ] **Step 4: NEGATIVE CONTROL** — port the 6 new upstream `PageList` tests, then remove **only**
the `grow` call site of `enforce_max_lines` and confirm a line-limit test fails. Add the
`MaxLinesExceeded` integrity check and confirm it fires when a page is manually appended past the
limit with integrity checks paused — an integrity variant no test can trigger is decoration.

- [ ] **Step 5: Commit**

```sh
git commit -m "vt/pagelist: init takes Options and enforces a line limit (oracle 5b2d3b7df 659a60ae5) (P3-W3 T1)"
```

## Task 2: `max_scrollback` → `max_scrollback_bytes` — the 204-occurrence rename, alone

- [ ] **Step 1** — `86f81fb5b` + `10bc43420`: `Screen::Options`/`Terminal::Options`
`max_scrollback` → `max_scrollback_bytes`, plus new `max_scrollback_lines: Option<usize>`; the byte
limit becomes **optional**, so `0` means "disable scrollback" **distinct from** "unlimited"
(`crates/rustty-vt/src/screen.rs` currently derives `no_scrollback` from `max_scrollback == 0` with no
unlimited encoding). Plumb through `ScreenSet`, `Selection`, `StringMap`, `Tabstops` tests.

- [ ] **Step 2: ONE atomic commit.** **204 occurrences across 45 files in 9 crates** — `rustty-vt`,
`rustty-vt-c`, `rustty-bench`, `rustty-core`, `rustty-renderer`, `rustty-font-shaper`, `rustty-termio`,
`rustty-apprt-c`, `rustty-inspector`. Includes `crates/rustty-bench/src/scrollback_compression.rs:234-238`,
which **documents** waiting for this rename.

- [ ] **Step 3: NEGATIVE CONTROL** — the semantic half is `0` ≠ unlimited. Assert **three** distinct
behaviours: `Some(0)` disables scrollback and erases, `None` is unlimited, `Some(n)` prunes at `n`. A
two-case test cannot distinguish the new encoding from the old.

- [ ] **Step 4: Commit**

```sh
git commit -m "vt: max_scrollback becomes max_scrollback_bytes with an optional unlimited encoding and a line companion (oracle 86f81fb5b 10bc43420) (P3-W3 T2)"
```

## Task 3: `PageList::Limits` (`f4c68d65e`) — the refactor that rewrites the existing prune path

- [ ] **Step 1** — extract `Limits { bytes: Limit, lines: Limit }` with `Limit { explicit, min }` and
`init`/`set`/`resize`/`max`/`exceeded`/`enforce`. **Delete** `PageList::max_size()`
(`crates/rustty-vt/src/pagelist/mod.rs:2175`) and `min_max_size()` (`:1254`) — both removed upstream;
only 3 in-crate `max_size()` callers, so cheap. Add `PageList::set_max_bytes`/`set_max_lines` and
`Terminal::set_scrollback_max_bytes`/`set_scrollback_max_lines`. **This relocates rustty's existing
`grow()` byte-prune path into `Limits::enforce`** — it is a rewrite, not an addition. Lowering a limit
prunes immediately; zero bytes degrades the primary screen to no-scrollback; alternate screens are
untouched. The byte limit stops being a fixed field and becomes `max(explicit, min)` recomputed on
every resize.

- [ ] **Step 2: `mark_activity()` at the top of `enforce`** — and strike
`crates/rustty-vt/src/pagelist/compress.rs:165-173`'s deferral plus its `.port/phase-2/DEFERRALS.md`
row in the same commit.

- [ ] **Step 3: UB HAZARD (area A §5(f))** — Zig's `Limits.enforce(self: *const Limits, pagelist: *PageList, key)`
passes a `*const Limits` that **lives inside the `*PageList` it mutates** — a self-alias. In Rust this
**cannot** be `(&self, &mut PageList)`. Port it as an inherent `PageList` method that **copies the
`Limits` value first**, or takes `*mut PageList`. **Do not mirror the Zig signature.**

- [ ] **Step 4: NEGATIVE CONTROL** — three tests: lowering `set_max_bytes` prunes **immediately**
(not at the next `grow`); `set_max_bytes(0)` on the primary screen erases history while the alternate
screen is untouched; and `Limits::enforce` bumps `activity_serial` while preserving
`last_serial`/`next_serial` — the exact shape of the existing
`pagelist_replacements_preserve_compression_continuation_and_mark_activity` test
(`crates/rustty-vt/src/pagelist/tests.rs:2365`), which is a proven-real guard. Remove the
`mark_activity()` and the third test must fail.

- [ ] **Step 5: Commit**

```sh
git commit -m "vt/pagelist: PageList::Limits with runtime setters, immediate pruning, and mark_activity on enforce (oracle f4c68d65e) (P3-W3 T3)"
```

## Task 4: The config half (`1092204df` `Limit`, `65c48213b` options and docs)

- [ ] **Step 1: `1092204df`** — port `src/config/limit.zig` (+115) as `Limit<T, DEFAULT>` into
`crates/rustty-config/src/value_types.rs`: `parse_cli`/`format_entry`/`optional`/`clone` and the
literal `"unlimited"` sentinel = `T::MAX`. Port its **3 oracle unit tests**.

- [ ] **Step 2: `65c48213b`** — rename `scrollback_limit` → `scrollback_limit_bytes`
(`crates/rustty-config/src/config_struct.rs:2327`, 16 in-crate sites, plus
`crates/rustty-config/src/c_get.rs` and `renderer_config.rs`), add `scrollback_limit_lines`
(default unlimited), and register
`cli::compatibility_renamed::<Config>(b"scrollback-limit-bytes")` — the machinery already exists at
`crates/rustty-args/src/args.rs:739` and is already used twice
(`config_struct.rs:9252,9258` for `background-blur-radius` and `adw-toolbar-style`), so the rename
costs users nothing.

- [ ] **Step 3: Fix the self-contradictory shipped doc.** **Delete**
`crates/rustty-config/src/config_struct.rs:2322-2323` — *"It is not currently possible to set an
unlimited scrollback buffer. This is a future planned feature."* — which sits directly above a default
lifted from the commit that added exactly that feature, and reaches users through
`+show-config --docs`, the generated `.5` man page and the webdata. Replace with upstream's final text
(`origin/main:src/config/Config.zig:1401-1405`).

- [ ] **Step 4: The field-count tax.** Bump **three** assertions —
`config_struct.rs:10570` (`CONFIG_FIELD_NAMES`), `:11060` (`ALL_KEYS`), `:11061` (`CONFIG_FIELDS`) —
and regenerate the **five** byte-exact goldens that contain the literal `scrollback-limit`:
`crates/rustty-extra/testdata/{_rustty,rustty.bash,rustty.fish,vim-syntax.vim,rustty.sublime-syntax}`.
**Fold `P3-W9 T3`'s `gtk-horizontal-tab-scroll` addition into this same commit** so the counts and
goldens are regenerated once, not three times (204 → 206).

```sh
cargo test --workspace --no-fail-fast          # NOT plain `cargo test --workspace`
```

Plain `cargo test --workspace` fails fast on the pre-existing `rustty-apprt-c` cold-target dylib
ordering issue, which **masks golden failures in later crates**.

- [ ] **Step 5: NEGATIVE CONTROL** — port upstream's
`test "scrollback limits"` and `test "compatibility: scrollback-limit renamed to bytes"`
(`origin/main:src/config/Config.zig:10865-10930`) **verbatim**. The alias test is the one that matters:
today's `config_scrollback_compression_defaults_and_parses` (`config_struct.rs:10655`) asserts
`scrollback_limit == 50_000_000`, which **pins the divergence rather than guarding the rename**. Also
assert `"unlimited"` round-trips through `format_entry` — a parse-only test passes with a
non-round-tripping formatter.

- [ ] **Step 6: Commit**

```sh
git commit -m "config: Limit(T, default) with an unlimited sentinel; scrollback-limit becomes scrollback-limit-bytes with a -lines companion and a compatibility alias (oracle 1092204df 65c48213b) (P3-W3 T4)"
```

## Task 5: The C-API half (`03d5fa268` + `a27e04e8f` + `5fd2973b9` + `2729996ea`) — one atomic commit

**Rationale.** The largest C-ABI break in the range: `RusttyTerminalOptions` is **removed entirely**.
A header/impl skew here is an ABI-parameter mismatch — the **same crash class as the sret bug**
(memory: `embed-abi-byvalue-sret-gate`). **Must not be split.**

- [ ] **Step 1** — `rustty_terminal_new(const RusttyAllocator*, RusttyTerminal*, uint16_t cols, uint16_t rows)`;
delete the `RusttyTerminalOptions` typedef (`include/rustty/vt/terminal.h:176-193`), the struct
(`crates/rustty-vt-c/src/terminal.rs:670-674`) and its `type_json` row — **and port upstream's
negative assertion** (`origin/main:src/terminal/c/types.zig:234` asserts
`!root.contains("GhosttyTerminalOptions")`). Add `RUSTTY_TERMINAL_OPT_SCROLLBACK_MAX_BYTES = 27` and
`…_MAX_LINES = 28` (`size_t*`; NULL ⇒ unlimited; `0` ⇒ disable+erase for bytes), `a27e04e8f`'s
`DATA_SCROLLBACK_MAX_BYTES = 34` / `DATA_SCROLLBACK_MAX_LINES = 35` (always primary screen;
`RUSTTY_NO_VALUE` when unlimited), and `5fd2973b9`'s +25 lines of docs on OPT 27/28. Flip
`rustty_terminal_set`'s doc from *"NULL clears the option to its default"* to upstream's
*"The behavior of a NULL value is specific to each option…"*.

- [ ] **Step 2: 18 call sites in the same commit.** The 16 examples
(`example/c-vt-cmake/src/main.c:15`, `c-vt-cmake-cross:15`, `c-vt-cmake-static:15`,
`c-vt-colors:95`, `c-vt-compression:38`, `c-vt-effects:82`, `c-vt-formatter:15`,
`c-vt-grid-ref-tracked:33`, `c-vt-grid-traverse:15`, `c-vt-kitty-graphics:80`, `c-vt-render:34`,
`c-vt-selection:57`, `c-vt-selection-gesture:62`, `c-vt-stream:15`, `cpp-vt-stream/src/main.cpp:14`,
`example/wasm-vt/index.html:214`) plus rustty-only `example/wasm-web-canvas/index.html:184`, plus the
`key.h`/`mouse.h` doc snippets that embed the old constructor.
`example/c-vt-compression/src/main.c` additionally gains the
`rustty_terminal_set(…, OPT_SCROLLBACK_MAX_BYTES, &n)` call upstream moved the limit into.

- [ ] **Step 3: `2729996ea`** — update the event tests for the new constructor.

- [ ] **Step 4: NEGATIVE CONTROL** — A5's new **header-driven** layout arm (`P3-W0 T2 Step 4`) and the
enum-discriminant arm (Step 5) must both bind here: temporarily mis-number `OPT_SCROLLBACK_MAX_BYTES`
to 26 (colliding with `OPT_CLIPBOARD_WRITE`) and confirm the enum arm FAILS. Then confirm the example
smoke build FAILS if one of the 18 call sites is left unconverted — WS19 T16 made
`example/*/build.sh` rebuild the dylib unconditionally, so this is now a real check rather than a link
against yesterday's ABI.

- [ ] **Step 5: Commit**

```sh
git commit -m "vt-c: remove RusttyTerminalOptions; scrollback limits become options 27/28 with data readers 34/35 (oracle 03d5fa268 a27e04e8f 5fd2973b9 2729996ea) (P3-W3 T5)"
```

## Task 6: The termio, inspector and bench tails

- [ ] **Step 1: termio (area E9)** — `crates/rustty-termio/src/termio_impl.rs:533`
`max_scrollback: opts.full_config.scrollback_limit` becomes
`max_scrollback_bytes: full_config.scrollback_limit_bytes.optional()` **and**
`max_scrollback_lines: full_config.scrollback_limit_lines.optional()`; `None` = unlimited.

- [ ] **Step 2: inspector (area F)** — `crates/rustty-inspector/src/widgets/screen.rs:838-843` (twice)
and `widgets/pagelist.rs:441` read `pages.max_size()`, which **no longer exists**; switch to
`pages.limits.max(Bytes)`. `widgets/pagelist.rs:413-435`'s "Page Bytes"/"Max Size"/"Tracked Pins" rows
→ "Scrollback Limit" is already ported with a PORT NOTE citing `6d5dda40d`; this completes it.

- [ ] **Step 3: bench** — `crates/rustty-bench/src/scrollback_compression.rs:234-238`'s documented
blocker resolves; delete the note and use the new field names.

- [ ] **Step 4: NEGATIVE CONTROL** — a termio-level KAT asserting the config → `PageList` limit
handoff: set `scrollback-limit-lines` to a small `n`, feed `> n` lines, assert `total_rows <= n`. A
test that only checks the field is *stored* is exactly the vacuous shape this whole workstream exists
to avoid — it would pass with `Limits::enforce` never called.

- [ ] **Step 5: Commit**

```sh
git commit -m "termio/inspector/bench: consume the byte and line scrollback limits (oracle 86f81fb5b f4c68d65e tails) (P3-W3 T6)"
```

## `P3-W3` finale

- [ ] `cargo test --workspace --no-fail-fast` green; all 5 goldens regenerated; the three `204` → `206`
assertions updated.
- [ ] A5 (all arms) + D3 + `check_comment_coverage.py` run; **G3b's config row is expected to RISE**
(the deleted "future planned feature" sentences become false "missing" entries) — record the number;
`P3-W10 T3` resolves it by construction.
- [ ] `.port/phase-2/DEFERRALS.md`'s `mark_activity`/`Limits` row struck; `MAIN_PARITY.md:128`'s
misleading "+ the 50 MB `scrollback-limit` docs" parenthetical corrected (the 50 MB default is **not**
from `0fb89f4ff`).
- [ ] Merge with both platforms green.

---

# `P3-W4` — Resize, cursor defaults, visibility, DECRQSS

**Branch:** `p3-w4-terminal-state`. Five commits whose halves live in four crates each; every task
here is an **atomic cross-crate landing**. Runs before or beside `P3-W3` (not after) so the
signature changes do not collide with `P3-W3`'s renames — coordinate: if `P3-W3 T2` has landed, use
the new field names.

## Task 1: `Screen::resize` failure-safety (`dde3d4d6b`)

- [ ] **Step 1** — `crates/rustty-vt/src/screen.rs:2396` `resize` → `resize_inner` is **verbatim
pre-fix**. Port: the `resize_tw` tripwire fail points (`saved_cursor_pin`, `pages`), the saved-cursor
pin tracked **before** any mutation, an extra `styles.use`/`hyperlink_set.use` reference held across
the resize so the error path can restore the cursor **without allocating**, the cursor node + serial
saved to detect page survival, and the new private `clear_prompt_for_redraw`.

- [ ] **Step 2: UB HAZARD (area A §5(a))** — the extra-reference trick plus the error-path restore
must **not** hold a `&mut Page` or `&mut Screen` across `PageList::resize`, which **reallocates
pages**. Raw pointers throughout; the restore writes back through `self.cursor` in a narrowly-scoped
`&mut`. Prove on Miri before wiring further.

- [ ] **Step 3: NEGATIVE CONTROL** — upstream's tests use `std.testing.FailingAllocator`; rustty's
substitute is a tripwire fail point (precedents: `crates/rustty-vt/src/terminal.rs:5244`,
`tabstops.rs:334`, `StartHyperlinkFp::DupeLink`). Add the fail-point module with **upstream's exact
names** so the parity is free, then: trip `saved_cursor_pin`, assert the cursor is unchanged and no
allocation occurred; trip `pages`, assert the screen dimensions are unchanged. Remove the extra style
reference and the first test must fail with an allocation.

- [ ] **Step 4: Commit**

```sh
git commit -m "vt/screen: make resize failure-safe with tripwire fail points and an allocation-free cursor restore (oracle dde3d4d6b) (P3-W4 T1)"
```

## Task 2: `Terminal.Resize` + full-featured resize (`89b103dd5`) — atomic across 4 crates

- [ ] **Step 1: vt** — `crates/rustty-vt/src/terminal.rs:4557`
`pub fn resize(&mut self, alloc, cols, rows)` → `resize(&mut self, alloc, value: Resize)` where
`Resize { cols, rows, cell_size_px: Option<{width, height}> }`, plus
`ResizeError { OutOfMemory, InvalidValue }`. Reject 0 dimensions; set pixel geometry **even when the
grid is unchanged**; **reset `synchronized_output` on every valid resize**; alt-screen
replace-or-fall-back-to-primary recovery. Fold in `b988efcfe` (`alt.io` → `self.io()`) by simply not
reproducing the pre-fix form — rustty has no `Terminal::io()` accessor and does not need one.

- [ ] **Step 2: stream_terminal** — add `Handler::resize(value: Terminal::Resize)`: resize, then when
`cell_size_px` is present **and** mode 2048 is on **and** `write_pty` exists, emit an in-band
`size_report::encode(Mode2048, …)`. **Keep it on `stream_terminal::Handler`** (area D §6.5): it
touches only `Terminal` + `Effects`, both already in `rustty-vt`, and moving it would force a
`rustty-vt` → apprt edge (R4).

- [ ] **Step 3: termio (area E8)** — `crates/rustty-termio/src/termio_impl.rs:1061` `resize` **deletes**
its own `self.terminal.width_px` assignment (`:1092`) and its
`modes.set(synchronized_output, false)` (`:1098`) — both move inside `Terminal::resize`.
`crates/rustty-termio/src/stream_handler.rs`'s mode-2048 resize switches to the struct form (no cell
size).

- [ ] **Step 4: C-API (area C)** — `crates/rustty-vt-c/src/terminal.rs:1587-1660` is verbatim
**pre-**`89b103dd5` (inline resize → saturating `width_px`/`height_px` mul →
`modes.set(SynchronizedOutput, false)` → in-band `size_report::encode`); delete 32 lines and delegate
to `wrapper.stream.handler.resize(…)`. Two C-API tests are reworked upstream.

- [ ] **Step 5: NEGATIVE CONTROL** — port all 4 upstream `resize*` tests. The load-bearing one asserts
`width_px` and the synchronized-output clear happen **exactly once**: a naive port that keeps both
call sites double-clears (harmless) but a port that removes both **without** the terminal-side move
**loses them entirely** — and no existing resize KAT would notice. Add a counter assertion, not a
value assertion.

- [ ] **Step 6: Commit**

```sh
git commit -m "vt: Terminal.Resize with cell geometry, sync-output reset and alt-screen recovery, threaded through stream_terminal, termio and the C API (oracle 89b103dd5 b988efcfe) (P3-W4 T2)"
```

## Task 3: `Terminal::resize` rollback (`a3c1caba5`)

- [ ] **Step 1** — strictly after Task 2. Roll back tabstops, `width_px`/`height_px`,
synchronized-output mode and screen dims; add the `resize_tw` fail points `tabstops`,
`primary_screen`, `alternate_screen`, `alternate_screen_init`; the commit point is followed by
`errdefer comptime unreachable` (Rust: an explicit `debug_assert!(false)`-style unreachable marker,
not `unreachable_unchecked`).

Note the `91f0cf67d` tabstop hunk **is already ported** (`crates/rustty-vt/src/terminal.rs:4579`,
guarded by `terminal_resize_preserves_tabstops_on_allocation_failure`); the rest is unguarded.

- [ ] **Step 2** — port the two `stream_terminal` tests:
`resize failure preserves terminal state and does not write` and
`resize effects do not change canonical terminal state`.

- [ ] **Step 3: NEGATIVE CONTROL** — one test per fail point, four total, each asserting a **different**
piece of rolled-back state. A single "resize failed and nothing changed" test cannot tell you which
rollback arm regressed. The `alternate_screen` arm must additionally assert the fall-back-to-primary
path, which is the only arm with observable *recovery* rather than restoration.

- [ ] **Step 4: Commit**

```sh
git commit -m "vt: roll back tabstops, pixel geometry, sync-output and screen dims on resize failure (oracle a3c1caba5) (P3-W4 T3)"
```

## Task 4: Cursor defaults move into terminal state (`c594031d5`) — atomic with the DELETIONS

- [ ] **Step 1: vt** — add `Terminal::Cursor { is_default, default_style, default_blink }`,
`Options::default_cursor_style`/`default_cursor_blink`, and
`Terminal::{set_cursor_style, set_default_cursor_style, set_default_cursor_blink}`.
`full_reset()` performs the reset.

- [ ] **Step 2: DELETE the old owners in the same commit.**
`crates/rustty-vt/src/stream_terminal.rs:108,227,1522-1531,1674-1681` (the `default_cursor_style` field,
its init, and the handler-side style/blink logic) and
`crates/rustty-termio/src/stream_handler.rs:123-127` + `:1310` `setCursorStyle` (−54 lines).
`crates/rustty-termio/src/termio_impl.rs:575` stops poking
`(*term.screens.active).cursor.cursor_style = opts.config.cursor_style` and passes the defaults into
`Terminal::init`; `change_config` calls the setters.
`crates/rustty-vt-c/src/terminal.rs:1389-1410` stops writing
`wrapper.stream.handler.default_cursor_style` directly and calls the setters.
**If the deletions do not land with the addition, DECSCUSR handling doubles up.**

- [ ] **Step 3: The mode-2031 trap.** `crates/rustty-termio/src/stream_handler.rs:938`
`self.default_cursor_blink.is_some()` reads a field that **moves**. A mechanical port that forgets to
re-point it silently breaks the "shell set an explicit blink, ignore mode changes" rule.

- [ ] **Step 4: NEGATIVE CONTROL** — port upstream's RIS round-trip test (set a style via DECSCUSR,
`full_reset`, assert the **default** style is restored — not the hardcoded initial one). Then a second
test for the mode-2031 rule: set an explicit blink, toggle mode 2031, assert the blink is unchanged.
The second test is the one that catches a mis-repointed `:938`.

- [ ] **Step 5: Commit**

```sh
git commit -m "vt: Terminal owns cursor defaults; delete the handler and termio copies (oracle c594031d5) (P3-W4 T4)"
```

## Task 5: Semantic stream-failure flag (`439d35e27`) — flag AND re-partition together

- [ ] **Step 1** — `crates/rustty-vt/src/stream_terminal.rs` has no `semantic_failure`
(`grep -c` = 0). Add the sticky `bool`, set in `vt()`'s catch. **In the same commit**, promote
`set_attribute`/`window_title`/`report_pwd` from swallow-and-log to propagating, and **leave**
`color_operation`/`kitty_color_operation`/`clipboard_contents` logged (graceful). Porting the flag
without the promotions gives a flag that **never fires** — the exact vacuous-guard shape
`.port/phase-3/README.md` warns about.

- [ ] **Step 2: C-API** — `RUSTTY_TERMINAL_DATA_VT_PROCESSING_ERROR = 33` (`bool*`; **not** cleared by
`rustty_terminal_reset()`). `crates/rustty-vt-c/src/terminal.rs:1789` `TerminalData` stops at
`ViewportActive = 32`.

- [ ] **Step 3: NEGATIVE CONTROL** — port
`semantic failure is sticky while processing continues`. Then, separately, feed a **`color_operation`**
failure and assert the flag stays **false** — that is the assertion that proves the re-partition was
ported rather than a blanket "any error sets the flag".

- [ ] **Step 4: Commit**

```sh
git commit -m "vt: sticky semantic-failure flag with the handler error re-partition, exposed as DATA 33 (oracle 439d35e27) (P3-W4 T5)"
```

## Task 6: DECRQSS response encoding (`39ae85f04`)

- [ ] **Step 1** — `crates/rustty-vt/src/dcs.rs` has `Handler` (`:69`) and `Command::Decrqss`
(`:280-286`) but no `encode`/`max_response_bytes`. Add `DECRQSS::max_response_bytes = 256` and
`DECRQSS::encode(t, buf)` producing `ESC P {0|1} $ r … ESC \`. Add the `dcs_handler` field and
`dcs_hook`/`dcs_put`/`dcs_unhook`/`dcs_command` to `stream_terminal` (rustty's DCS actions are still
the pin's no-ops). Requires `Terminal::print_attributes` (`P3-W1 T6 Step 3`) and
`Terminal::set_cursor_style` (Task 4).

- [ ] **Step 2: termio (area E11)** — delete
`crates/rustty-termio/src/stream_handler.rs:540-660`'s 60-line inline DECRQSS builder (the
`Sgr`/`Decscusr`/`Decstbm`/`Decslrm` arms with hand-rolled prefix-offset arithmetic) in favour of
`decrqss::encode(self.terminal, &mut response)` into a `max_response_bytes` buffer.

- [ ] **Step 3: the C-API test that answers an open question.** Area C §6.5 reports a **bounded
uncertainty**: rustty has the DCS DECRQSS machinery and the attribute responder
(`crates/rustty-vt/src/terminal.rs:4374`) but nobody traced whether the C `vt_write` path
(`crates/rustty-vt-c/src/terminal.rs:846`, `stream.next_slice`) delivers the reply through the
`write_pty` trampoline. **Port upstream's 20-line C-API test FIRST** — it answers the question
definitively either way, and it is cheaper than tracing.

- [ ] **Step 4: NEGATIVE CONTROL** — port `DECRQSS response encoding` and
`DECRQSS largest response fits fixed buffer`. The second is the real guard: shrink
`max_response_bytes` to 255 and it must fail. Keep the buffer `&mut [u8]`; **never format the response
through `String`.**

- [ ] **Step 5: Commit**

```sh
git commit -m "vt/dcs: DECRQSS encodes its own response into a fixed buffer; termio and the C API consume it (oracle 39ae85f04) (P3-W4 T6)"
```

## Task 7: Visibility reports, mode 2033 (`6c8c07981`) — end to end across 5 crates

- [ ] **Step 1: vt** — `crates/rustty-vt/src/terminal.rs` gains `flags.visible` (**exempt from RIS**:
view visibility survives a reset); `crates/rustty-vt/src/modes.rs:229-230,477-478` currently jump
2031 → 2048, insert `report_visibility = 2033`;
`crates/rustty-vt/src/device_status.rs` gains `Visibility`, `encode_visibility_report` and
`max_visibility_report_encode_size`, plus DSR entry `998` (question);
`stream_terminal` gains `send_visibility_report`, the `.visibility` DSR arm and the mode-2033 enable arm.

- [ ] **Step 2: termio (area E5)** — `crates/rustty-termio/src/message.rs` gains
`VisibilityReport { visible, force }`; `Termio::visibility_report()` uses the new encoder;
`Thread::drain_mailbox` gains the arm; `stream_handler` gains the mode-set arm (`force = true`) and the
DSR arm.

- [ ] **Step 3: core/Surface — and note this CHANGES EXISTING BEHAVIOUR.**
`crates/rustty-core/src/surface.rs:7303` `occlusion_callback` today only pushes
`renderer::Message::Visible` + `queue_render`. Add `Surface.visible: bool = true`, the dedup
(`if self.visible == visible { return; }`), the `terminal.flags.visible` write **under the renderer
lock**, and an **unforced** `visibility_report`. The dedup means repeated same-state occlusion
callbacks **no longer push a renderer message** — an observable change to the renderer path, not an
additive-only feature. Do not treat it as additive.

- [ ] **Step 4: C-API + header** — `include/rustty/vt/modes.h` currently jumps `:94` (2031) → `:95`
(2048); insert `#define RUSTTY_MODE_VISIBILITY_REPORT (rustty_mode_new(2033, false))` **only after the
mode exists in rustty-vt**, or the header is a lie.

- [ ] **Step 5: NEGATIVE CONTROL** — a KAT: enable mode 2033, drive `occlusion_callback(false)`,
assert the encoded report reaches the pty write queue; **plus** a dedup assertion that a **repeat**
call emits nothing; **plus** a RIS test asserting `flags.visible` **survives** the reset. Three
assertions because there are three independently droppable behaviours. Then remove the RIS exemption
and confirm the third fails.

- [ ] **Step 6: Also port `f00e90694`'s termio call site (area E6).**
`Termio::color_scheme_report_locked` stops hand-writing `"\x1B[?997;1n"`/`;2n` and calls
`device_status::encode_color_scheme_report` into a `max_color_scheme_report_encode_size` buffer. Output
is **byte-identical**, so any existing KAT passes either way — the guard has to be at the encoder level
(rustty already has `encode_color_scheme_report` at
`crates/rustty-vt/src/device_status.rs:118` with the test at `:175`). Assert the termio path **calls**
it, e.g. by making the test-only encoder count invocations.

- [ ] **Step 7: Commit**

```sh
git commit -m "vt/termio/core: DEC mode 2033 visibility reports end to end; route the color-scheme report through the shared encoder (oracle 6c8c07981 f00e90694) (P3-W4 T7)"
```

## `P3-W4` finale

- [ ] `cargo test -p rustty-vt -p rustty-vt-c -p rustty-termio -p rustty-core --no-fail-fast` green.
- [ ] Miri: the `Screen::resize` error path proven clean on an isolated harness.
- [ ] Merge with both platforms green.

---

# `P3-W5` — `page::Mask` → printSlice → render → stream fast paths → benches

**Branch:** `p3-w5-perf-family`. **`page::Mask` lands FIRST, alone**, because it is consumed by area
A's `Terminal::print_slice_fill`, area D's `render.zig` group scans, area F's
`renderer/generic.rs`, and area C's `include/rustty/vt/render.h`. Ported once with a **raw-base API**;
otherwise four areas hand-roll masked compares and diverge.

**Standing decision for this workstream (area A §7.6):** `f2a7652ab`'s explicit `@Vector` forms are a
**workaround for a Zig 0.16 codegen regression** (0.16 globally disabled LLVM loop auto-vectorization),
**not** R1-faithful behaviour. Port the **semantics**; benchmark rustty's scalar version; reach for
portable `core::simd` only if the numbers demand it. **Record the decision in the PORT NOTE** so a
future sweep does not "fix" it back.

## Task 1: `page::Mask` with a raw-base API (`446f80f4e`, `page.zig` half only)

- [ ] **Step 1** — port `Mask(T, fields, group_len)` (upstream `origin/main:src/terminal/page.zig:2303`,
+274 lines) plus `Row::Backing`/`Cell::Backing` backing-integer consts:
`bits`, `pattern`, `match`, `match_scalar`, `eql`, `eql_exact`, `eql_scalar`, `eql_prefix`, `strip`.
rustty has an unrelated `fn mask(bits: u32) -> u64` at `crates/rustty-vt/src/page.rs:524` and
hand-written `PERF(port)` masked stores (`page.rs:792`, `page_impl.rs:1327,1441`) — the new type
supersedes the hand-rolled ones.

- [ ] **Step 2: Rust shape** — Zig uses `@typeInfo`/`@FieldType` comptime reflection over packed
structs. Rust: **const-generic field-mask consts + a macro**, hand-expanded per field, following the
`adjust_field!` precedent at `crates/rustty-vt/src/pagelist/grow.rs:492`. **No trait-object
abstraction.** No exotic `unsafe` is required.

- [ ] **Step 3: UB HAZARD — the API signature is the mitigation, agree it before either consumer starts
(area D §6.2).** `eql_prefix` takes `cells[0..cell_count]` and indexes from an offset; in Rust it must
take `*const [Cell]` / `*const T` + len and use `ptr::read`, **not** `&[Cell]`/`&[Row]`, because both
consumers call it while a `*mut Page` alias to the same memory is live. A `&[Cell]` signature here
makes every call site UB and cannot be fixed later without touching both consumers.

- [ ] **Step 4: NEGATIVE CONTROL** — unit tests per method against a hand-computed expected mask for a
2-field and a 3-field case, **plus** a differential test asserting `match` and `match_scalar` agree on
1,000 random inputs and `eql`/`eql_scalar` likewise. The scalar arms are the oracle for the vector
arms; without the differential, a wrong `pattern` constant is invisible.

- [ ] **Step 5: Commit**

```sh
git commit -m "vt/page: comptime-equivalent Mask over packed Row/Cell fields with a raw-base API (oracle 446f80f4e page.zig half) (P3-W5 T1)"
```

## Task 2: `Terminal::print_slice` family (`47e26df60` → `cb2d78587` → `446f80f4e` rewrite)

- [ ] **Step 1: `47e26df60`** — new `Terminal::print_slice` (pub), `print_slice_fast`, `print_slice_fill`
(+675 lines in `Terminal.zig`). Confirmed new-in-range:
`git -C /Users/gael/projects/ghostty show 4749c4e93:src/terminal/Terminal.zig | grep printSlice` →
nothing; head has it at line 511.
- [ ] **Step 2: `cb2d78587`** — `print_slice_fill` fills style-only cell runs in bulk (+112). Strictly
after Step 1.
- [ ] **Step 3: `446f80f4e`** — rewrite `print_slice_fill` onto `Mask`.
- [ ] **Step 4: `f2a7652ab`** — semantics only, per the standing decision above.
- [ ] **Step 5: `b287f6d1a`'s test half** — the `eraseDisplay` workaround removal in
`testPrintSliceDifferential`, which `P3-W1 T4 Step 1` deferred because the test did not exist yet.

- [ ] **Step 6: NEGATIVE CONTROL** — the **differential** test is the whole guard: feed the same
codepoint stream through `print` one-at-a-time and through `print_slice`, and assert the resulting
screens are byte-identical (including styles, graphemes and wrap state). Then break the run-detection
boundary by one cell and it must fail. A test that only checks `print_slice` produces *something*
plausible cannot see a run-boundary off-by-one.

- [ ] **Step 7: Commit**

```sh
git commit -m "vt: batch printed codepoint runs into direct row fills via Mask (oracle 47e26df60 cb2d78587 446f80f4e f2a7652ab semantics) (P3-W5 T2)"
```

## Task 3: `Screen::cursor_scroll_region_up` (`77190bd02`) — **the heaviest Miri risk in the range**

- [ ] **Step 1** — new `cursor_scroll_region_up` + private `cursor_scroll_region_up_slow`: fast in-page
scroll-region rotate with `fastmem::rotate_once`, a zero-fill hot path, viewport-offset and tracked-pin
fixups; rewire `Terminal`'s scroll-region call sites (+184).
`grep -n "cursor_scroll_region_up\|rotate_once" crates/rustty-vt/src/{screen,terminal}.rs` → 0 today.

- [ ] **Step 2: UB HAZARD (area A §5(a), worst case)** — the fast path holds a `*mut Page` **plus** a
`rows` slice while mutating `tracked_pins` and `self.cursor.page_row`/`page_cell` through **other**
paths, then `assert_integrity()` re-walks the same memory. Requirements:
  - raw-pointer arguments **throughout**; never form `&mut Page`;
  - `fastmem::rotate_once(Row, rows)` must **not** take `&mut [Row]` while pins into those rows are live;
  - reach `row_data[vy]` through a `&mut *ptr.add(vy)` created inside the narrowest possible scope and
    dropped before the next raw access;
  - clear `dirty` through `*mut Row`, not `&mut Row`.
- [ ] **Step 3: PROVE ON MIRI on an isolated non-FFI harness before wiring into the build**
(`.port/phase-2/mirihar-chunk3b` precedent). Hand-reasoning Stacked Borrows here **has failed before**
in this program (memory: `shaper-hook-sb-clean-interleave`).

- [ ] **Step 4: NEGATIVE CONTROL** — two arms. (a) Correctness: a differential test asserting
`cursor_scroll_region_up(n)` equals the pre-existing slow path for every `(top, bottom, n)` in a small
exhaustive grid, including `n > region height` and a region spanning a page boundary (which must take
the `_slow` path). (b) Soundness: the Miri harness must **fail** if a `&mut Page` is reintroduced —
demonstrate that by temporarily doing so and recording the Miri error. A Miri run that is green both
before and after is not evidence.

- [ ] **Step 5: Commit**

```sh
git commit -m "vt/screen: fast in-page scroll-region rotate with raw-pointer pin fixups, Miri-proven (oracle 77190bd02) (P3-W5 T3)"
```

## Task 4: `RenderState` two-phase update (`446f80f4e`, `render.zig` half)

- [ ] **Step 1** — 12 missing hunks, enumerated in area D §3.1. Summary:
`pending_styles: Vec<StyleRun>` + `StyleRun { y, start, end, style }`; `update` splits into
`begin_update` + `end_update` (`update` becomes both); `if redraw { pending_styles.clear_retaining_capacity() }`;
the per-row `row_iterator` loop (`crates/rustty-vt/src/render.rs:659,663,718-728,1022`) becomes a
**per-page-chunk** loop over `viewport_pin.page_iterator(RightDown, None)` with
`take = min(chunk.end - chunk.start, self.rows - y)`, cursor detection hoisted to a per-chunk bounds
check, page `dirty` consumed once per chunk, and `last_dirty_page` **deleted**; the pin/serial
write-skip; `RowBuilder`; `scan_group_len = 8` with `RowDirtyMask = Mask<Row, {"dirty"}, 8>` and
`CellSpecialMask = Mask<Cell, {"content_tag","style_id"}, 8>`; the `cells.len != b.cols` resize skip;
run-based style detection (one `styles.get` per run); `end_update`'s denormalization with the defensive
`run.y >= row_cells.len` / `min(run.end, styles.len)` clamps; and the palette copied **only** under
`redraw` (`crates/rustty-vt/src/render.rs:592` is unconditional today).

Note `c9ad708ef`'s `serial` field **is** already ported (`render.rs:196`, `:705`, `:1065-1075`, guarded
by `flattened_highlights_require_matching_page_serial` at `:2086`) — that is the *only* part of
`446f80f4e` present.

- [ ] **Step 2: UB HAZARD — `RowBuilder` is the worst offender in area D (§4.1).** Upstream's
`RowBuilder` holds **seven simultaneous mutable column slices** of the `row_data` MultiArrayList plus
`pending_styles: *ArrayList(StyleRun)`, and its `row(p, page_row, vy)` receives `p: *page.Page` while
the caller is **concurrently iterating `page_rows: []page.Row` — a `&mut` slice into the very same
`Page` allocation** whose `dirty` bits it clears in the loop. Required shape:

```
RowBuilder { rows: *mut RowData, len: usize, alloc, cols, pending_styles: *mut Vec<StyleRun> }
fn row(&self, p: *mut Page, page_row: *const Row, vy: usize)
```

Never form `&mut Page`; reach cells via `(*page_row).cells.ptr((*p).memory)` raw. **rustty's `row_data`
is a `Vec<RowData>` of structs rather than column arrays, which is BETTER** — one raw base pointer
replaces seven slices. **Do not "improve" it into `split_at_mut` gymnastics.**

- [ ] **Step 3: Prove on Miri** on an isolated harness, as Task 3.

- [ ] **Step 4: NEGATIVE CONTROL** — port upstream's load-bearing
`incremental updates match full rebuild` test: drive a sequence of mutations, take the incremental
result and a from-scratch `redraw` result, assert byte equality. Then break the pin/serial write-skip
condition (skip when the **first** entry matches instead of first **and** last) and it must fail. This
is the single test that makes the whole restructure safe to land; without it the optimization is
unfalsifiable.

- [ ] **Step 5: Commit**

```sh
git commit -m "vt/render: two-phase begin_update/end_update with per-chunk iteration, RowBuilder and Mask group scans, Miri-proven (oracle 446f80f4e render.zig half) (P3-W5 T4)"
```

## Task 5: The renderer consumer (`446f80f4e`, `generic.zig` half) and `link.rs`

- [ ] **Step 1** — `crates/rustty-renderer/src/generic.rs:4564` still calls
`self.terminal_state.update(self.alloc, …)?`. Split: `begin_update(alloc, terminal)` **inside** the
state mutex, `end_update()` **outside** it, so style denormalization leaves the critical section
(upstream measures 2.7×–11× less lock hold).

- [ ] **Step 2: `9c9cf3e82`** — `crates/rustty-renderer/src/link.rs:162-215` allocates
`builder`/`map` and calls `render_state.string(...)` **before** the per-link
`match link.highlight` check. Add `Link::active(mouse_viewport, mouse_mods)` and early-return from
`render_cell_map` **before** allocating when no link can highlight.

- [ ] **Step 3: NEGATIVE CONTROL** — Step 1: prefer an **ordering** assertion (a flag set by
`end_update` that `rebuild_cells` asserts) over a lock-hold-duration measurement, which is flaky. The
renderer is render-complete offscreen, so an offscreen `update_frame` → `draw_frame` KAT covers the
sequencing without a window. Step 2: upstream uses `FailingAllocator{fail_index=0}`; rustty's
equivalent is an allocator that **panics on first use** (`render_cell_map` takes an explicit `alloc`),
asserting `result.count() == 0` and **zero allocations** for all three non-matching highlight modes. An
"it still returns no links" test passes without the early return.

- [ ] **Step 4: Commit**

```sh
git commit -m "renderer: hold the state mutex only for begin_update, and skip the link map allocation when no link can highlight (oracle 446f80f4e 9c9cf3e82) (P3-W5 T5)"
```

## Task 6: Stream fast paths as ONE series (`253e4f9c3`, `1a88f3622`, `300b42f42a`, `f6f79acce`, `8c523ed03`, `083d9709b`, `b5053153f`)

**Rationale.** Area D §5.6: **do not cherry-pick** — the fast paths interlock and each one changes
which action variants the handler must implement. Land with the BL-4 bench corpus.

- [ ] **Step 1: `253e4f9c3`** `consume_csi_params` (bulk-consume CSI digits/separators, dispatch on the
final byte). **UB HAZARD (area D §4.7):** it opens with `const p = &self.parser;`, accumulates locals,
then writes back through `p` **and calls `self.csi_dispatch_final(c)`**, which takes `&mut self` and
mutates `self.parser` again — two live `&mut` to the same field. Correct shape: finish **all** writes
through `p`, **end the borrow**, then call. Rust NLL handles this if the borrow genuinely ends; **do
NOT paper over it with a raw `*mut Parser` held across the call.**
- [ ] **Step 2: `1a88f3622` + `300b42f42a`** `csi_dispatch_final`, `csi_entry_byte`, and the
`@hasDecl(T, "vtRaw")` gate that keeps the inspector on the general path. **Reproduce the gate as a
Rust trait-associated `const HAS_VT_RAW: bool`** (or a marker trait). Silently dropping it **strips raw
events from the inspector** (area F).
- [ ] **Step 3: `f6f79acce`** bulk APC dispatch: `apc::Handler::feed_slice`,
`apc/glyph/request.rs` `CommandParser::feed_slice` (+ the `ReadFailed` arm),
`kitty/graphics_command.rs` `Parser::feed_slice`, `stream.rs` `Action::ApcPutSlice` + `apc_put_slice` +
`consume_apc_string`, the `stream_terminal` dispatch arm, and the termio arm
(`self.apc.feed_slice(self.alloc, value.bytes)`). **15 upstream tests**, especially
`stream: apc vector boundaries match scalar path`.

  **UB HAZARD (area D §4.6):** upstream's `.kitty => |*p| { p.feedSlice(rem) catch { p.deinit(); self.state = .ignore; }; }`
  borrows **into** `self.state` and then assigns `self.state`. In Rust
  `match &mut self.state { State::Kitty(p) => { … self.state = … } }` does not compile, and "fixing" it
  with a raw pointer to the payload while writing the discriminant through `&mut self.state` is an
  aliasing violation of the same kind. Correct shape:
  `let mut st = core::mem::replace(&mut self.state, State::Ignore);` operate on the owned `st`, write
  the final state back. Same for `.glyph => |*p|`.
- [ ] **Step 4: `8c523ed03`** vectorize the APC payload `invalid`-mask scan (strictly after Step 3).
- [ ] **Step 5: `47e26df60`'s stream half** `Action::PrintSlice` + the lane-wise printable-run scan in
`next_slice`, plus `simd::lanes(T)` (= 2× native vector width) in `crates/rustty-simd/src/lib.rs`, plus
the termio arm (`.print_slice => self.terminal.print_slice(value.cps)`). Needs `P3-W5 T2`.
- [ ] **Step 6: `083d9709b`** `crates/rustty-simd/cbits/simd/vt.cpp`: fuse the ASCII fast path into the
ESC scan — `WidenAsciiStore` (byte→u32 widening via `PromoteTo` on quarter vectors) and
`DecodeNonAsciiUntilControlSeq`. **The `HWY_TARGET == HWY_SCALAR` arm is mandatory** or wasm/scalar
targets break.
- [ ] **Step 7: `b5053153f`** `log_unsupported_once(fmt, args, key)` — a 16-slot atomic `u32`
per-call-site dedup table; convert the 8 `log.warn` sites.

- [ ] **Step 8: NEGATIVE CONTROL** — the series' guard is **scalar-vs-vector differential**, at three
levels: `decode simd matches scalar` (new upstream test) for Step 6;
`stream: apc vector boundaries match scalar path` for Steps 3–4; and a fuzz-style test feeding the
BL-4 bench corpus through both the fast and general stream paths asserting identical action sequences,
for Steps 1–2 and 5. Additionally: assert the **inspector** still receives raw events by driving a
`vtRaw`-implementing handler and counting them — remove the `HAS_VT_RAW` gate and that count must drop
to zero. That is the only test that can catch a silently-dropped gate.

- [ ] **Step 9: Commit**

```sh
git commit -m "vt/stream: CSI param and dispatch fast paths, bulk APC and print slices, fused SIMD ASCII scan, deduped unsupported logs (oracle 253e4f9c3 1a88f3622 300b42f42a f6f79acce 8c523ed03 47e26df60 083d9709b b5053153f) (P3-W5 T6)"
```

## Task 7: The two absent C-API exports (`446f80f4e`, `render.h` half)

- [ ] **Step 1** — add `rustty_render_state_begin_update(RusttyRenderState, RusttyTerminal)` and
`rustty_render_state_end_update(RusttyRenderState)` to `crates/rustty-vt-c/src/` and the **66 missing
lines** of `include/rustty/vt/render.h`: the `## Two-Phase Updates` group-doc section, the paragraph in
`rustty_render_state_update`'s doc explaining it is the both-phases convenience and that lock-holding
callers should prefer the split, the two declarations with their 20-line and 12-line docs (including
*"Calling this without a prior begin is a safe no-op"*).
`include/rustty/vt/render.h` currently stops at `rustty_render_state_update` (`:342`).

- [ ] **Step 2: NEGATIVE CONTROL** — **A5(d) (`P3-W0 T2 Step 2`) must flip from RED to GREEN here.**
That is this task's primary control: the gate was landed RED naming exactly these two symbols. Also add
a C example exercising begin/end and assert the "end without begin is a no-op" contract, which is the
only behavioural claim in the docs.

- [ ] **Step 3: Commit**

```sh
git commit -m "vt-c: expose render-state begin_update/end_update; A5(d) goes green (oracle 446f80f4e render.h half) (P3-W5 T7)"
```

## Task 8: `ScreenClone` and `TerminalStream` benchmarks (`446f80f4e`, `4c5b1d5d5`, `258de36d1`)

- [ ] **Step 1: `ScreenClone` (`446f80f4e`)** — `crates/rustty-bench/src/screen_clone.rs:80-99` has only
`Noop`/`Clone`/`Render` and `Options` (`:40`) has no `loops`. Add: `Options.loops: u32 = 1`;
`Mode::RenderLocked` (`50_000 * loops` × `state.screen = Alternate; state.begin_update(alloc, &terminal)`);
`Mode::RenderClean` (`3_000_000 * loops` × plain `update`); `Mode::RenderPartial`
(`pages.pin(active{x:0, y: rows/2})`, then `2_000_000 * loops` × `pin.mark_dirty(); state.update(...)`);
`step_render`'s bound `1000` → `50_000 * loops` and `do_not_optimize_away(state)` → `(&state)`;
registration in the `step_fn` switch and `Mode::from_name`.

- [ ] **Step 2: `TerminalStream` (`4c5b1d5d5` + `258de36d1` + `f2a7652ab`)** — delete the local
print-only `Handler` at `crates/rustty-bench/src/terminal_stream.rs:262` (its own comment says it
"only acts on `.print`") and use the **full** `rustty_vt::stream_terminal` readonly handler;
`crates/rustty-bench/src/terminal_stream.rs:223` `[0u8; 4096]` → `[0u8; 64 * 1024]` (matching
`termio`'s `buffer_capacity`) with an **unbuffered** reader so reads go straight into `buf`, avoiding a
per-chunk memcpy that would pollute the measurement; port the module doc-comment rewrite. Consequence
of not doing this: rustty's `terminal-stream` benchmark measures **materially less work** than
upstream's — no styles, cursor movement, erases or modes.

- [ ] **Step 3: NEGATIVE CONTROL** — the benchmarks have no assertions by nature, so the control is the
**`rustty-bench` action-set test from `P3-W0 T7 Step 2`**, which must go from RED to GREEN as the modes
are registered, plus a smoke run of each new mode with `loops=1` asserting a non-zero step count. A
mode registered in `from_name` but missing from the `step_fn` switch would otherwise be invisible.

- [ ] **Step 4: Commit**

```sh
git commit -m "bench: ScreenClone render-locked/clean/partial modes with a loops multiplier; TerminalStream uses the full handler and 64 KiB unbuffered reads (oracle 446f80f4e 4c5b1d5d5 258de36d1) (P3-W5 T8)"
```

## Task 9: The two wholly absent benchmarks (`7e14347c1` `HyperlinkMap`, `f6f79acce` `ApcParser`)

- [ ] **Step 1: `HyperlinkMap`** (155 lines, unblocked — `Page` and `hyperlink` are both ported):
fields `opts`, `page: Page`, `link_id: hyperlink::Id`, `entry_count`;
`Options { entries: u16 = 4096 (power of two, ≥16), load_percent: u8 = 100 (>100 clamped), loops: u16 = 1, mode }`;
`Mode { Lookup, Churn }`; `create`/`destroy`/`benchmark`/`setup`/`teardown` + the two step fns; 1 test.
Register in `crates/rustty-bench/src/cli.rs:22` (`Action`), `:47` (`from_name`), `:209`
(`main_action` dispatch) and `crates/rustty-bench/src/lib.rs`.

- [ ] **Step 2: `ApcParser`** (144 lines) — `Options { data: Option<&[u8]> }`, a `Stream<Handler>` whose
handler holds `apc::Handler` and dispatches `ApcStart`/`ApcPut`/**`ApcPutSlice`**/`ApcEnd` (parsing then
immediately dropping each completed command), 1 test, plus registration. **Blocked on `P3-W5 T6 Step 3`**
(`apc_put_slice`).

- [ ] **Step 3: NEGATIVE CONTROL** — port both upstream tests, and assert the `rustty-bench` action-set
test now lists **11** actions matching upstream. Then remove one `from_name` arm and confirm it fails —
the old `bench_registers_compression_actions` would not have.

- [ ] **Step 4: Commit**

```sh
git commit -m "bench: port the HyperlinkMap and ApcParser benchmarks (oracle 7e14347c1 f6f79acce) (P3-W5 T9)"
```

## `P3-W5` finale

- [ ] `cargo test -p rustty-vt -p rustty-vt-c -p rustty-renderer -p rustty-bench --no-fail-fast` green,
debug **and** release.
- [ ] Both Miri harnesses (`cursor_scroll_region_up`, `RowBuilder`) green, with the recorded
"reintroduce `&mut Page` → Miri errors" negative controls in the commit bodies.
- [ ] A5(d) green (2 exports); `rustty-bench` action-set test green (11 actions).
- [ ] Re-run the perf suite and compare against `perf-baseline-rust-vs-zig` **with the `quirks_memset`
note applied** — a shrunken margin against `origin/main` is the oracle getting faster.
- [ ] Merge with both platforms green.

---

# `P3-W6` — C-API surface and effects

**Branch:** `p3-w6-capi`. Everything area C owns that is **not** already claimed by `P3-W3 T5`
(the constructor break), `P3-W4` (resize/cursor/failure/DECRQSS/mode-2033 headers), `P3-W5 T7`
(render_state), or **WS19 T3** (the `Opaque` fix + A5 compiling the full header — **out of scope**).

## Task 1: Wave C0 — the cheap independent items

- [ ] **Step 1: `421fe8dab` stale comment** — `crates/rustty-vt-c/src/grid_ref.rs:227-232` transcribes
an oracle comment still showing the **pre-**`421fe8dab` `p.node.data.…` body, while the code at
`:256,267` correctly uses `rustty_vt::pagelist::Node::page(p.node)`. D9 (`check_stale_comments.sh`)
is advisory here; fix the comment and add the row to `stale-comments-D9.tsv`.
- [ ] **Step 2: `aea63d71f` test table** — the fix is structurally N/A in Rust
(`crates/rustty-vt-c/src/render.rs:1304` `let mut needed = cp0.len_utf8();` is already `usize`, so the
`u3` overflow cannot occur), but the **test-table extension is absent**:
`crates/rustty-vt-c/src/render/tests.rs:897` covers only `"e\u{301}"`. Port the 3-case table
(combining, 8-byte flag, family emoji) plus the exact-size and `cap == needed - 1` no-write cases.
`grep -rn '1F1FA\|1F468' crates/rustty-vt-c/src` → only `unicode.rs:96` today.
- [ ] **Step 3: `include/rustty/vt.h` include order** — rustty has `device.h` before `color_scheme.h`;
upstream has the reverse. Match upstream: zero risk, kills a permanent diff line. **Keep** rustty's two
extra `color_scheme` doc lines and the `@example c-vt-color-scheme` block (strictly better Doxygen) and
**record them as a deliberate divergence**, along with the `rustty.sh` identity string, so a future
mechanical header-diff gate does not churn. Add `PORT_URL`/`ORACLE_URL` to
`.port/tools/parity_names.env` so the URL choice is data, not a stray literal.
- [ ] **Step 4: `634ef7198` README lag** — `example/c-vt-effects/README.md` lists only
`write_pty, bell, title_changed`; add `clipboard_write` and the "one atomic write with decoded,
binary-safe MIME representations rather than protocol-specific OSC 52 data" paragraph. (`src/main.c` is
already correct — the callback is at `:42-71`.)
- [ ] **Step 5: `example/README.md`** — upstream has one; rustty does not. **Write a rustty-native one
describing the `build.sh` convention** (`.port/phase-2/EXAMPLES_PORT.md`); do **not** port upstream's
text, which documents the `build.zig` convention rustty deliberately replaced.

- [ ] **Step 6: NEGATIVE CONTROL** — Step 2's table must include the `cap == needed - 1` **no-write**
case and assert the output buffer is **untouched**; a "returns the needed length" test passes with a
buggy partial write. Step 3: assert the reordered `vt.h` still compiles (A5(a)) — the control is that
A5 already exists.

- [ ] **Step 7: Commit**

```sh
git commit -m "vt-c: refresh the grid_ref reference comment, extend the grapheme-UTF-8 test table, match upstream vt.h include order, document the clipboard_write effect (oracle 421fe8dab aea63d71f 634ef7198) (P3-W6 T1)"
```

## Task 2: Narrow the four closed enums received across the ABI

- [ ] **Step 1: the in-range one** — `ScrollViewportC.tag` becomes `c_int`, narrowed inside
`rustty_terminal_scroll_viewport` with the **same explicit `match`** used at
`crates/rustty-vt-c/src/terminal.rs:922-926` for `compress`'s mode. **Keep** the existing
`const _: () = assert!(size_of::<ScrollViewportTag>() == size_of::<c_int>())` so the ABI is provably
unchanged. `3a2e28329` added `Row = 3` to this very enum, so the defect is **this range's**.
- [ ] **Step 2: the three pre-existing ones, in the same sweep** (cheap, identical pattern):
`rustty_focus_encode`'s `event`, `rustty_sys_log_stderr`'s `level`, `rustty_size_report_encode`'s
`style`. **Do not misattribute them to this range** — record them as pre-existing in the commit body.
- [ ] **Step 3** — promote D10 (`P3-W0 T3`) from PROGRESS to **HARD** in `ci.sh`, with the allowlist
empty.

- [ ] **Step 4: NEGATIVE CONTROL** — D10 must go RED→GREEN (it was landed RED naming exactly these
four). Additionally, a C test passing `RUSTTY_SCROLL_VIEWPORT_MAX_VALUE` — an enumerator the header
**itself declares** — and asserting a defined error return rather than UB. That test is impossible to
write correctly against today's signature, which is the point.

- [ ] **Step 5: Commit**

```sh
git commit -m "vt-c: narrow inbound closed enums at the ABI boundary (scroll_viewport in-range; focus/sys/size_report pre-existing); D10 goes HARD (oracle 3a2e28329 follow-up) (P3-W6 T2)"
```

## Task 3: Effects — desktop notification and progress report (`c3655ba25`, `47d602c42`, `628adaf30`)

- [ ] **Step 1: header** — `RusttyTerminalDesktopNotification { size_t size; RusttyString title; RusttyString body; }`
+ `…Fn`; `RusttyTerminalProgressState { REMOVE=0, SET=1, ERROR=2, INDETERMINATE=3, PAUSE=4, MAX_VALUE }`;
`RusttyTerminalProgressReport { size_t size; RusttyTerminalProgressState state; int8_t progress; }`
(`-1` = omitted) + `…Fn`; `RUSTTY_TERMINAL_OPT_DESKTOP_NOTIFICATION = 29`,
`…OPT_PROGRESS_REPORT = 30`; the two new rows in the effect-callback doc table.
- [ ] **Step 2: vt** — `crates/rustty-vt/src/stream_terminal.rs:1918` `show_desktop_notification` and
`:1921` `progress_report` are deliberate **no-op stubs** matching the pin; wire them to
`Effects.desktop_notification` / `Effects.progress_report` and add the `readonly` entries.
- [ ] **Step 3: `types.rs`** — register **both** structs in the same commit.
`628adaf30`'s Zig `LibEnum` factory is structurally N/A (rustty hand-authors each `#[repr(C)]`
projection), but **the enum it shares is `47d602c42`'s type**, so it lands here.
- [ ] **Step 4: `RusttyString` fields carry BYTES.** Per the byte-slice rule, the notification title
and body are `&[u8]`/`Vec<u8>` on the Rust side, never `String`.

- [ ] **Step 5: NEGATIVE CONTROL** — A5's **header-driven** layout arm (`P3-W0 T2 Step 4`) is the
control: typedef either struct **without** its `types.rs` row and the gate must FAIL. Demonstrate that
by landing the header hunk first, observing RED, then adding the rows. Plus a C example asserting each
callback fires with the right `size` field (the sized-struct contract).

- [ ] **Step 6: Commit**

```sh
git commit -m "vt-c: desktop-notification and progress-report effects with registered sized structs (oracle c3655ba25 47d602c42 628adaf30) (P3-W6 T3)"
```

## Task 4: Kitty temp-file medium becomes a **directory** (`e8525c0fd`(iii) + `b988efcfe` + `960c2cca5`)

- [ ] **Step 1: vt (area D)** — `crates/rustty-vt/src/kitty/graphics_image.rs:128`
`pub temporary_file: bool` becomes a tagged union carrying the expected temp **directory**;
`:133 ALL` → `all_with_temp_dir(path)`; add `LoadingImage.temporary_directory`; and
`:526 fn isPathInTempDir(path)` takes the directory **explicitly** instead of consulting
`temp_dir::allocTmpDir()` at `:536`. **This is security-relevant:** upstream removed a
process-env dependency from a path-validation check (which directories a remote-controlled kitty `f=`
transmission may read from). It lands inside the Zig-0.16 commit, so the intent is inferred — treat it
as deliberate hardening and **do not keep the env lookup "temporarily"**.
- [ ] **Step 2: `960c2cca5` must not be lost.** Its fix does not exist in rustty because the buggy code
does not. The ported code must **already carry it**: bounds-check first, then copy **exactly `v.len`**,
and slice the directory as `tmp_dir_path[0..v.len]`. Add a `PORT NOTE` citing `960c2cca5` at that site.
- [ ] **Step 3: C-API + header** — OPT 17 / DATA 28 flip from `bool*` to `RusttyString*`
(restrict to a directory; the string is **copied into** the terminal; NULL = disable; empty when
disabled). `crates/rustty-vt-c/src/terminal.rs:1253,1269,2050` treat it as `bool` today;
`include/rustty/vt/terminal.h` documents `bool*`. `b988efcfe` is the header-doc half.
- [ ] **Step 4: termio (area E12)** — `crates/rustty-termio/src/termio_impl.rs:559`
`kitty_image_loading_limits: Limits::ALL` → `all_with_temp_dir(global::tmp_dir_path())`;
`change_config` likewise. **Blocked on `P3-W7 T1`** (`tmpDirPath()`). **Do not stub it as "allow
everything".**

- [ ] **Step 5: NEGATIVE CONTROL** — the security assertion: a transmission naming a path **outside**
the configured directory must be **rejected**, and — critically — the rejection must **not** depend on
the process `TMPDIR`. Prove that by setting `TMPDIR` to the attacker's directory and asserting the
rejection still happens. A test that only checks in-directory paths are accepted is blind to exactly
the bug this commit fixes. Plus a length test for `960c2cca5`: a directory string of
`max_path_bytes - 1` and one of `max_path_bytes + 1`, the latter rejected without a copy.

- [ ] **Step 6: Commit**

```sh
git commit -m "vt/kitty: temp-file medium restricts to an explicit directory instead of the process TMPDIR (oracle e8525c0fd b988efcfe 960c2cca5) (P3-W6 T4)"
```

## Task 5: Kitty transient usage hints and eviction order (`a65e11cc9`)

- [ ] **Step 1** — `Transmission.usage: Usage` (`packed(u32)`, bit 0 `transient`) parsed from the `N`
key; `Image.usage`; and re-key eviction on `block: u2 = (transient?0:1) + (used?2:0)` so
transient/unused images evict first. rustty's `Candidate`
(`crates/rustty-vt/src/kitty/graphics_storage.rs:921`) is `{ id, generation, used }` and the
comparator (`:836-857`) is `used` → `generation` → `id`; upstream head keys on `block` **first**.
`grep -rn "transient\|Usage" crates/rustty-vt/src/kitty/graphics_command.rs` → 0 today.

- [ ] **Step 2: NEGATIVE CONTROL** — port `transmission command with transient hint`, and **write the
eviction-order test upstream does not have** (area D §6): transient+unused evicts before
non-transient+unused before transient+used. Without it, `block` could be computed and never consulted
and every test would pass — this is a behaviour-visible eviction change with no upstream guard.

- [ ] **Step 3: Commit**

```sh
git commit -m "vt/kitty: transient usage hints re-key image eviction (oracle a65e11cc9) (P3-W6 T5)"
```

## Task 6: Viewport fingerprint storage reuse (`9659167ec`, `bc8bb6c0f`)

- [ ] **Step 1** — rustty is at the `c243d89bd` shape:
`fingerprint: Option<Fingerprint>` with `init`/`eql`/`deinit`
(`crates/rustty-vt/src/search/viewport.rs:41,123-197`). Port: non-optional `Fingerprint` (default
empty) — delete every `Option` unwrap at `:77,89,93,123-197`; `clear_and_retain_capacity()` (called by
`reset()` instead of dropping); replace `init` + `eql` with a single-pass
`update(&mut self, alloc, pages) -> Result<bool, OutOfMemory>` that calls
`ensure_total_capacity(pages.rows)` **first** (so an allocation failure preserves the old fingerprint),
overwrites in place tracking `changed`, then `shrink_retaining_capacity(i)` and returns
`changed || i != old_len`; invert the call site to `if !fingerprint.update(...)? { 'match: { … } }`;
iterate `fingerprint.entries` rather than `.nodes`, keeping the `end_append.last_row_wrapped`
trailing-overlap form.

- [ ] **Step 2: UB HAZARD (area D §4.3)** — upstream snapshots `fingerprint.entries.items` and in the
same scope calls `self.window.append_if_wrapped(node)` / `append(entry.node)`, which mutate the
`SlidingWindow` and can reallocate. `&self.fingerprint.entries` + `&mut self.window` are disjoint
fields so plain field borrows compile — **but** `ViewportSearch` reaches the `PageList` through a raw
alias, so `Fingerprint::update` must take `&mut self` (the fingerprint) and a `*mut PageList`, and must
not be reached via a `&mut self` method that also holds `&mut self.window`. If `fingerprint` is ever
put behind a raw pointer, switch the entries loop to `for i in 0..len` re-indexing each iteration.
**Do not reorder `ensure_total_capacity` after any mutation** — that ordering *is* the
allocation-failure contract.

Also respect `append_page`'s existing invariant, which upstream states explicitly: *"No pointer into
`page` may escape this function: compressed preserved pages own temporary decode storage, while
resident values borrow node memory."* No `&mut Node`/`&mut Page` may be formed while `page` is live, and
`Meta.rows` must be a **copy** (`page.size.rows`), never a re-read of `node.data.size.rows` — which is
what `83aada205` fixed.

- [ ] **Step 3: NEGATIVE CONTROL** — port `fingerprint only allocates on changes`. It must assert
**allocation count**, not just correctness. Then trip an allocation failure inside `update` and assert
the **old** fingerprint is intact — move `ensure_total_capacity` after the first write and that must
fail.

- [ ] **Step 4: Commit**

```sh
git commit -m "vt/search: reuse viewport fingerprint storage with a single-pass update (oracle 9659167ec bc8bb6c0f) (P3-W6 T6)"
```

## Task 7: `Selection` mixed-width tests and the `type_json` reconciliation

- [ ] **Step 1** — the `Selection.zig` clamping hunks are ported; the two **new** upstream tests are
not: `Selection: rectangle corners clamp across mixed-width pages` and
`Selection: containedRow clamps mixed-width pages`. Both build a mixed-width page list via
`pages.split(...)` + `second.page().size.cols = 2`. Port with `P3-W3`'s `split`/`page()` surface.
- [ ] **Step 2** — final `rustty_type_json()` reconciliation: upstream 34 / rustty 34, but the sets
differ. After `P3-W3 T5` deleted `RusttyTerminalOptions` and `P3-W6 T3` added the two effect structs,
assert both directions are empty and that the negative assertion for `RusttyTerminalOptions` is present.

- [ ] **Step 3: NEGATIVE CONTROL** — Step 1: the clamps are already ported, so these tests must be
verified to **fail without them** — temporarily revert one `min(…, cols-1)` arm and confirm. A test
ported onto already-correct code is unproven until you see it red. Step 2: A5's header-driven arm
covers it.

- [ ] **Step 4: Commit**

```sh
git commit -m "vt: mixed-width selection clamp regression tests; reconcile the type_json registry both directions (oracle b6f34be44 a9f5b7eba tests, 03d5fa268 negative assertion) (P3-W6 T7)"
```

## `P3-W6` finale

- [ ] A5 all arms green; D10 HARD and green; `cargo test -p rustty-vt-c --no-fail-fast` green.
- [ ] `bash .port/tools/gates/check_rustty_vt_headers.sh` and every `example/*/build.sh` green
(WS19 T16 made the dylib rebuild unconditional).
- [ ] Merge with both platforms green.

---

# `P3-W7` — OS / termio / platform

**Branch:** `p3-w7-platform`. Runs in **parallel** with `P3-W3`–`P3-W6`. `E16` (the global accessors)
lands first because `E12`, `E15` and `E20` all need it. `E1` (the read pipeline) lands **last** with a
Miri harness. **WS19 T17 already owns** the `posix_waitpid` `ECHILD` fix and the `threadEnter`
drain-before-`stop()` ordering — **out of scope here**, but this workstream must update the
`.port/phase-2/DEFERRALS.md` reachability paragraphs that `E1` widens (see Task 8).

## Task 1: `global` gains `environ` and `tmp_dir_path` (`E16`)

- [ ] **Step 1** — `crates/rustty-global/src/lib.rs:81-104` `GlobalState` has only `gpa`, `alloc`,
`action`, `logging`, `rlimits`, `resources_dir`. Add: the captured `environ` block + `environ()` /
`environ_map()` accessors, and `tmp_dir_path: Option<…>` + `tmp_dir_path()`. rustty's
`pub static STATE: OnceLock<GlobalState>` (`:68`) is **already** the shape upstream moved to
(`7121ab6c3` `state: ?GlobalState = null`), so no restructuring is needed. `io_impl` is N/A (no `std.Io`).
- [ ] **Step 2: the `InitOpts::tool` arm is N/A — record it so nobody adds it later.** Upstream needed
it because `main_bench.zig` funnels through the shared `global.init` and `+terminal-stream` failed with
`InvalidAction`. rustty's `crates/rustty-bench/src/main_bench.rs` never initializes global state
(`grep -n "STATE\|init_with_action\|global"` → 0 hits) and `crates/rustty-bench/src/cli.rs:46`
`Action::from_name` owns its own namespace, so the failure cannot occur. Same reasoning for
`main_gen.zig` → `rustty-gen`.
- [ ] **Step 3: the libghostty-vt import guard maps to a dependency-edge gate, not a comptime block.**
Upstream makes importing `global.zig` from the `lib` artifact a **compile error**; rustty's equivalent is
`.port/tools/gates/check_acyclic.sh` asserting no `rustty-vt`/`rustty-vt-c` → `rustty-global` edge. Add
that assertion explicitly (R4).

- [ ] **Step 4: NEGATIVE CONTROL** — add the `rustty-global` unit test for `environ_map()` /
`tmp_dir_path()`, then add a temporary `rustty-global` dependency to `crates/rustty-vt/Cargo.toml` and
confirm `check_acyclic.sh` FAILS. Revert. An edge gate that has never been seen red is not a gate.

- [ ] **Step 5: Commit**

```sh
git commit -m "global: capture the environment block and the temp dir; assert no lib-vt edge to rustty-global (oracle e8525c0fd a77c706a1 7121ab6c3 048619a6b) (P3-W7 T1)"
```

## Task 2: Byte-type the env family and fix a **live** non-UTF-8 panic (§3.1 + `E13`)

**Rationale.** This is a `String`-translation bug of exactly the class the byte-slice rule exists to
prevent, and it is **user-visible**. `crates/rustty-os/src/env.rs:43`:

```rust
pub fn getEnvMap() -> HashMap<String, String> {
    if is_flatpak() { HashMap::new() } else { std::env::vars().collect() }
}
```

`std::env::vars()` **panics** on any variable that is not valid UTF-8 (its docs say so; `vars_os()` is
the non-panicking form), and POSIX environment variables are arbitrary byte strings. `getEnvMap()` is on
the **surface-creation path** (`crates/rustty-core/src/surface.rs:2087`,
`crates/rustty-apprt/src/embedded.rs:3018`, `crates/rustty-apprt/src/gtk/class/surface.rs:2361`), so one
non-UTF-8 variable in the user's shell makes **every new surface panic**. The oracle at the pin returned
`std.process.EnvMap` over raw byte slices and had no such failure mode.

- [ ] **Step 1: RED — construct the reproducer.** Area E reported this as **inference, not a verified
crash**, and explicitly asked for a 5-minute check to decide P1 vs P3:

```sh
env FOO=$'\xff' cargo run -p rustty 2>&1 | tail -20
```

Record the outcome. **If it does not panic, downgrade the priority and say why** — do not carry an
unverified P1.

- [ ] **Step 2: GREEN** — re-type the whole family on `OsString`/`Vec<u8>`: `std::env::vars_os()`,
`std::env::var_os()`, and `OsStrExt::as_bytes` for the append/prepend arithmetic. Secondary sites:
`env.rs:117` `getenv` uses `std::env::var`, which maps a non-UTF-8 value to `None` — that matches Zig's
**Windows** branch (`InvalidWtf8` → null) but **not** POSIX, where `posix.getenv` returns the raw bytes,
so a non-UTF-8 `$PAGER`/`$LANG`/`$RUSTTY_LOG` is silently treated as unset. `env.rs:54,70,77`
`appendEnv`/`appendEnvAlways`/`prependEnv` take and return `&str`/`String`; PATH-like variables are byte
strings and a non-UTF-8 `PATH` component cannot round-trip.

- [ ] **Step 3: `E13` — move the functions where upstream moved them.** `appendEnv`/`appendEnvAlways`
become file-private in `crates/rustty-termio/src/exec.rs` (consumers at `:1923,1968,1996`);
`prependEnv` becomes file-private in `crates/rustty-termio/src/shell_integration.rs` (consumer `:609`).
**Move them as `&[u8]`/`Vec<u8>`, not as-is.** The four existing unit tests
(`appendEnv empty/existing`, `prependEnv empty/existing`) **move with the functions** — do not delete
them; `.port/tools/gates/check_tests_ported.py` needs the delta recorded.

- [ ] **Step 4: NEGATIVE CONTROL** — a `#[test]` that round-trips a **non-UTF-8** `PATH` component
through `append_env`/`prepend_env` and asserts the bytes are preserved. Revert one signature to `&str`
and it must fail to compile — which is the strongest control available for a type-level fix.

- [ ] **Step 5: Commit**

```sh
git commit -m "os/termio: byte-type the environment family (fixes a live non-UTF-8 surface-creation panic) and move append/prepend into their consumers (oracle e8525c0fd env.zig deletion) (P3-W7 T2)"
```

## Task 3: Thread environment maps explicitly and delete the test-side `setenv` race (`E15`, `f8e13f31e`)

**Rationale.** Two independent reasons, and the payoff is larger in Rust than in Zig.
(a) `crates/rustty-os/src/desktop.rs:211-234` (10 `setenv`/`unsetenv` calls) and
`crates/rustty-os/src/xdg.rs:302,334,357,389,435,438` mutate the **process** environment from
`#[cfg(test)]` code with **no serialization**. `setenv` is **not thread-safe against a concurrent
`getenv`** in glibc or Apple libc, and `cargo test` runs these on parallel threads by default — the exact
failure class recorded in memory `parallel-cargo-test-exposes-c-init-races`. (b) Rust 2024 makes
`std::env::set_var` `unsafe` for precisely this reason. **Porting E15 deletes the hazard rather than
papering over it with a global test mutex**, which would be strictly worse.

- [ ] **Step 1** — add the explicit env parameter to the six signatures:
`crates/rustty-os/src/desktop.rs:118 desktopEnvironment()`, `homedir.rs:25 home(buf)`,
`homedir.rs:228 expandHome(path, buf)`, `path.rs:27 expand(cmd)`, `file.rs:108 allocTmpDir()`,
`xdg.rs config(opts)`. `desktop::launchedFromDesktop` and `systemd::launchedBySystemd` read
`global::environ().get_posix(...)`.
- [ ] **Step 2** — rewrite every affected test to build a **local** env map; **delete** all 16
`setenv`/`unsetenv` calls from `#[cfg(test)]` code.
- [ ] **Step 3** — `E14`: `shell_integration::setup_bash` needs a map to find `HOME`
(`crates/rustty-termio/src/shell_integration.rs` uses `homedir::home(&mut buf)` today). Note upstream
uses `swapRemove` for `RUSTTY_BASH_ENV` and `orderedRemove` for `VTE_VERSION`/`LD_LIBRARY_PATH`; the map
is only iterated to build `envp`, where order is unspecified, so this is a fidelity nit, not a behaviour
change — match it anyway and say so.
- [ ] **Step 4** — `E19`/`E21` fold in here: `resourcesdir`'s env reads and the `open.rs` snap
`LD_LIBRARY_PATH` strip. `crates/rustty-os/src/resourcesdir.rs:39,49` are **already** `&self`
(upstream's new `*const` shape) — **do not "fix" that back**; rustty was ahead.
- [ ] **Step 5: `E17` is a deliberate NON-deletion — record it.** Upstream **deleted**
`os/hostname.zig`'s hand-rolled RFC 1123 `isValid` + its 25-assertion test in favour of
`std.Io.net.HostName.validate`. **Rust's std has no RFC 1123 hostname validator**, so mirroring the
deletion would silently drop validation. **Keep** `crates/rustty-os/src/hostname.rs:56` and its tests
(`:207-240`) — rustty's guard is now *stronger* than upstream's, which has none. Record the intentional
keep so a future mechanical sweep does not delete it. One caller:
`crates/rustty-cli/src/ssh_cache/disk_cache.rs:637`.
- [ ] **Step 6: `E18`** — `crates/rustty-os/src/locale.rs` already dropped the allocator
(Rust-natural). Add the two missing invariants: `assert!(!cfg!(test))` — *"does in-process mutation of
the environment and can't be run in a test"*, per the tripwire convention, `debug_assertions`-gated —
and the Windows early-return arm. `locale.rs:34` still calls `internal_os::getenv("LANG")`; switch to a
file-local `extern "C"` `getenv`, following upstream's own `locale.zig` precedent of declaring
file-local `setenv`/`unsetenv` after `os/env.zig` was deleted.

- [ ] **Step 7: NEGATIVE CONTROL** — add a **grep gate** banning `setenv`/`unsetenv` in `#[cfg(test)]`
code under `crates/rustty-os/`, and prove it RED by reverting one test, then GREEN. Plus a
`#[should_panic]` test that `ensure_locale()` refuses to run under `cfg(test)`. Plus: run
`cargo test -p rustty-os` **with default parallelism** 20 times and record zero flakes — the pre-fix
race is intermittent, so a single green run is not evidence.

- [ ] **Step 8: Commit**

```sh
git commit -m "os: thread environment maps explicitly and delete every setenv from test code; assert ensure_locale is never run under test (oracle e8525c0fd f8e13f31e) (P3-W7 T3)"
```

## Task 4: Delete `args.rs` and `env.rs` (the two upstream file deletions)

- [ ] **Step 1: `src/os/args.zig` (−130) → `crates/rustty-os/src/args.rs` (201 lines).** Upstream's
`ArgIterator` used `NSProcessInfo.arguments` via objc on macOS because under `NSApplicationMain` the
libc argv pointers Zig captures at startup are invalid. **The Rust motivation never existed:**
`std::env::args_os()` on macOS reads the real process argv (via `_NSGetArgv`) and is valid under
`NSApplicationMain`. Keeping `IteratorMacOS` means maintaining an `objc2`/`objc2-foundation` dependency
and a `getCString:`/`UTF8String` path for **zero benefit** — and that path is **lossy**:
`crates/rustty-os/src/args.rs:172` does `String::from_utf8_lossy(bytes).into_owned()` and `:66`/`:141`
return `Option<String>`, so a non-UTF-8 argv element is **corrupted** (a real concern for
`rustty -e <path with invalid UTF-8>`).

Migration order: (1) re-type the consumer seam on bytes —
`crates/rustty-args/src/args.rs:1626 args_iterator()`, `:1638 OsArgIterator`,
`crates/rustty-cli/src/action.rs:62,74 OsArgs` — to yield `OsString`; (2) repoint to
`std::env::args_os()`; (3) delete `crates/rustty-os/src/args.rs`, `pub mod args;`
(`crates/rustty-os/src/lib.rs:38`) and the `InitError` re-exports
(`crates/rustty-cli/src/rustty.rs:43,98-99`); (4) **keep** the `args` smoke test on the new seam —
upstream deleted it with the file, but the test-count contract needs the delta **recorded, not silently
dropped**. (5) The C-API argv path (`crates/rustty-apprt-c/src/lib.rs:239-263` `ARGV` `OnceLock`)
already matches upstream's new `.c` arm and its `Vec<Vec<u8>>` storage is already correctly byte-typed —
**keep it**. Note `ghostty_init(argc, argv)`'s **signature is unchanged** from the pin; only what it does
with argv changed.

- [ ] **Step 2: `src/os/env.zig` (−178) → `crates/rustty-os/src/env.rs` (232 lines).** Delete the file,
`pub mod env;` (`lib.rs:45`) and the 8 re-exports (`lib.rs:126-133`). Consumer migration, exhaustive:

| Site | → |
|---|---|
| `crates/rustty-termio/src/exec.rs:1923,1968,1996` | the moved fns (Task 2) |
| `crates/rustty-termio/src/shell_integration.rs:609` | the moved fn (Task 2) |
| `crates/rustty-core/src/surface.rs:2087` | `global::environ_map()` — **the panic site** |
| `crates/rustty-apprt/src/embedded.rs:3018` | `global::environ_map()` |
| `crates/rustty-apprt/src/gtk/class/surface.rs:2361` | `global::environ_map()` |
| `crates/rustty-apprt/src/lib.rs:199` (`RUSTTY_LOG`) | `global::environ().get_alloc(...)` — upstream moved this read **into** `global.init` |
| `crates/rustty-cli/src/pager.rs:139,142` | env-map lookup (`P3-W9`) |
| `crates/rustty-os/src/locale.rs:34` | file-local `extern "C" getenv` (Task 3) |
| `crates/rustty-apprt/src/gtk/class/application.rs:1826,1845,2080,2090` | **genuine process-env mutations with no upstream replacement in `os/`** — keep file-local `extern "C"` decls in the GTK module per the `locale.zig` precedent, plus a `sync_environ()` call. `P3-W8` owns the GTK side. |
| `crates/rustty-os/src/desktop.rs:211-234`, `xdg.rs:302-438` (tests) | **deleted** (Task 3) |

- [ ] **Step 3: `E20`** — `crates/rustty-os/src/file.rs:221 randomTmpPath` stops calling
`allocTmpDir()`+`freeTmpDir()` per call and uses the cached `global::tmp_dir_path()`; drop the
`allocTmpDir`/`freeTmpDir` re-exports (`crates/rustty-os/src/lib.rs:135,139`).
**`crypto_random_bytes` (`file.rs:206`) STAYS.** Upstream's switch to `global.io().random()` is a
`std.Io` plumbing artifact; the basename is used for temp-path **unguessability**, so do **not** weaken
it to a non-cryptographic RNG "for fidelity".

- [ ] **Step 4: NEGATIVE CONTROL** — `grep -rn 'rustty_os::env\|internal_os::env\|rustty_os::args' crates/`
must return **zero** hits outside the GTK `extern "C"` block, and the workspace must build. Plus: assert
`args_iterator()` preserves a non-UTF-8 argv byte exactly — the lossy `from_utf8_lossy` is the defect
being removed, and a UTF-8-only test cannot see it.

- [ ] **Step 5: Commit**

```sh
git commit -m "os: delete args.rs and env.rs, migrating every consumer to global accessors and byte-typed argv (oracle e8525c0fd deletions) (P3-W7 T4)"
```

## Task 5: The small `Command` and QoS items (`E29`, `E4`)

- [ ] **Step 1: `E29`** — `crates/rustty-termio/src/command.rs:1245 execvpeZ` is already byte-equivalent
to upstream's inlined PATH search (same default `b"/usr/local/bin:/bin/:/usr/bin"` at `:1261`, same
error precedence, same `PATH_MAX` buffer, called from `:484`). **One detail off:** `:1270`
`let err = ExecveError::FileNotFound;` is **never reassigned**, whereas upstream updates `err` each
iteration — so if the final PATH entry yields `ENOTDIR`, upstream reports `NOTDIR` and rustty reports
`NOENT`. Make it `mut` and assign from each `execve` result before the `match`. 2 lines. Observable only
in the child's stderr text (`NOENT` prints the friendly "not on the PATH" message; anything else prints
"unexpected error: E<name>").
- [ ] **Step 2: `E4`** — `setQosClass(.user_initiated)` on the read thread(s), macOS only. The primitive
exists (`crates/rustty-os/src/macos.rs:85`) with exactly one caller
(`crates/rustty-core/src/renderer_seam.rs:986`); `grep -rn "setQosClass" crates/rustty-termio/` → 0.
Upstream measured a **15 % throughput delta on M4 Max**. This is **separable from `E1`** and benefits
the current single read thread — land it now.

- [ ] **Step 3: NEGATIVE CONTROL** — Step 1: a test asserting the reported errno for a PATH whose last
entry is a **file, not a directory** (`ENOTDIR`); the current code reports `NOENT`, so the test is red
today. Step 2: assert `pthread_get_qos_class_np` **inside the spawned read thread** returns
`user_initiated`; a test that only checks the call did not error is blind to it being made on the wrong
thread.

- [ ] **Step 4: Commit**

```sh
git commit -m "termio: latch the real execve errno across the PATH search and raise the read thread's QoS class (oracle a77c706a1 2f0e6659d) (P3-W7 T5)"
```

## Task 6: `E10` — the two stream-handler slice arms

- [ ] **Step 1** — add `.print_slice => self.terminal.print_slice(value.cps)` and
`.apc_put_slice => self.apc.feed_slice(self.alloc, value.bytes)` to
`crates/rustty-termio/src/stream_handler.rs`. Blocked on `P3-W5 T6`.
- [ ] **Step 2: NEGATIVE CONTROL** — assert a multi-codepoint run through `print_slice` produces the
**same screen** as the same bytes fed one at a time. Existing per-codepoint KATs stay valid but cannot
see the new arm at all; the equivalence assertion is the only real control.
- [ ] **Step 3: Commit**

```sh
git commit -m "termio: dispatch the bulk print and APC slice actions (oracle f6f79acce 47e26df60) (P3-W7 T6)"
```

## Task 7: `E1` — the two-stage pty read pipeline (**last, with a Miri harness**)

**Rationale and the decision (area E §7.1).** This is a pure-performance change: the macOS kernel tty
output queue caps master reads at ~1 KiB, so the old serial loop stalls producers. **Port it, last.**
Two reasons not to skip: the `perf-baseline-rust-vs-zig` result was measured against a **pre-pipeline**
oracle, so skipping it means losing the bulk-output comparison on the platform rustty targets first; and
the `Pipeline` struct is self-contained and testable in isolation. **If it is deferred instead, it must
be recorded as a performance deferral with a measured gap, not left silently absent.**

- [ ] **Step 1** — split `crates/rustty-termio/src/exec.rs:3323 ReadThread::threadMainPosix` (today one
`'tight:` loop, one 2-fd `poll`, `let mut buf = [0u8; 1024]` at `:3411`) into a **gather stage**
(`gather_main_posix`, new thread `io-gather`) and a **parse stage**, coupled by a 4-slot × 64 KiB ring:
`Pipeline { mutex, batch_ready, slot_free, lens, head, tail, count, bridging, idle self-pipe, done }`,
`bridge_spin_max = 16`, `bridge_poll_timeout_ms = 1`, `bridge_threshold = 1024`,
`gather_budget_ns = 3ms`, a 3-fd poll set, and the buffer growing 1 KiB → 64 KiB. Fold in `bb0ac4c72`
("don't bridge pty reads while the parser is idle").

- [ ] **Step 2: `E2` is a TRAP — do not port it.** `bed47168c` ("bound POSIX read-ahead on non-Darwin")
was **REVERTED** by `60121a039` inside this range; the net diff of the pair is empty
(`bed47168c` is the older commit). Anyone porting commit-by-commit rather than from the range diff will
port the reverted version.

- [ ] **Step 3: UB HAZARD — the ring's core invariant is a Stacked-Borrows hazard.** Upstream's
invariant is "buffer contents need no locking, only ring metadata is guarded" — i.e. one thread holds a
slot's bytes while another holds an alias to the ring. In Rust that is **raw-pointer slot handoff**, not
`&mut` (memory: `rust-mut-args-strongly-protected`). Build the Miri harness **first**: feed a known
byte stream through the ring from a producer thread and assert byte-exact delivery **and ordering** on
the consumer, under Miri. Do not wire it into the FFI build until that harness is clean.

- [ ] **Step 4: Re-site `E3`.** `crates/rustty-termio/src/exec.rs:3489-3505`'s
`(*(*io).renderer_state).yield_to_demand()` is correct for the **current** loop shape and carries a PORT
NOTE saying so (the reasoning is verified: `process_output` locks *and* unlocks internally, so the mutex
is not held at the call). When the pipeline lands, it moves to the parse loop's **post-tail-advance**
point, right after `slot_free.signal`. **Do not lose it in the rewrite** — that is the entire content of
`d34b54e9b` on this side.

- [ ] **Step 5: Update the two `DEFERRALS.md` reachability paragraphs that `E1` widens.**
(a) The bounded-`killPid` divergence (`.port/phase-2/DEFERRALS.md:5128`) **strengthens**: the pipeline
returns early in two **new** places — `set_nonblock(fd)` failure and `io-gather` spawn failure — both of
which leave a forked child with **no reader on the pty master** while `threadEnter` has already
succeeded, so `threadExit`'s `stop()` runs into exactly the `?Es` wedging condition the deadline
protects against. Add those two arms to the "Reachability in PRODUCTION" paragraph. **Keep
`KILL_ABANDON` and the `elapsed >= KILL_ABANDON` check exactly where they are**
(`crates/rustty-termio/src/exec.rs:3083-3095`): upstream's loop is **still `while (true)`** at
`6ad1fe7d8`, so the divergence is unchanged in kind and is still the only thing between rustty and an
unclosable window. Do not let a mechanical rebase drop it. Upstream now passes `null` for the status
pointer (it never read the status) — cosmetic, no action.
(b) The `threadEnter` drain-before-`stop()` deferral (`DEFERRALS.md:5044`) is **untouched upstream** —
verified: the `errdefer self.subprocess.stop();` ordering and the subsequent `errdefer`s are context
lines in this range — but `E1` adds a **new class** of the same bug (the two arms above are not
`threadEnter` error arms at all). WS19 T17 owns the fix; this task **folds it into the pipeline work**,
because the pipeline already needs a "drain the master once" concept (the gather stage **is** that), so
the restructuring is cheap now and expensive later. Update the deferral entry to say so.

- [ ] **Step 6: NEGATIVE CONTROL** — the existing KAT `readthread_pty_bytes_land_in_screen`
(`crates/rustty-termio/src/exec.rs:4925`) covers the current shape and **would still pass after a
pipeline port**, so it is **not** a regression guard. Two new assertions are required: (a) a
**saturated-producer throughput** assertion (write ≫ 1 KiB from the child and assert the delivered byte
count over a fixed window exceeds the pre-pipeline rate); and (b) a **latency** assertion for a sub-1 KiB
interactive write (assert it is delivered without waiting for `gather_budget_ns`). Plus the Miri
ordering harness from Step 3, whose control is: deliberately drop the `slot_free` wait and confirm Miri
or the harness reports out-of-order/torn delivery.

- [ ] **Step 7: Commit**

```sh
git commit -m "termio: two-stage pty read pipeline with a 4x64 KiB ring, raw-pointer slot handoff, Miri-proven ordering (oracle 2f0e6659d bb0ac4c72 d34b54e9b re-site) (P3-W7 T7)"
```

## Task 8: Area E's N/A record

- [ ] **Step 1** — write the deliberate-non-port record into `.port/phase-2/DEFERRALS.md`'s
`# Justified R1 Divergences` (or a new `.port/phase-3/NON_PORTS.md` referenced from it), each with the
4-criterion basis: `src/quirks_memset.zig` (`E34`, with the perf-baseline note);
`os/hostname.zig`'s `isValid` deletion (`E17`, keep rustty's);
`os/windows.zig`'s rewrite (`E25` — rustty's `windows.rs` already declares its own `extern` set, so it
is **already at the destination shape**); `os/mach.zig`'s `next_mmap_addr_hint` (`E24`);
`os/flatpak.zig`'s `@cImport`→`gio_c` (`E26`); `src/main_ghostty.zig`'s iOS
`allow_stack_tracing = false` (`E36` — worth remembering **if** rustty ever targets iOS, but it is a Zig
unwinder issue with no Rust counterpart); `src/main.zig`'s `std.testing.log_level` (`E35` — Rust's
`cargo test` captures per-test output); `src/tripwire.zig` (`E38` — **doc comment only**; `enabled()`
semantics unchanged, so the `cfg!(debug_assertions)` equivalence needs **no rebase**).
- [ ] **Step 2: two upstream bugs that must NOT be reproduced.**
`src/os/main.zig`'s doc typo *"low-level operations, butos/main.zig"* (`E28`) and `src/pty.zig`'s
`@tagName(posix.errno(-1))` at two sites, which reports the errno of the literal `-1` rather than the
real errno (`E32`, cosmetic log text). The prose-fidelity carve-out covers obvious upstream mistakes.
- [ ] **Step 3: NEGATIVE CONTROL** — the record's control is WS19's G-P2 closure gate: every row must
name a criterion set and an owner, or G-P2 fails. Run it.
- [ ] **Step 4: Commit**

```sh
git commit -m "port: record area E's deliberate non-ports with 4-criterion bases, including quirks_memset and the hostname.isValid keep (P3-W7 T8)"
```

## `P3-W7` finale

- [ ] `cargo test -p rustty-os -p rustty-termio -p rustty-global --no-fail-fast` green on **both**
platforms; `cargo test -p rustty-os` run 20× with default parallelism, zero flakes.
- [ ] The Linux `desktop`/`xdg` tests re-run **on the VM** to confirm the `setenv` removal (that is the
right moment to use it — with the standing caution: **never** run a heavy `cargo build` concurrently
with headed compositor work on that VM).
- [ ] Miri ring harness green with its recorded negative control.
- [ ] Merge with both platforms green.

---

# `P3-W8` — UI tiers (macOS, renderer, font, apprt, inspector)

**Branch:** `p3-w8-ui`. Runs in parallel with Layer 2. **WS19 T4 already owns** the one-`gtk4::init()`-
per-binary fix (area F's §5 pre-work) and **WS19 T7** owns the inspector `CellChooser` restore path —
both **out of scope**. `metal/buffer.rs` is **`P3-W0 T5`**. Ordering inside: macOS first (highest
verified-value-per-hour, zero cross-area deps, a real `xcodebuild` gate, and two upstream test files
port essentially verbatim).

## Task 1: The macOS Swift block, part 1 — pasteboard (`49806fc4c`, `acd09c0a6`, `6e267d336`, `10565995b`)

- [ ] **Step 1: `49806fc4c`** — rewrite
`macos/Sources/Helpers/Extensions/NSPasteboard+Extension.swift:40-49`'s
`readObjects(forClasses:[NSURL.self])`-first ladder as `getOpinionatedStringContents`: per
`pasteboardItem`, in order — file URL → escaped path, else the item's plain string; join with a space;
nil when nothing usable. Fixes remote file promises (Transmit/Nova) **and** ordering.
- [ ] **Step 2: `acd09c0a6`** — port `macos/Tests/NSPasteboardTests.swift` (+224, **17 `@Test` cases**).
rustty's file is **33 lines**. All cases use **private named pasteboards**, so they neither touch the
user's clipboard nor need serialization.
- [ ] **Step 3: `6e267d336`** — `macos/Sources/RusttyKit/Surface View/SurfaceView_AppKit.swift:2214-2217`
drops `.URL` from `dropTypes`, and `:2237-2254`'s bespoke 18-line drop ladder becomes
`pb.getOpinionatedStringContents()`. Depends on Step 1.
- [ ] **Step 4: `10565995b`** — `macos/Sources/Features/Services/ServiceProvider.swift:36` gains
`options: [.urlReadingFileURLsOnly: true]`.

- [ ] **Step 5: NEGATIVE CONTROL** — the 17 ported cases are the control, but they must be seen **red
first**: run them against the un-rewritten helper and record which fail (expect the ordering and
file-promise cases). For Step 4, add a case asserting a **non-file** URL on the pasteboard is ignored.

- [ ] **Step 6: Commit**

```sh
xcodebuild test -project macos/Rustty.xcodeproj -scheme Rustty
git commit -m "macos: opinionated pasteboard string extraction with 17 ported tests; file-URL-only services reads (oracle 49806fc4c acd09c0a6 6e267d336 10565995b) (P3-W8 T1)"
```

## Task 2: The macOS Swift block, part 2 — lifetimes, cache, AppKit 27

- [ ] **Step 1: `0274e7ad8` + its test** — `QuickTerminalScreenStateCache.swift:112-113` uses
`screenSize.width <= … && height <= …` and `:77` grows `entry.screenSize` on a larger screen; require
**exact** geometry (`scale == backingScaleFactor && screenSize == frame.size`) and stop growing. Fixes a
stale partial-size restore after display reconnect. Port
`macos/Tests/QuickTerminalScreenStateCacheTests.swift` (+61) with a `MockSizedScreen` mirroring the
existing `macos/Tests/NSScreenTests.swift:84 MockNSScreen`.
- [ ] **Step 2: `a6edca2d7` BEFORE `35790a7e5`.** `a6edca2d7`: `Rustty.Surface.swift:26-36` frees the
surface **synchronously** when `deinit` already runs on the main thread (today it is an unconditional
`Task.detached { @MainActor in rustty_surface_free(surface) }`, which leaves unretained userdata
registered) — a teardown-ordering / use-after-free class. `35790a7e5` is a **revert** of "fix undo new
tab will cause a crash": drop the `DispatchQueue.main.async` wrapper at
`macos/Sources/Features/Terminal/TerminalController.swift:507-513` **and its now-obsolete comment citing
ghostty PR #9512**. The revert is only safe once `a6edca2d7` is in — **do not port a revert as "new
work" without confirming the real fix landed.**
- [ ] **Step 3: `f52f8aab9`** — `SurfaceView_AppKit.swift:295` `.publisher(for: .rusttySelectionDidChange, object: self)`
becomes `publisher(for:)` **without** `object:` plus a `.filter { [weak self] … }` identity check, to
break a notification-publisher retain cycle.
- [ ] **Step 4: `751a60df6`** — route preedit commits through key events: capture `hasMarkedText()`
**before** `unmarkText()` and, when a marked-text commit produces chars, send
`committedPreeditTextAction(RUSTTY_ACTION_PRESS, text:)` instead of `sendText`. The helper already
exists at `SurfaceView_AppKit.swift:1486` (used from `:1186`), so this is a 6-line change at
`:2039-2050`.
- [ ] **Step 5: macOS-27-only** — `2104e0749` hide `NSScrollPocket` in
`HiddenTitlebarTerminalWindow.swift:73-84` (note: the existing `NSScrollPocket` hits in
`SurfaceScrollView.swift:124-340` are a **different** frame-change workaround); `632aa6756` accept
`NSTitlebarAccessoryContainerView` as well as `…ClipView` at
`TitlebarTabsTahoeTerminalWindow.swift:167`; `fa3802a70` `Backport.swift:110-115` `frameResize(…)` →
`columnResize(directions:)`/`rowResize(directions:)` per HIG.

- [ ] **Step 6: NEGATIVE CONTROL** — Step 1's ported test is the control (assert a **larger** screen
does **not** hit the cache). Step 2: a test double counting `rustty_surface_free` calls, asserting the
free happens **before `deinit` returns** on the main thread. Steps 5: these are visual/OS-version-bound
— **mark them "ported, unverified on macOS 27" rather than green**, with the recipe (screenshot check on
a macOS 27 beta host) recorded in `VERIFICATION_DEBT.md`. Do not claim green for an unverifiable item.

- [ ] **Step 7: Commit**

```sh
git commit -m "macos: exact-geometry quick-terminal cache, synchronous main-thread surface free, publisher retain-cycle fix, preedit commits via key events, macOS 27 titlebar workarounds (oracle 0274e7ad8 a6edca2d7 35790a7e5 f52f8aab9 751a60df6 2104e0749 632aa6756 fa3802a70) (P3-W8 T2)"
```

## Task 3: `src/font` — the `z2d` native path inset (`d0e72a3ab`)

**Sequence matters; this is the area's second-biggest cost because the dependency must move first.**

- [ ] **Step 1: port `Path::offset` into `crates/rustty-z2d/src/path.rs` with its OWN unit tests.**
`crates/rustty-z2d/Cargo.toml:1` states the port targets **z2d 0.10.0**; upstream moved to **0.12.1**,
and `crates/rustty-z2d/src/path.rs` has **no `offset`**. Tests: offset of a closed rect, of an open
path, a negative inset, and degenerate/zero-length segments. **Do not let the sprite goldens be the only
witness for a new geometry algorithm.**
- [ ] **Step 2** — rewrite `crates/rustty-font/src/sprite/canvas.rs:718…` `innerStrokePath`: the
two-surface fill×stroke mask workaround (fill_sfc + stroke_sfc + manual alpha multiply + composite, with
its long PORT NOTE about the growable-clone workaround) collapses to
`path.offset(alloc, -line_width/2)` + `stroke_path`.
- [ ] **Step 3** — `crates/rustty-font/src/sprite/draw/powerline.rs:513` `staticPath::<4>()` →
`<6>` with a leading `line_to(1.0, 0.0)` and a trailing `line_to(0.0, float_height)` to force butt caps
perpendicular (`drawE0B5`).
- [ ] **Step 4** — regold **only** the 6 affected files. `cargo clean` **before** regenerating goldens
(memory: `rename-casing-fidelity`).

- [ ] **Step 5: NEGATIVE CONTROL — the expected pixel magnitude is known, use it.** Measured from the
oracle: ~**76** differing pixels (max delta 48) in the U+E0xx pair and ~**3** (max delta 16) in the
U+25xx pair. **If a regold moves far more than that, the `offset` port is wrong, not the goldens.**
That numeric expectation is the control; a "goldens updated, tests pass" workflow has none.

- [ ] **Step 6: Commit**

```sh
git commit -m "z2d/font: native Path::offset replaces the two-surface inner-stroke mask; drawE0B5 forces perpendicular butt caps; 6 goldens regolded (oracle d0e72a3ab) (P3-W8 T3)"
```

## Task 4: `src/font` — the missing SVG guard test (`b988efcfe`)

- [ ] **Step 1** — the behaviour is present and **stricter** than upstream:
`crates/rustty-font/src/opentype/svg.rs:62-64` has the offset bounds check, and `:75-79` adds a further
record-count guard upstream still lacks (documented as R6 UB-avoidance). The **test** is missing:
`svg.rs:169-178` has only `fn svg()`. Port upstream's `"SVG document list offset past end"` verbatim,
**plus** a second case for the record-count guard so the stronger behaviour is pinned too.
- [ ] **Step 2: NEGATIVE CONTROL** — remove the `:62-64` check and the first test must fail; remove
`:75-79` and only the second must fail. Two independent controls for two independent guards.
- [ ] **Step 3: Commit**

```sh
git commit -m "font/opentype: pin the SVG document-list bounds guards with tests (oracle b988efcfe) (P3-W8 T4)"
```

## Task 5: apprt non-hotkey items (`2f3814ca5`, `004c88e41`, `5ea45919d`)

- [ ] **Step 1: `2f3814ca5` honor suspended window state** —
`crates/rustty-apprt/src/gtk/class/surface.rs:2741` `fn update_occlusion(&self, visible: bool)` becomes
the no-argument `pub` form computing `mapped && !window_suspended()`, with `window_suspended()` via
`gtk::Window::is_suspended`; callers at `:3574,3581` change; `class/window.rs` connects
`notify::suspended` → `prop_suspended` → every displayed surface's `update_occlusion()`. The missing
behaviour is "keep rendering an occluded window" — a real power regression. **Coordinate with
`P3-W4 T7 Step 3`**, which changes the occlusion path on the core side.
- [ ] **Step 2: `004c88e41` clamp to the current monitor** — new `toplevel_compute_size` in
`crates/rustty-apprt/src/gtk/class/window.rs` (skip when maximized/fullscreen/quick-terminal; skip on
0-size or 0-bounds; re-assert size **only** when it exceeds compositor bounds) + connect
`gdk::Toplevel::compute-size` with `after = true` on the realize path.
**Confirm the binding first:** `Toplevel::connect_compute_size` + `ToplevelSize::bounds`/`set_size` must
exist in the pinned `gdk4` 0.11. **If the binding is missing, the cost moves from M to L** (manual
`gdk_toplevel_size_*` FFI) — decide before estimating.
- [ ] **Step 3: `5ea45919d` `gtk-horizontal-tab-scroll`** —
`crates/rustty-apprt/src/gtk/class/surface.rs:3299-3355` `scroll_horizontal` runs the tab-switch
accumulator unconditionally for `unit == Surface`; it must run **only** when the option is on **and**
the unit is `.surface`, and otherwise **forward** horizontal scroll to the terminal via
`scroll_callback(scaled.x * -1, 0, …)` (neovim etc.). The config field is `P3-W9 T3`.

- [ ] **Step 4: NEGATIVE CONTROL** — Step 1: on the VM, run headed, minimise (or fully cover) the
window, and assert the occlusion callback fires **and frame production stops** (observable: the existing
occlusion log line + a frame counter). Step 2: reproducing the clamp needs the window to move to a
**larger** monitor, i.e. two heads, and the VM has one — so unit-test the **guard conditions**
(maximized/fullscreen/quick-terminal/zero-size/zero-bounds early returns) and record honestly: *"logic
reviewed + guard conditions unit tested, geometry effect unverified"*. **Do not mark it green.** Step 3:
extract the predicate (`tab_scroll_enabled && unit == Surface`) so it is testable without a widget, and
assert the **forwarding** arm — a test of only the accumulator arm passes without the new behaviour.

- [ ] **Step 5: Commit**

```sh
git commit -m "apprt/gtk: honor xdg_toplevel suspended, clamp the max window size to the current monitor, honor gtk-horizontal-tab-scroll (oracle 2f3814ca5 004c88e41 5ea45919d) (P3-W8 T5)"
```

## Task 6: The Wayland hotkeys series, part 1 — the protocol-independent half (F-W1, F-W2)

**Verdict (area F §3): the winproto path is ADDITIVE with PREFERENCE, not a replacement.** Both
mechanisms ship upstream simultaneously; the portal path is untouched and still required. The choice is
per-session at runtime, keyed on whether the compositor advertises `vicinae_hotkey_manager_v1`; `noop`
and `x11` return `false` unconditionally, so **nothing in rustty's shipped WS15 portal implementation
becomes dead code**. And the two mechanisms are **not** behaviourally equivalent: the portal is a
session-scoped, user-mediated, persisted grant, while vicinae-hotkey is ephemeral, per-object and
client-chosen, with the compositor answering each request `bound` / `denied(reason,message)` /
`revoked(reason,message)`. **That per-binding failure channel is what motivated the other half of the
series** — the `BindFailed` boxed type, the `bind-failed` signal, and
`globalShortcutBindFailed` raising a **desktop notification** (not a toast — it can fire before any
window exists). **Porting the winproto arm without the failure channel would produce silent no-op
keybinds.**

- [ ] **Step 1: F-W1** — `key::keysym_from_trigger` + its upstream test verbatim;
`winproto::App::{bind_global_shortcuts, clear_global_shortcuts}` with `noop`/`x11` returning `false`;
`GlobalShortcuts::BindFailed` + the `bind-failed` signal + public `emit_trigger`/`emit_bind_failed`;
`Application::global_shortcut_bind_failed` → `desktop_notification`
(`crates/rustty-apprt/src/gtk/class/application.rs:2995`).

  **IMPLEMENTATION HAZARD:** `Application::winproto()` returns `Ref<'_, winproto::App>`
  (`class/application.rs:2454`) while `clear_global_shortcuts` needs `&mut` — route through
  `winproto_mut()` (`:2462`) and make sure `close()` does **not** hold a `Ref` across the call, **or you
  convert an upstream crash-fix into a `RefCell` double-borrow panic.**

- [ ] **Step 2: F-W2 / `0a117e079`'s tail** — `refresh()` tries winproto first, then
`if dbus_connection.is_none() { return; }`; `close()` calls `clear_global_shortcuts()` unconditionally.
The crash-safety half of `0a117e079` is **already present and independently derived**
(`crates/rustty-apprt/src/gtk/class/global_shortcuts.rs:254-300` hoists the subscription drops above the
connection check with a PORT NOTE; `:349-360` `request()` returns instead of `.?`-unwrapping).
**Divergence decision: KEEP rustty's "headless refresh still builds the map"** (`:227-249`) — two
existing KATs depend on it (`:974 global_shortcuts_refresh_builds_map_without_dbus`,
`:990 …_clears_prior_map`) and upstream's early return is only an optimisation with no observable
difference once a bus exists. **Record it in the port ledger.**

- [ ] **Step 3: NEGATIVE CONTROL** — headless unit tests for the keysym mapping and the notification
**body formatting** (pure string assembly), plus a signal-emission KAT in the style of
`global_shortcuts_activation_emits_trigger` (`global_shortcuts.rs:1009`). Then, on the VM, extend
`.port/tools/gates/ws15_globalshortcuts_portal_gate.sh` with **two** assertions: the log must **not**
contain `global shortcuts bound via winproto` (mutter does not advertise the global), and the existing
6-step portal chain must still go green. On mutter that is the **real-world** path, so it is a genuine
regression gate for the whole series.

- [ ] **Step 4: Commit**

```sh
git commit -m "apprt/gtk: winproto-first global-shortcut binding with a per-binding bind-failed notification channel (oracle 88bd4fdce 0a117e079 partial) (P3-W8 T6)"
```

## Task 7: The Wayland hotkeys series, part 2 — the wire half (F-W3, F-W4)

**Recorded facts that shape this task:** `vicinae_hotkey_v1` is an **unratified, testing-phase**
protocol — the XML's own description says *"currently in the testing phase. Backward incompatible
changes may be added."* It is authored by the vicinae launcher project, is **not** in
`wayland-protocols`, and **no mainstream compositor (mutter, KWin, wlroots) implements it today**, so on
the project's own GNOME VM `globals.get(...)` returns null and every run takes the portal path. rustty
has **no crates.io binding** for it. **Decision: port it** — the port's contract is fidelity, and the
arm is inert on every compositor that does not advertise the global, so the blast radius is zero. But
**vendor the XML with its upstream copyright header intact (MIT, Aurelien Brabant)** and record in the
port ledger that the protocol is testing-phase, so a future incompatible bump is expected rather than
surprising.

- [ ] **Step 1: F-W3** — vendor `vicinae-hotkey-v1.xml` (305 lines) verbatim under
`crates/rustty-apprt/protocols/`, generate client bindings with `wayland-scanner`
(`generate_client_code!`) in `crates/rustty-apprt/build.rs` — the same mechanism `wayland-protocols`
uses internally, so it is well-trodden, but it is **new machinery for this crate**. Add the
`Tag::VicinaeHotkeyManager` arm to `crates/rustty-apprt/src/gtk/winproto/wayland/globals.rs:60-70` and
the registry bind at `:246-266`.
- [ ] **Step 2: F-W4** — `Hotkeys` itself (164 lines): the entry-selection logic
(`leaf.flags.global`, skip `catch_all`, skip multi-action), `manager.bind`, the listener state machine,
and `Entry::fail` → `emit_bind_failed`.
- [ ] **Step 3** — the 3 new i18n msgids land **with this task**, not before:
`"The keybind was revoked by the system."`, `"The keybind was denied by the system."`,
`"Global keybind unavailable"`. Their `msgstr` is **empty in all 32 upstream files**, so landing them
early creates ~96 catalog lines that translate nothing and are reachable from nothing — and WS13 T1's
`code_msgids_all_wrapped_and_in_pot` gate is written to **fail** on exactly that shape.

- [ ] **Step 4: NEGATIVE CONTROL** — the **entry-selection logic is pure over a `Config` and needs no
compositor**; that is where most of the porting risk lives, so unit-test it there (global-only,
`catch_all` skipped, multi-action skipped, order preserved). For the **wire** half, the honest position
is verification debt with a runnable recipe: a minimal fake compositor in a `#[test]`-local
`wayland-server` (0.31, sibling of the vendored `wayland-client`) advertising
`vicinae_hotkey_manager_v1` on its own socket, with the client pointed at it via `WAYLAND_DISPLAY`.
That is a genuinely runnable test, ~a day of scaffolding — **schedule it as its own task rather than
pretending F-W4 is verified**. Interim marker: `#[test] #[ignore = "needs fake vicinae compositor"]`, so
the debt is visible in the test list and not only in a document. Also assert graceful **absence** (the
established pattern: the KDE globals are absent on GNOME and the code copes).

- [ ] **Step 5: Commit**

```sh
git commit -m "apprt/gtk: vendor vicinae-hotkey-v1 and bind global shortcuts through the Wayland manager (oracle 88bd4fdce c3b5cab94 7ee3ac9ec 0075c75b6 3024c5d19) (P3-W8 T7)"
```

## Task 8: Inspector metadata-only traversal assertion; renderer image staleness test

- [ ] **Step 1** — `6d5dda40d`'s page-tree half **is** ported
(`crates/rustty-inspector/src/widgets/pagelist.rs:166 compressionTable`, `:248 page_if_resident`,
`:319 page_preserving_state`, `:478-480 memory_stats()`, `:413-435` the limit-row swap;
`widgets/page.rs:156-168`), and WS19 T7 owns the `CellChooser` path. What is missing is a **headless
assertion** that the traversal is metadata-only: build a `PageList`, compress a node, run the draw path,
assert `memory_stats().compressed_pages` is **unchanged**. That invariant is already written down at
`widgets/pagelist.rs:21-26` and **unenforced** for this path.
- [ ] **Step 2** — `bdc0b6c19`'s renderer half is ported
(`crates/rustty-renderer/src/image.rs:288 next_generation()`, `:926`, `:938`, `:980`, `:1170`) but has
**no dedicated staleness test**. Add a same-id/different-generation replace assertion; cheap.

- [ ] **Step 3: NEGATIVE CONTROL** — Step 1: replace one `page_if_resident` call with `page()` and the
new test must fail. Step 2: pass a stale generation and assert the image is **not** replaced; a
"replace works" test passes with the comparison removed.

- [ ] **Step 4: Commit**

```sh
git commit -m "inspector/renderer: assert metadata-only pagelist traversal and kitty image generation staleness (oracle 6d5dda40d bdc0b6c19 guards) (P3-W8 T8)"
```

## `P3-W8` finale

- [ ] `xcodebuild test -project macos/Rustty.xcodeproj -scheme Rustty` green.
- [ ] `cargo test -p rustty-font -p rustty-z2d -p rustty-renderer -p rustty-inspector --no-fail-fast`
green; the 6 regolded PNGs within the expected pixel magnitude.
- [ ] VM: `bash .port/tools/gates/ws15_globalshortcuts_portal_gate.sh` green with the two new
assertions; headed occlusion check recorded.
- [ ] **R5 platform-gating discipline:** do **not** attempt to prove "the other platform's artifact is
unchanged" by hashing binaries — a prior lane got three different hashes from rebuilds of identical
source and correctly moved to A/B behaviour comparison. Use: (a) `cargo tree -e features` byte-identical
before/after for the untouched platform's feature set; (b) the untouched platform's test output (names +
pass/fail) identical; (c) confirm `metal/buffer.rs` sits inside a `#[cfg(target_os = "macos")]` module
tree so the Linux build never parses it.
- [ ] Unverifiable items marked "ported, unverified on macOS 27" / "geometry effect unverified" in
`VERIFICATION_DEBT.md` **with recipes** — not green.
- [ ] Merge with both platforms green.

---

# `P3-W9` — config / CLI / build / i18n / synthetic

**Branch:** `p3-w9-config-build`. The scrollback-limit config half is **`P3-W3 T4`** (it must land with
its mechanism). Everything else area G owns is here. **Reminder:** `pkg/`, `.github/`, `nix/`,
`flatpak/` and `build.zig.zon*` are **wholly N/A** and `po/` is 99 % N/A; the justifications are
recorded in Task 7 rather than re-derived per file.

## Task 1: The duplicated URL regex — fix both copies and gate the duplication (`9fc2a3085`)

**Rationale.** Upstream deleted the `trailing_spaces_at_eol` fragment from all three branches so trailing
whitespace is no longer part of a matched path. rustty has the regex in **TWO** places, because
`rustty-vt` cannot depend on `rustty-config` (R4): `crates/rustty-config/src/url.rs:65` (spliced at
`:127,134,155`) and `crates/rustty-vt/src/string_map.rs:240` (spliced at `:281,288`). Upstream has
**one** (`git grep -n trailing_spaces_at_eol origin/main -- src` → 0 hits; at the pin → 4, all in
`src/config/url.zig`). **Nothing detects if only one copy is edited** — a standing fidelity hole this
range merely exposes.

- [ ] **Step 1** — delete the fragment and its splices in **both** files; flip the 3 test expectations
in each.
- [ ] **Step 2** — add the gate area G's OQ-4 asks for: a single test asserting the two regex strings are
**byte-identical**. Cheap, and it converts a two-site edit with a silent-divergence hazard into a
one-line-per-site fix with a guarantee.
- [ ] **Step 3: NEGATIVE CONTROL** — edit **only** `rustty-config`'s copy and confirm the new
byte-identity test FAILS. Then fix both and confirm green. Also assert a URL followed by trailing spaces
**excludes** them — the existing table would pass with the fragment still present if no case exercises
EOL whitespace.
- [ ] **Step 4: Commit**

```sh
git commit -m "config/vt: exclude trailing spaces from URL path matches in both regex copies, and gate that they stay identical (oracle 9fc2a3085) (P3-W9 T1)"
```

## Task 2: The two config doc rewrites (`2da02f4d2`, `d71695550`)

- [ ] **Step 1: `2da02f4d2`** — `crates/rustty-config/src/config_struct.rs:1970-1982` still carries the
pin's KDE/KWin prose ("KDE Plasma (Wayland and X11)", "To configure KWin's global blur setting, open
System Settings…"); replace with upstream's `ext-background-effect-v1` + X11/KWin note (net −3 lines).
- [ ] **Step 2: `d71695550`** — `config_struct.rs:2731,2736,2746` still carry the pin's `keybind`
`global:` prose; replace with upstream's, which adds `vicinae-hotkey-v1`, restructures the XDG-portal
list and adds COSMIC. Both describe subsystems whose *behaviour* is `P3-W8`'s, but **the doc text is
portable now and should be**, so the generated man page and webdata track the oracle.
- [ ] **Step 3: expect G3b to get WORSE here** — roughly +15 (background-blur) and +12 (keybind) false
"missing" entries, because the gate reads the oracle tree **at the pin**, where the deleted sentences
still exist. Record the number; `P3-W10 T3` resolves it by construction. **Do not add an exception
list** — that is the shape R5 forbids.
- [ ] **Step 4: NEGATIVE CONTROL** — regenerate the man page and webdata and assert the new strings
appear in **both** outputs. A doc edit that does not reach the generated artifacts is invisible to users,
and `+show-config --docs` is a third consumer.
- [ ] **Step 5: Commit**

```sh
git commit -m "config: refresh the background-blur and keybind global: doc blocks (oracle 2da02f4d2 d71695550) (P3-W9 T2)"
```

## Task 3: `gtk-horizontal-tab-scroll` — and correct the two wrong ledger rows (`5ea45919d`, `b5f5ca078`)

- [ ] **Step 1** — add `gtk_horizontal_tab_scroll: bool = true` + its `"Available since 1.4.0."` line.
**Fold this into `P3-W3 T4`'s commit** so the three `204` assertions and the five goldens are
regenerated once (204 → 206), not three times.
- [ ] **Step 2: correct the ledger.** `.port/phase-2/MAIN_PARITY.md:197` (`5ea45919d`) says
*"n/a-tier BL-8 · gtk tier unported; Config option lands with that tier"* and `:196` (`b5f5ca078`) says
the same for its doc line. **Both are wrong:** the `Config` **field** has no GTK-apprt dependency —
rustty-config already carries sibling GTK options (`config_struct.rs:4664 gtk_wide_tabs`), because
`Config` is a data structure, not a runtime. Corroborating count: `Config.zig` has **63**
`"Available since"` at the pin and **64** at head; rustty has **63**, and the single new one is exactly
this option's. Reclassify both rows as **ABSENT, S**.
- [ ] **Step 3: also record that the 204-field contract's fidelity claim is currently FALSE** (area G
OQ-6). The three assertions guard rustty against **itself**, which is useful, but nothing asserts the set
matches the oracle's. Make the count-assertion comment record the **oracle SHA the number was last
reconciled against**, so the next drift is visible rather than inferred.
- [ ] **Step 4: NEGATIVE CONTROL** — assert the option parses, formats and round-trips, **and** that the
`ALL_KEYS`/`CONFIG_FIELDS`/`CONFIG_FIELD_NAMES` counts are 206 — three assertions that fail
independently if one registration site is missed. The behaviour half's control is `P3-W8 T5 Step 3`.
- [ ] **Step 5: Commit**

```sh
git commit -m "config: add gtk-horizontal-tab-scroll and reconcile the field-count contract against the oracle SHA (oracle 5ea45919d b5f5ca078) (P3-W9 T3)"
```

## Task 4: `src/synthetic` — the Kitty generator pair (`f6f79acce`)

- [ ] **Step 1** — port `src/synthetic/Kitty.zig` (+88) as `crates/rustty-synthetic/src/kitty.rs`:
`\x1b_G` + `a=t,f=100,i=<rand>;` + a length-rounded-to-4 random base64 payload + `\x1b\\`, plus
`base64_alphabet` via `Bytes::generate_alphabet`; 1 test.
- [ ] **Step 2** — port `src/synthetic/cli/Kitty.zig` (+70): the `+kitty` action with
`--data-len` (default 4096) and the `create`/`destroy`/`run` triple; 1 test.
- [ ] **Step 3** — register: `crates/rustty-synthetic/src/cli.rs` `Action::Kitty` + its arm;
`crates/rustty-synthetic/src/lib.rs` `pub mod kitty`.
- [ ] **Step 4: NEGATIVE CONTROL** — port both upstream tests, and assert the emitted payload length is a
**multiple of 4** and that the whole sequence parses as a valid kitty APC through
`rustty_vt::apc` — a generator that emits malformed sequences would still "pass" a length-only test while
silently invalidating every benchmark built on it.
- [ ] **Step 5: Commit**

```sh
git commit -m "synthetic: random Kitty graphics APC generator and its +kitty action (oracle f6f79acce) (P3-W9 T4)"
```

## Task 5: Re-vendor the theme corpus and gate its content (`4c1d69696`, `b513f1b20`, `02504bcce`, `07a56d08b`, `dc8d38503`)

- [ ] **Step 1** — upstream bumped `iterm2_themes`
`ghostty-themes-release-20260608-160426-8c84dd1` → `…-20260720-153658-97e244c` across 5 commits.
rustty **vendors** the corpus: `crates/rustty-extra/themes/` holds 536 entries (534 themes + `README.md`)
and `crates/rustty-extra/themes/README.md` pins provenance to the **old** tarball and hash. Fetch
`ghostty-themes-release-20260720-153658-97e244c.tgz`, re-vendor, and update the provenance README with
the new hash (`N-V-__8AAGxKBAAjYkT1jYUiOZDXQuj0u34ndIjAWxiR1KX9`).
- [ ] **Step 2: add the digest gate — it matters more than the bump.** `xtask/src/res_themes.rs` asserts
"exactly 534", so a **count** change surfaces but **content drift in an existing theme file will not**.
Add a manifest of per-file digests plus a total digest, checked by a gate.
- [ ] **Step 3: NEGATIVE CONTROL** — perturb one byte in one theme file and confirm the digest gate
FAILS while the count assertion still passes. That contrast is the justification for the gate.
- [ ] **Step 4: Commit**

```sh
git commit -m "extra: re-vendor the iterm2 theme corpus at 20260720-153658-97e244c with a per-file digest manifest gate (oracle 4c1d69696 b513f1b20 02504bcce 07a56d08b dc8d38503) (P3-W9 T5)"
```

## Task 6: The dependency bumps (`vaxis`, `zf`, `uucode`, `libxev`) and the CI-toolchain coupling

- [ ] **Step 1: `vaxis` 0.5.1 → 0.6.0** (`crates/rustty-vaxis`; consumers are `rustty-cli`'s
`list-themes` (`crates/rustty-cli/src/list_themes.rs:566-571`), `list-keybinds`, `list-colors`) and
**`zf` 0.10.3 → 0.11.0** (`crates/rustty-zf`; the `list-themes` fuzzy picker). Diff the upstream tarballs
and port the deltas; these are ported Zig libraries, not Cargo dependencies.
- [ ] **Step 2: `uucode` — and the CI pin it drags.** Upstream moved `uucode.x.types` → `uucode.types`
and `uucode.x.grapheme` → `uucode.grapheme` and changed the pinned tarball
(`4749c4e93`: `uucode-0.2.0-ZZjBPqZV…` → head: `uucode-2826a37a…` / hash `ZZjBPlK5…`), **still labelled
0.2.0**, and rewrote `src/build/uucode_config.zig` (+203, the `computeWidth` extension
removed/restructured). `crates/rustty-unicode/src/generated/` holds tables generated against **uucode
0.2.0 at the OLD revision**. If the new revision changed any property data, **rustty's tables are
silently stale and `checksum.rs` will not catch it** — it validates internal consistency, not upstream
data.
  **Ordering hazard:** `.github/workflows/ci.yml:69` installs **Zig 0.15.2** specifically as the G4-uni
  uucode oracle; the new uucode requires **Zig 0.16**, so regenerating forces that pin to move. Decide
  and record: bump the CI Zig pin, or defer the uucode bump with a written reason.
- [ ] **Step 3: `libxev`** — bump-driven; `crates/rustty-xev` is a D6 Zig-port-defer (memory:
`integration-tier-decisions`). Diff the range and record whether anything semantic landed; also re-check
the three xev `QUIRK` sites from `P3-W0 T5` against the new revision.
- [ ] **Step 4: `z2d` 0.10.0 → 0.12.1** is **`P3-W8 T3`** — cross-reference only, do not duplicate.
- [ ] **Step 5: NEGATIVE CONTROL** — for uucode, **regenerate the three `*_data.rs` files against the
new revision and byte-compare** against the committed ones (or diff the two tarballs' data outputs).
"The version label did not change" is **not** evidence: the label stayed `0.2.0` while the revision moved.
For vaxis/zf, the control is the existing `rustty-vaxis`/`rustty-zf` test suites plus a `list-themes`
smoke run — note `crates/rustty-cli/src/list_themes.rs:2614`'s interactive Loop path is
`#[ignore]`-on-device (it needs a real `/dev/tty`), so it is **exercised by no CI test** and the guard
there is compile-time only. Say so rather than implying coverage.
- [ ] **Step 6: Commit**

```sh
git commit -m "deps: bump the ported vaxis, zf, uucode and libxev revisions with a regenerated-table byte comparison (oracle e8525c0fd f2a7652ab dep table) (P3-W9 T6)"
```

## Task 7: The N/A record, the stale citations, and the CLI cosmetics

- [ ] **Step 1: cosmetics** — `crates/rustty-cli/src/edit_config.rs`'s failure message
`Error code={}` → `(E{s})` form (`f2a7652ab`); and fix the two stale oracle citations in
`crates-sys/wuffs-sys/build.rs:14,35`, which name `pkg/wuffs/src/c.zig` — a file upstream **deleted**;
the 10-entry `WUFFS_CONFIG__MODULE*` list itself is **byte-identical** to its new home
(`origin/main:pkg/wuffs/build.zig:5-14`), so only the citation rots. Same defect class as the `.pot`
references: an oracle path that no longer resolves makes fidelity auditing unreliable.
- [ ] **Step 2: write the auditable N/A record** into `.port/phase-3/NON_PORTS.md`, one line of
justification each, **with the proof where a proof was produced**:
  - `f2a7652ab` `Config.omitFramePointer()` — **proven N/A**:
    `rustc +nightly -Z unstable-options --print target-spec-json --target aarch64-apple-darwin` →
    `"frame-pointer": "non-leaf"`; rustc keeps frame pointers on Apple ARM unconditionally.
  - `f2a7652ab` `libsystem_override.sh` + `LibsystemOverrideStep.zig` — **proven N/A**: Zig's compiler-rt
    exports the mem/libm functions as **C** symbols; Rust's exports them only Rust-mangled
    (`nm -g $(rustc --print sysroot)/lib/rustlib/aarch64-apple-darwin/lib/libcompiler_builtins-*.rlib`
    → no `T _memcpy`/`T _memset`, 0 libm hits over 422 `T` symbols), so they cannot shadow libSystem at
    final link. This matters because `crates/rustty-vt-c` and `crates/rustty-apprt-c` both emit
    `staticlib` — rustty **is** in the same archive-consumer position and is still safe.
  - `d65cb5128` native-linker link for lib-vt on Apple + `pkg/apple-sdk/native_link.zig` — Zig's
    self-hosted Mach-O linker omits `LC_ENCRYPTION_INFO_64`; rustc uses `ld64`, which emits it. rustty
    ships **no iOS dylib** (`xtask xcframework` packages a **static** library, `xtask/src/main.rs:395,442`,
    macOS only). **Caveat to carry forward:** if rustty ever ships an iOS framework containing
    `rustty-vt-c`'s `cdylib`, `.github/scripts/check-apple-libghostty-vt.nu`'s four assertions become
    mandatory — and will pass. Record it as a precondition on the (unscheduled) iOS tier.
  - `1c861e3c4` `pkg/apple-sdk/include/math.h` and `dac134d25` libc++ availability — both work around
    Zig's **bundled** Clang resource headers / bundled libc++; rustty compiles C/C++ with Apple `clang`
    against the real SDK.
  - `84254a9d8` no-libcxx MSVC runtime — the Windows half is macOS-first N/A; the "don't drag in a C++
    runtime" half **already exists** (`crates-sys/simdutf-sys/build.rs:50` and
    `crates-sys/highway-sys/build.rs:68` both `cpp_link_stdlib(None)`).
  - `1fe1b2d23` static lib-vt on Windows; `f2a7652ab`'s `patch_rpath` → `patchelf {interp, rpath}`
    (**record the `--set-interpreter` half as a real requirement if a Nix lane is ever stood up**);
    `terminalOptions(.., optimize)`/`slow_runtime_safety` (no analogue — safety is a Cargo profile);
    `10e6ace6b`'s FreeBSD `trimEnd` (**and record D-1: rustty's `gen_locale` derives the locale dir from
    the `po/*.po` basename at `xtask/src/res_misc.rs:375`, which is deliberately MORE robust than the
    oracle's hardcoded list — do not "fix" it back**).
  - `pkg/harfbuzz/c.zig` and `pkg/wuffs/src/c.zig` **deletions**: nothing to follow. rustty never had a
    `c.zig`; `crates-sys/harfbuzz-sys/build.rs:116` runs `bindgen` over `wrapper.h`, which **is** the
    structural equivalent of upstream's new `hb_c` translate-c module — **upstream converged on rustty's
    design.**
  - all 13 `.github/` files, all of `nix/`, `flatpak/`, `build.zig.zon*`.
- [ ] **Step 3: the `po/` verdict** — port **only** `ae76cfbfc`'s single real translation
(`po/zh_CN.po`: `"Close Split"` → `"关闭分屏"`) and the untranslated `"Close Split"` msgid additions
where catalogs lack them (`po/be.po` has only `:293 msgid "Close Split?"`; `po/kk.po:178` has the bare
one — it was the pin commit's subject). **Do NOT port the `#:` reference renumbering** (2,142 lines): it
cites `src/apprt/gtk/**` at *upstream's* line numbers, and rustty's `po/io.airely.rustty.pot` already
carries those oracle paths verbatim for files that exist in **no** rustty file — copying upstream's
renumbering would make stale metadata **differently** stale. gettext ignores `#:` at lookup time, so the
functional impact is zero either way, and **WS13 T4 owns the real fix** (extraction against
`crates/rustty-apprt/`). The 3 vicinae msgids are **`P3-W8 T7 Step 3`**. Confirm the locale set did not
change (`git -C /Users/gael/projects/ghostty diff 4749c4e93..origin/main -- src/os/i18n_locales.zig` →
empty; rustty's 31-entry list at `crates/rustty-os/src/i18n_locales.rs` still matches) and that **R2 is
unaffected** (none of the new msgids contains an identity token).
- [ ] **Step 4: NEGATIVE CONTROL** — `msgfmt --statistics` on `po/zh_CN.po` before and after must show
the translated count move by exactly one. For Step 2, the control is G-P2: every `NON_PORTS.md` row must
carry a criterion set, or the closure gate fails.
- [ ] **Step 5: Commit**

```sh
git commit -m "port: translate zh_CN Close Split, refresh the edit-config error text and the wuffs provenance citation, and record area G's auditable N/A set with proofs (oracle ae76cfbfc f2a7652ab e8525c0fd) (P3-W9 T7)"
```

## `P3-W9` finale

- [ ] `cargo test --workspace --no-fail-fast` green; all 5 config goldens regenerated once.
- [ ] The theme digest gate green; `xtask resources` unchanged (verified: `GhosttyResources.zig`'s
`share/` destination set is byte-for-byte the same, and `mdgen`/`webgen` **content** is untouched, so
WS13's docs half is unaffected by Phase 3).
- [ ] G3b's config row recorded (expected ~39, up from 9); `P3-W10 T3` must bring it to 0.
- [ ] Merge with both platforms green.

---

# `P3-W10` — Finale: advance the pin

**Branch:** `p3-w10-advance-pin`. **This workstream requires that NO OTHER LANE IS ACTIVE.** It is the
one place in the program licensed to `git checkout` in the oracle repo, and the standing prohibition
exists precisely because concurrent lanes depend on that tree staying at `4749c4e93`.

## Task 1: Pre-flight — every workstream merged, every tracker green

- [ ] **Step 1** — confirm `P3-W0`…`P3-W9` are all merged to `main`:

```sh
git log --oneline --merges main | grep -c 'P3-W'
```

- [ ] **Step 2** — confirm the three `P3-W0` RED trackers are now green: A5(d) (2 exports), D10 (4 enum
sites, now HARD), the `rustty-bench` action set (11 actions).
- [ ] **Step 3** — confirm every `.port/phase-3/REACHABILITY.md` LIVE row's reproducer is un-ignored and
passing, and every `SAMPLING_CLOSURE.md` row has a verdict.
- [ ] **Step 4: NEGATIVE CONTROL** — run `python3 .port/tools/gates/check_phase2_closure.py` (WS19's
G-P2). It must be **0**. Phase 3 must not have re-opened Phase 2 debt; a non-zero result here means a
Phase 3 task filed a row into a closed workstream.
- [ ] **Step 5** — announce the lane freeze. Verify no other worktree has uncommitted work:

```sh
git worktree list
for w in $(git worktree list --porcelain | awk '/^worktree /{print $2}'); do echo "== $w"; git -C "$w" status --porcelain | head -5; done
```

## Task 2: Retire the beyond-pin bookkeeping

- [ ] **Step 1** — `.port/phase-2/MAIN_PARITY.md` was triaged at `53bd14fec` (2026-07-10) and scoped to
`src/` + `include/` only, so it **cannot speak** for this range or for `pkg/`, `po/`, `macos/`. It also
carries rows that are wrong in **both** directions. Corrections established by the assessments, to be
applied now if not already applied by their owning task:
  - `:76` `d0a26d146` "missing BL-1" → **PORTED**
    (`crates/rustty-vt/src/pagelist/grow.rs:576,666` + the verbatim test at `pagelist/tests.rs:2365`).
  - `:127` `6d5dda40d` "missing BL-1" → **PORTED**
    (`crates/rustty-inspector/src/widgets/pagelist.rs:62,413,466,2464`).
  - `:134` `0d015b2fc` "missing BL-1" → **PORTED** (9 provenance-tagged sites across
    `search/{sliding_window,active,viewport}.rs`).
  - `:128` `0fb89f4ff` — accurate for `0fb89f4ff` itself, but the 50 MB default is **not** from it
    (`P3-W3` fixed the text).
  - `:196,197` `b5f5ca078`/`5ea45919d` — reclassified by `P3-W9 T3`.
  - The **eleven** commits area D found listed as `missing BL-1` in the search subsystem
    (`73ac36fa5`, `86e1f7b8b`, `5d8eb78b7`, `d239f9805`, `83aada205`, `c243d89bd`, `a89c6133c`,
    `89eca063e`, `c9ad708ef`, `f6d9a582e`, `0d015b2fc`) — **all eleven are in rustty**, landed by WS7 at
    upstream's final shape, with `search/screen.rs` at 22 of 23 upstream tests and `search/pagelist.rs`
    at 9 of 9. Strike them.
- [ ] **Step 2** — rewrite `.port/ORACLE_SYNC.md`: **one** sync point at `6ad1fe7d8`, and **delete** the
"Beyond-pin work ported so far" table (BL-1…BL-7), which exists only because the pin was stale. Any
residue that is genuinely still open becomes a normal ledger row with a named owner, not a
"beyond-pin series".
- [ ] **Step 3: NEGATIVE CONTROL** — for every row struck as PORTED, the strike must cite a
`file:line` **and** a test or gate that would fail if it regressed. The binding rule from
`.port/phase-3/README.md`: *"A verdict of 'already ported' is only acceptable with evidence."* A strike
without a guard is a claim, not a verdict — leave those rows open.
- [ ] **Step 4: Commit**

```sh
git commit -m "port: correct 16 MAIN_PARITY rows in both directions and retire the beyond-pin series table (P3-W10 T2)"
```

## Task 3: Advance the oracle WORKING TREE — the only irreversible step

**This is what "advancing the pin" means.** Editing `.port/ORACLE_SYNC.md` alone changes nothing,
because `.port/tools/gates/check_comment_coverage.py` (and `check_comments_fidelity.py`) read a
**directory**, not a SHA. Advancing the tree removes the reference side of every beyond-pin G3b artifact
**by construction** — no exception list, no gate change, no R5 exposure.

**Rejected alternative, recorded so it is not re-proposed:** a second worktree
(`git -C ../ghostty worktree add ../ghostty-head origin/main`) pointed at by `GHOSTTY_SRC`. A
redirectable oracle path is precisely the spoof hole `.port/tools/ci.sh:114-129` documents killing for
G1b — the probe passes against a fake directory while the script counts zero reference sentences
everywhere. `P3-W0 T2 Step 6` **removed** the `GHOSTTY_SRC` env override for exactly this reason.

- [ ] **Step 1: record the before state**

```sh
git -C /Users/gael/projects/ghostty status --porcelain          # MUST be empty
git -C /Users/gael/projects/ghostty rev-parse HEAD              # must be 4749c4e93…
python3 .port/tools/gates/check_comment_coverage.py 2>&1 | tee /tmp/g3b_before.txt
python3 .port/tools/gates/check_test_counts.py; echo "rc=$?"
```

Expected before: `G3b [config]: missing=9`, and G1b green.

- [ ] **Step 2: advance the tree**

```sh
git -C /Users/gael/projects/ghostty fetch origin main
git -C /Users/gael/projects/ghostty checkout 6ad1fe7d8cbda36c77b337a96c9bea8a77883699
git -C /Users/gael/projects/ghostty rev-parse HEAD
```

- [ ] **Step 3: update `.port/ORACLE_SYNC.md`** — the pinned baseline becomes
`6ad1fe7d8cbda36c77b337a96c9bea8a77883699`, `2026-07-29`,
*"url: exclude trailing spaces from path matches (#13505)"*. Update the `.port/PORT_RULES.md` (R12) and
`.port/cycle-analysis.md` references to the SHA.

- [ ] **Step 4: re-run and triage**

```sh
python3 .port/tools/gates/check_comment_coverage.py 2>&1 | tee /tmp/g3b_after.txt
diff /tmp/g3b_before.txt /tmp/g3b_after.txt
```

**The config row must reach `missing=0`.** All 9 pre-existing entries were sentences the oracle itself
deleted — verbatim, they are the four `scrollback-limit` doc-rewrite lines (`65c48213b`) and the five
`Color.fromHex` / `Palette.parseCLI` lines (`2970e9a2a`) — and `P3-W9 T2`'s ~27 additions are the same
class. **If it does not reach 0, the residue is a REAL drop and must be triaged individually.** That is
the entire value of doing this as a distinct, verifiable step rather than folding it into a doc edit.

- [ ] **Step 5: NEGATIVE CONTROL** — the diff between `/tmp/g3b_before.txt` and `/tmp/g3b_after.txt`
**is** the control: it must show the reference-side sentences disappearing, not the gate going quiet. If
`distinctive ref comments` drops to 0 or the file count drops, the gate is reading the wrong tree —
investigate rather than accepting the green. (This is the `_vt_header_baseline.txt` failure mode: an
artifact that reports nothing reads as success.)

- [ ] **Step 6: Commit**

```sh
git commit -m "port: advance the oracle sync point to 6ad1fe7d8 (working tree moved; G3b config missing 9 -> 0) (P3-W10 T3)"
```

## Task 4: Absorb the gate flips the advance causes, then merge

- [ ] **Step 1: G1b goes RED for `rustty_vt_c` — expected, and a real signal.**
`check_test_counts.py` greps the oracle working tree, so it now compares against **head**: `src/terminal/c`
goes from **307** zig `test{}` at the pin to **363** at head, and `crates/rustty-vt-c` has **354**
`#[test]`. Close the gap; the missing tests are enumerated per-commit by area C §2 — the DECRQSS
`write_pty` test (`P3-W4 T6`), the two begin/end update tests (`P3-W5 T7`), `vt_processing_error`
(`P3-W4 T5`), the resize pair (`P3-W4 T2/T3`), the cursor-defaults RIS case (`P3-W4 T4`), the
constructor test updates (`P3-W3 T5`) and the grapheme table (`P3-W6 T1`). If those workstreams landed
their tests, most of the gap is already closed — measure, then fill the remainder.

```sh
python3 .port/tools/gates/check_test_counts.py 2>&1 | grep -i 'vt_c\|rustty_vt'
```

- [ ] **Step 2: re-run every gate against the advanced tree.** G1, G2, G3, G3b, G8, G9, D1–D9, A3, A4,
A5, G-LZ4, D10. Any that were computed against the pin now have a different reference; **a gate that
changes verdict here is reporting a real difference, not a bug.** Triage each; **do not weaken any gate
to absorb the change** (R5).

```sh
bash .port/tools/ci.sh 2>&1 | tail -80
```

- [ ] **Step 3: regenerate the range artifacts as a closing consistency check**

```sh
cd /Users/gael/projects/ghostty && git diff 6ad1fe7d8..origin/main --stat
```

Expected: **empty**. A non-empty diff means upstream moved during Phase 3; record the new gap as the
seed of the next resync rather than chasing it now.

- [ ] **Step 4: Both platforms, full sweep**

```sh
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo build --workspace
cargo test --workspace --no-fail-fast 2>&1 | tail -40
xcodebuild test -project macos/Rustty.xcodeproj -scheme Rustty
bash .port/tools/ci.sh 2>&1 | tail -80
ssh rvm 'cd ~/projects/rustty && git fetch && git checkout p3-w10-advance-pin && \
  cargo build --workspace && cargo test --workspace --no-fail-fast 2>&1 | tail -40'
ssh rvm 'cd ~/projects/rustty && cargo build --features gtk,wayland,x11 -p rustty && bash .port/tools/ci.sh 2>&1 | tail -80'
```

**Do NOT run `no_homebrew_gettext_gate.sh`.**

- [ ] **Step 5: NEGATIVE CONTROL for the whole phase** — re-run `P3-W0 T1`'s `HASH_LOG` 12→11 mutation
and confirm **G-LZ4 now fails** where at the start of Phase 3 the entire suite stayed green. That single
before/after is the cleanest demonstration that Phase 3 improved the instruments and not merely the code.

- [ ] **Step 6: Merge**

```sh
git checkout main && git merge --no-ff p3-w10-advance-pin \
  -m "merge: Phase 3 upstream resync — oracle sync point advanced 4749c4e93 -> 6ad1fe7d8 (365 commits; ~95 absent + ~19 partial semantic changes ported; 4 gates added, 4 strengthened)"
```

---

## Acceptance criteria

- [ ] `.port/ORACLE_SYNC.md` states **one** sync point, `6ad1fe7d8`, and the "beyond-pin work ported so
far" table is gone.
- [ ] `git -C /Users/gael/projects/ghostty rev-parse HEAD` is `6ad1fe7d8…` and
`git -C /Users/gael/projects/ghostty status --porcelain` is empty.
- [ ] `python3 .port/tools/gates/check_comment_coverage.py` reports `G3b [config]: missing=0`, with the
before/after diff recorded showing reference-side sentences disappearing (not the gate going quiet).
- [ ] `python3 .port/tools/gates/check_test_counts.py` exits 0 against the **advanced** tree, including
`rustty_vt_c` ≥ 363.
- [ ] Four **new** gates exist, each with a recorded reachable FAIL arm: **G-LZ4** (self-test reproduces
1,929 diverging rows), **D10** (was RED on 4 known exports, now HARD and green), the **theme digest**
gate (byte-perturbation control), and the **`setenv`-in-tests** grep gate.
- [ ] Four **strengthened** gates: A5 gains (d) oracle-export parity both directions, (b′) a
vacuous-shrink floor, (c′) a header-driven layout arm, and an enum-discriminant arm — each with a
recorded FAIL-arm demonstration.
- [ ] `_vt_header_baseline.txt` and `_vt_mismatch_report.txt` are deleted; no gate accepts a
`GHOSTTY_SRC` env override.
- [ ] `lz4_differential_exhaustive` no longer reports PASSED without running; `rustty-bench` is in a
gate and its action set matches upstream's 11.
- [ ] `.port/phase-3/{SAMPLING_CLOSURE,REACHABILITY,REVIEW_CLASSES,NON_PORTS,HASHMAP_CALLER_AUDIT}.md`
all exist, are referenced from `.port/phase-3/README.md`, and have zero unresolved rows.
- [ ] Every `FAITHFULNESS QUIRK` site is either fixed with its comment **deleted** (`metal/buffer.rs`) or
re-affirmed with a note recording the SHA range it was checked against.
- [ ] Every Miri-hazard task (`P3-W5 T3` `cursor_scroll_region_up`, `P3-W5 T4` `RowBuilder`,
`P3-W7 T7` the pipeline ring, `P3-W2 T3` backward-shift deletion) has a recorded
"reintroduce the `&mut` → Miri errors" negative control. Hand-reasoned Stacked Borrows is not accepted.
- [ ] `python3 .port/tools/gates/check_phase2_closure.py` (WS19 G-P2) still exits 0 — Phase 3 re-opened
no Phase 2 debt.
- [ ] Unverifiable items (macOS 27 AppKit workarounds, the `compute-size` monitor clamp, the
vicinae-hotkey positive path) are recorded in `VERIFICATION_DEBT.md` **with runnable recipes** and are
**not** marked green.
- [ ] The `perf-baseline-rust-vs-zig` record carries the `quirks_memset` note **before** any
re-benchmark against `origin/main`.
- [ ] Both platforms: `cargo build --workspace` + `cargo test --workspace --no-fail-fast` green;
`.port/tools/ci.sh` green on both; `xcodebuild test` green.







---

# `P3-W11` — Phase 3 completion sweep (the remainder, owned rather than parked)

**Branch:** `p3-w11-sweep`. **Created 2026-07-31**, after W0–W4 and W6–W9 merged.

This workstream exists because several lanes finished their *workstream* without
finishing every *task*, and unfinished work with no live owner is exactly how the
WS19 audit found 21 orphaned items. Rather than leave those tasks described only
in merge messages, they are collected here with an owner the `G-P2` gate can see.

**Nothing here is new scope.** Every item is a task the original plan already
specified, or debt a lane filed with a runnable recipe.

## Task 1: `P3-W6 T4` — Kitty temp-file → directory validation

Security-relevant: removes the process-`TMPDIR` dependency from kitty image path
validation. Its blocker is gone — `P3-W7` (which owned `global::tmp_dir_path`) is
merged, so `Limits.temporary_directory` is reachable now.

## Task 2: `P3-W6 T5` — Kitty transient usage / eviction

## Task 3: `P3-W6 T6` — Viewport fingerprint reuse

The largest remaining item: a single-pass rewrite carrying a **documented SB/UB
hazard**. Treat it the way `P3-W2 T1` treated the hash-map pointer audit — prove
the aliasing story before writing the fast path, not after.

## Task 4: `P3-W8 T6`/`T7` — the Wayland hotkeys series (F-W1 … F-W4)

**Not started at all** in the W8 lane; nothing of F-W1..F-W4 exists. The
assessment's insertion points and the plan's `RefCell` `winproto()`/`winproto_mut()`
hazard still stand as written in the `P3-W8` section above.

## Task 5: `P3-W8 T5 step 3` — now unblocked

It was blocked on `gtk-horizontal-tab-scroll`, which `P3-W9 T3` landed.

## Task 6: **V21** — the Surface-level visibility KAT

`.port/phase-2/VERIFICATION_DEBT.md` **V21**: `6c8c07981`'s Surface-level dedup
and the end-to-end "report reaches the pty" assertion. `rustty-core` has no
`Surface` fixture; the row carries a runnable recipe (extend `exec.rs`'s
`TestTermio` into a `TestSurface`) and names the exact mutation that must fail.

## `P3-W11` status, 2026-07-31 (end of the first lane pass)

| task | status |
|---|---|
| 1 — `P3-W6 T4` kitty temp-file -> directory | **LANDED** (`4d690b4`) |
| 2 — `P3-W6 T5` kitty transient usage / eviction | **LANDED** (`e67ff42`) |
| 3 — `P3-W6 T6` viewport fingerprint reuse | **LANDED** (`1a139c8`), Miri harness at `.port/phase-3/mirihar-fingerprint-update` |
| 4 — `P3-W8 T6`/`T7` Wayland hotkeys (F-W1..F-W4) | **LANDED 2026-07-31** on `p3-wayland-hotkeys`; F-W1..F-W4 all ported, i18n msgids landed with the code, portal gate extended and green on live mutter. Wire-path (compositor-side) verification is debt `V22` with a runnable fake-compositor recipe and an `#[ignore]`d marker test — the ONLY thing deferred; see below |
| 5 — `P3-W8 T5` step 3 `gtk-horizontal-tab-scroll` | **LANDED** (`9dff03b`), verified on the Linux VM |
| 6 — **V21** Surface visibility KAT | **(a) DISCHARGED** (`21cfb81`); **(b) PARTIAL**, still owned here |

### Task 4 — LANDED 2026-07-31 (branch `p3-wayland-hotkeys`)

The description below is the ORIGINAL scoping, kept for provenance. What actually
landed, whole, in one branch:

* **F-W1** `key::keysym_from_trigger` + the oracle's own test verbatim;
  `winproto::App::{bind,clear}_global_shortcuts` dispatch with `noop`/`x11`
  returning `false`; `GlobalShortcuts::BindFailed` boxed type + the `bind-failed`
  signal + public `emit_trigger`/`emit_bind_failed`;
  `Application::global_shortcut_bind_failed` → `desktop_notification`. The
  `RefCell` hazard is handled by taking `winproto_mut()` as a per-statement
  temporary in BOTH `close()` and `refresh()`; **the hazard was proven real** — a
  deliberately held `winproto()` `Ref` across `close()`'s `RefMut` aborts the live
  app with `RefCell already borrowed`.
* **F-W2** `refresh()` tries winproto first and logs
  `global shortcuts bound via winproto`. rustty's "headless refresh still builds
  the map" divergence is KEPT and recorded in `MAIN_PARITY.md`.
* **F-W3** `crates/rustty-apprt/protocols/vicinae-hotkey-v1.xml` vendored verbatim
  (305 lines, copyright header intact); `wayland-scanner` proc-macro codegen in
  `src/gtk/winproto/wayland/vicinae.rs` (wayland-rs 0.31 scans in the macro, not a
  build script; `build.rs` supplies the `rerun-if-changed` edge, plus a `bitflags`
  dependency the generated bitfield needs); `Tag::VicinaeHotkeyManager` + registry
  bind.
* **F-W4** `winproto/wayland/hotkeys.rs` — entry selection, `manager.bind`, the
  listener state machine, `Entry::fail` → `emit_bind_failed`. Two FORCED
  divergences, documented in that file's module docs and in `MAIN_PARITY.md`:
  the entry table lives in `Globals` with a `HotkeyId` userdata token (wayland-rs
  dispatches through one queue-state type with `Send + Sync` userdata), and signal
  emissions are deferred out of the queue-dispatch `RefCell` borrow (a `trigger`
  runs an arbitrary keybind action, and `toggle_quick_terminal` re-enters that
  same `RefCell`).
* **The 3 i18n msgids landed WITH the code**, via `xtask update-translations`;
  `code_msgids_all_wrapped_and_in_pot`'s pinned count went 24 → 27.
* **Verification:** unit + headed KATs on the Linux VM under
  `--features gtk,wayland,x11`; live-GNOME graceful-absence KAT; and
  `ws15_globalshortcuts_portal_gate.sh` extended with the "the winproto arm must
  DECLINE on mutter" assertion, which was itself proven non-vacuous.
* **Deferred, exactly as `P3-W8 T7 Step 4` licenses:** the fake-compositor harness
  for the wire path — `VERIFICATION_DEBT.md` **V22**, with a runnable recipe and
  an `#[ignore = "needs fake vicinae compositor"]` marker test.

### Original scoping (historical)

Nothing of F-W1..F-W4 exists. It was not started because it is not a sweep-sized
item and starting it half-way would have produced exactly the silent-no-op
keybind failure mode the `P3-W8 T6` preamble warns about. Its real size, from
the `P3-W8` text itself:

* **F-W1/F-W2** — `key::keysym_from_trigger`, `winproto::App::{bind,clear}_global_shortcuts`,
  `GlobalShortcuts::BindFailed` + the `bind-failed` signal, and
  `Application::global_shortcut_bind_failed` raising a **desktop notification**.
  Carries the `RefCell` hazard: `winproto()` hands out a `Ref<'_>` while
  `clear_global_shortcuts` needs `&mut`, so `close()` must route through
  `winproto_mut()` or an upstream crash-fix becomes a double-borrow panic.
* **F-W3** — vendoring `vicinae-hotkey-v1.xml` (305 lines, MIT, Aurelien
  Brabant) and adding `wayland-scanner` `generate_client_code!` machinery to
  `crates/rustty-apprt/build.rs`. **New machinery for that crate.**
* **F-W4** — `Hotkeys` itself (164 lines) plus 3 i18n msgids that must land
  *with* it, not before (WS13 T1's `code_msgids_all_wrapped_and_in_pot` gate is
  written to fail on msgids reachable from nothing).
* Verification: the entry-selection logic is pure and unit-testable, but the
  wire half needs a fake `wayland-server` compositor advertising
  `vicinae_hotkey_manager_v1` — ~a day of scaffolding, and `P3-W8 T7` Step 4
  already says to **schedule it as its own task rather than pretend F-W4 is
  verified**.

**Do not fold it into another sweep.** It wants its own workstream with the
fake-compositor harness scheduled alongside it.

### Task 6 residue

V21 part (b)'s last hop (`Thread::drainMailbox` -> `visibilityReport` ->
`queueWrite` -> the pty write queue) needs a pty-owning `ThreadData` fixture.
The remaining recipe is written into the V21 row.

## `P3-W11` finale

- [x] Every task above either landed or re-filed with an owner in an OPEN workstream.
- [x] `G-P2` green; `check_phase2_closure.py --self-test` green.
- [x] Both platforms green (macOS: 4373 tests, 0 failed, G-REL / wasm / fmt /
      clippy / acyclic / vt-headers all green. Linux VM: 0 `test result: FAILED`
      across the workspace; the `gtk,wayland,x11` predicate KAT green with its
      negative control observed RED).
