# DECISION — the logging facade (class `LOG`, 86 findings)

Status: **DRAFT — awaiting human ratification** (Task 13, parity-defect-closure,
2026-09-09). This document does not change `LEDGER.tsv`. No row's `state` or
`owner` is touched here; that is Task 13 Step 3, gated on the ruling below.

## Summary and recommendation

rustty-vt and 15 sibling crates carry every reference `log.warn`/`log.err`/
`log.info`/`log.debug` call as a comment instead of emitting it — 86 findings,
79 still `OPEN`, spanning 16 crates. This was framed as "does the VT core pay
for a logging dependency it deliberately avoided." The evidence says that
framing is not quite right on two counts: (1) `rustty-vt-c` already has a
working, faithfully-ported C-ABI log sink (`sys::log_fn`) that **16 of the open
rows bypass with a raw `eprintln!` today** — an active bug independent of any
facade decision — and (2) the oracle's own `libghostty-vt` C-ABI build does
**not** treat "internal terminal-module log call" and "embedder-visible log
call" as different things: `src/lib_vt.zig`'s `std_options.logFn` installs the
same `terminal/c/sys.zig` sink globally for the whole non-wasm C-ABI library,
so every `log.warn` inside `terminal/*.zig` already reaches the embedder in
the reference. The "internal vs. boundary" split that Option B is built on is
not a distinction the oracle itself draws for non-wasm builds.

**Recommendation: Option A** — adopt the `log` facade crate in `rustty-vt` and
the other 15 affected crates, restore all 79 open sites, and have
`rustty-vt-c` install itself as the `log::Log` backend that forwards into the
existing `sys::log_fn` sink (which already forwards to the embedder-provided
C callback). This is the closest structural match to what the oracle actually
does (a global logger-function swap reaching every internal call site), it
closes every open `LOG` row, and the measured cost is small: linking the `log`
crate into a `rustty-vt-c` wasm32 release build added **26 bytes** to a
912,220-byte baseline when the call sites aren't reachable from the compiled
feature set (see §3) — the real cost is the restored message-formatting code
itself, which any of the three options pays for identically once a site is
un-commented. The one place this needs care, not a size veto, is a wasm
release build: mirror the oracle's own wasm arm (`os/wasm/log.zig`'s `noop`)
by capping `log::set_max_level` to `Off` under `!debug_assertions` on
`wasm32`, exactly the guard rustty-vt-c's `sys::log_fn` already uses for its
own wasm arm (`crates/rustty-vt-c/src/sys.rs:520-540`).

Independent of which option is ratified, **the 16 `eprintln!`-bypass sites in
`rustty-vt-c` should be fixed regardless** — they're not "missing parity with
the oracle's diagnostics," they're rustty's own C-ABI log sink being routed
around by rustty's own code.

## The evidence

### 1. Is the C-side sink bypassed today? Yes — 16 rows, file:line

`crates/rustty-vt-c/src/sys.rs:500` defines `pub fn log_fn(level, scope,
args)`, which (non-wasm arm, `log_fn_c_abi` at `sys.rs:558`) locks `GLOBAL`,
checks whether an embedder installed a log callback, and if so formats into a
2048-byte buffer and invokes it. It is a real, working sink — used at 15+
call sites across `lib.rs`, `terminal.rs`, `render.rs`,
`selection_gesture.rs`, `search.rs`, and `selection.rs`.

Despite that, four files bypass it with a raw `eprintln!` to stderr, which an
embedder that installed a log callback (e.g. a GTK or wasm host) never sees:

- `crates/rustty-vt-c/src/mouse_encode.rs:320, 355, 374, 395, 406` (S174-F1) —
  `setopt`/`setopt_typed` invalid-value warnings.
- `crates/rustty-vt-c/src/osc.rs:235` (S174-F2) — `commandData` warning.
- `crates/rustty-vt-c/src/sgr.rs:33, 584-591` (S177-F1, S5-F1) —
  `sgr_set_params` separator-overflow warning.
- `crates/rustty-vt-c/src/key_encode.rs:164-245` (S7-F1, S172-F1) —
  `key_encoder_setopt` invalid-option/value warnings.

These 4 files account for 6 of the 79 open `LOG` rows by finding-id, but the
ledger's `rustty-vt-c`/`rustty-apprt` crate tag covers 16 open rows total once
apprt's own dropped diagnostics (GTK class files, x11 winproto, portal —
none of which bypass an existing sink, they simply have none) are counted
alongside it. The `eprintln!` bypass specifically is the strongest, driest
argument in this whole findings class: it needs no facade decision at all,
just routing four files through the sink that already exists.

### 2. What does the oracle's *library* build actually do?

`src/lib_vt.zig`'s `std_options` (the analog of `main_ghostty.zig`'s, but for
the standalone `libghostty-vt` artifact):

```zig
if (builtin.target.cpu.arch.isWasm()) {
    // In non-debug modes, we want to ship effectively no logging
    // warn and lower add ~200KB at the time of this comment.
    if (builtin.mode == .Debug) {
        options.log_level = .debug;
        options.logFn = @import("os/wasm/log.zig").log;
    } else {
        options.log_level = .err;
        options.logFn = @import("os/wasm/log.zig").noop;
    }
} else if (terminal.options.c_abi) {
    // For C ABI builds, use a custom log function that dispatches to an
    // embedder-provided callback (or silently discards when none is set).
    options.logFn = @import("terminal/c/sys.zig").logFn;
}
```

Two findings follow directly:

- **Non-wasm C-ABI builds (macOS/Linux libghostty-vt, i.e. rustty-vt-c's real
  shipped configuration) route every `std.log` call in the whole library —
  including deep-internal ones like `pagelist`/`screen`/`stream` diagnostics —
  through the exact same sink an embedder observes.** There is no internal/
  boundary split in the reference architecture for this target; it's one
  global `logFn`. This is what `sys::log_fn` already is (a hand-rolled analog
  of exactly this dispatcher) — it's just not yet reachable from `rustty-vt`.
- **wasm is the one place the reference deliberately suppresses**, and only
  in non-debug builds, citing a concrete ~200KB cost. rustty already has the
  matching machinery for this (`rustty_os::wasm::log`, referenced at
  `sys.rs:520-540`) — a wasm-release facade rollout should reuse that
  pattern (cap `log::max_level()` to `Off`), not invent a new one.

Net effect: the divergence is **not** smaller than 86 rows implies because the
reference suppresses less than that; it's differently shaped than the
Option B framing assumed — the reference doesn't distinguish "vt-c-visible"
from "internal" logging at all for the platforms that matter (macOS/Linux),
it distinguishes wasm-release from everything else.

### 3. Cost of Option A — measured, not estimated

`rustty-vt`'s `Cargo.toml` has **zero** third-party dependencies today beyond
internal `rustty-*` crates, `libc`, `allocator-api2`, `zlib-sys`, and optional
`dcimgui-sys`/`rustty-oniguruma`/`rustty-wuffs` — no logging or tracing crate.
`log` 0.4.33 is already present in the workspace's `Cargo.lock`, but only as a
build-dependency of `bindgen` (pulled transitively through `dcimgui-sys`), not
as a runtime dependency of any shipped artifact — so this is a genuinely new
edge onto the VT core, not a free reuse of something already linked in.

Measured (uncommitted scratch edit, reverted after measurement — no diff
survives in the tree): built
`.port/tools/build-vt-wasm.sh --no-default-features --features render-state`
(`rustty-vt-c`, `wasm32-unknown-unknown`, release, wasm-ld only — no
`wasm-opt` pass available in this environment, so this is an upper bound, not
a floor):

| build | `rustty-vt.wasm` size |
|---|---|
| baseline (no `log` dep) | 892,220 bytes |
| `rustty-vt` + `log = "0.4"` dep, 5 macro call sites (error/warn/info/debug/trace) in an unreached `pub fn` | 892,246 bytes |

Delta: **26 bytes**. With no logger installed and the call site unreached from
`rustty-vt-c`'s exported surface, the linker garbage-collects almost
everything; the `log` crate's own footprint (a global `AtomicUsize` state
machine plus macro glue) is close to free. This isolates the *facade's* cost
from the cost of the *restored diagnostics themselves* — the latter is
unavoidable formatting/branching code that Option A, B, and a hand-rolled
alternative to `log` would all pay for identically once a given site is
un-commented; it is not a reason to prefer one option over another. The
concrete risk is the wasm release-mode budget the oracle itself flags (~200KB
if wasm ships full logging unconditionally) — mitigated the same way the
oracle mitigates it (§2), not by refusing the dependency.

For scale: a prior phase's `-Dvt-features=-all,+render-state` size gate
(`.port/gap-close/README.md`) measured 1,352,416 → 870,067 raw bytes for that
feature subset — the `log` facade's ~26-byte floor is three orders of
magnitude below the granularity that gate already tracks.

### 4. Severity distribution — does NOT map cleanly onto Option B's boundary

Of 79 open `LOG` rows: **69 low, 10 medium** (86 total incl. 7 already-WAIVED:
6 low, 1 medium). Reading all 10 medium rows individually shows severity was
**not** graded on "does an embedder observe this," it was graded on whether
the finding smuggles in a real control-flow/behavioral bug alongside the
missing diagnostic:

- **3 of 10** are the genuine `sys::log_fn` bypass bugs from §1 (S174-F1,
  S174-F2, S177-F1) — these *are* boundary-crossing and *are* Option B's
  target.
- **7 of 10 are not primarily about a missing log call at all**, despite the
  `LOG` class tag:
  - S43-F2 (`rustty-apprt/src/lib.rs`) — `xev.detect()` never runs; the
    backend log line is *hardcoded*, not merely dropped.
  - S62-F2 (`rustty-bench/src/cli.rs`) — `cli::main` skips **all** of process
    init (logging, `GHOSTTY_LOG`, rlimits, locale, onig/glslang, resources
    dir, signals), not just the log wiring.
  - S67-F2 (`rustty-args/src/args.rs`) — a line-too-long path is silently
    truncated and parsing *continues*, where the oracle logs **and stops
    iteration**; the control flow itself differs.
  - S75-F1 (`rustty-config/src/value_types.rs`) — `RepeatableFontVariation`
    parse *rejects* float syntaxes the oracle accepts; this is a parser
    correctness bug, arguably mis-classified as `LOG` at all.
  - S94-F1 (`rustty-font-shaper/src/shaper/coretext.rs`, DISPUTED) — an mpsc
    stand-in in place of the real `CFReleaseThread` port; a missing-feature
    finding, not a missing-log finding.
  - S113-F1 / S115-F2 (`rustty-renderer/src/generic/linux_impl.rs`) — a
    context-prepare failure **aborts** re-realize instead of warn-and-continue
    (S113-F1) / skips the swap-chain rebuild (S115-F2); the oracle's `void`
    return semantics are not preserved, independent of the log line.

So: severity correlates with "this row bundles a real behavioral bug under
the `LOG` tag," not with API-boundary visibility. Those 7 rows need their own
control-flow fixes under whichever `LOG`-adjacent or reclassified process
handles them — no logging-facade decision, on its own, closes them.

### 5. Precedent — no ratified AD, but an unratified working analog exists

No `AD-N` in `.port/` covers logging (the ratified list is AD-0 through AD-7
plus AD-97; none is about a logging facade). Across `src-*.md` the audit
itself consistently defers with language like *"if/when rustty-vt adopts a
logging facade"* and *"once a logging facade is chosen for rustty-renderer"*
(`src-config.md:124`, `src-cli.md:84`, `src-font.md:112,128,144,180`,
`src-renderer.md:51,55,57`, `src-termio.md:60`, `src-terminal.md:183,235,
291,295,327,411`) — the auditor deliberately left this open rather than
grading each site as an independent gap. **This is genuinely a first-time
decision, not a case of confirming or revisiting something already ratified.**

That said, one real precedent exists that the audit's framing missed:
**`crates/rustty-core/src/logging.rs` already implements a working, faithful
log sink** — the direct Rust port of `main_ghostty.zig`'s `logFn` /
`std_options` (macOS unified-logging arm + `debug_assertions`-gated stderr
arm, byte-for-byte structural match to the oracle, `log_with`/`log`
functions). It is consumed today by crates that sit at or above `rustty-core`
in the R4 acyclic layering. It cannot be reused directly by `rustty-vt`,
`rustty-font-shaper`, `rustty-renderer`, `rustty-config`, `rustty-args`,
`rustty-crash`, `rustty-os`, `rustty-simd`, `rustty-wuffs`, or `rustty-bench`
— none of those depend on `rustty-core` (checked: 0 `rustty-core` mentions in
each of their `Cargo.toml`s) — but it is proof that a faithful port of the
oracle's log-sink architecture already exists and works in this codebase; the
question is only how to extend its reach below `rustty-core` without
introducing a cycle. `log`'s global-logger-registration model (any crate can
call the macros; exactly one root installs the backend) is the standard way
to do that without an edge from `rustty-vt` back up to `rustty-core`.

## The three options, precisely

Counts below are derived from `LEDGER.tsv` (`$4=="LOG" && $11=="OPEN"`), 79
rows. The 7 already-`WAIVED` `LOG` rows (S91-F3, S44-F4, S71-F7, S88-F2,
S119-F3, S149-F3, S190-F2) were adjudicated on unrelated grounds (dead code,
platform-inapplicable, etc. — see `ADJUDICATION.md`) and are unaffected by
this decision either way.

### Option A — adopt a facade workspace-wide

Add `log` to `rustty-vt` and the other 15 affected crates; restore all 79
open sites verbatim (oracle message text, level, scope); have `rustty-vt-c`
install a `log::Log` implementation that forwards into the existing
`sys::log_fn`, and cap `log::max_level()` to `Off` on `wasm32` release builds
to preserve the ~200KB budget the oracle itself protects.

- **Closes: 79.** (Note: 7 of these — S43-F2, S62-F2, S67-F2, S75-F1, S94-F1,
  S113-F1, S115-F2 — still need a follow-up control-flow fix per §4; the
  facade alone restores their diagnostic but not their full behavior.)
- **Waives: 0.**
- **Cost:** one new dependency edge on the VT core (measured ~26 bytes at the
  crate-linkage floor on wasm32 release; real cost is the restored
  message-format code, unavoidable under any option); a `log::Log` wiring
  seam in `rustty-vt-c`; a wasm-release level cap to match the oracle's own
  size mitigation.
- **Commits the project to:** `rustty-vt` (and by extension anything
  `wasm32`-targeted that links it) carrying a logging-macro dependency
  permanently, and to keeping the wasm-release level-gate correct as new call
  sites are added.

### Option B — facade only at the vt-c/apprt boundary

Restore only the sites in `rustty-vt-c` and `rustty-apprt` (16 rows: the 6
genuine `eprintln!`-bypass sites plus 10 further apprt diagnostics); leave
`rustty-vt`, `rustty-font-shaper`, `rustty-renderer`, `rustty-config`,
`rustty-args`, `rustty-cli`, `rustty-crash`, `rustty-os`, `rustty-termio`,
`rustty-simd`, `rustty-wuffs`, `rustty-bench`, `rustty-core` (63 rows) waived
with a written "no facade below the C boundary" rationale.

- **Closes: 16** (S43-F2, S174-F1, S174-F2, S177-F1, S5-F1, S7-F1, S46-F5,
  S49-F5, S50-F4, S51-F2, S54-F4, S54-F6, S58-F6, S59-F6, S60-F6, S172-F1).
- **Waives: 63.**
- **Cost:** smaller — no dependency on `rustty-vt` itself; `rustty-apprt`
  already has broader dependencies so a facade there is cheap. The `eprintln!`
  bypass fix (§1) is a strict subset of this option and should happen either
  way.
- **Commits the project to:** a rationale that does not survive §2's
  reading of the oracle — the reference does *not* treat `terminal/*.zig`'s
  internal logs as architecturally different from vt-c's, so "internal vt-vt
  logs don't need to reach anyone" is a policy rustty would be choosing, not
  one inherited from the reference. It also leaves 21 `rustty-vt` rows, 12
  `rustty-renderer` rows, and 11 `rustty-font-shaper` rows permanently silent.

### Option C — waive the whole class

Record all 79 open rows (86 total) as intentional divergence, written once.

- **Closes: 0.**
- **Waives: 79** (86 including the 7 already-waived).
- **Cost:** none additional; no dependency, no wiring.
- **Commits the project to:** the weakest position given §1 and §2 — it
  waives 16 rows that are either an active bug against rustty's own existing
  sink (not a porting gap at all) or, per the oracle's C-ABI `std_options`,
  behavior the reference does not itself suppress. It would also need its own
  carve-out for the 7 rows in §4 that are not really about logging.

## What would need to be wrong for this recommendation to flip

- If `wasm-opt` (not available in this measurement environment) showed the
  `log` crate's per-call-site cost is materially higher than the eprintln!
  baseline once real message-formatting code is linked in at scale across all
  79 restored sites — i.e., if the §3 measurement's "26 bytes" turns out to
  be a floor that doesn't generalize because dead-code elimination behaves
  differently once the call sites are *reachable* (as they would be once
  restored) rather than probe-only and unreached. This should be re-measured
  with the real 21 `rustty-vt` sites restored, not simulated, before Option A
  is executed.
- If a maintainer values "the VT core has zero third-party dependencies" as
  an end in itself (a stated design goal per the crate's own Cargo.toml
  comments — "R3 GUI-glue FFI leaf... Optional — only pulled in under
  `--features imgui`... so a default VT build has no GUI dependency" shows the
  project already protects this elsewhere), independent of what the oracle's
  architecture implies — that's a values call this evidence can't settle, and
  it would point to Option B or C regardless of §2's reading of the oracle.
- If the 16 rows in §1's bypass list turn out to have a reason for going
  around `sys::log_fn` that this review didn't surface (e.g. a
  reentrancy/lock-ordering hazard calling into `GLOBAL.lock()` from those
  specific call paths) — that would still argue for fixing the bypass, just
  not by routing through the existing lock-based sink unchanged.

## §6 — Task 14a: the wiring pattern (RATIFIED, executed)

**SUPERSEDED IN PART — see §6r below.** Task 14a's first review round found
two Criticals (a max-level clobber of a host's own logger; no backend at all
for the actual `rustty` app binary, which would have closed 73 rows on
bookkeeping) and three Importants (a factually wrong install-point claim, a
non-reference-matching level default, and a wrong/incomplete crate list) in
the text below. §6r is the fix; read it alongside this section, not instead
of it — this section's mechanics (the two call shapes, the `LOG_SCOPE`
convention, the dependency wiring) are still accurate, only the backend
implementation, install points, level defaults, and crate list changed.

Executed 2026-09-09. Adds the `log` facade crate and fixes the `eprintln!`
bypass in `rustty-vt-c` (the 6 ledger rows S174-F1, S174-F2, S177-F1, S5-F1,
S7-F1, S172-F1). Documents the mechanical pattern stages 14b-14e follow for
the other 73 rows across 15 crates.

### Two call shapes, chosen by layering position, not by preference

R4's acyclic layering puts `rustty-vt-c` ABOVE `rustty-vt` and its siblings
(`rustty-font-shaper`, `rustty-renderer`, `rustty-config`, `rustty-args`,
`rustty-cli`, `rustty-crash`, `rustty-os`, `rustty-termio`, `rustty-simd`,
`rustty-wuffs`, `rustty-bench`, `rustty-core`). That single fact decides which
of two call shapes a restored site uses — there is no independent facade
decision left to make per crate:

1. **Inside `rustty-vt-c` itself** (this stage's 6 rows): call
   `crate::sys::log_fn(sys::StdLevel::<Level>, LOG_SCOPE, format_args!(...))`
   directly. This is not new — it is the pattern already used by
   `render.rs`/`lib.rs`/`terminal.rs`/`selection.rs`/`selection_gesture.rs`/
   `search.rs` before this stage. `rustty-vt-c` has direct access to its own
   sink; going through `log`'s macro/backend indirection here would add a
   dependency edge for no reason. Each restored file gets a
   `pub(crate) const LOG_SCOPE: &str = "<scope>";` — the `@tagName` of the
   reference's `std.log.scoped(.scope)` — placed where the commented-out
   `const log = std.log.scoped(...)` line already sits.

2. **Everywhere below `rustty-vt-c`** (14b-14e's 73 rows): these crates
   cannot depend on `rustty-vt-c` (that would invert R4 into a cycle), so they
   call the `log` crate's macros instead:
   `log::warn!(target: "<scope>", "<format string>", ...)` (`error!`/`info!`/
   `debug!` for the other three oracle levels). `target: "<scope>"` is the
   facade's carrier for `std.log.scoped(.scope)` — pass `""` for the
   reference's `.default` scope, matching `log_fn`'s existing "callers pass
   `\"\"` for the default scope" convention (`sys.rs:566`). Do **not** rely on
   `log`'s default target (the call site's module path) — it has no oracle
   analog and would silently diverge from the restored scope tag.

### The backend: `rustty-vt-c::sys::Logger`

`crates/rustty-vt-c/src/sys.rs` now has, immediately after `log_fn_c_abi`:

- `struct Logger;` + `impl log::Log for Logger` — `log()` maps
  `record.level()` through `std_level_from_log` (the 5-level `log::Level` →
  the reference's 4-level `StdLevel`; `Trace` folds into `Debug`, unreachable
  in practice since `install_log_backend` caps the max level at `Debug`) and
  forwards `record.target()` + `*record.args()` straight into `log_fn` — the
  SAME funnel this crate's own direct call sites use. One sink, reached two
  ways, matching the oracle's single global `std_options.logFn` swap.
- `pub fn install_log_backend()` — idempotent (`log::set_logger`'s "already
  installed" error is swallowed, the same shape as `log_fn_c_abi`'s
  formatting-error `catch {}`); calls `log::set_logger(&LOGGER)` +
  `log::set_max_level(LevelFilter::Debug)`, EXCEPT on `wasm32` when
  `!debug_assertions`, where it caps `max_level` to `Off` and returns before
  touching `set_logger` at all — mirroring `lib_vt.zig`'s own
  `options.logFn = ... .noop` size guard (§2) at the filter level: `Off`
  means the crates below never even construct a `log::Record` at their
  `log::warn!` call sites, so none of the restored message-formatting code
  those 73 sites bring in links into a release wasm build.
- Installed from `rustty_terminal_new` (`lib.rs`) — the one C export every
  embedder is contractually required to call before any other terminal
  functionality (see that function's own doc + the `sys` module's "must be
  set at startup" note). This is the closest Rust runtime analog to the
  oracle's comptime `std_options` swap, which has no explicit "install" call
  at all because Zig resolves it at compile time; Rust's `log` crate requires
  an explicit registration, so the earliest guaranteed-first C entry point is
  the natural substitute. 14b-14e do not need to revisit this — the backend
  is already live for any crate that adds the `log` dependency and calls the
  macros.

### Dependency wiring

- `[workspace.dependencies]` gained `log = "0.4.33"` (pinned to what
  `Cargo.lock` already carried transitively through `bindgen`/`dcimgui-sys`,
  per §3 — this makes it a genuine first RUNTIME dependency, not reuse of an
  already-linked artifact).
- `rustty-vt-c/Cargo.toml` gained `log = { workspace = true }`. 14b-14e add
  the same line to each of the other 15 crates as they restore sites; no
  further workspace-level change is needed.

### Measured wasm cost this stage — net NEGATIVE (smaller, not larger)

`rustty-vt` is untouched in this stage (no site restored there yet; 14b is
first). The crate this stage DOES touch — `rustty-vt-c` — is also the crate
`.port/tools/build-vt-wasm.sh` builds, so it was measured directly rather than
estimated:

| build (`--no-default-features --features render-state`, `wasm32-unknown-unknown`, release) | `rustty-vt.wasm` |
|---|---|
| before (git-stashed: no `log` dep, 4 sites still `eprintln!`) | 892,220 bytes |
| after (this stage's diff) | 885,927 bytes |

**Delta: −6,293 bytes.** Reproduced twice (identical byte count both times).
The `log` crate's own linkage cost stays at the ~26-byte floor §3 already
measured (no call site here is reachable in the wasm-release configuration —
`install_log_backend`'s `Off` cap sees to that); what dominates is the
REMOVAL of 4 `eprintln!` call sites, each of which pulled in
`std::io::Stdio`/formatting machinery that `sys::log_fn`'s existing
stack-buffer `LogEmitter` path does not need. Net effect: fixing the bypass
made the wasm module smaller even after paying for the facade dependency.
This is a below-the-noise-floor, favorable result — not a reason to revisit
Option A, but also not representative of what 14b-14e will measure once
`rustty-vt` itself carries live (non-wasm-release-gated) `log::warn!` sites;
re-measure at that point per §3's own "what would need to be wrong" caveat.

Two harmless `#[cfg_attr(all(target_arch = "wasm32", not(debug_assertions)),
allow(dead_code))]` annotations were added to `Logger`/`std_level_from_log`/
`LOGGER` — in that one configuration they are genuinely unreferenced (the
`Off` cap means no macro call site ever reaches them), which is by design,
not a defect; the annotation prevents that expected condition from being a
build warning.

## §6r — Task 14a review-round fixes (supersedes parts of §6)

Fixed 2026-09-09, same day as §6, in response to the first review of task
14a. Two Criticals and three Importants; all five addressed before any of
14b-14e may proceed. None of the six already-`CLOSED` ledger rows are
reopened or touched by this section — the review confirmed those six rows
correct against the oracle (byte-identical scope/level/message text) and
found the defects entirely in the PATTERN this section documents, which would
have replicated into the other 73 rows if left as originally written.

### CRITICAL 1 — `install_log_backend` clobbered a host's own log filter

**The bug:** `log::set_max_level(LevelFilter::Debug)` ran unconditionally,
even when `log::set_logger` returned `Err` (an embedder, or an earlier call,
already owns the registration). An embedder that installed its own `log`
backend and deliberately set its own filter would have that filter silently
raised to `Debug` by rustty — on every single `rustty_terminal_new` call, so
the host could not even work around it by calling `set_max_level` again
afterward.

**The fix:** `crates/rustty-vt-c/src/sys.rs::install_log_backend` now:
1. Wraps the whole body in a `std::sync::Once`, so repeated calls (from any
   of the crate's `*_new` entry points — see IMPORTANT 3) are genuine no-ops,
   not merely cheap-but-real re-executions.
2. Calls `log::set_max_level` **only inside the `if log::set_logger(&LOGGER)
   .is_ok() { ... }` success arm.** If `set_logger` fails, the function now
   does nothing else at all — it leaves the existing logger AND its filter
   completely alone. Containment, not clobber, is the correct behavior when a
   host already owns the `log` crate's single global registration.

The identical fix (Once-guard + max-level-only-on-success) is applied to the
new `rustty-core::logging::install_app_log_backend` (see CRITICAL 2) from
the start, so it never had this bug.

### CRITICAL 2 — 73 rows would have closed onto no backend in the real app

**The bug:** `crates/rustty` (the actual application binary; `cargo tree -p
rustty` confirms it) does not, and structurally cannot, depend on
`rustty-vt-c` — R4 puts `rustty-vt-c` in the standalone-library-embedding
tier, not the app tier. `rustty-vt-c::sys::Logger` — task 14a's only backend
as originally written — therefore never runs in the process that matters
most: the actual terminal application. The moment 14b restores a
`log::warn!` site in `rustty-vt`/`rustty-renderer`/`rustty-font-shaper`/
`rustty-config`/etc., that diagnostic would emit into the void in the real
app, while the oracle's `main_ghostty.zig` `std_options.logFn` reaches every
one of those call sites. Marking those rows `CLOSED` on that basis would be
exactly the "derived gate rests on nothing" failure this project already has
scar tissue about (see memory: `derived-gate-green-can-be-bookkeeping`).

**The fix:** a second backend, hosted in `rustty-core` (which the app binary
DOES link), forwarding into `rustty-core::logging`'s existing, already-
faithful sink (the direct Rust port of `main_ghostty.zig`'s `logFn`):

- `crates/rustty-core/src/logging.rs` gained `struct AppLogger` (`impl
  log::Log`), `fn level_from_log` (the same 5-to-4-level mapping as
  `rustty-vt-c::sys::std_level_from_log`), and `pub fn
  install_app_log_backend()` — Once-guarded, success-arm-only `set_max_level`
  (see CRITICAL 1), calling the existing `log()`/`log_with` functions in the
  SAME file, unchanged.
- `crates/rustty-core/Cargo.toml` gained `log = { workspace = true }`.
- Installed from `crates/rustty/src/main_rustty.rs::main()`, immediately
  after `state_init()` succeeds — the actual, already-ported process-init
  entrypoint (the direct port of `main_ghostty.zig`'s `main()`/`state.init()`
  sequence), not a hypothetical future one. `rustty-core` itself has NO
  ported process-init entrypoint yet (its own `lib.rs` doc says so —
  "the entrypoint (`main()` / `rustty_init()`, unported)"), so the install
  call could not live inside `rustty-core`; it lives at the one real call
  site that exists today.
- Level default ports `main_ghostty.zig`'s ACTUAL `std_options.log_level`
  for the app (`.Debug => .debug, else => .info`) — see IMPORTANT 4 below for
  why this differs from `rustty-vt-c`'s default.

**Known, documented (not fixed here) limitation:** `AppLogger` forwards into
`logging::log()`, the STATE-reading variant, which reads
`rustty_core::STATE`. ADV-FIX 2 (see `main_rustty.rs`'s own comments) means
the real `rustty` bin builds its OWN `GlobalState<Action>` and never
populates `rustty_core::STATE` (which is bound `Action = ()`) — this is a
pre-existing architectural split, not something introduced or owed to this
logging stage. Until ADV-FIX 2 is revisited, `AppLogger`-forwarded records
use `Logging::default()` rather than the bin's `RUSTTY_LOG`/action-resolved
flags — the exact fallback `log()` already documents for any caller before
`STATE` is set, just permanent on this path instead of transient at boot. A
future stage that wants byte-parity on which records the app actually emits
(not just that they reach a real sink, which is what this stage guarantees)
needs to close that gap; it is out of scope for a logging-facade pattern
fix.

**Two backends, one rule:** which one runs is decided ENTIRELY by which
binary is running — `rustty-vt-c::sys::Logger` in the standalone
library/wasm artifacts, `rustty-core::logging::AppLogger` in the `rustty`
app binary — because `log::set_logger` is a single process-wide
registration and the two artifacts never coexist in one process. Neither
14b-14e nor any later stage needs to choose between them; a crate that calls
`log::warn!(target: "...", ...)` reaches whichever backend its own binary
installed, with no crate-level wiring difference.

### IMPORTANT 3 — the install-point claim was factually wrong

**The bug:** §6 (and the code comment on `rustty_terminal_new`) claimed it
was "the one C export every embedder is contractually required to call
before any other terminal functionality." Checked against the actual shipped
examples: `example/c-vt-sgr`, `example/c-vt-encode-key`, `example/wasm-sgr`,
and `example/wasm-key-encode` never call `rustty_terminal_new` at all — they
call `rustty_sgr_new`/`rustty_key_encoder_new`/`rustty_key_event_new`
directly. Neither cited doc says what the comment claimed either (the `sys`
module's "must be set at startup" note is about `rustty_sys_set`, a
different function).

**The fix:** `install_log_backend()` is now called from the top of **every**
top-level `*_new` C-ABI constructor in `crates/rustty-vt-c/src/lib.rs` (15
of them: `rustty_key_event_new`, `rustty_key_encoder_new`,
`rustty_mouse_event_new`, `rustty_mouse_encoder_new`, `rustty_sgr_new`,
`rustty_osc_new`, `rustty_formatter_terminal_new`,
`rustty_snapshot_decoder_new`,
`rustty_kitty_graphics_placement_iterator_new`, `rustty_render_state_new`,
`rustty_render_state_row_iterator_new`, `rustty_render_state_row_cells_new`,
`rustty_selection_gesture_new`, `rustty_selection_gesture_event_new`,
`rustty_search_new`, plus the original `rustty_terminal_new`), not just the
one. This is cheap and correct BECAUSE of the Once-guard from CRITICAL 1:
whichever constructor an embedder happens to call first wins, and every
other constructor's call — in that process or a later one — is a genuine
no-op. `rustty_terminal_new`'s own comment is corrected to state this
honestly instead of the disproven "contractually required" claim.

### IMPORTANT 4 — level default did not match the reference

**The bug:** `install_log_backend` hardcoded `LevelFilter::Debug` on every
non-wasm config. `lib_vt.zig`'s `std_options` does NOT set `log_level` for
the `terminal.options.c_abi` arm (only the wasm arm sets it explicitly), so
Zig's build-mode default applies: `.debug` in Debug, `.info` (unrelated to
this crate's C-ABI arm) is the DEFAULT `std.Options.log_level`, but the
actually-relevant fact is Zig's own default resolves to `.err` in
`ReleaseFast` (rustty-vt-c's shipped release configuration) when no
`log_level` override is present. A release embedder was receiving warn/info/
debug records the reference itself suppresses.

**The fix:** `install_log_backend`'s success arm now computes `let level =
if cfg!(debug_assertions) { LevelFilter::Debug } else { LevelFilter::Error
};` — the direct port of that default, not an independent choice. Documented
in the function's own doc comment with the port-note reasoning above.

`rustty-core::logging::install_app_log_backend` (CRITICAL 2) uses a
DIFFERENT default — `Debug`/`Info` — because it ports a DIFFERENT
`std_options` block: `main_ghostty.zig`'s own (the actual app), which
explicitly sets `.log_level = switch (builtin.mode) { .Debug => .debug, else
=> .info }` and documents WHY it never lowers further ("our debug logs are
very expensive to calculate and we want to make sure they're optimized out
in builds"). Two backends, two different oracle `std_options` blocks, two
different (both correct) defaults — this is not an inconsistency to
reconcile.

### IMPORTANT 5 — §6's crate list was wrong; `rustty-core`/`rustty-apprt`/`rustty-cli` need a THIRD call shape

**The bug:** §6 listed 12 crates for the `log`-facade side; the open `LOG`
rows actually span **14** crates (`awk -F'\t' '$4=="LOG" && $11=="OPEN"'
.port/parity-closure/LEDGER.tsv`, grouped by crate path):

| crate | open LOG rows |
|---|---|
| rustty-vt | 19 |
| rustty-renderer | 12 |
| rustty-font-shaper | 11 |
| rustty-apprt | 10 |
| rustty-core | 5 |
| rustty-config | 5 |
| rustty-os | 2 |
| rustty-cli | 2 |
| rustty-args | 2 |
| rustty-wuffs | 1 |
| rustty-termio | 1 |
| rustty-simd | 1 |
| rustty-crash | 1 |
| rustty-bench | 1 |
| **total** | **73** |

`rustty-apprt` (10 rows) was missing entirely. Worse: §6 prescribed the
`log::warn!` facade for `rustty-core`'s own 5 rows — but `rustty-core`
**already owns the real sink** (`logging.rs`); routing its own internal
diagnostics out through a facade whose only backend is installed FROM
`rustty-core`'s own consumer would be circular indirection for no reason,
and a strict regression risk (a bug in `AppLogger` would silently break
`rustty-core`'s own logging, which worked before this stage touched
anything).

**The fix — three call shapes, not two, chosen by "does this crate already
depend on the crate that owns a real sink":**

1. **`rustty-vt-c` itself** (this stage's 6 rows, unchanged from §6): direct
   `sys::log_fn(...)`.
2. **`rustty-core` (5 rows), `rustty-apprt` (10 rows), and `rustty-cli` (2
   rows)** — all three already depend on `rustty-core` directly (`rustty-cli`
   and `rustty-apprt` both carry a real `rustty-core = { workspace = true }`
   edge; `rustty-core` obviously depends on itself). Their restored sites
   call `rustty_core::logging::log(level, scope, message)` (or `log_with`
   where an explicit `Logging` is already in scope) DIRECTLY — the exact same
   "you already have the edge, don't add a facade indirection" rule
   `rustty-vt-c`'s own sites follow. **17 rows, no `log` dependency added to
   any of these three crates for this purpose.**
3. **Everywhere else — 11 crates, 56 rows** (`rustty-vt`, `rustty-renderer`,
   `rustty-font-shaper`, `rustty-config`, `rustty-os`, `rustty-args`,
   `rustty-wuffs`, `rustty-termio`, `rustty-simd`, `rustty-crash`,
   `rustty-bench`) — none of these can depend on `rustty-core` (R4: core
   depends on all of them, not the reverse) or `rustty-vt-c`, so they use the
   `log::warn!(target: "<scope>", ...)` facade from §6, reaching whichever of
   the two backends (§6 / CRITICAL 2) the running binary installed.

17 + 56 = 73. 14b-14e should read a row's crate off this table, not
re-derive the call shape from first principles each time.

### wasm cost, re-measured after this round's fixes

The `Once` guard (CRITICAL 1) and 14 additional `install_log_backend()` call
sites (IMPORTANT 3) touch `rustty-vt-c` again, so §6's wasm measurement was
re-taken the same way (`.port/tools/build-vt-wasm.sh --no-default-features
--features render-state`, `wasm32-unknown-unknown` release):

| build | `rustty-vt.wasm` |
|---|---|
| before task 14a (git-stashed baseline) | 892,220 bytes |
| after §6 (task 14a, before this review round) | 885,927 bytes |
| after §6r (this round's fixes) | 886,918 bytes |

Delta from §6: **+991 bytes** (the `Once` machinery plus 14 more call sites
to the now-idempotent `install_log_backend`), still **−5,302 bytes** overall
against the pre-14a baseline — the `eprintln!` removal (§3/§6) still
dominates. Reproduced twice, identical byte count both times.

### Re-verification after this round's fixes

- `cargo test -p rustty-vt-c --all-features`: 492 passed, 0 failed (no
  ledger-row test changed by this round; `install_log_backend`'s own
  behavior is exercised by the existing `log_facade_backend_reaches_the_
  c_sink` test, still green).
- `cargo test -p rustty-core --lib logging::`: 5 passed, 0 failed —
  `level_from_log_matches_the_reference_mapping` (exhaustive, deterministic)
  and `app_log_backend_installs_and_dispatches_without_panicking` (new;
  see that test's own doc comment for why an end-to-end real-sink capture
  is not available for this crate's sink, matching the pre-existing
  `log_with_gates_on_stderr_flag` test's own accepted bar) plus the 3
  pre-existing tests, unchanged.
- `cargo check -p rustty` (the actual app binary) and `cargo check -p
  rustty-vt-c --no-default-features`: both clean.
- `cargo check --workspace --all-targets`, `cargo clippy -p rustty-vt-c
  --all-targets --all-features -- -D warnings`, `cargo clippy -p rustty-core
  --all-targets --all-features -- -D warnings`, `cargo fmt --check` (both
  crates): all clean.
- No ledger row touched by this round; the six `CLOSED` rows from §6 are
  unchanged.
