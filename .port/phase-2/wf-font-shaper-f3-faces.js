export const meta = {
  name: 'font-shaper-f3-faces',
  description: 'F3: the font faces/discovery layer of a NEW rustty-font-shaper crate (ABOVE vt+font per D1). face.zig + face/freetype (freetype-sys+harfbuzz-sys) + face/coretext (objc2 core-text/graphics/foundation) + Collection + DeferredFace + CodepointResolver + discovery (coretext macOS; fontconfig deferred). ~46 tests. -sys foundation DONE.',
  phases: [
    { title: 'Scaffold', detail: 'create rustty-font-shaper crate + face.zig backend selector (3t)' },
    { title: 'FreetypeFace', detail: 'face/freetype.zig (9t) — freetype-sys face + hb_font' },
    { title: 'CoretextFace', detail: 'face/coretext.zig (8t) — CoreText via objc2' },
    { title: 'Collection', detail: 'Collection(13t) + DeferredFace(2t) + CodepointResolver(3t)' },
    { title: 'Discovery', detail: 'discovery.zig (8t) — coretext (macOS); fontconfig deferred' },
    { title: 'Verify', detail: 'count-gate + tests + ASan + G3b + faithfulness' },
  ],
}
const G = '/Users/gael/projects/ghostty', R = '/Users/gael/projects/rustty'
const GFF = G + '/src/font'
const GATES = R + '/.port/tools/gates'
const RULES = 'OBEY: PARITY CONSTITUTION + ' + R + '/.port/PORT_RULES.md. R1 = PORT EVERY test{} (count is a CONTRACT); EVERY line/comment verbatim; assertion-for-assertion, INDEPENDENT expected values. R3 faithful; R5 NO weakening. Reference READ-ONLY ' + GFF + '/. This creates a NEW crate rustty-font-shaper (ABOVE vt+font — the D1 3-tier split, memory apc-glyf-font-layering-d1; rustty-font stays PURE below vt). '
  + 'DEPS READY: rustty-vt (2019t), rustty-font (F0 pure parsing+sprite, 70t), freetype-sys (FreeType 2.13.2, KAT-green), harfbuzz-sys (11.0.0, KAT-green — hb_ft bridging works), rustty-render-types. CoreText via the crates.io objc2 framework crates: objc2-foundation, objc2-core-graphics, objc2-core-text — these REPLACE ghostty pkg/macos (map macos.text.* -> objc2_core_text CTFont/CTFontDescriptor, macos.graphics.* -> objc2_core_graphics CGContext/CGBitmapContext, macos.foundation.* -> objc2_foundation CFString/CFArray) and are declared as macOS-only deps (a cfg target_os = macos dependency table). fontconfig discovery = Linux -> DEFER (document); web_canvas face = wasm -> feature-gate OFF. '
  + 'The backend matrix (font/backend.zig): macOS default=coretext, Linux default=fontconfig_freetype — cfg/feature matrix, parity targets both defaults. [:0]const u8->Vec<u8>. SOUNDNESS (memory): raw-ptr discipline across the C/objc FFI. MIRI: FFI (freetype/harfbuzz/CoreText) CANNOT run under Miri (R6) -> ASan-cover the FFI tests, Miri only pure-Rust helpers. Run gates FOREGROUND. Commit trailer "Co-Authored-By: Claude Opus 4.8 (1M context)". '
async function robust(p, o) { let r = await agent(p, o); if (!r) r = await agent(p, { ...o, label: (o.label || 'a') + '-retry' }); return r }
const STEP = { type: 'object', additionalProperties: true, required: ['ok', 'blockers'], properties: { ok: { type: 'boolean' }, tests_added: { type: 'integer' }, builds: { type: 'boolean' }, commit: { type: 'string' }, notes: { type: 'string' }, blockers: { type: 'array', items: { type: 'string' } } } }
const VERIFY = { type: 'object', additionalProperties: true, required: ['verdict', 'blockers'], properties: { verdict: { type: 'string', enum: ['GREEN', 'RED'] }, tests_passing: { type: 'integer' }, count_ok: { type: 'boolean' }, asan_clean: { type: 'boolean' }, g3b_missing: { type: 'integer' }, tautological_tests: { type: 'array', items: { type: 'string' } }, notes: { type: 'string' }, blockers: { type: 'array', items: { type: 'string' } } } }

phase('Scaffold')
const scaf = await robust(RULES + 'TASK (create rustty-font-shaper + backend selector). Create crates/rustty-font-shaper (add to workspace members; Cargo.toml deps: rustty-vt, rustty-font, rustty-render-types, rustty-util, freetype-sys, harfbuzz-sys; and objc2-foundation/objc2-core-graphics/objc2-core-text as macOS-only deps under a cfg target_os = macos dependency table). Port ' + GFF + '/face.zig (112loc, 3t — the backend selector: Face = the active backend Face type; Options/RenderOptions/Metrics re-exports) + wire lib.rs + the shared face types. GATES FOREGROUND: cargo build -p rustty-font-shaper; cargo test -p rustty-font-shaper (3 pass); clippy -p rustty-font-shaper --all-targets -D; fmt; check_acyclic.sh (font-shaper -> vt+font+sys, no cycle; rustty-font still NO vt edge). Commit "rustty: font-shaper F3 scaffold + face backend selector". Return ok+tests_added+builds+commit+blockers.', { schema: STEP, effort: 'high', label: 'scaffold', phase: 'Scaffold' })

async function fill(label, phaseName, prompt) {
  return robust(RULES + 'Scaffold: ' + JSON.stringify(scaf && scaf.commit) + '\n' + prompt
    + ' GATES FOREGROUND: cargo build -p rustty-font-shaper; cargo test -p rustty-font-shaper (new tests pass + no regression); ASan on the new FFI tests (Miri cannot run C/objc FFI — R6); clippy --all-targets -D; fmt; scoped G3b for the ported .zig missing=0. Commit. Return ok+tests_added+builds+commit+blockers.',
    { schema: STEP, effort: 'high', label, phase: phaseName })
}
phase('FreetypeFace')
const ft = await fill('freetype-face', 'FreetypeFace', 'TASK. Port ' + GFF + '/face/freetype.zig (1439loc, 9t — the FreeType face: FT_New_Memory_Face, glyphIndex, renderGlyph (FT_Load_Glyph + FT_Render_Glyph -> Atlas bitmap), metrics, an hb_font via harfbuzz-sys hb_ft_font_create) into rustty-font-shaper src/face/freetype.rs using freetype-sys + harfbuzz-sys. Commit "rustty: font-shaper freetype face + 9 tests".')
phase('CoretextFace')
const ct = await fill('coretext-face', 'CoretextFace', 'TASK. Port ' + GFF + '/face/coretext.zig (1162loc, 8t — the CoreText face: CTFont from a descriptor, glyphIndex, renderGlyph via CGBitmapContext -> Atlas, metrics) into rustty-font-shaper src/face/coretext.rs via objc2-core-text (CTFont/CTFontDescriptor), objc2-core-graphics (CGContext/CGColorSpace/CGBitmapContext), objc2-foundation (CFString/CFAttributedString). Map ghostty macos.text/graphics/foundation 1:1. Commit "rustty: font-shaper coretext face + 8 tests".')
phase('Collection')
const coll = await fill('collection', 'Collection', 'TASK. Port ' + GFF + '/{Collection.zig 1523/13t (the font Collection: add/getFace/getIndex, the style/size grid), DeferredFace.zig 530/2t (lazy face load), CodepointResolver.zig 562/3t (codepoint -> face resolution, fallback chain)} into rustty-font-shaper. Commit "rustty: font-shaper Collection + DeferredFace + CodepointResolver + 18 tests".')
phase('Discovery')
const disc = await fill('discovery', 'Discovery', 'TASK. Port ' + GFF + '/discovery.zig (1349loc, 8t — font discovery: the CoreText discovery on macOS via objc2-core-text CTFontDescriptor/CTFontCollection; the fontconfig arm is Linux -> DEFER + document; Descriptor/discover). Machine-dependent system-font-query tests -> pin/#[ignore], run the deterministic ones. Commit "rustty: font-shaper discovery (coretext; fontconfig deferred) + tests".')

phase('Verify')
const verify = await robust(RULES + 'Phases: ' + JSON.stringify([scaf, ft, ct, coll, disc].map(s => s && s.commit)) + '\n'
  + 'TASK (independent verify — default RED). Add rustty-font-shaper module mappings to ' + GATES + '/check_test_counts.py (GHF=src/font; face/freetype 9, face/coretext 8, face 3, Collection 13, DeferredFace 2, CodepointResolver 3, discovery 8 minus deferred). Confirm FOREGROUND: cargo build --workspace; `python3 ' + GATES + '/check_test_counts.py` => font-shaper modules rust>=zig (count_ok, deferrals documented) + WHOLE gate GREEN; cargo test -p rustty-font-shaper (~46 minus deferred pass) + no workspace regression (vt 2019, font 70); `bash ' + GATES + '/sanitizers.sh rustty-font-shaper "" address` (ASan clean); clippy --all-targets -D + fmt --check + ' + GATES + '/check_acyclic.sh (rustty-font STILL no vt edge). G3b the ported .zig missing=0 (modulo deferred fontconfig). Update NEXT_PHASE_PLAN (F3 faces done; F4 shaper next). Spot-check 3 (a freetype renderGlyph, a coretext renderGlyph, a Collection getFace) faithful. Verdict GREEN only if count_ok + build + tests + ASan + g3b=0(modulo defer) + clippy + fmt + acyclic AND no regression. Report verdict + tests_passing + count_ok + asan_clean + g3b_missing + tautological_tests + blockers.',
  { schema: VERIFY, effort: 'high', label: 'verify', phase: 'Verify' })
return { scaf, ft, ct, coll, disc, verify }
