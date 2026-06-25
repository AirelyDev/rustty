export const meta = {
  name: 'vt12-finale',
  description: 'Terminal (vt-12) FINALE: integrate gate (whole-Terminal + stream_terminal G3b, full suite, ASan, broader Miri pass, clippy/fmt/acyclic) -> the complete-Terminal commit; then adversarial parity review of Terminal + stream_terminal (376+65 tests, Handler dispatch, *mut Terminal soundness, byte payloads, response helpers). Completes the crown jewel\'s third giant.',
  phases: [
    { title: 'Integrate', detail: 'whole-Terminal+stream_terminal G3b + suite + ASan + Miri-pass + ci -> complete-Terminal commit' },
    { title: 'Review', detail: 'adversarial parity review (tests + Handler + *mut Terminal + byte payloads + responses)' },
  ],
}
const G = '/Users/gael/projects/ghostty', R = '/Users/gael/projects/rustty'
const T = G + '/src/terminal'
const GATES = R + '/.port/tools/gates'
const TG3B = 'python3 ' + GATES + '/check_comment_coverage.py --crate vt --ref-files ' + T + '/Terminal.zig --port-glob "' + R + '/crates/rustty-vt/src/terminal.rs"'
const STG3B = 'python3 ' + GATES + '/check_comment_coverage.py --crate vt --ref-files ' + T + '/stream_terminal.zig --port-glob "' + R + '/crates/rustty-vt/src/stream_terminal.rs"'
const RULES = 'OBEY: PARITY CONSTITUTION + ' + R + '/.port/PORT_RULES.md. Reference READ-ONLY ' + T + '/{Terminal.zig,stream_terminal.zig}. vt-12a/b/c done — Terminal + stream_terminal fully ported (modulo apc font-block); 1417 tests pass. Deferrals: DEFERRALS.md (apc cluster = continuing font-block deferral past vt-12; the only remaining vt deferral). Miri strategy: MIRI_EXCEPTIONS.md (per-sub-batch Miri restored; broader pass at finales). Run gates FOREGROUND. Commit trailer "Co-Authored-By: Claude Opus 4.8 (1M context)". '
async function robust(p, o) { let r = await agent(p, o); if (!r) r = await agent(p, { ...o, label: (o.label || 'a') + '-retry' }); return r }
const INTEG = { type: 'object', additionalProperties: true, required: ['verdict', 'blockers'], properties: { verdict: { type: 'string', enum: ['GREEN', 'RED'] }, suite_tests: { type: 'integer' }, terminal_g3b_missing: { type: 'integer' }, st_g3b_missing: { type: 'integer' }, residual_all_apc_deferred: { type: 'boolean' }, asan_clean: { type: 'boolean' }, miri_pass_clean: { type: 'boolean' }, commit: { type: 'string' }, notes: { type: 'string' }, blockers: { type: 'array', items: { type: 'string' } } } }
const REVIEW = { type: 'object', additionalProperties: true, required: ['verdict', 'blockers'], properties: { verdict: { type: 'string', enum: ['PARITY', 'NOT_PARITY'] }, tautological_or_weakened: { type: 'array', items: { type: 'string' } }, handler_faithful: { type: 'boolean' }, mut_terminal_sound: { type: 'boolean' }, byte_payloads_faithful: { type: 'boolean' }, unsound_unsafe_or_ub: { type: 'array', items: { type: 'string' } }, notes: { type: 'string' }, blockers: { type: 'array', items: { type: 'string' } } } }

phase('Integrate')
const integ = await robust(RULES +
  'TASK (integrate Terminal vt-12). LOOP-to-green, all FOREGROUND: (1) cargo test -p rustty-vt => all pass (>=1417; report count; verify the 376 Terminal + 65 stream_terminal reference test{} blocks are present). (2) `' + TG3B + '` + `' + STG3B + '` => report missing; EVERY residual missing MUST map to the apc font-block deferral (Terminal glyph_protocol/glyph_glossary + stream_terminal apcEnd .kitty/.glyph arms) — confirm residual_all_apc_deferred (list any NOT apc-covered as a blocker). (3) `bash ' + GATES + '/sanitizers.sh rustty-vt terminal address` + `... stream_terminal address` + `... kitty address` => 0 ASan findings (asan_clean). (4) BROADER MIRI PASS (the finale check per MIRI_EXCEPTIONS.md): cargo +nightly miri test -p rustty-vt in ISOLATION on a representative subset across Terminal groups — print, cursor, an erase, an insert_lines/scroll, set_attribute, a mode, resize, switch_screen, semantic_prompt, a Handler stream-driven test, + the 2 prior-UB-fixed paths (a clear_cells-driven test, a kitty_images test) => 0 UB/leak (miri_pass_clean); heavy multi-page tests stay ASan-covered. (5) clippy -p rustty-vt --all-targets -D + fmt --check + ' + GATES + '/check_acyclic.sh + ' + R + '/.port/tools/ci.sh non-Miri portions. Keep lib.rs PORT STATUS: stub (rustty-vt still has render/input/search/tmux + apc to go). Commit "rustty: complete Terminal (vt-12) — Terminal + stream_terminal + reactivated formatter/kitty/Screen tests". GREEN only if pass + both G3b residuals all-apc-deferred + asan_clean + miri_pass_clean. Return all fields + commit.',
  { schema: INTEG, effort: 'high', label: 'integrate', phase: 'Integrate' })

phase('Review')
const review = await robust(RULES +
  'TASK (adversarial parity review — default NOT_PARITY). Integrate: ' + JSON.stringify(integ) + '\n'
  + '1. TESTS: the 376 Terminal + 65 stream_terminal reference test{} ported, NONE tautological (expected values independent of code under test) or weakened. Sample the hardest: a print wide-char/grapheme, an insert_lines/delete_lines margin-crossing, a resize-reflow, a switch_screen alt, a DA/DSR response (via vtStream + write_pty capture), a semantic_prompt OSC133, a mode set/reset through the stream.\n'
  + '2. HANDLER (stream_terminal): is the impl stream::Handler dispatch faithful to vtFallible (every action -> the right Terminal method)? Are the Effects callbacks (write_pty/bell/...) faithfully invoked? handler_faithful.\n'
  + '3. *mut Terminal SOUNDNESS: the Stream owns Handler which aliases Terminal via *mut Terminal; confirm the deref discipline (no &mut held across re-reads; the strongly-protected SB class absent — re-spot-check Miri on 2 stream-driven tests). mut_terminal_sound.\n'
  + '4. BYTE PAYLOADS: pwd/title are Vec<u8>+NUL (getters Option<&[u8]>), xtversion/response strings faithful (R2 ghostty->rustty only where it is the product name, NOT effect-provided data). byte_payloads_faithful.\n'
  + '5. Any unsound unsafe / UB across terminal.rs + stream_terminal.rs (beyond the benign term_size int-to-ptr warning). \n'
  + 'Any tautology/weakening/unfaithful-dispatch/unsound-unsafe => NOT_PARITY with specifics. Confirm the ONLY remaining vt deferral is the apc font-block. Return verdict + tautological_or_weakened + handler_faithful + mut_terminal_sound + byte_payloads_faithful + unsound_unsafe_or_ub + blockers + notes.',
  { schema: REVIEW, effort: 'high', label: 'review', phase: 'Review' })
return { integ, review }
