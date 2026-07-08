export const meta = {
  name: 'search-binding-arm',
  description: 'Fill the LAST unported performBindingAction first-half arm — the terminal-search `.search` arm (core Surface.zig:4933) + its rustty-vt search-thread prerequisite: (a) retype terminal::search::thread::Options.mutex from *const std::sync::Mutex<()> to the shared guard-less rustty_zigstd::thread::Mutex the reference passes; (b) the self-referential construct-then-spawn order (store Search with thread=undefined, spawn with a stable &s.state, assign the handle); (c) the core .search arm. The rustty-vt search LOGIC is ported. Design (guarded: STOPS if the whole thing is Surface-construction/windowing-blocked with no testable rustty-vt slice) -> adversary -> fold -> port (stop-if-no-design) -> verify.',
  phases: [
    { title: 'Design', detail: 'audit the rustty-vt terminal::search::thread state (is the thread body ported or a seam? the Options.mutex type) + the core .search arm (Surface.zig:4933); scope the mutex unification + the construct-then-spawn + what is testable at the vt level vs Surface-blocked. WRITE the design doc (guarded). STOP-report if no testable vt slice exists.' },
    { title: 'Adversary-design', detail: 'attack: the mutex retype (does it break the ported search callers/tests?); the self-referential construct-then-spawn aliasing (rust-mut-args-strongly-protected — the &s.state stable-address hazard); the .search arm fidelity; dropped test{}' },
    { title: 'Fold-fixes', detail: 'if the adversary returned any fixes OR blocking issues (regardless of verdict), fold them INTO the doc' },
    { title: 'Port', detail: 'implement exactly as the hardened doc specifies; STOP if the design doc is absent' },
    { title: 'Verify', detail: 'panel: R1-comment fidelity (manual), correctness (the search-thread + the mutex + the .search arm), aliasing (construct-then-spawn), gates + tests' },
  ],
}

const REPO = '/Users/gael/projects/rustty'
const REF = '/Users/gael/projects/ghostty'
const DESIGN_DOC = REPO + '/.port/phase-2/SEARCH_BINDING_ARM_DESIGN.md'

const DESIGN_SCHEMA = {
  type: 'object', additionalProperties: false,
  required: ['wrote_doc', 'vt_slice', 'scope_methods', 'aliasing_note', 'test_plan', 'compile_boundary', 'summary'],
  properties: {
    wrote_doc: { type: 'boolean' },
    vt_slice: { type: 'string' },
    scope_methods: { type: 'array', items: { type: 'string' } },
    aliasing_note: { type: 'string' },
    test_plan: { type: 'string' },
    compile_boundary: { type: 'string' },
    summary: { type: 'string' },
  },
}
const CRITIQUE_SCHEMA = {
  type: 'object', additionalProperties: false,
  required: ['verdict', 'blocking_issues', 'fixes', 'summary'],
  properties: {
    verdict: { type: 'string', enum: ['SOUND', 'SOUND-WITH-FIXES', 'BROKEN'] },
    blocking_issues: { type: 'array', items: { type: 'string' } },
    fixes: { type: 'array', items: { type: 'string' } },
    summary: { type: 'string' },
  },
}
const FOLD_SCHEMA = {
  type: 'object', additionalProperties: false,
  required: ['wrote_doc', 'folded', 'summary'],
  properties: {
    wrote_doc: { type: 'boolean' },
    folded: { type: 'array', items: { type: 'string' } },
    summary: { type: 'string' },
  },
}
const VERDICT_SCHEMA = {
  type: 'object', additionalProperties: false,
  required: ['lens', 'verdict', 'findings'],
  properties: {
    lens: { type: 'string' },
    verdict: { type: 'string', enum: ['PASS', 'CONCERNS', 'FAIL'] },
    findings: {
      type: 'array',
      items: {
        type: 'object', additionalProperties: false,
        required: ['severity', 'file', 'issue'],
        properties: { severity: { type: 'string', enum: ['blocker', 'major', 'minor'] }, file: { type: 'string' }, issue: { type: 'string' } },
      },
    },
  },
}

const COMMON = [
  'REPO=' + REPO + '. REFERENCE (read-only)=' + REF + ' (Zig 0.15.2, src/Surface.zig performBindingAction .search arm + src/terminal/search*). The design doc is ' + DESIGN_DOC + '.',
  'STATE: rustty-core is comprehensively ported; the C-F ledger (DEFERRALS.md) marks the `performBindingAction .search` arm (Surface.zig:4933) as THE ONLY unported first-half switch arm (crates/rustty-core/src/surface.rs, verbatim above an unimplemented!()). Every OTHER search arm (start_search/search_selection/end_search/navigate_search) ports LIVE (their Some-branches present but dormant because self.search is always None until this lands). The rustty-vt terminal search LOGIC is ported.',
  'THE TWO BLOCKERS (per the C-F ledger): (a) SEARCH-THREAD MUTEX-TYPE UNIFICATION — `terminal::search::thread::Options.mutex` is still `*const std::sync::Mutex<()>` (ported pre-C0) and must retype to the shared guard-less `rustty_zigstd::thread::Mutex` that the reference passes as `self.renderer_state.mutex`. (b) SELF-REFERENTIAL CONSTRUCT-THEN-SPAWN — the reference stores the `Search` struct FIRST with `.thread = undefined`, then spawns the search thread with a STABLE `&s.state` (a pointer into the now-stored Search), then assigns the returned thread handle. This is the pinned-address / raw-ptr pattern (memory rust-mut-args-strongly-protected + intrusive-ptr-and-placeholder-fill): `&s.state` must be a stable address (the Search stored in a Box / a field that does not move), threaded raw, not a `&mut` that outlives the spawn.',
  'Rules: ' + REPO + '/.port/PORT_RULES.md — R1 (100% fidelity: every line/comment/test{}; the .search arm verbatim body is preserved as a comment — port it faithfully), R4 (ACYCLIC), R5 (anything still genuinely blocked stays unimplemented!() seam + verbatim body). memory shaper-hook-sb-clean-interleave / rust-mut-args-strongly-protected: the construct-then-spawn stable-address aliasing must be SB-clean — if a new raw-alias-crossed-by-&mut pattern arises, PROVE on .port/phase-2/mirihar-chunk3b/ (extend it). G3b comment-coverage COVERS rustty-vt + rustty-core — keep green.',
  'TESTABILITY: the core .search arm lives in Surface::performBindingAction; a full Surface is windowing-blocked (Surface::init panics earlier at Renderer::surfaceInit) — so the .search arm itself may be COMPILE-only (faithful, unblocks the seam) if it cannot be driven headlessly. BUT the rustty-vt search-thread (the mutex-typed Options + the search thread that scans the Screen for a query) IS testable directly: spawn a search over a Screen containing a known string, assert it finds the match(es). Prioritize a real rustty-vt search-thread test. If the .search arm can be exercised via a testable seam (a search-thread-only harness), do so; otherwise land it compile-faithful + note it.',
].join('\n')

phase('Design')
const design = await agent(
  'ARCHITECT: fill the terminal-search `.search` binding arm + its rustty-vt search-thread prerequisite. You MUST write ' + DESIGN_DOC + ' with the Write tool (guarded — wrote_doc=true only if written). Design ONLY; touch NO crates/. FIRST audit the rustty-vt terminal::search::thread (is the thread body ported or itself a seam? the Options.mutex type + its callers) + the core .search arm (surface.rs, verbatim body) — determine what is testable at the vt level (see COMMON). If the search-thread is itself a large unported seam or the whole thing is Surface/windowing-blocked with no testable vt slice, record vt_slice=none + return.\n' + COMMON + '\n\n' +
  'If a testable vt slice exists, DECIDE + specify (concrete Rust signature blocks + the mutex retype + the construct-then-spawn + OPEN RISKS): (1) the `terminal::search::thread::Options.mutex` retype to `rustty_zigstd::thread::Mutex` + every caller/test updated; (2) the self-referential construct-then-spawn (Search stored in a stable slot, &s.state threaded raw — the aliasing discipline); (3) the core .search arm faithful to Surface.zig:4933; (4) the rustty-vt search-thread test (spawn a search over a Screen, assert matches) + whether the .search arm is testable or compile-faithful; (5) dead-code + no-regression. Return the schema: wrote_doc, vt_slice, scope_methods, aliasing_note, test_plan, compile_boundary, summary.',
  { label: 'design:search', agentType: 'general-purpose', schema: DESIGN_SCHEMA })

if (!design.wrote_doc) {
  log('DESIGN GUARD TRIPPED: design agent did NOT write the design doc. Aborting before port.')
  return { aborted: 'design-doc-not-written', design }
}
const _vs = String(design.vt_slice).trim().toLowerCase()
if (_vs.startsWith('none') || _vs.startsWith('blocked') || _vs.startsWith('no ') || _vs.startsWith('no-')) {
  log('NO TESTABLE VT SLICE: ' + design.vt_slice + ' — the search feature is Surface/windowing-blocked or the search-thread is a large seam; DEFERRING. No port attempted.')
  return { aborted: 'search-no-vt-slice', design }
}
log('search design written (vt slice exists). scope=' + (design.scope_methods || []).join(',').slice(0, 120))

phase('Adversary-design')
const critique = await agent(
  'ADVERSARIAL reviewer: BREAK the search-binding-arm design at ' + DESIGN_DOC + '. Re-derive INDEPENDENTLY from source.\n' + COMMON + '\n\n' +
  'Read the design + Surface.zig performBindingAction .search arm + src/terminal/search* + the ported rustty-vt terminal::search + the core surface.rs .search seam. CHECK, breaking each: (1) the MUTEX RETYPE — does changing Options.mutex from *const std::sync::Mutex to rustty_zigstd::thread::Mutex break any ported search caller/test; is the guard-less lock/unlock used correctly? (2) the CONSTRUCT-THEN-SPAWN ALIASING — is `&s.state` a genuinely STABLE address (Search in a Box/non-moving slot), threaded raw (NOT a &mut spanning the spawn)? Any raw-alias-crossed-by-&mut / use-after-move hazard (rust-mut-args-strongly-protected)? Does it need a mirihar candidate? (3) the .search arm fidelity vs Surface.zig:4933; (4) is the rustty-vt search-thread genuinely testable (real matches over a real Screen)? (5) any reference test{} dropped (search tests)? Return the schema: verdict, blocking_issues, concrete fixes, summary.',
  { label: 'adversary:search-design', agentType: 'general-purpose', schema: CRITIQUE_SCHEMA })

log('search adversary: ' + critique.verdict + ' | blocking=' + critique.blocking_issues.length + ' fixes=' + critique.fixes.length)

phase('Fold-fixes')
const needsFold = (critique.fixes && critique.fixes.length) || (critique.blocking_issues && critique.blocking_issues.length)
let fold = null
if (needsFold) {
  fold = await agent(
    'DESIGN EDITOR: the adversary reviewed the search-binding-arm design (' + DESIGN_DOC + ') and returned verdict=' + critique.verdict + ' with fixes/issues to FOLD IN.\n' + COMMON + '\n\n' +
    'ADVERSARY BLOCKING ISSUES:\n- ' + (critique.blocking_issues || []).join('\n- ') + '\nADVERSARY FIXES:\n- ' + (critique.fixes || []).join('\n- ') + '\n\n' +
    'For EACH: verify against the reference/rustty source yourself (reject any the source contradicts; note rejected + why in `folded`). IF the adversary shows the construct-then-spawn is UB-prone or the slice is not testable, record that prominently so the port STOPS or adopts the safe remedy. Edit ' + DESIGN_DOC + '. Preserve structure. Touch NO crates/. You MUST end with the doc written. Return: wrote_doc, folded, summary.',
    { label: 'fold:adversary-fixes', agentType: 'general-purpose', schema: FOLD_SCHEMA })
  if (!fold.wrote_doc) {
    log('FOLD GUARD TRIPPED: doc not present after folding. Aborting before port.')
    return { aborted: 'design-doc-missing-after-fold', design, critique, fold }
  }
  log('search fold-fixes: ' + (fold.folded || []).length + ' items :: ' + String(fold.summary).slice(0, 120))
} else {
  log('search design fully clean — no fold needed; porting directly from the doc.')
}

phase('Port')
const portReport = await agent(
  'PORTING engineer: implement the search-binding-arm + the rustty-vt search-thread prerequisite EXACTLY as specified in the (adversary-hardened) design doc ' + DESIGN_DOC + '. FIRST Read the doc + verify it EXISTS + is substantive — if absent or a stub, STOP + report "design doc missing".\n' + COMMON + '\n\n' +
  'Port: the Options.mutex retype (+ every caller/test), the self-referential construct-then-spawn (stable-address, raw-ptr discipline — PROVE any new raw-alias pattern on .port/phase-2/mirihar-chunk3b/ under SB+TB), and the core .search arm faithful to Surface.zig:4933. Every line+comment ported (R1 — preserve EVERY reference comment verbatim; keep G3b green for vt + core); byte-faithful. Add the rustty-vt search-thread test (spawn a search over a Screen, assert matches). Anything still genuinely blocked stays an unimplemented!() seam with verbatim body.\n' +
  'GATES (run; ALL green, never weaken): cargo build --workspace, cargo fmt --check, cargo clippy --workspace --all-targets -- -D warnings, cargo test -p rustty-vt -p rustty-core (+ any crate touched), bash .port/tools/gates/check_acyclic.sh, python3 .port/tools/gates/check_test_counts.py, python3 .port/tools/gates/check_comment_coverage.py. If a gate can not go green HONESTLY, STOP + report the precise blocker (a search-thread data race is a REAL failure — do not paper over). Do NOT git commit. Report: the mutex retype, the construct-then-spawn, the .search arm, the search-thread test + result, each gate PASS/FAIL with tail, any reference comment/line you could not place.',
  { label: 'port:search', agentType: 'general-purpose' })

log('search port done. Head: ' + String(portReport).slice(0, 200))

phase('Verify')
const LENSES = [
  'R1 COMMENT + LINE FIDELITY: diff EVERY ported/changed line AND comment of the .search arm (surface.rs) + the search-thread mutex retype (rustty-vt) against Surface.zig / the search reference. Report any dropped/truncated/summarized comment (BLOCKER; keep G3b green) or dropped branch/line. Confirm any reference search test{} is ported.',
  'CORRECTNESS + ALIASING: (a) the mutex retype is correct (guard-less lock/unlock, no caller broken); (b) the construct-then-spawn is SB-sound — `&s.state` stable-address, raw-threaded, no use-after-move / raw-alias-crossed-by-&mut (re-run/extend the mirihar harness under SB+TB if a new pattern); (c) the .search arm is faithful; (d) the search-thread finds real matches. Run the rustty-vt search-thread test (+ under ASan/TSan if a thread race is possible).',
  'GATES + no-regression: RUN cargo build --workspace, cargo fmt --check, cargo clippy --workspace --all-targets -- -D warnings, cargo test -p rustty-vt -p rustty-core, bash .port/tools/gates/check_acyclic.sh, python3 .port/tools/gates/check_test_counts.py, python3 .port/tools/gates/check_comment_coverage.py — report REAL results (tails). Existing vt + core tests must still pass.',
]
const verdicts = await parallel(LENSES.map((lens, i) => () =>
  agent(
    'ADVERSARIAL verifier for the terminal-search binding arm (UNCOMMITTED working tree). LENS:\n' + lens + '\n' + COMMON + '\nFind the changed files (git diff/status), compare vs the reference, and RUN the gates your lens names. Report the schema: lens, verdict PASS/CONCERNS/FAIL, findings (severity, file, issue). Only REAL evidenced issues; if clean, empty findings + PASS + name the two things you attacked hardest in `lens`.',
    { label: 'verify:' + i, phase: 'Verify', agentType: 'general-purpose', schema: VERDICT_SCHEMA })))

const allFindings = verdicts.filter(Boolean).flatMap(v => (v.findings || []).map(f => ({ ...f, lens: v.lens })))
const blockers = allFindings.filter(f => f.severity === 'blocker')
log('search verify: ' + verdicts.filter(Boolean).map(v => v.verdict).join(', ') + ' | ' + blockers.length + ' blockers, ' + allFindings.length + ' findings')
return { design, design_critique: critique, fold, port_report: portReport, verify: verdicts, findings: allFindings, blockers }
