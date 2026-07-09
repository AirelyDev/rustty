export const meta = {
  name: 'vaxis-c1b-uucode-gwidth',
  description: 'vaxis chunk 1b — vendor the uucode-0.1.0 subset (get/types/grapheme/utf8 + EAW/GC/emoji/grapheme-break tables) as a rustty-vaxis module + port gwidth/unicode (11 gwidth tests pin exact widths): design -> adversary -> fold -> port -> verify(+Miri)',
  phases: [
    { title: 'Design', detail: 'resolve the uucode table-sourcing (build.rs-from-UCD vs vendored tables) + the runtime subset; STOP-guard if intractable' },
    { title: 'Adversary', detail: 'attack the table fidelity + Unicode-version correctness' },
    { title: 'Fold', detail: 'fold fixes' },
    { title: 'Port', detail: 'A: uucode subset+tables; B: gwidth+unicode (sequential)' },
    { title: 'Verify', detail: 'gates + R1 fidelity (11 gwidth widths exact) + Miri' },
  ],
}

const R = '/Users/gael/projects/rustty'
const VX = '/Users/gael/.cache/zig/p/vaxis-0.5.1-BWNV_LosCQAGmCCNOLljCIw6j6-yt53tji6n6rwJ2BhS'
const UU = '/Users/gael/.cache/zig/p/uucode-0.1.0-ZZjBPj96QADXyt5sqwBJUnhaDYs_qBeeKijZvlRa0eqM'
const DESIGN = `${R}/.port/phase-2/VAXIS_C1B_DESIGN.md`

const rules = `
Full-fidelity Zig->Rust port (rustty). vaxis chunk 1b: the UNICODE-DATA FOUNDATION for the vaxis port. Sources (read-only): vaxis 0.5.1 at ${VX}/src/{gwidth,unicode}.zig; uucode 0.1.0-vanilla at ${UU}/src/*.zig + ${UU}/src/build/{Ucd,tables}.zig. Target: crates/rustty-vaxis (chunk 1a value types already landed; PORT STATUS done). uucode DECISION = Option B: vendor uucode 0.1.0 as a MODULE INSIDE rustty-vaxis (crates/rustty-vaxis/src/uucode/) — NOT rustty-unicode (its 0.2.0+ghostty-stripped config lacks the raw fields + would introduce version skew).
SCOPE:
- uucode SUBSET vaxis uses (port faithfully): runtime get.zig (the get(field,cp) 2-stage stage1/stage2 table lookup), types.zig (the enums vaxis needs: EastAsianWidth u3, GeneralCategory u5, GraphemeBreak u5 + whatever grapheme.zig needs), grapheme.zig (the grapheme-break Iterator, UAX#29), utf8.zig (the utf8 decode Iterator). Only the properties vaxis reads: east_asian_width, general_category, is_emoji_presentation, grapheme_break.
- The TABLES (the crux): uucode generates stage1/stage2 tables at build-time from UCD text files via build/Ucd.zig (parser) + build/tables.zig (packer). Reproduce them BIT-FAITHFULLY. PREFER: a build.rs that parses VENDORED UCD .txt files (vendor them in-tree per R3, at uucode 0.1.0's exact Unicode version — read Ucd.zig for the version/filenames) and builds the same stage1/stage2 tables (port the packing logic). FALLBACK (if the generator is too involved): vendor the pre-generated tables as Rust static arrays (still vaxis's exact data — but you must GENERATE them faithfully; note no zig toolchain is assumed here, so build.rs-from-UCD is likely the tractable path). Either way the tables MUST reproduce the exact values gwidth's 11 tests + unicode's grapheme results pin.
- gwidth.zig (216 LOC, 11 test): the Method enum + eawToWidth + gwidth/gwidthUnicode over the uucode subset. unicode.zig (64 LOC): the grapheme iteration wrapper.
R1: EVERY line/comment/test{} ported; the 11 gwidth tests pin EXACT widths (any divergence = silently-wrong TUI layout). Byte-slices -> &[u8], never lossy String. R4 acyclic (rustty-vaxis stays a leaf or deps only leaves). R6: PURE -> Miri primary. Gates: build/test/clippy -D/fmt/check_acyclic.sh/check_test_counts.py/check_comment_coverage.py + Miri.`

const D_SCHEMA = { type:'object', additionalProperties:false, required:['doc_path','disposition','summary','table_approach','key_decisions','risks'], properties:{
  doc_path:{type:'string'}, disposition:{type:'string',description:"'ready' or 'blocked: <reason — esp. if the UCD data / table generation cannot be made faithful+tractable without a zig toolchain>'"},
  summary:{type:'string'}, table_approach:{type:'string',description:'exactly how the stage1/stage2 tables are produced bit-faithfully (build.rs-from-vendored-UCD with the packing logic, or vendored pre-generated arrays) + the Unicode version + which UCD files are vendored'},
  key_decisions:{type:'array',items:{type:'string'}}, risks:{type:'array',items:{type:'string'}} } }

phase('Design')
const design = await agent(
  `${rules}\n\nDESIGN PHASE. Write ${DESIGN}. Read ${UU}/src/{root,get,types,grapheme,utf8,config}.zig + ${UU}/src/build/{Ucd,tables}.zig (understand the stage1/stage2 table structure + the get() lookup + which UCD files+Unicode-version Ucd.zig parses) + ${VX}/src/{gwidth,unicode}.zig. RESOLVE table_approach: determine uucode 0.1.0's exact Unicode version + the UCD .txt files it parses (UnicodeData/EastAsianWidth/DerivedCoreProperties/emoji-data/GraphemeBreakProperty/etc.), and specify EITHER a build.rs that vendors those UCD files + reproduces the stage1/stage2 packing (PREFERRED — auditable, no zig), OR vendored pre-generated arrays (say exactly how you'd generate them faithfully). Map the runtime subset -> rustty modules (crates/rustty-vaxis/src/uucode/{types,get,grapheme,utf8,tables}.rs) + gwidth.rs + unicode.rs. Confirm the 11 gwidth tests' codepoints resolve correctly under the chosen tables. If the table generation genuinely cannot be made faithful+tractable in this env, disposition 'blocked: <precise reason>' + STOP. Else 'ready'. Return the object.`,
  { label:'design:vaxis-c1b', phase:'Design', agentType:'general-purpose', schema:D_SCHEMA })

if (!design || (design.disposition||'').toLowerCase().startsWith('blocked')) {
  log(`VAXIS-C1b DESIGN BLOCKED: ${design ? design.disposition : 'design failed'}`)
  return { blocked:true, design }
}

phase('Adversary')
const ADV_SCHEMA = { type:'object', additionalProperties:false, required:['verdict','fixes','blocking_issues'], properties:{
  verdict:{enum:['SOUND','SOUND_WITH_FIXES','UNSOUND']}, fixes:{type:'array',items:{type:'string'}}, blocking_issues:{type:'array',items:{type:'string'}} } }
const adversary = await agent(
  `${rules}\n\nADVERSARY PHASE. Design at ${DESIGN}; table_approach: "${design.table_approach}". BREAK it on TABLE FIDELITY (the crux): (1) is the Unicode version EXACTLY uucode 0.1.0's (read Ucd.zig — a wrong version silently changes EAW/GC for some codepoints)? are the vendored UCD files the right ones + right version? (2) does the stage1/stage2 packing reproduce uucode's tables.zig BIT-EXACT (block size, the default/fallback values, the property-to-int encoding matching types.zig enum values)? (3) does eawToWidth + gwidth match vaxis's algorithm exactly (the ambiguous-width, emoji-presentation, zero-width, control-char cases)? (4) do ALL 11 gwidth test codepoints resolve to the pinned widths under the design's tables (walk a few by hand: wide CJK, ambiguous, emoji, combining, control)? (5) grapheme.zig UAX#29 boundary rules faithful? (6) is the build.rs deterministic + does the vendored UCD stay in-tree (R3)? (7) any lossy String? R4 acyclic? List every fix + blocker. verdict SOUND only if the tables are provably bit-exact.`,
  { label:'adversary:vaxis-c1b', phase:'Adversary', agentType:'Explore', schema:ADV_SCHEMA })

const needsFold = adversary && ((adversary.fixes && adversary.fixes.length) || (adversary.blocking_issues && adversary.blocking_issues.length))
if (needsFold) {
  phase('Fold')
  await agent(
    `${rules}\n\nFOLD PHASE. Adversary issues on ${DESIGN}. Fixes: ${JSON.stringify(adversary.fixes)}. Blocking: ${JSON.stringify(adversary.blocking_issues)}. Update ${DESIGN} IN PLACE to fix every one (re-read uucode/vaxis/UCD as needed). If a blocker (e.g. UCD version unresolvable / tables not reproducible) is genuine, add a top 'DISPOSITION: blocked' line. Return a short summary.`,
    { label:'fold:vaxis-c1b', phase:'Fold', agentType:'general-purpose' })
}

phase('Port')
const P_SCHEMA = { type:'object', additionalProperties:false, required:['disposition','files_changed','build_ok','tests_ported','miri_clean','notes'], properties:{
  disposition:{type:'string'}, files_changed:{type:'array',items:{type:'string'}}, build_ok:{type:'boolean'}, tests_ported:{type:'number'}, miri_clean:{type:'boolean'}, notes:{type:'string'} } }
const portA = await agent(
  `${rules}\n\nPORT PHASE A (uucode subset + tables). Implement per ${DESIGN} — if 'DISPOSITION: blocked', STOP. Create crates/rustty-vaxis/src/uucode/{types,get,grapheme,utf8,tables}.rs (+ mod wiring), vendor the UCD .txt files in-tree (per R3) + the build.rs (or vendored tables) producing the stage1/stage2 tables BIT-FAITHFULLY, preserving every comment (R1). Port uucode's own relevant test{} if any. Build \`cargo build -p rustty-vaxis\` (the build.rs must run) + a quick table spot-check test (a few codepoints' EAW/GC match uucode). Report files_changed, build_ok, notes. Do NOT git commit.`,
  { label:'port:vaxis-c1b-A', phase:'Port', agentType:'general-purpose', schema:P_SCHEMA })

let portB = null
if (portA && !(portA.disposition||'').toLowerCase().startsWith('stopped')) {
  portB = await agent(
    `${rules}\n\nPORT PHASE B (gwidth + unicode). Port A landed the uucode subset. Port ${VX}/src/gwidth.zig -> crates/rustty-vaxis/src/gwidth.rs (Method enum, eawToWidth, gwidth/gwidthUnicode over the uucode subset) + ${VX}/src/unicode.zig -> unicode.rs, preserving every comment (R1) and porting the 11 gwidth test{} assertion-for-assertion (EXACT widths). Update lib.rs PORT STATUS. Build + \`cargo test -p rustty-vaxis\` (all pass incl. the 11 gwidth) + \`cargo +nightly miri test -p rustty-vaxis\`. Report files_changed, build_ok, tests_ported, miri_clean, notes. Do NOT git commit.`,
    { label:'port:vaxis-c1b-B', phase:'Port', agentType:'general-purpose', schema:P_SCHEMA })
}

const stopped = (p) => !p || (p.disposition||'').toLowerCase().startsWith('stopped')
if (stopped(portA) || stopped(portB)) {
  log(`VAXIS-C1b PORT INCOMPLETE: A=${portA&&portA.disposition} B=${portB&&portB.disposition}`)
  return { design, adversary, portA, portB, verify:null }
}

phase('Verify')
const V_SCHEMA = { type:'object', additionalProperties:false, required:['dimension','verdict','findings'], properties:{
  dimension:{type:'string'}, verdict:{enum:['PASS','FAIL']}, findings:{type:'array',items:{type:'string'}} } }
const verify = await parallel([
  () => agent(`${rules}\n\nVERIFY (GATES+MIRI). Independently re-run + confirm GREEN: cargo build --workspace; cargo test -p rustty-vaxis; cargo +nightly miri test -p rustty-vaxis; cargo clippy -p rustty-vaxis --all-targets -- -D warnings; cargo fmt --check; bash .port/tools/gates/check_acyclic.sh; python3 .port/tools/gates/check_test_counts.py; python3 .port/tools/gates/check_comment_coverage.py. Report each. verdict PASS only if ALL green (esp. the 11 gwidth tests).`,
    { label:'verify:gates', phase:'Verify', agentType:'Explore', schema:V_SCHEMA }),
  () => agent(`${rules}\n\nVERIFY (R1 + TABLE FIDELITY). (1) The 11 gwidth tests pass with EXACT widths (spot-check 3-4 codepoints by hand against vaxis: a wide CJK, an ambiguous, an emoji, a combining/zero-width). (2) The stage1/stage2 tables reproduce uucode 0.1.0 bit-exact (right Unicode version, right UCD files, right packing/encoding). (3) gwidth eawToWidth + grapheme.zig UAX#29 faithful (every comment, every branch). (4) unicode.zig grapheme iteration faithful. (5) UCD vendored in-tree (R3), build.rs deterministic. (6) no lossy String; R4 acyclic. Report findings; verdict PASS only if the widths are provably exact + R1 intact.`,
    { label:'verify:fidelity', phase:'Verify', agentType:'Explore', schema:V_SCHEMA }),
])

return { design, adversary, portA, portB, verify }
