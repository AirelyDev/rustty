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
  `!debug_assertions`, where it caps `max_level` to `Off` — mirroring
  `lib_vt.zig`'s own `options.logFn = ... .noop` size guard (§2) at the
  filter level.
  **CORRECTED by task-14wasm (2026-09-09) — see §8: the claim that
  originally followed here ("`Off` means ... none of the restored
  message-formatting code those 73 sites bring in links into a release wasm
  build") is FALSE and must not be relied on.** `log::set_max_level` sets an
  `AtomicUsize` (`log`'s `MAX_LOG_LEVEL_FILTER`) that every `log::warn!`
  expansion loads and compares AT RUNTIME (`log::max_level()`); the format
  string, the `Debug`/`Display` impls the arguments need, and the
  `log::Record` construction machinery are all still compiled into the
  binary regardless of what that runtime value is — only the *call* is
  skipped when the check fails. This is a *runtime* gate, not a *comptime*
  one, and it is not the oracle's `.log_level = .err` analog: that field is
  read by Zig's `comptime` `std.log` machinery and erases the guarded
  `std.log.warn`/etc. call sites from the binary entirely. §8 records the
  actual comptime-equivalent mechanism (the `log` crate's own
  `release_max_level_*` Cargo features, wasm32-target-gated) and measures
  its effect.
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

> **THE TABLE IS AUDIT-DERIVED AND THEREFORE UNDER-INCLUSIVE (noted 2026-09-11).** It was
> built from OPEN audit rows, so a crate with real reference `log.*` sites but no audit row
> does not appear in it — and one such crate had to have the `log` dependency added by a
> later stage. **`G-LOGFILE` (`.port/tools/gates/check_log_reconcile.py`), not this table, is
> authoritative on which crates need the facade**: it enumerates the reference tree rather
> than the audit, and every log-bearing reference file is declared or excluded in
> `LOG-PORTMAP.tsv`. Use the table to read a row's crate; use the gate to ask whether a crate
> is in scope at all. (Same root cause as N15, one level up.)
>
> **CALL SHAPE 2 IS SUPERSEDED — see §6r4 (2026-09-10).** The crate TABLE below is
> correct and still binding: read a row's crate off it. The three-shape RULE below is
> not — call shape 2 (`rustty_core::logging::log(...)` direct from `rustty-core` /
> `rustty-apprt` / `rustty-cli`) has **zero** call sites in the workspace and cannot be
> measured by `G-LOGFILE`. §6r4 records what those crates actually do and why.

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
| rustty-bench | 1 — **see §6r2/§6r3 below: NOT closable as a bare `log::warn!` restoration; read that note first** |
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

## §6r2 — Task 14a review round 2 fixes

Fixed 2026-09-09, same day as §6/§6r, in response to the second review of
task 14a. CRITICAL 1, IMPORTANT 4, and IMPORTANT 5 were confirmed addressed
by the reviewer and needed no further change. CRITICAL 2 and IMPORTANT 3
were confirmed still open in specific, named places; fixed below. No ledger
row touched.

### CRITICAL 2 — closed for real this time, in three parts

**1. No install on the libghostty/macOS path.** `rustty_init`
(`crates/rustty-apprt-c/src/lib.rs`) — the port of `main_c.zig`'s
`ghostty_init`, the process init for the LIBRARY artifact — never called
`install_app_log_backend()`. Since `rustty_build_config::apprt::app_runtime`
is `None` on everything but Linux/FreeBSD, **the shipped macOS application is
libghostty (this crate), not the `rustty` exe** — so §6r's fix, which only
touched `main_rustty.rs`, left the primary-platform app with no backend at
all. Fixed: `rustty_init` now calls `install_app_log_backend()` as its first
action, before anything else (including its own `log()` calls for the
Windows-unsupported and detect-action error paths).

**2. The install was too late in `main()`.** Both `main_rustty.rs::main()`
and (now) `rustty_init` install BEFORE calling `state_init()` /
`rustty_apprt::init_with_action` — not after. `init_with_action` runs the
bootstrap sequence (locale, crash-init, glslang, oniguruma, resources dir,
i18n) that several `LOG` rows restore diagnostics INSIDE (e.g. S111-F5 in
`rustty-os/src/locale.rs`), and only calls `rustty_core::STATE.set` at the
very end of that sequence. Installing after `state_init()` returned meant
every bootstrap-window site executed before any logger existed, reaching
nothing — while the oracle's `std_options.logFn` is live from process entry.
Now: bootstrap-window sites read `Logging::default()` via `logging::log`'s
own documented pre-`STATE` fallback (oracle-faithful — the reference's own
zero-initialized `state` gives the identical "default logging config"
behavior before its own `state.init()` resolves the real flags); every site
after `state_init()`/`init_with_action` succeeds reads the fully resolved
flags.

**3. `rustty-bench` — the verdict stands, but the ORIGINAL rationale was
wrong; corrected in round 3 (see §6r3).** The claim that `rustty-bench` "is
not linked from the app either" is **false for libghostty**: `cargo tree -p
rustty-apprt-c -i rustty-bench` shows a real edge, forced at
`crates/rustty-apprt-c/src/lib.rs:160` (`extern crate rustty_bench as _;`,
retaining `rustty_benchmark_cli` in the C-ABI staticlib). The `rustty-bench`
LIBRARY is very much part of the shipped macOS artifact, and once
`rustty_init` installs `install_app_log_backend()` (item 1 above), any
`log::warn!` a restored `rustty-bench` site emits from CODE REACHED THROUGH
`rustty_benchmark_cli` in that artifact DOES reach a real backend. What
remains genuinely unreachable is narrower than originally stated: S62-F2's
OWN site (`rustty-bench/src/cli.rs:201-208`) is reached SOLELY from the
standalone `[[bin]]` entry point (`main_bench.rs`'s `fn main()`, `cli::main`
directly) — that binary has no `rustty-core` dependency and is not launched
by anything that installs either backend. So: the crate-wide "not
closable" framing was too broad; **only `rustty-bench` rows whose restored
site is reached solely from the standalone bin (S62-F2 confirmed; check
each other `rustty-bench` row individually against this before restoring
it) are unclosable without adding a bin-side install** — the vast bulk of
`rustty-bench`'s code, reached through the libghostty artifact, already has
a real backend once item 1 lands.

### The `STATE` claim in §6r was wrong; corrected

§6r asserted `AppLogger`-forwarded records "fall back to `Logging::default()`
... permanent for this path rather than transient at boot." This is false:
`rustty_core::STATE` **is** populated on the real path
(`main_rustty.rs::main()` -> `state_init()` -> `rustty_apprt::
init_with_action` -> `rustty_core::STATE.set(state)`, with `state.logging =
compute_logging(action_detected, RUSTTY_LOG)` — the fully resolved flags).
Since the install now runs BEFORE `state_init()` (item 2 above), the
`Logging::default()` fallback is exactly what it should be: real, but
strictly bounded to the bootstrap window before `STATE` is set — not a
permanent divergence. `crates/rustty-core/src/logging.rs`'s doc comment on
`install_app_log_backend` is corrected in place (marked as a correction, not
silently rewritten) to say this instead.

### IMPORTANT 3 — the last uncovered constructor, plus the stateless entries

`rustty_snapshot_decoder_new_buf` (`crates/rustty-vt-c/src/lib.rs`) was a
genuine gap in the "every top-level `*_new`" sweep — `example/c-vt-snapshot`
calls it BEFORE `rustty_snapshot_decoder_new`. Fixed: it now installs too
(17 constructors covered, not 16).

Beyond `*_new`: `example/c-vt-size-report`, `example/c-vt-build-info`,
`example/c-vt-color-scheme`, and `example/c-vt-encode-focus` call no
constructor at all — only a single stateless encode/info function
(`rustty_size_report_encode`, `rustty_build_info`,
`rustty_color_scheme_report_encode`, `rustty_focus_encode`). The review
offered a choice (cover them, or state the limitation); this round covers
them — each now also calls `install_log_backend()` as its first action, since
`rustty_build_info` in particular has its own `log.warn` guard in the
reference and is exactly the kind of site this stage exists to fix. The
"whichever constructor an embedder calls first wins" framing in §6r no
longer needs a qualifier: every `#[no_mangle]` entry point reachable as a
shipped example's FIRST call now installs.

### Same class as CRITICAL 1 — the wasm-release arm was an inverted clobber

`install_log_backend`'s wasm-release arm called `log::set_max_level(Off)`
UNCONDITIONALLY, with no `set_logger` success-arm guard — silencing a wasm
host's own already-installed filter instead of raising it (CRITICAL 1's bug,
inverted). Fixed by removing the special-cased early return entirely and
unifying the function: `log::set_logger(&LOGGER)` is now attempted on EVERY
target, and `log::set_max_level` — `Off` on non-debug wasm32, `Debug`/`Error`
elsewhere per IMPORTANT 4 — runs only inside that one shared success arm.
`Logger`/`std_level_from_log`/`LOGGER` are consequently always referenced now
(the three `#[cfg_attr(..., allow(dead_code))]` annotations from §6 no
longer apply and were removed).

### Two forward-looking notes recorded, not fixed

- **`rustty-config` already has a strictly-more-reachable path.**
  `config_struct.rs:5396-5415`'s `format_log`/`log_warn` read
  `rustty_global::logging()`, a leaf-crate process-global every crate below
  `rustty-core` can name — no install call needed, works in tests/benches/
  exe/lib alike. Routing `rustty-config`'s 5 `LOG` rows through the
  `log::warn!` facade (as §6r's table currently has them) would give that one
  crate TWO divergent logging paths, with the new one strictly *less*
  reachable than the one it already has. 14b-14e should consider moving
  `rustty-config`'s 5 rows into the direct-call group (parallel to
  `rustty-core`/`rustty-apprt`/`rustty-cli`) rather than the facade group —
  not decided or executed here, just flagged before those rows are restored.
- **Latent recursion once `rustty-os` restores its own `log::warn!` sites.**
  `AppLogger::log` -> `logging::log` -> `log_with` -> (macOS)
  `rustty_os::macos::Log`. `rustty-os` is one of the 11 facade crates (2
  `LOG` rows). If any restored site on the `rustty_os::macos::Log` code path
  itself (or anything it calls) ever logs via `log::warn!`, that call
  recurses back into `AppLogger::log` while already inside it. Nothing in the
  current `rustty-os` codebase does this (checked: `macos::Log`'s own source
  has no `log::` macro call), so it is not live today — but 14b-14e must
  check this specifically when restoring `rustty-os`'s 2 rows, before, not
  after, landing them.

### wasm cost, re-measured again

| build | `rustty-vt.wasm` |
|---|---|
| before task 14a | 892,220 bytes |
| after §6 | 885,927 bytes |
| after §6r (round 1 fixes) | 886,918 bytes |
| after §6r2 (round 2 fixes, this section) | 888,020 bytes |

Delta from §6r: **+1,102 bytes** (5 more `install_log_backend()` call sites
plus `log::set_logger` now attempted unconditionally on wasm-release instead
of short-circuited). Still **−4,200 bytes** overall against the pre-14a
baseline. Reproduced twice, identical.

### Re-verification after this round's fixes

- `cargo test -p rustty-vt-c --all-features`: 492 passed, 0 failed.
- `cargo test -p rustty-core --lib logging::`: 5 passed, 0 failed (doc-only
  change to `install_app_log_backend`'s comment; no test behavior changed).
- `cargo check -p rustty`, `cargo check -p rustty-apprt-c`: both clean.
- `cargo check --workspace --all-targets`: clean.
- `cargo clippy -p rustty-vt-c --all-targets --all-features -- -D warnings`,
  `cargo clippy -p rustty-core --all-targets --all-features -- -D warnings`,
  `cargo clippy -p rustty-apprt-c --all-targets -- -D warnings`, `cargo
  clippy -p rustty --all-targets -- -D warnings`: all clean.
- `cargo fmt --check` (all touched crates): clean.
- No ledger row touched; the six `CLOSED` rows are unchanged.

## §6r3 — Task 14a review round 3 fixes

Fixed 2026-09-09, same day as §6/§6r/§6r2, in response to the third review
of task 14a. CRITICAL 2(a)/(b)/(c), the `STATE` correction, and the
wasm-release containment were all confirmed addressed by the reviewer and
needed no further change. Three things fixed below; no ledger row touched.
After this round the reachability position is: **72 of 73 rows closable**
(all but S62-F2), with the two bootstrap-window rows correct once finding 2
below is read alongside them, and the 40 wasm-linked rows correct once
finding 3's backend lands.

### 1. IMPORTANT 3 residual — the fifth stateless entry point

`example/c-vt-modes` calls `rustty_mode_report_encode` and nothing else
exported — `rustty_mode_new`/`_value`/`_ansi` are `static inline` in
`include/rustty/vt/modes.h`, invisible to a `*_new`/exported-symbol sweep,
which is why this one was missed alongside the four stateless entry points
round 2 already covered. Fixed: `rustty_mode_report_encode`
(`crates/rustty-vt-c/src/lib.rs`) now installs too. All entry points a
shipped example reaches first now install; the "every entry point reachable
as a shipped example's first call now installs" claim in §6r2 is true as
written.

### 2. Bootstrap records now use resolved flags, not defaults

Round 2 moved the install call before `state_init()`/`rustty_apprt::
init_with_action` so bootstrap-window sites get a real (if default) sink
instead of nothing. The review found a real, NEW gap this introduced: the
oracle resolves `self.logging` EARLY inside `global.init` (`global.zig:59-82`
zero-init, then :139/:147-155 resolve it) — before signals, the version log,
`crash.init`, locale, glslang, oniguruma, resourcesDir, i18n — so every one
of the reference's own log calls in that window already reads the RESOLVED
value, never a default. The port computed the same value at
`rustty-apprt/src/lib.rs`'s `compute_logging` call but only published it via
`rustty_global::set_logging` at the very END of `init_with_action`
(previously right before `STATE.set`) — so between "computed" and
"published," `init_with_action`'s OWN direct `log_with(..., &logging)` call
sites (passing the value explicitly) were correct, while a restored FACADE
`log::warn!` in a sub-init called from the SAME function (e.g. S111-F5 in
`rustty-os/src/locale.rs`) would read `rustty_global::logging()`'s stale
default — a real, user-visible divergence (`rustty +list-fonts` on Linux:
oracle suppresses the locale warning under an action, port would print it
into the action's output).

**Fix:** moved `rustty_global::set_logging(logging)` from the end of
`init_with_action` to immediately after `logging` is computed (`logging` is
`Copy` and never reassigned after that point, so this is safe — verified by
reading every use of the binding between the two points). Left the original
call site in place too — now a harmless, idempotent no-op (`OnceLock::set`
on an already-set cell) — so the "one computation site, mirrored into the
leaf global, right where `state.logging`/`STATE.set` are finalized" shape
stays visible at both points. Changed `rustty-core::logging::log`'s
pre-`STATE` fallback from `Logging::default()` to `rustty_global::logging()`
— which is what `main_ghostty.zig:131`'s `logFn` actually reads
(`global.logging()`), and which itself defaults sensibly
(`rustty-global/src/lib.rs`'s `LOGGING.get().copied().unwrap_or_default()`)
before anything has run. Net effect: the pre-`STATE` window now reads
whatever `rustty_global::logging()` currently holds — a real default before
`init_with_action` computes anything, and the FULLY RESOLVED value from the
moment `init_with_action` computes it onward, which (after this fix) is
BEFORE the bootstrap sub-inits run, matching the oracle's own ordering
exactly. This makes the port structurally identical to the oracle for this
window, not merely closer.

Both `crates/rustty-core/src/logging.rs`'s doc comment on `log` and §6r2's
"KNOWN LIMITATION"/"STATE claim" text are superseded by this fix; do not
restore the "permanent `Logging::default()` fallback" framing from §6r2 —
after this fix, the fallback is `rustty_global::logging()`, which is a real
default only before ANY init has computed anything, not a permanent
divergence from the resolved flags.

### 3. A third artifact — `rustty-wasm` — had no backend at all

`crates/rustty-wasm/src/lib.rs`'s `log_sink` module only `#[used]`-retains
`rustty_os::wasm::log::freestanding::log`'s symbol; it never registered a
`log::Log`. `cargo tree -p rustty-wasm` links `rustty-vt`, `rustty-config`,
`rustty-os`, `rustty-args`, `rustty-font-shaper`, `rustty-simd` — **40 of the
73 rows' crates**. The oracle's `main_wasm.zig` DOES install
`freestanding::log` as its `std_options.logFn`, live for the whole module —
this is CRITICAL 2(a)'s exact shape, one artifact over: pre-existing (not
introduced by this stage's diff), but it would have silently muted 40 rows
in the wasm build the moment 14b-14e restored them.

**Fix, hosted in `rustty-os` rather than `rustty-wasm` itself:**
`rustty-os` is already a dependency of every crate that owns a real
JS-callable entry point in this artifact (`rustty-config`'s wasm-feature
`config_new`, `rustty-wasm`'s own `rustty_wasm_atlas_data`/`_size`), so
hosting the registration there — rather than in `rustty-wasm`, which
NOTHING calls into first — avoids a layering problem entirely. Added to
`crates/rustty-os/src/wasm/log.rs` (`#[cfg(target_arch = "wasm32")]`
throughout): `struct WasmLogger` (`impl log::Log`, forwarding into
`freestanding::log`), `fn level_text` (the same 5-to-4-level mapping as the
other two backends), and `pub fn install_log_backend()` — `Once`-guarded,
`set_max_level` only in the `set_logger`-success arm (same contained shape
as the other two backends). Level default ports `main_wasm.zig`'s own
`std_options.log_level` (`.Debug => .debug, .ReleaseSmall => .warn, else =>
.info`; the Rust build model's missing `ReleaseSmall` cfg collapses that arm
into `else => .info`, the SAME documented divergence `rustty-wasm`'s own
`LOG_LEVEL` const already carries). `rustty-os/Cargo.toml` gained a
`[target.'cfg(target_arch = "wasm32")'.dependencies] log = { workspace =
true }` entry.

Installed from `rustty-config::wasm::config_new` (the actual first
JS-callable entry point) and, as cheap `Once`-guarded insurance,
`rustty-wasm`'s own `rustty_wasm_atlas_data`/`rustty_wasm_atlas_size` (in
case a host calls an atlas export before ever calling `config_new`).

A proving test (`facade_tests::install_log_backend_registers_without_
panicking`, `#[cfg(target_arch = "wasm32")]`-gated) was added alongside the
new module. **Verification limitation, disclosed rather than papered over:**
this environment has no `wasmtime`/`wasmer` (checked: `which wasmtime
wasmer` finds neither), and `cargo test -p rustty-os --target
wasm32-unknown-unknown` is ALREADY broken independent of this change (a
pre-existing, unrelated `libc::sysconf` reference in
`page_allocator.rs` that only compiles under `cfg(target_family = "unix")`,
confirmed via `git stash` — the same failure occurs on the pre-round-3 tree).
So the new test is verified to TYPE-CHECK (`cargo build -p rustty-os
--target wasm32-unknown-unknown --features wasm-host-owned` succeeds) but
could not be EXECUTED in this environment. This is a real, disclosed gap,
not a claimed pass.

### wasm cost, re-measured a third time

| build | `rustty-vt.wasm` |
|---|---|
| before task 14a | 892,220 bytes |
| after §6 | 885,927 bytes |
| after §6r | 886,918 bytes |
| after §6r2 | 888,020 bytes |
| after §6r3 (this section) | 888,131 bytes |

Delta from §6r2: **+111 bytes** (one more `install_log_backend()` call site
in `rustty_mode_report_encode`; finding 3's `rustty-os`/`rustty-config`/
`rustty-wasm` changes do not touch `rustty-vt-c`'s own wasm artifact at all,
so they are not reflected in this table — they affect the SEPARATE
`rustty_wasm.wasm` artifact, which could not be built or measured in this
environment for the pre-existing, unrelated reason above). Still **−4,089
bytes** net against the pre-14a baseline. Reproduced twice, identical.

### Doc corrections in this round

- §6r's crate table (the one 14b-14e are told to read a row's crate off)
  now annotates the `rustty-bench` row with a pointer to this section rather
  than leaving it looking identical to every closable row.
- §6r2's rationale for "why `rustty-bench` isn't closable" was itself wrong
  in one claim ("it is not linked from the app either") — `cargo tree -p
  rustty-apprt-c -i rustty-bench` shows the edge (`rustty-apprt-c/src/
  lib.rs:160`'s `extern crate rustty_bench as _;`, forcing
  `rustty_benchmark_cli` into the libghostty/macOS staticlib). The verdict on
  S62-F2 is unchanged (still not closable — its own site is reached solely
  from the standalone `[[bin]]`), but the blanket "the whole crate is
  unreachable" framing was too broad: most of `rustty-bench`'s code IS
  reached through the libghostty artifact and DOES have a real backend once
  finding 1 (round 2) lands. §6r2's paragraph 3 is corrected in place (marked
  as a correction) rather than silently rewritten.

### Re-verification after this round's fixes

- `cargo test -p rustty-vt-c --all-features`: 492 passed, 0 failed.
- `cargo test -p rustty-core --lib logging::`: 5 passed, 0 failed.
- `cargo test -p rustty-apprt-c --lib rustty_init`: 1 passed, 0 failed.
- `cargo test -p rustty-apprt --lib`: 12 passed, 0 failed.
- `cargo test -p rustty-os --lib`: 84 passed, 0 failed (host target; the new
  wasm32-only facade test does not run here by construction).
- `cargo test -p rustty-config --lib --features wasm`: 227 passed, 0 failed.
- `cargo test -p rustty-wasm --lib`: 0 tests (host target; this crate's log
  facade code is wasm32-only).
- `cargo check --workspace --all-targets`: clean.
- `cargo clippy -p {rustty-vt-c,rustty-core,rustty-apprt-c,rustty-apprt,
  rustty-os,rustty-config,rustty-wasm,rustty} --all-targets --all-features
  -- -D warnings`: all clean (rustty-wasm/rustty-os without `--all-features`
  where a feature doesn't apply).
- `cargo fmt --check` (all eight touched crates): clean.
- `cargo build -p rustty-os --target wasm32-unknown-unknown
  --features wasm-host-owned`: clean (confirms the new wasm-only module
  type-checks for the real target).
- No ledger row touched; the six `CLOSED` rows are unchanged.

---

## §6r4 — call shape 2 never happened: what the crates ACTUALLY do (2026-09-10)

Recorded by task-14d-2, which hit the divergence while restoring
`src/apprt/gtk/class/config.zig`'s one site and was nearly misled by it. This
section **supersedes §6r IMPORTANT 5's three-shape rule**. IMPORTANT 5's crate
TABLE is untouched and still binding — only its prescription changes.

### The divergence, measured

IMPORTANT 5 says `rustty-core` (5 rows), `rustty-apprt` (10) and `rustty-cli`
(2) — the three crates that already have a real `rustty-core` edge — restore
their sites as `rustty_core::logging::log(level, scope, message)` DIRECTLY,
"the exact same 'you already have the edge, don't add a facade indirection'
rule `rustty-vt-c`'s own sites follow", and that no `log` dependency is added
to any of the three.

Three stages later, that is not what any of them does:

```
$ grep -rn --include='*.rs' 'logging::log(' crates/
(no matches)
```

- **`rustty-core`** restored `App.zig`'s 11 sites (task-14d-1) and
  `Surface.zig`'s 100 sites as `log::warn!`/`info!`/`debug!` with
  `target: LOG_SCOPE`. `crates/rustty-core/Cargo.toml` carries
  `log = { workspace = true }`.
- **`rustty-apprt`** restored `gtk/class/config.zig`'s site the same way
  (task-14d-2) and gained the same dependency line.
- **`rustty-cli`**'s 2 rows are still `OPEN`, so it has no restored site either
  way.

The only surviving direct call in the workspace is
`crates/rustty/src/main_rustty.rs:472`'s `logging::log_with(...)`, which is the
bin's own port of `main_ghostty.zig`'s `logFn` — not a restored `LOG` row and
not what call shape 2 described.

> **CORRECTED by task-14d-3: the grep above missed three.** `grep 'logging::log('`
> is anchored on the PATH-QUALIFIED spelling. `crates/rustty-apprt-c/src/lib.rs`
> imported the sink as `use rustty_core::logging::{log, Level};` and called it as
> a bare `log(Level::Err, "default", ...)` — three sites, the port of
> `src/main_c.zig`'s `std.log.err`/`.info`. So call shape 2 had **three**
> adherents, not zero. **This does not change the ruling**; it strengthens the
> second argument for it. Those three were exactly what §6r4 predicts:
> `discover_emitters` cannot see the shape, so `src/main_c.zig` measured
> 3-short while emitting all 3, and its allowance would have stayed stale
> forever. task-14d-3 converted them to
> `log::{error,info}!(target: "")` and the file reconciled to 0. The lesson is
> the one PROCEDURE.md's blind spot 3 already states in another form: a grep
> anchored on one spelling of a call cannot enumerate that call's sites.
>
> That conversion surfaced a second defect, fixed in the same commit: the
> facade's `.default` scope is the EMPTY target, and `log_with`'s port of
> `logFn` tests `scope == "default"`, so `target: ""` rendered `warning(): msg`
> where the reference prints `warning: msg`. Two live sites already had it
> (`renderer/overlay.rs`, `shadertoy.rs`). Normalized in
> `rustty_core::logging::scope_from_target`.

### Why the prescription does not survive contact, and the practice does

1. **The indirection it forbids lands in the same function one frame later.**
   `AppLogger::log` (§6r CRITICAL 2) forwards straight into
   `rustty_core::logging::log`. So `log::warn!(target: "app", …)` from
   `rustty-core` and `logging::log(Level::Warn, "app", …)` from `rustty-core`
   reach the SAME sink, with the same `Logging::default()` fallback that
   CRITICAL 2's known-limitation note describes. The anti-indirection argument
   is real for `rustty-vt-c` (whose sink ends in a C callback the facade cannot
   reach without a backend) and empty for these three.
2. **`G-LOGFILE` cannot see the direct shape.** `discover_emitters`'s `FN_DEF`
   regex is `\bfn\s+(log_[A-Za-z0-9_]*)\s*[(<]` — it requires a `log_`
   PREFIX. `logging.rs`'s sink is `pub fn log(...)`, so it is never discovered
   as an emitter and every call to it is counted as a DROPPED diagnostic. A row
   closed on call shape 2 would leave a permanently stale allowance behind it,
   which the ratchet then reports as RED forever. This is not a reason the
   shape is wrong — it is a reason the shape cannot be adopted without teaching
   the gate first, and no stage has.

### The rule, restated

**Two shapes, not three:**

1. **`rustty-vt-c` itself** — direct `sys::log_fn(...)`. Unchanged from §6.
2. **Every other crate, all 14 in IMPORTANT 5's table** —
   `log::{warn,error,info,debug}!(target: "<scope>", ...)`, `""` for the
   reference's `.default` scope. `rustty-core`, `rustty-apprt` and
   `rustty-cli` are NOT special-cased.

**With one documented exception, which is the reverse of the one IMPORTANT 5
made:** `rustty-config` keeps its OWN `format_log`/`log_{info,warn,debug}`
sink and does not use the facade. §6r2's forward-looking note called this
correctly before any of its rows were restored, and task-14d-2 confirmed it in
practice: those helpers read `rustty_global::logging()`, a leaf-crate
process-global that needs no install call and works in tests, benches, the exe
and the lib alike, so the facade would be strictly LESS reachable there. All
**30** live emissions in that crate use it, and `G-LOGFILE` counts a
`log_*`-prefixed fn facade as an emission, so the shape is measurable.

> **NUMBER CORRECTED by task-14d-3.** This said "all 26". 26 is ONE reference
> file's count — `src/config/Config.zig`'s arm, which G-LOGFILE reports as
> `config_struct.rs+value_types.rs=26`. The crate carries four more through the
> same sink: `file_load.rs=2` and `path.rs=2`. The substantive claim (every
> emission in `rustty-config` goes through its own `format_log`/`log_*` sink,
> none through the `log` facade) is unchanged and still true; only the number
> was wrong. Read a crate-wide count off `check_log_reconcile.py --report`,
> which reports per REFERENCE FILE, never off a single arm.

### The cost this accepts, stated plainly

A `log::warn!` emits nothing until some binary calls
`install_log_backend`/`install_app_log_backend`, and it is additionally gated by
`log::max_level()`. A direct `logging::log(...)` would fire unconditionally.
That difference is invisible in the `rustty` app binary (`main_rustty.rs`
installs the backend immediately after `state_init()`, before anything logs) and
in the `rustty-vt-c` artifacts (every `*_new` C export installs), but it is real
for **unit tests, benches, and any embedder that links one of these crates
without installing a backend** — those see silence where the reference logs.
That is precisely the property `rustty-config` declines to give up, and it is
the one thing a future stage might revisit. Revisiting it means teaching
`discover_emitters` about the bare `fn log` sink FIRST, in the same commit, with
a self-test arm (R5).

---

## §7 — Reachability constraints binding stages 14b–14e (controller, after round-3 review)

The logging pattern is **settled**; three rows are nevertheless **not closable** and are annotated in
`LEDGER.tsv`'s `owner` column so a later stage cannot mark them green by mistake.

| row | crate | why |
|---|---|---|
| `S62-F2` | rustty-bench | Site is `cli.rs:201-208 pub fn main()`, reached only from the standalone `[[bin]]` (`main_bench.rs`), never from `rustty_benchmark_cli`. No artifact installs a backend there. (The bench *library* IS linked into libghostty via `rustty-apprt-c/src/lib.rs:160`; only this row's own site is bin-only.) |
| `S91-F7` | rustty-font-shaper | Only artifact is `rustty_web_canvas.wasm`, which has **no** `log::Log` backend and no `install_log_backend` call site anywhere in `rustty-web-canvas` or `rustty-font-shaper`. |
| `S97-F2` | rustty-font-shaper | Same artifact, same gap. |

**Whichever stage owns `rustty-font-shaper` (11 rows) must first add an install call to the
`rustty_wc_*` entry points**, then S91-F7/S97-F2 become closable. The demo's real call order
(`example/wasm-web-canvas/index.html`) is `rustty_wc_alloc:86` → `_face_new:94` → `_atlas_new:98` →
`_face_render_glyph:103` → `_shaper_new:153` → `_shape:162` → and only at `:168`
`rustty_terminal_new`, the vt-c installer — so everything before line 168 runs unlogged today. The
dependency edges already exist (`rustty-web-canvas` → `rustty-vt-c` → `rustty-os`;
`rustty-font-shaper`'s own optional `rustty-os` edge at `Cargo.toml:53`).

**Per-row check required for the other font-shaper rows** (S81-F1, S84-F1, S86-F2, S95-F1): each is
closable on the native app path via `AppLogger`, but if its file is also reached from
`rustty_wc_shaper_shape`, that artifact mutes it. Check per row rather than assuming.

**Six stateless vt-c exports still do not install** — `rustty_paste_encode` (`lib.rs:519`),
`rustty_color_parse`/`_x11`/`_palette_entry` (`:214/228/242`), `rustty_alloc` (`:413`),
`rustty_wasm_alloc` (`:2016`). No shipped example reaches only those, so no row is muted today; close
whenever convenient.

**Carried to the finale:** (a) a Node-based end-to-end proof that a `log::` macro reaches the wasm
sink — `node` v25.6.0 is on PATH and `.port/tools/wasm-parity.mjs` / `wasm-vt-roundtrip.mjs` already
compile, instantiate and call into rustty wasm artifacts, so the earlier "no runtime available" note
was under-researched and a ~40-line sibling harness is the right shape; (b) the stale comment at
`crates/rustty/src/main_rustty.rs:131-137`, which still says bootstrap sites read
`Logging::default()` — superseded by the round-3 fix.

> **(a) is DONE (task-14wasm-proof, 2026-09-10).** The harness is
> **`.port/tools/wasm-log-emission.mjs`**. It compiles the DEBUG
> `rustty_web_canvas.wasm`, instantiates it with an `env.log` that decodes the `(ptr, len)` string
> out of `ex.memory.buffer` — that import IS the terminal end of
> `log::warn!` → `WasmLogger::log` → `freestanding::log` → `js::log` — and drives
> `rustty_wc_alloc` → `rustty_wc_deferred_new` → `rustty_wc_deferred_has_codepoint` with an invalid
> CSS font value. It CAPTURES, verbatim:
>
> ```
> warning(deferred_face): failed to init face for codepoint check face=OTTOgarbage-font-bytes err=InvalidFontStr
> ```
>
> Those three exports are `rustty-web-canvas`-owned and were among the twelve that did NOT install a
> backend before the artifact-wide installer fix (`3a90678c`), so this is the exact host that used to
> lose the record. A counterfactual artifact built with the twelve install calls deleted captures
> NOTHING through the same drive (`--expect-silence`), and each build is RED under the other's
> expectation — the installer fix is proven to have mattered at RUNTIME, not merely in the source
> scan the two `install_contract_tests` perform. Two negative controls guard the positive arm (the
> presentation fast path emits zero records; a well-formed font emits the success-path
> `debug(font_face): face initialized: monospace` and its `metrics font=… value=FaceMetrics {…}`
> sibling, and zero failure warns). DEBUG-only is asserted in the harness, since `release` strips the
> call sites. Wired into `ci.sh`'s opt-in `--webgl` lane (which already requires the wasm32 target),
> ahead of the Chrome-dependent gate; it exits 3 BLOCKED rather than skipping green when the artifact
> or `node` is absent.
>
> Two brief-vs-code corrections found while building it, recorded here because they outlive the task:
> 1. **A well-formed input through this path does NOT produce zero log calls.** `Face::init_named`'s
>    success arm emits `face initialized: {}` and `calc_metrics` emits `metrics font=… value=…`, both
>    at DEBUG, and `install_log_backend` sets `LevelFilter::Debug` under `debug_assertions`. The
>    negative control has to be the presentation FAST PATH (which never initializes a face); the
>    well-formed case is asserted by its exact records instead.
> 2. **`.port/tools/web-canvas-glue.mjs` did not supply `env.log`,** so
>    `web-canvas-instantiate.mjs` had been failing with `LinkError: Import #14 "env" "log"` ever
>    since `install_wasm_log_backend()` put that import into the artifact — an unnoticed regression
>    from this plan's own work, in a script no gate runs. The glue now supplies a discarding default
>    (overridable), and that script passes again.

## §8 — task-14wasm: comptime-equivalent gate for the wasm release build (2026-09-09)

**Blocks stage 14c. Closes no ledger rows.** A review of stage 14b found that §6's claim (quoted and
struck through above) — that `sys::install_log_backend`'s `log::set_max_level(Off)` cap on
`wasm32`+`!debug_assertions` keeps the restored diagnostic strings out of a release wasm build — is
false, and that the restored strings were in fact linking into the artifact as dead weight.

### Diagnosis, verified against `log` 0.4.33's own source

- `log::warn!`/etc. expand to (`log-0.4.33/src/macros.rs:123`): `if lvl <= $crate::STATIC_MAX_LEVEL
  && lvl <= $crate::max_level() { /* build a Record, format args, dispatch */ }`.
- `log::max_level()` reads `MAX_LOG_LEVEL_FILTER`, an `AtomicUsize` — a RUNTIME value.
  `log::set_max_level` (what `install_log_backend` calls) writes that atomic. This check can only
  ever skip the *call*, at *runtime*; the compiler cannot fold it away because the atomic's value is
  not known until the process runs, so the format-string/`Record`-construction/`Debug`-impl code the
  `if` body needs is always compiled in and linked.
- `STATIC_MAX_LEVEL` (`log-0.4.33/src/lib.rs:1637-1643`) is a `const`, resolved entirely at COMPILE
  TIME from `cfg!(debug_assertions)` and the crate's own `release_max_level_*` Cargo features
  (`off`/`error`/`warn`/`info`/`debug`/`trace`; unset defaults to `Trace`, i.e. no compile-time cap).
  Because `lvl` at each call site is also a compile-time literal (`Level::Warn`, etc.), `lvl <=
  STATIC_MAX_LEVEL` is a fully-const comparison; when it folds to `false` the entire guarded body —
  including the argument-formatting and `Record`-construction code — is dead code the compiler is
  free to (and does, in a release build) strip. This is the genuine analog of the oracle's comptime
  `options.log_level = .err` gate (`ghostty/src/lib_vt.zig:447-455`), which similarly erases guarded
  `std.log.warn`/etc. call sites from the binary at compile time.
- `grep -rn "release_max_level" .` returned **no matches** anywhere in this workspace before this
  stage — confirmed with `grep -rn "release_max_level" . --include="*.toml" --include="*.rs"
  --include="*.sh" --include="*.mjs"`. The compile-time gate the oracle actually relies on had no
  Rust equivalent wired up; only the runtance one existed.

**Diagnosis confirmed in full** — the runtime/comptime distinction is real, `release_max_level_off`
was genuinely unset, and §6's claim was false. No refutation found.

### The mechanism chosen: a wasm32-target-gated `log` dependency feature

`release_max_level_off` is a *global* switch on `log`'s own compilation unit: Cargo's feature
unification means every crate in a given target's dependency graph shares ONE compiled `log` crate
with the UNION of features anyone requested for that target. Turning it on in `[workspace.
dependencies]`, or unconditionally on any crate's `log = { workspace = true }` line, would therefore
also silence `rustty-core::logging::AppLogger` — the native `rustty` app binary's own sink — the
first time that binary itself gets a `!debug_assertions` (release) build. That is exactly the
"silently mutes the app build" outcome the task ruled out; it would undo everything stages 14a/14b
are restoring for the real embedder use case.

The workspace already has an established mechanism for exactly this shape of problem —
target-conditioned dependency tables — used one line above the fix (`rustty-os` is pulled in only
`[target.'cfg(target_arch = "wasm32")'.dependencies]` for the `os/wasm/log.zig` sink). The same table
in `crates/rustty-vt-c/Cargo.toml` now also carries:

```toml
[target.'cfg(target_arch = "wasm32")'.dependencies]
rustty-os = { workspace = true }
log = { workspace = true, features = ["release_max_level_off"] }
```

Cargo only activates a `[target.'cfg(...)'.dependencies]` table when the ACTUAL compilation target
matches the cfg predicate — not the host, not some other crate's target — so this feature is only
ever requested when something is being compiled `--target wasm32-unknown-unknown`. The native
`rustty` binary (built for `x86_64`/`aarch64`, never `wasm32`) never touches this table, so `log`'s
`STATIC_MAX_LEVEL` stays at its unset default (`Trace`, i.e. no compile-time cap) for that build
regardless of debug/release profile. Verified directly:

- `cargo tree -p rustty-vt-c --target wasm32-unknown-unknown -e features -i log | grep release_max_level`
  → `log feature "release_max_level_off"` (present).
- `cargo tree -p rustty --target x86_64-apple-darwin -e features -i log | grep release_max_level`
  → no output (absent).

Combined with `log`'s own `cfg!(debug_assertions)` outer branch, the net effect is: `STATIC_MAX_LEVEL
== Off` only for a `wasm32` target compiled `!debug_assertions` (i.e. `--release`) — precisely the
one configuration the oracle's own guard targets (`lib_vt.zig`'s `.log_level = .err` is itself only
reached by the non-debug wasm build arm), and no other configuration is touched. One declaration
covers every crate reachable from a `rustty-vt-c`-rooted wasm artifact (feature unification does the
rest for `rustty-vt` today and whatever 14c-14e add below it), so 14c-14e do not need to repeat this
per crate — only a NEW wasm-artifact root crate that does not already depend on `rustty-vt-c` would
need its own such table.

The runtime `set_max_level(Off)` cap in `install_log_backend` is left in place — it is not wrong, just
insufficient on its own; it remains useful defense-in-depth (e.g. a debug-build wasm host, where the
compile-time gate does not apply, still gets the runtime filter) and its own doc comment in
`crates/rustty-vt-c/src/sys.rs` no longer claims it prevents linking.

### Measured: `.port/tools/build-vt-wasm.sh --no-default-features --features render-state`

| build | `rustty-vt.wasm` |
|---|---|
| before (this stage's Cargo.toml edit reverted, working tree otherwise at `23702ea0`) | 911,987 bytes |
| after (this stage's `[target.'cfg(target_arch = "wasm32")'.dependencies]` feature line) | 884,786 bytes |

**Delta: −27,201 bytes (≈3.0%).** Reproduced twice with `cargo clean -p rustty-vt-c --release --target
wasm32-unknown-unknown` between runs; identical byte count (884,786) both times. This is larger than
the `+18,253` bytes stage 14b added, i.e. this fix alone recovers more than 14b cost and leaves the
artifact smaller than it was even before 14b's restored `rustty-vt` sites — confirming the restored
strings really were the dead weight diagnosed, not the `log` crate's own baseline linkage cost (which
§3/§6 already measured at ~26 bytes).

### Proof the native build still logs

`cargo test -p rustty-core --lib logging::` — 5/5 pass, including
`app_log_backend_installs_and_dispatches_without_panicking`, which installs `AppLogger` and drives a
real `log::warn!`-shaped record through it end to end. `cargo test -p rustty-vt-c --lib sys::` — 11/11
pass (non-wasm/native target), including `log_facade_backend_reaches_the_c_sink`, which installs
`Logger` and asserts a `log::warn!` call is captured by the C-ABI sink. Both tests build for the host
target (`x86_64-apple-darwin`), which — per the `cargo tree -i log` check above — never carries
`release_max_level_off`, so both prove the native/non-wasm path is unaffected by this stage.

### Concerns

- The runtime `Off` cap's own doc comments inside `crates/rustty-vt-c/src/sys.rs` (around
  `install_log_backend`) still contain language close to §6's original false claim ("which `Off`
  guarantees by never constructing a `log::Record` ... short-circuits first") — accurate about
  runtime dispatch, misleading about linking. Not corrected in this stage (out of the stated scope,
  which is the pattern fix + the decision doc); whichever stage next touches that function should
  update its comment to point at this section instead of re-deriving the runtime/comptime
  distinction.
- Checked (not just assumed): `cargo tree -p rustty-web-canvas --target wasm32-unknown-unknown -e
  features -i log` also shows `release_max_level_off` active, because `rustty-web-canvas` depends on
  `rustty-vt-c` (`build-web-canvas-wasm.sh` builds `-p rustty-web-canvas --target
  wasm32-unknown-unknown`) and Cargo evaluates a `[target.'cfg(...)']` table against the actual
  compile target, not the crate that declared it — so this one declaration already covers that
  artifact too. Not independently re-measured by BYTE COUNT in this stage (only `build-vt-wasm.sh`'s
  artifact was, per the task's scope); worth a size re-check once `rustty-font-shaper`'s own `LOG`
  rows are restored, since that crate's sites are the ones actually reachable from
  `rustty_web_canvas.wasm` (§7's font-shaper gap notes) and were not touched by 14a/14b.
  `build-main-wasm.sh`'s artifact was not checked at all this stage — do a `cargo tree ... -i log`
  pass on it before assuming coverage.

### §8r — fix-review round 1 (2026-09-09): the gate reached only two of the three wasm artifacts

The §8 text above concluded that "one declaration covers every crate reachable from a
`rustty-vt-c`-rooted wasm artifact ... so 14c-14e do not need to repeat this per crate", and its own
Concerns bullet flagged that `build-main-wasm.sh`'s artifact had not been checked. The review did
that check. **It was not covered.** Feature unification is per-build-graph, and `rustty-wasm` — the
root of `rustty_wasm.wasm` — does not depend on `rustty-vt-c` at all. It does depend on `rustty-vt`,
whose `log = { workspace = true }` (`crates/rustty-vt/Cargo.toml:204`) is unconditional, so every
diagnostic string stages 14a/14b restored in `rustty-vt` was still linking into that artifact.

**The rule, corrected:** the feature must be declared once per WASM ARTIFACT ROOT CRATE — the crate
named in a `cargo build -p <crate> --target wasm32-unknown-unknown` — not once per workspace, and not
once per crate that contains call sites. Feature unification then covers everything below that root.

| artifact | root crate | covered by | `STATIC_MAX_LEVEL == Off` |
|---|---|---|---|
| `rustty_vt.wasm` | `rustty-vt-c` | its own table | yes |
| `rustty_web_canvas.wasm` | `rustty-web-canvas` | depends on `rustty-vt-c` | yes |
| `rustty_wasm.wasm` | `rustty-wasm` | **its own table, added in this round** | yes (was: no) |

Verified for all three with `cargo tree -p <root> --target wasm32-unknown-unknown -e features -i log`,
and that `rustty`, `rustty-vt-c` and `rustty-core` all show NO `release_max_level_off` when the same
query is run against `aarch64-apple-darwin` — the native app build keeps its logging.

Measured on `rustty_wasm.wasm` (`.port/tools/build-main-wasm.sh --release`, `cargo clean -p
rustty-wasm --release --target wasm32-unknown-unknown` between runs):

| build | `rustty_wasm.wasm` |
|---|---|
| before (this round's `crates/rustty-wasm/Cargo.toml` table reverted) | 1,000,535 bytes |
| after | 992,196 bytes |

**Delta: −8,339 bytes.** Smaller than `rustty_vt.wasm`'s −27,201 because this artifact links a
narrower slice of `rustty-vt` (no `rustty-vt-c` C-ABI surface), but it is the same dead weight, and it
would have grown with every row stages 14c-14e restore.

#### Regression found by building the artifact rather than reasoning about it

`rustty_wasm.wasm` did not build at all. Task 3's `@embed-abi` freeze in
`crates/rustty-input/src/command.rs` asserts LP64 field offsets (`action` at 8, `title` at 16,
`description` at 24) as unconditional `const _: () = assert!(...)`. On `wasm32-unknown-unknown`
pointers are 4 bytes, so all three fail const evaluation and `rustty-input` — reachable from
`rustty-wasm` via `rustty-config` — does not compile. Task 3 landed those asserts and was reviewed
four times without anyone compiling for `wasm32`; this is the same class as the four Linux breakages,
one target over.

Fixed by gating the offset asserts on `#[cfg(target_pointer_width = "64")]` while leaving the
`@embed-abi` comment annotations (which the A4 gate greps) unconditional. `rustty-embed.h` is a
native-host header and no wasm artifact exports that C ABI, so nothing is lost. The size assert was
also tightened from `4 * size_of::<*const c_char>()` — tautological under any `repr`, the Task 3
review's own finding, still standing — to the hard literal `32`. `check_embed_h_abi.sh` still passes
(19/60 aggregates frozen) and `cargo build --workspace` is green.

**Standing constraint, added:** a change that can affect layout, features, or linkage must be built
for `wasm32-unknown-unknown` as well as the Linux lane before it is called done. All three artifacts
build green as of this round: `rustty_vt.wasm` 884,786 · `rustty_web_canvas.wasm` 1,587,089 ·
`rustty_wasm.wasm` 992,196.
