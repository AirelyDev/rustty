export const meta = {
  name: 'stage-c-terminfo',
  description: 'Stage C leaf: rustty-terminfo — the terminfo Source model + entry (terminfo/{Source 2t, ghostty 1t, main 1t} = 4t). Deps only rustty-identity (R2). Self-contained. R2 nuance: the entry .names includes "xterm-ghostty" (a compat TERM id behind a HACK comment) — follow rustty-identity for project name, flag the TERM-name call.',
  phases: [
    { title: 'Port', detail: 'Source + ghostty entry + main into rustty-terminfo (R2 identity)' },
    { title: 'Verify', detail: 'count-gate + tests + Miri + G3b + faithfulness + R2 TERM-name note' },
  ],
}
const G = '/Users/gael/projects/ghostty', R = '/Users/gael/projects/rustty'
const TI = G + '/src/terminfo'
const GATES = R + '/.port/tools/gates'
const RULES = 'OBEY: PARITY CONSTITUTION + ' + R + '/.port/PORT_RULES.md. R1 = PORT EVERY test{} (count is a CONTRACT); EVERY line/comment verbatim; assertion-for-assertion, INDEPENDENT expected values. R2 = rustty identity (use rustty-identity crate for the project name; do NOT hardcode "ghostty"). Reference READ-ONLY ' + TI + '/. Fills rustty-terminfo (crates/rustty-terminfo/, lib.rs stub; deps rustty-identity — PORTED). Plan: ' + R + '/.port/phase-2/NEXT_PHASE_PLAN.md (Stage C). '
  + 'KEY: ' + TI + '/Source.zig (243loc, 2t — the terminfo Source model + serializer) + ghostty.zig (397loc, 1t — the terminfo capability entry, a big data table with .names) + main.zig (13loc, 1t). R2 NUANCE: ghostty.zig .names includes "xterm-ghostty" with a HACK comment about app-compat recognition. Follow rustty-identity for the PROJECT name; for the compat TERM-name string ("xterm-ghostty") port the value + HACK comment FAITHFULLY (it is an app-facing compat id, not an internal identifier) but FLAG in blockers if you judge it needs an explicit R2 rename decision (xterm-ghostty vs xterm-rustty) — do not silently rename a compat id. [:0]const u8->Vec<u8>; byte-slices &[u8]/Vec<u8>. '
  + 'MIRI: per MIRI_EXCEPTIONS.md Miri-subset ISOLATED. PROCESS LESSONS (memory): port ALL tests (count enforces), scoped-G3b. Run gates FOREGROUND. Commit trailer "Co-Authored-By: Claude Opus 4.8 (1M context)". '
async function robust(p, o) { let r = await agent(p, o); if (!r) r = await agent(p, { ...o, label: (o.label || 'a') + '-retry' }); return r }
const STEP = { type: 'object', additionalProperties: true, required: ['ok', 'blockers'], properties: { ok: { type: 'boolean' }, tests_added: { type: 'integer' }, builds: { type: 'boolean' }, term_name_decision: { type: 'string' }, commit: { type: 'string' }, notes: { type: 'string' }, blockers: { type: 'array', items: { type: 'string' } } } }
const VERIFY = { type: 'object', additionalProperties: true, required: ['verdict', 'blockers'], properties: { verdict: { type: 'string', enum: ['GREEN', 'RED'] }, tests_passing: { type: 'integer' }, count_ok: { type: 'boolean' }, miri_subset_clean: { type: 'boolean' }, g3b_missing: { type: 'integer' }, r2_term_name: { type: 'string' }, notes: { type: 'string' }, blockers: { type: 'array', items: { type: 'string' } } } }

phase('Port')
const port = await robust(RULES +
  'TASK (port terminfo). Port FULLY (every test) into rustty-terminfo: ' + TI + '/Source.zig (2t) -> src/source.rs; ' + TI + '/ghostty.zig (1t, the entry table) -> src/entry.rs (or rustty.rs); ' + TI + '/main.zig (1t) -> wire src/lib.rs. Apply R2 to the project name via rustty-identity; port the xterm-ghostty compat name + HACK comment faithfully (flag if it needs an R2 rename call). GATES FOREGROUND: cargo build -p rustty-terminfo; cargo test -p rustty-terminfo (4 pass); Miri-subset 2 tests ISOLATED (0 UB/leak); clippy -p rustty-terminfo --all-targets -D; fmt; check_comment_coverage.py scoped to the 3 terminfo files missing=0. Commit "rustty: port rustty-terminfo (Source + entry, 4 tests) — Stage C". Return ok+tests_added+builds+term_name_decision+commit+blockers.',
  { schema: STEP, effort: 'high', label: 'port', phase: 'Port' })

phase('Verify')
const verify = await robust(RULES + 'Port: ' + JSON.stringify(port && { commit: port.commit, term_name_decision: port.term_name_decision, blockers: port.blockers }) + '\n'
  + 'TASK (independent verify — default RED). Add rustty-terminfo mappings to ' + GATES + '/check_test_counts.py (Source.zig/ghostty.zig/main.zig vs the rust files; GH base = src/terminfo). Confirm FOREGROUND: cargo build --workspace; `python3 ' + GATES + '/check_test_counts.py` => terminfo rust>=zig (count_ok) + ALL GREEN; cargo test -p rustty-terminfo (4 pass); Miri-subset 2 tests ISOLATED; clippy --all-targets -D + fmt --check + ' + GATES + '/check_acyclic.sh; G3b 3 files missing=0. Spot-check the Source serializer test + the entry .names faithful. Report the R2 TERM-name decision (r2_term_name: what the ported entry emits + whether it needs human sign-off). Verdict GREEN only if count_ok + build + tests + Miri + g3b=0 + clippy + fmt + acyclic green. Report verdict + tests_passing + count_ok + miri_subset_clean + g3b_missing + r2_term_name + blockers.',
  { schema: VERIFY, effort: 'high', label: 'verify', phase: 'Verify' })
return { port, verify }
