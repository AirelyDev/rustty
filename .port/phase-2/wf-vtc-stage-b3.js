export const meta = {
  name: 'vtc-stage-b3',
  description: 'Stage B v3: the vt-c mutual atom is SMALLER than v2 scoped — only terminal<->grid_ref/grid_ref_tracked/selection (+ kitty as opaque type) is mutual; render/gesture/formatter/io/sgr/types CONSUME terminal one-way and port normally after it. Done: foundation+io-leaves (60t) + cell/row/style leaves (70bdb92). Skeleton1 = the mutual atom types+sigs (todo!()) so it compiles; fill it; then port the one-way consumers normally; then header+ABI.',
  phases: [
    { title: 'Skeleton1', detail: 'mutual atom: terminal+grid_ref+grid_ref_tracked+selection types + ScrollViewport union + Effects trampoline + kitty opaque type, all fns todo!() -> compiles' },
    { title: 'FillTerminal', detail: 'terminal.zig bodies + 89 tests' },
    { title: 'FillRefsSelKitty', detail: 'grid_ref(11)+grid_ref_tracked(4)+selection(3)+kitty_graphics(35) bodies+tests (53t)' },
    { title: 'Consumers1', detail: 'render(38)+selection_gesture(19) — import terminal, port normally (57t)' },
    { title: 'Consumers2', detail: 'formatter(11)+key_encode(7)+mouse_encode(7)+osc(3)+sgr(2, Action [8]u64 union)+types(2) (32t)' },
    { title: 'HeaderVerify', detail: 'rustty_* exports + cbindgen header + C/C++ compile + ABI gates + full verify (307)' },
  ],
}
const G = '/Users/gael/projects/ghostty', R = '/Users/gael/projects/rustty'
const GC = G + '/src/terminal/c'
const GATES = R + '/.port/tools/gates'
const RSC = R + '/crates/rustty-vt-c'
const G3B = (zig, rs) => 'python3 ' + GATES + '/check_comment_coverage.py --crate rustty-vt-c --ref-files ' + zig + ' --port-glob "' + RSC + '/src/' + rs + '"'
const RULES = 'OBEY: PARITY CONSTITUTION + ' + R + '/.port/PORT_RULES.md. R1 = PORT EVERY test{} (307 c/ tests total; 60+ landed); EVERY line/comment verbatim; INDEPENDENT expected values. R2 = rustty_* exports. R3 = NEVER fabricate a placeholder C type (rustty-vt types are source of truth). R5 = NEVER weaken/stub to pass. Reference READ-ONLY ' + GC + '/. '
  + 'RECON (from the skeleton agent — these are LEGITIMATE FFI projections to author, NOT fabrications): rustty-vt EXPOSES page::Cell(u64,cval), page::Row(u64,cval), point::Coordinate(#[repr(C)]), pagelist::grow::{Scrollbar,ScrollbarC,cval}, color::RGB(=RGB.C). rustty-vt DEFERS to vt-c: point::Point::C/CValue/cval (TaggedUnion [2]u64), terminal::ScrollViewport::C (TaggedUnion [2]u64; terminal.rs:316 enum is plain), sgr::Attribute::C/Tag/CValue/Unknown::C (TaggedUnion [8]u64), color::PaletteC=[256]RGB. DONE in vt-c: foundation (allocator/build_info/color/focus/modes/result/sys/main) + io-leaves (paste/key_event/mouse_event/size_report) + cell.rs/row.rs/style.rs (70bdb92). '
  + 'D2 (SIGNED OFF): cbindgen header; the hand-authored tagged unions = `#[repr(C)] struct { tag: <fieldless repr(C) enum, c_int width>, value: <#[repr(C)] union { <variant>, _padding: [u64;N] }> }` with N PINNED LITERAL — ScrollViewport [2]u64, stream Action [16]u64 (verify the literal vs stream.zig:229 before pinning), sgr Attribute [8]u64 — + `const _: () = assert!(size_of::<Value>()==size_of::<[u64;N]>()); assert!(align_of==8); assert!(size_of::<Tag>()==size_of::<c_int>());` (BUILD-failure gate) + the verbatim "arbitrarily chosen/verify before shipping" TODO. Exports = #[no_mangle] pub extern "C" fn rustty_<name> (1:1 of lib_vt.zig 168 @export ghostty_* lines). '
  + 'SOUNDNESS (memory): raw-ptr across the C boundary (*mut Terminal, NOT &mut re-read across integrity). MIRI per MIRI_EXCEPTIONS.md ISOLATED (heavy C-boundary -> ASan). Run gates FOREGROUND. Commit trailer "Co-Authored-By: Claude Opus 4.8 (1M context)". '
async function robust(p, o) { let r = await agent(p, o); if (!r) r = await agent(p, { ...o, label: (o.label || 'a') + '-retry' }); return r }
const STEP = { type: 'object', additionalProperties: true, required: ['ok', 'blockers'], properties: { ok: { type: 'boolean' }, tests_added: { type: 'integer' }, builds: { type: 'boolean' }, commit: { type: 'string' }, notes: { type: 'string' }, blockers: { type: 'array', items: { type: 'string' } } } }
const VERIFY = { type: 'object', additionalProperties: true, required: ['verdict', 'blockers'], properties: { verdict: { type: 'string', enum: ['GREEN', 'RED'] }, vtc_test_count: { type: 'integer' }, count_gate_ok: { type: 'boolean' }, abi_asserts_present: { type: 'boolean' }, cbindgen_header_ok: { type: 'boolean' }, c_cpp_compile_ok: { type: 'boolean' }, miri_subset_clean: { type: 'boolean' }, asan_clean: { type: 'boolean' }, g3b_missing: { type: 'integer' }, padding_pinned: { type: 'boolean' }, no_todo_left: { type: 'boolean' }, tautological_tests: { type: 'array', items: { type: 'string' } }, notes: { type: 'string' }, blockers: { type: 'array', items: { type: 'string' } } } }

phase('Skeleton1')
const skel = await robust(RULES +
  'TASK (mutual-atom skeleton — MECHANICAL type+signature transcription; NO bodies, NO tests; the crate MUST compile + still pass the 60 existing tests). This is the IRREDUCIBLE mutual cluster (terminal<->grid_ref/grid_ref_tracked/selection). Lay down in rustty-vt-c/src/: terminal.rs, grid_ref.rs, grid_ref_tracked.rs, selection.rs, and a kitty_graphics.rs with at least the `KittyGraphics` OPAQUE type (terminal only exposes *ImageStorage so an opaque pointer/ZST alias suffices for compile; full kitty fills later). For EACH: define every `#[repr(C)]` extern struct/enum BYTE-EXACT (from the c/*.zig + the recon above — CGridRef, CTrackedGridRef/TrackedGridRef, CSelection + SelectWord/Between/Line option structs, the terminal wrapper structs), the ScrollViewport hand-authored [2]u64 tagged union + its const size/align asserts, the Effects raw-ptr trampoline (store `*mut TerminalWrapper` userdata — NO @fieldParentPtr — + the 9 callback fn-ptr typedefs WritePtyFn/BellFn/ColorSchemeFn/DeviceAttributesFn/EnquiryFn/XtversionFn/TitleChangedFn/PwdChangedFn/SizeFn + CDeviceAttributes {Primary{conformance_level:u16,features:[64]u16,num_features:usize},Secondary{3x u16},Tertiary{unit_id:u32}}), and EVERY extern fn as a SIGNATURE with `todo!("vt-c <name>")` body. Wire lib.rs mod tree. DO NOT defer any of these 5 files — it is types+signatures only, fully specified; complete ALL of it so the mutual graph compiles. GATES FOREGROUND: cargo build -p rustty-vt-c (compiles, todo!() ok); cargo test -p rustty-vt-c (60 still pass); fmt; check_acyclic.sh. Commit "rustty: vt-c mutual-atom skeleton (terminal/grid_ref/selection/kitty-opaque) compiles (Stage B v3)". Return ok+builds+commit+notes(every fn sig present?)+blockers.',
  { schema: STEP, effort: 'high', label: 'skeleton1', phase: 'Skeleton1' })

async function fill(label, phaseName, prompt) {
  return robust(RULES + 'Mutual-atom skeleton landed (compiles): ' + JSON.stringify(skel && skel.commit) + '\n'
    + 'TASK (fill bodies + ALL tests; crate stays compiling). ' + prompt + ' GATES FOREGROUND: cargo build -p rustty-vt-c; cargo test -p rustty-vt-c (new tests pass + no regression); Miri-subset 2 tests ISOLATED (heavy C-boundary -> ASan-cover); clippy --all-targets -D; fmt; scoped G3b missing=0. Commit. Return ok+tests_added+builds+commit+blockers.',
    { schema: STEP, effort: 'high', label, phase: phaseName })
}

phase('FillTerminal')
const t = await fill('fill-terminal', 'FillTerminal', 'Fill ' + GC + '/terminal.zig bodies (new/free/resize/reset/vt_write/mode get-set/scroll_viewport ScrollViewport dispatch/selection+grid_ref accessors/Effects trampoline via the raw-ptr userdata seam) + ALL 89 tests. Commit "rustty: vt-c terminal + 89 tests (Stage B v3)".')

phase('FillRefsSelKitty')
const rsk = await fill('fill-refsselkitty', 'FillRefsSelKitty', 'Fill ' + GC + '/{grid_ref.zig 11t, grid_ref_tracked.zig 4t, selection.zig 3t} bodies + tests, and port ' + GC + '/kitty_graphics.zig FULLY (35t — replace the opaque KittyGraphics with the real C API). 53 tests. Commit "rustty: vt-c grid_ref/selection + kitty_graphics + 53 tests (Stage B v3)".')

phase('Consumers1')
const c1 = await fill('consumers1', 'Consumers1', 'Port (full bodies+tests; they @import terminal which now exists, so NORMAL file ports — no skeleton needed) ' + GC + '/{render.zig 38t, selection_gesture.zig 19t}. 57 tests. Commit "rustty: vt-c render + selection_gesture + 57 tests (Stage B v3)".')

phase('Consumers2')
const c2 = await fill('consumers2', 'Consumers2', 'Port (full) ' + GC + '/{formatter.zig 11t, key_encode.zig 7t, mouse_encode.zig 7t, osc.zig 3t, sgr.zig 2t (incl the sgr Attribute [8]u64 hand-authored tagged union + const-asserts per D2), types.zig 2t (the ~29 C-ABI extern-struct registry — LAST, now all types exist)}. 32 tests. Commit "rustty: vt-c formatter/key_encode/mouse_encode/osc/sgr/types + 32 tests (Stage B v3)".')

phase('HeaderVerify')
const verify = await robust(RULES + 'Fill phases: ' + JSON.stringify([skel, t, rsk, c1, c2].map(s => s && s.commit)) + '\n'
  + 'TASK (export surface + header + FINAL VERIFY — default RED; completes vt-c). (1) Audit rustty_* exports vs lib_vt.zig 168 @export lines; confirm NO todo!() remains anywhere in rustty-vt-c (grep). (2) cbindgen header-gen (xtask header-gen) -> include/rustty.h + C/C++ self-consistency compile gate (clang -xc + -xc++, double-include) per .port/phase-1/cbindgen-identity.md; resolve parse_deps cross-crate type emission. (3) Add "vt_c" mapping to ' + GATES + '/check_test_counts.py (c/*.zig vs rustty-vt-c/src) -> full gate GREEN incl vt_c (~307, count_gate_ok). (4) cargo test -p rustty-vt-c (all ~307 pass); ScrollViewport size_of==16 + Action size_of==128 + sgr Attribute [8]u64 const-asserts present + padding PINNED (abi_asserts_present + padding_pinned). (5) Miri-subset ~4 tests ISOLATED (0 UB/leak) + `bash ' + GATES + '/sanitizers.sh rustty-vt-c "" address` (ASan clean). (6) clippy --all-targets -D + fmt --check + check_acyclic.sh. (7) G3b all 28 c/ files missing=0. (8) DEFERRALS.md: ScrollViewport + stream Action + sgr Attribute C-ABI RESOLVED. Spot-check 3 tests faithful. Verdict GREEN only if count_gate_ok(307) + abi_asserts_present + padding_pinned + cbindgen_header_ok + c_cpp_compile_ok + no_todo_left + all pass + Miri/ASan + g3b=0 + clippy/fmt/acyclic. Commit "rustty: vt-c export surface + cbindgen header + ABI gates — vt-c COMPLETE (307 tests, Stage B)". Return verdict + vtc_test_count + count_gate_ok + abi_asserts_present + cbindgen_header_ok + c_cpp_compile_ok + miri_subset_clean + asan_clean + g3b_missing + padding_pinned + no_todo_left + tautological_tests + blockers.',
  { schema: VERIFY, effort: 'high', label: 'headerverify', phase: 'HeaderVerify' })
return { skel, t, rsk, c1, c2, verify }
