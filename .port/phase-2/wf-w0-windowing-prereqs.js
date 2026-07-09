export const meta = {
  name: 'w0-windowing-prereqs',
  description: 'Sub-project #1 chunk W0 (deps + cf_release_thread xev rewire + IOSurfaceLayer port): design -> adversary -> fold -> port -> verify',
  phases: [
    { title: 'Design', detail: 'write W0_DESIGN.md — the exact port design for T1/T2/T3' },
    { title: 'Adversary', detail: 'independently re-grep references to break the design' },
    { title: 'Fold', detail: 'fold fixes into W0_DESIGN.md' },
    { title: 'Port', detail: 'make the real edits per the design; build + KATs' },
    { title: 'Verify', detail: 'gates + R1 fidelity panel' },
  ],
}

const R = '/Users/gael/projects/rustty'
const G = '/Users/gael/projects/ghostty'
const PLAN = `${R}/docs/superpowers/plans/2026-07-08-macos-windowing-sub1.md`
const DESIGN = `${R}/.port/phase-2/W0_DESIGN.md`

const rules = `
Full-fidelity Zig->Rust port (rustty). Reference (read-only): ghostty at ${G}. Target: ${R}.
Governing plan: ${PLAN} — chunk W0, tasks W0.T1/T2/T3, plus the plan's "Canonical Interfaces" + "Global Constraints" (AUTHORITATIVE).
R1 (100% fidelity: every comment/test/assertion ported; Zig byte-slices -> &[u8]/Vec<u8>/CString, NEVER lossy String; deferred = unimplemented!() + verbatim body). R4 (acyclic: rustty-renderer MUST NOT dep rustty-apprt-api; check_acyclic.sh green). R6 (pure->Miri, FFI->ASan). Gates: cargo build --workspace / cargo test / clippy -D warnings / fmt / check_acyclic.sh / check_test_counts.py / check_comment_coverage.py.
W0 scope: T1 = add objc2-quartz-core to rustty-renderer + rustty-xev to rustty-os (macOS-gated) + the metal.rs mod line; T2 = rewire crates/rustty-os/src/cf_release_thread.rs off the LOCAL placeholder xev mod (unimplemented!() at ~288-376) onto the REAL rustty-xev Loop/Async, + a live cf_release KAT (crates/rustty-os/tests/cf_release_live.rs); T3 = port ghostty src/renderer/metal/IOSurfaceLayer.zig (183 LOC, CALayer subclass via objc2 define_class/allocateClassPair) -> crates/rustty-renderer/src/metal/iosurface_layer.rs with set_surface->Result + set_surface_sync + set_display_callback + as_ca_layer() upcast (snake_case; the plan's Canonical IOSurfaceLayer interface).`

const DESIGN_SCHEMA = { type:'object', additionalProperties:false, required:['doc_path','disposition','summary','key_decisions','risks'], properties:{
  doc_path:{type:'string'}, disposition:{type:'string',description:"'ready' or 'blocked: <precise reason>'"},
  summary:{type:'string'}, key_decisions:{type:'array',items:{type:'string'}}, risks:{type:'array',items:{type:'string'}} } }

phase('Design')
const design = await agent(
  `${rules}\n\nDESIGN PHASE. Write ${DESIGN} — the EXACT, faithful port design for W0 T1/T2/T3. For each task: the precise edits (file + what changes), the objc2-quartz-core feature gate to use (VERIFY against the crate: CALayer/kCAGravityTopLeft fall under 'CoreAnimation'; CAMetalLayer is its own feature), the cf_release_thread rewire mapping (reference ghostty src/os/cf_release_thread.zig — the CF-release bg thread + mach-port Async wakeup + xev Loop; map the local placeholder Loop/Async->rustty_xev; preserve every comment; the raw *mut Thread callback ABI discipline), the IOSurfaceLayer objc2 define_class subclass shape (init/release/set_surface[main-thread dispatch2, Result]/set_surface_sync/set_display_callback/as_ca_layer), and the exact test KATs (from the plan, made runnable). Cite real reference + current line numbers. Read the actual files first (current cf_release_thread.rs, the reference IOSurfaceLayer.zig + cf_release_thread.zig, existing metal/*.rs objc2 idioms, both Cargo.tomls). If a genuine blocker exists (e.g. objc2-quartz-core cannot express the subclass), set disposition 'blocked: <reason>' and STOP. Otherwise 'ready'. Return the structured object.`,
  { label:'design:W0', phase:'Design', agentType:'general-purpose', schema:DESIGN_SCHEMA })

if (!design || (design.disposition||'').toLowerCase().startsWith('blocked')) {
  log(`W0 DESIGN BLOCKED: ${design ? design.disposition : 'design agent failed'}`)
  return { blocked:true, design }
}

phase('Adversary')
const ADV_SCHEMA = { type:'object', additionalProperties:false, required:['verdict','fixes','blocking_issues'], properties:{
  verdict:{enum:['SOUND','SOUND_WITH_FIXES','UNSOUND']},
  fixes:{type:'array',items:{type:'string'}},
  blocking_issues:{type:'array',items:{type:'string'}} } }
const adversary = await agent(
  `${rules}\n\nADVERSARY PHASE. The design is at ${DESIGN}. Your job is to BREAK it by independent re-grep, NOT to agree. Check: (1) R1 fidelity — does the cf_release_thread rewire preserve EVERY comment + the exact reference control flow (re-read ghostty src/os/cf_release_thread.zig)? any dropped assertion/comment? (2) does the local placeholder xev mod deletion leave any dangling reference / does the real rustty_xev Async.wait callback ABI actually match what the design assumes (re-read crates/rustty-xev/src/async.rs wait signature)? (3) the raw *mut Thread callback aliasing — is it SB/TB-sound (no &mut *self reborrow while a raw alias is live)? (4) objc2-quartz-core feature gate + define_class subclass — is the assumed feature/API real (re-check)? IOSurfaceLayer set_surface main-thread dispatch2 shape faithful to the Zig? (5) do the KATs actually compile against the produced signatures + the cf_release retain-count assertion make sense? (6) acyclicity: rustty-os->rustty-xev edge — confirm it does not cycle. List every real fix in 'fixes' and any true blocker in 'blocking_issues'. verdict SOUND only if zero fixes AND zero blockers.`,
  { label:'adversary:W0', phase:'Adversary', agentType:'Explore', schema:ADV_SCHEMA })

const needsFold = adversary && ((adversary.fixes && adversary.fixes.length) || (adversary.blocking_issues && adversary.blocking_issues.length))
if (needsFold) {
  phase('Fold')
  await agent(
    `${rules}\n\nFOLD PHASE. The adversary found issues on the W0 design at ${DESIGN}. Fixes: ${JSON.stringify(adversary.fixes)}. Blocking issues: ${JSON.stringify(adversary.blocking_issues)}. Update ${DESIGN} IN PLACE to incorporate every fix and resolve every blocker (re-read the reference/current files as needed to get it right). If a blocking issue is genuinely unresolvable, add a top-of-doc 'DISPOSITION: blocked: <reason>' line so the port phase STOPS. Otherwise ensure the doc is fully corrected + coherent. Return a short summary of what you changed.`,
    { label:'fold:W0', phase:'Fold', agentType:'general-purpose' })
}

phase('Port')
const PORT_SCHEMA = { type:'object', additionalProperties:false, required:['disposition','files_changed','build_ok','kats','gate_results','notes'], properties:{
  disposition:{type:'string',description:"'done' or 'stopped: <reason>'"},
  files_changed:{type:'array',items:{type:'string'}},
  build_ok:{type:'boolean'},
  kats:{type:'array',items:{type:'object',additionalProperties:false,required:['name','passed'],properties:{name:{type:'string'},passed:{type:'boolean'}}}},
  gate_results:{type:'array',items:{type:'object',additionalProperties:false,required:['gate','ok'],properties:{gate:{type:'string'},ok:{type:'boolean'}}}},
  notes:{type:'string'} } }
const port = await agent(
  `${rules}\n\nPORT PHASE. Implement W0 STRICTLY per the design at ${DESIGN} — do NOT improvise beyond it; if the doc's first lines say 'DISPOSITION: blocked', STOP and return disposition 'stopped: <reason>' making NO edits. Otherwise make ALL W0 edits in this order (they share files + have deps): T1 (both Cargo.tomls + metal.rs mod) FIRST, then T2 (cf_release_thread.rs + tests/cf_release_live.rs), then T3 (metal/iosurface_layer.rs). Follow R1 verbatim-comment fidelity. After editing: run \`cargo build -p rustty-renderer -p rustty-os\` (fix the objc2-quartz-core feature gate empirically if it fails — try 'CoreAnimation'+'CAMetalLayer'), run the new KATs (\`cargo test -p rustty-os --test cf_release_live\` on macOS; \`cargo test -p rustty-renderer iosurface\`), then the gates (cargo clippy -p rustty-renderer -p rustty-os -- -D warnings; cargo fmt; bash .port/tools/gates/check_acyclic.sh). Report exactly what you changed, build_ok, each KAT pass/fail, each gate ok/fail, and any deviation from the design in notes. Do NOT git commit (the human self-verifies + commits).`,
  { label:'port:W0', phase:'Port', agentType:'general-purpose', schema:PORT_SCHEMA })

if (!port || (port.disposition||'').toLowerCase().startsWith('stopped')) {
  log(`W0 PORT STOPPED: ${port ? port.disposition : 'port agent failed'}`)
  return { design, adversary, port, verify: null }
}

phase('Verify')
const VER_SCHEMA = { type:'object', additionalProperties:false, required:['dimension','verdict','findings'], properties:{
  dimension:{type:'string'}, verdict:{enum:['PASS','FAIL']}, findings:{type:'array',items:{type:'string'}} } }
const verify = await parallel([
  () => agent(`${rules}\n\nVERIFY (GATES). Independently re-run + confirm: cargo build --workspace; cargo test -p rustty-os -p rustty-renderer; cargo clippy -p rustty-os -p rustty-renderer -- -D warnings; cargo fmt --check; bash .port/tools/gates/check_acyclic.sh; python3 .port/tools/gates/check_test_counts.py (if present); python3 .port/tools/gates/check_comment_coverage.py (if present). Report each as a finding with pass/fail. verdict PASS only if ALL green.`,
    { label:'verify:gates', phase:'Verify', agentType:'Explore', schema:VER_SCHEMA }),
  () => agent(`${rules}\n\nVERIFY (R1 FIDELITY). Diff the ported W0 code against the reference: (1) cf_release_thread.rs — every comment from ghostty src/os/cf_release_thread.zig preserved? control flow faithful? the local placeholder xev mod fully removed with no dangling refs? (2) iosurface_layer.rs — faithful to ghostty src/renderer/metal/IOSurfaceLayer.zig (all comments, the CALayer subclass, set_surface main-thread dispatch, setDisplayCallback)? snake_case + Canonical IOSurfaceLayer interface (set_surface->Result, as_ca_layer)? (3) the raw *mut Thread callback aliasing sound? (4) no lossy String where Zig had byte-slices? Report findings; verdict PASS only if R1 is intact.`,
    { label:'verify:fidelity', phase:'Verify', agentType:'Explore', schema:VER_SCHEMA }),
])

return { design, adversary, port, verify }
