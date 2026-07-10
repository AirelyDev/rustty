# rustty Validation Report #1 — Full-Port Audit (2026-07-10)

Scope: full-fidelity Zig→Rust port of Ghostty (rules in `.port/PORT_RULES.md`; parity contract in
`.port/phase-0/plan/DEFINITION_OF_PARITY.md`). Eleven specialist auditors (test fidelity, breadth
fidelity, ledger integrity, unsafe soundness, FFI soundness, numeric semantics,
assertion/format/comptime semantics, two line-level drift walks, test-suite quality, workspace
hygiene, gate machinery) plus adversarial verification of every blocker/high candidate. All audits
were **static** — the workspace target-dir was locked, so no cargo/Miri/ASan execution informed any
conclusion below except read-only Python gate runs and shell-level gate execution.

---

## 1. Verdict

**The port itself is in remarkably good health; the machinery that is supposed to prove that is
not.** Line-level walks of the VT parser front half and the screen/page back half found zero
wrong-behavior drift; the 3,526-test suite shows exact assertion parity on every sampled pair
(75+ deep-compared tests, zero transcription errors); every substantive Zig terminal test exists in
Rust with a ~+35 surplus; the dependency graph is acyclic, self-contained, and vendored-identical
where verifiable. But the verification layer is currently **not binding**: ci.sh is RED today
(G1), the only HARD differential gate (G4) is structurally unarmable and passes while checking
nothing about rustty, G2's per-test assertion check silently compares almost nothing, G6/G3b skip
three claimed-done crates through a status-string mismatch, GitHub CI cannot pass as configured,
and the crown-jewel crate self-declares "stub" so progress-mode gates never bind on 62% of the
port. **The single most important risk: the project's confidence currently rests on porting
discipline and manual review, not on its gates — a future regression in rustty-vt (assertion
weakening, comment drops, differential divergence) would today pass every automated check.** Fix
the gate layer before porting further. One genuine runtime-soundness defect exists
(rustty-vaxis `Loop::stop()` SB/TB protector UB on the interactive-TUI exit path) and should be
fixed with the already-proven mirihar methodology.

---

## 2. Confirmed findings (adversarially verified, ranked)

Severities below are post-verification (corrected) severities. Rules: R1 = full fidelity
(lines/comments/tests), R3 = self-containment, R4 = acyclic layering, R5 = never weaken gates,
R6 = memory-safety proof (Miri/ASan).

### BLOCKER

**B1. G4 differential gate cannot arm — it keys on a phantom binary and calls comparator.py with
flags that don't exist; today it passes while checking nothing about rustty.**
`.port/tools/gates/run_differential.sh:57,88,98-99` — violates **R5** (DoP gate-contract breach).
- The "real" arm triggers only if `target/release/rustty-c-vt-runner` exists — no such bin target
  exists anywhere in the workspace (rustty-vt-c is cdylib/staticlib/rlib only; the name exists only
  in docs). Even if it existed, the invocation passes `--case/--rust-bin/--ref-golden`, none of
  which `comparator.py` (real CLI: `input --ref --port ...`) accepts — argparse exit 2 per case,
  masked by `>/dev/null 2>&1`.
- The arming path that DOES exist today (`build_port_driver.sh` → `harness/bin/vt_driver_port`;
  `target/debug/librustty_vt_c.a` is present and exports 162 unmangled `rustty_*` symbols) is never
  checked. Worse, once the port driver emits output, `gate_empty_divergence.sh` (asserts ~100%
  divergence) FAILS → run_differential.sh exit 3 → ci.sh records a mere WARNING. The documented
  "auto-flips to the real byte-exact gate" (ci.sh:45-46, CI.md) is false: the flip lands on
  WARNING, never RED/GREEN.
- Failure scenario: rustty-vt diverges byte-wise from ghostty on any fixture; every CI run stays
  green ("HARNESS-WIRED (port empty)") — with 2,032 vt tests ported, "port empty" is dramatically
  stale.
- Fix: build librustty_vt_c + vt_driver_port; re-key the arm condition on
  vt_driver_port/RUSTTY_VT_DRIVER; drive comparator.py with its real CLI over
  `harness/fixtures/*.vt`; add a cvt-corpus runner for the 21 c-vt golden cases; demote
  gate_empty_divergence to the truly-empty branch so a present-but-diverging port is RED, not
  BLOCKED.

**B2. ci.sh is RED right now: G1 exits 1 — bench (claimed done) genuinely dropped a contract test;
args/extra/termio fail the name-set diff.**
`.port/tools/gates/check_tests_ported.py` (live run); `crates/rustty-bench/src/lib.rs:14` —
violates **R1** (contract count breach on a claimed-done crate) and **R5** (permanently-red gate).
- Live: bench 7/8 RED (genuine — see H3), args 65/64, extra 10/2, termio 47/34 RED on false-MISSING
  matcher artifacts (see H2/H4). ci.sh classifies a RED progress gate as HARD failure; an honest
  full-green run is impossible today even ignoring cargo steps.
- Fix: port benchmark/main.zig's anonymous refAllDecls block as a `ref_all_decls` #[test]
  (bench → 8/8), then fix the name matcher (H4). That alone flips G1 green in progress mode.

### HIGH

**H1. rustty-vaxis `Loop::stop()` forms `&mut Tty` / `&Vaxis` on the main thread while the read
thread concurrently uses the same allocations — SB/TB protected-tag UB on the shipped
interactive-TUI exit path.**
`crates/rustty-vaxis/src/loop_.rs:206-211` (vs `tty.rs:229,239`, `vaxis.rs:1138`) — violates
**R6**.
- stop() runs before `handle.join()` (deliberately, to wake the blocked reader) and calls
  `(*tty).writer()` — a whole-Tty `&mut self` Unique retag — while the read thread is by design
  blocked inside `Tty::read(&self)` holding a call-protected shared borrow of the same Tty;
  `device_status_report(&self)` holds a protected `&Vaxis` while the read thread may do plain
  writes to `(*vx).caps.*`. Under Stacked Borrows (the project's R6 gate) this is protector-
  violation UB, exercised on every interactive `list_themes` exit. Verification correction: it is
  SB/TB abstract-machine UB, not a hardware data race (DSR reads no Vaxis field) — still a
  soundness-contract breach Miri would flag. The mirihar `loopthread_shipped` proof does NOT cover
  this overlap (it models tty/vaxis as bare `*mut u32` and serializes via channel handshakes), so
  MIRI_EXCEPTIONS.md:708's "zero unsafe path unverified" claim is falsified by this site. The port
  already un-faithfully fixed the same class once (loop_.rs:27-38), so keeping this one is
  inconsistent by the project's own standard.
- Fix: project the writer via `addr_of_mut!((*tty).tty_writer)` without a whole `&mut Tty` (or
  replace the DSR wake with a self-pipe/quit-fd), then extend mirihar-chunk3b to model the
  stop()-vs-read overlap and re-prove.

**H2. G1's name matcher cannot match Zig names mixing camelCase with punctuation — the live
MISSING list is dominated by false positives, training everyone to ignore the R1 backstop.**
`.port/tools/gates/_gatelib.py:325` (name_candidates) — violates **R5** (gate neutralized;
fails-red, so no false green, but the real bench miss hides in the noise).
- `name_candidates()` splits only on `[-_ ]`, keeps colons, never camel-splits inside
  space-separated parts: 'parseIntoField: bool' never yields `parse_into_field_bool`; 'bash: ENV'
  never yields `bash_env`. All flagged pairs verified ported on both sides (printed as EXTRA).
- Fix: compose the transforms — replace non-word runs with `_`, camel-split each token, lowercase;
  re-run G1; whatever remains MISSING is a genuine R1 gap.

**H3. bench crate claims done but genuinely dropped 1 of its 8 contract test blocks
(benchmark/main.zig's anonymous refAllDecls).**
`crates/rustty-bench/src/lib.rs:14` — violates **R1** (only claimed-done crate with
ported < contract) and leaves G1 permanently RED (**R5**: R5 forbids editing the contract instead).
- The "no Rust analogue" rationale is contradicted by 30 `ref_all_decls` #[test] fns across the
  workspace (crash, vt/kitty, unicode, wuffs, zf, ...). DEFERRALS.md:3288 documents the drop but a
  documented drop that leaves CI permanently red is not handled.
- Fix: add a ref_all_decls-style #[test] referencing each pub mod — bench hits 8/8, no contract
  edit.

**H4. G1's name_candidates does not generate the port's actual slug convention — flipping any big
crate to done produces mass false MISSING (vt: 1,170 of 1,970 names; font 94; config 92; input 60).**
`.port/tools/gates/_gatelib.py:325-355` — violates **R5** (blocks honest done-claims workspace-wide;
creates structural pressure to never claim done).
- Verified by direct execution: the port's uniform slug ('DynamicPalette: changeDefault preserves
  changes' → `dynamic_palette_change_default_preserves_changes`, exists at color.rs:1392) is never
  in the candidate set. check_parity_surface.py's `id_variants` has the identical flaw. Spot-checks
  show even non-camel-rescuable names ARE ported — essentially all 1,170 are false MISSING.
- Fix (single highest-leverage gate fix): add ONE candidate form — non-alnum→space, camel-split,
  lowercase, join `_`. A matcher correction, not an R5 weakening (counts still enforced). Also
  rename `extra::get_description_test` → `get_description`.

**H5. Done-detection is inconsistent across gates: G6 (check_leaks.sh) and G3b
(check_comment_coverage.py) grep literal 'done' only — rustty-args, rustty-termio, rustty-wuffs
(status 'ported', claimed-done per _gatelib) silently escape the memory-safety and comment-content
gates; --all-done never visits crates-sys/.**
`.port/tools/gates/check_leaks.sh:71-75`; `check_comment_coverage.py:142-145` — violates **R5**
(breaks ci.sh's own claimed-done-goes-RED promise) and weakens **R6** coverage (termio: 233 unsafe
occurrences; wuffs: 27 — no automated Miri/ASan).
- Fix: make both scripts consume `_gatelib.is_claimed_done` (G3b: one-line import; check_leaks:
  widen the grep to done|complete|parity|ported|final and probe src/main.rs); iterate
  crates-sys/*/Cargo.toml or delete the unreachable FFI_CRATES sys entries. Note: rustty-wuffs
  wraps C FFI but is absent from FFI_CRATES — route it to ASan, not Miri, when widening.

**H6. G2's per-test WEAKENED check is vacuous for nearly all tests: it requires the Rust fn name to
EQUAL the raw Zig string name — impossible for string-named tests (94.6% of the reference).**
`.port/tools/gates/check_assertions.py:124` — violates **R5** (DoP §G2 promises per-test
assertion-for-assertion comparison; only 43/3,065 named ref tests are actually compared today; a
claimed-done crate could gut one test's assertions and stay green if the crate total holds).
- Fix: resolve ref-name → ported-fn via the (fixed, H4) name_candidates mapping before comparing;
  keep >= semantics.

**H7. GitHub CI cannot pass ci.sh as configured: stable-only toolchain makes G6 a HARD failure
(exit 2 = "claimed-done regression"), no zig makes G4-uni permanently BLOCKED-as-warning, and
vt_driver_ref is not git-tracked so G4 is BLOCKED even on the macOS leg that claims to enforce it
HARD.**
`.github/workflows/ci.yml:44-50`; `check_leaks.sh:29-31` — violates **R5** ("CI is REAL" claim at
ci.sh:7 is currently false everywhere; the latest completed run is FAIL on both legs, G1+G6).
- Also: ci.yml's "G4 reports PROGRESS (exit 78)" comment matches no script; CI.md documents
  missing-miri as BLOCKED (exit 3) but the script exits 2 — undocumented divergence.
- Fix: install nightly+miri and pinned zig in ci.yml; make require_miri exit 3 (matching documented
  semantics) or treat miri as a hard install; decide vt_driver_ref policy honestly (R3 keeps the
  ghostty oracle off CI → document G4 as local-only, don't pretend the macOS leg runs it).

### MEDIUM (confirmed)

**M1. rustty-vt PORT STATUS: "stub / Scaffold only — no real logic yet" on the crown-jewel crate
(2,034 tests, ~62% of the port).** `crates/rustty-vt/src/lib.rs:7` — **R1/R5-adjacent ledger
drift.** The header is factually false and self-contradictory (line 5: "crown jewel, 2001 ported
tests"). Verification refuted the "permanent gate evasion" framing: --strict binds regardless of
status, check_test_counts.py binds per-module today (GREEN), scoped G3b bypasses the done-trigger,
and an honest done-flip is currently impossible without the H4 matcher fix (1,170 false MISSING).
The gate exemption is documented progress-mode design, not evasion — but the header misleads every
auditor and must be flipped once H4 lands. Fix: after H4, flip to done/ported (imguiKey seam noted,
DEFERRALS.md:231) and run G2/G3/G3b.

**M2. rustty-config header claims only C1 leaf value-types are ported; the project's own plan says
the config cluster is COMPLETE (C1-C3 ✅, 190 tests, config_struct.rs ≈ 12k lines).**
`crates/rustty-config/src/lib.rs:7` — **status drift**; token 'C1' keeps progress gates
non-binding. No missing test/assertion found (188 > 167 contract). Fix: accurate status +
done-flip after H4.

**M3. rustty-vt-c header says heavy c/ files "land in later sub-batches" — all 34 modules landed
(308 tests, zero live todo!); terminal.rs:823/:1647 comments falsely claim todo!()s remain;
GAP_ANALYSIS.md:153 still lists the get/set stubs as open.**
`crates/rustty-vt-c/src/lib.rs:8` — **status drift** (crate exceeds contract; completion ledgered
at DEFERRALS.md:150 but gates read lib.rs). Fix: update header + the two stale comments + GAP §3.8.

**M4. Two windowed-present KATs stay #[ignore]d citing "Metal::loop_enter ... still
unimplemented!()" — false since W5 (metal_impl.rs:493 is live); MIRI_EXCEPTIONS.md:659-672
explicitly instructs un-ignoring them; W5_DESIGN §7's "no test gets un-ignored" was a mis-scoped
check that only looked at the renderer crate.**
`crates/rustty-apprt-c/tests/macos_window_host.rs:313,377` — **R5-adjacent** (missed documented
un-ignore obligation; the apprt-c live-render-thread first-pixels KAT is unexercised while the
milestone was declared complete; partial compensation exists via the headless
loop_enter_display_callback KAT at generic.rs:6553). Fix: un-ignore both and run on a Metal host —
if they fail, that is the signal the ignore was suppressing.

**M5. rustty-vaxis (claimed done) carries 4 live unimplemented!() seams; the two zigimg seams cite
"(DEFERRALS.md)" but DEFERRALS.md has no entry (vaxis coverage ends at Chunk 3b), and
Image::draw/cell_size's "later chunk" rationale is stale (the chunk sequence is finished).**
`crates/rustty-vaxis/src/vaxis.rs:997,1018`; `image.rs:223,248` — **ledger drift** (false in-code
hard-tracker citation; seams tracked only in design docs; unreachable by current deliverables, no
R1 loss — verbatim Zig bodies preserved). Fix: add a vaxis 4a/4b DEFERRALS section covering all
four; fix the lib.rs "only remaining" sentence.

**M6. No `[profile.release]` overflow-checks anywhere: every plain-arithmetic Zig ReleaseSafe trap
becomes a silent wrap in rustty release builds (systemic).**
Root `Cargo.toml` (no [profile.*]) — **drift from the port's own written doctrine**
(BUN_PORT_LEARNINGS.md:127/:733). Verification correction: ghostty's PRIMARY macOS artifacts (tip
main build, lib-vt xcframework, tagged/snap/flatpak) are ReleaseFast where the same overflow is UB,
so Rust's defined wrap is not worse than the mirrored artifact — but the gap is undocumented,
release builds are already exercised (build_port_driver.sh:75), and the fix is one line. Fix: add
`[profile.release] overflow-checks = true` (strictly safer than ReleaseFast, trap-parity with
ReleaseSafe), optionally a `release-fast` profile mirroring the tagged artifact; record the fork in
PORT_RULES.md. Interacts with the debug_assert/kitty-multiply/divExact items in §3.

**M7. G2's assertion-parity bar is heavily diluted: the reference counts only testing.expect*,
while the Rust counter also counts every .unwrap()/.expect()/`?` — 40% of tests could lose ALL real
assertions and still pass rust>=ref; ~70% could lose at least one.**
`.port/tools/gates/_gatelib.py:358-373` — **R5** (weaker-than-documented safety net; the DoP G2
prose promises "fewer assertions is a blocker"). The suite's actual fidelity is currently
maintained by porting discipline, not this gate (all samples at exact parity). Fix: count only
assert!-family (+ unwrap_err/expect_err/should_panic) on the Rust side, or subtract a per-test
baseline equal to the Zig body's non-testing `try` count (available in tests.jsonl).

**M8. R6 automation gap beyond H5: miri-run.sh and sanitizers.sh are single-crate manual harnesses;
the only automated pass (G6 --all-done) covers 18 crates. The by-design exclusions (in-progress
crates) are documented; the real holes are the 'ported'-status trio (H5), the structurally
unreachable crates-sys/*, and rustty-vt's stale 'stub' keeping ~5,129 unsafe sites out of the
automated set.**
`.port/tools/miri-run.sh:17`; `sanitizers.sh:19` — **R6 coverage drift.** Fix: add an
--all-done-style enumerator driven by _gatelib with an explicit FFI→ASan / pure→Miri split
covering crates/ and crates-sys/; wire MIRI_EXCEPTIONS as the only allowed skip set.

---

## 3. Medium/low ledger (unverified as reported by auditors; grouped, compact)

### 3a. Gate machinery (feeds the §2 gate cluster)
- **G8 vacuously green forever**: ci.sh:88 passes no --metrics; no candidate emitter exists; the
  perf/size contract has never been compared since phase-1. Fix: rustty-bench emits BASELINE-shaped
  JSON; mark UNARMED until then. (medium)
- **G5 fuzz gate permanently "FUZZ-WIRED"**: no crates/rustty-vt/fuzz dir, cargo-fuzz not
  installed, no self-flip mechanism at all. (medium)
- **G1 --strict requires ported == contract EXACTLY**, but sanctioned style adds extra tests
  everywhere (workspace 3,244 > 3,227) — the final locked gate is unsatisfiable as written. Decide
  now (calm, not at lock time): strict = ported >= contract with empty MISSING; record in DoP.
  (medium)
- **check_test_counts.py** — the strongest per-module gate and the ONLY fidelity net for the 11
  non-contract crates (vaxis/zf/xev/wuffs/z2d/zigstd/oniguruma/font-shaper/...) — is not registered
  in ci.sh. Add it, gated on an oracle-checkout probe (exit 3 BLOCKED on CI per R3). (medium)
- **G1 structural holes**: duplicate ref names collapse via set-union (61 names, e.g. 14×
  'parseCLI' → 1 entry), 76 anonymous tests count-only, Rust-only infra tests count toward the
  contract. Fix: per-file name diff, Counter multiplicity, `// rustty-only` tag. (medium)
- **G1 cannot see rustty-font-shaper**: crate_map['font'] owns shaper/discovery/Collection
  (contract 178) but G1 scans crates/rustty-font only ('font in 74/178') — strict will RED at
  finale with everything ported. Teach G1 a crate→multi-dir map. (medium)
- **Gate README drift**: describes a check_leaks.sh that no longer exists (dry mode, --strict,
  BLOCKED-on-missing-miri), stale extra/xtask contract note (3225 vs live 3227), stale crate/edge
  counts. (medium)
- Gate hygiene nits: PORT_STATUS_RE truncates multi-word statuses ('in progress' → 'in');
  crate_status reads only first 8KB; G4/G5 self-tests assert wiring only (which is why the broken
  real-arm flags survived since phase-1); gate-weakening history scan came back clean — only
  strengthenings found. (low)

### 3b. Ledger / status-header drift (same class as M1-M5; sweep in one pass)
- rustty-renderer lib.rs DEFERRED block ~90% stale (W0-W5 + config/Kitty/overlay all landed; only
  inspector overlayFeatures at generic.rs:4566 + GL/WebGL/GTK remain). (medium)
- DEFERRALS.md R-d (:505) and R-d P1 (:3134) never marked RESOLVED; W2 section says loop_enter
  "PANICS until W3" (filled by W5); no W0/W1/W4/W5 sections exist. ~25 phantom open seams for any
  ledger-driven workflow. (medium)
- DEFERRALS.md synthetic entry (:3266) still "body unported" — ported same day (fb12303); its 4
  live cli.rs binary-entry seams tracked only in SYNTHETIC_DESIGN.md. (medium)
- DEFERRALS.md cli C5 (:2618): boo's blocker (vaxis/zf "unported") no longer exists; boo.rs:22 is
  the crate's only live seam. cli lib.rs header stale the same way. (medium)
- Four crates have NO 'PORT STATUS:' marker (identity, terminfo, xev — uses 'CHUNK STATUS', z2d) →
  crate_status()='unknown', progress gates never bind. (medium)
- rustty-core lib.rs "stub ... C7..C15 land later" — C7-C15/C-T/C-F/W3 all landed; only the two
  inspector seams (surface.rs:2843/2877) remain. (medium)
- rustty-apprt "chunk E0 — scaffold" — E1-E11 landed; 13 tracked E4/E10 leaves remain. (medium)
- rustty-termio "xev-loop paths D6-deferred" — D6 RESOLVED 07-03 (live shell); real residual is
  windows/flatpak arms. Gates DO bind (status 'ported'); pure doc drift. (medium)
- rustty-font-shaper "F3 — shape.zig lands later" — full D1 tier landed (110 tests); residual is 3
  fontconfig arms + web_canvas. (medium)
- GAP_ANALYSIS.md contradicted on ~6 major claims (runtime-thread was stale at birth;
  macos-windowing, tui-cli, synthetic/bench/extra, ledger backlog, seam count 92→69). Stamp
  superseded-by or regenerate. (medium)
- rustty-wuffs: 5 live swizzle seams have no DEFERRALS entry under the crate's own name (only
  stale pre-crate renderer lines). (low)
- vt12-terminal-missing-tests.txt stale (all 179 ported; 2 renames noted); stream.rs header still
  declares the landed SIMD path as deferred; rustty-input/vt input-slice ownership docs wrong
  (5-file claim vs 10 files). (low)

### 3c. Soundness hazards (static; none demonstrated as live UB besides H1)
- **Page::get_row materializes `&mut Row` → *mut** (page_impl.rs:1278-1281) — fragile
  Unique-derived tag, inconsistent with siblings get_row_and_cell/get_cells; all 12 current callers
  consume immediately (latent pattern-(b) hazard, not demonstrated UB). One-line fix: pure pointer
  arithmetic. (medium)
- **SIGWINCH registration has no deregistration** (tty.rs:249-258): dropped/moved Loop leaves a
  dangling *mut in process-global HANDLERS; next SIGWINCH = UAF in signal context. Faithful to
  libvaxis shape, but safe-looking API invites misuse. (medium)
- **MIRI_EXCEPTIONS rows 8-13 self-cite the excepted test as its own covering variant**; the
  grow-prune branch has no distinct Miri-passing coverage; two evidence commits (58a5faa, c46a6a2)
  resolve to nothing in current history. Add a Miri-tractable prune test; re-pin hashes. (medium)
- decode_png_wrapper leaks the C-callback-allocated buffer every kitty PNG decode
  (vt-c/sys.rs:176). (medium)
- extern "C-unwind" display path lets a renderer panic unwind into Core Animation/ObjC frames (UB);
  Zig reference aborts. catch_unwind+abort or plain extern "C". (metal_impl.rs:829) (medium)
- No catch_unwind in any of the 271 exported C-ABI fns; documented placeholder leaves PANIC (=host
  abort) when driven — abort-vs-error-code policy for the finished product is unrecorded.
  (apprt-c lib.rs:712) (medium)
- IOSurfaceLayer ivars are unsynchronized Cell across threads (formally UB, mirrors reference;
  AtomicPtr is a free fix). glslang re-inits every call (faithful reference quirk, documented).
  (low)
- Positive attestations: vt-c container_of seam verified sound (offsets/provenance/aliasing/
  lifecycle); CString/buffer-ownership/null-parity sample clean; freetype/sentry/dcimgui/TIS/Metal
  init & thread-affinity verified; transmute/static-mut/shared-to-mut greps clean; the
  bitmap_allocator layout oddity is byte-identical to the Zig oracle (R1: keep). (low, recorded)

### 3d. Numeric / assertion semantics (mostly resolved by M6's profile decision)
- **Zig ReleaseSafe-active asserts ported as debug_assert!** (109 sites in rustty-vt; e.g.
  screen.rs:1167 guarding an unchecked `cell.add(n)`) — no rustty profile reproduces the
  ReleaseSafe safety level; overflow-checks alone does NOT restore these. Decide + document the
  parity mode (debug-assertions in release, or a `release-safe` profile used by harness/CI).
  (medium)
- **bitmap_allocator free(): @divExact ported as plain `/`** (bitmap_allocator.rs:201) — the
  heap-corruption tripwire silently floors instead of trapping; guards the grapheme/hyperlink
  storage under the crown jewel. Add a div_exact helper + ordering debug_assert. (medium)
- **Kitty graphics expected-size multiply** (graphics_image.rs:324,597): attacker-controlled
  u32 w*h*bpp overflows usize — Zig ReleaseSafe traps, rustty release wraps and mis-validates.
  Fixed automatically by overflow-checks=true. (medium)
- @intFromFloat (trap) → `as` (saturate) in selection-gesture pixel math — the one cast class a
  profile switch cannot fix; helper or document the fork. (low)
- quirks::inlineAssert is always-on assert!, inverting the quirk's ReleaseFast-elision purpose
  (perf + shipped-behavior drift in ~20 crates' files; internally inconsistent with vt's
  debug_assert usage). Pick a policy. (low)
- Inconsistent std.debug.assert mapping (assert! vs debug_assert!) across the same op surface
  (screen.rs:1167 vs terminal.rs:3438). Same policy decision. (low)
- config format_entry_field `_` arm: comptime-unreachable becomes silent Ok(()) in release; cfg-fmt
  NaN prints "NaN" vs Zig "nan"; print_repeat payload narrowed u16 vs usize. (low)
- Positive attestation: wrapping/saturating/div-mod/@truncate/guarded-@intCast idioms are
  systematically faithful across the full grep sweep — the findings above are the exceptions. (low)

### 3e. Fidelity drift (line-level walks + test quality)
- **Error-path integrity-gate parity**: Zig `defer assertIntegrity()` runs on error returns;
  several Rust ports skip it via `?` (screen.rs:2828,2960; pagelist/erase.rs:91,278,535) — OOM
  corruption goes undetected at exactly the point the oracle catches it (checked builds only).
  Route through inner fns. (medium)
- **tmux_enter_and_implicit_exit** is a no-op (TMUX_DCS_WIRED=false, documented) AND its dormant
  body dropped the 2 variant asserts while claiming "verbatim faithful port" (dcs.rs:767-776).
  Restore when the tmux wiring lands; fix the note now. (medium)
- erase_chars truncates count_req to u16 BEFORE the min-clamp (terminal.rs:3619) — unreachable via
  VT stream, reachable via pub API/FFI. Clamp in usize first. (low)
- xev kqueue_file_io_on_thread_pool: empty-body #[test] silently passes where the reference runs a
  live macOS body — mark #[ignore = "deferred: ThreadPool body"] so the skip is visible. (medium)
- font Atlas wasm "happy path" (4 assertions) feature-gated OFF, not in the DEFERRALS per-test
  ledger. 6 vacuous vt Zig blocks (4 refAllDecls + 2 empty decltests) unported — zero behavioral
  loss; record as documented exceptions. config.zig/z2d.zig anon refAllDecls blocks missing the
  conventional compile-touch. manualStyleUpdate's dead errdefer line untranscribed
  (screen.rs:2868). sliding_window case-insensitive test lost 2 inline comments. (low)
- Positive attestations recorded: 25/25 sampled tests at exact assertion-count parity, 9 deep
  value-diffs clean; all 24 #[ignore]s legitimate; the 166 zero-assert tests all resolve (helper
  asserts / faithful 0-assert originals / documented skips); no lossy assertion downgrades
  anywhere; per-crate delta table — every non-vt crate meets or exceeds its floor. (low)

### 3f. Dependency / build hygiene
- fontconfig-sys is an empty stub (links="fontconfig", no vendor, compiles nothing) — the one -sys
  crate where R3's vendored contract is unmet; coherent under macOS-first but must land before any
  Linux font-discovery milestone. (medium)
- gettext-sys vendors only the gnulib localename slice — ghostty's full libintl runtime
  (dcigettext/loadmsgcat/...) absent; i18n message translation has no linkable backing. (medium)
- Two independent libpng 1.6.43 compilations (freetype-sys private bundle without links=, plus
  libpng-sys with links="png") — latent duplicate-symbol conflict Cargo cannot police; today
  latent (libpng-sys has zero consumers). Single-artifact layout or a link-graph guard. (medium)
- Root-manifest comment claims libpng-sys backs dcimgui-sys (it deliberately doesn't); zlib
  substitution comment claims Zig std "wraps" zlib (it's pure Zig — reword + pin corrupt-stream
  error parity in the differential harness); no [profile.*] anywhere (see M6) — the extern-C
  unwind-abort FFI failure mode is implicit, not pinned. (low)
- Positive attestation: 55 members all in-repo, 58 external deps all registry-sourced, zero
  ghostty-path references, G7 acyclic GREEN (independent Tarjan confirms 0 SCCs,
  font-has-no-vt-edge holds), simdutf + glslang vendor trees byte-identical to ghostty's. (low)

### 3g. Unicode tables provenance
- props/grapheme/symbols stage tables are embedded generator output verified by provenance header
  only — no in-repo regeneration or checksum gate; a single wrong u16 among ~20k would be invisible
  until a specific codepoint misrenders. One build-enabled job: dump ghostty's generated arrays,
  diff numerically, add a checksum test. (low, but the one comptime-artifact class R1 currently
  takes on faith — parse table / UTF-8 DFA / modes / function_keys were all verified bit-identical)

---

## 4. Coverage & blind spots

### Per-auditor coverage (honest self-reports, condensed)

| Auditor | Fully examined | Sampled | Skipped |
|---|---|---|---|
| vt test-fidelity | Per-file census of all 103 Zig terminal files (2,178 blocks, 0 mismatches vs inventory); all Rust #[test] censuses; name-level match of all 2,159 named tests; all 179 vt12-missing entries | 50 tests assertion-count parity (40/40 + 10/10 exact); 1 full side-by-side diff | Assertion-content diffs for ~2,100 tests; input/ slice; any test execution |
| breadth (19 non-vt crates) | Crate→source mapping; both-side counts for all 19; per-file rosters for 15; hollow-test scan with manual adjudication | ~27 assertion-level body pairs | Comment-line census (R1, 42,069 lines); parity-surface ids (677); gate execution; vaxis uucode bit-fidelity |
| status/ledger | All 39 status headers vs test counts + all 69 live seams vs DEFERRALS; GAP_ANALYSIS full diff vs git log; gate status semantics | crates-sys headers; per-file parity of "done" claims | cargo runs; phase-0 per-file reconciliation; batch1-3 reports |
| unsafe-soundness (vt/datastruct/zigstd/vaxis) | page_impl.rs, ref_counted_set.rs, vaxis loop_.rs, bitmap_allocator alloc/free, tty signal machinery, MIRI_EXCEPTIONS (714 lines), Cell/Row repr; repo-wide greps (transmute 0, static mut 0, all Send/Sync + shared-to-mut chased) | terminal.rs/screen.rs/formatter.rs unsafe (grep-driven; 894+529+247 blocks) | Line-by-line of the bulk vt unsafe bodies; erase/grow/scroll/iter/hyperlink/kitty bodies; vaxis modules beyond loop_/tty/queue; any Miri/ASan run |
| FFI-boundary | vt-c extern surface + container_of seam; oniguruma all 8 files; crash sentry; simd base64 line-by-line; wuffs png decode; iosurface_layer; 12+ null-parity pairs | apprt-c (~15 of 74 bodies); jpeg.rs (assumed symmetric) | ~60% of apprt-c bodies; vt-c render/selection full bodies; crates-sys build.rs internals; inspector widget bodies |
| numeric-semantics | Build profiles; exhaustive wrapping/saturating-op inventories both sides; all div/mod + @divExact sites; slice reinterpret | ~15 of 621 ` as u32` / 773 ` as usize` sites; hot plain arithmetic | Cast-by-cast review of core/apprt/termio/cli/font-shaper; 32-bit targets; vendored C |
| assertion/format/comptime | All 154 debug_assert sites + callee purity; reverse Zig sweep for side-effecting asserts (3 hits, all verified); unreachable-drift grep (0); parse table/UTF-8 DFA/modes/function_keys bit-compared; key_encode + vt/cfg-fmt/termio format strings | config_struct format dispatch (macro layer, not all 203 arms) | unicode generated tables regeneration (build-locked; provenance-only); log-format strings; renderer/apprt format sites |
| VT front-half walk | Parser.zig/csi/osc/dcs/stream.zig vs Rust top-to-bottom incl. all 68 tests; parse table mechanically compared (0 mismatches) | dependencies (C0 tables, param defaults per command) | osc/parsers/* bodies; UTF8Decoder internals; sgr internals; stream_terminal |
| screen/page back-half walk | Terminal op layer, Screen back half, PageList entry points, page cell/row mgmt — both sides line-by-line incl. RAII-guard-vs-defer order | writeCell style tail; resizeWithoutReflow; compact; cursorCopy | **Terminal.print/printCell (front-half print path)**; selection fns (selectLine/Word/Output); dumpString; kitty seams (deferral-annotated); both ~11k-line test files |
| test-suite quality | All 3,526 #[test] smell-parsed; 166 zero-assert tests cross-referenced; ~150 citation-no-assert sites triaged; G1/G2 sources read; G1 executed | 25 seeded tests (counts), 9 deep value-diffs | Value-diff of the other ~1,150 citation-matched tests; non-contract crate test quality; G3/G3b depth |
| workspace hygiene | Root manifest, full cargo metadata graph, all 17 build.rs, links keys, vendor version headers vs ghostty pins, simdutf/glslang byte-diffs, cycle gates run + independent Tarjan | gettext-sys file list | Byte-verifying remote content-hash-pinned tarballs; any build |
| gate machinery | All gate sources + ci.sh + ci.yml read; every Python gate + self-test executed read-only; status census of all 40 crates; done-flip simulation; tool-presence probes | check_test_counts.py (executed, skimmed); check_baseline_regression compare loop | Any cargo/miri/fuzz execution; G6 self-test; comparator.py comparison internals; phase-0 inventory generators |

### What NO auditor examined (targets for the next sweep)

1. **Any dynamic execution**: not one test, Miri, ASan, fuzz, or differential run informed this
   report (target-dir lock; read-only mandate). The entire soundness/fidelity picture is static.
   First action after unlocking: full `cargo test --workspace`, the vt Miri suite against
   MIRI_EXCEPTIONS, G6 --all-done, and an armed G4 run.
2. **Terminal.print/printCell** — the vt front-half print path (grapheme-attach-on-overwrite, wide
   chars at margins) fell between the two line-level walks' scopes. Highest-value un-walked
   hot-path code in the port.
3. **Selection functions** (selectLine/selectWord/selectOutput/promptClickMove) and
   dumpString/testWriteString — explicitly skipped by the back-half walk.
4. **The bulk unsafe bodies** of terminal.rs (894 blocks), screen.rs (529), formatter.rs (247),
   erase/grow/scroll/iter/hyperlink/kitty — sampled only; the sampled blocks follow base-provenance
   discipline, but no per-block review exists.
5. **Assertion-content diffs** for ~2,100 vt tests and value-diffs for ~1,150 citation-matched
   tests (counts/names only). H4+H6 fixes would make the gates carry part of this.
6. **The R1 comment-fidelity census** (42,069 comment lines) — no auditor attempted content-level
   comment verification beyond spot checks; G3/G3b aggregates are the only net.
7. **~60% of rustty-apprt-c export bodies**, vt-c render/selection full bodies, inspector widget
   bodies, crates-sys build.rs internals.
8. **rustty-unicode generated tables** — provenance-only; needs the one-time generator-diff job
   (§3g).
9. **32-bit / non-macOS targets** (accepted: macOS-first locked fork) and vendored
   content-hash-pinned tarball byte-identity (version-header verification only).
10. **Batch 1-3 audit reports and the phase-0 inventory generators** (census scripts) — consumed,
    never audited.
11. **comparator.py comparison internals** and G6's self-test (runs cargo miri) — unexecuted.

---

## 5. Refuted appendix (do NOT re-raise without new evidence)

1. **z2d dropped reference tests (surface/pixel/compositor/multisample)** — REFUTED: z2d's own
   tests were excluded from the parity contract before the port (wf-font-f0-z2d.js:16; z2d absent
   from tests.jsonl/check_test_counts); pixel-exactness is gated by the glyf/sprite PNG KATs; the
   flagship `downsample` example has no caller in the workspace. Residual: low hygiene note only.
2. **rustty-input 'stub' status false** — REFUTED: input.zig is only ~2/22 decls carved into
   lib.rs; the crate is 113/116 vs contract (gap = files ported in rustty-vt); 'stub' is the honest
   gate-protective status and a done-flip would RED G1. Residual: header wording drift (low).
3. **rustty-cfg-fmt 'stub' rationale dead** — REFUTED: the frozen crate_map assigns cfg-fmt 9
   blocks incl. key.zig's 'Value' test, which deliberately lives in rustty-config (documented Flag
   A3 resolution); done-flip would RED G1 at 8/9. Residual: one stale word in lib.rs:11 (low).
4. **SIGWINCH handler async-signal-unsafe (Mutex/Condvar in signal context)** — REFUTED as a port
   finding: line-for-line faithful to upstream libvaxis 0.5.1 (ghostty's pinned dep), already
   documented at MIRI_EXCEPTIONS.md:693-695; the deadlock scenario was overstated (Condvar::wait
   releases the mutex). Upstream-inherited note only.
5. **Production never calls onig_initialize** — REFUTED: the vendored oniguruma self-initializes in
   onig_reg_init with the exact utf8 encoding rustty passes (regcomp.c:7665-7675; hard-error branch
   compiled out; warning is a no-op) — functionally identical to ghostty's explicit init; the
   omission is the documented BREAK #1 deferral (DEFERRALS.md:1863-1873, :2965-2968) with the
   verbatim line preserved in rustty-global. Residual: theoretical first-compile race, strictly
   subsumed by the scheduled Phase-3 wiring (low).
6. **Reachable unreachable!() panic on DCS ESC P ... p with tmux_control_mode on** — REFUTED as a
   new defect: the runtime stub itself is the documented deferral (DEFERRALS.md:343-361 names the
   unreachable!(), the feature interaction, and the resolution sub-batch that flips
   TMUX_DCS_WIRED); faithful body preserved verbatim per R1; no shipping binary or test reaches it.
   Reminder only: the tmux DCS wiring sub-batch must land before any dogfood build.
7. **PORT STATUS header is a self-declared evasion switch (vt 'stub', terminfo unmarked = permanent
   ungrading)** — REFUTED: --strict ignores status entirely (check_tests_ported.py:136-142,
   check_parity_surface strict self-test), the DoP completion contract is marker-independent, and
   'unknown' fails conservative. Residual: interim regression-arming is opt-in — covered by M1 and
   the §3b marker sweep.

---

*Report generated 2026-07-10 from the batch-4 validation sweep. All findings above marked
CONFIRMED survived independent adversarial verification (each verdict's evidence is preserved in
the batch records); severities shown are post-verification.*
