export const meta = {
  name: 'phase2-batch3-os',
  description: 'Phase 2 batch 3: port rustty-os (58 tests, 34 files) assertion-for-assertion + comment-for-comment. Right-tool memory gate: Miri for pure-logic tests, native cargo test for syscall/I-O tests (Miri can not interpret real OS), ASan for macOS CoreFoundation/objc FFI. G3b + all gates + adversarial review, loop-until-green. Null-guarded.',
  phases: [
    { title: 'Port', detail: 'port rustty-os, loop-until-green; then adversarial review' },
    { title: 'Integrate', detail: 'full ci.sh + report' },
  ],
}

const G = '/Users/gael/projects/ghostty'
const R = '/Users/gael/projects/rustty'
const INV = R + '/.port/phase-0/inventory'
const GATES = R + '/.port/tools/gates'
const RULES = 'PARITY CONSTITUTION + ' + R + '/.port/PORT_RULES.md. R1=100% fidelity: port EVERY line; preserve EVERY comment VERBATIM (retarget ghostty->rustty only, R2); port EVERY test{} block as #[test] ASSERTION-FOR-ASSERTION; NOTHING dropped/reworded/weakened/stubbed. BEWARE the tautological-test trap: a ported test`s "expected" value must come from an INDEPENDENT source, never be re-derived from the same code path under test (this exact bug occurred in unicode). G3b (check_comment_coverage.py) deterministically catches dropped comments — do not drop any. R3: rustty must NOT depend on the ghostty tree; copy any needed C source. Reference READ-ONLY at ' + G + '/src (pinned 4749c4e). Contracts: ' + INV + '/tests-summary.json (crate_map "os"; gates accept rustty- prefix), tests.jsonl, comments-census.json. Work in ' + R + '. Capture command output to a var THEN grep (a `| tail` pipe MASKS exit codes). Never claim a green you did not run. '

const PORT_SCHEMA = { type:'object', additionalProperties:true, required:['crate','tests_ported','tests_contract','all_gates_green','blockers'], properties:{
  crate:{type:'string'}, files_ported:{type:'array',items:{type:'string'}},
  tests_ported:{type:'integer'}, tests_contract:{type:'integer'}, assertions_ported:{type:'integer'},
  gates:{type:'object',additionalProperties:true}, g3b_missing:{type:'integer'},
  miri_status:{type:'string'}, miri_excluded_tests:{type:'array',items:{type:'string'}},
  ffi_asan_tests:{type:'array',items:{type:'string'}},
  all_gates_green:{type:'boolean'}, notes:{type:'string'}, blockers:{type:'array',items:{type:'string'}} } }
const REVIEW_SCHEMA = { type:'object', additionalProperties:true, required:['crate','verdict','blockers'], properties:{
  crate:{type:'string'}, verdict:{type:'string',enum:['PARITY','NOT_PARITY']},
  dropped_or_missing_tests:{type:'array',items:{type:'string'}},
  tautological_or_neutered_tests:{type:'array',items:{type:'string'}},
  dropped_or_weakened_assertions:{type:'array',items:{type:'string'}},
  dropped_or_reworded_comments:{type:'array',items:{type:'string'}},
  unsound_unsafe_or_ub:{type:'array',items:{type:'string'}},
  behavior_or_layout_diffs:{type:'array',items:{type:'string'}},
  miri_exclusions_justified:{type:'string'}, gates_reproduced_green:{type:'boolean'},
  notes:{type:'string'}, blockers:{type:'array',items:{type:'string'}} } }
const INTEG_SCHEMA = { type:'object', additionalProperties:true, required:['verdict','blockers'], properties:{
  verdict:{type:'string',enum:['GREEN','RED']}, ci_pass:{type:'boolean'},
  total_tests_ported:{type:'integer'}, commit:{type:'string'}, blockers:{type:'array',items:{type:'string'}} } }

async function robust(p,o){ let r=await agent(p,o); if(!r) r=await agent(p,{...o,label:(o.label||'a')+'-retry'}); return r }

const c = 'rustty-os'
phase('Port')
const portPrompt = RULES +
  'TASK: Port crate **rustty-os** from src/os/ (34 files, 58 test blocks). It depends on rustty-datastruct, rustty-global, rustty-util, rustty-build-config (all already ported leaves). \n' +
  'MEMORY GATE — RIGHT TOOL PER JOB (os does real syscalls/file-I/O/env which Miri CANNOT interpret): \n' +
  '  - Run the FULL test suite under native `cargo test -p rustty-os` (this is the correctness oracle; ALL 58 tests must pass natively — never skip or weaken any).\n' +
  '  - For tests that perform real OS I/O/syscalls (file ops, env, locale, tmpdir, pty, exec), mark them `#[cfg_attr(miri, ignore)]` so `cargo +nightly miri test` runs the PURE-LOGIC tests cleanly (UB/leak check) while the I/O tests are Miri-skipped (they still run fully under native). This is NOT weakening (native runs them); document each Miri-excluded test + why in miri_excluded_tests.\n' +
  '  - For any macOS CoreFoundation/objc or other C FFI (copy the needed C/headers per R3 if used; src/os may use the macos pkg), those tests use ASan not Miri (record in ffi_asan_tests).\n' +
  'STEPS (loop to green): (1) read src/os/ + the contract (' + INV + '/tests-summary.json crate_map "os", tests.jsonl, comments-census.json). (2) Port ALL code to idiomatic parity-preserving Rust in crates/rustty-os/src/, preserving EVERY comment verbatim, same algorithms/layouts; handle platform cfg (macos/linux/freebsd/wasm/windows) faithfully — on this macOS host the macos + cross-platform paths are exercised; keep the linux/gtk paths ported (compiled-checked) for the Linux baseline. (3) Port EVERY test{} as #[test] assertion-for-assertion — each test`s expected values from an INDEPENDENT source (no tautologies). (4) lib.rs `//! PORT STATUS: done`. (5) gates (LOOP to green, real exit codes): `cargo test -p rustty-os` (58 pass), `cargo clippy -p rustty-os --all-targets -- -D warnings` (0; parity-first #[allow] only with // PARITY note), `cargo fmt -p rustty-os -- --check`, `python3 ' + GATES + '/check_tests_ported.py --crate rustty-os --strict`, `check_assertions.py --crate rustty-os --strict`, `check_comments_fidelity.py --crate rustty-os --strict`, `check_comment_coverage.py --crate rustty-os` (G3b missing=0 — restore any dropped/reworded comment), `check_acyclic.sh`, `check_leaks.sh --crate rustty-os` (Miri on the non-excluded tests: 0 UB/leaks). (6) Commit "rustty: port rustty-os (phase 2, batch 3)" + Co-Authored-By: Claude Opus 4.8 (1M context).\n' +
  'Return structured result. If a gate cannot be green honestly, STOP with a precise blocker — NEVER weaken/stub/tautologize a test or drop a comment.'
let result = await robust(portPrompt, { schema:PORT_SCHEMA, effort:'high', label:'port:'+c, phase:'Port' })

const reviewPrompt = RULES +
  'TASK (adversarial parity review of **rustty-os** — default NOT_PARITY when uncertain). Porter claims: ' + JSON.stringify(result) + '\n' +
  'Independently vs reference (' + G + '/src/os) + contract: (1) EVERY test{} ported with SAME assertions/values, NONE tautological/neutered/stubbed (check that each test`s expected value is independent of the code under test — this bit unicode), NONE silently Miri-ignored that is actually pure-logic. (2) EVERY comment preserved verbatim — run `python3 ' + GATES + '/check_comment_coverage.py --crate rustty-os` yourself, confirm missing=0, vet REVIEW-band candidates. (3) unsound unsafe/UB (esp. raw FFI, libc calls, ptr handling). (4) behavior/layout/platform-cfg divergence. (5) are the Miri exclusions JUSTIFIED (genuinely I/O-doing, run fully under native)? (6) RE-RUN every gate yourself with captured exit codes incl. `cargo test -p rustty-os` and `check_leaks.sh --crate rustty-os`. Any drop/weakening/tautology/unsound-unsafe/behavior-diff/unjustified-Miri-skip/non-green-gate => NOT_PARITY with specifics.'
let review = await robust(reviewPrompt, { schema:REVIEW_SCHEMA, effort:'high', label:'review:'+c, phase:'Port' })
if (review && review.verdict !== 'PARITY') {
  const fixP = portPrompt + '\n\nPrior attempt FAILED adversarial review. FIX (regress nothing): ' + JSON.stringify(review)
  result = await robust(fixP, { schema:PORT_SCHEMA, effort:'high', label:'refix:'+c, phase:'Port' })
  review = await robust(reviewPrompt + '\n\n(Re-review after fixes.) New claim: ' + JSON.stringify(result), { schema:REVIEW_SCHEMA, effort:'high', label:'rereview:'+c, phase:'Port' })
}

phase('Integrate')
const integ = await robust(
  RULES + 'TASK (integrate batch 3). Run ' + R + '/.port/tools/ci.sh; confirm CI RESULT: PASS (incl. G4-uni HARD gate still 0 diffs, G7 acyclic, G6 Miri --all-done). Confirm rustty-os: cargo test 58 pass, G1(58/58)/G2/G3/G3b green, Miri clean on non-excluded tests, clippy-D/fmt clean. Report total tests ported vs 3234. Verify no port crate depends on ghostty (R3). Tree clean + committed. Address any NOT_PARITY before GREEN. Write ' + R + '/.port/phase-2/batch3-REPORT.md + batch3-report.json. ' +
  'Port+review: ' + JSON.stringify({result, review}) + '\n' +
  'Return verdict. GREEN only if rustty-os fully parity (58 tests, assertion-for-assertion, no tautologies, G3b=0, Miri clean on non-I/O tests + native all-pass, review PARITY) and full CI PASS.',
  { schema:INTEG_SCHEMA, effort:'high', label:'integrate-batch3', phase:'Integrate' }
)
return { result, review, integ }
