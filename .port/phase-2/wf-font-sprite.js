export const meta = {
  name: 'font-sprite',
  description: 'Heavy-tier F0 sprite tier (font-pure, source-available, pixel-exact): the two z2d prerequisites (Surface::composite + stroke tessellation) then the ~6250-loc sprite glyph renderer (sprite.zig + common/canvas + draw/{braille,block,geometric_shapes,powerline,branch,special,box,symbols_for_legacy_computing,supplement} + Face.zig) into rustty-font, gated by the sprite/testdata PNG known-answer suite (36 refs). Draw modules are 0-test -> G3b comment-coverage per module + the final Face PNG suite are the gates.',
  phases: [
    { title: 'Z2dComposite', detail: 'z2d Surface::composite (alpha8+rgb, .src/.src_over) via run_pixel_t + sanity/Miri' },
    { title: 'Z2dStroke', detail: 'z2d stroke tessellation subset sprite uses (line/strokePath: stroke_plotter+Pen+polyline_plotter+hairline) + sanity/Miri' },
    { title: 'Scaffold', detail: 'sprite.zig(Sprite enum) + draw/common.zig + canvas.zig (composite+stroke now available) + unit tests' },
    { title: 'DrawA', detail: 'draw/{braille,block,geometric_shapes,powerline,branch,special} (the small-mid modules)' },
    { title: 'DrawBox', detail: 'draw/box.zig (929loc box-drawing)' },
    { title: 'DrawLegacy', detail: 'draw/{symbols_for_legacy_computing(1413),supplement(633)}' },
    { title: 'Face', detail: 'sprite/Face.zig (550, 2t) + the headline PNG known-answer suite (render all sprites, 36 testdata refs)' },
    { title: 'Verify', detail: 'count-gate + tests + PNG pixel-exact + Miri + G3b(all sprite/*.zig) + font_no_vt_edge' },
  ],
}
const G = '/Users/gael/projects/ghostty', R = '/Users/gael/projects/rustty'
const SP = G + '/src/font/sprite'
const Z2D = '/Users/gael/.cache/zig/p/z2d-0.10.0-j5P_Hu-6FgBsZNgwphIqh17jDnj8_yPtD8yzjO6PpHRQ'
const GATES = R + '/.port/tools/gates'
const RULES = 'OBEY: PARITY CONSTITUTION + ' + R + '/.port/PORT_RULES.md. R1 = PORT EVERY test{} (count is a CONTRACT) + full G3b comment coverage of every sprite/*.zig (the draw modules are 0-test — G3b is their gate); EVERY line/comment verbatim; PIXEL-EXACT for the PNG known-answer suite. R3 faithful; R5 NO weakening/stub-to-pass. Reference READ-ONLY ' + G + '/src/font/ + the z2d source at ' + Z2D + ' (READ-ONLY). rustty-z2d (pure Rust) has: Transformation/Path/Surface/pixel formats + the .none + multisample_4x AA fill rasterizer + composite_stride + the scalar compositor (glyf_rasterize PNG known-answer PASSES pixel-exact). rustty-font PURE & BELOW vt (memory apc-glyf-font-layering-d1). Plan: memory heavy-tier-strategy (F0 sprite). '
  + 'PREREQUISITES (z2d, still unimplemented!()): (1) Surface::composite (surface.rs:484) — port z2d surface.zig composite for the used operators/surfaces (alpha8 + rgb; .src + .src_over) via the ported compositor::run_pixel_t. (2) STROKE — port the z2d stroke tessellation SUBSET sprite uses (canvas line()/strokePath/innerStrokePath: stroke_plotter + Pen/Face/Slope + polyline_plotter + the hairline rasterizer; dashes may not be needed — scope to sprite usage). Both with z2d-internal sanity tests + isolated Miri. '
  + 'PNG known-answer: decode sprite/testdata/*.png refs (copy from ' + SP + '/testdata/ into rustty-font/res/, READ-ONLY) via the `png` dev-dependency + assert pixel-exact alpha8. [:0]const u8->Vec<u8>. SOUNDNESS (memory): raw-ptr discipline. MIRI per MIRI_EXCEPTIONS.md ISOLATED. PROCESS LESSONS (memory): per-subbatch scoped-G3b. Run gates FOREGROUND. Commit trailer "Co-Authored-By: Claude Opus 4.8 (1M context)". '
async function robust(p, o) { let r = await agent(p, o); if (!r) r = await agent(p, { ...o, label: (o.label || 'a') + '-retry' }); return r }
const STEP = { type: 'object', additionalProperties: true, required: ['ok', 'blockers'], properties: { ok: { type: 'boolean' }, tests_added: { type: 'integer' }, builds: { type: 'boolean' }, commit: { type: 'string' }, notes: { type: 'string' }, blockers: { type: 'array', items: { type: 'string' } } } }
const VERIFY = { type: 'object', additionalProperties: true, required: ['verdict', 'blockers'], properties: { verdict: { type: 'string', enum: ['GREEN', 'RED'] }, tests_passing: { type: 'integer' }, count_ok: { type: 'boolean' }, pixel_exact: { type: 'boolean' }, font_no_vt_edge: { type: 'boolean' }, miri_subset_clean: { type: 'boolean' }, g3b_missing: { type: 'integer' }, notes: { type: 'string' }, blockers: { type: 'array', items: { type: 'string' } } } }

phase('Z2dComposite')
const comp = await robust(RULES + 'TASK (z2d Surface::composite). Port z2d surface.zig `composite` (crates/rustty-z2d/src/surface.rs:484 unimplemented!()) for alpha8 + rgb surfaces, .src + .src_over operators, via the ported compositor::run_pixel_t. + z2d-internal sanity tests (composite two alpha8 surfaces .src / .src_over) + isolated Miri. GATES FOREGROUND: cargo build -p rustty-z2d; cargo test -p rustty-z2d (new sanity pass + no regression vs 51); Miri-subset 2 ISOLATED (0 UB/leak); clippy -D; fmt. Commit "rustty: z2d Surface::composite (alpha8/rgb, src/src_over)". Return ok+tests_added+builds+commit+blockers.', { schema: STEP, effort: 'high', label: 'z2d-composite', phase: 'Z2dComposite' })

phase('Z2dStroke')
const stroke = await robust(RULES + 'Composite: ' + JSON.stringify(comp && comp.commit) + '\nTASK (z2d stroke). Port the z2d stroke tessellation SUBSET the sprite canvas uses (line/strokePath/innerStrokePath -> painter::stroke; stroke_plotter + Pen/Face/Slope + polyline_plotter + hairline rasterizer). Scope to what sprite/canvas.zig actually calls (dashes likely unused). + z2d sanity tests (stroke a line, stroke a rect outline, hairline) + isolated Miri. GATES FOREGROUND: cargo build -p rustty-z2d; cargo test -p rustty-z2d (pass + no regression); Miri-subset 2 ISOLATED; clippy -D; fmt. Commit "rustty: z2d stroke tessellation (sprite subset)". Return ok+tests_added+builds+commit+blockers.', { schema: STEP, effort: 'high', label: 'z2d-stroke', phase: 'Z2dStroke' })

async function draw(label, phaseName, prompt) {
  return robust(RULES + 'z2d composite+stroke landed: ' + JSON.stringify([comp, stroke].map(s => s && s.commit)) + '\n' + prompt
    + ' GATES FOREGROUND: cargo build -p rustty-font; cargo test -p rustty-font (no regression); Miri-subset 1-2 ISOLATED if applicable; clippy --all-targets -D; fmt; scoped G3b for the ported sprite/*.zig files missing=0 (the draw modules are 0-test, so G3b is the gate). Commit. Return ok+tests_added+builds+commit+blockers.',
    { schema: STEP, effort: 'high', label, phase: phaseName })
}

phase('Scaffold')
const scaf = await draw('scaffold', 'Scaffold', 'TASK (sprite scaffold). Port ' + G + '/src/font/sprite.zig (the Sprite enum) + ' + SP + '/draw/common.zig (374loc, Canvas/Color/Alignment helpers) + ' + SP + '/canvas.zig (512loc — the drawing Canvas over z2d, now composite+stroke exist) into rustty-font (src/sprite/) + any inline unit tests + a small alpha8 assemble sanity test. Commit "rustty: font sprite scaffold (Sprite/common/canvas)".')
phase('DrawA')
const drawA = await draw('draw-a', 'DrawA', 'TASK (small-mid draw modules). Port ' + SP + '/draw/{braille.zig 148, block.zig 177, geometric_shapes.zig 198, powerline.zig 394, branch.zig 504, special.zig 372} into rustty-font src/sprite/draw/. Commit "rustty: font sprite draw braille/block/geometric/powerline/branch/special".')
phase('DrawBox')
const drawBox = await draw('draw-box', 'DrawBox', 'TASK (box-drawing). Port ' + SP + '/draw/box.zig (929loc — the box-drawing glyphs) into rustty-font src/sprite/draw/box.rs. Commit "rustty: font sprite draw box (box-drawing)".')
phase('DrawLegacy')
const drawLeg = await draw('draw-legacy', 'DrawLegacy', 'TASK (legacy-computing). Port ' + SP + '/draw/{symbols_for_legacy_computing.zig 1413, symbols_for_legacy_computing_supplement.zig 633} into rustty-font src/sprite/draw/. Commit "rustty: font sprite draw legacy-computing symbols".')

phase('Face')
const face = await draw('face', 'Face', 'TASK (Face + PNG suite). Port ' + SP + '/Face.zig (550loc, 2t — the sprite Face that composites draw modules into per-256 atlases) + the headline "sprite face render all sprites" PNG known-answer test (composite ALL draw modules across the 4 metric sets, byte-for-byte vs the ~36 sprite/testdata/*.png refs via the png dev-dep). Copy sprite/testdata/*.png into rustty-font/res/. Commit "rustty: font sprite Face + PNG known-answer suite (pixel-exact)".')

phase('Verify')
const verify = await robust(RULES + 'Phases: ' + JSON.stringify([comp, stroke, scaf, drawA, drawBox, drawLeg, face].map(s => s && s.commit)) + '\n'
  + 'TASK (independent verify — default RED; completes F0 sprite). Add font_sprite* mappings to ' + GATES + '/check_test_counts.py (GHF=src/font/sprite). Confirm FOREGROUND: cargo build --workspace; `python3 ' + GATES + '/check_test_counts.py` => sprite rust>=zig + WHOLE gate GREEN (count_ok); cargo test -p rustty-font + -p rustty-z2d (all pass incl the sprite PNG known-answer suite — pixel_exact); cargo +nightly miri test ~4 ISOLATED (0 UB/leak); clippy --all-targets -D + fmt --check + ' + GATES + '/check_acyclic.sh + CONFIRM rustty-font STILL NO rustty-vt edge (font_no_vt_edge); G3b ALL sprite/*.zig + draw/*.zig missing=0 (the draw-module gate). Spot-check 3 (a braille glyph, a box-drawing glyph, the render-all-sprites PNG compare) faithful + pixel-exact. Verdict GREEN only if count_ok + pixel_exact + font_no_vt_edge + build + tests + Miri + g3b=0 + clippy + fmt + acyclic AND no regression. Report verdict + tests_passing + count_ok + pixel_exact + font_no_vt_edge + miri_subset_clean + g3b_missing + blockers.',
  { schema: VERIFY, effort: 'high', label: 'verify', phase: 'Verify' })
return { comp, stroke, scaf, drawA, drawBox, drawLeg, face, verify }
