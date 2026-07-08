export const meta = {
  name: 'renderer-config-message-tier',
  description: 'Finish the rustty-renderer config-message/thread integration surface (now unblockable via the real DerivedConfig::init<C: RendererConfig>): Message::initChangeConfig<C> (construct a ChangeConfig message), the paired Message::deinit ChangeConfig path, and thread::DerivedConfig::init<C> (feasibility-gated by the thread custom_shader_animation field). Completes the cross-thread config-change path that pairs with the landed change_config. Design (guarded, STOPS if a piece is blocked) -> adversary -> fold -> port (stop-if-no-design) -> verify.',
  phases: [
    { title: 'Design', detail: 'confirm Message::initChangeConfig<C> is unblockable via the real DerivedConfig::init<C>; check thread::DerivedConfig::init<C> (the custom_shader_animation field) feasibility; scope the deinit path; WRITE the design doc (guarded). STOP-report per-piece if blocked.' },
    { title: 'Adversary-design', detail: 'attack: does initChangeConfig faithfully build the ChangeConfig message; acyclic; the deinit/ownership; is thread::init genuinely feasible or must it stay a seam; dropped test{}' },
    { title: 'Fold-fixes', detail: 'fold adversary findings INTO the doc; skipped when SOUND' },
    { title: 'Port', detail: 'implement exactly as the hardened doc specifies; STOP if the design doc is absent' },
    { title: 'Verify', detail: 'panel: R1-comment fidelity (manual), correctness (the config-message construction + deinit), gates' },
  ],
}

const REPO = '/Users/gael/projects/rustty'
const REF = '/Users/gael/projects/ghostty'
const DESIGN_DOC = REPO + '/.port/phase-2/RENDERER_CONFIG_MESSAGE_TIER_DESIGN.md'

const DESIGN_SCHEMA = {
  type: 'object', additionalProperties: false,
  required: ['wrote_doc', 'initchangeconfig_feasible', 'thread_init_feasible', 'scope_methods', 'test_plan', 'compile_boundary', 'summary'],
  properties: {
    wrote_doc: { type: 'boolean' },
    initchangeconfig_feasible: { type: 'string' },
    thread_init_feasible: { type: 'string' },
    scope_methods: { type: 'array', items: { type: 'string' } },
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
  'REPO=' + REPO + '. REFERENCE (read-only)=' + REF + ' (Zig 0.15.2, src/renderer/message.zig + Thread.zig). The design doc is ' + DESIGN_DOC + '.',
  'STATE: the rustty-renderer offscreen surface is DONE + committed (render, config-swap via change_config, resize, kitty, bg-image, custom-shaders). DerivedConfig::init<C: RendererConfig> is now REAL (the RendererConfig accessor trait, impl-d in rustty-config; acyclic). This milestone finishes the config-message/thread integration surface — the seams that were "config-coupled, deferred" and are now unblockable BECAUSE DerivedConfig::init<C> is real.',
  'THE SEAMS: (1) Message::initChangeConfig<C> (crates/rustty-renderer/src/message.rs ~:177, verbatim ref body preserved as comment) — constructs a Message::ChangeConfig from a config by calling DerivedConfig::init<C>(alloc, config) + wrapping (the reference heap-allocates the DerivedConfig + a thread ptr; rustty uses Box). (2) Message::deinit ChangeConfig path (the reference frees the ChangeConfig heap allocs — in rustty the Box drops; the comment block notes this was "left to land WITH initChangeConfig"). (3) thread::DerivedConfig::init<C> (crates/rustty-renderer/src/thread.rs ~:69) — the THREAD-tier DerivedConfig (distinct from the renderer DerivedConfig: it has a custom_shader_animation field). VERIFY whether it is now unblockable (can custom_shader_animation be extracted via a RendererConfig-style accessor?) or must stay a seam.',
  'THE ACYCLIC BOUND: Message::initChangeConfig<C> / thread::DerivedConfig::init<C> take `<C>` and now require `<C: RendererConfig>` (the renderer-defined accessor trait). rustty-config impls RendererConfig for Config; the concrete C binds in core. Confirm adding the bound breaks no compile-caller (core uses its own renderer_seam placeholder; check the callers). No renderer->config edge (check_acyclic).',
  'Rules: ' + REPO + '/.port/PORT_RULES.md — R1 (100% fidelity: every line/comment/test{}; byte-faithful), R4 (ACYCLIC; renderer gains NO config edge; run check_acyclic.sh), R5 (anything still genuinely blocked stays an unimplemented!() seam + verbatim body). G3b comment-coverage SKIPS rustty-renderer, so the adversary/verify are the ONLY R1 backstop.',
  'TESTABILITY: Message::initChangeConfig<C> is tested with the #[cfg(test)] MockConfig (impl RendererConfig) — assert it builds a Message::ChangeConfig whose DerivedConfig has the extracted fields, and that Message::deinit/Drop frees it (no leak — the tracking allocator / a Drop assert). thread::DerivedConfig::init<C> similarly if landed. Renderer canNOT dep rustty-config (even dev).',
].join('\n')

phase('Design')
const design = await agent(
  'ARCHITECT: finish the rustty-renderer config-message/thread integration surface. You MUST write ' + DESIGN_DOC + ' with the Write tool (guarded — wrote_doc=true only if written). Design ONLY; touch NO crates/. Read the reference (message.zig initChangeConfig/deinit + Thread.zig DerivedConfig.init) + the ported message.rs/thread.rs/generic.rs (DerivedConfig::init<C>, RendererConfig) + the Message::ChangeConfig variant shape + the callers of these seams (core).\n' + COMMON + '\n\n' +
  'DECIDE + specify (concrete Rust signature blocks + the caller-bound analysis + OPEN RISKS): (1) Message::initChangeConfig<C: RendererConfig> — the faithful construction (DerivedConfig::init + the Box/Message::ChangeConfig wrap); confirm the <C> -> <C: RendererConfig> bound breaks no caller. (2) The Message::deinit ChangeConfig path (the Box-drop / DerivedConfig::deinit). (3) thread::DerivedConfig::init<C> — is it unblockable (map custom_shader_animation to a RendererConfig-style accessor) or does it stay a seam? Decide + justify (STOP-report that piece if blocked, land the others). (4) the mock-C tests + a leak/Drop check. (5) dead-code + no-regression (the 52 tests stay green). Return the schema: wrote_doc, initchangeconfig_feasible, thread_init_feasible, scope_methods, test_plan, compile_boundary, summary.',
  { label: 'design:configmsg', agentType: 'general-purpose', schema: DESIGN_SCHEMA })

if (!design.wrote_doc) {
  log('DESIGN GUARD TRIPPED: design agent did NOT write the design doc. Aborting before port.')
  return { aborted: 'design-doc-not-written', design }
}
if (String(design.initchangeconfig_feasible).toLowerCase().includes('blocked')) {
  log('initChangeConfig BLOCKED: ' + design.initchangeconfig_feasible + ' — the primary seam is blocked; aborting (pick a different next surface). No port attempted.')
  return { aborted: 'initchangeconfig-blocked', design }
}
log('config-message design written. scope=' + (design.scope_methods || []).join(',').slice(0, 140) + ' | thread_init=' + String(design.thread_init_feasible).slice(0, 60))

phase('Adversary-design')
const critique = await agent(
  'ADVERSARIAL reviewer: BREAK the config-message-tier design at ' + DESIGN_DOC + '. Re-derive INDEPENDENTLY from source.\n' + COMMON + '\n\n' +
  'Read the design + message.zig initChangeConfig/deinit + Thread.zig DerivedConfig.init + the ported message.rs/thread.rs/generic.rs. CHECK, breaking each: (1) FIDELITY — does initChangeConfig faithfully port the reference construction (DerivedConfig::init + the heap-alloc/Box + the Message::ChangeConfig wrap), not summarized? Does the deinit faithfully free (Box drop == the reference frees)? (2) ACYCLICITY — does the <C: RendererConfig> bound break any real compile-caller; no renderer->config edge (check_acyclic)? (3) thread::DerivedConfig::init<C> — is the design right about its feasibility (is custom_shader_animation REALLY extractable via an accessor, or is the design over-claiming and it must stay a seam)? (4) OWNERSHIP/LEAK — any leak or double-free in the Message::ChangeConfig alloc/dealloc (the reference destroys v.impl + v.thread)? (5) any reference test{} dropped? Return the schema: verdict, blocking_issues, concrete fixes, summary.',
  { label: 'adversary:configmsg-design', agentType: 'general-purpose', schema: CRITIQUE_SCHEMA })

log('config-message adversary: ' + critique.verdict + ' | blocking=' + critique.blocking_issues.length + ' fixes=' + critique.fixes.length)

phase('Fold-fixes')
const needsFold = critique.verdict !== 'SOUND' && ((critique.fixes && critique.fixes.length) || (critique.blocking_issues && critique.blocking_issues.length))
let fold = null
if (needsFold) {
  fold = await agent(
    'DESIGN EDITOR: the adversary reviewed the config-message-tier design (' + DESIGN_DOC + ') and returned verdict=' + critique.verdict + '. FOLD its findings INTO the doc (single authoritative source).\n' + COMMON + '\n\n' +
    'ADVERSARY BLOCKING ISSUES:\n- ' + (critique.blocking_issues || []).join('\n- ') + '\nADVERSARY FIXES:\n- ' + (critique.fixes || []).join('\n- ') + '\n\n' +
    'For EACH: verify against the reference/rustty source yourself (reject any the source contradicts; note rejected + why in `folded`). Edit ' + DESIGN_DOC + ' to incorporate the valid ones. Preserve structure. Touch NO crates/. You MUST end with the doc written. Return: wrote_doc, folded, summary.',
    { label: 'fold:adversary-fixes', agentType: 'general-purpose', schema: FOLD_SCHEMA })
  if (!fold.wrote_doc) {
    log('FOLD GUARD TRIPPED: doc not present after folding. Aborting before port.')
    return { aborted: 'design-doc-missing-after-fold', design, critique, fold }
  }
  log('config-message fold-fixes: ' + (fold.folded || []).length + ' items :: ' + String(fold.summary).slice(0, 120))
} else {
  log('config-message design SOUND — no fold needed; porting directly from the doc.')
}

phase('Port')
const portReport = await agent(
  'PORTING engineer: implement the config-message/thread tier EXACTLY as specified in the (adversary-hardened) design doc ' + DESIGN_DOC + '. FIRST Read the doc + verify it EXISTS + is substantive — if absent or a stub, STOP + report "design doc missing".\n' + COMMON + '\n\n' +
  'Implement Message::initChangeConfig<C: RendererConfig> + the Message::deinit ChangeConfig path + (if the design landed it) thread::DerivedConfig::init<C>, per the design. Every line+comment ported (R1 — preserve EVERY reference comment verbatim); byte-faithful; no leak/double-free in the ChangeConfig alloc/dealloc. Add the design-specified mock-C + leak tests. Anything still genuinely blocked stays an unimplemented!() seam with verbatim body.\n' +
  'GATES (run; ALL green, never weaken): cargo build --workspace, cargo fmt --check, cargo clippy --workspace --all-targets -- -D warnings, cargo test -p rustty-renderer (+ any crate touched), bash .port/tools/gates/check_acyclic.sh (CRITICAL — no config edge), python3 .port/tools/gates/check_test_counts.py, python3 .port/tools/gates/check_comment_coverage.py. If a gate can not go green HONESTLY, STOP + report the precise blocker. Do NOT git commit. Report: methods ported (line ranges), the initChangeConfig construction + deinit, the thread::init disposition (landed or still a seam + why), the tests + results, each gate PASS/FAIL with tail, any reference comment/line you could not place.',
  { label: 'port:configmsg', agentType: 'general-purpose' })

log('config-message port done. Head: ' + String(portReport).slice(0, 200))

phase('Verify')
const LENSES = [
  'R1 COMMENT + LINE FIDELITY (the PRIMARY lens — G3b does NOT gate rustty-renderer): diff EVERY ported line AND comment of Message::initChangeConfig + Message::deinit (+ thread::DerivedConfig::init if landed) against message.zig / Thread.zig. Report any dropped/truncated/summarized comment (BLOCKER) or dropped branch/line. Confirm any reference test{} in range is ported.',
  'CORRECTNESS + ACYCLICITY + OWNERSHIP: (a) initChangeConfig faithfully builds the ChangeConfig message (DerivedConfig::init + Box wrap); (b) no leak/double-free in the alloc/dealloc (the Box drop frees exactly once — run the tests, ideally under the tracking allocator); (c) the <C: RendererConfig> bound + acyclicity — run bash .port/tools/gates/check_acyclic.sh (no config edge). Run the mock-C tests.',
  'GATES + no-regression: RUN cargo build --workspace, cargo fmt --check, cargo clippy --workspace --all-targets -- -D warnings, cargo test -p rustty-renderer, bash .port/tools/gates/check_acyclic.sh, python3 .port/tools/gates/check_test_counts.py, python3 .port/tools/gates/check_comment_coverage.py — report REAL results (tails). The existing 52 tests must still pass.',
]
const verdicts = await parallel(LENSES.map((lens, i) => () =>
  agent(
    'ADVERSARIAL verifier for the renderer config-message/thread tier (UNCOMMITTED working tree). LENS:\n' + lens + '\n' + COMMON + '\nFind the changed files (git diff/status), compare vs the reference, and RUN the gates your lens names. Report the schema: lens, verdict PASS/CONCERNS/FAIL, findings (severity, file, issue). Only REAL evidenced issues; if clean, empty findings + PASS + name the two things you attacked hardest in `lens`.',
    { label: 'verify:' + i, phase: 'Verify', agentType: 'general-purpose', schema: VERDICT_SCHEMA })))

const allFindings = verdicts.filter(Boolean).flatMap(v => (v.findings || []).map(f => ({ ...f, lens: v.lens })))
const blockers = allFindings.filter(f => f.severity === 'blocker')
log('config-message verify: ' + verdicts.filter(Boolean).map(v => v.verdict).join(', ') + ' | ' + blockers.length + ' blockers, ' + allFindings.length + ' findings')
return { design, design_critique: critique, fold, port_report: portReport, verify: verdicts, findings: allFindings, blockers }
