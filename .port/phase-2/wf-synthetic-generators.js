export const meta = {
  name: 'synthetic-generators',
  description: 'Sub-project #3 — port ghostty src/synthetic/** (Bytes/Generator/Osc/Utf8/cli + cli/{Ascii,Osc,Utf8}/main, ~800 LOC, 9 test{}) into rustty-synthetic: design -> port -> verify(+Miri)',
  phases: [
    { title: 'Design', detail: 'brief faithful-port design + STOP-guard' },
    { title: 'Port', detail: 'port all synthetic files + tests into rustty-synthetic' },
    { title: 'Verify', detail: 'gates + R1 fidelity + Miri (pure crate)' },
  ],
}

const R = '/Users/gael/projects/rustty'
const G = '/Users/gael/projects/ghostty'
const DESIGN = `${R}/.port/phase-2/SYNTHETIC_DESIGN.md`

const rules = `
Full-fidelity Zig->Rust port (rustty). Reference (read-only): ghostty at ${G}. Target: ${R} (crate crates/rustty-synthetic; currently a stub lib.rs). rustty-synthetic already deps rustty-vt + rustty-os (+ add rustty-args if cli parsing needs it — it is below both, acyclic).
R1: EVERY line/comment/test{} ported — assertion-for-assertion; Zig byte-slices -> &[u8]/Vec<u8>/CString, NEVER lossy String; comptime artifacts bit-faithful. R4 acyclic (check_acyclic.sh). R6: rustty-synthetic is PURE Rust -> Miri is the primary gate (cargo +nightly miri test -p rustty-synthetic). Gates: build/test/clippy -D/fmt/check_acyclic.sh/check_test_counts.py/check_comment_coverage.py + Miri.
SCOPE (ghostty src/synthetic/): Generator.zig (42 LOC, the PRNG generator interface), Bytes.zig (119, 1 test), Osc.zig (366, 3 tests — generates OSC sequences, deps terminal + os/string_encoding.urlPercentEncode), Utf8.zig (229, 3 tests), cli.zig (115, 1 test), main.zig (25, 1 test), cli/Ascii.zig (68,1), cli/Osc.zig (82,1), cli/Utf8.zig (111,1). Total ~9 test{} — the Phase-0 count contract; check_test_counts.py must go >= for the synthetic module.
Set crate lib.rs 'PORT STATUS: done' once filled (matches the convention).`

const D_SCHEMA = { type:'object', additionalProperties:false, required:['doc_path','disposition','summary','key_decisions','risks'], properties:{
  doc_path:{type:'string'}, disposition:{type:'string',description:"'ready' or 'blocked: <reason>'"}, summary:{type:'string'}, key_decisions:{type:'array',items:{type:'string'}}, risks:{type:'array',items:{type:'string'}} } }

phase('Design')
const design = await agent(
  `${rules}\n\nDESIGN PHASE. Write ${DESIGN} — a concise faithful-port design for rustty-synthetic. Read ALL of ${G}/src/synthetic/*.zig + cli/*.zig first. Map each Zig file -> a rustty module (Generator/Bytes/Osc/Utf8/cli/main + cli/{ascii,osc,utf8}); pin the PRNG (std.Random) -> Rust equivalent (faithful seedable PRNG — likely a small ported xoshiro/the exact std.Random algorithm ghostty uses, NOT rand crate unless it matches byte-for-byte; if the generator output must be bit-reproducible, port the exact algorithm), the OSC generation's terminal + urlPercentEncode deps, and the 9 test{} blocks (assertion-for-assertion). Note any comptime/byte-faithfulness concern. If a genuine blocker, disposition 'blocked' + STOP. Else 'ready'. Return the object.`,
  { label:'design:synthetic', phase:'Design', agentType:'general-purpose', schema:D_SCHEMA })

if (!design || (design.disposition||'').toLowerCase().startsWith('blocked')) {
  log(`SYNTHETIC DESIGN BLOCKED: ${design ? design.disposition : 'design failed'}`)
  return { blocked:true, design }
}

phase('Port')
const P_SCHEMA = { type:'object', additionalProperties:false, required:['disposition','files_changed','build_ok','tests_ported','miri_clean','notes'], properties:{
  disposition:{type:'string'}, files_changed:{type:'array',items:{type:'string'}}, build_ok:{type:'boolean'}, tests_ported:{type:'number'}, miri_clean:{type:'boolean'}, notes:{type:'string'} } }
const port = await agent(
  `${rules}\n\nPORT PHASE. Implement per ${DESIGN} — if blocked, STOP. Port EVERY synthetic file into crates/rustty-synthetic/src/ (module per Zig file), preserving every comment (R1) and porting all 9 test{} assertion-for-assertion. Wire the module tree in lib.rs + set 'PORT STATUS: done'. Add rustty-args dep if cli parsing needs it. Build \`cargo build -p rustty-synthetic\`, run \`cargo test -p rustty-synthetic\` (all tests pass), and \`cargo +nightly miri test -p rustty-synthetic\` (pure crate -> must be Miri-clean). Report files_changed, build_ok, tests_ported (count), miri_clean, notes (incl. the PRNG fidelity approach). Do NOT git commit.`,
  { label:'port:synthetic', phase:'Port', agentType:'general-purpose', schema:P_SCHEMA })

if (!port || (port.disposition||'').toLowerCase().startsWith('stopped')) {
  log(`SYNTHETIC PORT STOPPED`)
  return { design, port, verify:null }
}

phase('Verify')
const V_SCHEMA = { type:'object', additionalProperties:false, required:['dimension','verdict','findings'], properties:{
  dimension:{type:'string'}, verdict:{enum:['PASS','FAIL']}, findings:{type:'array',items:{type:'string'}} } }
const verify = await parallel([
  () => agent(`${rules}\n\nVERIFY (GATES+MIRI). Independently re-run + confirm GREEN: cargo build --workspace; cargo test -p rustty-synthetic; cargo +nightly miri test -p rustty-synthetic; cargo clippy -p rustty-synthetic --all-targets -- -D warnings; cargo fmt --check; bash .port/tools/gates/check_acyclic.sh; python3 .port/tools/gates/check_test_counts.py; python3 .port/tools/gates/check_comment_coverage.py. Report each. verdict PASS only if ALL green (incl. Miri).`,
    { label:'verify:gates', phase:'Verify', agentType:'Explore', schema:V_SCHEMA }),
  () => agent(`${rules}\n\nVERIFY (R1 FIDELITY). Diff each ported rustty-synthetic module vs its ghostty src/synthetic/*.zig: (1) every comment preserved verbatim? (2) all 9 test{} blocks ported assertion-for-assertion (count + content)? (3) the PRNG/Generator output bit-faithful to the Zig std.Random algorithm (a differential fuzzer's generator MUST reproduce the same sequence — verify the algorithm, not just 'a PRNG')? (4) OSC/Utf8 generation byte-exact (urlPercentEncode faithful)? (5) no lossy String? Report findings; verdict PASS only if R1 fully intact.`,
    { label:'verify:fidelity', phase:'Verify', agentType:'Explore', schema:V_SCHEMA }),
])

return { design, port, verify }
