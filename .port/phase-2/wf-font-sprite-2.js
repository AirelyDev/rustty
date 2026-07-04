export const meta = {
  name: 'font-sprite-2',
  description: 'Finish the F0 sprite tier: the z2d managed Context drawing API (new prereq for branch/special) then branch.zig+special.zig, then symbols_for_legacy_computing(1413)+supplement(633), then Face.zig + the PNG known-answer suite (render all sprites, 36 testdata refs). Done so far (HEAD d1deb66): z2d composite+stroke, sprite scaffold, braille/block/geometric/powerline/box. rustty-font 67 + rustty-z2d 57.',
  phases: [
    { title: 'Z2dContext', detail: 'z2d managed Context API (set_source/move_to/line_to/curve_to/close_path/arc->bezier/fill/stroke + line_width/line_cap_mode) + sanity/Miri' },
    { title: 'BranchSpecial', detail: 'draw/branch.zig + draw/special.zig (now Context available)' },
    { title: 'Legacy', detail: 'draw/{symbols_for_legacy_computing(1413), supplement(633)}' },
    { title: 'Face', detail: 'sprite/Face.zig (550, 2t) + the PNG known-answer suite (render all sprites, 36 testdata refs)' },
    { title: 'Verify', detail: 'count-gate + tests + PNG pixel-exact + Miri + G3b(all sprite/draw *.zig) + font_no_vt_edge' },
  ],
}
const G = '/Users/gael/projects/ghostty', R = '/Users/gael/projects/rustty'
const SP = G + '/src/font/sprite'
const Z2D = '/Users/gael/.cache/zig/p/z2d-0.10.0-j5P_Hu-6FgBsZNgwphIqh17jDnj8_yPtD8yzjO6PpHRQ'
const GATES = R + '/.port/tools/gates'
const RULES = 'OBEY: PARITY CONSTITUTION + ' + R + '/.port/PORT_RULES.md. R1 = PORT EVERY test{} + full G3b comment coverage of every sprite/draw/*.zig (the draw modules are 0-test — G3b is their gate); EVERY line/comment verbatim; PIXEL-EXACT for the PNG known-answer suite. R3 faithful; R5 NO weakening/stub-to-pass. Reference READ-ONLY ' + G + '/src/font/ + the z2d source ' + Z2D + ' (READ-ONLY). '
  + 'DONE (HEAD d1deb66): rustty-z2d (pure Rust) has Path/Surface/pixel formats + .none+multisample_4x fill + composite/composite_stride + stroke tessellation (non-hairline/non-dashed) + the scalar compositor. rustty-font has the sprite scaffold (Sprite enum/common/canvas) + draw/{braille,block,geometric_shapes,powerline,box}. glyf_rasterize + z2d proven pixel-exact. rustty-font PURE & BELOW vt (memory apc-glyf-font-layering-d1). Plan: memory heavy-tier-strategy. '
  + 'NEW z2d PREREQUISITE: the MANAGED Context drawing API (crates/rustty-z2d/src/context.rs currently only init/deinit/set_transformation) — branch.zig branchNode() + special.zig underline_dotted/curly use canvas.getContext() -> ctx.{setSource,setLineWidth,moveTo,lineTo,curveTo,arc,closePath,fill,stroke} + line_cap_mode. Port that Context API (arc via arc->bezier tessellation; fill/stroke delegate to the ported painter) with z2d-internal sanity tests + isolated Miri. '
  + 'PNG known-answer: decode sprite/testdata/*.png refs (copy from ' + SP + '/testdata/ into rustty-font/res/) via the `png` dev-dependency + assert pixel-exact alpha8. MIRI per MIRI_EXCEPTIONS.md ISOLATED. Run gates FOREGROUND. Commit trailer "Co-Authored-By: Claude Opus 4.8 (1M context)". '
async function robust(p, o) { let r = await agent(p, o); if (!r) r = await agent(p, { ...o, label: (o.label || 'a') + '-retry' }); return r }
const STEP = { type: 'object', additionalProperties: true, required: ['ok', 'blockers'], properties: { ok: { type: 'boolean' }, tests_added: { type: 'integer' }, builds: { type: 'boolean' }, commit: { type: 'string' }, notes: { type: 'string' }, blockers: { type: 'array', items: { type: 'string' } } } }
const VERIFY = { type: 'object', additionalProperties: true, required: ['verdict', 'blockers'], properties: { verdict: { type: 'string', enum: ['GREEN', 'RED'] }, tests_passing: { type: 'integer' }, count_ok: { type: 'boolean' }, pixel_exact: { type: 'boolean' }, font_no_vt_edge: { type: 'boolean' }, miri_subset_clean: { type: 'boolean' }, g3b_missing: { type: 'integer' }, notes: { type: 'string' }, blockers: { type: 'array', items: { type: 'string' } } } }

phase('Z2dContext')
const ctx = await robust(RULES + 'TASK (z2d managed Context API). Port the z2d 0.10.0 Context drawing API into crates/rustty-z2d/src/context.rs (currently only init/deinit/set_transformation/get_transformation): set_source, line_width/line_cap_mode fields, move_to/line_to/curve_to/close_path (path building), arc (arc->bezier tessellation, from z2d arc.zig), fill (delegate to painter::fill with the current source+fill_rule), stroke (delegate to painter::stroke with the current line_width/cap). + z2d-internal sanity tests (fill a moved/lined/closed path; stroke a curve; arc a circle) + isolated Miri. GATES FOREGROUND: cargo build -p rustty-z2d; cargo test -p rustty-z2d (sanity pass + no regression vs 57); Miri-subset 2 ISOLATED (0 UB/leak); clippy -D; fmt. Commit "rustty: z2d managed Context drawing API (set_source/path/arc/fill/stroke)". Return ok+tests_added+builds+commit+blockers.', { schema: STEP, effort: 'high', label: 'z2d-context', phase: 'Z2dContext' })

async function draw(label, phaseName, prompt) {
  return robust(RULES + 'z2d Context landed: ' + JSON.stringify(ctx && ctx.commit) + '\n' + prompt
    + ' GATES FOREGROUND: cargo build -p rustty-font; cargo test -p rustty-font (no regression); Miri-subset 1-2 ISOLATED if new tests; clippy --all-targets -D; fmt; scoped G3b for the ported sprite/draw/*.zig missing=0. Commit. Return ok+tests_added+builds+commit+blockers.',
    { schema: STEP, effort: 'high', label, phase: phaseName })
}
phase('BranchSpecial')
const bs = await draw('branch-special', 'BranchSpecial', 'TASK. Port ' + SP + '/draw/branch.zig (504loc, uses ctx.arc + box.arc) + ' + SP + '/draw/special.zig (372loc — underline/double/dashed/dotted/curly, strikethrough, overline, cursor_*) into rustty-font src/sprite/draw/. Commit "rustty: font sprite draw branch + special".')
phase('Legacy')
const leg = await draw('legacy', 'Legacy', 'TASK. Port ' + SP + '/draw/{symbols_for_legacy_computing.zig 1413, symbols_for_legacy_computing_supplement.zig 633} into rustty-font src/sprite/draw/. Commit "rustty: font sprite draw legacy-computing symbols".')

phase('Face')
const face = await draw('face', 'Face', 'TASK (Face + PNG suite — the headline gate). Port ' + SP + '/Face.zig (550loc, 2t — composites draw modules into per-256 atlases; renderGlyph dispatch over the Sprite enum) + the "sprite face render all sprites" PNG known-answer test (composite ALL draw modules across the 4 metric sets, byte-for-byte vs the ~36 sprite/testdata/*.png refs via the png dev-dep). Copy sprite/testdata/*.png -> rustty-font/res/. This is the pixel-exact gate for the whole sprite tier. Commit "rustty: font sprite Face + PNG known-answer suite (pixel-exact) — sprite tier COMPLETE".')

phase('Verify')
const verify = await robust(RULES + 'Phases: ' + JSON.stringify([ctx, bs, leg, face].map(s => s && s.commit)) + '\n'
  + 'TASK (independent verify — default RED; completes F0 sprite + the font-pure tier). Add font_sprite* mappings to ' + GATES + '/check_test_counts.py (GHF=src/font/sprite). Confirm FOREGROUND: cargo build --workspace; `python3 ' + GATES + '/check_test_counts.py` => sprite rust>=zig + WHOLE gate GREEN (count_ok); cargo test -p rustty-font + -p rustty-z2d (all pass incl the sprite PNG known-answer suite — pixel_exact); cargo +nightly miri test ~4 ISOLATED (0 UB/leak); clippy --all-targets -D + fmt --check + ' + GATES + '/check_acyclic.sh + CONFIRM rustty-font STILL NO rustty-vt edge (font_no_vt_edge); G3b ALL sprite/*.zig + draw/*.zig missing=0. Update NEXT_PHASE_PLAN (F0 font-pure DONE; next = F1 -sys / face tier). Spot-check 3 (a branch glyph, a legacy-computing glyph, the render-all-sprites PNG compare) faithful + pixel-exact. Verdict GREEN only if count_ok + pixel_exact + font_no_vt_edge + build + tests + Miri + g3b=0 + clippy + fmt + acyclic AND no regression. Report verdict + tests_passing + count_ok + pixel_exact + font_no_vt_edge + miri_subset_clean + g3b_missing + blockers.',
  { schema: VERIFY, effort: 'high', label: 'verify', phase: 'Verify' })
return { ctx, bs, leg, face, verify }
