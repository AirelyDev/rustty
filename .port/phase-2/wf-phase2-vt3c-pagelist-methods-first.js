export const meta = {
  name: 'phase2-vt3c-pagelist-methods-first',
  description: 'rustty-vt PageList (205 tests), corrected structure: METHODS-FIRST then TESTS-LAST. Core (struct: zigstd MemoryPool for Node/Pin pools, hand-rolled faithful page-aligned buffer manager, AutoArrayHashMap PinSet) + lifecycle; then method clusters (grow,scroll,resize,erase,pin,iter,access) port METHODS ONLY (no per-cluster test gating, methods call each other freely); then ALL 205 tests; then integrate + Miri + adversarial review. Null-guarded.',
  phases: [
    { title: 'Core', detail: 'PageList struct + page-buffer manager + lifecycle + pure getters (methods, no tests)' },
    { title: 'Methods', detail: 'grow,scroll,resize,erase,pin,iter,access METHODS only (crate compiles cumulatively)' },
    { title: 'Tests', detail: 'port ALL 205 PageList tests (full method surface now exists)' },
    { title: 'Integrate+Review', detail: '205 pass + G3b + Miri + ci.sh; adversarial review' },
  ],
}
const G='/Users/gael/projects/ghostty', R='/Users/gael/projects/rustty'
const INV=R+'/.port/phase-0/inventory', GATES=R+'/.port/tools/gates'
const RULES='PARITY CONSTITUTION + '+R+'/.port/PORT_RULES.md. R1=100% fidelity: EVERY line ported; EVERY comment VERBATIM (retarget ghostty->rustty only, R2); EVERY test{} as #[test] ASSERTION-FOR-ASSERTION; nothing dropped/reworded/weakened/stubbed/tautologized. TAUTOLOGY TRAP: test expected values from an INDEPENDENT source. G3b (HARDENED) catches dropped comments. R3: NO ghostty-tree dep. MEMORY MODEL: use rustty_zigstd::MemoryPool for the typed Node pool + Pin pool, and rustty_zigstd::AutoArrayHashMapUnmanaged for PinSet. For the PAGE BUFFERS, do NOT try to use zigstd MemoryPoolAligned internals (its API does not expose what PageList needs); instead implement the page-buffer management DIRECTLY in pagelist/mod.rs as the faithful behavioral equivalent of the reference`s `MemoryPoolAligned([std_size]u8, page_align)` + `arena.child_allocator` usage: a free-list of std_size page-aligned buffers (via std::alloc with Layout aligned to page_size_min) reused LIFO; NON-STANDARD pages (layout.total_size > std_size) allocated directly page-aligned and freed individually; reset() retains buffers up to retain_with_limit AND zeroes the retained buffers (the reference @memset(0) on retained mmap memory) — reproduce this behavior verbatim with a documented PORT NOTE that it mirrors the Zig MemoryPoolAligned/arena path. page.Page/Cell/Row (byte-identical), render-types Offset/size::MAX_PAGE_SIZE are ported. STRUCTURE FIX (critical): port METHODS first across all clusters (impl PageList blocks in pagelist/*.rs; methods may call each other freely; the crate compiles once all methods exist), and port the 205 TESTS LAST in a dedicated phase (the tests are integration tests exercising many methods, so they cannot be split per-cluster). MEMORY GATE: native cargo test (all) + Miri (pins/page-buffer raw pointers — 0 UB/leaks). rustty-vt CROWN JEWEL, MULTI-SUB-BATCH: keep lib.rs PORT STATUS: stub. Reference READ-ONLY '+G+'/src/terminal/PageList.zig (14803 loc, 205 tests). Contract: '+INV+'/tests-summary.json crate_map "vt". Capture output to a var THEN grep. Never claim an unrun green. '
async function robust(p,o){let r=await agent(p,o);if(!r)r=await agent(p,{...o,label:(o.label||'a')+'-retry'});return r}
const PORT={type:'object',additionalProperties:true,required:['ok','blockers'],properties:{ok:{type:'boolean'},methods_done:{type:'array',items:{type:'string'}},compiles:{type:'boolean'},tests_passing:{type:'integer'},notes:{type:'string'},blockers:{type:'array',items:{type:'string'}}}}
const INTEG={type:'object',additionalProperties:true,required:['verdict','blockers'],properties:{verdict:{type:'string',enum:['GREEN','RED']},pagelist_tests:{type:'integer'},g3b_missing_pagelist:{type:'integer'},miri_clean:{type:'boolean'},commit:{type:'string'},blockers:{type:'array',items:{type:'string'}}}}
const REVIEW={type:'object',additionalProperties:true,required:['verdict','blockers'],properties:{verdict:{type:'string',enum:['PARITY','NOT_PARITY']},tautological_or_neutered_tests:{type:'array',items:{type:'string'}},dropped_comments:{type:'array',items:{type:'string'}},behavior_or_layout_diffs:{type:'array',items:{type:'string'}},unsound_unsafe_or_ub:{type:'array',items:{type:'string'}},notes:{type:'string'},blockers:{type:'array',items:{type:'string'}}}}

phase('Core')
let core=await robust(RULES+
 'TASK: PageList CORE (methods, NO tests yet) into crates/rustty-vt/src/pagelist/mod.rs (skeleton from 9f93baf has the struct + todo! stubs + a MemoryPool SHELL to REMOVE). Rework the struct: Node pool = rustty_zigstd::MemoryPool<Node>; Pin pool = rustty_zigstd::MemoryPool<Pin>; PinSet = rustty_zigstd::AutoArrayHashMapUnmanaged; PAGE buffers = a hand-rolled faithful page-aligned buffer manager (per RULES — std_size buffer free-list + non-standard direct alloc + reset retain+zero). Preserve ALL struct fields + comments verbatim. Port (methods only, faithful): init, deinit, reset, initPages, createPage, createPageExt, destroyNode, destroyNodeExt, minMaxSize, initialCapacity (uses render-types MAX_PAGE_SIZE), verifyIntegrity/assertIntegrity/pauseIntegrityChecks, and the pure getters getTopLeft/totalRows/totalPages/maxSize. Defer clone + getBottomRight (need iter/pin/grow — they land in the Methods phase). NO tests in this phase. Gates: cargo build -p rustty-vt GREEN, clippy -D, fmt, check_comment_coverage --crate rustty-vt (mod.rs comments covered). Commit "rustty: PageList core methods on zigstd (vt-3c)" + Co-Authored-By: Claude Opus 4.8 (1M context). Keep PORT STATUS stub. Return ok+methods_done+compiles+blockers. If a CORE method genuinely needs an un-ported sibling, note it for the Methods phase; do NOT fabricate.',
 {schema:PORT,effort:'high',label:'pl-core',phase:'Core'})

phase('Methods')
const MC=['grow (grow,increaseCapacity,compact,split,scrollbar)','scroll (scroll,scrollClear)','resize (resize + all reflow/grow/shrink helpers)','erase (eraseRow,eraseRowBounded,eraseHistory,eraseActive)','pin (pin,trackPin,untrackPin,countTrackedPins,trackedPins,pinIsValid,pointFromPin + full impl Pin: rowAndCell,cells,isBetween,before,leftWrap,rightWrap,down,up,eql)','iter (promptIterator,cellIterator,rowIterator,pageIterator + their next())','access (getCell,getBottomRight,clone,diagram,highlightSemanticContent,clearDirty,isDirty,cval,eql + any remaining methods)']
const methodResults=[]
for(let i=0;i<MC.length;i++){const c=MC[i]
 const r=await robust(RULES+
  'TASK: port PageList METHODS (NO tests) for cluster: '+c+' — into the matching crates/rustty-vt/src/pagelist/*.rs file (impl PageList block). Methods may call methods in other cluster files (all are being ported; the crate compiles once each phase`s methods land). Preserve EVERY comment verbatim. Use rustty_zigstd for allocator/pool/ordered-map needs. After porting: cargo build -p rustty-vt GREEN (cumulative), clippy -D, fmt, check_comment_coverage --crate rustty-vt (your methods` comments covered, no NEW missing from your file). Commit "rustty: PageList methods '+c.split(' ')[0]+' (vt-3c)" + trailer. Keep PORT STATUS stub. Return ok+methods_done+compiles+blockers. NEVER fabricate/drop a comment. If a method needs a genuinely-unported NON-PageList sibling (e.g. Screen), use a documented deferred seam.',
  {schema:PORT,effort:'high',label:'pl-m:'+c.split(' ')[0],phase:'Methods'})
 methodResults.push({cluster:c.split(' ')[0],r})
 if(r && r.ok===false && r.blockers && r.blockers.length) break
}

phase('Tests')
const TB=['part 1: init/deinit/reset, grow/increaseCapacity/compact/split, scrollbar, getTopLeft/getBottomRight/totalRows/totalPages/maxSize tests','part 2: scroll/scrollClear, resize (all reflow/grow/shrink) tests','part 3: erase (all), pin/trackPin/untrackPin/pinIsValid/pointFromPin/Pin-method tests','part 4: iterators (prompt/cell/row/page), getCell, clone, diagram, dirty, and ALL remaining PageList tests']
const testResults=[]
for(let i=0;i<TB.length;i++){const t=TB[i]
 const r=await robust(RULES+
  'TASK: port a BATCH of PageList tests (all PageList methods are now ported). BATCH: '+t+'. Port these test{} blocks from '+G+'/src/terminal/PageList.zig as #[test] in the appropriate pagelist/*.rs (co-locate with the method they primarily exercise, or a pagelist/tests.rs), ASSERTION-FOR-ASSERTION with INDEPENDENT expected values (NO tautologies). Gates: cargo test -p rustty-vt (your new tests pass, cumulative), clippy -D, fmt, check_leaks --crate rustty-vt (Miri 0 UB/leaks on these tests). Commit "rustty: PageList tests '+(i+1)+'/4 (vt-3c)" + trailer. Return ok+tests_passing+blockers. NEVER weaken/tautologize/drop a test or comment.',
  {schema:PORT,effort:'high',label:'pl-t'+(i+1),phase:'Tests'})
 testResults.push({batch:i+1,r})
 if(r && r.ok===false && r.blockers && r.blockers.length) break
}

phase('Integrate+Review')
const integ=await robust(RULES+
 'TASK (integrate PageList). cargo build -p rustty-vt GREEN; LOOP to green (real exit codes): cargo test -p rustty-vt (ALL 205 PageList tests present+passing — verify vs PageList.zig`s 205 test blocks), clippy -D, check_comment_coverage --crate rustty-vt (pagelist/*.rs 0 missing), check_leaks --crate rustty-vt (Miri 0 UB/leaks), fmt, check_acyclic; full '+R+'/.port/tools/ci.sh -> PASS. commit "rustty: complete PageList (vt-3c) — 205 tests" + trailer. Keep lib.rs PORT STATUS stub. Return GREEN only if all 205 present+passing + G3b 0 + Miri clean + ci.sh PASS; else RED + precise blockers. Report pagelist_tests + g3b_missing_pagelist + miri_clean.',
 {schema:INTEG,effort:'high',label:'pl-integrate',phase:'Integrate+Review'})
const review=await robust(RULES+
 'TASK (adversarial review of PageList — default NOT_PARITY when uncertain). Integrate: '+JSON.stringify(integ)+'\n'+
 'vs '+G+'/src/terminal/PageList.zig: (1) all 205 test{} ported, NONE tautological/neutered (sample resize/scroll/pin/iter). (2) check_comment_coverage --crate rustty-vt: NONE of MISSING in pagelist/*.rs. (3) behavior divergence (scrollback growth, resize reflow, pin tracking across splits, the hand-rolled page-buffer manager: std_size reuse + non-standard alloc + reset retain+zero MUST match the reference MemoryPoolAligned/arena behavior). (4) unsound unsafe/UB (page buffers + pins). (5) RE-RUN cargo test -p rustty-vt + check_leaks --crate rustty-vt (Miri) with captured exit codes. Any tautology/drop/weakening/behavior-diff/unsound-unsafe/missing-test => NOT_PARITY with specifics.',
 {schema:REVIEW,effort:'high',label:'pl-review',phase:'Integrate+Review'})
return {core, methodResults, testResults, integ, review}
