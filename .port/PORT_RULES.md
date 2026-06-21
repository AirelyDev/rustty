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
feature **`identity-ghostty`** flips every value to the reference's, used **only**
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
