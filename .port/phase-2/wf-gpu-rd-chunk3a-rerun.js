export const meta = {
  name: 'gpu-rd-chunk3a-rerun',
  description: 'Clean re-attempt of the broken Chunk-3a workflow (its design agent flaked): independently re-derive the Chunk-3a design FROM SOURCE + adversarially attack the COMMITTED Chunk-3a (bd2c2eb + 82e6a34) with a 3-lens panel running the gates. NON-DESTRUCTIVE validation — reports flaws for fix, does not re-port committed green code.',
  phases: [
    { title: 'Re-derive', detail: 'independently re-derive the correct Chunk-3a design from source + diff vs the committed design/code (design-doc guard)' },
    { title: 'Attack', detail: '3 adversary lenses attack the committed code + run the gates' },
  ],
}

const REPO = '/Users/gael/projects/rustty'
const REF = '/Users/gael/projects/ghostty'

const DERIVE_SCHEMA = {
  type: 'object', additionalProperties: false,
  required: ['wrote_doc', 'agreement', 'divergences', 'summary'],
  properties: {
    wrote_doc: { type: 'boolean' },
    agreement: { type: 'string', enum: ['MATCHES', 'MATCHES-WITH-NITS', 'DIVERGES'] },
    divergences: { type: 'array', items: { type: 'string' } },
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
  'REPO=' + REPO + '. REFERENCE (read-only)=' + REF + ' (Zig 0.15.2).',
  'CONTEXT: GPU R-d Chunk 3a is ALREADY COMMITTED (bd2c2eb the 6-type config hoist; 82e6a34 the renderer skeleton) and gate-green. The driving workflow BROKE (its design agent returned garbage, wrote no design doc); the port agents recovered by self-authoring the design + code, which was NOT independently adversary-reviewed against a real design. This re-attempt validates the committed work properly. DO NOT re-port or rewrite the committed code — attack it and report flaws.',
  'Committed Chunk-3a surface: crates/rustty-renderer/src/generic.rs (the monomorphic Renderer struct + nested SwapChain/FrameState/CustomShaderState + DerivedConfig + init<C>/deinit seams); crates/rustty-render-types/src/lib.rs (the 6 hoisted config types: WindowColorspace/WindowPaddingColor/BackgroundImagePosition/BackgroundImageFit/FontShapingBreak/BackgroundBlur + DeepEqual); crates/rustty-config/src/{value_types,config_struct,c_get,deep_equal}.rs (re-export + config-local free-fn parsing); crates/rustty-renderer/src/message.rs (ChangeConfig -> real DerivedConfig); crates/rustty-vt/src/lib.rs (Scrollbar alias). Design doc: .port/phase-2/GPU_RD_CHUNK3A_DESIGN.md.',
  'Rules: ' + REPO + '/.port/PORT_RULES.md — R1 (100% fidelity: every line/comment/test{} ported), R4 (crate graph ACYCLIC; renderer canNOT dep rustty-config), R5 (deferred = unimplemented!() seam with verbatim Zig body). Decision: generic.zig ports MONOMORPHICALLY (concrete Renderer over Metal). AlphaBlending precedent (5b9eb65) = the config-hoist pattern.',
].join('\n')

phase('Re-derive')
const derive = await agent(
  'You are an INDEPENDENT ARCHITECT. Re-derive the CORRECT Chunk-3a design FROM SOURCE (do not read the committed design doc first — derive it yourself), then diff your derivation against the committed artifacts to catch anything the recovered version got wrong.\n' + COMMON + '\n\n' +
  'STEP 1 (derive fresh): from ' + REF + '/src/renderer/generic.zig (the Renderer struct decl ~46-95 + fields ~96-232; DerivedConfig 536-660; SwapChain/FrameState/CustomShaderState 241-470) + ' + REPO + '/crates/rustty-config value-type defs, independently determine: (a) which of the ~17 config-typed DerivedConfig fields are cleanly HOISTABLE to rustty-render-types vs NOT (cycle/leaf-invariant reasons) — you should arrive at a specific split; (b) how surface_mailbox must be handled given apprt-api deps renderer AND apprt::surface::Mailbox does not exist; (c) what renderer-local types must be authored (AtomicHealth, DisplayLink); (d) the SwapChain semaphore backpressure (permits=buf_count, next_frame wait / release_frame post / deinit drain).\n' +
  'STEP 2 (diff vs committed): NOW read .port/phase-2/GPU_RD_CHUNK3A_DESIGN.md + crates/rustty-renderer/src/generic.rs + the hoist diff (git show bd2c2eb). Compare your independent derivation against them. Flag every DIVERGENCE where the committed choice differs from what you derived, and judge whether the committed choice is WRONG or just an alternative.\n' +
  'GUARD: you MUST return the schema with wrote_doc=false (you are validating, not writing) and a concrete agreement verdict. Report: wrote_doc, agreement (MATCHES / MATCHES-WITH-NITS / DIVERGES), the concrete divergences (each: what you derived vs what committed, and is committed WRONG), and a summary of whether the committed Chunk-3a design is sound.',
  { label: 'rederive:chunk3a', agentType: 'general-purpose', schema: DERIVE_SCHEMA })

log('Re-derive: agreement=' + derive.agreement + ' divergences=' + derive.divergences.length + ' | ' + String(derive.summary).slice(0, 160))

phase('Attack')
const LENSES = [
  'CONFIG-HOIST + ACYCLICITY: attack the 6-type hoist + the config parsing refactor. Are exactly the RIGHT 6 types hoisted (no type mis-hoisted that should have stayed, none that should have hoisted left behind)? Do the config-local free-fn string_to_enum/to_name/to_name_z reproduce the EXACT byte literals of the removed bare_enum_names! macro for all 6 (a behavioral regression in parse/format/C-API is a blocker)? Does render-types stay a pure leaf (grep its Cargo.toml)? RUN: cargo test -p rustty-config, bash .port/tools/gates/check_acyclic.sh, python3 .port/tools/gates/check_test_counts.py, python3 .port/tools/gates/check_comment_coverage.py — report the real results. Try hardest to find a parse/format divergence and a mis-hoist.',
  'MAILBOX-SEAM + PLACEHOLDER-SAFETY + DerivedConfig: attack the deferral decisions. Is the surface_mailbox deferred-placeholder seam sound (does anything in Chunk-3a scope actually READ it, which would break)? Are the TerminalColor/Path/RepeatablePath inhabited placeholders SAFE — i.e., is DerivedConfig::init genuinely deferred (unimplemented!) so the placeholders never reach a real read, and does the design PROVE the Chunk-4 golden will need the real init<C> (not the placeholder)? Is AtomicHealth a faithful port of std.atomic.Value(Health) (correct ordering/semantics)? Is DisplayLink a correct macOS placeholder? Is message.rs ChangeConfig pointing at the real DerivedConfig without cycling? Try hardest to find a placeholder that IS reached, or a cycle.',
  'R1-FIDELITY + SEMAPHORE + DEAD-CODE: attack faithfulness. Diff generic.zig Renderer/SwapChain/FrameState/CustomShaderState/DerivedConfig line-and-comment against generic.rs — is EVERY line + comment ported or R5-seamed (the now-restored generic.zig:46-80 hierarchy diagram present; any OTHER dropped comment)? Is the SwapChain semaphore backpressure faithful (with_permits(SWAP_CHAIN_COUNT); next_frame wait(); release_frame post(); deinit drains buf_count; the errdefer-post reasoning)? Is FrameState/CustomShaderState init/resize/swap faithful? Is the pub-widening dead-code approach sound (no field/method actually dead under -D warnings)? RUN cargo clippy -p rustty-renderer --all-targets -- -D warnings + cargo test -p rustty-renderer. Try hardest to find a dropped comment/line and a semaphore-semantics divergence.',
]
const verdicts = await parallel(LENSES.map((lens, i) => () =>
  agent(
    'ADVERSARIAL validator re-attempting the broken Chunk-3a workflow. Attack the COMMITTED (do NOT rewrite) Chunk-3a. LENS:\n' + lens + '\n' + COMMON + '\nCompare vs ' + REF + '/src/renderer/generic.zig and the config sources. RUN the gates your lens names. Report the schema: lens, verdict PASS/CONCERNS/FAIL, findings (severity blocker/major/minor, file, issue — a real regression/cycle/dropped-comment/reached-placeholder is a blocker). If genuinely clean after trying hard, empty findings + PASS + name the two things you attacked hardest.',
    { label: 'attack:' + i, phase: 'Attack', agentType: 'general-purpose', schema: VERDICT_SCHEMA })))

const allFindings = verdicts.filter(Boolean).flatMap(v => (v.findings || []).map(f => ({ ...f, lens: v.lens })))
const blockers = allFindings.filter(f => f.severity === 'blocker')
const majors = allFindings.filter(f => f.severity === 'major')
log('Attack: ' + verdicts.filter(Boolean).map(v => v.verdict).join(', ') + ' | ' + blockers.length + ' blockers, ' + majors.length + ' majors')

return {
  rederive: derive,
  verdicts,
  blockers,
  majors,
  clean: blockers.length === 0 && derive.agreement !== 'DIVERGES',
}
