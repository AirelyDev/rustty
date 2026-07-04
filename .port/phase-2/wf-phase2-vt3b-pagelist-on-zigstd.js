export const meta = {
  name: 'phase2-vt3b-pagelist-on-zigstd',
  description: 'rustty-vt sub-batch 3b: port PageList (205 tests) now that rustty-zigstd (MemoryPool/MemoryPoolAligned/ArenaAllocator/AutoArrayHashMap) exists. Core stage reworks the struct + lifecycle on zigstd, then method clusters (grow,scroll,resize,erase,pin,iter+access) port methods+tests, then integrate (205) + Miri + adversarial review. vt stays PORT STATUS stub. Null-guarded.',
  phases: [
    { title: 'Core', detail: 'PageList struct on rustty-zigstd pools + init/deinit/reset/clone/createPage/getters + core tests' },
    { title: 'Clusters', detail: 'grow, scroll, resize, erase, pin, iter+access — methods + their tests' },
    { title: 'Integrate', detail: 'all 205 PageList tests + G3b + Miri + ci.sh' },
    { title: 'Review', detail: 'adversarial: tautologies, comments, behavior, unsafe/UB' },
  ],
}
const G='/Users/gael/projects/ghostty', R='/Users/gael/projects/rustty'
const INV=R+'/.port/phase-0/inventory', GATES=R+'/.port/tools/gates'
const RULES='PARITY CONSTITUTION + '+R+'/.port/PORT_RULES.md. R1=100% fidelity: EVERY line ported; EVERY comment VERBATIM (retarget ghostty->rustty only, R2); EVERY test{} as #[test] ASSERTION-FOR-ASSERTION; nothing dropped/reworded/weakened/stubbed/tautologized. TAUTOLOGY TRAP (hit unicode): test expected values from an INDEPENDENT source. G3b (check_comment_coverage.py, HARDENED) catches dropped comments. R3: NO ghostty-tree dep. The MEMORY MODEL is now available: use `rustty_zigstd::{ArenaAllocator (ResetMode FreeAll/RetainCapacity/RetainWithLimit), MemoryPool, MemoryPoolAligned, AutoArrayHashMap/AutoArrayHashMapUnmanaged}` (faithful Zig-stdlib equivs, Miri-clean) for PageList`s NodePool/PagePool(aligned)/PinPool + ArenaAllocator(reset retain_with_limit) + PinSet. page.Page/Cell/Row (byte-identical) + render-types Offset/size (MAX_PAGE_SIZE) are ported. Add rustty-zigstd to crates/rustty-vt/Cargo.toml deps. MEMORY GATE: native cargo test (all) + Miri (PageList pins/offset raw pointers — UAF/OOB/uninit MUST be clean; the int-to-ptr relative-Offset provenance pattern is the documented sound one). rustty-vt CROWN JEWEL, MULTI-SUB-BATCH: port ONLY PageList; keep lib.rs PORT STATUS: stub. The pagelist/ skeleton exists (commit 9f93baf, todo! stubs) — replace the MemoryPool SHELL in mod.rs with rustty_zigstd pools. Reference READ-ONLY '+G+'/src/terminal/PageList.zig (14803 loc, 205 tests). Contract: '+INV+'/tests-summary.json crate_map "vt", tests.jsonl, comments-census.json. Capture output to a var THEN grep (`| tail` masks exit codes). Never claim an unrun green. '
async function robust(p,o){let r=await agent(p,o);if(!r)r=await agent(p,{...o,label:(o.label||'a')+'-retry'});return r}
const PORT={type:'object',additionalProperties:true,required:['ok','tests_passing','blockers'],properties:{ok:{type:'boolean'},methods_done:{type:'array',items:{type:'string'}},tests_passing:{type:'integer'},g3b_clean_for_my_files:{type:'boolean'},notes:{type:'string'},blockers:{type:'array',items:{type:'string'}}}}
const INTEG={type:'object',additionalProperties:true,required:['verdict','blockers'],properties:{verdict:{type:'string',enum:['GREEN','RED']},pagelist_tests:{type:'integer'},g3b_missing_pagelist:{type:'integer'},miri_clean:{type:'boolean'},commit:{type:'string'},blockers:{type:'array',items:{type:'string'}}}}
const REVIEW={type:'object',additionalProperties:true,required:['verdict','blockers'],properties:{verdict:{type:'string',enum:['PARITY','NOT_PARITY']},tautological_or_neutered_tests:{type:'array',items:{type:'string'}},dropped_comments:{type:'array',items:{type:'string'}},behavior_or_layout_diffs:{type:'array',items:{type:'string'}},unsound_unsafe_or_ub:{type:'array',items:{type:'string'}},notes:{type:'string'},blockers:{type:'array',items:{type:'string'}}}}

phase('Core')
let core=await robust(RULES+
 'TASK: port the PageList CORE on rustty-zigstd, into crates/rustty-vt/src/pagelist/mod.rs (skeleton from 9f93baf has the struct + todo! stubs + a MemoryPool SHELL to REMOVE). (1) Add rustty-zigstd to crates/rustty-vt/Cargo.toml deps. (2) Rework the PageList struct: the `pool` field uses rustty_zigstd MemoryPool(Node) / MemoryPoolAligned(page buffer, page-aligned) / MemoryPool(Pin) + the page-arena via rustty_zigstd::ArenaAllocator; PinSet uses rustty_zigstd::AutoArrayHashMapUnmanaged. Remove the placeholder shell. Preserve ALL struct fields + comments verbatim. (3) Port faithfully: init, deinit, reset (uses ArenaAllocator reset(retain_with_limit) + the BufNode-zeroing behavior verbatim), initPages, createPage/createPageExt, destroyNode/destroyNodeExt, minMaxSize, initialCapacity (uses render-types MAX_PAGE_SIZE), clone, and the pure getters getTopLeft/getBottomRight/totalPages/totalRows/maxSize, plus verifyIntegrity/assertIntegrity/pauseIntegrityChecks. (4) Port the tests exercising these (init/deinit/reset/clone/getters). Gates (real exit codes): cargo build/test -p rustty-vt (core tests pass), clippy -D, check_comment_coverage --crate rustty-vt (mod.rs comments covered, no NEW missing from mod.rs), check_leaks --crate rustty-vt (Miri 0 UB/leaks), fmt. Commit "rustty: vt PageList core on zigstd (sub-batch 3b)" + Co-Authored-By: Claude Opus 4.8 (1M context). Keep PORT STATUS stub. Return ok+methods_done+tests_passing+g3b_clean_for_my_files+blockers. If still blocked, STOP with the precise missing piece (do NOT fabricate).',
 {schema:PORT,effort:'high',label:'pl-core',phase:'Core'})

phase('Clusters')
const CL=[
 {id:'grow', desc:'grow.rs: grow, increaseCapacity, compact, scrollbar (+ their tests).'},
 {id:'scroll', desc:'scroll.rs: scroll, scrollClear (+ their tests).'},
 {id:'resize', desc:'resize.rs: resize + ALL private helpers (reflow/no-reflow, grow/shrink cols+rows) + the many resize tests (the largest cluster).'},
 {id:'erase', desc:'erase.rs: eraseRow, eraseRowBounded, eraseHistory, eraseActive (+ tests).'},
 {id:'pin', desc:'pin.rs: pin, trackPin, untrackPin, countTrackedPins, trackedPins, pinIsValid, pointFromPin, Pin type methods (rowAndCell, cells, isBetween, before, leftWrap/rightWrap, down/up overflow) (+ tests). Uses AutoArrayHashMap PinSet.'},
 {id:'iter', desc:'iter.rs + access.rs: promptIterator/cellIterator/rowIterator/pageIterator (+ next()), getCell, clearDirty/isDirty, split, diagram, highlightSemanticContent, cval, eql, and ALL remaining PageList tests not yet ported.'},
]
const clusterResults=[]
for(let i=0;i<CL.length;i++){const c=CL[i]
 const r=await robust(RULES+
  'TASK: port PageList cluster **'+c.id+'** into crates/rustty-vt/src/pagelist/ (core + skeleton exist; FILL your cluster`s impl block + tests in '+c.id+'.rs; do not touch other clusters/lib.rs). CLUSTER: '+c.desc+'\n'+
  'Faithful Rust (impl PageList), EVERY comment verbatim, EVERY test as #[test] assertion-for-assertion with INDEPENDENT expected values. Use rustty_zigstd for any allocator/pool/ordered-map needs. Verify (real exit codes): cargo build/test -p rustty-vt (cumulative pass), clippy -D, check_comment_coverage --crate rustty-vt (your comments covered, no NEW missing), check_leaks --crate rustty-vt (Miri 0 UB/leaks), fmt. Commit "rustty: vt PageList cluster '+c.id+' (sub-batch 3b)" + trailer. Keep PORT STATUS stub. Return ok+methods_done+tests_passing+g3b_clean_for_my_files+blockers. Deferred seam (documented) only for genuinely-unported siblings (e.g. Screen); never pull unported files. NEVER weaken/tautologize/drop.',
  {schema:PORT,effort:'high',label:'pl:'+c.id,phase:'Clusters'})
 clusterResults.push({cluster:c.id,r})
 if(r && r.ok===false && r.blockers && r.blockers.length) break
}

phase('Integrate')
const integ=await robust(RULES+
 'TASK (integrate PageList). (1) cargo build -p rustty-vt GREEN. (2) LOOP to green (real exit codes): cargo test -p rustty-vt (ALL 205 PageList tests present+passing — verify vs PageList.zig`s 205 test blocks), clippy -D, check_comment_coverage --crate rustty-vt (pagelist/*.rs: 0 missing), check_leaks --crate rustty-vt (Miri 0 UB/leaks), fmt, check_acyclic. (3) full '+R+'/.port/tools/ci.sh -> PASS. (4) commit "rustty: complete PageList (sub-batch 3b) — 205 tests" + trailer. (5) write '+R+'/.port/phase-2/vt3-REPORT.md. Keep lib.rs PORT STATUS stub. Return GREEN only if all 205 present+passing + G3b 0 + Miri clean + ci.sh PASS; else RED + precise blockers (never weaken). Report pagelist_tests + g3b_missing_pagelist + miri_clean.',
 {schema:INTEG,effort:'high',label:'pl-integrate',phase:'Integrate'})

phase('Review')
const review=await robust(RULES+
 'TASK (adversarial review of PageList — default NOT_PARITY when uncertain). Integrate: '+JSON.stringify(integ)+'\n'+
 'vs '+G+'/src/terminal/PageList.zig: (1) all 205 test{} ported, NONE tautological/neutered/stubbed (sample resize/scroll/pin/iterator tests — independent expected values). (2) check_comment_coverage.py --crate rustty-vt: NONE of MISSING belong to pagelist/*.rs. (3) behavior divergence (scrollback growth, resize reflow, pin tracking across splits, ArenaAllocator reset(retain_with_limit) usage). (4) unsound unsafe/UB (pins + offset arena + zigstd pools). (5) RE-RUN cargo test -p rustty-vt + check_leaks --crate rustty-vt (Miri) with captured exit codes. Any tautology/drop/weakening/behavior-diff/unsound-unsafe/missing-test => NOT_PARITY with specifics.',
 {schema:REVIEW,effort:'high',label:'pl-review',phase:'Review'})
return {core, clusterResults, integ, review}
