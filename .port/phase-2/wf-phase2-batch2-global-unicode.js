export const meta = {
  name: 'phase2-batch2-global-unicode',
  description: 'Phase 2 batch 2: port rustty-global (break #1 state-holder extraction) and rustty-unicode (5 tests + BIT-IDENTICAL Unicode table generation) assertion-for-assertion + comment-for-comment, with G3b content-comment coverage + Miri, per-crate gates + adversarial review, loop-until-green. Null-guarded.',
  phases: [
    { title: 'Port', detail: 'serial: global -> unicode; each ported then adversarially reviewed, loop-until-green' },
    { title: 'Integrate', detail: 'full ci.sh + report' },
  ],
}

const G = '/Users/gael/projects/ghostty'
const R = '/Users/gael/projects/rustty'
const INV = R + '/.port/phase-0/inventory'
const GATES = R + '/.port/tools/gates'
const RULES = 'PARITY CONSTITUTION + ' + R + '/.port/PORT_RULES.md. R1=100% fidelity: port EVERY line; preserve EVERY comment VERBATIM (retarget ghostty->rustty identifiers only, R2); port EVERY test{} block as #[test] ASSERTION-FOR-ASSERTION; NOTHING dropped/reworded/weakened. (A deterministic gate G3b checks that every distinctive reference comment survives — drops WILL be caught, so do not drop or reword comments.) Tests use STANDARD Rust assertions. R6 memory gate = MIRI for these pure-Rust crates. Reference READ-ONLY at ' + G + '/src (pinned 4749c4e); rustty must NOT depend on it. Contracts: ' + INV + '/tests-summary.json (crate_map; gates accept rustty- prefixed names), tests.jsonl, comments-census.json. Work in ' + R + '. Capture command output to a var THEN grep (a `| tail` pipe MASKS exit codes). Never claim a green you did not run. '

const PORT_SCHEMA = { type:'object', additionalProperties:true, required:['crate','tests_ported','tests_contract','all_gates_green','blockers'], properties:{
  crate:{type:'string'}, files_ported:{type:'array',items:{type:'string'}},
  tests_ported:{type:'integer'}, tests_contract:{type:'integer'}, assertions_ported:{type:'integer'},
  gates:{type:'object',additionalProperties:true}, g3b_missing:{type:'integer'}, miri_clean:{type:'boolean'},
  all_gates_green:{type:'boolean'}, breaks_handled:{type:'array',items:{type:'string'}},
  notes:{type:'string'}, blockers:{type:'array',items:{type:'string'}} } }
const REVIEW_SCHEMA = { type:'object', additionalProperties:true, required:['crate','verdict','blockers'], properties:{
  crate:{type:'string'}, verdict:{type:'string',enum:['PARITY','NOT_PARITY']},
  dropped_or_missing_tests:{type:'array',items:{type:'string'}},
  dropped_or_weakened_assertions:{type:'array',items:{type:'string'}},
  dropped_or_reworded_comments:{type:'array',items:{type:'string'}},
  unsound_unsafe_or_ub:{type:'array',items:{type:'string'}},
  behavior_or_layout_diffs:{type:'array',items:{type:'string'}},
  table_bit_identical:{type:'string'}, gates_reproduced_green:{type:'boolean'},
  notes:{type:'string'}, blockers:{type:'array',items:{type:'string'}} } }
const INTEG_SCHEMA = { type:'object', additionalProperties:true, required:['verdict','blockers'], properties:{
  verdict:{type:'string',enum:['GREEN','RED']}, ci_pass:{type:'boolean'},
  total_tests_ported:{type:'integer'}, commit:{type:'string'},
  per_crate:{type:'array',items:{type:'object',additionalProperties:true}},
  blockers:{type:'array',items:{type:'string'}} } }

async function robust(prompt, opts){ let r=await agent(prompt,opts); if(!r) r=await agent(prompt,{...opts,label:(opts.label||'a')+'-retry'}); return r }

const GATECMDS = (c) =>
  'Per-crate gates (LOOP until ALL green, real exit codes): `cargo test -p ' + c + '`; ' +
  '`cargo clippy -p ' + c + ' --all-targets -- -D warnings` (0 warnings, parity-first: keep reference literals/names via justified #[allow] + // PARITY note, never a fix that changes a value/behavior); ' +
  '`cargo fmt -p ' + c + ' -- --check`; ' +
  '`python3 ' + GATES + '/check_tests_ported.py --crate ' + c + ' --strict`; ' +
  '`python3 ' + GATES + '/check_assertions.py --crate ' + c + ' --strict`; ' +
  '`python3 ' + GATES + '/check_comments_fidelity.py --crate ' + c + ' --strict`; ' +
  '`python3 ' + GATES + '/check_comment_coverage.py --crate ' + c + '` (G3b: missing MUST be 0 — restore any dropped/reworded reference comment verbatim); ' +
  '`bash ' + GATES + '/check_acyclic.sh`; ' +
  '`bash ' + GATES + '/check_leaks.sh --crate ' + c + '` (Miri: 0 UB, 0 leaks). '

const SPECS = [
  { name:'rustty-global',
    notes:'BREAK #1: this is the extraction of src/global.zig — the process-global state hub. In the reference, global.zig holds the global App/state and is imported bottom-up everywhere. Port it as rustty-global: the LEAF state-holder (OnceLock/once_cell-style container with the same fields/semantics) WITHOUT the upward deps on apprt/cli/renderer/crash/font (those high-level wirings move to rustty-core when it is ported — record them as deferred seams, do NOT pull those crates in; rustty-global must stay a leaf so the bottom-up importers can depend on it). Preserve ALL of global.zig`s comments verbatim. 0 test blocks expected.' },
  { name:'rustty-unicode',
    notes:'Port src/unicode/ (8 files, 5 test blocks). CRITICAL: the reference generates Unicode tables (grapheme-break + east-asian width + general-category) — in ghostty via build/UnicodeTables.zig + the uucode dep. rustty-unicode MUST produce BIT-IDENTICAL tables (parity is byte-exact for non-GPU output). Either (a) generate the tables in build.rs from the SAME Unicode UCD version the reference pins, or (b) embed the reference`s generated table bytes — whichever guarantees bit-identical. PROVE bit-identicality: build the reference table artifact and diff, OR feed a sweep of codepoints (0..0x10FFFF, plus the grapheme-break test vectors) through both rustty and the reference (via the libghostty-vt grapheme/width API or the reference unicode module) and confirm IDENTICAL width + grapheme-break results. Do NOT use a different Unicode version or a pure-Rust unicode crate (parity-breaking). Port all 5 test blocks assertion-for-assertion.' },
]

phase('Port')
const ported = []
for (let i=0;i<SPECS.length;i++){
  const s = SPECS[i]
  const portPrompt = RULES +
    'TASK: Port crate **' + s.name + '**. ' + s.notes + '\n' +
    'STEPS (loop to green): (1) read the reference source under ' + G + '/src + the contract (' + INV + '/tests-summary.json crate_map, tests.jsonl, comments-census.json). (2) Port ALL code to idiomatic parity-preserving Rust in crates/' + s.name + '/src/, preserving EVERY comment verbatim. (3) Port EVERY test{} as #[test] assertion-for-assertion. (4) Set lib.rs `//! PORT STATUS:` to `done`. (5) ' + GATECMDS(s.name) + ' (6) Commit "rustty: port ' + s.name + ' (phase 2)" + Co-Authored-By: Claude Opus 4.8 (1M context).\n' +
    'Return structured result with EXACT counts + gate results (incl. g3b_missing=0, miri_clean). If a gate cannot be green honestly, STOP with a precise blocker — NEVER weaken a test/gate or drop a comment.'
  let result = await robust(portPrompt, { schema:PORT_SCHEMA, effort:'high', label:'port:'+s.name, phase:'Port' })

  const reviewPrompt = RULES +
    'TASK (adversarial parity review of **' + s.name + '** — default NOT_PARITY when uncertain). Porter claims: ' + JSON.stringify(result) + '\n' +
    'Independently vs reference (' + G + '/src) + contract: (1) EVERY test{} ported with SAME assertions/values? (2) EVERY comment preserved VERBATIM (spot-check content; run `python3 ' + GATES + '/check_comment_coverage.py --crate ' + s.name + '` yourself and confirm missing=0 AND vet any REVIEW-band candidates for genuine rewording). (3) unsound unsafe/UB? (4) behavior/layout divergence? ' + (s.name==='rustty-unicode' ? 'CRITICAL: independently verify the Unicode tables are BIT-IDENTICAL to the reference (diff the generated artifact or sweep codepoints through both) — report table_bit_identical with the evidence. A non-identical table is NOT_PARITY. ' : '') +
    '(5) RE-RUN every gate yourself (cargo test, clippy -D, fmt, check_tests_ported --strict, check_assertions --strict, check_comments_fidelity --strict, check_comment_coverage [G3b], check_acyclic, check_leaks [Miri]) with captured exit codes. Any drop/weakening/unsound-unsafe/behavior-diff/non-identical-table/non-green-gate => NOT_PARITY with specifics.'
  let review = await robust(reviewPrompt, { schema:REVIEW_SCHEMA, effort:'high', label:'review:'+s.name, phase:'Port' })

  if (review && review.verdict !== 'PARITY') {
    const fixP = portPrompt + '\n\nPrior attempt FAILED adversarial review. FIX (regress nothing): ' + JSON.stringify(review)
    result = await robust(fixP, { schema:PORT_SCHEMA, effort:'high', label:'refix:'+s.name, phase:'Port' })
    review = await robust(reviewPrompt + '\n\n(Re-review after fixes.) New claim: ' + JSON.stringify(result), { schema:REVIEW_SCHEMA, effort:'high', label:'rereview:'+s.name, phase:'Port' })
  }
  ported.push({ crate:s.name, result, review })
}

phase('Integrate')
const integ = await robust(
  RULES +
  'TASK (integrate batch 2). Run ' + R + '/.port/tools/ci.sh and confirm CI RESULT: PASS (cargo build/test/clippy-D/fmt green, G7 acyclic, all progress gates incl G3b + G6 Miri green). Confirm per-crate G1/G2/G3/G3b green + Miri clean for rustty-global and rustty-unicode (use rustty- names). For unicode, re-confirm tables bit-identical. Report total tests ported vs 3234. Verify no port crate depends on ghostty (R3). Ensure git tree clean + committed. Address any NOT_PARITY findings before GREEN. Write ' + R + '/.port/phase-2/batch2-REPORT.md + batch2-report.json. ' +
  'Per-crate results: ' + JSON.stringify(ported) + '\n' +
  'Return verdict. GREEN only if both crates fully parity (tests==contract, assertion-for-assertion, G3b missing=0, Miri clean, unicode tables bit-identical, reviews PARITY) and full CI PASS.',
  { schema:INTEG_SCHEMA, effort:'high', label:'integrate-batch2', phase:'Integrate' }
)

return { ported, integ }
