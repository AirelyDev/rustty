export const meta = {
  name: 'extra-generators',
  description: 'Sub-project #3 — port ghostty src/extra/** (bash/fish/zsh/vim/sublime shell-completion + editor-syntax generators, 941 LOC, 2 test{}) reusing the config/Action reflection: design -> port -> verify',
  phases: [
    { title: 'Design', detail: 'map Zig comptime @typeInfo(Config/Action) reflection -> rustty Key/Action surface; STOP-guard if inadequate' },
    { title: 'Port', detail: 'port the 5 generators + tests, byte-exact output' },
    { title: 'Verify', detail: 'gates + R1 fidelity (byte-exact generated scripts) + Miri' },
  ],
}

const R = '/Users/gael/projects/rustty'
const G = '/Users/gael/projects/ghostty'
const DESIGN = `${R}/.port/phase-2/EXTRA_DESIGN.md`

const rules = `
Full-fidelity Zig->Rust port (rustty). Reference (read-only): ghostty at ${G}. Target: ${R}.
SCOPE (ghostty src/extra/): bash.zig (337 LOC), fish.zig (202,1 test), zsh.zig (231), vim.zig (121,1 test), sublime.zig (50). These GENERATE shell-completion (bash/fish/zsh) + editor-syntax (vim/sublime) files by COMPTIME-reflecting over Config fields (@typeInfo(Config).@"struct".fields) and the CLI Action enum (@typeInfo(Action).@"enum".fields + @field(Action,name).options()). Driven by main_build_data.zig's +gen-* actions (the bin entry stays deferred).
THE CRUX: rustty must reproduce the SAME field/action iteration WITHOUT Zig comptime. rustty ALREADY reifies this: crate::config_struct::Key (an enum with one variant per Config field, + string_to_enum), and rustty-helpgen already does genConfigField/genActions reflection over Config/Action/KeybindAction (crates/rustty-helpgen/src/lib.rs) — REUSE that surface (field names in declaration order, per-field type classification: bool/enum-with-values/path/int/etc., and the Action variants + their options structs). The generated completion scripts must be BYTE-IDENTICAL to ghostty's (the 2 test{} in fish.zig/vim.zig assert generated output).
WHERE TO LAND: a new crate rustty-extra (or a module in rustty-helpgen/rustty-cli — the design decides the R4-acyclic home; it deps config + the Action/help reflection, so it sits at/above helpgen). rustty-args/rustty-config/rustty-helpgen are candidates for deps.
R1: EVERY line/comment/test{} ported; byte-slices -> &[u8]/Vec<u8>/CString, NEVER lossy String (completion scripts are bytes). R4 acyclic. R6 pure->Miri. Gates: build/test/clippy -D/fmt/check_acyclic.sh/check_test_counts.py/check_comment_coverage.py + Miri.`

const D_SCHEMA = { type:'object', additionalProperties:false, required:['doc_path','disposition','summary','reflection_plan','key_decisions','risks'], properties:{
  doc_path:{type:'string'}, disposition:{type:'string',description:"'ready' or 'blocked: <precise reason — esp. if the config/Action reflection surface cannot reproduce the byte-exact output>'"},
  summary:{type:'string'}, reflection_plan:{type:'string',description:'exactly how rustty reproduces Zig comptime @typeInfo(Config/Action) field iteration + per-field type classification via the reified Key/Action + helpgen surface'},
  key_decisions:{type:'array',items:{type:'string'}}, risks:{type:'array',items:{type:'string'}} } }

phase('Design')
const design = await agent(
  `${rules}\n\nDESIGN PHASE. Write ${DESIGN}. Read ALL of ${G}/src/extra/*.zig (esp. how bash.zig/zsh.zig classify each Config field's type to emit completions, and how they walk Action + options), then read crates/rustty-config/src/config_struct.rs (the Key enum), crates/rustty-helpgen/src/lib.rs (genConfigField/genActions), the rustty Action enum (find it: rustty-cli or rustty-args), and confirm the reflection surface EXPOSES enough (field names in declaration order + per-field type kind + enum value lists + Action variants + their option fields) to reproduce byte-exact output. Fill reflection_plan with the exact mapping. Pin the crate home (R4-acyclic). If the reflection surface CANNOT reproduce byte-exact completions (e.g. per-field type kind not exposed), set disposition 'blocked: <what's missing>' + STOP (do NOT hand-fake it). Else 'ready'. Return the object.`,
  { label:'design:extra', phase:'Design', agentType:'general-purpose', schema:D_SCHEMA })

if (!design || (design.disposition||'').toLowerCase().startsWith('blocked')) {
  log(`EXTRA DESIGN BLOCKED: ${design ? design.disposition : 'design failed'}`)
  return { blocked:true, design }
}

phase('Port')
const P_SCHEMA = { type:'object', additionalProperties:false, required:['disposition','files_changed','build_ok','tests_ported','miri_clean','notes'], properties:{
  disposition:{type:'string'}, files_changed:{type:'array',items:{type:'string'}}, build_ok:{type:'boolean'}, tests_ported:{type:'number'}, miri_clean:{type:'boolean'}, notes:{type:'string'} } }
const port = await agent(
  `${rules}\n\nPORT PHASE. Implement per ${DESIGN} — if blocked, STOP. Port the 5 generators (bash/fish/zsh/vim/sublime) into the crate/module the design chose, reusing the config/Action/helpgen reflection surface (reflection_plan: ${design.reflection_plan}). Preserve every comment (R1); the generated completion/syntax output must be BYTE-IDENTICAL — port both test{} (fish, vim) assertion-for-assertion. Wire it + set 'PORT STATUS: done'. Build, \`cargo test\`, \`cargo +nightly miri test\` on the crate. Report files_changed, build_ok, tests_ported, miri_clean, notes (incl. the reflection reuse + whether a config-reflection accessor had to be added). Do NOT git commit.`,
  { label:'port:extra', phase:'Port', agentType:'general-purpose', schema:P_SCHEMA })

if (!port || (port.disposition||'').toLowerCase().startsWith('stopped')) {
  log(`EXTRA PORT STOPPED`)
  return { design, port, verify:null }
}

phase('Verify')
const V_SCHEMA = { type:'object', additionalProperties:false, required:['dimension','verdict','findings'], properties:{
  dimension:{type:'string'}, verdict:{enum:['PASS','FAIL']}, findings:{type:'array',items:{type:'string'}} } }
const verify = await parallel([
  () => agent(`${rules}\n\nVERIFY (GATES+MIRI). Independently re-run + confirm GREEN: cargo build --workspace; cargo test (the extra crate); cargo +nightly miri test (the extra crate); cargo clippy (the extra crate) --all-targets -- -D warnings; cargo fmt --check; bash .port/tools/gates/check_acyclic.sh; python3 .port/tools/gates/check_test_counts.py; python3 .port/tools/gates/check_comment_coverage.py. Report each. verdict PASS only if ALL green.`,
    { label:'verify:gates', phase:'Verify', agentType:'Explore', schema:V_SCHEMA }),
  () => agent(`${rules}\n\nVERIFY (R1 FIDELITY + BYTE-EXACT). Diff each ported generator vs its ghostty src/extra/*.zig: (1) every comment verbatim? (2) both test{} (fish, vim) ported assertion-for-assertion? (3) THE GENERATED OUTPUT byte-identical — does the rustty field/Action iteration produce the EXACT same completion/syntax text (same field order, same per-field completion values, same escaping)? Spot-check a few Config fields + Action variants against the Zig output. (4) no lossy String (scripts are bytes)? (5) R4 acyclic? Report findings; verdict PASS only if the output is byte-exact + R1 intact.`,
    { label:'verify:fidelity', phase:'Verify', agentType:'Explore', schema:V_SCHEMA }),
])

return { design, port, verify }
