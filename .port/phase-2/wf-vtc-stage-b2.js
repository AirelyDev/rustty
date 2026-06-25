export const meta = {
  name: 'vtc-stage-b2',
  description: 'Stage B v2 (re-phased — cluster discipline): the remaining 247 vt-c tests are a MUTUALLY-DEPENDENT C-ABI core (terminal<->grid_ref/selection/render, kitty, cell/row/style). Foundation (38t) + io-leaves (22t) = 60t already landed (eb9fe72). Lay down ALL remaining c/ types + extern fn sigs (todo!()) so vt-c compiles with the full type graph, THEN fill bodies+tests per file. D2 signed off: cbindgen header, ScrollViewport [2]u64 + Action [16]u64 pinned-padding repr(C) unions + const size-asserts, rustty_* exports.',
  phases: [
    { title: 'Skeleton', detail: 'all remaining c/ types (extern structs + TaggedUnions) + extern fn sigs todo!() -> vt-c compiles' },
    { title: 'FillTerminal', detail: 'terminal.zig bodies + 89 tests (the central wrapper + ScrollViewport)' },
    { title: 'FillTypesRefs', detail: 'cell/row/style/sgr + grid_ref/grid_ref_tracked/selection bodies + 36 tests' },
    { title: 'FillKitty', detail: 'kitty_graphics bodies + 35 tests' },
    { title: 'FillRenderGesture', detail: 'render + selection_gesture bodies + 57 tests' },
    { title: 'FillIoTypes', detail: 'key_encode/mouse_encode/osc/formatter + types.zig bodies + 30 tests' },
    { title: 'HeaderVerify', detail: 'rustty_* export surface + cbindgen header + C/C++ compile + ABI gates + full verify (307)' },
  ],
}
const G = '/Users/gael/projects/ghostty', R = '/Users/gael/projects/rustty'
const GC = G + '/src/terminal/c'
const GATES = R + '/.port/tools/gates'
const RSC = R + '/crates/rustty-vt-c'
const G3B = (zig, rs) => 'python3 ' + GATES + '/check_comment_coverage.py --crate rustty-vt-c --ref-files ' + zig + ' --port-glob "' + RSC + '/src/' + rs + '"'
const RULES = 'OBEY: PARITY CONSTITUTION + ' + R + '/.port/PORT_RULES.md. R1 = PORT EVERY test{} (the COUNT is a contract — 307 c/ tests total; 60 already landed at eb9fe72); EVERY line/comment verbatim; assertion-for-assertion, INDEPENDENT expected values. R2 = export symbols rustty_* (NOT ghostty_*). R3 = no behavior change (vt-c is a pure FFI projection; rustty-vt types are source of truth — NEVER fabricate a placeholder C type). R5 = NEVER weaken/stub to pass. Reference READ-ONLY ' + GC + '/. rustty-vt DONE (2019t); rustty-lib has C-ABI helpers; rustty-vt-c has foundation (allocator/build_info/color/focus/modes/result/sys/main) + io-leaves (paste/key_event/mouse_event/size_report) = 60t at eb9fe72. '
  + 'CLUSTER DISCIPLINE (this is why the first attempt failed — terminal.zig hard-deps grid_ref/selection/cell/kitty at COMPILE time, and grid_ref.zig imports terminal back = MUTUAL): the Skeleton phase lays down ALL remaining c/ types as `#[repr(C)]` extern structs + every extern fn as a SIGNATURE with `todo!()`/`unimplemented!()` body, so the whole crate COMPILES with the full mutual type graph; THEN fill phases replace the todo!() bodies + add the #[test]s per file (the crate stays compiling throughout). This mirrors the vt-12a struct-skeleton -> vt-12b method-fill that worked for the Terminal cluster. NEVER stub a TYPE incorrectly — extern struct layouts must be byte-exact from the start (they carry const size-asserts). '
  + 'D2 DECISION (SIGNED OFF): cbindgen emits rustty.h (xtask header-gen + cbindgen.toml); ScrollViewport (Terminal.zig ~1748, [2]u64) + stream Action c_union (stream.zig ~229, [16]u64) are HAND-AUTHORED `#[repr(C)] struct { tag: <fieldless repr(C) enum, c_int width>, value: <#[repr(C)] union { <variant>, _padding: [u64;N] }> }` with N PINNED LITERAL (the ABI size anchor; carry the verbatim "arbitrarily chosen"/"verify before shipping" TODO). Port lib/union.zig:33 as `const _: () = assert!(size_of::<Value>()==size_of::<[u64;N]>()); assert!(align_of==8); assert!(size_of::<Tag>()==size_of::<c_int>());` (BUILD-failure gate). Exports = `#[no_mangle] pub extern "C" fn rustty_<name>` (1:1 port of lib_vt.zig 168 @export ghostty_* lines). '
  + 'SOUNDNESS (memory): raw-ptr discipline across the C boundary — *mut Terminal etc., NOT &mut re-read across integrity walks. The terminal.zig Effects/@fieldParentPtr trampoline needs a deliberate raw-ptr userdata design (no Rust fieldParentPtr). [:0]const u8->Vec<u8>/&[u8]. MIRI: per MIRI_EXCEPTIONS.md ISOLATED; heavy C-boundary tests -> ASan-cover. PROCESS LESSONS (memory): port ALL tests (count enforces), scoped-G3b. Run gates FOREGROUND. Commit trailer "Co-Authored-By: Claude Opus 4.8 (1M context)". '
async function robust(p, o) { let r = await agent(p, o); if (!r) r = await agent(p, { ...o, label: (o.label || 'a') + '-retry' }); return r }
const STEP = { type: 'object', additionalProperties: true, required: ['ok', 'blockers'], properties: { ok: { type: 'boolean' }, tests_added: { type: 'integer' }, builds: { type: 'boolean' }, commit: { type: 'string' }, notes: { type: 'string' }, blockers: { type: 'array', items: { type: 'string' } } } }
const VERIFY = { type: 'object', additionalProperties: true, required: ['verdict', 'blockers'], properties: { verdict: { type: 'string', enum: ['GREEN', 'RED'] }, vtc_test_count: { type: 'integer' }, count_gate_ok: { type: 'boolean' }, abi_asserts_present: { type: 'boolean' }, cbindgen_header_ok: { type: 'boolean' }, c_cpp_compile_ok: { type: 'boolean' }, miri_subset_clean: { type: 'boolean' }, asan_clean: { type: 'boolean' }, g3b_missing: { type: 'integer' }, padding_pinned: { type: 'boolean' }, tautological_tests: { type: 'array', items: { type: 'string' } }, notes: { type: 'string' }, blockers: { type: 'array', items: { type: 'string' } } } }

phase('Skeleton')
const skel = await robust(RULES +
  'TASK (cluster skeleton — no tests, no logic, MUST compile). Lay down ALL remaining c/ types + extern fn signatures (todo!() bodies) so rustty-vt-c compiles with the FULL mutual type graph. Files to skeleton (types + fn sigs only): ' + GC + '/{cell.zig, row.zig, style.zig, sgr.zig, types.zig, grid_ref.zig, grid_ref_tracked.zig, selection.zig, kitty_graphics.zig, terminal.zig, render.zig, selection_gesture.zig, key_encode.zig, mouse_encode.zig, osc.zig, formatter.zig}. Define every `#[repr(C)]` extern struct BYTE-EXACT (cell/row/style/sgr/CGridRef/CSelection/render Colors/etc.) incl the ScrollViewport [2]u64 + Action [16]u64 hand-authored unions with PINNED padding + the const size/align asserts (these must be correct NOW, they are types not logic). Every extern fn -> signature with `todo!("vt-c <name>")`. Design the terminal Effects trampoline raw-ptr userdata seam (sig only). Wire mod tree in lib.rs. GATES FOREGROUND: cargo build -p rustty-vt-c (COMPILES, todo!() ok); clippy may warn on todo!() — allow; cargo fmt; check_acyclic.sh. Commit "rustty: vt-c cluster skeleton — full C-ABI type graph compiles (Stage B v2)". Return ok+builds+commit+notes(types defined)+blockers.',
  { schema: STEP, effort: 'high', label: 'skeleton', phase: 'Skeleton' })

async function fill(label, phaseName, files, prompt) {
  return robust(RULES + 'Skeleton landed (full type graph compiles): ' + JSON.stringify(skel && skel.commit) + '\n'
    + 'TASK (fill bodies + tests, crate stays compiling). ' + prompt + ' Replace the todo!() bodies in these files with faithful ports + ALL their test{} blocks: ' + files + '. GATES FOREGROUND: cargo build -p rustty-vt-c; cargo test -p rustty-vt-c (the new tests pass + no regression); Miri-subset 2 tests ISOLATED (raw C-boundary -> ASan-cover heavy ones); clippy --all-targets -D; fmt; scoped G3b missing=0 for the filled files. Commit. Return ok+tests_added+builds+commit+blockers.',
    { schema: STEP, effort: 'high', label, phase: phaseName })
}

phase('FillTerminal')
const t = await fill('fill-terminal', 'FillTerminal', GC + '/terminal.zig (89t)',
  'Fill terminal.zig — the central Terminal C wrapper (new/free/resize/reset/vt_write/mode get-set/the ScrollViewport scroll_viewport TaggedUnion dispatch/selection+grid_ref accessors/Effects trampoline via the raw-ptr userdata seam). 89 tests. Commit "rustty: vt-c terminal + 89 tests (Stage B v2)".')

phase('FillTypesRefs')
const tr = await fill('fill-typesrefs', 'FillTypesRefs', GC + '/{cell.zig 8t, row.zig 6t, style.zig 2t, sgr.zig 2t, grid_ref.zig 11t, grid_ref_tracked.zig 4t, selection.zig 3t}',
  'Fill the cell/row/style/sgr C projections (extern-struct @bitCast round-trips + their tests) and grid_ref/grid_ref_tracked/selection (CGridRef/CSelection fns + tracked pins). 36 tests. Commit "rustty: vt-c cell/row/style/sgr + grid_ref/selection + 36 tests (Stage B v2)".')

phase('FillKitty')
const k = await fill('fill-kitty', 'FillKitty', GC + '/kitty_graphics.zig (35t)',
  'Fill kitty_graphics.zig — the Kitty graphics C API (35t). Commit "rustty: vt-c kitty_graphics + 35 tests (Stage B v2)".')

phase('FillRenderGesture')
const rg = await fill('fill-rendergesture', 'FillRenderGesture', GC + '/{render.zig 38t, selection_gesture.zig 19t}',
  'Fill render.zig (the renderer-facing C surface, 38t) + selection_gesture.zig (19t). 57 tests. Commit "rustty: vt-c render + selection_gesture + 57 tests (Stage B v2)".')

phase('FillIoTypes')
const io = await fill('fill-iotypes', 'FillIoTypes', GC + '/{key_encode.zig 7t, mouse_encode.zig 7t, osc.zig 3t, formatter.zig 11t, types.zig 2t}',
  'Fill key_encode/mouse_encode/osc/formatter (the terminal-coupled io C wrappers) + types.zig (the ~29 C-ABI extern-struct type registry + its 2 tests, now that all types exist). 30 tests. Commit "rustty: vt-c key_encode/mouse_encode/osc/formatter/types + 30 tests (Stage B v2)".')

phase('HeaderVerify')
const verify = await robust(RULES + 'Fill phases: ' + JSON.stringify([skel, t, tr, k, rg, io].map(s => s && s.commit)) + '\n'
  + 'TASK (export surface + header + FINAL VERIFY — default RED; completes vt-c). (1) Audit rustty_* exports vs lib_vt.zig`s 168 @export lines — every ghostty_<name> has #[no_mangle] pub extern "C" fn rustty_<name>; confirm NO todo!() remains in any c/ wrapper. (2) cbindgen header-gen (xtask header-gen) -> include/rustty.h; C/C++ self-consistency compile gate (clang -xc + -xc++, double-include) per .port/phase-1/cbindgen-identity.md (cbindgen_header_ok + c_cpp_compile_ok); resolve the parse_deps cross-crate-type emission noted in the foundation. (3) Add a "vt_c" mapping to ' + GATES + '/check_test_counts.py (c/*.zig vs rustty-vt-c/src) -> `python3 ' + GATES + '/check_test_counts.py` GREEN incl vt_c (~307, count_gate_ok). (4) cargo test -p rustty-vt-c (all 307 pass); confirm ScrollViewport size_of==16 + Action size_of==128 const-asserts present + padding PINNED literal (abi_asserts_present + padding_pinned). (5) cargo +nightly miri test -p rustty-vt-c ~4 tests ISOLATED (0 UB/leak); `bash ' + GATES + '/sanitizers.sh rustty-vt-c "" address` (ASan clean). (6) clippy --all-targets -D + fmt --check + check_acyclic.sh. (7) G3b all 28 c/ files missing=0. (8) DEFERRALS.md: ScrollViewport + stream Action c_union RESOLVED. Spot-check 3 tests (scroll_viewport TaggedUnion round-trip, a grid_ref/selection call, a kitty_graphics call) faithful, not tautological. Verdict GREEN only if count_gate_ok(307) + abi_asserts_present + padding_pinned + cbindgen_header_ok + c_cpp_compile_ok + all pass + Miri/ASan + g3b=0 + clippy/fmt/acyclic + NO todo!() left. Commit "rustty: vt-c export surface + cbindgen header + ABI gates — vt-c COMPLETE (307 tests, Stage B)". Return verdict + vtc_test_count + count_gate_ok + abi_asserts_present + cbindgen_header_ok + c_cpp_compile_ok + miri_subset_clean + asan_clean + g3b_missing + padding_pinned + tautological_tests + blockers.',
  { schema: VERIFY, effort: 'high', label: 'headerverify', phase: 'HeaderVerify' })
return { skel, t, tr, k, rg, io, verify }
