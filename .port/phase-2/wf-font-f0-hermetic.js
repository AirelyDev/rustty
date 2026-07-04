export const meta = {
  name: 'font-f0-hermetic',
  description: 'Heavy-tier F0 (hermetic, no z2d/C/vt): Atlas.zig (13t, pure texture-atlas packing) + CodepointMap.zig (1t; discovery.zig dep is likely just a Descriptor type -> seam or a thin port) into rustty-font (PURE, below vt). Validates the F0 gate path. CodepointMap un-blocks config RepeatableCodepointMap.',
  phases: [
    { title: 'Port', detail: 'Atlas + CodepointMap -> rustty-font (pure, no vt/C)' },
    { title: 'Verify', detail: 'count-gate + tests + Miri/ASan + G3b + faithfulness + un-block config RepeatableCodepointMap' },
  ],
}
const G = '/Users/gael/projects/ghostty', R = '/Users/gael/projects/rustty'
const GF = G + '/src/font'
const GATES = R + '/.port/tools/gates'
const RULES = 'OBEY: PARITY CONSTITUTION + ' + R + '/.port/PORT_RULES.md. R1 = PORT EVERY test{} (count is a CONTRACT); EVERY line/comment verbatim; assertion-for-assertion, INDEPENDENT expected values. R3 faithful. R4: rustty-font stays PURE & BELOW vt (NO vt edge, memory apc-glyf-font-layering-d1). Reference READ-ONLY ' + GF + '/. rustty-font has the parsing layer (glyf/Glyph/Metrics/opentype/embedded/nerd, 30t). Plan: ' + R + '/.port/phase-2/NEXT_PHASE_PLAN.md + memory heavy-tier-strategy (F0). '
  + 'KEY: Atlas.zig (889loc, 13t — texture-atlas rect packing; deps quirks/fastmem/tripwire->rustty-util, os/wasm + zig-js are wasm-gated -> #[cfg(feature="wasm")] OFF). CodepointMap.zig (125loc, 1t — codepoint->descriptor map; imports discovery.zig — if it only references a Descriptor/type, SEAM it as a local type or a thin rustty-font type [the C-lib discovery itself lands later in rustty-font-shaper F3]; do NOT pull a C-lib edge into pure rustty-font). [:0]const u8->Vec<u8>. SOUNDNESS (memory): raw-ptr discipline. '
  + 'MIRI per MIRI_EXCEPTIONS.md ISOLATED (pure -> Miri-clean). PROCESS LESSONS (memory): port ALL tests (count enforces), scoped-G3b. Run gates FOREGROUND. Commit trailer "Co-Authored-By: Claude Opus 4.8 (1M context)". '
async function robust(p, o) { let r = await agent(p, o); if (!r) r = await agent(p, { ...o, label: (o.label || 'a') + '-retry' }); return r }
const STEP = { type: 'object', additionalProperties: true, required: ['ok', 'blockers'], properties: { ok: { type: 'boolean' }, tests_added: { type: 'integer' }, builds: { type: 'boolean' }, commit: { type: 'string' }, notes: { type: 'string' }, blockers: { type: 'array', items: { type: 'string' } } } }
const VERIFY = { type: 'object', additionalProperties: true, required: ['verdict', 'blockers'], properties: { verdict: { type: 'string', enum: ['GREEN', 'RED'] }, tests_passing: { type: 'integer' }, count_ok: { type: 'boolean' }, font_no_vt_edge: { type: 'boolean' }, miri_subset_clean: { type: 'boolean' }, g3b_missing: { type: 'integer' }, tautological_tests: { type: 'array', items: { type: 'string' } }, notes: { type: 'string' }, blockers: { type: 'array', items: { type: 'string' } } } }

phase('Port')
const port = await robust(RULES +
  'TASK (F0 hermetic). Port FULLY (every test) into rustty-font: ' + GF + '/Atlas.zig (13t) -> src/atlas.rs (wasm parts #[cfg(feature="wasm")] OFF); ' + GF + '/CodepointMap.zig (1t) -> src/codepoint_map.rs (seam the discovery.zig type-dep as a local Descriptor type or a thin rustty-font type — keep rustty-font PURE, no C/vt edge). Wire lib.rs. GATES FOREGROUND: cargo build -p rustty-font; cargo test -p rustty-font (Atlas 13 + CodepointMap 1 pass + no regression vs 30); cargo +nightly miri test -p rustty-font on 2 Atlas tests ISOLATED (0 UB/leak); clippy -p rustty-font --all-targets -D; fmt; check_comment_coverage.py scoped to Atlas.zig + CodepointMap.zig missing=0; check_acyclic.sh (rustty-font still has NO rustty-vt edge). Commit "rustty: font F0 Atlas + CodepointMap (14 tests, hermetic)". Return ok+tests_added+builds+commit+blockers.',
  { schema: STEP, effort: 'high', label: 'port', phase: 'Port' })

phase('Verify')
const verify = await robust(RULES + 'Port: ' + JSON.stringify(port && port.commit) + '\n'
  + 'TASK (independent verify — default RED). Add font_atlas (Atlas.zig 13) + font_codepoint_map (CodepointMap.zig 1) mappings to ' + GATES + '/check_test_counts.py (GHF base = src/font). Confirm FOREGROUND: cargo build --workspace; `python3 ' + GATES + '/check_test_counts.py` => font_atlas/font_codepoint_map rust>=zig + WHOLE gate GREEN (count_ok); cargo test -p rustty-font (Atlas 13 + CodepointMap 1 + parsing 30 pass); cargo +nightly miri test -p rustty-font ~3 ISOLATED (0 UB/leak); clippy --all-targets -D + fmt --check + ' + GATES + '/check_acyclic.sh + CONFIRM rustty-font has NO rustty-vt edge (font_no_vt_edge — the D1 invariant); G3b Atlas.zig + CodepointMap.zig missing=0. If CodepointMap now provides the type config`s RepeatableCodepointMap needs, note that config can un-defer it (a follow-up). Spot-check 2 tests (an Atlas reserve/grow, the CodepointMap get) faithful, not tautological. Verdict GREEN only if count_ok + font_no_vt_edge + build + tests + Miri + g3b=0 + clippy + fmt + acyclic green AND no regression. Report verdict + tests_passing + count_ok + font_no_vt_edge + miri_subset_clean + g3b_missing + tautological_tests + blockers + notes(config RepeatableCodepointMap unblockable?).',
  { schema: VERIFY, effort: 'high', label: 'verify', phase: 'Verify' })
return { port, verify }
