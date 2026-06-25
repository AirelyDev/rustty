export const meta = {
  name: 'vtc-stage-b4',
  description: 'Stage B v4: fill the vt-c mutual core using the D3-decided trampoline seam (Option 1, vt-c-local, NO rustty-vt change). Skeleton (7c67b93) + leaves done = 76t. Seam phase implements the @fieldParentPtr->offset_of! container_of trampoline + heap-pin lifecycle; then fill refs/selection (terminals now constructable), then terminal tests, then kitty, consumers, header. -> 307.',
  phases: [
    { title: 'Seam', detail: 'D3 trampoline (offset_of! container_of) + Terminal new/free/vt_write/resize/reset/mode + 9 Effects trampolines + scroll_viewport + probe tests (Miri-clean)' },
    { title: 'FillRefsSel', detail: 'point/grid_ref/grid_ref_tracked/selection bodies + tests + terminal grid_ref accessors (18t)' },
    { title: 'FillTerminal', detail: 'remaining terminal.zig tests (bodies done across Seam+RefsSel) -> 89 terminal total' },
    { title: 'FillKitty', detail: 'kitty_graphics bodies + 35 tests' },
    { title: 'Consumers1', detail: 'render(38)+selection_gesture(19) — consume terminal one-way (57t)' },
    { title: 'Consumers2', detail: 'formatter/key_encode/mouse_encode/osc/sgr(Action [8]u64)/types (32t)' },
    { title: 'HeaderVerify', detail: 'rustty_* exports + cbindgen header + C/C++ compile + ABI gates + full verify (307)' },
  ],
}
const G = '/Users/gael/projects/ghostty', R = '/Users/gael/projects/rustty'
const GC = G + '/src/terminal/c'
const GATES = R + '/.port/tools/gates'
const RSC = R + '/crates/rustty-vt-c'
const RULES = 'OBEY: PARITY CONSTITUTION + ' + R + '/.port/PORT_RULES.md. R1 = PORT EVERY test{} (307 c/ tests; 76 landed at de7986f); EVERY line/comment verbatim; INDEPENDENT expected values. R2 = rustty_* exports. R3 = faithful, rustty-vt types source of truth, NEVER fabricate. R5 = NEVER weaken/stub to pass. Reference READ-ONLY ' + GC + '/. The skeleton (commit 7c67b93) defined all mutual-atom types + todo!() fn sigs; foundation+io-leaves+cell/row/style filled (76t). '
  + 'D3 SEAM (SIGNED OFF — Option 1, vt-c-LOCAL, NO rustty-vt change; memory vtc-effects-trampoline-seam-d3): REUSE rustty_vt::stream_terminal::Handler as the wrapper handler (NOT a separate CHandler — Zig reuses it: terminal.zig:288 t.vtHandler()); install vt-c-local Rust trampoline fns into handler.effects. The C fn-ptr table + userdata stay on vt-c`s Effects (sibling of stream on TerminalWrapper). RECOVERY (the @fieldParentPtr port): `let w = (handler_ptr as *const u8).sub(core::mem::offset_of!(TerminalWrapper, stream.handler)) as *mut TerminalWrapper;` (nested offset_of! stable on 1.93.1; fallback = sum two offset_of!s if cross-crate nested path rejected). NO #[repr(C)] needed (C holds only the opaque *mut TerminalWrapper). SB SOUNDNESS: recover via RAW *mut TerminalWrapper; read wrapper.effects/wrapper.terminal (SIBLINGS of the handler, not inside the strongly-protected &mut Handler) through raw ptrs; NEVER form an overlapping &mut TerminalWrapper while the &mut Handler is live (the rust-mut-args-strongly-protected discipline). STORAGE: TerminalWrapper { terminal: *mut Terminal (separate Box::into_raw), stream: Stream<static> (already correct — alias bakes in H=Handler), effects, tracked_grid_refs }. LIFECYCLE: new = Box::into_raw Terminal -> vt_handler() -> install trampolines into handler.effects -> Stream::init -> Box::into_raw wrapper; free = Box::from_raw wrapper -> null tracked-ref back-ptrs -> stream.deinit -> Box::from_raw terminal + deinit -> drop (Terminal freed AFTER stream). The ScreenSet pattern (screen_set.rs). '
  + 'D2 (signed off): ScrollViewport [2]u64 (done in skeleton) + stream Action [16]u64 + sgr Attribute [8]u64 hand-authored repr(C) unions, PINNED literal padding + const size/align asserts. Exports = #[no_mangle] pub extern "C" fn rustty_<name> (1:1 of lib_vt.zig 168 @export). MIRI per MIRI_EXCEPTIONS.md ISOLATED (heavy C-boundary -> ASan). Run gates FOREGROUND. Commit trailer "Co-Authored-By: Claude Opus 4.8 (1M context)". '
async function robust(p, o) { let r = await agent(p, o); if (!r) r = await agent(p, { ...o, label: (o.label || 'a') + '-retry' }); return r }
const STEP = { type: 'object', additionalProperties: true, required: ['ok', 'blockers'], properties: { ok: { type: 'boolean' }, tests_added: { type: 'integer' }, builds: { type: 'boolean' }, commit: { type: 'string' }, notes: { type: 'string' }, blockers: { type: 'array', items: { type: 'string' } } } }
const VERIFY = { type: 'object', additionalProperties: true, required: ['verdict', 'blockers'], properties: { verdict: { type: 'string', enum: ['GREEN', 'RED'] }, vtc_test_count: { type: 'integer' }, count_gate_ok: { type: 'boolean' }, abi_asserts_present: { type: 'boolean' }, cbindgen_header_ok: { type: 'boolean' }, c_cpp_compile_ok: { type: 'boolean' }, seam_miri_clean: { type: 'boolean' }, asan_clean: { type: 'boolean' }, g3b_missing: { type: 'integer' }, padding_pinned: { type: 'boolean' }, no_todo_left: { type: 'boolean' }, tautological_tests: { type: 'array', items: { type: 'string' } }, notes: { type: 'string' }, blockers: { type: 'array', items: { type: 'string' } } } }

phase('Seam')
const seam = await robust(RULES +
  'TASK (implement the D3 trampoline seam + Terminal lifecycle machinery). In ' + RSC + '/src/terminal.rs, replace the todo!() bodies for the LIFECYCLE + EFFECTS machinery per the D3 SEAM above: zig_terminal, new (the Box::into_raw lifecycle), free, vt_write, resize, reset, mode_get, mode_set, get/get_multi, scroll_viewport (the ScrollViewport [2]u64 union dispatch), and the 9 Effects trampolines (write_pty/bell/color_scheme/device_attributes/enquiry/xtversion/title_changed/pwd_changed/size — each: recover wrapper via offset_of! container_of, read wrapper.effects.<c_fn_ptr> + userdata via RAW ptr, call the C fn-ptr). Port the terminal.zig tests that exercise ONLY the lifecycle/effects (new/free/vt_write/mode/scroll_viewport/effect-callback round-trips — likely ~15-25 of the 89). GATES FOREGROUND: cargo build -p rustty-vt-c; cargo test -p rustty-vt-c (probes pass + 76 floor, no regression); cargo +nightly miri test -p rustty-vt-c on 2-3 SEAM tests ISOLATED (a vt_write->write_pty callback round-trip + new/free) -> 0 UB/leak (THIS is the soundness gate for the container_of recovery); clippy --all-targets -D; fmt. Commit "rustty: vt-c Terminal trampoline seam + lifecycle (D3, Stage B v4)". Return ok+tests_added+builds+commit+notes(miri clean?)+blockers.',
  { schema: STEP, effort: 'high', label: 'seam', phase: 'Seam' })

async function fill(label, phaseName, prompt) {
  return robust(RULES + 'Seam landed (trampoline + lifecycle work, Miri-clean): ' + JSON.stringify(seam && seam.commit) + '\n'
    + 'TASK (fill bodies + ALL tests; crate stays compiling, terminals constructable via the seam). ' + prompt + ' GATES FOREGROUND: cargo build -p rustty-vt-c; cargo test -p rustty-vt-c (new tests pass + no regression); Miri-subset 2 tests ISOLATED (heavy C-boundary -> ASan); clippy --all-targets -D; fmt; scoped G3b missing=0. Commit. Return ok+tests_added+builds+commit+blockers.',
    { schema: STEP, effort: 'high', label, phase: phaseName })
}

phase('FillRefsSel')
const rs = await fill('fill-refssel', 'FillRefsSel', 'Fill point.rs (from_c/cval), ' + GC + '/{grid_ref.zig (from_pin/to_pin + grid_ref_cell/row/graphemes/hyperlink_uri/style), grid_ref_tracked.zig, selection.zig (word/word_between/line/all/output/format/adjust/order/ordered/contains/equal)} bodies + ALL their tests, AND the terminal.rs grid_ref/grid_ref_track accessor bodies (which call grid_ref/selection). 18+ tests. Commit "rustty: vt-c point/grid_ref/grid_ref_tracked/selection + 18 tests (Stage B v4)".')

phase('FillTerminal')
const t = await fill('fill-terminal', 'FillTerminal', 'Port the REMAINING terminal.zig tests (all bodies now exist across Seam + RefsSel — selection/grid_ref accessors filled) so terminal.rs reaches all 89 terminal.zig tests. Commit "rustty: vt-c terminal tests complete (89 total, Stage B v4)".')

phase('FillKitty')
const k = await fill('fill-kitty', 'FillKitty', 'Port ' + GC + '/kitty_graphics.zig FULLY (replace the opaque KittyGraphics with the real Kitty graphics C API) + 35 tests. Commit "rustty: vt-c kitty_graphics + 35 tests (Stage B v4)".')

phase('Consumers1')
const c1 = await fill('consumers1', 'Consumers1', 'Port (full bodies+tests; they @import terminal which now exists — NORMAL file ports) ' + GC + '/{render.zig 38t, selection_gesture.zig 19t}. 57 tests. Commit "rustty: vt-c render + selection_gesture + 57 tests (Stage B v4)".')

phase('Consumers2')
const c2 = await fill('consumers2', 'Consumers2', 'Port (full) ' + GC + '/{formatter.zig 11t, key_encode.zig 7t, mouse_encode.zig 7t, osc.zig 3t, sgr.zig 2t (incl the sgr Attribute [8]u64 hand-authored tagged union + const-asserts per D2), types.zig 2t (the ~29 C-ABI extern-struct registry — LAST)}. 32 tests. Commit "rustty: vt-c formatter/key_encode/mouse_encode/osc/sgr/types + 32 tests (Stage B v4)".')

phase('HeaderVerify')
const verify = await robust(RULES + 'Fill phases: ' + JSON.stringify([seam, rs, t, k, c1, c2].map(s => s && s.commit)) + '\n'
  + 'TASK (export surface + header + FINAL VERIFY — default RED; completes vt-c). (1) Audit rustty_* exports vs lib_vt.zig 168 @export lines; confirm NO todo!() remains anywhere in rustty-vt-c (grep). (2) cbindgen header-gen (xtask header-gen) -> include/rustty.h + C/C++ self-consistency compile gate (clang -xc + -xc++, double-include) per .port/phase-1/cbindgen-identity.md; resolve parse_deps cross-crate type emission. (3) Add "vt_c" mapping to ' + GATES + '/check_test_counts.py (c/*.zig vs rustty-vt-c/src) -> full gate GREEN incl vt_c (~307, count_gate_ok). (4) cargo test -p rustty-vt-c (all ~307 pass); ScrollViewport size_of==16 + Action size_of==128 + sgr Attribute [8]u64 const-asserts present + padding PINNED (abi_asserts_present + padding_pinned). (5) cargo +nightly miri test on ~4 tests incl a seam trampoline round-trip ISOLATED (0 UB/leak; seam_miri_clean) + `bash ' + GATES + '/sanitizers.sh rustty-vt-c "" address` (ASan clean). (6) clippy --all-targets -D + fmt --check + check_acyclic.sh (vt-c->vt+lib, NO rustty-vt change introduced). (7) G3b all 28 c/ files missing=0. (8) DEFERRALS.md: ScrollViewport + Action + sgr C-ABI RESOLVED. Spot-check 3 tests faithful. Verdict GREEN only if count_gate_ok(307) + abi_asserts_present + padding_pinned + cbindgen_header_ok + c_cpp_compile_ok + no_todo_left + seam_miri_clean + all pass + ASan + g3b=0 + clippy/fmt/acyclic. Commit "rustty: vt-c export surface + cbindgen header + ABI gates — vt-c COMPLETE (307 tests, Stage B)". Return verdict + vtc_test_count + count_gate_ok + abi_asserts_present + cbindgen_header_ok + c_cpp_compile_ok + seam_miri_clean + asan_clean + g3b_missing + padding_pinned + no_todo_left + tautological_tests + blockers.',
  { schema: VERIFY, effort: 'high', label: 'headerverify', phase: 'HeaderVerify' })
return { seam, rs, t, k, c1, c2, verify }
