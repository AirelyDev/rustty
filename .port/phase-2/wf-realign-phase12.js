export const meta = {
  name: 'realign-phase12',
  description: 'Phases 1+2 of the realignment plan: release-profile semantics, vaxis SB-UB mirihar fix, soundness/semantics mediums batches, then the great ledger/status sweep and the contract-gate payoff run',
  whenToUse: 'Run after wf-realign-phase0 lands (gates must be honest first)',
  phases: [
    { title: 'T10 profiles', detail: 'release overflow-checks + release-fast + release-safe (DG2/DG4)' },
    { title: 'Parallel fixes', detail: 'T11 vaxis H1 mirihar-proven; T12 soundness mediums; T13 semantics mediums' },
    { title: 'T14 ledger sweep', detail: 'status headers + DEFERRALS + GAP_ANALYSIS realignment' },
    { title: 'Payoff', detail: 'binding-mode contract gates over newly-done crates' },
  ],
}

const PLAN = 'docs/superpowers/plans/2026-07-10-port-realignment-and-validation-hardening.md'
const REPORT = '.port/phase-2/VALIDATION_REPORT.md'

const IMPL = {
  type: 'object',
  required: ['task', 'status', 'changed_files', 'evidence'],
  properties: {
    task: { type: 'string' },
    status: { type: 'string', enum: ['done', 'blocked'] },
    changed_files: { type: 'array', items: { type: 'string' } },
    evidence: { type: 'string' },
    blocker: { type: 'string' },
    notes: { type: 'string' },
  },
}
const VERDICT = {
  type: 'object',
  required: ['ok', 'issues'],
  properties: {
    ok: { type: 'boolean' },
    issues: { type: 'array', items: { type: 'string' } },
    r5_concerns: { type: 'array', items: { type: 'string' } },
  },
}
const FIX = {
  type: 'object',
  required: ['task', 'committed', 'summary'],
  properties: {
    task: { type: 'string' },
    committed: { type: 'boolean' },
    commit_ref: { type: 'string' },
    summary: { type: 'string' },
    residual: { type: 'string' },
  },
}

const COMMON = `
REPO: /Users/gael/projects/rustty. You are executing ONE item of the realignment plan (${PLAN}); evidence base ${REPORT} (file:line for every claim) — read your item's sections first.
HARD RULES:
- NEVER modify anything under /Users/gael/projects/ghostty (read-only oracle, R3) — but DO read it: every fix must mirror the Zig reference's exact behavior (R1); read the Zig source before changing the Rust.
- R5: never weaken/skip/#[ignore]/loosen any test or gate; blocked > bent.
- R6/memory doctrine: any SB/TB-class fix must be PROVEN on an isolated Miri harness (extend .port/phase-2/mirihar-chunk3b or a sibling), reproducing the UB first (harness test FAILS under Miri), then showing the fix clean. Never hand-reason aliasing.
- Every behavioral fix carries a test (fail-first where the plan says so). Comments follow R1: preserve ported comments; when a Rust primitive's failure mode differs from the Zig builtin it replaced, rewrite the adjacent comment to the RUST contract (detector D9 doctrine).
- cargo may block on the target-dir lock while siblings run — wait, never kill.
- Git: commit only when told, explicit paths only; index.lock → sleep 3, retry ≤10x; never reset/checkout/stash/pull; no push.
Return via StructuredOutput with decisive command output as evidence.`

function taskPipeline(t) {
  return async () => {
    const impl = await agent(`${COMMON}
YOUR ITEM: ${t.id} — ${t.title}.
${t.spec}
Do NOT commit — a fixer commits after adversarial review.`,
      { label: `impl:${t.id}`, phase: t.phase, schema: IMPL })
    if (!impl || impl.status === 'blocked') {
      log(`${t.id} BLOCKED: ${impl ? impl.blocker : 'agent failed'}`)
      return { task: t.id, committed: false, summary: `BLOCKED: ${impl ? impl.blocker : 'agent failed'}` }
    }
    const verdict = await agent(`${COMMON}
ADVERSARIAL VERIFIER for ${t.id} (${t.title}). Spec was:
${t.spec}
Implementation report:
${JSON.stringify(impl, null, 1)}
Default ok=false. Re-read the diff yourself (git diff -- <changed files>), re-run its verification commands, diff the fix against the Zig reference behavior (R1), and hunt R5 weakening (silenced gate/test, deleted assertion, softened comment). For SB/TB items: confirm the harness actually FAILED pre-fix and is CLEAN post-fix (read the harness diff; re-run miri there yourself). List issues with file:line.`,
      { label: `verify:${t.id}`, phase: t.phase, schema: VERDICT })
    return agent(`${COMMON}
SURGICAL FIXER for ${t.id} (${t.title}). Implementation:
${JSON.stringify(impl, null, 1)}
Verdict:
${JSON.stringify(verdict, null, 1)}
Fix each real issue surgically (skip hallucinated ones with a note in residual); re-run verification to green; commit exactly this item's files with message "${t.commit}" (append " (${t.id}, VALIDATION_REPORT ${t.evidRef})" ). Explicit paths only.`,
      { label: `fix:${t.id}`, phase: t.phase, schema: FIX })
  }
}

// ---------------- T10: profiles (must land before the parallel fixes: they test under it)
phase('T10 profiles')
const t10 = await taskPipeline({
  id: 'T10', phase: 'T10 profiles', title: 'release-profile semantics (M6/DG2/DG4)',
  commit: 'workspace: release overflow-checks=true + release-fast + release-safe profiles; kitty expected-size capacity test; PORT_RULES DG2/DG4 record',
  evidRef: 'M6, §3d',
  spec: `Execute "Task 10: Release-profile semantics" in ${PLAN} exactly: the three-profile TOML block for the ROOT Cargo.toml is in the plan; point build_port_driver.sh and the G4 harness at --profile release-safe; write the Kitty expected-size capacity test (read ghostty's src/terminal/kitty/graphics_image.zig expected-size validation FIRST and mirror its exact trap/error behavior for w=0xFFFF_FFFF,h=2,bpp=4); record DG2+DG4 resolution in .port/PORT_RULES.md. Verify: cargo build --workspace --release green; cargo test -p rustty-vt --profile release-safe for the new test (or --release if harness profile selection belongs only to the harness scripts — match what the plan says).`,
})()
log(`T10: ${t10 && t10.committed ? 'committed' : 'NOT committed — ' + (t10 ? t10.summary : 'failed')}`)

// ---------------- Parallel fixes
phase('Parallel fixes')
const fixes = [
  {
    id: 'G4FIX', phase: 'Parallel fixes', title: 'lib_version identity split (G4 c-vt-build-info divergence)',
    commit: 'rustty-vt: port ghostty\'s deliberate app/lib version split — lib_version via rustty-identity (identity-oracle flips to 0.1.0-dev); G4 c-vt-build-info byte-parity restored',
    evidRef: 'T9 new finding (armed G4)',
    spec: `The armed G4 differential found its first real divergence: case c-vt-build-info, golden "Version: 0.1.0-dev / Version pre: dev" vs port "0.1.0 / (none)". Root cause: ghostty build.zig:7-10 deliberately gives libghostty its OWN version ("We use a separate version from the app"): const lib_version = "0.1.0-dev"; rustty-vt/src/options.rs:74 instead derives VERSION_STRING from env!("CARGO_PKG_VERSION"). Chosen fix (structure-faithful, R1+R2): port the SPLIT, not the coincidence — (1) add the vt lib version to crates/rustty-identity (the R2 source-of-truth crate; see its existing VT VERSION at src/lib.rs:72 and the identity-oracle feature pattern used by its siblings): rustty value stays "0.1.0" (or the identity crate's chosen rustty lib version), identity-oracle flips it to the reference's exact "0.1.0-dev" with pre = "dev"; (2) rustty-vt options.rs consumes it from rustty-identity instead of CARGO_PKG_VERSION, preserving the ported comments and updating them per D9 to name the new source; (3) mirror ghostty's version_string/pre derivation logic exactly (read build.zig:7-30 + the vt build options plumbing first). Do NOT change the workspace version (rejected alternative (a): it ripples all 39 crates and conflates app with lib identity — record this rejection in the commit body per the marker-discipline doctrine). Verify: cargo build -p rustty-vt-c --features identity-oracle (or however the harness selects the parity profile — read build_port_driver.sh) then bash .port/tools/gates/run_differential.sh → c-vt-build-info OK, 21/21 leg-B cases green, G4 exit 0. Also check rustty-identity's own tests still pass and no OTHER fixture regresses.`,
  },
  {
    id: 'T11', phase: 'Parallel fixes', title: 'vaxis Loop::stop() SB/TB protector UB (H1)',
    commit: 'rustty-vaxis: fix Loop::stop protected-tag UB via field projection; mirihar overlap model proves pre-fix UB and post-fix clean; MIRI_EXCEPTIONS 708 corrected',
    evidRef: 'H1',
    spec: `Execute "Task 11" in ${PLAN} exactly, in its step order: (1) FIRST extend the mirihar harness with the stop()-vs-read overlap model and show Miri FAILS on the current shape (paste the SB violation); (2) fix loop_.rs:206-211 per the plan (addr_of_mut projection like the shipped loop_.rs:27-38 pattern; for the DSR wake, read upstream libvaxis 0.5.1's shape first and prefer the faithful form); (3) harness clean + cargo +nightly miri test -p rustty-vaxis (tractable set) clean; (4) correct MIRI_EXCEPTIONS.md:708. Then run the interactive smoke: printf 'q' | cargo run -p rustty --quiet -- list-themes 2>/dev/null or the nearest scriptable equivalent — evidence what you ran.`,
  },
  {
    id: 'T12', phase: 'Parallel fixes', title: 'soundness mediums batch (§3c, 10 items)',
    commit: 'soundness mediums: get_row ptr-arith, png decode leak, C-unwind abort, apprt-c panic policy, SIGWINCH, MIRI_EXCEPTIONS 8-13, div_exact, erase_chars clamp, error-path integrity, iosurface AtomicPtr',
    evidRef: '§3c',
    spec: `Execute "Task 12" in ${PLAN}: the 10 enumerated §3c items, EACH as its own commit in the listed order (item 4's panic policy: write the policy into PORT_RULES.md as part of the item; item 9 routes ?-early-returns through inner fns so assert_integrity runs on error paths exactly like Zig's defer — read each Zig site first). Items 1,5,6,10 need Miri/ASan proof per the R6 doctrine above; item 2 needs an ASan-visible leak test (fail-first). For every item paste the verifying command output. If an item proves NOT to be a defect on close reading (e.g. upstream-faithful shape that must stay per R1), document it in the validation report's terms in notes and make the minimal documentation change instead — do not force a code change the reference contradicts.`,
  },
  {
    id: 'T13', phase: 'Parallel fixes', title: 'semantics mediums batch (§3d/§3e residue + M4)',
    commit: 'semantics mediums: tmux DCS asserts, intFromFloat fork, inlineAssert policy, assert consistency, config fmt arms, xev empty-body test, font wasm ledger, windowed-present KATs un-ignored',
    evidRef: '§3d/§3e, M4',
    spec: `Execute "Task 13" in ${PLAN}: each listed item its own commit. Two special notes: (a) the xev empty-body test — the plan flags that #[ignore] would violate R5's spirit; resolve it the honest way: port the real ThreadPool body if tractable, else make the skip VISIBLE and ledgered (DEFERRALS entry + a test body that asserts the deferral is still tracked), decide with the verifier; (b) M4: un-ignore apprt-c/tests/macos_window_host.rs:313,377 and RUN them on this Metal host — if they fail, that is the suppressed signal: capture the failure verbatim in notes as a NEW FINDING, leave the tests un-ignored and failing is NOT acceptable for commit — instead report status=blocked for that item with the failure so it gets triaged (the rest of the batch still commits).`,
  },
]
const fr = await parallel(fixes.map(t => taskPipeline(t)))
log(`Parallel fixes: ${fr.filter(Boolean).filter(r => r.committed).length}/${fixes.length} committed`)

// ---------------- T14 ledger sweep
phase('T14 ledger sweep')
const t14 = await taskPipeline({
  id: 'T14', phase: 'T14 ledger sweep', title: 'great status/ledger sweep (M1-M5 + §3b)',
  commit: 'ledger realign: PORT STATUS headers accurate workspace-wide; DEFERRALS reconciled (zero phantom seams); GAP_ANALYSIS superseded; stale docs fixed',
  evidRef: 'M1-M5, §3b',
  spec: `Execute "Task 14" in ${PLAN}: every §3b item is pre-verified with file:line in ${REPORT} — work through ALL of them (status flips incl. vt/config/vt-c/core/apprt/termio/font-shaper/renderer/cli; add PORT STATUS to identity/terminfo/xev/z2d; DEFERRALS additions W0-W5/vaxis-4a4b/synthetic/wuffs and resolutions; GAP_ANALYSIS superseded-by stamp; vt12-terminal-missing-tests resolved; stream.rs SIMD header; input-slice ownership doc). Cross-check each flip against reality before writing it (test counts, live seams). Commit per crate cluster as the plan says (you own all the commits for this task). Every live unimplemented!()/todo!() must map to a live DEFERRALS entry when you finish — enumerate and prove it in evidence.`,
})()
log(`T14: ${t14 && t14.committed ? 'committed' : 'NOT committed — ' + (t14 ? t14.summary : 'failed')}`)

// ---------------- Payoff: binding-mode contract gates
phase('Payoff')
const payoff = await agent(`${COMMON}
YOUR TASK: the Phase-2 payoff run (${PLAN} Task 14 Step 2). With headers honest and matchers fixed, run in order and paste each verdict block:
python3 .port/tools/gates/check_tests_ported.py
python3 .port/tools/gates/check_assertions.py
python3 .port/tools/gates/check_comment_coverage.py
python3 .port/tools/gates/check_parity_surface.py
python3 .port/tools/gates/check_test_counts.py
Then bash .port/tools/ci.sh 2>&1 | tee .port/phase-2/ci-phase2-payoff.log and read the CI SUMMARY.
Every residual RED on a now-done crate is a GENUINE R1 finding: enumerate each precisely (crate, missing name/count, ref file) in notes — fix only the surgical ones (a truly missing test port: port it assertion-for-assertion from the Zig source with its comments); report anything structural as a blocker. NEVER flip a header back to silence a gate (R5). Commit the log + any test ports (explicit paths, message "phase-2 payoff: contract gates binding on done crates; <n> residual findings fixed/filed").`,
  { label: 'payoff:gates', phase: 'Payoff', schema: IMPL })

return {
  t10, fixes: fr, t14, payoff,
  summary: `T10 ${t10 && t10.committed ? 'ok' : 'FAILED'}; fixes ${fr.filter(Boolean).filter(r => r.committed).length}/3; T14 ${t14 && t14.committed ? 'ok' : 'FAILED'}; payoff ${payoff ? payoff.status : 'failed'} — ${(payoff && (payoff.notes || payoff.evidence) || '').slice(0, 500)}`,
}
