export const meta = {
  name: 'vt5-parser',
  description: 'rustty-vt sub-batch vt-5: the VT escape-sequence Parser cluster (parse_table + Parser), 26 tests. parse_table↔Parser mutual cycle handled struct-first (State/Action enums -> transition table -> state machine). Parser embeds osc::Parser (vt-6 done) + emits osc_dispatch. Unblocks stream (vt-9).',
  phases: [
    { title: 'Port', detail: 'parse_table + Parser struct-first: enums -> table -> state machine + 26 tests' },
    { title: 'Verify', detail: 'all Parser tests + Miri + ASan + G3b + faithfulness' },
  ],
}
const G = '/Users/gael/projects/ghostty', R = '/Users/gael/projects/rustty'
const T = G + '/src/terminal'
const GATES = R + '/.port/tools/gates'
const RULES = 'OBEY: PARITY CONSTITUTION + ' + R + '/.port/PORT_RULES.md. R1 = 100% fidelity: EVERY line; EVERY comment VERBATIM (retarget only, R2); EVERY test{} as #[test] ASSERTION-FOR-ASSERTION, INDEPENDENT expected values (no tautology); nothing dropped/weakened. Reference READ-ONLY. osc is DONE (crate::osc::Parser + osc::Command available — Parser embeds osc::Parser as a field, runs next/end/reset inline, emits Action::OscDispatch(osc::Command)). Deps ansi/charsets/csi/sgr/modes/lib all ported. Keep lib.rs PORT STATUS: stub. Run gates FOREGROUND. Commit trailer "Co-Authored-By: Claude Opus 4.8 (1M context)". '
async function robust(p, o) { let r = await agent(p, o); if (!r) r = await agent(p, { ...o, label: (o.label || 'a') + '-retry' }); return r }
const STEP = { type: 'object', additionalProperties: true, required: ['ok', 'blockers'], properties: { ok: { type: 'boolean' }, tests_added: { type: 'integer' }, builds: { type: 'boolean' }, commit: { type: 'string' }, notes: { type: 'string' }, blockers: { type: 'array', items: { type: 'string' } } } }
const VERIFY = { type: 'object', additionalProperties: true, required: ['verdict', 'blockers'], properties: { verdict: { type: 'string', enum: ['GREEN', 'RED'] }, parser_tests_passing: { type: 'integer' }, miri_clean: { type: 'boolean' }, asan_clean: { type: 'boolean' }, g3b_missing: { type: 'integer' }, tautological_tests: { type: 'array', items: { type: 'string' } }, notes: { type: 'string' }, blockers: { type: 'array', items: { type: 'string' } } } }

phase('Port')
const port = await robust(RULES +
  'TASK (port the Parser cluster, struct-first). Files: ' + T + '/Parser.zig (1099 loc, 25 tests) + ' + T + '/parse_table.zig (388 loc, 1 test) into crates/rustty-vt/src/parser.rs + crates/rustty-vt/src/parse_table.rs (both currently stub). The cycle: parse_table imports Parser (State/Action enums); Parser imports parse_table (the transition table). STRUCT-FIRST: (1) define Parser\'s State + Action + TransitionAction enums first; (2) port parse_table.zig — the VT500 transition table (Zig builds it at comptime via generateTransitionTable; reproduce faithfully as a Rust const fn / static table — byte-for-byte same transitions) + its 1 test; (3) port Parser.zig — the struct (state, osc_parser: osc::Parser field, intermediates/params buffers), init/deinit/reset, next(c) -> the 3-action tuple, the action helpers (collect/param/csiDispatch/escDispatch/oscDispatch/dcsHook etc.). Parser EMBEDS crate::osc::Parser (call osc_parser.next/end/reset inline; Action::OscDispatch carries osc::Command). Port ALL 25 Parser tests + the 1 parse_table test assertion-for-assertion. '
  + 'GATES FOREGROUND: cargo build -p rustty-vt; cargo test -p rustty-vt parser (the 25) + cargo test -p rustty-vt parse_table (the 1) pass + no regression in the full suite; cargo +nightly miri test -p rustty-vt on ~3 parser tests (state machine = Miri-fast; 0 UB/leak); clippy -p rustty-vt --all-targets -- -D warnings; fmt --check; comment coverage `python3 ' + GATES + '/check_comment_coverage.py --crate vt --ref-files ' + T + '/Parser.zig --ref-files ' + T + '/parse_table.zig --port-glob "' + R + '/crates/rustty-vt/src/parser.rs" --port-glob "' + R + '/crates/rustty-vt/src/parse_table.rs"` => missing=0. Commit "rustty: vt Parser + parse_table (vt-5)". Return ok+tests_added+builds+commit+blockers+notes. If the table or a test genuinely needs an un-ported dep, surface it.',
  { schema: STEP, effort: 'high', label: 'parser', phase: 'Port' })

phase('Verify')
const verify = await robust(RULES +
  'TASK (independent verify — default RED). Port result: ' + JSON.stringify(port && { commit: port.commit, blockers: port.blockers }) + '\n'
  + 'Confirm FOREGROUND: cargo build -p rustty-vt; cargo test -p rustty-vt (Parser 25 + parse_table 1 pass; full suite no regression — osc 243 + PageList 210 + vt-4 still green); cargo +nightly miri test -p rustty-vt on ~4 parser tests incl. an osc_dispatch one (0 UB/leak); `bash ' + GATES + '/sanitizers.sh rustty-vt parser address` (ASan clean); clippy --all-targets -D + fmt --check + ' + GATES + '/check_acyclic.sh. G3b (Parser.zig + parse_table.zig) missing=0. CRITICAL faithfulness: spot-check that the transition table matches the reference (a few representative state/byte -> (state,action) entries vs parse_table.zig — a wrong table silently breaks all VT parsing) AND 3 Parser tests (incl. an osc_dispatch + a csi_dispatch) are faithful, not tautological. Verdict GREEN only if all green AND table verified faithful AND g3b_missing=0. Report verdict + parser_tests_passing + miri_clean + asan_clean + g3b_missing + tautological_tests + blockers.',
  { schema: VERIFY, effort: 'high', label: 'verify', phase: 'Verify' })
return { port, verify }
