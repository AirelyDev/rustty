export const meta = {
  name: 'vt12b4-sgr',
  description: 'rustty-vt vt-12b-4: fill Terminal SGR/protected (set_attribute, print_attributes[93], set_protected_mode) + their tests + the SGR/protected-dependent tests deferred from earlier batches (erase-with-bg, protected eraseDisplay, etc.). Leaves modes/resize/switch/semantic/Handler todo!().',
  phases: [
    { title: 'Fill', detail: 'Terminal SGR/protected methods + their tests + unblocked deferred tests' },
    { title: 'Verify', detail: 'scoped G3b + tests + Miri-subset + ASan + faithfulness + no over-reach' },
  ],
}
const G = '/Users/gael/projects/ghostty', R = '/Users/gael/projects/rustty'
const T = G + '/src/terminal'
const GATES = R + '/.port/tools/gates'
const SCOPED_G3B = 'python3 ' + GATES + '/check_comment_coverage.py --crate vt --ref-files ' + T + '/Terminal.zig --port-glob "' + R + '/crates/rustty-vt/src/terminal.rs"'
const RULES = 'OBEY: PARITY CONSTITUTION + ' + R + '/.port/PORT_RULES.md. R1 = 100% fidelity (every line/comment verbatim; tests assertion-for-assertion, independent expected values). Reference READ-ONLY ' + T + '/Terminal.zig. vt-12b-1/2/3a/3b landed; 1103 tests pass. Plan: ' + R + '/.port/phase-2/TERMINAL_PLAN.md. '
  + 'KEY: set_attribute applies an sgr::Attribute to the cursor style via Screen::set_attribute + manual_style_update (both ported); print_attributes (~93 loc) encodes the current SGR state to a writer (&mut dyn io::Write); set_protected_mode sets the protected-mode flag. sgr::Attribute + the parser are ported. [:0]const u8->Vec<u8>. SOUNDNESS RULE (memory): no &mut Page/Row/[Cell] args held across assert_integrity — use raw ptrs. '
  + 'MIRI: per MIRI_EXCEPTIONS.md (restored) Miri-subset on new tests in ISOLATION. PROCESS LESSON: SCOPED G3b of JUST the methods filled this batch (diff vs prior baseline) — a single dropped comment fails this gate. '
  + 'IMPORTANT: fill ONLY set_attribute/print_attributes/set_protected_mode; leave deccolm/resize/switch_screen[_mode]/full_reset/semantic_prompt/cursor_is_at_prompt/set+get_pwd/title/Handler-dispatch todo!(). Keep lib.rs PORT STATUS: stub. Run gates FOREGROUND. Commit trailer "Co-Authored-By: Claude Opus 4.8 (1M context)". '
async function robust(p, o) { let r = await agent(p, o); if (!r) r = await agent(p, { ...o, label: (o.label || 'a') + '-retry' }); return r }
const STEP = { type: 'object', additionalProperties: true, required: ['ok', 'blockers'], properties: { ok: { type: 'boolean' }, tests_passing: { type: 'integer' }, methods_filled: { type: 'array', items: { type: 'string' } }, scoped_g3b_clean: { type: 'boolean' }, builds: { type: 'boolean' }, commit: { type: 'string' }, notes: { type: 'string' }, blockers: { type: 'array', items: { type: 'string' } } } }
const VERIFY = { type: 'object', additionalProperties: true, required: ['verdict', 'blockers'], properties: { verdict: { type: 'string', enum: ['GREEN', 'RED'] }, tests_passing: { type: 'integer' }, miri_subset_clean: { type: 'boolean' }, asan_clean: { type: 'boolean' }, scoped_g3b_clean: { type: 'boolean' }, no_overreach: { type: 'boolean' }, tautological_tests: { type: 'array', items: { type: 'string' } }, notes: { type: 'string' }, blockers: { type: 'array', items: { type: 'string' } } } }

phase('Fill')
const fill = await robust(RULES +
  'TASK (fill Terminal SGR/protected). In crates/rustty-vt/src/terminal.rs, replace the todo!() bodies of ONLY: set_attribute, print_attributes, set_protected_mode — faithful ports from ' + T + '/Terminal.zig. Then PORT the SGR/protected tests + any tests deferred from earlier batches now unblocked (erase-with-SGR-bg, protected-mode eraseDisplay/eraseLine/eraseChars, setAttribute groups). DEFER tests needing still-todo!() methods (modes/resize/semantic). Leave all other methods todo!(). '
  + 'GATES FOREGROUND: cargo build -p rustty-vt; cargo test -p rustty-vt (your tests pass + 1103 prior no regression); SCOPED G3b `' + SCOPED_G3B + '` — 0 missing attributable to the filled methods (diff vs baseline; a single dropped comment FAILS); cargo +nightly miri test -p rustty-vt on 2 new tests (ISOLATED, 0 UB/leak); `bash ' + GATES + '/sanitizers.sh rustty-vt terminal address` (ASan clean); clippy -p rustty-vt --all-targets -- -D warnings; fmt --check. Commit "rustty: Terminal SGR/protected + tests (vt-12b-4)". Return ok+tests_passing+methods_filled+scoped_g3b_clean+builds+commit+notes+blockers.',
  { schema: STEP, effort: 'high', label: 'fill', phase: 'Fill' })

phase('Verify')
const verify = await robust(RULES +
  'TASK (independent verify — default RED). Fill: ' + JSON.stringify(fill && { commit: fill.commit, methods_filled: fill.methods_filled, blockers: fill.blockers }) + '\n'
  + 'Confirm FOREGROUND: cargo build -p rustty-vt; cargo test -p rustty-vt (no regression vs 1103); cargo +nightly miri test -p rustty-vt on ~2 new tests (ISOLATED, 0 UB/leak); `bash ' + GATES + '/sanitizers.sh rustty-vt terminal address` (ASan clean); clippy --all-targets -D + fmt --check + ' + GATES + '/check_acyclic.sh. SCOPED G3b (`' + SCOPED_G3B + '`): the filled SGR/protected methods drop 0 comments — grep the reference comment fragments of set_attribute/print_attributes/set_protected_mode and confirm each is present in terminal.rs (scoped_g3b_clean). NO OVER-REACH: only SGR/protected filled; modes/resize/switch/semantic/Handler still todo!() (no_overreach). Spot-check 3 tests (a setAttribute 256-color, a protected eraseDisplay, a print_attributes round-trip) faithful vs Terminal.zig, not tautological. Verdict GREEN only if build + tests + Miri-subset + ASan + scoped_g3b_clean + clippy + fmt green AND no_overreach AND no regression. Report verdict + tests_passing + miri_subset_clean + asan_clean + scoped_g3b_clean + no_overreach + tautological_tests + blockers.',
  { schema: VERIFY, effort: 'high', label: 'verify', phase: 'Verify' })
return { fill, verify }
