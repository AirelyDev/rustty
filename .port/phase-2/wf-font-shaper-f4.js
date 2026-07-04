export const meta = {
  name: 'font-shaper-f4',
  description: 'F4: the shaper layer completing rustty-font-shaper. shape.zig + shaper/{run,feature,Cache,noop} framework + shaper/harfbuzz (24t, harfbuzz-sys) + shaper/coretext (27t, objc2 core-text) + SharedGrid(3t) + SharedGridSet(4t) — the crate the renderer consumes (font_grid: *SharedGrid). Deps vt(RenderState.Cell)+unicode+font+the F3 faces. ~63 tests.',
  phases: [
    { title: 'Framework', detail: 'shape.zig + shaper/{run,feature,Cache,noop} — the shaper framework + noop + features + run' },
    { title: 'Harfbuzz', detail: 'shaper/harfbuzz.zig (24t) — HarfBuzz shaping via harfbuzz-sys' },
    { title: 'Coretext', detail: 'shaper/coretext.zig (27t) — CoreText shaping via objc2-core-text' },
    { title: 'SharedGrid', detail: 'SharedGrid(3t) + SharedGridSet(4t) — the renderer-facing glyph grid + set' },
    { title: 'Verify', detail: 'count-gate + tests + ASan + G3b + faithfulness; rustty-font-shaper COMPLETE' },
  ],
}
const G = '/Users/gael/projects/ghostty', R = '/Users/gael/projects/rustty'
const GFF = G + '/src/font'
const GATES = R + '/.port/tools/gates'
const RULES = 'OBEY: PARITY CONSTITUTION + ' + R + '/.port/PORT_RULES.md. R1 = PORT EVERY test{} (count is a CONTRACT); EVERY line/comment verbatim; assertion-for-assertion, INDEPENDENT expected values. R3 faithful; R5 NO weakening. Reference READ-ONLY ' + GFF + '/. This completes rustty-font-shaper (D1 tier ABOVE vt+font). '
  + 'DONE (HEAD 737369c): rustty-font-shaper has face.zig + face/{freetype,coretext} + Collection/DeferredFace/CodepointResolver + discovery (47t). Deps: rustty-vt (RenderState.Cell for shaping), rustty-unicode, rustty-font, freetype-sys, harfbuzz-sys, objc2-core-text/graphics/foundation (macOS). The shaper produces the glyph runs the renderer draws. '
  + 'shaper/harfbuzz uses harfbuzz-sys (hb_buffer/hb_shape over the face`s hb_font, feature application). shaper/coretext uses objc2-core-text (CTLine/CTRun over a CTFont). shape.zig selects the backend (cfg matrix: macOS=coretext or harfbuzz; both testable on macOS). SharedGrid = the shared glyph atlas grid the renderer reads (font_grid). MIRI: FFI (harfbuzz/CoreText) CANNOT run under Miri (R6) -> ASan-cover the FFI tests, Miri only pure helpers. [:0]const u8->Vec<u8>. SOUNDNESS (memory): raw-ptr discipline. Run gates FOREGROUND. Commit trailer "Co-Authored-By: Claude Opus 4.8 (1M context)". '
async function robust(p, o) { let r = await agent(p, o); if (!r) r = await agent(p, { ...o, label: (o.label || 'a') + '-retry' }); return r }
const STEP = { type: 'object', additionalProperties: true, required: ['ok', 'blockers'], properties: { ok: { type: 'boolean' }, tests_added: { type: 'integer' }, builds: { type: 'boolean' }, commit: { type: 'string' }, notes: { type: 'string' }, blockers: { type: 'array', items: { type: 'string' } } } }
const VERIFY = { type: 'object', additionalProperties: true, required: ['verdict', 'blockers'], properties: { verdict: { type: 'string', enum: ['GREEN', 'RED'] }, tests_passing: { type: 'integer' }, count_ok: { type: 'boolean' }, asan_clean: { type: 'boolean' }, g3b_missing: { type: 'integer' }, shaper_complete: { type: 'boolean' }, tautological_tests: { type: 'array', items: { type: 'string' } }, notes: { type: 'string' }, blockers: { type: 'array', items: { type: 'string' } } } }

phase('Framework')
const fw = await robust(RULES + 'TASK (shaper framework). Port ' + GFF + '/shape.zig (102loc, 1t — the Shaper interface + backend select) + ' + GFF + '/shaper/{run.zig 409/0t (TextRun + RunIterator), feature.zig 354/2t (font-feature settings parse), Cache.zig 106/1t (shaper LRU cache), noop.zig 143/1t (the no-op shaper)} into rustty-font-shaper src/shaper/. GATES FOREGROUND: cargo build -p rustty-font-shaper; cargo test -p rustty-font-shaper (new pass + no regression vs 47); ASan on any FFI tests; clippy --all-targets -D; fmt; scoped G3b missing=0. Commit "rustty: font-shaper shape framework + run/feature/Cache/noop". Return ok+tests_added+builds+commit+blockers.', { schema: STEP, effort: 'high', label: 'framework', phase: 'Framework' })

async function fill(label, phaseName, prompt) {
  return robust(RULES + 'Framework: ' + JSON.stringify(fw && fw.commit) + '\n' + prompt
    + ' GATES FOREGROUND: cargo build -p rustty-font-shaper; cargo test -p rustty-font-shaper (new tests pass + no regression); ASan on the FFI tests (Miri cannot run C/objc FFI — R6); clippy --all-targets -D; fmt; scoped G3b missing=0. Commit. Return ok+tests_added+builds+commit+blockers.',
    { schema: STEP, effort: 'high', label, phase: phaseName })
}
phase('Harfbuzz')
const hb = await fill('harfbuzz-shaper', 'Harfbuzz', 'TASK. Port ' + GFF + '/shaper/harfbuzz.zig (2172loc, 24t — the HarfBuzz Shaper: hb_buffer over a run, feature application, hb_shape, glyph-run extraction) into rustty-font-shaper src/shaper/harfbuzz.rs via harfbuzz-sys + the face hb_font. Commit "rustty: font-shaper harfbuzz shaper + 24 tests".')
phase('Coretext')
const ct = await fill('coretext-shaper', 'Coretext', 'TASK. Port ' + GFF + '/shaper/coretext.zig (2678loc, 27t — the CoreText Shaper: CTLine/CTRun over a CTFont, glyph-run extraction) into rustty-font-shaper src/shaper/coretext.rs via objc2-core-text (CTLine/CTRun/CTFont) + objc2-foundation. Commit "rustty: font-shaper coretext shaper + 27 tests".')
phase('SharedGrid')
const sg = await fill('sharedgrid', 'SharedGrid', 'TASK. Port ' + GFF + '/{SharedGrid.zig 540/3t (the shared glyph grid: renderGlyph caching over Collection+Atlas, the font_grid the renderer reads), SharedGridSet.zig 834/4t (the set keyed by config/font descriptors)} into rustty-font-shaper. Commit "rustty: font-shaper SharedGrid + SharedGridSet + 7 tests".')

phase('Verify')
const verify = await robust(RULES + 'Phases: ' + JSON.stringify([fw, hb, ct, sg].map(s => s && s.commit)) + '\n'
  + 'TASK (independent verify — default RED; completes rustty-font-shaper). Add the F4 shaper module mappings to ' + GATES + '/check_test_counts.py (GHF=src/font; shape 1, shaper/harfbuzz 24, shaper/coretext 27, feature 2, Cache 1, noop 1, SharedGrid 3, SharedGridSet 4). Confirm FOREGROUND: cargo build --workspace; `python3 ' + GATES + '/check_test_counts.py` => font-shaper F4 modules rust>=zig + WHOLE gate GREEN (count_ok); cargo test -p rustty-font-shaper (~110 total: 47 F3 + 63 F4 minus any deferred) + no workspace regression (vt 2019, font 74); `bash ' + GATES + '/sanitizers.sh rustty-font-shaper "" address` (ASan clean); clippy --all-targets -D + fmt --check + ' + GATES + '/check_acyclic.sh (rustty-font STILL no vt edge; font-shaper deps vt+font+sys). G3b the ported .zig missing=0. Update NEXT_PHASE_PLAN (F4 shaper done; rustty-font-shaper COMPLETE; renderer now unblocked — it consumes SharedGrid). Spot-check 3 (a harfbuzz shape run, a coretext shape run, a SharedGrid renderGlyph) faithful. Verdict GREEN only if count_ok + build + tests + ASan + g3b=0 + clippy + fmt + acyclic AND no regression (shaper_complete). Report verdict + tests_passing + count_ok + asan_clean + g3b_missing + shaper_complete + tautological_tests + blockers.',
  { schema: VERIFY, effort: 'high', label: 'verify', phase: 'Verify' })
return { fw, hb, ct, sg, verify }
