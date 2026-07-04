export const meta = {
  name: 'phase2-vt3-pagelist',
  description: 'rustty-vt sub-batch 3: port PageList.zig (14.8k loc, 205 tests) via intra-file decomposition into a pagelist/ module — prep (struct + core in mod.rs, cluster sub-file stubs) then sequential cluster groups (core, grow, scroll, resize, erase, pin, iterators/access) each porting its methods + the tests exercising them, then integrate (205 tests, G3b, Miri) + adversarial review. vt stays PORT STATUS stub. Null-guarded.',
  phases: [
    { title: 'Prep', detail: 'pagelist/ module: struct + core internals in mod.rs; cluster sub-file stubs; build green' },
    { title: 'Clusters', detail: 'sequential: core, grow/compact, scroll, resize, erase, pin/track, iterators/access — methods + their tests' },
    { title: 'Integrate', detail: 'all 205 PageList tests + G3b + Miri + gates' },
    { title: 'Review', detail: 'adversarial: tautologies, comments, behavior/layout, unsafe/UB' },
  ],
}
const G='/Users/gael/projects/ghostty', R='/Users/gael/projects/rustty'
const INV=R+'/.port/phase-0/inventory', GATES=R+'/.port/tools/gates'
const RULES='PARITY CONSTITUTION + '+R+'/.port/PORT_RULES.md. R1=100% fidelity: EVERY line ported; EVERY comment VERBATIM (retarget ghostty->rustty only, R2); EVERY test{} as #[test] ASSERTION-FOR-ASSERTION; nothing dropped/reworded/weakened/stubbed/tautologized. TAUTOLOGY TRAP (hit unicode): test expected values from an INDEPENDENT source, never re-derived from the code under test. G3b (check_comment_coverage.py, HARDENED) catches dropped comments. R3: NO ghostty-tree dep. Preserve byte-exact behavior/layout; PageList manages the page linked-list + scrollback over the already-ported page.Page/Cell/Row (byte-identical) + render-types Offset. MEMORY GATE: native cargo test (all) + Miri (PageList uses the page offset-arena + tracked pins via raw pointers — run Miri; UAF/OOB/uninit MUST be clean; the relative-Offset int-to-ptr provenance pattern is the documented sound one from render-types/page). rustty-vt is the CROWN JEWEL, MULTI-SUB-BATCH: port ONLY PageList here; keep lib.rs PORT STATUS: stub. Reference READ-ONLY '+G+'/src/terminal/PageList.zig (pinned 4749c4e, 14803 lines, 205 test blocks). Contract: '+INV+'/tests-summary.json crate_map "vt" (PageList.zig = its share), tests.jsonl, comments-census.json. Capture output to a var THEN grep (`| tail` masks exit codes). Never claim an unrun green. '
async function robust(p,o){let r=await agent(p,o);if(!r)r=await agent(p,{...o,label:(o.label||'a')+'-retry'});return r}
const PORT={type:'object',additionalProperties:true,required:['ok','tests_passing','blockers'],properties:{ok:{type:'boolean'},methods_done:{type:'array',items:{type:'string'}},tests_passing:{type:'integer'},g3b_clean_for_my_files:{type:'boolean'},notes:{type:'string'},blockers:{type:'array',items:{type:'string'}}}}
const INTEG={type:'object',additionalProperties:true,required:['verdict','blockers'],properties:{verdict:{type:'string',enum:['GREEN','RED']},pagelist_tests:{type:'integer'},g3b_missing_pagelist:{type:'integer'},miri_clean:{type:'boolean'},commit:{type:'string'},blockers:{type:'array',items:{type:'string'}}}}
const REVIEW={type:'object',additionalProperties:true,required:['verdict','blockers'],properties:{verdict:{type:'string',enum:['PARITY','NOT_PARITY']},tautological_or_neutered_tests:{type:'array',items:{type:'string'}},dropped_comments:{type:'array',items:{type:'string'}},behavior_or_layout_diffs:{type:'array',items:{type:'string'}},unsound_unsafe_or_ub:{type:'array',items:{type:'string'}},notes:{type:'string'},blockers:{type:'array',items:{type:'string'}}}}

phase('Prep')
const prep=await robust(RULES+
 'TASK (prep PageList intra-file decomposition). Replace the crates/rustty-vt/src/PageList stub with a `pagelist/` module dir: (1) pagelist/mod.rs = the PageList struct definition (ALL fields, verbatim-retargeted, same layout), the internal Node/page-pool/list types it owns, and the CORE lifecycle (init, deinit, reset) + the lowest-level private page-management helpers that every cluster needs — ported faithfully with their comments + tests. (2) Stub sub-files (each `impl PageList { }` empty + a doc header): grow.rs (grow/increaseCapacity/compact/scrollbar/maxSize), scroll.rs, resize.rs, erase.rs, pin.rs (pins+tracking), iter.rs (prompt/cell/row/page iterators), access.rs (getCell/getTopLeft/getBottomRight/totalPages/dirty/clone/split/diagram/highlightSemanticContent/cval/eql). (3) Update lib.rs: `pub mod pagelist;` + `pub use pagelist::PageList;` (replace the old stub mod). (4) `cargo build -p rustty-vt` GREEN. Commit "rustty: vt PageList module skeleton (sub-batch 3 prep)" + Co-Authored-By: Claude Opus 4.8 (1M context). Keep lib.rs PORT STATUS stub. Return ok+tests_passing(core tests if any)+blockers.',
 {schema:PORT,effort:'high',label:'pl-prep',phase:'Prep'})

phase('Clusters')
const CL=[
 {id:'core', desc:'mod.rs CORE already has init/deinit/reset; now port clone + the remaining core/page-pool methods that other clusters depend on, and getTopLeft/getBottomRight/totalPages/maxSize. Plus the tests exercising these.'},
 {id:'grow', desc:'grow.rs: grow, increaseCapacity, compact, scrollbar, and their tests.'},
 {id:'scroll', desc:'scroll.rs: scroll, scrollClear, and their tests.'},
 {id:'resize', desc:'resize.rs: resize and ALL its private helpers (the most complex method — reflow/no-reflow, grow/shrink cols+rows) and the MANY resize tests. This is the largest cluster.'},
 {id:'erase', desc:'erase.rs: eraseRow, eraseRowBounded, eraseHistory, eraseActive, and their tests.'},
 {id:'pin', desc:'pin.rs: pin, trackPin, untrackPin, countTrackedPins, trackedPins, pinIsValid, pointFromPin, the Pin type methods, and their tests.'},
 {id:'iter', desc:'iter.rs + access.rs: promptIterator/cellIterator/rowIterator/pageIterator (+ their next()), getCell, clearDirty/isDirty, clone(if not in core), split, diagram, highlightSemanticContent, cval, eql, and ALL remaining PageList tests not yet ported.'},
]
const clusterResults=[]
for(let i=0;i<CL.length;i++){
 const c=CL[i]
 const r=await robust(RULES+
  'TASK: port PageList cluster **'+c.id+'** into crates/rustty-vt/src/pagelist/ (skeleton exists; FILL your cluster`s impl block + tests; do not touch other clusters` files or lib.rs). CLUSTER: '+c.desc+'\n'+
  'Port the methods to faithful Rust (impl PageList in '+c.id+'.rs or the noted file), preserving EVERY comment verbatim; port the tests exercising these methods as #[test] assertion-for-assertion with INDEPENDENT expected values. Verify (real exit codes): cargo build -p rustty-vt, cargo test -p rustty-vt (cumulative PageList tests pass), clippy -D (parity #[allow]+note), check_comment_coverage --crate rustty-vt (your methods` comments covered — no NEW missing from your file), check_leaks --crate rustty-vt (Miri 0 UB/leaks), fmt. Commit "rustty: vt PageList cluster '+c.id+' (sub-batch 3)" + trailer. Keep PORT STATUS stub. Return ok+methods_done+tests_passing+g3b_clean_for_my_files+blockers. If a method needs a not-yet-ported sibling (e.g. Screen), use a documented deferred seam; do NOT pull in unported files. NEVER weaken/tautologize/drop.',
  {schema:PORT,effort:'high',label:'pl:'+c.id,phase:'Clusters'})
 clusterResults.push({cluster:c.id,r})
 if(r && r.ok===false && r.blockers && r.blockers.length) break
}

phase('Integrate')
const integ=await robust(RULES+
 'TASK (integrate PageList). (1) cargo build -p rustty-vt GREEN. (2) LOOP to green (real exit codes): cargo test -p rustty-vt (ALL PageList`s 205 tests present+passing — verify against PageList.zig`s 205 test blocks; the vt crate total grows accordingly), clippy -D, check_comment_coverage --crate rustty-vt (PageList files: 0 missing — restore any dropped/reworded comment across pagelist/*.rs), check_leaks --crate rustty-vt (Miri 0 UB/leaks — PageList`s pin/offset raw-pointer code), fmt, check_acyclic. (3) full '+R+'/.port/tools/ci.sh -> PASS. (4) commit "rustty: complete PageList (sub-batch 3) — 205 tests" + trailer. (5) write '+R+'/.port/phase-2/vt3-REPORT.md. Keep lib.rs PORT STATUS stub (vt still has more sub-batches). Return verdict GREEN only if all PageList tests present+passing + G3b 0 + Miri clean + ci.sh PASS; else RED + precise blockers (never weaken). Report pagelist_tests + g3b_missing_pagelist + miri_clean.',
 {schema:INTEG,effort:'high',label:'pl-integrate',phase:'Integrate'})

phase('Review')
const review=await robust(RULES+
 'TASK (adversarial review of PageList — default NOT_PARITY when uncertain). Integrate claims: '+JSON.stringify(integ)+'\n'+
 'vs '+G+'/src/terminal/PageList.zig: (1) all 205 test{} ported, NONE tautological/neutered/stubbed (sample the resize, scroll, pin, and iterator tests — expected values independent, not echoing PageList state). (2) run check_comment_coverage.py --crate rustty-vt; confirm NONE of the MISSING belong to pagelist/*.rs (remaining missing = other still-stub vt files only). (3) behavior/layout divergence (scrollback growth, reflow on resize, pin tracking across page splits, offset arithmetic). (4) unsound unsafe/UB (pins + offset arena raw pointers; provenance/bounds). (5) RE-RUN cargo test -p rustty-vt + check_leaks --crate rustty-vt (Miri) with captured exit codes. Any tautology/drop/weakening/behavior-diff/unsound-unsafe/missing-test => NOT_PARITY with specifics.',
 {schema:REVIEW,effort:'high',label:'pl-review',phase:'Review'})
return {prep, clusterResults, integ, review}
