export const meta = {
  name: 'vt12c-kitty',
  description: 'rustty-vt vt-12c-kitty: reactivate the deferred kitty graphics now that Screen + Terminal exist. Storage: graphics_storage->ImageStorage (19t, needs Screen) + Screen kitty_images hooks. Exec: graphics_exec->execute (8t, needs Terminal) + Terminal kittyGraphics/setKittyGraphics* wrappers + the 2 stream_terminal kitty-graphics tests. Render: graphics_unicode render_placement/grid (14t + 58 comments, needs ImageStorage). +~41 tests.',
  phases: [
    { title: 'Storage', detail: 'graphics_storage ImageStorage + Screen kitty_images hooks + 19 tests' },
    { title: 'Exec', detail: 'graphics_exec execute + Terminal kittyGraphics wrappers + 8 tests + 2 stream tests' },
    { title: 'Render', detail: 'graphics_unicode render_placement/grid + 14 tests + 58 comments' },
    { title: 'Verify', detail: 'kitty G3b->0 + tests + Miri-subset + ASan + faithfulness' },
  ],
}
const G = '/Users/gael/projects/ghostty', R = '/Users/gael/projects/rustty'
const K = G + '/src/terminal/kitty'
const GATES = R + '/.port/tools/gates'
const RULES = 'OBEY: PARITY CONSTITUTION + ' + R + '/.port/PORT_RULES.md. R1 = 100% fidelity (every line/comment verbatim; tests assertion-for-assertion, independent expected values). Reference READ-ONLY. vt-12b done — Screen + Terminal fully ported; 1353 tests pass. These kitty files were DEFERRED in vt-7 (DEFERRALS.md, needed Screen/Terminal/ImageStorage). Plan: ' + R + '/.port/phase-2/TERMINAL_PLAN.md. '
  + 'KEY: kitty graphics is gated by build_options.kitty_graphics (a cargo feature/cfg). graphics_storage::ImageStorage stores images + placements, methods take &Screen/&mut Screen (now ported); graphics_exec::execute runs a kitty graphics Command against a Terminal (now ported); graphics_unicode render_placement/grid need ImageStorage+Image. Wire: Screen.kitty_images:ImageStorage hooks (the deferred PORT NOTE sites in screen.rs), Terminal kittyGraphics->execute + setKittyGraphics*->kitty_images.setLimit, kitty/graphics.rs re-exports (ImageStorage + execute), and the 2 deferred kitty-graphics tests in stream_terminal. SOUNDNESS RULE (memory): &mut Page/Row/Cell/Screen args re-read across assert_integrity -> raw ptrs; addr_of_mut! discipline. [:0]const u8->Vec<u8>; PNG decode = sys.decode_png null hook (vt-7 seam). '
  + 'MIRI: per MIRI_EXCEPTIONS.md (restored) Miri-subset on new tests in ISOLATION. PROCESS LESSON: SCOPED G3b of just-filled files (kitty G3b -> 0; graphics_unicode 58 deferred comments now land). '
  + 'Run gates FOREGROUND. Commit trailer "Co-Authored-By: Claude Opus 4.8 (1M context)". Update DEFERRALS.md as each entry resolves. '
async function robust(p, o) { let r = await agent(p, o); if (!r) r = await agent(p, { ...o, label: (o.label || 'a') + '-retry' }); return r }
const STEP = { type: 'object', additionalProperties: true, required: ['ok', 'blockers'], properties: { ok: { type: 'boolean' }, tests_passing: { type: 'integer' }, builds: { type: 'boolean' }, commit: { type: 'string' }, notes: { type: 'string' }, blockers: { type: 'array', items: { type: 'string' } } } }
const VERIFY = { type: 'object', additionalProperties: true, required: ['verdict', 'blockers'], properties: { verdict: { type: 'string', enum: ['GREEN', 'RED'] }, tests_passing: { type: 'integer' }, miri_subset_clean: { type: 'boolean' }, asan_clean: { type: 'boolean' }, kitty_g3b_missing: { type: 'integer' }, deferrals_resolved: { type: 'boolean' }, tautological_tests: { type: 'array', items: { type: 'string' } }, notes: { type: 'string' }, blockers: { type: 'array', items: { type: 'string' } } } }

phase('Storage')
const storage = await robust(RULES +
  'TASK (graphics_storage ImageStorage). Fill crates/rustty-vt/src/kitty/graphics_storage.rs from ' + K + '/graphics_storage.zig (~1364 loc, 19 tests; ImageStorage: add/get/delete images + placements, eviction by total-bytes limit, needs &Screen/&mut Screen). Wire Screen.kitty_images: ImageStorage — fill the deferred kitty_images hook sites in screen.rs (setLimit/evict/dirty PORT NOTEs). Port the 19 tests. GATES FOREGROUND: build (with kitty_graphics feature); cargo test -p rustty-vt kitty (19 pass + 1353 prior no regression); SCOPED G3b on graphics_storage.zig missing=0; Miri-subset 2 tests ISOLATED; ASan; clippy -D; fmt. Commit "rustty: kitty ImageStorage (graphics_storage) + Screen hooks + 19 tests (vt-12c-kitty)". Return ok+tests_passing+builds+commit+blockers.',
  { schema: STEP, effort: 'high', label: 'storage', phase: 'Storage' })

phase('Exec')
const exec = await robust(RULES + 'Storage landed: ' + JSON.stringify(storage && { commit: storage.commit, blockers: storage.blockers }) + '\n'
  + 'TASK (graphics_exec execute + Terminal wiring). Fill crates/rustty-vt/src/kitty/graphics_exec.rs from ' + K + '/graphics_exec.zig (~575 loc, 8 tests; execute(Terminal, Command) -> runs the kitty graphics command). Wire Terminal: fill kitty_graphics->execute + set_kitty_graphics_size_limit/loading_limits->kitty_images, and the 2 deferred kitty-graphics arms/tests in stream_terminal.rs. Wire kitty/graphics.rs re-exports (ImageStorage + execute). Port the 8 graphics_exec tests + the 2 stream_terminal kitty-graphics tests. GATES FOREGROUND: build; cargo test -p rustty-vt (10 new pass + no regression); SCOPED G3b graphics_exec.zig missing=0; Miri-subset 2 tests; ASan; clippy -D; fmt. Commit "rustty: kitty execute (graphics_exec) + Terminal wrappers + 10 tests (vt-12c-kitty)". Return ok+tests_passing+builds+commit+blockers.',
  { schema: STEP, effort: 'high', label: 'exec', phase: 'Exec' })

phase('Render')
const render = await robust(RULES + 'Exec landed: ' + JSON.stringify(exec && { commit: exec.commit, blockers: exec.blockers }) + '\n'
  + 'TASK (graphics_unicode render_placement/grid). Fill the DEFERRED render_placement() + grid() methods in crates/rustty-vt/src/kitty/graphics_unicode.rs (signatures documented at graphics_unicode.rs:170-179; need ImageStorage+Image, now available) with the verbatim 58 inner comments from ' + K + '/graphics_unicode.zig + port the 14 deferred tests (11 placement-iterator + 3 render, graphics_unicode.rs:448-452). GATES FOREGROUND: build; cargo test -p rustty-vt (14 new pass + no regression); SCOPED G3b graphics_unicode.zig missing=0 (the 58 comments now present); Miri-subset 2 tests; ASan; clippy -D; fmt. Commit "rustty: kitty graphics_unicode render_placement/grid + 14 tests (vt-12c-kitty)". Update DEFERRALS.md (kitty entries resolved). Return ok+tests_passing+builds+commit+blockers.',
  { schema: STEP, effort: 'high', label: 'render', phase: 'Render' })

phase('Verify')
const verify = await robust(RULES + 'Phases: ' + JSON.stringify([storage, exec, render].map(s => s && s.commit)) + '\n'
  + 'TASK (independent verify — default RED). Confirm FOREGROUND: cargo build -p rustty-vt (+ with kitty_graphics feature); cargo test -p rustty-vt (all ~41 new kitty tests pass + no regression vs 1353); cargo +nightly miri test -p rustty-vt on ~4 kitty tests (ISOLATED, 0 UB/leak — image storage/placement raw ptrs); `bash ' + GATES + '/sanitizers.sh rustty-vt kitty address` (ASan clean); clippy --all-targets -D + fmt --check + ' + GATES + '/check_acyclic.sh. SCOPED G3b on graphics_storage.zig + graphics_exec.zig + graphics_unicode.zig => missing=0 (the 58 graphics_unicode comments + the storage/exec comments all present now). Confirm DEFERRALS.md kitty entries (graphics_storage/exec, render_placement, graphics.rs re-exports) all RESOLVED. Spot-check 3 tests (an image-store/evict, a graphics transmit-exec, a unicode render-placement) faithful, not tautological. Verdict GREEN only if build + tests + Miri-subset + ASan + kitty_g3b_missing=0 + deferrals_resolved + clippy + fmt green AND no regression. Report verdict + tests_passing + miri_subset_clean + asan_clean + kitty_g3b_missing + deferrals_resolved + tautological_tests + blockers.',
  { schema: VERIFY, effort: 'high', label: 'verify', phase: 'Verify' })
return { storage, exec, render, verify }
