export const meta = {
  name: 'zf-fuzzy',
  description: 'TUI-CLI groundwork — port the zf fuzzy finder (zf.zig/filter.zig/clib.zig, ~794 LOC, 9 test{}) into a new rustty-zf crate: design -> port -> verify(+Miri)',
  phases: [
    { title: 'Design', detail: 'brief faithful-port design (rank algorithm bit-fidelity) + STOP-guard' },
    { title: 'Port', detail: 'port zf into rustty-zf + all 9 tests' },
    { title: 'Verify', detail: 'gates + R1 fidelity (rank values identical) + Miri' },
  ],
}

const R = '/Users/gael/projects/rustty'
const ZF = '/Users/gael/.cache/zig/p/zf-0.10.3-OIRy8RuJAACKA3Lohoumrt85nRbHwbpMcUaLES8vxDnh'
const DESIGN = `${R}/.port/phase-2/ZF_DESIGN.md`

const rules = `
Full-fidelity Zig->Rust port (rustty). This ports the VENDORED dep 'zf' (natecraddock/zf 0.10.3) that ghostty uses for fuzzy-matching in cli/list_themes.zig (zf.rank sorts the theme picker). Source (read-only): ${ZF}/src/zf/*.zig (zf.zig 152 LOC/3 test, filter.zig 460/4, clib.zig 182/2). Target: a NEW crate ${R}/crates/rustty-zf (named rustty-* per R4; a pure-Rust leaf, no rustty deps; the vendored-dep-port pattern like rustty-z2d/rustty-wuffs).
R1: EVERY line/comment/test{} ported assertion-for-assertion; Zig byte-slices -> &[u8]/Vec<u8>, NEVER lossy String (zf matches over &[u8]). The fuzzy-RANK ALGORITHM must be BIT-FAITHFUL — list_themes sorts by the f64 rank + relies on the exact match/no-match decision, so rank() must return identical values to zf (the 9 test{} pin exact ranks/highlights). R4 acyclic. R6: PURE Rust -> Miri primary (clib.zig's C-ABI is extern "C" -> Miri-excluded/ASan, but its logic is testable). Gates: build/test/clippy -D/fmt/check_acyclic.sh/check_test_counts.py/check_comment_coverage.py + Miri.
zf.zig API to expose: rank(str, tokens, RankOptions)->?f64, rankToken, highlight, highlightToken, RankOptions/RankTokenOptions. filter.zig: the core scoring (smith-waterman-ish + bonus tables). clib.zig: the C-ABI (name the exports rustty_zf_* per R2; ghostty_/zf_ alias deferred to #2).`

const D_SCHEMA = { type:'object', additionalProperties:false, required:['doc_path','disposition','summary','key_decisions','risks'], properties:{
  doc_path:{type:'string'}, disposition:{type:'string',description:"'ready' or 'blocked: <reason>'"}, summary:{type:'string'}, key_decisions:{type:'array',items:{type:'string'}}, risks:{type:'array',items:{type:'string'}} } }

phase('Design')
const design = await agent(
  `${rules}\n\nDESIGN PHASE. Write ${DESIGN} — a concise faithful-port design for rustty-zf. Read ALL of ${ZF}/src/zf/{zf,filter,clib}.zig first. Map each -> a rustty module (zf.rs/filter.rs/clib.rs + lib.rs); pin the scoring algorithm bit-faithfully (the bonus/gap tables, the f64 arithmetic order, the case-sensitivity/strict-path options, the tokenization); decide the C-ABI export names (rustty_zf_*); note any Zig-ism (comptime, ArrayList, std.mem) -> Rust mapping. The 9 test{} (esp. filter.zig's rank assertions) are the fidelity contract. If a genuine blocker, disposition 'blocked' + STOP. Else 'ready'. Return the object.`,
  { label:'design:zf', phase:'Design', agentType:'general-purpose', schema:D_SCHEMA })

if (!design || (design.disposition||'').toLowerCase().startsWith('blocked')) {
  log(`ZF DESIGN BLOCKED: ${design ? design.disposition : 'design failed'}`)
  return { blocked:true, design }
}

phase('Port')
const P_SCHEMA = { type:'object', additionalProperties:false, required:['disposition','files_changed','build_ok','tests_ported','miri_clean','notes'], properties:{
  disposition:{type:'string'}, files_changed:{type:'array',items:{type:'string'}}, build_ok:{type:'boolean'}, tests_ported:{type:'number'}, miri_clean:{type:'boolean'}, notes:{type:'string'} } }
const port = await agent(
  `${rules}\n\nPORT PHASE. Implement per ${DESIGN} — if blocked, STOP. Create crates/rustty-zf (Cargo.toml + add to workspace) and port zf.zig/filter.zig/clib.zig (module-per-file), preserving every comment (R1) and porting all 9 test{} assertion-for-assertion (the exact rank f64 values + highlight index arrays). Set 'PORT STATUS: done'. Build \`cargo build -p rustty-zf\`, run \`cargo test -p rustty-zf\` (all 9 pass with identical ranks), and \`cargo +nightly miri test -p rustty-zf\`. Report files_changed, build_ok, tests_ported, miri_clean, notes (incl. the rank-algorithm fidelity approach + any f64-ordering care). Do NOT git commit.`,
  { label:'port:zf', phase:'Port', agentType:'general-purpose', schema:P_SCHEMA })

if (!port || (port.disposition||'').toLowerCase().startsWith('stopped')) {
  log(`ZF PORT STOPPED`)
  return { design, port, verify:null }
}

phase('Verify')
const V_SCHEMA = { type:'object', additionalProperties:false, required:['dimension','verdict','findings'], properties:{
  dimension:{type:'string'}, verdict:{enum:['PASS','FAIL']}, findings:{type:'array',items:{type:'string'}} } }
const verify = await parallel([
  () => agent(`${rules}\n\nVERIFY (GATES+MIRI). Independently re-run + confirm GREEN: cargo build --workspace; cargo test -p rustty-zf; cargo +nightly miri test -p rustty-zf; cargo clippy -p rustty-zf --all-targets -- -D warnings; cargo fmt --check; bash .port/tools/gates/check_acyclic.sh; python3 .port/tools/gates/check_test_counts.py; python3 .port/tools/gates/check_comment_coverage.py. Report each. verdict PASS only if ALL green.`,
    { label:'verify:gates', phase:'Verify', agentType:'Explore', schema:V_SCHEMA }),
  () => agent(`${rules}\n\nVERIFY (R1 FIDELITY). Diff rustty-zf vs ${ZF}/src/zf/*.zig: (1) every comment verbatim? (2) all 9 test{} ported assertion-for-assertion with the EXACT rank f64 values + highlight index arrays (a differential fuzzy matcher: any scoring divergence flips theme-picker order)? (3) the scoring algorithm bit-faithful — bonus/gap tables, arithmetic order, case/strict options, tokenization all exact? (4) no lossy String (matches over &[u8])? (5) C-ABI exports rustty_zf_* (alias deferred)? R4 acyclic? Report findings; verdict PASS only if rank output is provably identical + R1 intact.`,
    { label:'verify:fidelity', phase:'Verify', agentType:'Explore', schema:V_SCHEMA }),
])

return { design, port, verify }
