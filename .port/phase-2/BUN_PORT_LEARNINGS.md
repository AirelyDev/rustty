# Bun Zig→Rust Rewrite — Learnings for rustty

**Status:** research synthesis, 2026-07-10. Sections 1–4 are recommendations; section 5
is PROPOSED rule text, **not yet agreed**.
**Inputs:** 5 research agents covering Bun's PR oven-sh/bun#30412 ("Rewrite Bun in Rust",
~1M lines, merged 2026-05-14), its deleted internal docs (docs/PORTING.md @ f6ad291ea032,
LIFETIMES.tsv, CYCLEBREAK.md, rust-rewrite-plan.md, PORT_NOTES_PLAN.md, CI_RUST_MIGRATION.md),
its committed `.claude/workflows/*.workflow.js` orchestration scripts (@ ed1a70f81708),
five post-release regression post-mortems, the 548-comment HN thread (item 48016880), and
Bun's published unsafe-code audit (bun.com/bun-unsafe-audit) + TigerStyle context.
**Frame:** every learning is scored against rustty PORT_RULES R1–R6
(R1 100% fidelity; R2 rustty identity + ghostty-parity profile; R3 self-contained,
reference as external oracle; R4 acyclic 39-crate layering + cycle_check; R5 gates never
weakened; R6 Miri primary / ASan FFI / LSan Linux CI).

---

## 1. TL;DR — the 10 highest-leverage adoptions, ranked

1. **GHOSTTY LIFETIMES.tsv** — a one-time, whole-of-reference pointer-field ownership
   table (Bun's schema: file/struct/field/zig_type/class/rust_type/evidence; 11-class
   taxonomy, 2,252 rows, "trust it over local guessing"), built by an evidence-cited
   classify workflow with 3-vote default-refute verification. Front-loads exactly the
   decisions that produced rustty's two SB-UB incidents; gives every R6 Miri harness a
   written per-field invariant. Highest-leverage single artifact found.
2. **Semantic-mismatch detector suite** (section 4) — Bun shipped five well-documented
   regression classes (slice-cast slop, latent OOB + release bounds checks, placeholder
   constants, comptime→runtime injection, flattened sanitizer switch). Each has a cheap
   grep/test/CI detector; all five classes are HIGH-likelihood in a terminal emulator.
3. **Unsafe census + unsound-safe ledger** (Bun's bun-unsafe-audit method): one committed
   reproducible `rg` command as ground truth, origin × destination taxonomy via
   `// SAFETY(origin/dest):` tags, per-crate density split by R6 tier, monotone-density
   rule, and a SEPARATE ledger of unsound *safe* functions — the class grep-for-unsafe
   audits cannot see and rustty's known hazard class lives in.
4. **Marker taxonomy + perf-intent channel** — `TODO(port)` / `PERF(port): <zig idiom>` /
   `PORT NOTE:` / per-file PORT STATUS confidence trailer, all machine-greppable; a
   bench-until-green recovery phase greps PERF(port) before declaring R2 perf parity.
   Bun's 7,120-marker census → leverage-ranked backlog is the endgame shape.
5. **Main-parity workflow** — standing wf script that enumerates upstream ghostty commits
   touching ported files, verdicts ported/missing/partial with file:line evidence +
   2-vote adversarial verify. Load-bearing doctrine: "a git merge does NOT port the fix"
   and "ALSO port the test hunks from the same commit."
6. **Oracle-test mechanics** — cached per-file baselines from the reference binary,
   parity = **pass-count equality, not exit code**; failures produce `.diag` files
   (crash/hang/diverge taxonomy) as the fixer's only runtime evidence; new regression
   tests are invalid unless they FAIL on the oracle side; fail-first proof documented
   per fix (`tests/regression/issue_NNN.rs`).
7. **Written semantic-mapping tables** — Bun's integer map (`@intCast`→`try_from().unwrap()`,
   never bare `as` for narrowing; `+%`→`wrapping_add`; `@intFromFloat` saturation note),
   assert map (`assert`→`debug_assert!`, `catch unreachable`→`.expect()`, never
   `unwrap_unchecked` until proven), and zip-truncation rule (`for (a,b)` carries an
   implicit length assert → `debug_assert_eq!` is R1-mandatory) — adopt as binding tables.
8. **Comptime & dispatch policy** — the comptime ladder (generic > const-generic > macro,
   `if b { f::<true>() }` bridge, "trait bound IS the @hasDecl check"), plus the
   cross-tier dispatch pattern: manual vtable for cold paths, low-tier `(tag, *mut ())` +
   high-tier hoisted match for hot paths — preserves Zig inline-else codegen without an
   R4-violating edge. "A dep cycle is a layering bug, not something to route around."
9. **Swarm economics + hygiene for wf scripts** — one expensive pass (build/Miri) per
   round with N read-only fix agents; contiguous-slice worktree shards, ≤16 fixes/round,
   push-per-round; agent git discipline verbatim ("Never git reset/checkout/stash;
   commit explicit paths; hooks disabled; NO push"); chunked-write protocol for >1000-LOC
   files; counted `new_unsafe == 0` review field.
10. **Living-rulebook discipline** — every newly discovered failure class becomes a named
    rule in PORT_RULES the same day, paired with a retroactive sweep workflow over
    already-ported code; periodically run a "zig-leakage"-style adversarial audit of the
    rulebook itself (trial-port-diff: port 3 files by the rules vs Rust-native, every
    divergence indicts a specific rule).

---

## 2. Source map

**S1 — docs/PORTING.md deep-dive** (blob @ f6ad291ea032, 756 lines, deleted pre-merge in
10cfa0b237). The Phase-A rulebook agents read whole before each file: exact
type/idiom/allocator/error/comptime/concurrency maps, a verify-enforced Forbidden list,
marker conventions, and the pre-computed LIFETIMES.tsv. Its git history is a chronicle of
bans added per failure class. Richest source for §3.1.

**S2 — PR #30412 + workflow scripts + internal docs** (@ ed1a70f81708). The execution
machinery: phases A–H, per-file implement→adversarial-verify→surgical-fix pipelines,
CYCLEBREAK.md symbol taxonomy, test-swarm rounds against the unchanged TS suite
(USE_SYSTEM_BUN=1 baselines), worktree shards, main-parity re-porting, dedup and unsafe
sweeps, and PR #30224's must-not-squash pure-git-mv restructure. Richest source for §3.3/§3.5.

**S3 — Post-release regression post-mortems** (issues #31188, #31503, #30693, #30678;
PRs #31504, #30694, #30684, #30875). Five distinct Zig→Rust semantic-mismatch classes
that escaped ALL of Bun's gates and shipped to canary, each with a documented root cause
and a derivable detector. Basis of §3.2 and §4.

**S4 — HN thread 48016880** (548 comments incl. Jarred Sumner in-thread). External
critique: unsafe-dense transliteration, unreviewable mega-diffs, determinism-vs-oracle
debate, the two genuinely hard gaps (pointer aliasing, comptime), crate denylist
rationale, cost economics (~$200-500 tokens for 700k lines — verification, not
generation, is the binding constraint). Basis of most of §3.6.

**S5 — bun.com/bun-unsafe-audit + TigerStyle** (+ src/ptr/*.rs heritage). The
post-port unsafe accounting method: 13,365 sites, one rg command, 8-origin × 5-destination
taxonomy, adjudicated dual classification with published agreement, unsound-safe-fn
ledger, soundness-first roadmap, and the Zig `Shared`→documented `Rc` alias precedent.
Basis of §3.4 and the proposed accounting scheme.

---

## 3. Learnings catalog

Tags: **ADOPT** (take as-is), **ADAPT** (take with rustty-specific modification),
**COVERED** (already embodied by R1–R6 / memory notes), **REJECT** (conflicts with the
locked plan). Duplicates across research agents are merged; one entry per learning.

### 3.1 Porting rules (idiom & semantic mappings)

- **Deinit-as-ownership-oracle** — ADOPT. Field ownership for `[]const u8`/pointers is
  decided by reading `deinit` ("freed → `Box<[u8]>`; never freed + literals → `&'static`;
  arena → raw"), never by adding lifetimes speculatively; "default to Box/&'static/raw,
  add `<'a>` only with a proof" is the R6-compatible tie-breaker.
  <https://github.com/oven-sh/bun/blob/f6ad291ea032/docs/PORTING.md>
- **LIFETIMES.tsv pre-computed pointer table** — ADOPT (TL;DR #1). 11 classes
  (OWNED/SHARED/BORROW_PARAM/BORROW_FIELD/STATIC/BACKREF/INTRUSIVE/FFI/ARENA/UNKNOWN +
  a GHOSTTY_C class replacing JSC_BORROW), each row citing "file:line — what you saw";
  agents use the rust_type column verbatim. Kills the highest-variance per-agent decision.
  <https://github.com/oven-sh/bun/blob/f6ad291ea032/docs/LIFETIMES.tsv>
- **Integer-semantics map** — ADOPT verbatim as a binding table (feeds PROPOSED R7).
  `@intCast`→`T::try_from(x).unwrap()` (never bare `as` for narrowing; `as` reserved for
  `@truncate`); `+%`→`wrapping_add`; `-|`→`saturating_sub`; `@intFromFloat` — Rust
  saturates where Zig is UB, keep Zig's prior range checks, don't add new ones;
  `@enumFromInt`→`#[repr(uN)]` transmute or debug-asserting `from_raw`, never
  FromPrimitive in hot paths. Reproduces Zig ReleaseSafe semantics in the rustty profile
  with a PERF(port)-marked `as` option for the parity profile.
  <https://github.com/oven-sh/bun/blob/f6ad291ea032/docs/PORTING.md>
- **Assert/unreachable map** — ADOPT. `bun.assert`→`debug_assert!`; `unreachable`→
  `unreachable!()` (checked; Zig's release-UB deliberately NOT reproduced);
  `catch unreachable`→`.expect("unreachable")`, never `unwrap_unchecked()` "until Phase B
  proves the invariant". Exactly R1's every-assertion-ported plus an R5-compatible safety
  upgrade; write down the "checked-until-proven, then PERF(port)" escalation path.
- **Zip-truncation / memcpy semantics** — ADOPT. `for (a,b) |x,y|` asserts equal lengths
  in Zig; `.zip()` silently truncates → `debug_assert_eq!(a.len(), b.len())` is a ported
  assertion under R1, not an addition. `@memcpy`→`copy_from_slice` (panics on mismatch,
  same as Zig, non-overlapping only); overlap → `copy_within`/`ptr::copy`.
- **Comptime ladder** — ADOPT (TL;DR #8). `comptime T: type`→plain generic (not const
  generics); `comptime b: bool`→`<const B: bool>`, demote to runtime arg + PERF(port)
  only when not gating a hot inner loop; runtime→comptime bridge
  `if b { f::<true>() } else { f::<false>() }`; `inline else`→written-out match arms or a
  trait; macros only for token-pasting / @typeInfo-layout-branching. Keep a per-instance
  decision log (generic/trait/macro/build.rs) — the one axis where line-fidelity is
  structurally impossible and drift hides (HN: sufficientsoup/dathinab).
- **@typeInfo/@hasDecl → traits, never runtime checks** — ADOPT. "A trait bound IS the
  @hasDecl check"; optional decls → trait with default method; field-name reflection →
  targeted derive only. This is how the apprt surface stays honest.
- **@tagName/@errorName → `strum::IntoStaticStr`; Display is the wrong channel** —
  ADOPT. "Never Display/to_string() here — that is the human message, not the tag."
  Ghostty uses @tagName pervasively (config enums, key names, OSC); conflation causes
  snapshot/config-roundtrip divergence invisible to unit review.
- **Error model (Copy `NonZeroU16` interned errors; never anyhow/Box<dyn Error>)** —
  ADAPT. rustty already uses per-module thiserror enums (Ghostty error sets are explicit);
  transferable: errors stay Copy + payload-free (R1 field semantics), @errorName parity
  via IntoStaticStr with the EXACT Zig tag string, and the interned-u16 design if an
  anyerror-equivalent is ever needed across the C ABI (R2).
- **defer/errdefer 5-rung ladder + unit-scopeguard ban** — ADOPT as written defer policy.
  Rungs: delete (Drop covers), delete (? drops on error path), scopeguard with real state
  + into_inner disarm, RAII for non-freeing side effects, and "`scopeguard::guard` with
  unit state `()` is banned — it always means a missed RAII opportunity." The formally
  rejected `defer_mut!(*mut T, F)` macro independently confirms rustty's
  shaper-hook-sb-clean-interleave memory.
- **deinit→Drop with enumerated exceptions** — ADOPT. Keep explicit bodies only for side
  effects beyond freeing; `#[repr(C)]`-across-FFI types keep `unsafe fn destroy(*mut Self)`
  (matches vt-c trampoline seam); explicit early release is `close(self)` by value, never
  `pub fn deinit(&mut self)`.
- **Allocator-param policy: delete by default, keep load-bearing arenas** — ADOPT the
  decision procedure. Arena crates keep `&'bump Bump` / `typed_arena` (stable-address
  slabs — rustty's Page structures are the analog; "borrow checker enforces reset()"
  serves R6); everywhere else `std.mem.Allocator` param → delete. Adversarially derived:
  "95% of these params are dead weight."
- **Global-allocator precondition stated once** — ADAPT. State in PORT_RULES which
  allocator each build profile uses, asserted in CI, so every allocator-param deletion is
  justified by one auditable rule. (See also Class 5 in §3.2 — never fold the switch.)
- **Bytes-not-String doctrine** — COVERED (zig-byteslice-to-rust-rawbytes memory). ADOPT
  the two refinements: `bstr::BStr::new(bytes)` for Display/Debug (never from_utf8_lossy),
  and the explicit `std::path` ban rationale for path code.
- **Struct/layout map + all-bool bitflags criterion** — ADOPT the sharp edges:
  `packed struct(uN)` → bitflags! ONLY if every field is bool, else
  `#[repr(transparent)]` newtype with shift accessors (Ghostty style/cell flags are
  mixed-width → newtype form); `mem::zeroes` → `mem::zeroed` only for repr(C) POD with no
  NonNull/NonZero/enum fields + `// SAFETY: all-zero is a valid T` (an R6 guard).
- **Collections: std by default; custom only with a NAMED behavioral constraint** —
  ADOPT the shape. Every surviving custom type cites its constraint (FFI ABI / SoA /
  hash determinism / on-disk format); Wyhash11's disk-compat carve-out generalizes: any
  Zig algorithm with on-disk/wire compatibility ports bit-exact, never swapped. Annotate
  rustty's ported IntrusiveLinkedList/CircBuf with their constraints.
- **Concurrency: Mutex owns its data; defensive locks deleted via the Sync-bound oracle** —
  ADAPT. Ghostty's renderer/termio threading is load-bearing — do NOT delete-by-default;
  but adopt both structural rules: `lock: Lock, table: HashMap` → `table: Mutex<HashMap>`,
  and "delete + `// PERF(port): was Lock-guarded`, let a failing `T: Sync` bound prove the
  need" (compiler as arbiter strengthens R5).
- **Global-singleton table + `*mut` aliasing doctrine** — COVERED in essence
  (rust-mut-args-strongly-protected, shaper-hook memories independently derived
  "hold `*mut T`, deref per-access, never bind `&mut *get()` across calls"). ADOPT the
  codified banned-pattern list verbatim (static mut + `&mut`; `&'static mut` fields;
  re-used `&mut` across calls) and the per-access-shape table
  (thread-the-pointer / OnceLock / LazyLock / session-owned) as a named PORT_RULES section.
- **Platform conditionals & lazy-evaluation parity** — ADOPT all four: `cfg!` type-checks
  BOTH branches (use `#[cfg]` when the dead branch names platform-only items); log macros
  must keep arg expressions inside the disabled branch; `comptimePrint`→`concat!`/
  `formatcp!`, never runtime `format!`; `thread_local!` with `const { }` init;
  `@branchHint(.cold)`→`#[cold]`.
- **Cross-tier dispatch: cold vtable / hot (tag,ptr)+hoisted match** — ADOPT (TL;DR #8).
  Low tier stores `(tag: u8, *mut ())`, high tier owns the match with direct call arms →
  "LLVM inlines exactly like Zig"; explicit ban on Box<dyn Trait>/enum_dispatch for the
  hot whitelist; keep the whitelist discipline (name which dispatches count as hot).
  <https://github.com/oven-sh/bun/blob/f6ad291ea0/docs/PORTING.md>
- **Layering-workaround bans** — COVERED by R4 + prior seam work; ADOPT the grep-able ban
  list: fn-ptr hooks with one consumer, `extern "Rust"` between crates,
  `*mut c_void` downcast to one concrete type at every read site — each means
  "the code is in the WRONG CRATE." Bun sanctioned AtomicPtr hooks early and had to
  revert; agents WILL invent these under pressure unless the guide names them.
- **Forbidden leak-shaped fixes** — ADOPT. `Box::leak`/`mem::forget` for `&'static`;
  `unsafe { &*(p as *const _) }` lifetime extension; `.clone()` to escape a borrow;
  unpaired `ManuallyDrop`. These encode "if the Zig freed it, Rust must too" = R1
  fidelity for resource lifetimes, and feed R6/LSan.
- **ZERO todo!() rule** — ADAPT. Directly contradicts the locked placeholder-leaf
  doctrine (INHABITED placeholders with `unimplemented!()` leaves) — keep rustty's
  doctrine, but note Bun also began with annotated stubs and hardened to ZERO at endgame:
  rustty's C-F unimplemented audit is the required dated endgame sweep.
- **Re-implementing linked C is forbidden; crates.io-for-pure-Zig exception** — C half
  COVERED by R3 (same C libs via -sys). REJECT the crates.io exception as policy (R1:
  rustty hand-ported z2d/zf for fidelity); ADOPT the warning as process — hand-ported
  bit-twiddling is a bug magnet, so R1 hand-ports get extra oracle differential tests
  (R3 permits) plus Miri.
- **Dependency denylist** — ADOPT as written rule. Bun: "No tokio, rayon, hyper,
  async-trait, futures. No std::fs" because "Bun owns its event loop and syscalls."
  rustty analog: no async runtime, no rayon; I/O routes through the ported libxev;
  render thread stays in core. Prevents a future agent "helpfully" introducing tokio in
  a leaf crate. <https://news.ycombinator.com/item?id=48017418>
- **Naming/structure rules** — COVERED in spirit by R1 mirroring; ADOPT the crisp
  additions: acronym rule ("a run of ≥2 uppercase letters is one segment": toUTF8→to_utf8),
  a sanctioned-reshape-exceptions list (out-param constructors → `Result<Self,E>`;
  deinit→Drop) so fidelity review distinguishes sanctioned divergence from drift, and the
  labeled-block rule (`brk:` → Rust labeled block; hoist to helper only if >40 lines AND
  ≥3 break points).
- **Extension-trait dependency inversion** — ADOPT as the generalized rule. "The base
  type has no mention of [the higher tier]; to_js/from_js live as extension-trait methods
  in the higher crate" — the identical move rustty made twice (D1 font layering,
  renderer-config accessor trait), now written as doctrine.
- **Runtime assertions ported as DOCUMENTED static guarantees** — ADOPT. A Zig assertion
  is faithfully ported when kept as `debug_assert!` OR replaced by a named static
  guarantee with a mandatory mapping comment (e.g. "ThreadLock debug-assert → enforced
  statically by `Rc<T>: !Send + !Sync`") so the R1 completeness critic can tick the line.
  <https://github.com/oven-sh/bun/blob/main/src/ptr/shared.rs>
- **Isomorphic Zig abstraction → documented std type-alias** — ADAPT under R1. When a
  Ghostty abstraction is behaviorally isomorphic to a std type, port as
  `pub type X = Rc<T>` plus an exhaustive legacy→Rust method-map doc table (that table IS
  the fidelity artifact), and record the decision forbidding a gratuitous custom clone
  ("4 uses tree-wide, 8 bytes per allocation is negligible").
- **TigerStyle transfer** — ADOPT assertions positive-and-negative-space + "put a limit
  on everything" where Ghostty has them; REJECT static-allocation-at-startup and literal
  zero-deps (R1 forbids re-architecting; R3 locks the C libs); ADAPT u32-over-usize as
  "keep Ghostty's declared widths, convert to usize only at the slice-index boundary."
  <https://tigerstyle.dev/>

### 3.2 Semantic-mismatch taxonomy (regressions that shipped past all of Bun's gates)

- **Class 1 — slice reinterpret: Zig truncates, Rust panics** — ADOPT detector D1.
  `bun.reinterpretSlice(u16, buf)` silently drops a trailing odd byte (@divTrunc);
  `bytemuck::cast_slice` panics on slop (#31188, odd-length UTF-16 Blob). Fix restored
  truncate semantics: `&buf[..buf.len() & !1]`. Every Zig slice reinterpret needs an
  explicit truncate-vs-error decision + odd/misaligned-length test. HIGH likelihood:
  page memory as cell/style arrays, OSC 52 UTF-16 payloads. Miri catches misalignment UB
  but NOT truncate-vs-panic divergence. <https://github.com/oven-sh/bun/issues/31188>
- **Class 2a — latent Zig OOB detonates under Rust release bounds checks** — ADOPT
  detector D2. `ptrs[allocated]` write-then-increment off-by-one was silent OOB in
  ReleaseFast Zig for years, hard panic in Rust (#31503/#31504); Zig's own
  `IntFittingRange(0, max + 1)` encoded the truth. Rules: derive array length from the
  counter-range expression; capacity-saturation tests drive every bounded store to its
  ceiling; triage step — a panic where Zig "worked" ⇒ re-run the oracle under
  ReleaseSafe/Debug before assuming the port is wrong (R3). HIGH likelihood: Ghostty's
  fixed-capacity structures (CSI param buffers, page capacities, kitty quotas).
  <https://github.com/oven-sh/bun/pull/31504>
- **Class 2b — placeholder constants ported by value silently shrink capacity 32×** —
  ADAPT detector D3. A literal `64` behind TODO(port) vs the derived `count / 4` (=2048)
  cut a store ceiling ~32× and made 2a reachable. Rules: derived constants port as the
  same const EXPRESSION, never a folded literal; zero placeholder-constant markers
  survive a sub-batch gate (scoped per per-subbatch-scoped-g3b, not deferred to finale);
  constants-parity diff vs the Zig twin.
- **Class 3 — comptime string processing ported to runtime = escape injection** — ADOPT
  detector D4. Zig's `inline fn pretty(comptime fmt, args)` rewrote tags on the TEMPLATE
  before substitution; the runtime-fn port parsed fully-rendered bytes, so an OSC 8 ST
  (`ESC \`) collided with the tag grammar and corrupted output (`ai`→`air`, #30693).
  Fix restored phase separation via a proc-macro. Rule: Zig `comptime` param ⇒ Rust
  macro/const/generic; a runtime-fn port is a flagged deviation requiring proof no
  runtime bytes can alias the template grammar. VERY HIGH likelihood: Ghostty
  comptime-builds CSI/SGR responses and the data domain IS escape sequences.
  <https://github.com/oven-sh/bun/issues/30693>
- **Class 4 — coupled-emission-site invariant drift** — ADAPT detector D5. Two
  individually-correct modules computed "is Fast Refresh active" differently → client
  crash (#30678/#30684); no single file diff violates R1, so per-file fidelity review is
  blind. Rule: catalog every predicate consulted at 2+ emission/consumption sites as a
  named paired invariant; debug_assert at the producer, graceful degrade at the consumer,
  one integration test through the seam. rustty pairs: dirty-tracking↔renderer damage,
  style-id refcounts Page↔style set, kitty placements↔atlas, mode state↔key encoder.
  <https://github.com/oven-sh/bun/pull/30684>
- **Class 5 — flattened comptime build switch blinds the sanitizer gate** — ADOPT
  detector D6. Zig's ASan builds switched off mimalloc so ASan's interceptor saw
  allocations; the port hard-coded `#[global_allocator] Mimalloc`, so ASan/LSan "passed"
  while blind for the whole 1.4 line (fixed in #30875, which then found leaks in
  essentially every subsystem). Rules: every Zig `builtin.mode`/build-option switch ports
  as `cfg`, never constant-folded; CI canary tests that deliberately leak and
  heap-overflow MUST fail the sanitizer build (proves the gate can see); suppression-list
  entries require per-entry justification. This is R5+R6 made concrete.
  <https://github.com/oven-sh/bun/pull/30875>
- **Cross-cutting — ported comments asserting invariants the new primitive broke** —
  ADAPT (refines R1 practice). The pre-fix #31188 comment claimed "len is even after BOM
  strip" and "mirrors Zig reinterpretSlice" — both false. Rule: when a Zig builtin is
  replaced by a non-identical Rust primitive, the adjacent comment MUST be rewritten to
  the Rust primitive's failure mode (fidelity to intent over stale text), and any
  "len/align is guaranteed X" comment near a panicking cast gets a test asserting X.
- **Regression-fix workflow shape** — ADOPT. Per escaped defect: dedicated
  `tests/regression/issue_NNN.rs` reproducing the exact user-visible failure; documented
  fail-first proof (stash fix → test fails → pop → passes); panic policy "reachable
  pathological input ⇒ Err; unreachable-by-invariant ⇒ ported assertion"; ST-integrity
  golden assertions for emitted VT bytes ("every OSC opener is followed by a correct ST").

### 3.3 Review & validation workflow

- **Living rulebook + same-day rule + retroactive sweep** — ADOPT (TL;DR #10). Each
  PORTING.md ban commit was paired with a sweep workflow purging existing instances;
  R5 means rules only ever tighten. rustty: codify in .port/PORT_RULES.md, not only
  memory files. <https://github.com/oven-sh/bun/commits/ed1a70f81708/docs/PORTING.md>
- **Zig-leakage adversarial audit of the rulebook itself** — ADAPT. 8 dimension auditors
  + trial-port-diff (port 3 files by the rules, rewrite Rust-native, diff indicts
  specific rules), findings face 3 default-refute votes with a mandatory
  constraint_check. rustty scores rules against "preserves fidelity AND Miri-soundness,"
  not idiomatic-ness.
  <https://github.com/oven-sh/bun/blob/ed1a70f81708d7d137de8de057d11668c5f4e220/.claude/workflows/porting-md-zigleakage.workflow.js>
- **Evidence-cited classification + 3-vote default-refute + sampled confident claims** —
  ADOPT as the reusable verification shape (pointer fields, comment-coverage audits,
  SAFETY audits): claims cite "file:line — what you saw"; every UNKNOWN plus a 12% random
  sample of high-confidence claims get 3 adversarial voters told "Default refuted=true if
  uncertain"; ≥2 refutes overturns.
- **Implement → adversarial-verify → surgical-fix pipeline** — ADOPT for wf-*.js.
  Verifier works a concrete violation checklist keyed to PORT_RULES, "Default to
  ok=false"; fixer is confined ("Surgical edits only") with the escape valve "if an issue
  is wrong (verifier hallucinated), skip it and note"; orchestrator canonicalizes paths
  itself ("do NOT let agents pick"); every stage returns machine-checkable JSON.
  <https://github.com/oven-sh/bun/blob/ed1a70f81708d7d137de8de057d11668c5f4e220/.claude/workflows/phase-a-port.workflow.js>
- **2-vote intersect + default-reject tiebreak** — ADOPT. Two independent verifiers,
  intersect agreed findings, tiebreak disputes with "Default confirmed=false unless you
  verify against .zig." Right polarity for R1: a false 'ported' verdict silently loses
  fidelity; a false 'missing' only costs a re-check.
- **Architecture docs with adversarially-verified claims + mechanical perf invariants** —
  ADOPT. "Every non-obvious factual claim survived 3-vote adversarial verification at the
  cited file:line"; hot-path parity as checkable invariants (cargo asm shows zero call
  instructions; compile-asserted struct sizes).
  <https://github.com/oven-sh/bun/blob/f6ad291ea0/docs/rust-rewrite-plan.md>
- **Oracle-test mechanics** — ADOPT (TL;DR #6). Baseline cached per file from the
  reference binary; parity = pass-count equality, not exit code; `.diag` files
  (crash/hang/diverge) are the fixer's only runtime evidence; "your test is NOT VALID if
  it passes with [the oracle]" for new regression tests.
  <https://github.com/oven-sh/bun/blob/ed1a70f81708d7d137de8de057d11668c9f4e220/.claude/workflows/phase-g-test-swarm-v3.workflow.js>
- **One-expensive-pass-per-round economics** — ADOPT. "ONE rebuild/round"; fix and review
  agents are FORBIDDEN to build ("you work from the diagnostic file + reading source");
  only the dedicated build-break fixer loops on cargo check. rustty analog: one
  Miri/test pass per round amortized across N read-only fix agents (R6 runs are the
  expensive gate).
- **Worktree shards** — ADOPT for finale test-bringup: contiguous slices (not modulo) so
  shards touch disjoint subsystems; own worktree + branch + target/; ≤16 fixes/round;
  push after every round for durability; rebase-never-reset on re-entry; flock-guard any
  shared-tree apply phase.
- **Agent git discipline** — ADOPT verbatim into every wf prompt: "Never git
  reset/checkout/stash/rebase/pull. Commit explicit paths:
  `git -c core.hooksPath=/dev/null add '<paths>' && git commit -q`. NO push." (Orchestrator
  pushes; cycle-break agents got "NEVER run git.")
- **Main-parity workflow** — ADOPT (TL;DR #5). Per upstream .zig commit: read the diff,
  map to the .rs, verdict with file:line evidence, 2-vote verify, port the missing —
  including the commit's test hunks. "A .zig→.rs git merge does NOT port the fix."
  <https://github.com/oven-sh/bun/blob/ed1a70f81708d7d137de8de057d11668c9f4e220/.claude/workflows/phase-h-main-parity.workflow.js>
- **Marker census → leverage-ranked backlog** — ADOPT for the C-F unimplemented audit
  endgame: categorize all markers (Bun: 7,120 across 6 categories), rank refactors by
  `unsafe_removed × affected_count / effort`, reframe as adoption sweeps of existing
  abstractions, and record explicit written REJECTIONS of tempting-but-unsound
  alternatives so agents don't re-propose them.
  <https://github.com/oven-sh/bun/blob/f6ad291ea0/docs/PORT_NOTES_PLAN.md>
- **Dedup sweeps** — ADOPT. Independent per-file agents re-port small shared helpers;
  schedule periodic canonical-helper sweeps with "−LOC, N sites" accounting, careful that
  dedup never drops ported comments/assertions from the canonical copy (R1).
- **Anti-justification-comment rule (phase-asymmetric)** — ADAPT. Early-phase markers are
  tracked debt (required); late-phase "a paragraph to justify a workaround" is a
  rejection signal ("fix the code instead"). Scope to NEW agent-authored prose only —
  ported Zig comments are untouchable under R1.
- **Chunked-write protocol** — ADOPT into wf prompts for big files: Write ~800 lines to a
  fn/impl boundary, then Edit-append with the exact last 2 lines as anchor; segmented
  Reads for >1800-LOC sources.
- **Oracle testing beats determinism** — ADOPT framing. "The port is going to rely
  entirely on oracle testing"; with a non-deterministic porter EVERY chunk needs its own
  verification event — rustty's per-chunk gates are the compensating control and must
  never be batched away (R5). <https://news.ycombinator.com/item?id=48021720>
- **Line-by-line as anti-drift device + differential fuzzing BEFORE deviation** — ADOPT.
  The fidelity instruction's real function is suppressing restructuring/guessing to
  maximize bug-compatibility; pair with differential fuzzing against the ghostty oracle
  on ported subsystems before any idiomatic deviation is considered.
- **Ladybird addendum: new Rust-side tests on top of ported ones** — ADOPT systematically:
  every fidelity-driven unsafe pattern (raw-ptr args, intrusive nodes) gets NEW Rust
  tests/Miri harnesses with no Zig counterpart (mirihar pattern, made mandatory).
- **Test blind spot / canary sessions** — ADAPT. Ported-tests-pass only covers what the
  original suite encodes (Bun's ICC-profile bug); rustty: differential-run rustty vs
  ghostty on shared corpora, and treat GPU/visual paths (no unit-gate per
  heavy-tier-strategy) as the designated blind spot needing manual comparison sessions.
- **Zig test-allocator leak gate must not be silently lost** — COVERED by design (R6
  LSan exists for this), sharpened: the leak check is part of the ORIGINAL tests'
  semantics, so dropping it is an R1 fidelity violation — verify LSan covers every ported
  test binary, not just FFI crates. <https://news.ycombinator.com/item?id=48029660>
- **Never trust model priors about Zig** — ADOPT. LLMs are markedly weaker at Zig than
  Rust; the porting risk is on the READING side. Resolve Zig std/comptime semantics from
  the vendored reference source at Ghostty's pinned Zig version (R3), never from memory.
  <https://news.ycombinator.com/item?id=48017606>
- **Tool-not-output for mechanical sublayers** — ADAPT selectively. A full transpiler is
  infeasible (comptime), but replayable generator scripts beat N one-shot translations
  for enum/table/keybind data, C-header bindings (sys-generated-config-header), corpora.
- **Walk-the-git-history porting** — REJECT as method (rustty ports the pinned state);
  ADAPT one piece: consult reference git blame when ported code contains bug-fix-shaped
  oddities — port it either way (R1), but history tells you which assertion/test should
  accompany it.
- **Cost economics** — ADOPT as planning input. ~700k lines ≈ $200-500 of generation:
  re-porting a flawed chunk is ~free, so never protect one out of sunk cost; budget in
  verification passes (Miri runs, review sub-batches), not lines or tokens.
  <https://news.ycombinator.com/item?id=48023157>
- **Side-by-side comparison snapshot per completed subsystem** — ADAPT from Jarred's
  end-state criteria: record a Zig-vs-Rust behavior/perf comparison against the oracle at
  each phase exit rather than deferring all comparison to project end.
- **Managed rollout** — ADAPT for release: canary/parity profile first (R2), pre-built
  issue-classification tooling, CI failure reporting distinguishes "new vs
  also-on-baseline" so oracle-side flakes don't burn porting time.

### 3.4 Memory & unsafe strategy

- **Reproducible-command census with exact reconciliation** — ADOPT (TL;DR #3). Ground
  truth is one committed rg command (`unsafe \{|unsafe fn|unsafe extern|unsafe impl|
  unsafe trait`, vendor excluded); the taxonomy must reconcile to it exactly
  (accounted_for == total, unaccounted == 0); comment false positives itemized, not
  chased. Commit as `.port/audits/count-unsafe.sh` + pinned-commit snapshots.
  <https://bun.com/bun-unsafe-audit>
- **Density per kLOC, split by gate tier** — ADOPT. Bun 13.7/kLOC → projected 4.2;
  rusty_v8 38.4, wasmtime 5.5. "Density tracks how close the code sits to a C boundary."
  rustty reports per crate tier: pure Miri-gated crates should trend wasmtime-like;
  *-sys crates are legitimately dense; one workspace number would hide the R6 boundary.
- **Single-line vs structural split** — ADOPT. 78% of Bun's blocks are single-line
  (one C call each) — cheap to audit; the dangerous residue is structural (raw `*mut Self`
  state machines, `&self`→`&mut` casts). Record block line-span distribution; weight
  structural sites separately — rustty's Page/Row/Cell raw-ptr helpers are the kind a
  raw count under-weights.
- **Origin taxonomy: the PORT, not the C boundary, is the biggest unsafe generator** —
  ADOPT with rustty origins {zig-port-idiom, ffi-boundary, raw-page-aliasing,
  callback-ctx, lifetime-erasure, uninit-buffer, marker-trait, perf}. Bun:
  zig-port-legacy 34% > ffi-boundary 30%. R1 guarantees the port-idiom origin dominates;
  measuring it shows fidelity ≠ permanent unsafety. rustty's two biggest structural
  patterns (raw *mut Self re-entrancy reborrows, &self→&mut casts — 2,750 of Bun's first
  3,531 measured sites) are exactly the hazards in rustty's memory notes — COVERED as
  hazard, ADOPT as named census category.
- **Five destinations with a hard 'fixed' bar** — ADOPT verbatim: made-safe/no-slowdown,
  made-safe/one-check ("a real bug becomes a clean crash instead of exploitable
  corruption"), made-safe/redesign, stays-unsafe/consolidated, stays-unsafe/correct.
  Bar: "A site counts as fixed only if safe code can't cause undefined behavior in a
  release build. Debug assertions don't qualify; neither do wrappers that just hide the
  invariant." (R5-compatible: a hiding wrapper is a weakened gate.)
- **Unsound-SAFE-function ledger** — ADOPT (TL;DR #3). Bun's five hand-verified unsound
  safe fns (lifetime-less `Header` holding `*const u8`; borrowing signature with
  ownership semantics; `unsafe impl Sync` "a lie to the type system";
  `as_mut(&self)->&mut Self`) were "invisible to any audit that greps for the unsafe
  keyword." rustty ledger: every safe fn/macro wrapping a Page/Row raw-ptr deref, every
  &self→&mut accessor, every lifetime-less pointer-holding struct; triage
  reachable-from-VT-stream / internal-only / theoretical; reviewed at every sub-project
  finale.
- **Soundness first, count second (S0)** — ADOPT. Fixing the unsound safe API surface
  changes nothing in the metric and always outranks count-reduction; count-reduction is
  never reported as the fix for a soundness bug.
- **RAII enabler lands first; migrate one type at a time under Miri** — ADOPT. Never
  ship a refcount handle without an owning RAII form; "the migration hazard is a
  double-free where the manual deref() and the new Drop both fire; converting one type
  at a time and running Miri catches it" — exactly the R6/mirihar workflow.
- **Wrappers relocate vs discharge** — ADOPT. Refuse credit for safe-looking wrappers
  whose soundness Miri can't demonstrate ("the aliasing obligation is relocated, not
  discharged"). Endorsed: `Cell<T>` for Copy scalars (same machine code, Miri-clean);
  release-checked borrow cell (overlap panics instead of aliasing) for non-Copy
  interleaves; otherwise honest `unsafe fn`. Direct prescription for Page/Row/Cell seams.
- **Four redesign recipes for borrows across re-entrant calls** — ADOPT as shared
  vocabulary when a Miri SB failure needs structure, not more raw pointers:
  (1) take-before-the-call, (2) split-the-struct so borrows are disjoint,
  (3) defer the re-entrant call (usually also fixes a behavioral re-entrancy bug),
  (4) externalize state into cells behind `&self`. Complements
  shaper-hook-sb-clean-interleave.
- **3-4 exhaustive classes per question, measured on a sample first** — ADAPT. Reuse
  Bun's three questions nearly verbatim (is the aliased field Copy? does the borrow
  cross an interleaving call? is the back-pointer's owner immortal / refcountable /
  mortal — Bun measured 69/27/3) and extrapolate from a measured sample to keep a full
  census affordable per sub-project. Steal ParentRef's debug-liveness-generation for
  mortal owners (Miri-visible, R6-compatible).
- **Audit review shape: 2 finders × 2 adversarial reviewers × adjudicator, publish
  agreement** — ADOPT via .port/phase-2 wf scripts; one finder's partition is the
  accounting basis; commit raw per-group reports; publish inter-classifier agreement
  ("Where agreement was low, trust the number less").
- **'Unsafe for performance' is ~1%** — ADOPT as review stance: `get_unchecked`/`set_len`
  justified only by "hot path" needs a profile or a closed-form release capacity assert;
  a higher-than-~1% perf-unsafe share is a smell.
- **Irreducible floor centralized in -sys crates; pure externs redeclared `safe fn`** —
  ADOPT. Audit each extern fn once as pure-vs-stateful; Rust-2024
  `unsafe extern { pub safe fn f(...) }` for pure ones (proof in the signature),
  stateful ones behind ONE owning handle type per C handle; "the call instruction itself
  is the irreducible floor." Shrinks the ASan-gated surface R6 must cover. Plus the
  counted review gate: `new_unsafe == 0` outside FFI, one safe wrapper per extern
  ("adding unsafe {} at N>2 call sites for the same extern is wrong"); workspace lints
  `unsafe_op_in_unsafe_fn=deny`, `clippy::undocumented_unsafe_blocks=deny`.
- **Memory-safety review rubric** — ADAPT Bun's CLAUDE.md checklist with rustty's
  re-entrancy sources (VT callbacks into apprt, C-API trampolines, GTK signal handlers)
  substituted for "user JS": pair every acquisition with its release at the acquisition
  site; re-audit above new early returns; "who frees this, when, on which paths, with
  which allocator" in one sentence; never let a pointer outlive its memory (named
  rejected shapes); refcounts provably balanced on every terminal path; "never add a ref
  just to silence ASAN — find the actual imbalance" (direct R5 corollary).
  <https://github.com/oven-sh/bun/blob/main/CLAUDE.md>
- **Clone-injection silently destroys perf** — ADOPT cheap check: grep-audit new
  `.clone()`/`.to_vec()`/`.to_owned()` per chunk against the Zig side's copy behavior;
  lean on phase-0 benchmark corpora for regression detection. The quiet failure mode of
  an agent that prefers compiling code over faithful code.
  <https://news.ycombinator.com/item?id=48023207>
- **GC-boundary rules** — REJECT the JSC specifics (no GC'd VM in Ghostty); ADAPT two
  shapes: foreign-runtime handles get newtype wrappers encoding !Send/!Sync and
  Drop-thread requirements in the type (ObjC/AppKit/Metal objects have exactly the
  drop-on-wrong-thread hazard), and "both halves required" pairing audits for
  resource-accounting APIs.
- **Post-merge unsoundness hunt** — ADAPT. Bun shipped passing tests yet still found
  atomic-cast UB and unsound free-APIs afterward; R6 catches these earlier but only on
  exercised paths — schedule a dedicated NOALIAS/PTR-audit pass (Bun: NOALIAS_HUNT,
  PTR_AUDIT) plus first-class endgame sweeps (phase-comment cleanup, marker backlog,
  per-crate dead-code deletion).
  <https://github.com/oven-sh/bun/commit/d4514457e837fc3c884496c70a90df555bb95221>

### 3.5 Crate structure & scale mechanics

- **Pure git-mv commits, must-not-squash** — ADOPT. File moves between crates (future
  font 3-tier split) land as: (1) pure `git mv`, zero content edits (blame/--follow
  survive at 100% similarity), (2) mechanical path fixups, (3) residual fixups; with an
  explicit `git log --follow` traceability check and declared non-goals.
  <https://github.com/oven-sh/bun/pull/30224>
- **Directory == crate before porting; deterministic path mapping owned by the
  orchestrator** — ADAPT. rustty can't restructure the external reference (R3), but the
  deterministic zig-path→rs-path function computed by the orchestrator ("do NOT let
  agents pick") is directly adoptable in wf scripts. rustty's crate map already plays
  the acyclic-area role (R4).
- **Cycle-breaking by exhaustive symbol classification** — ADAPT for resistant
  break-list entries: classify each symbol on the edge DELETE / FORWARD_DECL / TYPE_ONLY
  / MOVE_DOWN / GENUINE (Bun: 477 symbols across 174 edges); remember "tier collapse"
  (make it a module, not a crate) and "move-in extracts from the .zig, not the
  possibly-already-edited .rs." Hard agent scoping: read only your section, edit only
  your crate. <https://github.com/oven-sh/bun/blob/f6ad291ea0/docs/CYCLEBREAK.md>
- **Two-phase Phase A (non-compiling faithful draft)** — REJECT as rustty's main loop
  (R5/R6 require compiled, tested code per sub-batch; HN confirmed the failure mode:
  773k non-compiling lines nobody could evaluate, "very high chance all this code gets
  thrown out"); placeholder-leaf doctrine already gives the parallelism while staying
  green. Narrow ADAPT: a non-compiling faithful draft alongside an INHABITED placeholder
  may accelerate giant GPU/GTK leaf fills, since gates apply to what's wired into the
  workspace. <https://news.ycombinator.com/item?id=48022618>
- **Strangler-fig plan designed then abandoned** — ADAPT. Validates rustty's
  whole-port choice; salvage the surviving gate set for the finale (cross-target
  cargo-check matrix, perf-neutrality bench gate, binary-size tracking, cargo-tree
  layering assert in CI) and "kill criteria by hour N" as an honest per-sub-project
  checkpoint device.
  <https://github.com/oven-sh/bun/blob/f6ad291ea0/docs/rust-rewrite-plan.md>
- **CI realities: cargo does not cross-compile like Zig** — ADAPT. Zig bundles
  libc+SDK per target; cargo delegates to host C toolchains, so rustty's -sys deps won't
  cross-compile the way ghostty's `zig build` does. Keep a cargo-check matrix for
  cfg-gated code ("#[cfg]-gated code is not type-checked unless the matching target is
  built") even for targets not shipped.
  <https://github.com/oven-sh/bun/blob/f6ad291ea0/docs/CI_RUST_MIGRATION.md>
- **Reference lifecycle + oracle-retirement checklist** — COVERED by R3 (external
  oracle is the stronger form); ADOPT the exit checklist: before ever declaring the port
  self-contained, machine-verify no build script, test, or doc-include reads the ghostty
  checkout; scaffolding removal happens as one atomic commit recoverable at a pinned SHA.
- **Compile-time cost absorbed by the crate graph** — COVERED. R4's acyclic many-crate
  layering is the mitigation (parallel, cacheable units); ADAPT: keep third-party deps
  and proc-macros minimal in hot-path crates so `cargo build --workspace` stays fast at
  Ghostty scale. <https://news.ycombinator.com/item?id=48019067>
- **FFI hygiene details** — ADOPT: tag-and-payload structs the C side mutates stay
  `#[repr(C)] { tag: u8, value: ... }`, NEVER a Rust enum (niche/discriminant layout is
  not the C contract — R2 parity); externs live in -sys crates with TODO(port)+sweep for
  stragglers; ABI-in-macro-position is impossible — per-platform callconv needs an
  attribute proc-macro (saves a dead end if apprt ever needs it).

### 3.6 Risks & critiques (external signal)

- **Phase-A output was "highly unidiomatic... with a ton of unsafe"; 'clean up later'
  never got a plan** — COVERED with a caveat: rustty accepts fidelity-first unsafe (R1)
  but Miri/ASan-gates every chunk (R6); internalize that cleanup must be designed
  per-pattern at port time (as rustty's raw-ptr-arg and raw-self conventions were), not
  postponed globally. <https://news.ycombinator.com/item?id=48017933>
- **Safety requires NON-LOCAL refactoring; flag paradigm-clash points during the port** —
  ADAPT. "Making things safe often requires refactoring not localized to a single
  function"; adopt the habit of flagging clash points at port time rather than trusting
  a later idiomatic pass. <https://news.ycombinator.com/item?id=48018756>
- **The two genuinely hard gaps: pointer aliasing and comptime** — COVERED for aliasing
  (rustty solved it via raw-ptr args + isolated Miri harnesses); ADAPT for comptime:
  keep the per-instance rendering decision log (§3.1 comptime ladder).
  <https://news.ycombinator.com/item?id=48019281>
- **Memory-representation choice is a 5-10× difficulty lever** — COVERED empirically;
  reusable rule: settle a subsystem's memory convention in a small Miri-proven pilot
  before mass-porting it (rustty already proves the riskiest seam first per tier).
  <https://news.ycombinator.com/item?id=48017792>
- **Mega-diffs are unreviewable; review-at-porting-time is the only workable shape** —
  COVERED: per-subbatch-scoped-g3b exists precisely for this; strongest external
  validation yet that R5 must hold against the temptation of bulk phases.
  <https://news.ycombinator.com/item?id=48021351>
- **Reviewer proficiency must match the TARGET language** — ADAPT: in rustty, "review
  competence" = the mechanical gates (Miri strict-provenance, cycle_check, ASan/LSan) +
  scoped review; hand-reasoning about unsafe aliasing is NOT adequate review (matches
  the shaper-hook lesson: prove on a harness). <https://news.ycombinator.com/item?id=48018742>
- **GuB-42 baseline risk framing** — COVERED; keep as the one-line justification for
  refusing 'improvements' during porting: "AI can save a lot of time when making
  ports... but it doesn't protect from regressions" — every deviation from the working
  original is an unforced regression opportunity.
  <https://news.ycombinator.com/item?id=48017453>
- **The porting bible was never committed** — COVERED in substance (PORT_RULES lives
  in-repo); ADOPT the good half: §-section citations in code comments pointing at the
  rules doc; never let the cited doc live outside version control where citations rot.
  <https://github.com/oven-sh/bun/blob/main/src/ptr/lib.rs>
- **Style guides fail at enforcement; machine gates replace prose** — COVERED (R4-R6
  posture); transferable test: any rule that matters must be expressible as a command
  that fails CI, otherwise it decays. <https://bun.com/blog/bun-in-rust>
- **Pilot-subsystem evaluation** — COVERED (decision locked; bottom-up small-boundary
  sequence IS this shape); keep "prove the riskiest seam first" explicit in heavy-tier
  planning.

---

## 4. Semantic-mismatch detector spec

Concrete automated checks to add, each mapped to the Bun regression class it kills and
its hook into `.port/tools/ci.sh`. All are additive gates (R5-clean). Proposed home:
`.port/tools/gates/` scripts invoked from ci.sh, plus test conventions.

**D1 — slice-reinterpret audit** (kills Class 1, #31188).
Grep: `rg -n 'bytemuck::cast_slice|align_to|from_raw_parts.*as \*(const|mut) u(16|32|64)|bytes_of|cast_slice_mut' crates/`
against an allowlist file `.port/audits/slice-reinterpret.tsv` where every hit carries a
per-site decision (`truncate` | `error` | `exact-len-proven`) citing the Zig origin
line, and every `truncate`/`error` site names its odd/misaligned-length unit test.
Unlisted hit ⇒ gate fails. Hook: ci.sh static-checks stage, before cargo test.

**D2 — capacity-saturation tests** (kills Class 2a, #31503).
One-time audited inventory of every `[T; N]` + monotonic-counter pair (write-then-
increment order vs N; array length derived from the same expression the Zig counter
range implies, e.g. `IntFittingRange(0, max+1)` ⇒ `[T; MAX+1]`). Convention:
`#[test] fn <name>_capacity_saturation()` drives every bounded store to its ceiling.
Gate: inventory file requires a named test per entry. Triage rule (process, not script):
a Rust panic where Zig "worked" ⇒ re-run the ghostty oracle in ReleaseSafe/Debug first.
Hook: normal cargo-test lane; inventory check in ci.sh static-checks.

**D3 — constants-by-expression parity** (kills Class 2b).
(a) Grep gate: zero `TODO\(port\)|FIXME\(port\)|placeholder` markers in any crate at its
sub-batch gate (scoped, per per-subbatch-scoped-g3b — never deferred to finale).
(b) Constants-parity script: extract numeric consts from the Zig reference file (phase-0
inventory has the paths) and diff against the Rust twin; derived constants must appear
as the same const EXPRESSION, not a folded literal. Hook: ci.sh gate stage per sub-batch;
completeness-critic input.

**D4 — comptime-phase preservation + golden VT bytes** (kills Class 3, #30693).
(a) Per-chunk inventory: every `comptime` parameter in the chunk's Zig files must map to
a Rust const/generic/macro; a runtime-fn port is a flagged deviation requiring a written
grammar-collision argument ("no runtime bytes can alias the template grammar").
(b) Golden byte tests on ALL emitted escape/format output: byte-exact expected strings +
an ST-integrity assertion helper (every OSC/DCS/APC opener followed by a correct
terminator; no stray bytes after tag processing) run over the reply corpus. Hook:
(a) completeness-critic checklist item; (b) cargo-test lane in rustty-vt and any crate
emitting VT bytes.

**D5 — paired-invariant catalog** (kills Class 4, #30678).
Per sub-project file `.port/phase-2/<tier>_INVARIANTS.md`: every predicate/flag
consulted at 2+ emission/consumption sites across crates is a named invariant with
(producer assert, consumer graceful-degrade, one integration test through the seam).
Known rustty pairs to seed: dirty-tracking↔renderer damage; style-id refcounts
Page↔style set; kitty placements↔renderer atlas; mode state↔key encoder. Gate: review
checklist requires the catalog updated when a chunk touches either end. Hook: G3b-style
review gate; integration tests in the cargo-test lane.

**D6 — sanitizer-visibility canary + cfg-mapping audit** (kills Class 5, #30875).
(a) Canary tests compiled only in sanitizer builds: one deliberately leaks, one
heap-overflows; CI asserts the ASan/LSan lane FAILS on them (then filters them out of
the pass/fail roll-up) — proving the gate can see the port's allocations. If rustty ever
installs a custom global allocator or links an allocator via a C dep, this becomes
mandatory before the next merge.
(b) Grep audit: every `builtin.mode`/build-option/comptime-switch in the chunk's Zig
maps to a `cfg`, never a constant-folded value; flattened switches are gate failures.
(c) Sanitizer suppression entries require a per-entry WHY comment; unexplained entries
fail. Hook: ASan/LSan lanes in ci.sh; (b) in the completeness critic.

**D7 — unsafe census + unsound-safe ledger** (kills unsafe sprawl + the invisible class).
`.port/audits/count-unsafe.sh`: the exact Bun rg command, per-crate density per kLOC
split by R6 tier, block line-span distribution, snapshot JSON pinned to the commit,
sum-over-categories == total. Lint: every unsafe block carries
`// SAFETY(<origin>/<dest>): <one-sentence owner/aliasing contract>` — missing tag fails
the script. Parallel ledger `.port/audits/UNSOUND_SAFE.md`: safe fns/macros wrapping
raw-ptr derefs, &self→&mut accessors, lifetime-less pointer-holding structs; triaged
reachable-from-VT-stream / internal / theoretical. Monotone rule: per-crate density may
only decrease or stay flat between snapshots; an increase requires a written
origin/dest justification in the change. Cadence: snapshot at every sub-project finale.
Hook: ci.sh audit stage (advisory per-commit, blocking at finale gates).

**D8 — clone-injection audit** (kills silent perf regression; HN selfmodruntime).
Per-chunk grep of NEW `.clone()|.to_vec()|.to_owned()|.into_owned()` in the diff,
cross-checked against the Zig side's copy behavior; unexplained copies flagged in review;
phase-0 benchmark corpora as the quantitative backstop. Hook: review-gate checklist +
bench lane at tier finales.

**D9 — stale-comment contract check** (kills the cross-cutting comment hazard).
Review rule + spot-grep: wherever a Zig builtin was replaced by a Rust primitive with
different failure semantics (reinterpretSlice→cast_slice, catch unreachable→expect,
wrapping/saturating ops), the adjacent ported comment must state the RUST primitive's
contract; any comment of the form "len/align is guaranteed X" near a panicking cast
requires a test asserting X. Hook: completeness-critic checklist item (comments are
already inventoried per R1).

---

## 5. Proposed PORT_RULES amendments — **PROPOSED, NOT YET AGREED**

Draft text for discussion. Numbering continues from R6. Each is designed to only
tighten (R5-compatible).

### PROPOSED R7 — Replacement-primitive semantic fidelity

When a Zig builtin/idiom is replaced by a Rust primitive with different edge semantics,
the divergence is decided explicitly per site, never inherited silently:

- **Narrowing:** `@intCast` → `T::try_from(x).unwrap()`. Bare `as` is reserved for
  `@truncate` (intentional wrap). A proven-hot loop may use `as` only with
  `// PERF(port): @intCast` and a benchmark citation.
- **Wrapping/saturating:** `+%`/`-%`/`*%` → `wrapping_*`; `+|`/`-|` → `saturating_*`.
  Never bare operators for these (debug panic / release wrap is neither Zig semantic).
- **Float→int:** `@intFromFloat` → `as` (Rust saturates where Zig is UB). Keep the Zig
  code's prior range checks; do not add new ones.
- **Slice reinterprets:** every site records truncate-vs-error and carries an
  odd/misaligned-length test (detector D1).
- **Multi-object loops:** `for (a, b) |x, y|` ports with
  `debug_assert_eq!(a.len(), b.len())` — the length assert is part of the source
  semantics under R1, because `.zip()` silently truncates.
- **Asserts:** `std.debug.assert` → `debug_assert!`; `unreachable` → `unreachable!()`
  (checked); `catch unreachable` → `.expect(...)`. `unreachable_unchecked`/
  `unwrap_unchecked` are forbidden until the invariant is proven (Miri harness or
  closed-form argument), then marked `// PERF(port)`. Assertions are side-effect-free;
  a `debug_assert!` never discharges a safety obligation and never validates untrusted
  input — VT-stream and config validation is release-checked.
- **Comments:** a ported comment describing the OLD primitive's behavior is rewritten to
  the Rust primitive's contract (fidelity to intent over stale text); flagged in review
  (detector D9).

### PROPOSED R8 — Comptime-phase and build-switch preservation

- A Zig `comptime` parameter maps to a Rust const/generic/macro (ladder: plain generic >
  const generic > macro). Porting one to a runtime function is a flagged deviation
  requiring a written argument that no runtime bytes can alias the compile-time grammar
  (detector D4), plus golden byte tests on any emitted escape/format output.
- Zig build-mode/option switches (`builtin.mode`, build options, sanitizer/allocator
  toggles) port as `cfg`, NEVER constant-folded to one profile's value. Allocator and
  instrumentation plumbing is the named highest-risk case (Bun Class 5).
- Derived constants port as the same const EXPRESSION as the Zig source, never a folded
  literal; zero placeholder constants survive a sub-batch gate (detector D3).
- Sanitizer lanes carry self-test canaries (intentional leak + overflow must fail)
  whenever the global allocator or instrumentation configuration changes (detector D6).

### PROPOSED R9 — Unsafe accounting and the unsound-safe ledger

- Ground truth: `.port/audits/count-unsafe.sh` (committed rg command); snapshots pinned
  to commits, categories reconcile exactly to the total.
- Every unsafe block carries `// SAFETY(<origin>/<dest>): <owner/aliasing contract>`
  using the fixed origin set {zig-port-idiom, ffi-boundary, raw-page-aliasing,
  callback-ctx, lifetime-erasure, uninit-buffer, marker-trait, perf} and Bun's five
  destinations. Missing tags fail the audit script.
- "Fixed" bar: safe code cannot cause UB in a release build; debug assertions do not
  qualify; wrappers that hide the invariant do not qualify.
- A parallel unsound-safe ledger tracks every safe wrapper of a raw-ptr deref,
  &self→&mut accessor, and lifetime-less pointer-holding struct, triaged
  reachable-from-VT-stream / internal / theoretical; soundness fixes outrank
  count-reduction, always.
- Per-crate unsafe density is monotone non-increasing between snapshots; an increase
  requires a written justification in the introducing change. Reviews count
  `new_unsafe`; acceptance outside -sys/FFI crates requires `new_unsafe == 0`; each
  extern fn gets exactly one safe wrapper (pure externs may be `safe fn`
  redeclarations after an individual pure-vs-stateful review).
- Census cadence: snapshot + unsound-safe pass at every sub-project finale; RAII
  migrations convert one type at a time under the owning crate's Miri/ASan harness.

### PROPOSED R10 — Marker and perf-intent discipline

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
  placeholder-leaf doctrine and its dated C-F endgame audit.

### PROPOSED R11 — Adversarial verification protocol for bulk classification & review

- Split contexts: implementer, verifier, and fixer are separate agents; the verifier
  works a concrete violation checklist keyed to PORT_RULES and defaults to
  not-ok-if-uncertain; the fixer makes surgical edits only and may skip a hallucinated
  finding with a note; the orchestrator owns all path/naming decisions and all pushes.
- Bulk classifications (pointer fields, comment coverage, SAFETY tags, upstream-parity
  verdicts) require per-claim evidence in the form "file:line — what you saw"; every
  low-confidence claim plus a fixed random sample (~10-15%) of high-confidence claims
  receives 2-3 independent adversarial votes with default-refute polarity; majority
  overturns. Inter-classifier agreement is published with the artifact.
- Standing artifact: GHOSTTY_LIFETIMES.tsv — every pointer-typed struct field in the
  remaining pointer-heavy tiers classified once (Bun's 11-class taxonomy + GHOSTTY_C),
  consumed verbatim by port agents ("trust it over local guessing"); UNKNOWN ⇒
  `Option<NonNull<T>>` + TODO(port).
- Swarm hygiene: one expensive pass (build/Miri) per round with read-only fix agents;
  contiguous-slice sharding; capped fixes per round; push per round; agents never run
  destructive git and commit explicit pathspecs with hooks disabled.

### PROPOSED R12 — Upstream parity tracking

- A standing main-parity workflow enumerates every upstream ghostty commit (since the
  pinned reference SHA) touching ported files; each gets a verdict
  ported/missing/partial/not-applicable with file:line evidence and 2-vote adversarial
  verification; missing ones are ported WITH the same commit's test hunks.
- Doctrine: updating the reference checkout does not port anything — "a merge does NOT
  port the fix; only an explicit .rs change does."
- Every escaped defect gets `tests/regression/issue_NNN.rs` with a documented fail-first
  proof, and (for oracle-differential tests) proof the test fails against the
  pre-fix/oracle side.

---

*End of document. Section 5 requires explicit agreement before any of R7–R12 becomes
binding; sections 1–4 are actionable as workflow/tooling changes under existing R1–R6.*
