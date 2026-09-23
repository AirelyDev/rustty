# rustty PORT RULES (amendments to the Parity Constitution)

These are the binding decisions agreed during alignment. They AMEND the Parity
Constitution in the kickoff prompt. Where they conflict, these win. Every
autonomous phase/workflow MUST obey them.

## R1 — 100% FULL FIDELITY (no loss, ever)

The port reproduces the reference with **zero loss of content**:

- **Every line of code** is ported (translated to idiomatic, parity-preserving Rust).
- **Every comment is preserved** — doc comments, inline comments, TODOs, license
  headers, ASCII diagrams, citations/spec references. A comment may be *updated*
  only to reference the correct rustty name/path/type when it names a
  ghostty-specific identifier (see R2); its meaning must not change. Comments are
  NEVER dropped or summarized away.
- **Every `test {}` block** → a Rust test, **assertion-for-assertion**. The
  Phase-0 inventory count is a contract; a missing test or dropped assertion is a
  blocker.
- **Every assertion, error case, edge case, and code path** is preserved.
- Comptime-generated artifacts (parse tables, etc.) are reproduced bit-for-bit.

**No-loss, not no-addition.** Necessary Rust-only glue (traits/DI introduced to
break crate cycles per R4, `unsafe` wrappers around C FFI, allocator shims) is
allowed *because it adds nothing-lost* — it must never replace, omit, or weaken
reference logic, comments, or tests. Such additions are the minimum required to
compile/links and are themselves reviewed for soundness.

Verification: differential tests + ported assertions + an adversarial
"completeness critic" that diffs comment/test/assertion inventories per module
against the Phase-0 inventory. Any delta is a blocker.

## R2 — Product identity: rename to rustty, with a ghostty parity profile

All **published** artifacts carry the `rustty` identity (clean standalone, NOT a
ghostty drop-in): binary `rustty`, `rustty_*` C symbols, `include/rustty.h` +
`rustty/vt/*`, `TERM=xterm-rustty`, `TERM_PROGRAM=rustty`, terminfo `rustty`,
config dir `~/.config/rustty`, version/XTVERSION/DA responses reporting rustty.

A single `rustty-identity` source-of-truth crate holds all of these. A build
feature **`identity-oracle`** flips every value to the reference's, used **only**
by the parity harness so differential output matches the reference byte-for-byte
(no canonicalization hacks). The identity-EQUIVALENCE gates from the constitution
(ghostty.h ABI match, Swift-app-links-unchanged, terminfo==ghostty) are dropped;
all BEHAVIORAL parity gates remain fully in force.

## R3 — Self-contained, no ghostty dependency

rustty copies every needed C source into its own tree and builds it standalone
(`cc` + `bindgen`). It has **zero build- or run-time dependency** on
`/Users/gael/projects/ghostty`. The reference is used ONLY as an external oracle
by the parity harness (separate process / pre-generated golden corpora), never as
a Cargo dependency. C deps stay the SAME libraries (freetype, harfbuzz,
fontconfig, oniguruma, simdutf, highway, glslang, spirv-cross, wuffs, zlib,
libpng, sentry/breakpad, dcimgui). No pure-Rust substitutions during parity
phases (those are Goal-2 candidates only). Rust *bindings* to the same C libs
(gtk-rs, objc) are fine — they are not reimplementations.

## R4 — Crate structure: Layered Hybrid, acyclic, gate-enforced

Crates are named `rustty-*` only. The crate graph MUST be acyclic (Cargo
requirement) and is enforced by `.port/tools/cycle_check.py` run as a gate.

Cycle-breaking refactors (R4 breaks) are behavior-preserving structural changes
ONLY; they redistribute code across crates and add minimal glue, never altering
logic/comments/tests (R1). See `.port/cycle-analysis.md` for the full break-list.
The master hub is `global.zig` (→ `rustty-global` leaf + wiring).

FIRM crates (proven clean): `rustty-{util,identity,datastruct,unicode,simd,lib,
global,build-config,os,render-types,vt,vt-c,crash}`.

TARGET upper-tier crates, carved incrementally as their break lands, under the
acyclicity gate: `rustty-{cfg-fmt,args,apprt-api,font,config,input,inspector,
renderer,termio,apprt,core,cli}`. Documented grouping fallback allowed if a split
proves uneconomical.

## R5 — Gates are invariant; never weaken to pass

Never weaken, skip, xfail, or loosen a test or gate. A red gate that cannot be
made green honestly → STOP and surface a precise blocker. Parity is the
invariant; performance is the gradient (Goal 2, only after Goal 1 is locked).

## R6 — Memory safety & leak gate: the right tool per job (Miri primary)

Memory safety/UB/leak detection uses the correct tool for each crate, not a
custom reimplementation:

- **Pure-Rust crates → Miri** (`cargo +nightly miri test -p <crate>`): the PRIMARY
  gate. Catches UB (use-after-free, OOB, uninit, invalid aliasing) AND leaks —
  strictly stronger than the reference's `std.testing.allocator` leak check.
  Installed and validated (`miri 0.1.0`).
- **Rust↔C FFI crates → AddressSanitizer** (`-Zsanitizer=address`): Miri can't run
  foreign C, so ASan covers UB across the boundary (simd, font, renderer, config,
  crash, inspector).
- **FFI leak detection → Linux CI** (LSan/valgrind): unavailable on arm64 macOS, so
  deferred to the Linux CI box — documented, never skipped.
- The tracking-allocator approach is **removed** — it was a wrong default chosen
  only because Miri appeared uninstalled. Gate runner: `.port/tools/gates/check_leaks.sh`.
- Miri is slow (interpreted): it gates per-crate completion + CI; fast native
  `cargo test` gates assertion parity on every porting iteration.

General principle (per user guidance): pick the right, considered tool for each
job, weigh tradeoffs (OS/arch matters), and escalate only when a tradeoff is a
genuine judgment call for the user. See memory `right-tool-for-the-job`.

## DG2/DG4 — Release-profile semantics (locked fork, 2026-07-10)

Ghostty's two build modes differ in failure semantics: its dev builds are Zig
**ReleaseSafe** (integer overflow / bounds / `std.debug.assert` all TRAP);
its tagged/shipped artifacts are **ReleaseFast** (the same overflow is UB,
asserts elided). rustty resolves the fork in the root `Cargo.toml`
(`[profile.*]`, three profiles):

- **DG2 → option (a).** `[profile.release] overflow-checks = true`: rustty
  release keeps trap-parity with ReleaseSafe on plain arithmetic — strictly
  safer than the mirrored ReleaseFast artifact (where the same overflow is
  UB) — restoring trap semantics for e.g. the Kitty expected-size multiply
  (VALIDATION_REPORT M6/§3d). The `release-fast` profile
  (`overflow-checks = false`; Rust's defined wrap, never UB) mirrors the
  tagged ReleaseFast artifact for Goal-2 bench/perf work ONLY — never for
  parity gates (R5).
- **DG4 → option (a).** Zig `std.debug.assert` is active in ReleaseSafe, but
  its faithful port `debug_assert!` (109 rustty-vt sites) compiles out of a
  plain Rust release. The `release-safe` profile (`inherits = "release"`,
  `debug-assertions = true`, overflow-checks inherited ON) restores the
  oracle's ReleaseSafe assert semantics with zero code churn; the
  differential harness (`.port/phase-0/harness/build_port_driver.sh` →
  G4's `vt_driver_port`) builds with `--profile release-safe`. The CI test
  lane (`ci.sh` step 4, `cargo test --workspace`) runs the dev profile,
  where `debug-assertions` are already ON — so both assert-bearing lanes
  execute the oracle's ReleaseSafe assert semantics.

Regression anchor: `crates/rustty-vt/src/kitty/graphics_image.rs`
`image_load_expected_size_capacity_width_u32_max` — the §3d Kitty
`w*h*bpp` capacity path, mirrored against `graphics_image.zig:389-394`
(the oracle's `max_dimension` guard fires BEFORE the u32 multiply, so
out-of-range dimensions ERROR with `DimensionsTooLarge`, never trap).

## DG5 — C-ABI panic policy: ABORT, never unwind, never error-code (locked fork, 2026-07-10)

The decision recorded for VALIDATION_REPORT §3c (the 271 exported C-ABI fns of
`rustty-vt-c` + `rustty-apprt-c`, and every Rust callback handed to foreign
code — ObjC/Core Animation, C fn-pointer slots):

- **Policy: a Rust panic reaching a C-ABI boundary ABORTS the process.** This
  is the R1-faithful mirror of the reference: a Zig panic in a
  `callconv(.c)` export (or any Zig fn a C host calls) prints and aborts — it
  never unwinds into the caller and never turns into an error code. Documented
  placeholder leaves (`unimplemented!()` seams per the placeholder-leaf
  doctrine) therefore PANIC→ABORT when driven; that IS the policy, not a gap.
  No `catch_unwind`→error-code translation layer is added anywhere: the
  reference has no such behavior, and inventing one would diverge (R1).
- **Mechanical application:** every exported fn and every callback passed to
  foreign code uses plain `extern "C"` — NEVER `extern "C-unwind"`. Rust
  defines that a panic escaping an `extern "C"` boundary aborts, so the
  policy is enforced by the ABI string itself, uniformly, with zero per-fn
  wrappers. (Gated continuously by `no_c_unwind_in_c_api_sources`, which
  scans `crates/rustty-vt-c/src`, `crates/rustty-apprt-c/src`, AND
  `crates/rustty-renderer/src` for `extern "C-unwind"` — 0 tokens as of
  2026-07-10.) Callbacks that run inside foreign frames
  where an abort message matters (the Metal display path) additionally wrap
  their body in `abort_on_unwind` (catch_unwind + `std::process::abort`) —
  defense in depth, same observable policy.
- **Regression anchors:** `rustty-apprt-c` `exported_c_abi_panic_aborts_host`
  (a panicking `extern "C"` export driven in a subprocess must die by
  SIGABRT) and `rustty-renderer` `abort_on_unwind_aborts_process` (the
  display-path guard). New exported fns MUST be `extern "C"`; a reviewer
  seeing `"C-unwind"` on an exported/installed fn treats it as a policy
  violation unless a documented unwind contract with the host exists (none
  does today).

---

# R7–R12 — Bun-port adoptions (BINDING, ratified 2026-07-10)

Provenance: drafted as PROPOSED rules in `.port/phase-2/BUN_PORT_LEARNINGS.md`
§5 (detector references D1–D9 anchor in its §4); ratified in full — all six —
on **2026-07-10** (plan `docs/superpowers/plans/2026-07-10-port-realignment-
and-validation-hardening.md`, Task 15, decision gate DG1). Each rule only
tightens (R5-compatible). Where a rule cites a gate/artifact that a Phase-3
plan task is still landing, that is noted inline; the rule is binding now and
the artifact's absence is tracked work, never a license to skip.

## R7 — Replacement-primitive semantic fidelity

When a Zig builtin/idiom is replaced by a Rust primitive with different edge semantics,
the divergence is decided explicitly per site, never inherited silently:

- **Narrowing:** `@intCast` → `T::try_from(x).unwrap()`. Bare `as` is reserved for
  `@truncate` (intentional wrap). A proven-hot loop may use `as` only with
  `// PERF(port): @intCast` and a benchmark citation.
- **Wrapping/saturating:** `+%`/`-%`/`*%` → `wrapping_*`; `+|`/`-|` → `saturating_*`.
  Never bare operators for these (under the landed DG2/DG4 profiles — root
  `Cargo.toml` `[profile.release] overflow-checks = true`, `release-safe`,
  `release-fast` — bare operators TRAP in dev and release; that is still not
  the Zig `+%` wrap semantic, and under `release-fast` they wrap, which is
  neither semantic decided per site).
- **Float→int:** `@intFromFloat` → `as` (Rust saturates where Zig is UB). Keep the Zig
  code's prior range checks; do not add new ones.
- **Slice reinterprets:** every site records truncate-vs-error and carries an
  odd/misaligned-length test (detector D1; gate `check_slice_reinterpret.sh`
  lands via plan Task 18).
- **Multi-object loops:** `for (a, b) |x, y|` ports with
  `debug_assert_eq!(a.len(), b.len())` — the length assert is part of the source
  semantics under R1, because `.zip()` silently truncates.
- **Asserts:** `std.debug.assert` → `debug_assert!`; `unreachable` → `unreachable!()`
  (checked); `catch unreachable` → `.expect(...)`. `unreachable_unchecked`/
  `unwrap_unchecked` are forbidden until the invariant is proven (an isolated
  non-FFI Miri proof harness per the in-repo mirihar doctrine —
  `.port/phase-2/mirihar-chunk3b`, `.port/phase-2/mirihar-wire2`; memory
  `shaper-hook-sb-clean-interleave` — or a closed-form argument), then marked
  `// PERF(port)`. Assertions are side-effect-free;
  a `debug_assert!` never discharges a safety obligation and never validates untrusted
  input — VT-stream and config validation is release-checked.
- **Comments:** a ported comment describing the OLD primitive's behavior is rewritten to
  the Rust primitive's contract (fidelity to intent over stale text); flagged in review
  (detector D9).

## R8 — Comptime-phase and build-switch preservation

- A Zig `comptime` parameter maps to a Rust const/generic/macro (ladder: plain generic >
  const generic > macro). Porting one to a runtime function is a flagged deviation
  requiring a written argument that no runtime bytes can alias the compile-time grammar
  (detector D4), plus golden byte tests on any emitted escape/format output.
- Zig build-mode/option switches (`builtin.mode`, build options, sanitizer/allocator
  toggles) port as `cfg`, NEVER constant-folded to one profile's value. The
  profile side of this fork is already landed in-repo: root `Cargo.toml`
  `[profile.release]` (overflow-checks on), `[profile.release-fast]`,
  `[profile.release-safe]` per §DG2/DG4 above — code must stay switchable
  across those profiles, not folded to any one of them. Allocator and
  instrumentation plumbing is the named highest-risk case (Bun Class 5).
- Derived constants port as the same const EXPRESSION as the Zig source, never a folded
  literal; zero placeholder constants survive a sub-batch gate (detector D3).
- Sanitizer lanes carry self-test canaries (intentional leak + overflow must fail)
  whenever the global allocator or instrumentation configuration changes (detector D6).

## R9 — Unsafe accounting and the unsound-safe ledger

- Ground truth: `.port/audits/count-unsafe.sh` (committed rg command; lands via
  plan Task 17); snapshots pinned to commits, categories reconcile exactly to
  the total.
- Every unsafe block carries `// SAFETY(<origin>/<dest>): <owner/aliasing contract>`
  using the fixed origin set {zig-port-idiom, ffi-boundary, raw-page-aliasing,
  callback-ctx, lifetime-erasure, uninit-buffer, marker-trait, perf} and Bun's five
  destinations. Missing tags fail the audit script.
- "Fixed" bar: safe code cannot cause UB in a release build; debug assertions do not
  qualify; wrappers that hide the invariant do not qualify.
- A parallel unsound-safe ledger (`.port/audits/UNSOUND_SAFE.md`, lands via plan
  Task 17) tracks every safe wrapper of a raw-ptr deref,
  &self→&mut accessor, and lifetime-less pointer-holding struct, triaged
  reachable-from-VT-stream / internal / theoretical; soundness fixes outrank
  count-reduction, always.
- Per-crate unsafe density is monotone non-increasing between snapshots; an increase
  requires a written justification in the introducing change. Reviews count
  `new_unsafe`; acceptance outside -sys/FFI crates requires `new_unsafe == 0`; each
  extern fn gets exactly one safe wrapper (pure externs may be `safe fn`
  redeclarations after an individual pure-vs-stateful review).
- Census cadence: snapshot + unsound-safe pass at every sub-project finale; RAII
  migrations convert one type at a time under the owning crate's Miri/ASan
  harness (R6 tooling, runner `.port/tools/gates/check_leaks.sh`). Where a
  covering test is Miri-intractable, exception routing follows the strict
  4-criteria protocol of `.port/phase-2/MIRI_EXCEPTIONS.md` — right-tool
  compensating coverage (native + ASan + tracking allocator +
  Miri-on-lighter-tests), never a skip.

## R10 — Marker and perf-intent discipline

- Machine-greppable markers only: `TODO(port): <reason>` (unresolvable now — "flagging
  is better than wrong code"), `PERF(port): <zig idiom>` (idiom downgraded to the plain
  form; the perf intent is debt), `PORT NOTE: <reshape reason>` (sanctioned structural
  divergence). Large ported files carry a PORT STATUS trailer (source path, confidence
  high/medium/low, marker count).
- `PERF(port)` is mandatory wherever a Zig perf idiom (appendAssumeCapacity, arena
  bulk-free, stack-fallback alloc, comptime monomorphization, packed tagged pointer) is
  ported as the plain idiomatic form. Before R2 perf parity is declared for a tier, a
  bench-driven pass greps `PERF(port)` and resolves each against the ghostty benchmarks.
- Endgame: a marker census by category feeds a leverage-ranked backlog
  (`unsafe_removed × affected_count / effort`); rejected-alternative decisions are
  recorded so they are not re-proposed. Sub-batch gates admit zero placeholder
  CONSTANTS (R8); `unimplemented!()` placeholder leaves remain governed by the
  placeholder-leaf doctrine and its dated C-F endgame audit (memory
  `placeholder-leaf-doctrine`).

## R11 — Adversarial verification protocol for bulk classification & review

- Split contexts: implementer, verifier, and fixer are separate agents; the verifier
  works a concrete violation checklist keyed to PORT_RULES and defaults to
  not-ok-if-uncertain; the fixer makes surgical edits only and may skip a hallucinated
  finding with a note; the orchestrator owns all path/naming decisions and all pushes.
- Bulk classifications (pointer fields, comment coverage, SAFETY tags, upstream-parity
  verdicts) require per-claim evidence in the form "file:line — what you saw"; every
  low-confidence claim plus a fixed random sample (~10-15%) of high-confidence claims
  receives 2-3 independent adversarial votes with default-refute polarity; majority
  overturns. Inter-classifier agreement is published with the artifact.
- Standing artifact: `.port/audits/GHOSTTY_LIFETIMES.tsv` (lands via plan Task
  16) — every pointer-typed struct field in the
  remaining pointer-heavy tiers classified once (Bun's 11-class taxonomy + GHOSTTY_C),
  consumed verbatim by port agents ("trust it over local guessing"); UNKNOWN ⇒
  `Option<NonNull<T>>` + TODO(port).
- **LIFETIMES prospective prong:** no pointer-heavy tier ports without its
  GHOSTTY_LIFETIMES.tsv rows landing first (2-reviewer consensus). This is item
  1 of the pre-port tier checklist below (plan Task 16 Step 4 + Task 22 Step 1;
  memory `adopt-lifetimes-tsv`).
- Swarm hygiene: one expensive pass (build/Miri) per round with read-only fix agents;
  contiguous-slice sharding; capped fixes per round; push per round; agents never run
  destructive git and commit explicit pathspecs with hooks disabled.

## R12 — Upstream parity tracking

- A standing main-parity workflow (`.port/phase-2/wf-main-parity.js`, lands via
  plan Task 19; ledger `.port/phase-2/MAIN_PARITY.md`) enumerates every
  upstream ghostty commit (since the pinned reference SHA, `20abdb50a`
  (machine-readable: `.port/tools/gates/ORACLE_PIN`, the one file the gates read) — advanced
  from `4749c4e9` by `P3-W10 T3` on 2026-07-31, from `6ad1fe7d8` by the
  Phase-4 resync on 2026-08-17, from `12967b68f` by the Phase-5 resync on
  2026-08-25, from `683d8db64` by the gap-close on 2026-08-26, then from
  `88f57ee66` by the Phase-6 resync on 2026-09-01; see `.port/ORACLE_SYNC.md`)
  touching ported files; each gets a verdict
  ported/missing/partial/not-applicable with file:line evidence and 2-vote adversarial
  verification; missing ones are ported WITH the same commit's test hunks. It
  runs as a standing per-sub-project ritual. The reference checkout at
  `/Users/gael/projects/ghostty` stays read-only: `git fetch` is the one
  sanctioned write (remote refs only), never checkout/pull/reset. The **sole**
  exception is a deliberate pin advance, which requires that no other lane is
  active and follows the checklist in `.port/ORACLE_SYNC.md` ("Advancing the pin"),
  which updates `ORACLE_PIN` in the same commit as this rule's SHA.
  Note `.port/phase-2/MAIN_PARITY.md` was triaged at `53bd14fec` scoped to
  `src/`+`include/`, so it cannot speak for `53bd14fec..6ad1fe7d8`; the Phase-3
  record is `docs/superpowers/plans/2026-07-29-phase3-upstream-resync.md` + `.port/phase-3/`.
- Doctrine: updating the reference checkout does not port anything — "a merge does NOT
  port the fix; only an explicit .rs change does."
- Every escaped defect gets `tests/regression/issue_NNN.rs` with a documented fail-first
  proof, and (for oracle-differential tests) proof the test fails against the
  pre-fix/oracle side.

## R13 — Reference defects are not ported (BINDING, user ruling 2026-09-11)

Fidelity is owed to what the reference is **for**, not to its bugs. Where the reference has
a **defect reachable from real input** — a crash (a null unwrap such as `x.?`, an
`@intCast`/bounds/overflow trap, a `catch unreachable`, an `unreachable` branch), undefined
behaviour, a leak, or a deadlock that malformed, external, or otherwise realistic input can
trigger — the port does **not** reproduce it:

1. **Port the evident intent, handled safely.** Take the error or degraded path the
   reference's own surrounding code implies (a failed response, an error return, a skipped
   element, a logged warning it already has). Do not invent new user-visible behaviour or
   new message text; where the reference offers nothing to reuse, choose the most
   conservative safe handling and say so.
2. **Prove it.** A test drives the triggering input and proves the port does not crash,
   leak, or hang, with a negative control: reinstate the reference's behaviour and confirm
   the test fails.
3. **Mark it at the site.** A `PORT NOTE: reference defect (R13)` stating the reference's
   behaviour, the reachability, the safe handling chosen, and the `UPSTREAM-DEFECTS.md` id.
4. **Record it for upstream.** An entry in `.port/parity-closure/UPSTREAM-DEFECTS.md` with the
   reference location, the consequence in ReleaseSafe and ReleaseFast, who can trigger it, how
   the port handles it, and a proposed upstream fix — so fixes can be proposed to Ghostty.

**The boundary with R7.** R7 still governs **invariant** checks: an `unreachable`, `@intCast`,
or assert on a condition the reference guarantees by construction ports as a checked
assertion, because it never fires and porting it preserves the invariant. R13 applies when the
condition is **not** guaranteed — when input from outside the function's control (a peer
process, a file, the VT stream, the config, the network, the kernel) can violate it. When
unsure, trace where the input comes from before deciding; "the reference also traps" is not
by itself a reason to trap.

**Non-crash reference defects** found along the way — a latent compile error in unused code,
a copy-paste error in a message, dead code with commented callers, an unchecked call whose
error surfaces somewhere misleading, a reference test too weak to fail — are also recorded in
`UPSTREAM-DEFECTS.md` (as `kind: latent` or `kind: test-weakness`) so they are proposable. Those
do not change port behaviour unless R1 says the port must diverge; message text and test
assertions otherwise stay faithful.

Precedent this rule reverses: parity-closure S51-F7 was briefly ruled "port the panic"; the
user overruled it. S209-F1 (tmux layout `@intCast` panicking on tmux-supplied dimensions) was
ported as a panic by task 15 and is re-addressed under this rule.

## Pre-port tier checklist (binding, 2026-07-10)

Per-tier PRE-PORT GATE for the remaining tiers (gtk ~23k LOC, swift-app glue,
inspector widgets, platform-linux/windows). No porting workflow may start for a
tier without ALL of (plan Task 22 Step 1):

1. LIFETIMES.tsv rows for every pointer-typed field in the tier's Zig sources
   (Task 16 process, prospective prong — 2-reviewer consensus).
2. Paired-invariant catalog entries for cross-crate predicates the tier
   introduces (D5).
3. Comptime-parameter inventory for the tier (D4a) with per-instance rendering
   decisions logged.
4. Constants-parity extraction for the tier's Zig files (D3).
5. Design doc + adversary pass (existing doctrine), now REQUIRED to cite
   LIFETIMES rows for any raw-pointer seam.
6. Slice-reinterpret allowlist extension for any new reinterpret sites (D1).
