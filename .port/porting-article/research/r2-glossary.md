# r2 dossier: plain-language glossary for internal IDs

Key: `glossary`. Snapshot: HEAD `1d22ba80d` (`phase-8-upstream-resync`), 2026-10-08.
Companion files:
- `data/r2-glossary-ids.csv`: 162 rows (id, category, collision note, plain description, verbatim
  text, source, suggested phrasing, provenance). Every row is DOCUMENTED at the path it cites.
- `r2-outline-plain.md`: BRIEF.md §1 rewritten without internal ids.

## How this was built

- Ids were harvested from BRIEF.md and the eight dossiers with a regex pass (MEASURED:
  `python3 ids.py` over `research/*.md`; 276 distinct tokens incl. false positives such as
  line numbers `L118`). Each real id was traced to its definition.
- Definitions come from `.port/PORT_RULES.md` (R1-R14, DG2/DG4/DG5, pre-port checklist),
  `.port/phase-2/BUN_PORT_LEARNINGS.md` §4 (detector spec), `.port/tools/CI.md` (gate tables),
  `.port/tools/ci.sh` (gate registry: **101 entries**, MEASURED by `reg.py`, which parses every
  `"ID|HARD|PROGRESS|runner"` line and the comment block above it), `.port/phase-0/plan/
  DEFINITION_OF_PARITY.md` (G1-G9, G3b), ledger headers in `.port/parity-closure/`,
  `.port/audits/GHOSTTY_LIFETIMES.tsv`, `.port/ORACLE_SYNC.md`, `AGENTS.md`, and the author's
  agent memory under `~/.claude/projects/-Users-gael-projects-rustty/memory/` (design decisions
  D1/D3/D6, aliasing rule, user rulings). **Memory files are not in the repo**; anything quoted
  from them is the agent's record of a decision and should be confirmed by the author before
  publication.

## ID collisions (the main hazard)

The same short id means different things in different files. In the article, never print any of
them; when reading the dossiers, resolve by context:

| Id family | Meanings | How to tell |
|---|---|---|
| **D1..D9** | (a) Bun-derived regression **detectors** (ci.sh, 07-12); (b) **design decisions** from the 06-25/26 design panels (D1 font layering, D2 cbindgen, D3 container_of, D6 libxev); (c) `DG1..DG5` = decision gates in the 07-10 realignment plan | "detector", ci.sh, CI.md ⇒ (a). "layering", "font", "xev", "seam" ⇒ (b). BRIEF §1 item 4 "D1 (vt→font)" is (b); item 6 "D1-D6, D8, D9" is (a). |
| **D10 / D11** | stb resize parity gate / closed-enum C-ABI gate. Prose in P3-W0 called the enum gate "D10" before it was renumbered. | ci.sh:1211 comment. |
| **N###** | (a) Port-findings ledger entries (NEW-FINDINGS.md, N1..N530); (b) BRIEF/website chart ids N1..N51 | "N11 open", "N437", "fixed N500" ⇒ (a); a table row with a chart type ⇒ (b). |
| **A#** | (a) ABI gates A3, A4, A5; (b) BRIEF chart ids A1..A8; (c) apprt chunks A0/A1 | "sret", "header" ⇒ (a). |
| **V##** | (a) chart ids in bugs.md (V1..V12) and Bun's widgets (V1 replay, V4 CI race); (b) VERIFICATION_DEBT items (V17, V27-V29) | measurement context ⇒ (b). |
| **R#** | (a) PORT_RULES R1..R14; (b) "DECISIONS.md R2" (Phase 7 decision cited by G-UDDOCS); (c) "R2 perf parity" in R10 = Goal 2; (d) perf rounds r1..r5; (e) R13-GF2/R13-L2 = task ids; (f) "R-a/R-d" renderer chunks | Uppercase R + rule topic ⇒ (a). |
| **M#** | LIFETIMES audit severity (M1, M2); VALIDATION_REPORT item M6; Apple M4 Pro chip; perf "M4-floor" optimisation step | context. |
| **E#** | usage eras E1..E3 vs embedded-apprt chunks E0..E10 | date ranges ⇒ eras. |
| **W##** | ISO week (W25..W40) vs wave inside a phase (P7-W13) vs WS1..WS20 roadmap workstreams vs macOS windowing chunks W0..W5 | prefix. |
| **S##** | parity-audit rows S1-F1..S##-F# vs parity-tail tasks S-4, S-15 vs S7 allocator prototype | dash placement. |
| **L###** | line numbers in dossiers vs lifetimes-audit low findings L7, L18 | context. |
| **-SELF** | suffix: the gate's own self-test (injects a violation and requires RED). Reconciles BRIEF §4 (MEASURED by `reg.py`): the registry has **101** distinct ids = **77** base checks + **24** `-SELF` self-tests; 75 HARD / 26 PROGRESS. The "86 ids (64 + 22 -SELF)" figure is the same registry minus the 11 detectors (D1-D6, D8-D11r) and 4 ABI ids (A3, A4, A5, A5-MASK-SELF), with `G1-SELF` apparently counted among the 64. 4 self-tests have no same-named base gate: G-MIRIRUN-SELF, G-SAN-SELF (covers all sanitizer gates), G-ORACLE-SH-SELF, A5-MASK-SELF. Article: "101 registered checks, 24 of which test the checkers themselves". | suffix |

## Glossary

Notation: ⚠ marks a collision note. Verbatim texts are exact excerpts (ellipses mark cuts).

### Rules and locked decisions

| ID | Plain description | Verbatim (source) | Article phrasing |
|---|---|---|---|
| Parity Constitution | The original kickoff prompt that set the port's goals; the numbered rules amend it. | "These are the binding decisions agreed during alignment. They AMEND the Parity Constitution in the kickoff prompt. Where they conflict, these win." (.port/PORT_RULES.md (header)) | the original project brief |
| R1 | Full fidelity: every line, comment, test and assertion of the reference is ported; Rust-only glue may be added but nothing may be lost. | "The port reproduces the reference with **zero loss of content** ... **Every comment is preserved** ... Comments are NEVER dropped or summarized away. ... **No-loss, not no-addition.**" (.port/PORT_RULES.md R1) | the full-fidelity rule: nothing lost, every comment and test carried over |
| R2 ⚠ Also 'DECISIONS.md R2' (a Phase 7 decision, see G-UDDOCS) and 'R2 perf parity' inside R10 (= Goal 2). Different things. | Product identity: everything shipped is named rustty; a build feature flips names back to Ghostty's only so the differential harness can compare output byte for byte. | "All **published** artifacts carry the `rustty` identity ... A build feature **`identity-oracle`** flips every value to the reference's, used **only** by the parity harness so differential output matches the reference byte-for-byte" (.port/PORT_RULES.md R2) | the rename rule, with a build switch that restores Ghostty's names for side-by-side testing |
| R3 | Self-contained: the same C libraries are vendored and built from source; Ghostty is never a build dependency, only an external oracle; no pure-Rust substitutes. | "rustty copies every needed C source into its own tree and builds it standalone ... The reference is used ONLY as an external oracle ... No pure-Rust substitutions during parity phases" (.port/PORT_RULES.md R3) | same C libraries, vendored; Ghostty is only consulted as a reference |
| R4 | The crate graph must be acyclic; cycle-breaking refactors may only move code, never change logic, comments or tests. | "The crate graph MUST be acyclic (Cargo requirement) and is enforced by `.port/tools/cycle_check.py` run as a gate. Cycle-breaking refactors (R4 breaks) are behavior-preserving structural changes ONLY" (.port/PORT_RULES.md R4) | the acyclic-crates rule |
| R5 | Gates are never weakened to pass; a gate that cannot be greened honestly stops the work and is reported. | "Never weaken, skip, xfail, or loosen a test or gate. A red gate that cannot be made green honestly → STOP and surface a precise blocker. Parity is the invariant; performance is the gradient" (.port/PORT_RULES.md R5) | the never-weaken-a-gate rule |
| R6 | Memory-safety tooling per crate: Miri for pure Rust, AddressSanitizer for Rust/C FFI, LSan and Valgrind for FFI leaks (amended 10-02: LSan also runs on macOS). | "Pure-Rust crates → Miri ... the PRIMARY gate ... Rust↔C FFI crates → AddressSanitizer ... FFI leak detection → Linux CI (LSan/valgrind)" (.port/PORT_RULES.md R6) | the right-tool-per-crate memory-safety rule |
| R7 | Replacement-primitive fidelity: where a Zig builtin maps to a Rust primitive with different edge behaviour (casts, wrapping math, slices, zip, asserts), the choice is made explicitly per site. | "When a Zig builtin/idiom is replaced by a Rust primitive with different edge semantics, the divergence is decided explicitly per site, never inherited silently" (.port/PORT_RULES.md R7 (ratified 2026-07-10)) | the rule on Zig-to-Rust primitive semantics (casts, overflow, zip, asserts) |
| R8 | Compile-time (comptime) parameters stay compile-time in Rust (generics, const generics, macros); build switches become cfg, never folded to one value; derived constants keep their expressions. | "A Zig `comptime` parameter maps to a Rust const/generic/macro ... Zig build-mode/option switches ... port as `cfg`, NEVER constant-folded to one profile's value." (.port/PORT_RULES.md R8) | the keep-compile-time-things-compile-time rule |
| R9 | Unsafe accounting: a committed census script, a SAFETY comment on every unsafe block, an unsound-safe-wrapper ledger, and a 'fixed' bar that debug asserts do not meet. | ""Fixed" bar: safe code cannot cause UB in a release build; debug assertions do not qualify; wrappers that hide the invariant do not qualify." (.port/PORT_RULES.md R9) | the unsafe-accounting rule |
| R10 | Greppable markers: TODO(port), PERF(port) and PORT NOTE, plus a PORT STATUS trailer on large files. | "Machine-greppable markers only: `TODO(port): <reason>` ... `PERF(port): <zig idiom>` ... `PORT NOTE: <reshape reason>`" (.port/PORT_RULES.md R10) | the porting-marker convention |
| R11 | Adversarial verification: implementer, verifier and fixer are separate agents; bulk classifications need file:line evidence and default-refute votes; the pointer-lifetime ledger must exist before a pointer-heavy tier is ported. | "the verifier works a concrete violation checklist keyed to PORT_RULES and defaults to not-ok-if-uncertain ... no pointer-heavy tier ports without its GHOSTTY_LIFETIMES.tsv rows landing first (2-reviewer consensus)" (.port/PORT_RULES.md R11) | the separate-implementer-and-reviewer rule |
| R12 | Upstream parity tracking: every upstream Ghostty commit since the pin gets a verdict; moving the reference forward ports nothing by itself. | "Doctrine: updating the reference checkout does not port anything — "a merge does NOT port the fix; only an explicit .rs change does."" (.port/PORT_RULES.md R12) | the upstream-tracking rule |
| R13 | Reference defects are not ported: a Ghostty crash, UB, leak or deadlock reachable from real input is handled safely, tested with a negative control, marked at the site and recorded for upstream. Extended 09-23 to non-crash bugs that break evident intent. | "Fidelity is owed to what the reference is **for**, not to its bugs. ... "the reference also traps" is not by itself a reason to trap." (.port/PORT_RULES.md R13 (user ruling 2026-09-11, extended 2026-09-23)) | the never-port-a-bug rule |
| R14 | No silent skips: a test that cannot run must say so (ignored, cfg-scoped, or fail under a strict CI variable), never return early and report ok. | "A Rust test that returns early reports `ok`, which cannot be told apart from a pass." (.port/PORT_RULES.md R14 (2026-09-23)) | the no-silent-skips rule |
| Pre-port tier checklist | Six things that must exist before a remaining tier (GTK, Swift glue, inspector, platform code) may be ported: lifetime rows, paired invariants, comptime inventory, constants extraction, design doc with adversary pass, slice allowlist. | "No porting workflow may start for a tier without ALL of (plan Task 22 Step 1): 1. LIFETIMES.tsv rows for every pointer-typed field ..." (.port/PORT_RULES.md (Pre-port tier checklist)) | a pre-port checklist |
| RUSTTY_ENUM_MAX_VALUE sentinel | Every C enum in the vt headers ends with a _MAX_VALUE sentinel so C compilers size it as int. | "All C enums in `include/rustty/vt/` must have a `_MAX_VALUE = RUSTTY_ENUM_MAX_VALUE` sentinel as the last entry to force int enum sizing (pre-C23 portability)." (AGENTS.md:26) | an int-sized-enum sentinel rule |

### Design decisions (D-ids that are not detectors)

| ID | Plain description | Verbatim (source) | Article phrasing |
|---|---|---|---|
| DG1 ⚠ DG = decision gate in the 07-10 realignment plan; unrelated to detector or design-decision D-ids. | The decision to ratify the six Bun-derived rules (R7-R12) as binding. | "Ratify PROPOSED R7–R12 into PORT_RULES.md ... Ratify all six; they only tighten" (docs/superpowers/plans/2026-07-10-port-realignment-and-validation-hardening.md:31) | the decision to adopt the six Bun-inspired rules |
| DG2 | Release builds keep integer-overflow checks on, matching Ghostty's safety-checked mode; a separate release-fast profile exists only for benchmarks. | "`[profile.release] overflow-checks = true`: rustty release keeps trap-parity with ReleaseSafe on plain arithmetic" (.port/PORT_RULES.md DG2/DG4) | overflow checks stay on in release |
| DG4 | A release-safe profile turns debug assertions back on so the differential harness runs with Ghostty's ReleaseSafe assert semantics. | "The `release-safe` profile (`inherits = "release"`, `debug-assertions = true` ...) restores the oracle's ReleaseSafe assert semantics" (.port/PORT_RULES.md DG2/DG4) | a release-with-asserts profile |
| DG5 | A Rust panic that reaches the C ABI aborts the process, as a Zig panic does; no unwinding, no error codes. | "**Policy: a Rust panic reaching a C-ABI boundary ABORTS the process.**" (.port/PORT_RULES.md DG5) | panics at the C boundary abort, as in Ghostty |
| D1 (decision) ⚠ Collides with detector D-ids. | Font/terminal layering: rustty-font is a pure parsing crate below rustty-vt; vt depends on font, never the reverse; the shaper sits in a new crate above both. | "keep rustty-font a **pure parsing crate BELOW rustty-vt**; rustty-vt depends on rustty-font ... `rustty-font` must have **NO rustty-vt edge**" (~/.claude/projects/-Users-gael-projects-rustty/memory/apc-glyf-font-layering-d1.md (design panel 2026-06-25)) | the font-below-terminal layering decision |
| D2 (decision) ⚠ Collides with detector D-ids. | The vt C API header is generated with cbindgen (only mentioned in passing). | "the libghostty milestone, vt-c D2 cbindgen" (~/.claude/projects/-Users-gael-projects-rustty/memory/integration-tier-decisions.md) | (omit) |
| D3 (decision) ⚠ Collides with detector D-ids. | The vt C API recovers its heap wrapper from inside a callback using offset_of! (the Rust form of Zig's @fieldParentPtr / C's container_of), without changing rustty-vt. | "**container_of recovery** (the faithful `@fieldParentPtr` port) ... never form an overlapping `&mut TerminalWrapper` while the `&mut Handler` is live" (~/.claude/projects/-Users-gael-projects-rustty/memory/vtc-effects-trampoline-seam-d3.md) | the container_of callback decision |
| D6 (decision) ⚠ Collides with detector D-ids. | libxev (Zig event loop) is ported as a Rust crate, not bound as C, and deferred until a live shell needed it. | "libxev is **Zig source, not a C library** → port faithfully as a new low crate `rustty-xev` ... DEFER" (~/.claude/projects/-Users-gael-projects-rustty/memory/integration-tier-decisions.md) | the event-loop port decision |
| RendererConfig accessor trait | The renderer reads configuration through a trait it defines and rustty-config implements, so the renderer never depends on the config crate. | "renderer-defined RendererConfig accessor trait impl'd IN rustty-config (renderer never names a config type; no config edge)" (~/.claude/projects/-Users-gael-projects-rustty/memory/MEMORY.md index) | a trait seam that keeps the renderer free of the config crate |
| Command-in-termio | Command.zig ports into the terminal-I/O crate, not core, because putting it in core created a cycle. | "**Command.zig → `rustty-termio::command`, NOT `rustty-core`.** The old PORT_PLAN ... put it in core, which CYCLES" (~/.claude/projects/-Users-gael-projects-rustty/memory/integration-tier-decisions.md) | - |
| macOS-first / ghostty_* aliases | Two locked product forks (07-08): build macOS first; keep the Swift app byte-identical by exporting ghostty_* alias symbols over the rustty_* C ABI. | "**C-ABI symbol identity = `ghostty_*` aliases over a canonical `rustty_*` ABI** — the reused Swift app ... stays byte-identical to upstream" (~/.claude/projects/-Users-gael-projects-rustty/memory/rustty-completion-plan.md) | reuse the Swift app unchanged through alias symbols |

### Regression detectors

| ID | Plain description | Verbatim (source) | Article phrasing |
|---|---|---|---|
| D1 (detector) ⚠ Collides with design decisions D1/D2/D3/D6 (2026-06-25/26) - see 'decision' rows. | Slice-reinterpret audit: every cast of a byte slice to a wider type is listed with a truncate-or-error decision and a test for odd or misaligned lengths. | "every `bytemuck::cast_slice`/`align_to`/widening `from_raw_parts` hit is allowlisted in `slice-reinterpret.tsv` with a decision + guard test; unlisted/stale/unguarded ⇒ RED" (.port/tools/CI.md D1; .port/phase-2/BUN_PORT_LEARNINGS.md §4) | a check on byte-slice reinterpretation (Bun regression class 1) |
| D2 (detector) ⚠ Collides with design decisions D1/D2/D3/D6 (2026-06-25/26) - see 'decision' rows. | Capacity-saturation tests: every fixed array paired with a counter has a test that fills it to the ceiling. | "every fixed `[T; N]` + write-then-increment counter pair is inventoried and names a `_capacity_saturation` test that drives it to the ceiling" (.port/tools/CI.md D2) | fill-to-capacity tests for fixed buffers |
| D3 (detector) ⚠ Collides with design decisions D1/D2/D3/D6 (2026-06-25/26) - see 'decision' rows. | Constants keep their defining expression rather than a folded literal; no placeholder markers survive. | "derived Zig constants port as the *same expression*, not a folded literal; port markers clean" (.port/tools/CI.md D3) | constants ported as expressions |
| D4 (detector) ⚠ Collides with design decisions D1/D2/D3/D6 (2026-06-25/26) - see 'decision' rows. | Comptime-phase preservation plus byte-exact golden tests for every escape sequence the terminal emits. | "byte-exact golden VT bytes + OSC/ST-integrity tests (`d4_golden_vt_bytes.rs`)" (.port/tools/CI.md D4) | golden byte tests on emitted escape sequences |
| D5 (detector) ⚠ Collides with design decisions D1/D2/D3/D6 (2026-06-25/26) - see 'decision' rows. | A catalog of invariants that two crates must agree on (producer assert, consumer degrade, one seam test). | "the cross-site producer/consumer invariant catalog (`VT_INVARIANTS.md`) is complete and every anchor + seam test present" (.port/tools/CI.md D5) | a catalog of cross-crate invariants |
| D6 (detector) ⚠ Collides with design decisions D1/D2/D3/D6 (2026-06-25/26) - see 'decision' rows. | Sanitizer canary: a deliberate leak and overflow must make the sanitizer lane fail, proving it can still see the port's heap. | "an ASan-visibility **canary** proves the sanitizer lane can still SEE the port's heap, + cfg-mapping/suppression audit" (.port/tools/CI.md D6) | a canary proving the sanitizers still see allocations |
| D7 (detector) ⚠ Collides with design decisions D1/D2/D3/D6 (2026-06-25/26) - see 'decision' rows. | Planned unsafe census and unsound-safe ledger; not built as a detector because rule R9 and the census script cover it. | "D7 is intentionally absent (no detector was carved for it)." (.port/tools/CI.md) | (omit; the unsafe census covers it) |
| D8 (detector) ⚠ Collides with design decisions D1/D2/D3/D6 (2026-06-25/26) - see 'decision' rows. | Clone-injection audit: new .clone()/.to_vec() calls listed for review against the Zig side's copy behaviour (advisory). | "advisory review list of unexplained copies; confirmed by the G8 bench lane at tier finales (always exit 0)" (.port/tools/CI.md D8) | a review list of new copies |
| D9 (detector) ⚠ Collides with design decisions D1/D2/D3/D6 (2026-06-25/26) - see 'decision' rows. | Stale-comment check: where a primitive with different failure behaviour replaced a Zig builtin, the adjacent ported comment must describe the Rust behaviour (advisory). | "candidate stale/semantic-mismatch comments printed for review; sealed via `stale-comments-D9.tsv` (always exit 0)" (.port/tools/CI.md D9) | a stale-comment check |
| D10 ⚠ Was called 'D10' in P3-W0 prose for the enum gate; renamed D11. | Glyph resampler parity: the vendored stb_image_resize output must match Ghostty's own build of the same header, within a stated tolerance, at all four Zig optimize modes. | "the vendored stb_image_resize glyph resampler vs ghostty's OWN build of the byte-identical header, at all four Zig optimize modes, plus a negative control" (.port/tools/ci.sh:1204-1209) | a glyph-resize parity check |
| D11 | No closed C enum may be received by value across the C ABI, because an out-of-range value from C is undefined behaviour in Rust before any match can check it. | "no CLOSED #[repr(C)] enum may be received BY VALUE across the C ABI. An out-of-range tag is materialized by the callee's prologue, before any `match` can narrow it" (.port/tools/ci.sh:1211-1230) | a C-ABI enum safety check |
| D11r | Runtime half of D11: a real C program passes the out-of-range sentinel value and the port must return a defined result. | "D11r compiles a real C program against the published header + shipped dylib, passes the `…_MAX_VALUE` enumerator the header itself declares, and asserts a DEFINED result comes back" (.port/tools/ci.sh:1232-1241) | the runtime test for that enum check |

### Gates

| ID | Plain description | Verbatim (source) | Article phrasing |
|---|---|---|---|
| G1 | Every Zig test block in the frozen Phase 0 inventory (3,234) has a same-named Rust test. | "**3234** test blocks, count == contract, names match" (.port/tools/CI.md; .port/phase-0/plan/DEFINITION_OF_PARITY.md §G1) | the test-count contract |
| G1b | Per module, Rust test count >= Zig test count, read live from the reference checkout (covers the dependency ports too). | "per mapped module: rust `#[test]` `>=` zig `test{}` minus documented apc-deferred" (.port/tools/CI.md) | a per-module test-count check |
| G2 | Assertion-for-assertion: no ported test has fewer assertions than its Zig original (floor 11,975). | "assertion-for-assertion, per-test `rust >= ref` (floor **11975**)" (.port/tools/CI.md) | an assertion-count check |
| G3 | Comment-line counts per file must not drop after the name rename. | "per-file comment delta `>= 0` after identity-rename (census **42069**)" (.port/tools/CI.md) | a comment-count check |
| G3b | Content-aware comment fidelity: every distinctive reference comment (5+ words) must have a fuzzy match in the ported crate. | "every *distinctive* reference comment (a phrase/sentence — not a separator or ASCII diagram) must have a fuzzy match somewhere in the ported crate's comments" (.port/phase-0/plan/DEFINITION_OF_PARITY.md §G3b) | the comment-fidelity check |
| G4 | Byte-exact differential: the same C driver runs fixtures through libghostty-vt and librustty-vt; any output difference fails. | "runs the real byte-exact differential — leg A: every `harness/fixtures/*.vt` through `vt_driver_ref` vs `vt_driver_port`" (.port/tools/CI.md) | the byte-for-byte differential test against Ghostty |
| G4-uni | Unicode property differential against the reference. | (.port/tools/ci.sh:535) | a Unicode-table differential |
| G5 | Differential fuzz smoke run over the VT parser. | "if `cargo fuzz` + the `vt_diff` target ... exist it runs a short smoke budget and fails on any new artifact" (.port/tools/CI.md) | differential fuzzing |
| G6 | Memory safety lane: Miri (Stacked Borrows) for pure-Rust crates, ASan for FFI; any UB or leak fails. | "for every *claimed-done* crate it runs the right tool per R6 — Miri ... for pure-Rust crates, ASan ... for FFI crates" (.port/tools/CI.md) | the Miri lane |
| G6b | Leak check for compressed scrollback pages, which Miri cannot reach. | "PageList compressed-page encodings are not leaked" (.port/tools/ci.sh:1131) | - |
| G6c | A second Miri lane with debug assertions off, so heavy page-compression tests finish. | "the `-C debug-assertions=off` vt Miri lane ... ADDITIVE to G6, never a replacement" (.port/tools/ci.sh:1147) | a Miri lane without integrity asserts |
| G6t | Miri with the Tree Borrows aliasing model over the pointer-heavy code. | "Miri Tree Borrows over the pointer-heavy modules and crates" (.port/tools/ci.sh:1155) | the Tree Borrows lane |
| G7 | The Cargo crate graph is acyclic. | "crate graph acyclic (R4)" (.port/tools/CI.md) | the no-cycles check |
| G8 | Benchmarks, memory, binary size and startup must not regress past tolerance against a baseline from Ghostty's build. | "every perf/mem/size/startup metric `<= baseline*(1+tol)`" (.port/tools/CI.md) | the performance regression fence |
| G8b | The full performance fence run: builds Rust and Ghostty counterparts at the pin and compares (Fence 1) plus a Rust-vs-own-history fence (Fence 2). | "Fence 1 (G8b's oracle half) compares against Ghostty's bench built at THE ORACLE PIN" (.port/tools/perf/README.md:1-5) | the full benchmark fence |
| G9 | Every config key, keybind, action and terminfo entry is covered and tested; no ghostty_* names left. | "every config/keybind/action/terminfo id covered + test-referenced; zero residual `ghostty_*` (R2)" (.port/tools/CI.md) | the feature-surface coverage check |
| A3 ⚠ Collides with BRIEF viz id A3 (review replay). | The embedding header is exactly Ghostty's ghostty.h with names renamed, nothing added or dropped. | "Asserts include/rustty-embed.h is EXACTLY ghostty's upstream include/ghostty.h ... with every ghostty_* token renamed to rustty_*" (.port/tools/ci.sh:1243-1250) | the header-identity check |
| A4 ⚠ Collides with BRIEF viz id A4 (CI swimlanes). | Struct-size check for the embedding header: every by-value return type must have the same size in Rust and C (a mismatch crashed the menu shortcuts on arm64). | "HARD-requires every by-value-return type (the AAPCS64 sret crash class) to carry a Rust `@embed-abi` size-freeze that matches. Built after rustty_config_trigger shipped a 20-byte TriggerC vs the header's 12" (.port/tools/ci.sh:1252-1258) | the struct-size check for by-value C returns |
| A5 ⚠ Collides with BRIEF viz id A5 (crate replay). | Struct layout (size and field offsets) check for the libvt C headers. | "Cross-checks every cbindgen/curated vt struct's sizeof + field OFFSETS against the Rust `#[repr(C)]` layout" (.port/tools/ci.sh:1260-1266) | the C struct-layout check |
| -SELF suffix | A gate's own self-test: it injects a violation and requires the gate to fail, so the gate cannot pass vacuously. | "each fail-closed detector ships a `--self-test` proving it flips RED on an injected violation" (.port/tools/CI.md) | self-testing gates |
| G-LINT | Runs the non-Rust linters (shellcheck, prettier, typos, swiftlint, alejandra, pinact). | (.port/tools/ci.sh (registry entry and its comment)) | - |
| G-MIRIRUN-SELF | Self-test of the Miri runner's result classifier. | (.port/tools/ci.sh (registry entry and its comment)) | - |
| G-CCONSUMER | Builds C consumers via CMake and pkg-config (first run found 3 real packaging defects). | (.port/tools/ci.sh (registry entry and its comment)) | - |
| G-EXAMPLES | Builds and runs every example program. | (.port/tools/ci.sh (registry entry and its comment)) | - |
| G-DIST | Builds the source tarball and checks its shape. | (.port/tools/ci.sh (registry entry and its comment)) | - |
| G-NIX | nix flake check in a hermetic sandbox (BLOCKED on macOS). | (.port/tools/ci.sh (registry entry and its comment)) | - |
| G-XTARGET | Cross-target builds of the vt C library. | (.port/tools/ci.sh (registry entry and its comment)) | - |
| G-I18N | Checks every translation catalogue against the template. | (.port/tools/ci.sh (registry entry and its comment)) | - |
| G-BLUEPRINT | Compiles and format-checks the GTK Blueprint templates. | (.port/tools/ci.sh (registry entry and its comment)) | - |
| G-VALGRIND | Valgrind memcheck over every crate's test binaries (Linux). | (.port/tools/ci.sh (registry entry and its comment)) | - |
| G-SAN-SELF | Proves each sanitizer catches a synthetic overflow, leak and race. | (.port/tools/ci.sh (registry entry and its comment)) | - |
| G-ASAN | AddressSanitizer over every workspace crate; also re-runs every Miri-excepted test. | (.port/tools/ci.sh (registry entry and its comment)) | - |
| G-LSAN | LeakSanitizer (Linux and, via rustc's ASan runtime, macOS). | (.port/tools/ci.sh (registry entry and its comment)) | - |
| G-MSAN | MemorySanitizer (Linux). | (.port/tools/ci.sh (registry entry and its comment)) | - |
| G-HWASAN | Hardware-tagged ASan (aarch64 Linux only). | (.port/tools/ci.sh (registry entry and its comment)) | - |
| G-TSAN | ThreadSanitizer over the threaded crates. | (.port/tools/ci.sh (registry entry and its comment)) | - |
| G-LSAN-GTK | LeakSanitizer over a live GTK window under headless sway. | (.port/tools/ci.sh (registry entry and its comment)) | - |
| G-XCF | Checks the Apple XCFramework bundle shape and exports. | (.port/tools/ci.sh (registry entry and its comment)) | - |
| G-FEATMATRIX | Builds crates alone and with optional features, since workspace feature unification hides missing features. | (.port/tools/ci.sh (registry entry and its comment)) | - |
| G-FONTBACKEND | Builds and tests every font backend that builds on macOS. | (.port/tools/ci.sh (registry entry and its comment)) | - |
| G-EOL | Byte fidelity of fixtures and corpora (catches git line-ending normalisation). | (.port/tools/ci.sh (registry entry and its comment)) | - |
| G-ORACLE-SH-SELF | Proves gates cannot be pointed at a different reference tree through environment variables. | (.port/tools/ci.sh (registry entry and its comment)) | - |
| G-XTASK | Runs the xtask build subcommands nothing else ran. | (.port/tools/ci.sh (registry entry and its comment)) | - |
| G-DOXYGEN | Generates the C API docs with no new warnings. | (.port/tools/ci.sh (registry entry and its comment)) | - |
| G-REL | Runs tests in release mode. | (.port/tools/ci.sh (registry entry and its comment)) | - |
| G-GTKINIT | Static check: one gtk4::init() per test binary. | (.port/tools/ci.sh (registry entry and its comment)) | - |
| G-SKIP | No silent skips in tests (enforces R14). | (.port/tools/ci.sh (registry entry and its comment)) | - |
| G-P2 | Phase 2 closure: every open item has an owner in an open workstream. | (.port/tools/ci.sh (registry entry and its comment)) | - |
| G-FILES | Every reference file has a Rust counterpart or an argued non-port. | (.port/tools/ci.sh (registry entry and its comment)) | - |
| G-SYMS | Every reference pub fn has a Rust counterpart symbol. | (.port/tools/ci.sh (registry entry and its comment)) | - |
| G-TESTS | Tests added upstream after the frozen inventory are also ported (reads the live reference). | (.port/tools/ci.sh (registry entry and its comment)) | - |
| G-CENSUS | Writes down the gap between the frozen Phase 0 inventory and the live reference, so it cannot widen unnoticed. | (.port/tools/ci.sh (registry entry and its comment)) | - |
| G-ONIG | Builds and tests the oniguruma-regex feature lane, which no other lane compiled. | (.port/tools/ci.sh (registry entry and its comment)) | - |
| G-VTCLIB | The vt C library keeps the reference's no-PNG-decoder default. | (.port/tools/ci.sh (registry entry and its comment)) | - |
| G-ARTIFACT | Only the vt C crate may set the process-wide 'library' artifact flag. | (.port/tools/ci.sh (registry entry and its comment)) | - |
| G-VTALLOC | Bans two slow allocator Vec methods in the vt crates. | (.port/tools/ci.sh (registry entry and its comment)) | - |
| G-GTKFEAT | Builds and lints the GTK, Wayland and X11 feature code (Linux). | (.port/tools/ci.sh (registry entry and its comment)) | - |
| G-XEVREACH | The wasm artifacts must not depend on the event loop. | (.port/tools/ci.sh (registry entry and its comment)) | - |
| G-BENCH | Benchmark crate unit tests plus a test-count floor. | (.port/tools/ci.sh (registry entry and its comment)) | - |
| G-WINCHK | Type-checks the OS layer, vt and vt C API for Windows MSVC. | (.port/tools/ci.sh (registry entry and its comment)) | - |
| G-XEV-IOS | Type-checks the event loop for iOS. | (.port/tools/ci.sh (registry entry and its comment)) | - |
| G-WINPROTO-DEFAULT | Pins the default GUI dependency graph per target (Linux, macOS, FreeBSD). | (.port/tools/ci.sh (registry entry and its comment)) | - |
| G-PARITY | Every row of the 497-row parity audit is closed or waived with a reason. | (.port/tools/ci.sh (registry entry and its comment)) | - |
| G-UDDOCS | Every Ghostty-defect entry has an upstream status and a matching line in the user-facing bug-fix log. | (.port/tools/ci.sh (registry entry and its comment)) | - |
| G-LOGFILE | Per reference file, every log call site is ported or accounted for. | (.port/tools/ci.sh (registry entry and its comment)) | - |
| G-REFGEOM | Re-measures the reference's set-item struct geometry on three targets. | (.port/tools/ci.sh (registry entry and its comment)) | - |
| G-ESCTEST | Runs the esctest2 conformance suite against both libraries; outcomes must match. | (.port/tools/ci.sh (registry entry and its comment)) | - |
| G-LZ4 | The LZ4 encoder's byte stream matches the reference's, not just round-trips. | (.port/tools/ci.sh (registry entry and its comment)) | - |
| G-THEMES | SHA-256 manifest of the vendored theme corpus. | (.port/tools/ci.sh (registry entry and its comment)) | - |
| G-VTFEAT | The vt feature-trimming switches remove exactly the reference's exports. | (.port/tools/ci.sh (registry entry and its comment)) | - |
| G-ABISCHEMA | The shipped C ABI type manifest validates against its JSON schema. | (.port/tools/ci.sh (registry entry and its comment)) | - |
| G-KAITAI | The snapshot wire format's Kaitai spec parses every golden fixture. | (.port/tools/ci.sh (registry entry and its comment)) | - |
| G-XEVSCOPE ⚠ Old name of G-XEVREACH. | Renamed; see G-XEVREACH. | (.port/tools/ci.sh:814) | - |

### Ledgers and ledger ids

| ID | Plain description | Verbatim (source) | Article phrasing |
|---|---|---|---|
| UD### (e.g. UD131, UD377) | An entry in the Ghostty-defect ledger: a bug in Ghostty or a pinned dependency, with consequence, reachability, the port's safe handling, a test and a proposed fix. 205 entries, ids sparse up to UD378. | "Defects **in the Ghostty reference itself** ..., recorded so fixes can be proposed upstream." (.port/parity-closure/UPSTREAM-DEFECTS.md) | Ghostty defect #n / 'one of the 205 recorded Ghostty defects' |
| N### (e.g. N11, N437, N500) ⚠ Collides with BRIEF chart ids N1-N51. | An entry in the port-findings ledger: a defect in Rustty's own port found outside the parity audit (335 entries, N1..N530, gaps from block reservation). | "This file records defects found while closing a `LEDGER.tsv` row that the original parity audit did not catch." (.port/parity-closure/NEW-FINDINGS.md) | a port finding (a bug in the Rust port itself) |
| N11 | The one still-open port finding: about 35 restored log lines print Rust's derived Debug format where Zig prints its own aggregate format (log text only). | "restored `log` sites render Rust's derived formatter where Zig renders its builtin aggregate formatter, crate-wide across ~35 sites ... severity: low (message-text divergence in a diagnostic" (.port/parity-closure/NEW-FINDINGS.md:498) | one open, low-severity log-text difference |
| S##-F# (e.g. S1-F1, S51-F7) ⚠ Collides with 'S-##' parity-tail task ids and 'S7' allocator prototype. | A row of the 497-row parity audit ledger: audit section number, finding number. | "Ledger (`LEDGER.tsv`), 497 audited findings: 404 CLOSED, 93 WAIVED, 0 OPEN" (.port/parity-closure/LEDGER.tsv; README.md:15) | a parity-audit finding |
| U# / P4-U# (pre-rule bug notes) | The superseded 'faithfully ported, report upstream' list from before the never-port-a-bug rule; folded into the defect ledger. | "Bugs that are FAITHFULLY PORTED — report upstream, do not fix locally ... SUPERSEDED — 2026-09-11" (.port/phase-3/UPSTREAM_BUGS.md) | the earlier list of Ghostty bugs copied faithfully |
| LIFETIMES.tsv | Pointer-field ownership ledger: every pointer field in the pointer-heavy Zig code classified (owned, borrowed, intrusive, FFI...) with the Rust type agents must use. 216 rows at creation, 314 now. | "Port agents consume this table VERBATIM: "trust it over local guessing". ... UNKNOWN => Option<NonNull<T>> + TODO(port)." (.port/audits/GHOSTTY_LIFETIMES.tsv (header)) | the pointer-lifetime ledger |
| M1, M2, L7, L18 (lifetimes audit) ⚠ M4 elsewhere = Apple chip; M6 = VALIDATION_REPORT item; L### elsewhere = line numbers. | Medium (M) and low (L) findings from the 07-10 lifetimes audit; M1+M2 were latent aliasing UB in the event loop's MPSC queue. | "20 findings: 0 high, 2 medium, 18 low" (.port/audits/LIFETIMES_AUDIT_2026-07-10.md) | two medium findings in the lifetime audit |
| COMMENT-NOT-CARRIED.tsv | The 79 reference comments deliberately not carried (each excused by a mechanical rule, e.g. jokes with no fact). | "A row here excuses ONE oracle comment line, and only because the row proves, mechanically, one of two things" (.port/parity-closure/COMMENT-NOT-CARRIED.tsv) | the list of comments not carried |
| MIRI_EXCEPTIONS.md | Tests too slow for Miri, each allowed only under four criteria including that every unsafe path they touch is covered by a lighter Miri test. | "**No uncovered unsafe path** *(the teeth)* — every PageList method the test exercises is also exercised by ≥ 1 test that PASSES under Miri" (.port/phase-2/MIRI_EXCEPTIONS.md:17-29) | the Miri exception list |
| V## (e.g. V17, V27-V29) ⚠ Collides with BRIEF/bugs chart ids V1-V12 and Bun-article widget ids V1/V4. | An item in the verification-debt ledger (long runs and re-measurements owed). | (.port/phase-2/VERIFICATION_DEBT.md) | (omit; describe the measurement) |
| F-### / W36-B03 (Codex audit) | Finding ids from the Codex second-opinion audit. | (.port/codex-vet/) | a Codex audit finding |
| NON_PORTS / DEFERRALS / LOOSE_ENDS | Ledgers of things deliberately not ported, deferred with verbatim bodies, or left unfinished. | (.port/phase-3/NON_PORTS.md, .port/phase-4/NON_PORTS.md, .port/phase-2/DEFERRALS.md, .port/phase-3/LOOSE_ENDS.md) | - |
| VALIDATION_REPORT (§3c, M6) | The 07-10 soundness validation report; its items fed the release-profile and panic decisions. | (.port/phase-2/VALIDATION_REPORT.md) | - |
| docs/ghostty-bug-fixes.md | User-facing log of every place Rustty deliberately behaves differently from Ghostty to fix a Ghostty bug (141 entries). | "lists every place Rustty deliberately behaves differently from Ghostty to fix a Ghostty bug" (.port/PORT_RULES.md R13) | the public list of Ghostty bugs fixed in Rustty |

### Phases, waves, tasks, eras

| ID | Plain description | Verbatim (source) | Article phrasing |
|---|---|---|---|
| Phase 0 | Reference contract: inventory of tests/comments/assertions, baseline benchmarks, parity definition (06-21). | (.port/phase-0/) | the contract phase |
| Phase 1 | Workspace scaffold of 29 crates and the first gates (06-21). | (.port/phase-1/REPORT.md) | the scaffold |
| Phase 2 / 2b | The main porting phase: vt and lower tiers, then upper tiers (font, renderer, core, embedded apprt) to 07-08 and beyond. | (.port/phase-2/) | the main port |
| Phase 3..8 | Upstream resyncs: each moves the Ghostty reference commit forward and ports the delta (3: 07-29, 4: 08-17, 5: 08-25, 6: 09-01, 7: 09-28, 8: 10-07). Phase 7 also added the Miri and sanitizer matrix. | (.port/ORACLE_SYNC.md:57-62) | the Nth upstream resync |
| P#-W## (e.g. P7-W13, P8-W9) ⚠ Collides with ISO week W## and WS## workstreams. | Phase number, wave number inside that phase (a batch of parallel work). | (.port/phase-7/README.md) | (omit; give the date and what was done) |
| T# / T1b / T0-C ⚠ Collides with nothing important; drop. | Task number inside a wave or plan. | (commit subjects) | (omit) |
| WS1-WS20 ⚠ Collides with P#-W## waves. | Workstreams of the 07-23 completion roadmap (e.g. WS10 perf, WS14 wasm/WebGL, WS19 closure audit). | (docs/superpowers/plans/2026-07-23-rustty-completion-ROADMAP.md) | the July completion plan's workstreams |
| S-## (e.g. S-4, S-15) ⚠ Collides with S##-F# audit rows. | Tasks in the parity tail after the closure audit. | (.port/parity-closure/) | (omit) |
| R13-xx (e.g. R13-GF2, R13-L2) ⚠ Not a rule; a task id. | Tasks of the never-port-a-bug rollout. | (commit subjects) | (omit) |
| Chunk ids (vt-12b-3a, C0-C15, C-F, E0-E10, F0-F3, A0/A1, R-a/R-d, W0-W5) ⚠ E## collides with eras E1-E3; W0-W5 collides with waves and weeks; A0/A1 with ABI gates. | Porting chunks: vt sub-batches; core chunks and the final audit (C-F); embedded apprt; font tiers; apprt API/embedded; renderer pure/GPU; macOS windowing sub-project. | (~/.claude/projects/-Users-gael-projects-rustty/memory/heavy-tier-strategy.md; placeholder-leaf-doctrine.md) | (omit) |
| Sub-project #1..#7 | The 07-08 completion sequence: macOS windowing, Swift app reuse, headless fill, GTK, Windows, inspector, WASM. | (~/.claude/projects/-Users-gael-projects-rustty/memory/rustty-completion-plan.md) | - |
| W25..W40 (ISO weeks) ⚠ Collides with waves. | ISO calendar weeks of 2026 used in charts. | (strategy.md) | use dates |
| E1 / E2 / E3 (eras) ⚠ Collides with embedded chunks E0-E10. | Usage-stats eras: 06-21..07-09 before the Bun realignment; 07-10..08-31 post-Bun rules; 09-01..10-08 resyncs and closure. | (usage-stats.md:201-203) | the three eras of the project |
| r1..r5 (perf rounds) ⚠ Collides with rules R1-R5 (case only). | Optimisation rounds on 07-16. | (perf.md:69-81) | optimisation round N |

### Other terms

| ID | Plain description | Verbatim (source) | Article phrasing |
|---|---|---|---|
| Goal 1 / Goal 2 | Goal 1 = parity; Goal 2 = performance and later improvements, only after parity is locked. | "Parity is the invariant; performance is the gradient (Goal 2, only after Goal 1 is locked)." (.port/PORT_RULES.md R5) | parity first, performance second |
| placeholder-leaf doctrine | Subsystems not yet portable (GPU, event loop, inspector) became real structs whose methods panic with unimplemented!(), so the code around them could be ported and compiled live. | "define an INHABITED placeholder type (struct, NOT uninhabited `enum {}`) whose methods are `unimplemented!("...deferred, blueprint §X")`" (~/.claude/projects/-Users-gael-projects-rustty/memory/placeholder-leaf-doctrine.md) | stub leaves that panic until their subsystem lands |
| HARD / PROGRESS / BLOCKED | HARD fails CI when red; PROGRESS reports progress while crates are stubs but fails once a crate claims done; BLOCKED (exit 3) means an input is missing and is shown as a warning, never a pass. | "A BLOCKED gate is surfaced as a WARNING, never swallowed." (.port/tools/CI.md) | blocking / progress / blocked gates |
| PORT NOTE / TODO(port) / PERF(port) / PORT STATUS | In-code markers: sanctioned divergence; unresolved item; perf idiom downgraded; per-file status trailer. | (.port/PORT_RULES.md R10) | porting markers in the code |
| oracle | The read-only Ghostty checkout used only as a reference to compare against. | "The reference is used ONLY as an external oracle" (.port/PORT_RULES.md R3) | the reference build of Ghostty |
| pin / ORACLE_PIN | The exact Ghostty commit the port claims parity with (now a60e9e2a5); stored in one file the gates read. | (.port/tools/gates/ORACLE_PIN; .port/ORACLE_SYNC.md) | the pinned Ghostty commit |
| advance-first | Resync doctrine from Phase 4: move the pin first, let gates go red, and use the red count as the progress meter. | "Those REDs are the honest progress meter for Phase 4, not regressions" (.port/ORACLE_SYNC.md:63-71) | move the reference first, then port until green |
| SB / TB | Stacked Borrows and Tree Borrows: the two aliasing models Miri checks Rust code against. | () | Rust's aliasing models (Stacked Borrows, Tree Borrows) |
| mirihar | A small isolated Miri harness that reproduces one borrow shape without FFI, to prove a raw-pointer design sound (33 total). | (.port/phase-2/mirihar-*) | an isolated Miri proof harness |
| SDD | Subagent-driven development: worktree subagents implement, separate agents review (ledgers in gitignored .superpowers/sdd). | (.superpowers/sdd/) | subagent-driven development |
| wf-*.js | Workflow orchestration scripts that fan work out to agents (212 written). | (.port/phase-2/wf-*.js) | orchestration scripts |
| rvm / rwin | The Fedora aarch64 Linux VM and the Windows 11 ARM64 VM on the Mac. | (~/.claude/projects/-Users-gael-projects-rustty/memory/windows-vm-rwin.md) | the Linux VM / the Windows VM |
| SCC | Strongly connected component: Ghostty's 345 files that all import each other in a cycle. | (.port/cycle-analysis.md) | one big import cycle |
| sret / AAPCS64 | The arm64 calling convention returns large structs through a hidden pointer; a size mismatch between Rust and C crashes. | (~/.claude/projects/-Users-gael-projects-rustty/memory/embed-abi-byvalue-sret-gate.md) | a struct-return ABI mismatch |
| ReleaseSafe / ReleaseFast | Zig build modes: ReleaseSafe traps on overflow and failed asserts; ReleaseFast (what Ghostty ships) treats them as undefined behaviour. | (.port/PORT_RULES.md DG2/DG4) | Zig's checked and unchecked release modes |
| comptime flattening | Porting a Zig compile-time parameter as a runtime value; the main source of perf gaps. | (~/.claude/projects/-Users-gael-projects-rustty/memory/comptime-flattening-perf-hazard.md) | - |
| crown jewel crates | rustty-vt, rustty-core, rustty-renderer. | "the crown-jewel crates are `rustty-vt` ... `rustty-core`, and `rustty-renderer`" (AGENTS.md) | the core crates |
| DoP | Definition of Parity, the Phase 0 document defining gates G1-G9. | (.port/phase-0/plan/DEFINITION_OF_PARITY.md) | the parity definition |
| GHOSTTY_C | LIFETIMES class for Ghostty's own C-API pointer fields (replaces Bun's JSC_BORROW). | (.port/audits/GHOSTTY_LIFETIMES.tsv) | - |

### Brief-internal ids

| ID | Plain description | Verbatim (source) | Article phrasing |
|---|---|---|---|
| A1-A8 / N1-N51 (BRIEF §3) ⚠ Collide with ABI gates A3-A5 and port findings N###. | Chart ids in BRIEF.md's visualization inventory. Never print them. | (BRIEF.md §3) | (never in prose) |
| V1-V12 (bugs.md), V1/V4 (Bun) ⚠ Collide with VERIFICATION_DEBT V##. | Chart ids in the bugs dossier and Bun-article widget ids. | (bugs.md, bun-article.md) | (never in prose) |

## Quotable rule texts (12)

Exact text, with ids stripped only where marked `[…]`. The author approved quoting rules
verbatim; ids inside a quote should be cut or glossed in brackets, never left bare.

1. **Comment fidelity (full-fidelity rule).** `.port/PORT_RULES.md` R1, 2026-06-21:
   > Every comment is preserved — doc comments, inline comments, TODOs, license headers, ASCII
   > diagrams, citations/spec references. […] Comments are NEVER dropped or summarized away.

   Pair with the measured result: 34,345 distinctive comments checked, 0 missing, 79
   deliberately not carried (0.23%).

2. **No-loss, not no-addition.** R1:
   > No-loss, not no-addition. Necessary Rust-only glue […] is allowed *because it adds
   > nothing-lost* — it must never replace, omit, or weaken reference logic, comments, or tests.

3. **Never weaken a gate.** R5:
   > Never weaken, skip, xfail, or loosen a test or gate. A red gate that cannot be made green
   > honestly → STOP and surface a precise blocker. Parity is the invariant; performance is the
   > gradient.

4. **Zig-to-Rust primitives (casts and zip).** R7, ratified 2026-07-10:
   > Narrowing: `@intCast` → `T::try_from(x).unwrap()`. Bare `as` is reserved for `@truncate`
   > (intentional wrap).

   > Multi-object loops: `for (a, b) |x, y|` ports with `debug_assert_eq!(a.len(), b.len())` —
   > the length assert is part of the source semantics […], because `.zip()` silently truncates.

5. **Aliasing: Zig pointers are not `&mut`.** Two sources. In-repo, the pointer-lifetime ledger's
   class definition (`.port/audits/GHOSTTY_LIFETIMES.tsv` header):
   > BORROW_FIELD — longer-lived non-owning alias stored in a struct field; owner is another
   > structure -> raw ptr, never &mut (SB discipline)

   Agent memory `rust-mut-args-strongly-protected.md` (not in repo; confirm with author):
   > the Zig reference passes `*Page` / `*Row` / `[]Cell` — Zig pointers/slices are UNPROTECTED —
   > so it's sound there. The Rust `&mut` is a *Rust-only over-constraint*.

   And the method (memory `shaper-hook-sb-clean-interleave.md`): "do NOT hand-reason SB, TEST
   it" on an isolated non-FFI Miri harness.

6. **Lifetimes ledger before porting.** R11:
   > no pointer-heavy tier ports without its GHOSTTY_LIFETIMES.tsv rows landing first
   > (2-reviewer consensus).

   And the ledger header: Port agents consume this table VERBATIM: "trust it over local
   guessing". […] UNKNOWN => Option<NonNull<T>> + TODO(port).

7. **Adversarial review.** R11:
   > Split contexts: implementer, verifier, and fixer are separate agents; the verifier works a
   > concrete violation checklist […] and defaults to not-ok-if-uncertain; the fixer makes
   > surgical edits only and may skip a hallucinated finding with a note

8. **What "fixed" means for unsafe.** R9:
   > "Fixed" bar: safe code cannot cause UB in a release build; debug assertions do not qualify;
   > wrappers that hide the invariant do not qualify.

9. **Never port a bug.** R13, user ruling 2026-09-11:
   > Fidelity is owed to what the reference is **for**, not to its bugs.

   > When unsure, trace where the input comes from before deciding; "the reference also traps"
   > is not by itself a reason to trap.

   Extension 2026-09-23: "the port implements the intended behaviour, not the bug." The user's
   words that triggered it (agent memory `reference-defects-not-ported.md`, confirm): "We
   shouldn't port the crash... Fix it here, update the plan/rules, and document all issues like
   this so we can propose upstream fixes later. Do this for past work too."

10. **Parity only.** User ruling 2026-09-27 (memory `scope-parity-only.md`, confirm):
    > Our goal is only parity at this point, so no need to plan beyond what ghostty provides here.

    And 2026-09-28 (memory `decide-by-parity-minus-bugs.md`): "the aim is rustty parity with the
    exception of copying bugs."

11. **No silent skips.** R14, 2026-09-23:
    > A Rust test that returns early reports `ok`, which cannot be told apart from a pass.

12. **Tracking upstream.** R12:
    > updating the reference checkout does not port anything — "a merge does NOT port the fix;
    > only an explicit .rs change does."

Reserves: DG5 "a Rust panic reaching a C-ABI boundary ABORTS the process."; MIRI_EXCEPTIONS
criterion 3 "No uncovered unsafe path *(the teeth)*"; CI.md "A BLOCKED gate is surfaced as a
WARNING, never swallowed."; memory `derived-gate-green-can-be-bookkeeping.md` "a green result
proves nothing until you check what the green rests on"; AGENTS.md enum sentinel rule.

## BRIEF.md phrases to scrub (find → replace)

| BRIEF text | Plain replacement |
|---|---|
| "layering decisions D1 (vt→font, never font→vt)" | "the font crate sits below the terminal crate and never depends on it" |
| "adopted R7-R12, the LIFETIMES.tsv ledger … 8 regression-class detectors (D1-D6, D8, D9)" | "adopted six new porting rules, a pointer-lifetime ledger, an unsafe census and eight automated checks, each aimed at a regression class Bun reported" |
| "G3b comment fidelity" | "the comment-fidelity check" |
| "the A4 sret gate" | "a check that every struct returned by value has the same size in Rust and C" |
| "Never port a bug: R13 and the defect ledger" | "Never port a bug: the defect ledger" |
| "205 UD entries" | "205 recorded Ghostty defects" |
| "Miri G6 … Tree Borrows … G6c" | "the Miri lanes (Stacked Borrows, Tree Borrows, and a run with integrity asserts off)" |
| "UD350, UD376 and UD377 (Highway)" | "three defects found by Valgrind, LSan and TSan (one in Google Highway)" |
| "N11 is open" | "one low-severity difference is still open: about 35 log lines format values differently" |
| "Phase 7", "P8-W9 T1b" | dates plus what happened ("the October resync", "the final cross-host build check on 10-08") |
| "G8b is owed" | "no benchmark run at the current reference commit yet" |
| "oracle pin" | "the pinned Ghostty commit" |
| "SB/TB" | "Stacked Borrows / Tree Borrows (Rust's aliasing models, checked by Miri)" |
| "E1/E2/E3" | "before the July realignment / July-August / September-October" |
| "rvm", "rwin" | "a Fedora aarch64 VM", "a Windows 11 ARM64 VM" |

## Measured: host hardware (for the methodology appendix)

MEASURED now (`sysctl -n machdep.cpu.brand_string hw.memsize hw.ncpu; sw_vers -productVersion`):
**Apple M4 Pro, 48 GiB (51,539,607,552 B), 14 cores, macOS 26.5.1**. The author said "MacBook
Pro M4"; the chip string is **M4 Pro**, which matches perf.md line 4 and MIRI_EXCEPTIONS.md
("M4 Pro, 8 processes"). The macOS version at benchmark time (07-16, 10-07) is not recorded
here; 26.5.1 is today's.

## Gaps

- Memory-file quotes (aliasing rule, R13 trigger quote, parity-only quote) are outside the repo;
  the author must confirm before they appear as quotes. In-repo alternatives are given.
- Design decisions D2, D4, D5 (06-25/26 panels) are only referenced in passing; their full
  records are in `.port/phase-2/*BLUEPRINT*.md` and were not traced (they are not used in BRIEF).
- `S##-F#` audit section numbering was not mapped to areas (the area files
  `.port/parity-closure/src-*.md` hold that); not needed for prose.
- Codex audit finding ids (F-###, W36-B03) were not traced into `.port/codex-vet/`.
- Chunk ids in the timeline (C2, F3, C14, E0-E10) are glossed only as families.

## Fact-check

Independent re-derivation, 2026-10-08, HEAD `1d22ba80d`. Method differs from the dossier's
(`grep`/`sort -u` over `ci.sh` instead of `r2-glossary-registry.py`; `git show` per commit;
whitespace-normalised substring search for every quote).

1. **Glossary size: CORRECTED (wording).** `data/r2-glossary-ids.csv` has 162 rows, confirmed
   (`csv.DictReader`, all `DOCUMENTED`). Categories: gate 73, rule 17, term 15, ledger 14,
   phase 14, detector 12, decision 11, concept 4, brief-internal 2. It does **not** hold 101
   registry rows: 80 of the 101 registry ids have their own row (all 77 base checks plus 3
   self-tests without a base gate: G-MIRIRUN-SELF, G-SAN-SELF, G-ORACLE-SH-SELF); the other 21
   `-SELF` ids are covered by one "-SELF suffix" row. A5-MASK-SELF has no row of its own. Say
   "covers all 101 registry ids (80 with their own row, 21 self-tests through the suffix
   row)". "Each row cites its source": 161 of 162; the `SB / TB` row has an empty `source`.
2. **Registry size and split: CONFIRMED.** `grep -oE '"[A-Za-z0-9-]+\|(HARD|PROGRESS)\|'
   .port/tools/ci.sh | sort -u`: 101 lines, 101 distinct ids, 75 HARD, 26 PROGRESS, 24 ending
   `-SELF`; first entry line 533 (G7), last line 1327 (G-KAITAI).
3. **86 / 22 reconciliation: CONFIRMED (arithmetic), mechanism PLAUSIBLE.** Removing the 11
   D-ids (D1-D6, D8, D9, D10, D11, D11r) and 4 A-ids (A3, A4, A5, A5-MASK-SELF) leaves exactly
   86, of which 23 end `-SELF`. The 22 is the `G-*-SELF` set (G1-SELF excluded); the source
   regex (codebase.md:409, `G[0-9]...` / `G-...`, with a `G6.` artifact removed) explains why
   G1-SELF landed among the 64, but that regex was not re-run. Orphan self-tests re-derived
   by checking each `X-SELF` for a registry id `X`: exactly A5-MASK-SELF, G-MIRIRUN-SELF,
   G-ORACLE-SH-SELF, G-SAN-SELF.
4. **D1 collision: CONFIRMED.** BRIEF.md:33 (item 4) "layering decisions D1 (vt→font, never
   font→vt)" = memory `apc-glyf-font-layering-d1.md` ("design-panel unanimous, 2026-06-25");
   BRIEF.md:42 (item 6) "D1-D6, D8, D9" = detectors wired by `94ca0243e` 2026-07-12 ("wire
   D1-D9 semantic-mismatch detectors into ci.sh"; ci.sh:1170 `"D1|PROGRESS|...check_slice_
   reinterpret.sh"`); CI.md:244 heading. The 07-10 plan line 31 opens the DG table (`| DG1 |
   Ratify PROPOSED R7–R12 ...`).
5. **Other collisions: CONFIRMED (spot-checked).** NEW-FINDINGS.md headings run to N530;
   BRIEF chart ids include A1 and N51; bugs.md uses V1-V12 and also V27/V29; V17/V27/V28/V29
   appear in `.port` ledgers; S121-F1 rows in `src-stb.md`; M1/M2 in `.port/audits`. W##, E#,
   L### families not re-checked individually.
6. **D7: CONFIRMED, with a better source.** CI.md:250 says only "D7 is intentionally absent
   (no detector was carved for it)" and does not say why. The "census covers it" part is
   documented elsewhere: BUN_PORT_LEARNINGS.md:691 "**D7 — unsafe census + unsound-safe
   ledger**", and plan 2026-07-10 Task 17 "Unsafe census + UNSOUND_SAFE ledger (D7, R9)". Cite
   those two, not CI.md alone.
7. **Rule adoption dates: CONFIRMED.** `git show <c> -- .port/PORT_RULES.md` headings added:
   15c1a9bd7 06-21 `## R1`..`## R5` (R1-R5 present in that blob); d9fd637ce 06-21 `## R6`;
   92a86031b 07-10 `## DG2/DG4`; ccd14200e 07-10 `# R7–R12 ... ratified 2026-07-10`; 7dd991582
   09-11 `## R13`; f94ab8e81 09-23 adds "the port implements the intended behaviour, not the
   bug"; 1096fa4cd 09-23 `## R14`; fd7c17c56 10-02 adds "*Amended 2026-10-02 (P7-W16a, N501):*"
   (LSan on macOS). Omission: **DG5** (C-ABI panic policy: abort) was added the same day in
   `9f87df18e` 2026-07-10; add it if the arc lists the DGs.
8. **N11: CONFIRMED.** NEW-FINDINGS.md:498 heading "~35 sites"; :502 "severity: low
   (message-text divergence in a diagnostic ...)"; :548 "state: OPEN — scope recorded, stopped
   under the 2-hour rule (G-1, 2026-09-28)". The cited range 498-504 holds the heading and
   severity; the OPEN state is at line 548.
9. **Host hardware: CONFIRMED.** `sysctl`: "Apple M4 Pro", 51,539,607,552 B (= 48 GiB), 14
   cores (10 performance + 4 efficiency); `sw_vers` 26.5.1. perf.md:4 "All runs on one host:
   Apple M4 Pro"; MIRI_EXCEPTIONS.md:3103 "2026-10-02, M4 Pro, 8 processes". Caveat:
   MIRI_EXCEPTIONS.md:2049 records one re-measure pass "on a host visibly slower than the M4
   Pro", so not every Miri timing came from this machine.
10. **Quotes: CONFIRMED, with one caveat.** All 12 quotes and 5 reserves were found in their
    cited files after collapsing whitespace (PORT_RULES R1/R5/R7/R9/R11/R12/R13/R14;
    GHOSTTY_LIFETIMES.tsv header; MIRI_EXCEPTIONS; CI.md; memory files
    `rust-mut-args-strongly-protected`, `shaper-hook-sb-clean-interleave`,
    `reference-defects-not-ported`, `scope-parity-only` (dated 09-27 in its body),
    `decide-by-parity-minus-bugs` (09-28), `derived-gate-green-can-be-bookkeeping`). Caveat:
    quotes 1 and 4 drop the source's markdown bold (`**Every comment is preserved**`,
    `**Narrowing:**`). That is fine for prose, but they are not byte-exact.
11. **Plain outline: CONFIRMED.** The given regex over `r2-outline-plain.md` returns nothing
    (rc=1). A wider sweep (DG#, V#, E#, W##, S-#, L#, G-XXX, -SELF, SB/TB, rvm/rwin) finds only
    "M4 Pro", which is the chip name and not an id. 18 numbered items. Italic author notes are
    present: motivation (l.16), hardware (l.65), disclosure (l.87), Claude Code on the Mac and
    the Linux VM (l.107), subscription with the maxed-out, count-half extrapolation (l.109-111).
