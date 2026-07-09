export const meta = {
  name: 'vaxis-c1a-valuetypes',
  description: 'vaxis chunk 1a — port the vaxis value types (Cell/Style/Color, Key, Mouse, GraphemeCache, Winsize) into rustty-vaxis; gwidth/unicode DEFERRED (uucode-0.1.0 blocker): design -> port -> verify(+Miri)',
  phases: [
    { title: 'Design', detail: 'faithful-port design for the value types (Image->struct+R5 seams); STOP-guard' },
    { title: 'Port', detail: 'port the value types + tests into rustty-vaxis' },
    { title: 'Verify', detail: 'gates + R1 fidelity + Miri' },
  ],
}

const R = '/Users/gael/projects/rustty'
const VX = '/Users/gael/.cache/zig/p/vaxis-0.5.1-BWNV_LosCQAGmCCNOLljCIw6j6-yt53tji6n6rwJ2BhS'
const DESIGN = `${R}/.port/phase-2/VAXIS_C1_DESIGN.md`

const rules = `
Full-fidelity Zig->Rust port (rustty). This ports the VENDORED dep 'vaxis' (rockorager/libvaxis 0.5.1) used by ghostty's interactive CLI. Source (read-only): ${VX}/src/*.zig. Target: a NEW crate ${R}/crates/rustty-vaxis (rustty-* per R4; vendored-dep-port pattern like rustty-z2d). This is CHUNK 1a of a multi-chunk vaxis port — the VALUE TYPES only.
CHUNK-1a SCOPE (port faithfully): Cell.zig (221 LOC — Style, Color[+Color.Scheme], Segment, the Cell struct + its packed SGR bits), Key.zig (458, 5 test — Key + all key constants + matches/matchExact + the Modifiers/KittyFlags packed structs), Mouse.zig (50), GraphemeCache.zig (20), and the Winsize struct (main.zig:38). ~5 test{} (Key 5).
DEFERRED THIS CHUNK (do NOT port): gwidth.zig + unicode.zig — BLOCKED on a uucode-0.1.0 width-table decision (vaxis pins uucode 0.1.0-vanilla for east_asian_width/general_category/is_emoji_presentation; rustty-unicode has 0.2.0+ghostty-stripped-config, missing those raw fields). Recorded in VAXIS_C1_DESIGN.md + DEFERRALS.md. Any Cell/Key method that NEEDS gwidth (e.g. Cell width via grapheme width) becomes an R5 unimplemented!() seam + verbatim body this chunk.
Image coupling: Cell embeds Image (Image.zig deps external zigimg, unused by the CLI) — port the Image STRUCT SHAPE faithfully; zigimg-dependent method bodies = R5 unimplemented!() seams + verbatim body. Methods needing later-chunk types (Window/Screen) = R5 seams too.
DEPS: prefer NO rustty deps for the value types (Style/Color/Key/Mouse/GraphemeCache are self-contained); if GraphemeCache/Cell genuinely need a unicode primitive, use rustty-unicode (leaf, acyclic) — but do NOT pull gwidth in.
R1: EVERY line/comment/test{} ported assertion-for-assertion; Zig byte-slices -> &[u8]/Vec<u8>, NEVER lossy String (Key.text/Segment.text/grapheme are bytes). Packed structs (Modifiers u3, KittyFlags u5, Style SGR bits, Color/Kind unions) reproduce Zig bit layout + bitcast-eql semantics EXACTLY (the Key matchers rely on std.meta.eql over packed layouts). R4 acyclic. R6: PURE -> Miri primary. Gates: build/test/clippy -D/fmt/check_acyclic.sh/check_test_counts.py/check_comment_coverage.py + Miri.`

const D_SCHEMA = { type:'object', additionalProperties:false, required:['doc_path','disposition','summary','key_decisions','risks'], properties:{
  doc_path:{type:'string'}, disposition:{type:'string',description:"'ready' or 'blocked: <reason>'"}, summary:{type:'string'}, key_decisions:{type:'array',items:{type:'string'}}, risks:{type:'array',items:{type:'string'}} } }

phase('Design')
const design = await agent(
  `${rules}\n\nDESIGN PHASE. Update/write ${DESIGN} for CHUNK 1a (value types; gwidth/unicode explicitly deferred with the uucode-0.1.0 blocker recorded). Read ${VX}/src/{Cell,Key,Mouse,GraphemeCache}.zig + main.zig Winsize. Map each -> a rustty module; pin the packed-struct bit layouts + bitcast-eql (Modifiers/KittyFlags/Style SGR/Color union) so Key matches/matchExact are faithful; decide the Image struct-shape + R5 zigimg seams; pin deps (prefer none). Add a DEFERRALS.md entry for gwidth/unicode (the uucode-0.1.0 vs 0.2.0 skew; Option A extend rustty-unicode vs Option B vendor uucode-0.1.0 tables — human decision pending). Disposition 'ready' for the value-types subset (it is unblocked). Return the object.`,
  { label:'design:vaxis-c1a', phase:'Design', agentType:'general-purpose', schema:D_SCHEMA })

if (!design || (design.disposition||'').toLowerCase().startsWith('blocked')) {
  log(`VAXIS-C1a DESIGN BLOCKED: ${design ? design.disposition : 'design failed'}`)
  return { blocked:true, design }
}

phase('Port')
const P_SCHEMA = { type:'object', additionalProperties:false, required:['disposition','files_changed','build_ok','tests_ported','miri_clean','notes'], properties:{
  disposition:{type:'string'}, files_changed:{type:'array',items:{type:'string'}}, build_ok:{type:'boolean'}, tests_ported:{type:'number'}, miri_clean:{type:'boolean'}, notes:{type:'string'} } }
const port = await agent(
  `${rules}\n\nPORT PHASE. Implement per ${DESIGN} — if blocked, STOP. Create crates/rustty-vaxis (Cargo.toml + workspace) and port the chunk-1a value types (module-per-file: cell.rs[Style/Color/Segment/Cell], key.rs, mouse.rs, grapheme_cache.rs + lib.rs with Winsize + the image.rs struct-shape+R5 seams), preserving every comment (R1) and porting the 5 Key test{} assertion-for-assertion. gwidth/unicode NOT ported (deferred); Cell methods needing gwidth = R5 seams. Set 'PORT STATUS: done (chunk 1a — value types; gwidth/unicode + rendering/tty/loop are later chunks)'. Build \`cargo build -p rustty-vaxis\`, \`cargo test -p rustty-vaxis\`, \`cargo +nightly miri test -p rustty-vaxis\`. Report files_changed, build_ok, tests_ported, miri_clean, notes (incl. which methods are R5 seams + the packed-struct eql approach). Do NOT git commit.`,
  { label:'port:vaxis-c1a', phase:'Port', agentType:'general-purpose', schema:P_SCHEMA })

if (!port || (port.disposition||'').toLowerCase().startsWith('stopped')) {
  log(`VAXIS-C1a PORT STOPPED`)
  return { design, port, verify:null }
}

phase('Verify')
const V_SCHEMA = { type:'object', additionalProperties:false, required:['dimension','verdict','findings'], properties:{
  dimension:{type:'string'}, verdict:{enum:['PASS','FAIL']}, findings:{type:'array',items:{type:'string'}} } }
const verify = await parallel([
  () => agent(`${rules}\n\nVERIFY (GATES+MIRI). Independently re-run + confirm GREEN: cargo build --workspace; cargo test -p rustty-vaxis; cargo +nightly miri test -p rustty-vaxis; cargo clippy -p rustty-vaxis --all-targets -- -D warnings; cargo fmt --check; bash .port/tools/gates/check_acyclic.sh; python3 .port/tools/gates/check_test_counts.py; python3 .port/tools/gates/check_comment_coverage.py. Report each. verdict PASS only if ALL green.`,
    { label:'verify:gates', phase:'Verify', agentType:'Explore', schema:V_SCHEMA }),
  () => agent(`${rules}\n\nVERIFY (R1 FIDELITY). Diff rustty-vaxis chunk-1a modules vs ${VX}/src/{Cell,Key,Mouse,GraphemeCache}.zig + main.zig Winsize: (1) every comment verbatim? (2) the 5 Key test{} ported assertion-for-assertion? (3) packed structs (Modifiers/KittyFlags/Style SGR/Color union) reproduce the Zig bit layout + bitcast-eql so Key matches/matchExact are correct (spot-check a matcher)? (4) Style/Color faithful (Color.Scheme, the SGR flag bits)? (5) Image seams + gwidth-needing methods are R5 (unimplemented!() + verbatim), not fake logic? gwidth/unicode NOT ported? (6) no lossy String? R4 acyclic? Report findings; verdict PASS only if R1 intact.`,
    { label:'verify:fidelity', phase:'Verify', agentType:'Explore', schema:V_SCHEMA }),
])

return { design, port, verify }
