export const meta = {
  name: 'vt3c-tests-port',
  description: 'Port the remaining ~162 PageList tests (43/205 done) to reach all 205 — assertion-for-assertion, no tautology. Full method surface (117/117) now exists, so tests compile. Loop-until-205 with per-batch commits (resumable) + completeness critic. Native cargo test only; Miri + adversarial review are the separate integrate workflow.',
  phases: [
    { title: 'Tests', detail: 'loop: port ~10 lowest-index un-ported tests per batch, cargo test, commit' },
    { title: 'Complete', detail: 'completeness critic: all 205 mapped + no tautology' },
  ],
}
const G = '/Users/gael/projects/ghostty', R = '/Users/gael/projects/rustty'
const RULES = 'OBEY: PARITY CONSTITUTION + ' + R + '/.port/PORT_RULES.md. Port Zig `test {...}` blocks to Rust #[test] ASSERTION-FOR-ASSERTION: every assertion preserved, every edge/error case, comments verbatim (retarget ghostty->rustty only). '
  + 'TAUTOLOGY TRAP (critical): expected values must come from an INDEPENDENT source (hand-computed / the test\'s own literal setup), NEVER read back from the same code path under test. Do not weaken, skip, or `#[ignore]` any test. '
  + 'The full PageList method surface is now ported (117/117) — every method the tests call exists, so do NOT defer or stub any test. Reference READ-ONLY ' + G + '/src/terminal/PageList.zig — its 205 `test "..." {` blocks span lines 5583-14757. Port tests into ' + R + '/crates/rustty-vt/src/pagelist/tests.rs (the existing 43 live there). NAME each ported test fn after the reference test string (snake_case) and add a marker comment `// ref: test "<exact name>"` immediately above it, so already-ported tests are detectable by grep. Run all gates FOREGROUND (or with bounded `timeout`), capture output, inspect — NEVER background-and-poll. Every commit ends with the trailer "Co-Authored-By: Claude Opus 4.8 (1M context)". '
async function robust(p, o) { let r = await agent(p, o); if (!r) r = await agent(p, { ...o, label: (o.label || 'a') + '-retry' }); return r }
const BATCH = { type: 'object', additionalProperties: true, required: ['ok', 'tests_ported_total', 'blockers'], properties: { ok: { type: 'boolean' }, tests_ported_total: { type: 'integer' }, ported_this_batch: { type: 'array', items: { type: 'string' } }, commit: { type: 'string' }, notes: { type: 'string' }, blockers: { type: 'array', items: { type: 'string' } } } }
const CRIT = { type: 'object', additionalProperties: true, required: ['verdict', 'count', 'blockers'], properties: { verdict: { type: 'string', enum: ['GREEN', 'RED'] }, count: { type: 'integer' }, missing_indices: { type: 'array', items: { type: 'integer' } }, tautological: { type: 'array', items: { type: 'string' } }, notes: { type: 'string' }, blockers: { type: 'array', items: { type: 'string' } } } }

phase('Tests')
let total = 43, iter = 0, noProgress = 0
const results = []
while (total < 205 && iter < 24) {
  iter++
  const r = await robust(RULES +
    'TASK: port the ~10 LOWEST-reference-index UN-PORTED PageList tests. STEP 1 — determine what is un-ported: list the 205 `test "..." {` in PageList.zig in source order (grep ^test); a reference test is PORTED iff tests.rs already has a #[test] whose name/marker matches it. STEP 2 — port the next ~10 un-ported tests in reference order into tests.rs, faithful & assertion-for-assertion with INDEPENDENT expected values, named after the ref test (snake_case) with the `// ref: test "<name>"` marker. STEP 3 — gates FOREGROUND (capture+inspect): cargo test -p rustty-vt (ALL pass, cumulative), cargo clippy -p rustty-vt --all-targets -- -D warnings, cargo fmt -p rustty-vt --check. STEP 4 — commit "rustty: PageList tests (vt-3c) — <new_total>/205" + trailer. Return ok + tests_ported_total = `grep -c "#\\[test\\]" tests.rs` AFTER your commit + ported_this_batch (the ref names you added) + commit + blockers. If a test genuinely cannot pass because a method is wrong (not missing), STOP with a precise blocker rather than weakening the test.',
    { schema: BATCH, effort: 'high', label: 't:' + iter, phase: 'Tests' })
  results.push(r)
  if (!r) break // died — resume continues from last commit
  if (typeof r.tests_ported_total === 'number') {
    if (r.tests_ported_total <= total) { noProgress++; } else { noProgress = 0; total = r.tests_ported_total }
  }
  if (r.ok === false && r.blockers && r.blockers.length) break
  if (noProgress >= 2) break // two consecutive no-progress rounds — stop to avoid spinning
  log(`tests ported: ${total}/205 (iter ${iter})`)
}

phase('Complete')
const crit = await robust(RULES +
  'TASK (completeness critic — default RED when uncertain). (1) Count #[test] in ' + R + '/crates/rustty-vt/src/pagelist/*.rs — must be 205 (203 PageList + 2 Screen-prompt tests live in PageList.zig). (2) Map EACH of the 205 reference `test "..."` blocks to a port #[test]; report any missing as missing_indices (1-based source order). (3) cargo test -p rustty-vt — ALL pass (capture result). (4) Sample ~12 ported tests across scroll/resize-reflow/erase/pin/iter/clone and verify NONE are tautological (expected value independent of the code under test) or weakened/neutered vs the reference assertions. Verdict GREEN only if count==205 AND missing_indices empty AND all pass AND no tautology; else RED with specifics. Return verdict+count+missing_indices+tautological+blockers.',
  { schema: CRIT, effort: 'high', label: 'critic', phase: 'Complete' })
return { results, crit }
