export const meta = {
  name: 'phase2-batch1-finalize',
  description: 'Finalize Phase-2 batch-1: resolve remaining clippy (-D warnings) parity-first + raw-pointer soundness + fmt; independent adversarial review of all 4 leaf crates (datastruct never got one); full gate re-run + batch report. Null-guarded against transient API errors.',
  phases: [
    { title: 'Fix', detail: 'clippy -D warnings + fmt green, parity-preserving; raw-pointer soundness' },
    { title: 'Review', detail: 'independent adversarial parity review of all 4 crates' },
    { title: 'Integrate', detail: 'full gate re-run + batch1 report + commit' },
  ],
}

const G = '/Users/gael/projects/ghostty'
const R = '/Users/gael/projects/rustty'
const INV = R + '/.port/phase-0/inventory'
const CRATES = ['rustty-util', 'rustty-build-config', 'rustty-lib', 'rustty-datastruct']
const RULES = 'PARITY CONSTITUTION + ' + R + '/.port/PORT_RULES.md. R1=100% fidelity (every comment preserved; every test assertion-for-assertion; NOTHING weakened). R6=Miri memory gate. Reference READ-ONLY at ' + G + '/src (pinned 4749c4e); rustty must NOT depend on it. Contracts: ' + INV + '/tests-summary.json (crate_map keys are UNPREFIXED e.g. "datastruct"; the gates also accept "rustty-datastruct" now), tests.jsonl, comments-census.json. Work in ' + R + '. Capture command output to a var then grep — do NOT read exit codes through a `| tail` pipe (it masks them). Never claim a green you did not run. '

const FIX_SCHEMA = { type:'object', additionalProperties:true, required:['clippy_green','fmt_green','blockers'], properties:{
  clippy_green:{type:'boolean'}, fmt_green:{type:'boolean'},
  warnings_fixed:{type:'array',items:{type:'string'}},
  parity_allows:{type:'array',items:{type:'string'}},
  soundness_decisions:{type:'array',items:{type:'string'}},
  tests_still_green:{type:'boolean'}, miri_still_green:{type:'boolean'},
  commit:{type:'string'}, blockers:{type:'array',items:{type:'string'}} } }
const REVIEW_SCHEMA = { type:'object', additionalProperties:true, required:['crate','verdict','blockers'], properties:{
  crate:{type:'string'}, verdict:{type:'string',enum:['PARITY','NOT_PARITY']},
  dropped_or_missing_tests:{type:'array',items:{type:'string'}},
  dropped_or_weakened_assertions:{type:'array',items:{type:'string'}},
  dropped_comments:{type:'array',items:{type:'string'}},
  unsound_unsafe_or_ub:{type:'array',items:{type:'string'}},
  behavior_or_layout_diffs:{type:'array',items:{type:'string'}},
  gates_reproduced_green:{type:'boolean'}, notes:{type:'string'},
  blockers:{type:'array',items:{type:'string'}} } }
const INTEG_SCHEMA = { type:'object', additionalProperties:true, required:['verdict','blockers'], properties:{
  verdict:{type:'string',enum:['GREEN','RED']},
  cargo_test_green:{type:'boolean'}, clippy_green:{type:'boolean'}, fmt_green:{type:'boolean'},
  g1_g2_g3_green:{type:'boolean'}, acyclic_green:{type:'boolean'}, miri_green:{type:'boolean'},
  total_tests_ported:{type:'integer'}, commit:{type:'string'},
  blockers:{type:'array',items:{type:'string'}} } }

async function robust(prompt, opts) {
  let r = await agent(prompt, opts)
  if (!r) r = await agent(prompt, { ...opts, label: (opts.label || 'agent') + '-retry' })
  return r
}

phase('Fix')
const fix = await robust(
  RULES +
  'TASK: Make `cargo clippy -p rustty-util -p rustty-build-config -p rustty-lib -p rustty-datastruct --all-targets -- -D warnings` and `cargo fmt -- --check` GREEN for the 4 batch crates, WITHOUT breaking parity. Safe machine-applicable fixes already applied (committed 6dabe90; tests 4/13/76/0 green). Remaining warnings + the REQUIRED resolution (verify each against the reference, then apply):\n' +
  '- `clippy::approx_constant` at crates/rustty-datastruct/src/comparison.rs:380-381 — VERIFIED reference comparison.zig:262-263 uses the literal 3.14159 / 3.14158 to test deepEqual. KEEP the literals; add `#[allow(clippy::approx_constant)]` + a `// PARITY:` note. (Using consts::PI would destroy the test.)\n' +
  '- `clippy::not_unsafe_ptr_arg_deref` ×2 (SOUNDNESS): (a) crates/rustty-lib/src/allocator/convenience.rs — these are `pub extern "C"` WASM allocator exports that null-check then deref; they ARE the FFI boundary, so add a module-level `#[allow(clippy::not_unsafe_ptr_arg_deref)]` with a justification comment (the C/Wasm caller owns pointer validity; marking exports `unsafe` does not help C callers and is ABI-neutral noise). (b) crates/rustty-datastruct/src/split_tree.rs `init(view: *mut V)` — this is an internal Rust constructor taking a caller-provided raw view pointer; the CORRECT fix is to mark it `pub unsafe fn init` (encode the contract: caller must pass a valid `view`), updating its callers/tests to `unsafe {}`. Prefer marking `unsafe` over `#[allow]` for internal APIs. Confirm the reference `init` semantics.\n' +
  '- `clippy::should_implement_trait` (method `next`) ×3 — the reference exposes an inherent `next()` (iterator-like). Keep the reference API; add `#[allow(clippy::should_implement_trait)]` + `// PARITY:` note (do NOT silently replace with std Iterator if it changes the public API shape).\n' +
  '- `clippy::needless_range_loop` ×5 (split_tree grid/top/bottom, intrusive_linked_list nodes) — these mirror the reference index-based loops. Prefer a faithful, clear fix: if an iterator form is unambiguous and behavior-identical, apply it; otherwise `#[allow(clippy::needless_range_loop)]` + `// PARITY: mirrors reference index loop`. Behavior must be identical either way.\n' +
  '- `clippy::enum_variant_names` ×1 — keep the reference variant name; `#[allow]` + note.\n' +
  '- `clippy::len_without_is_empty` (CircBuf) ×1 — reference uses `empty()`. Either `#[allow]` + note, or add an additive `is_empty()` delegating to `empty()` (do not remove `empty()`).\n' +
  '- `empty line after doc comment` ×2 and `unnecessary braces around method argument` ×3 — apply the trivial fix; ensure NO reference comment is deleted and NO assertion changed.\n' +
  'After fixing: run `cargo test` (must stay 4/13/76/0), `cargo clippy ... -- -D warnings` (ZERO warnings), `cargo fmt -- --check`, and `bash ' + R + '/.port/tools/gates/check_leaks.sh --crate <c>` (Miri clean) for all 4. Commit "rustty: batch-1 clippy/fmt + raw-pointer soundness (parity-preserving)" + Co-Authored-By: Claude Opus 4.8 (1M context). Return structured result; if any fix would weaken a test or break parity, STOP with a blocker.',
  { schema: FIX_SCHEMA, effort: 'high', label: 'fix', phase: 'Fix' }
)

phase('Review')
const reviews = await parallel(CRATES.map(c => () => robust(
  RULES +
  'TASK (independent adversarial parity review of **' + c + '** — default NOT_PARITY when uncertain; this crate may not have been reviewed yet). Compare the Rust port (crates/' + c + '/src) against the reference (' + G + '/src) and the contract (' + INV + '): (1) is EVERY reference test{} present as a Rust #[test] with the SAME assertions and SAME values/edge-cases? (2) is EVERY comment preserved in CONTENT (not just count — spot-check distinctive comments, license headers, implementation notes, spec citations)? (3) any UNSOUND unsafe / UB (esp. raw-pointer handling, packed layouts)? (4) any behavior or memory-layout divergence (integer widths, overflow/wrap, allocator semantics, iteration order)? (5) RE-RUN the gates yourself and confirm genuinely green: `cargo test -p ' + c + '`, `python3 ' + R + '/.port/tools/gates/check_tests_ported.py --crate ' + c + ' --strict`, `check_assertions.py --crate ' + c + ' --strict`, `check_comments_fidelity.py --crate ' + c + ' --strict`, `bash ' + R + '/.port/tools/gates/check_leaks.sh --crate ' + c + '`. Any dropped/weakened test/assertion/comment, any unsound unsafe, any behavior diff, or any non-green gate => NOT_PARITY with specifics.',
  { schema: REVIEW_SCHEMA, effort: 'high', label: 'review:' + c, phase: 'Review' }
)))

phase('Integrate')
const integ = await robust(
  RULES +
  'TASK (integrate + final batch-1 gate). Run ' + R + '/.port/tools/ci.sh and confirm: cargo build/test --workspace green, `cargo clippy --workspace -- -D warnings` ZERO warnings, `cargo fmt -- --check` clean, G7 acyclic green, and per-crate G1/G2/G3 green for all 4 done crates (use rustty- names). Run `bash ' + R + '/.port/tools/gates/check_leaks.sh --all-done` (Miri clean). Report total tests ported vs 3234. Verify no port crate depends on ghostty (R3). Ensure git tree clean + committed. Address any NOT_PARITY review findings before declaring GREEN. Write ' + R + '/.port/phase-2/batch1-REPORT.md + batch1-report.json (per-crate: tests ported/contract, assertions, comments, Miri, clippy/fmt, review verdict; the parity_allows + soundness decisions). ' +
  'FIX result: ' + JSON.stringify(fix) + '\nREVIEWS: ' + JSON.stringify(reviews) + '\n' +
  'Return the verdict. GREEN only if: all 4 crates tests==contract + assertion-for-assertion + comment fidelity + Miri clean + clippy(-D)/fmt clean + reviews PARITY, AND full workspace builds/tests green + acyclic.',
  { schema: INTEG_SCHEMA, effort: 'high', label: 'integrate', phase: 'Integrate' }
)

return { fix, reviews, integ }
