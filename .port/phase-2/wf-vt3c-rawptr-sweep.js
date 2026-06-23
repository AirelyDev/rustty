export const meta = {
  name: 'vt3c-rawptr-sweep',
  description: 'A2: blanket-convert every `let X = unsafe { &mut (*Y).data }` in pagelist/tests.rs to a raw `*mut PageData` (addr_of_mut!) + fix use sites. Eliminates the Stacked-Borrows UB class (a held &mut invalidated by Pin::row_and_cell shared-retag) deterministically — and is MORE faithful (the reference uses a Zig pointer). Zero assertion/logic changes. Loop-until-zero, per-batch commits, compiler-checked.',
  phases: [
    { title: 'Sweep', detail: 'convert ~20 bindings/batch -> raw ptr + fix uses, cargo build+test green, commit; loop until none remain' },
    { title: 'Verify', detail: 'native 205 pass + clippy/fmt + G3b 0 + scoped Miri on the 2 known-UB tests = clean' },
  ],
}
const G = '/Users/gael/projects/ghostty', R = '/Users/gael/projects/rustty'
const GATES = R + '/.port/tools/gates'
const T = R + '/crates/rustty-vt/src/pagelist/tests.rs'
const PAT = 'unsafe { &mut (*'  // the binding signature to eliminate
const G3B_FULL = 'python3 ' + GATES + '/check_comment_coverage.py --crate vt --ref-files ' + G + '/src/terminal/PageList.zig --port-glob "' + R + '/crates/rustty-vt/src/pagelist/*.rs"'
const RULES = 'OBEY: PARITY CONSTITUTION + ' + R + '/.port/PORT_RULES.md. This is a SOUNDNESS fix in TEST code — change ZERO assertions/logic/expected-values, only the page-access idiom. '
  + 'WHY: the porters translated the reference `const page = &s.pages.first.?.data` (a Zig *pointer*) as `let page = unsafe { &mut (*..).data }` (a Rust unique &mut). When that &mut is held across a call that reads the same page through a pin (Pin::row_and_cell at pin.rs:219, lookup_grapheme, iterators), Miri Stacked-Borrows invalidates it -> UB (2 confirmed: pagelist_clone_partial_trimmed_left_reclaims_styles, pagelist_resize_reflow_less_cols_wrapped_rows_with_graphemes). The faithful translation of a Zig pointer is a RAW `*mut`. '
  + 'TRANSFORM (apply to EVERY occurrence in ' + T + '): change `let <v> = unsafe { &mut (*<expr>).data };` to `let <v> = unsafe { core::ptr::addr_of_mut!((*<expr>).data) };` (now `<v>: *mut PageData`), then fix EVERY use of <v>: `<v>.field`/`<v>.method(..)` -> `(*<v>).field`/`(*<v>).method(..)` inside an `unsafe { }` block; if <v> is passed where `&mut PageData` is expected, pass `unsafe { &mut *<v> }` (a fresh short-lived reborrow, NOT held across pin calls). cargo build will flag every missed use site (raw ptr has no auto-deref) — fix until it compiles. Run gates FOREGROUND. Commit trailer "Co-Authored-By: Claude Opus 4.8 (1M context)". '
async function robust(p, o) { let r = await agent(p, o); if (!r) r = await agent(p, { ...o, label: (o.label || 'a') + '-retry' }); return r }
const STEP = { type: 'object', additionalProperties: true, required: ['ok', 'remaining', 'blockers'], properties: { ok: { type: 'boolean' }, converted: { type: 'integer' }, remaining: { type: 'integer' }, commit: { type: 'string' }, notes: { type: 'string' }, blockers: { type: 'array', items: { type: 'string' } } } }
const VERIFY = { type: 'object', additionalProperties: true, required: ['verdict', 'blockers'], properties: { verdict: { type: 'string', enum: ['GREEN', 'RED'] }, remaining_bindings: { type: 'integer' }, tests_passing: { type: 'integer' }, miri_known_ub_clean: { type: 'boolean' }, g3b_missing: { type: 'integer' }, notes: { type: 'string' }, blockers: { type: 'array', items: { type: 'string' } } } }

phase('Sweep')
let remaining = 999, iter = 0, noProgress = 0
const steps = []
while (remaining > 0 && iter < 16) {
  iter++
  const r = await robust(RULES +
    'TASK: convert the NEXT ~20 lowest-line `' + PAT + '...).data };` bindings in ' + T + ' to the raw-pointer idiom (per TRANSFORM above) and fix their use sites. Find them: `grep -n "&mut (\\*" ' + T + ' | grep "\\.data }"`. Do ~20, then: cargo build -p rustty-vt --tests (GREEN — fixes every flagged use site), cargo test -p rustty-vt pagelist (still 205 pass / 0 fail), clippy -p rustty-vt --all-targets -- -D warnings, fmt --check. Commit "rustty: raw-ptr sweep tests.rs (vt-3c UB-class) — batch ' + iter + '". Return ok + converted (this batch) + remaining = `grep -c "&mut (\\*.*).data }" ' + T + '` AFTER commit + commit + blockers.',
    { schema: STEP, effort: 'high', label: 'sweep:' + iter, phase: 'Sweep' })
  steps.push(r)
  if (!r) break
  if (typeof r.remaining === 'number') { if (r.remaining >= remaining) noProgress++; else { noProgress = 0; remaining = r.remaining } }
  if (r.ok === false && r.blockers && r.blockers.length) break
  if (noProgress >= 2) break
  log(`raw-ptr sweep: ${remaining} bindings remaining (iter ${iter})`)
}

phase('Verify')
const verify = await robust(RULES +
  'TASK (independent verify — default RED). (1) remaining_bindings = `grep -c "&mut (\\*.*).data }" ' + T + '` MUST be 0. (2) cargo test -p rustty-vt pagelist = 205 pass/0 fail. (3) clippy --all-targets -D warnings + fmt --check green. (4) ' + G3B_FULL + ' => g3b_missing (comment coverage unchanged by the sweep). (5) The two PREVIOUSLY-UB tests are now Miri-clean: run FOREGROUND `MIRIFLAGS="-Zmiri-disable-isolation" cargo +nightly miri test -p rustty-vt -- --exact pagelist::tests::pagelist_clone_partial_trimmed_left_reclaims_styles pagelist::tests::pagelist_resize_reflow_less_cols_wrapped_rows_with_graphemes --test-threads=1` -> both pass, 0 UB (miri_known_ub_clean). GREEN only if remaining_bindings==0 AND 205 pass AND miri_known_ub_clean AND build/clippy/fmt green. Return verdict + remaining_bindings + tests_passing + miri_known_ub_clean + g3b_missing + blockers.',
  { schema: VERIFY, effort: 'high', label: 'verify', phase: 'Verify' })
return { steps, verify }
