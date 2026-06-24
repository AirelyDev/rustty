export const meta = {
  name: 'vt10-screen-structs',
  description: 'rustty-vt sub-batch vt-10: struct-first skeleton for the Screen-SCC (Screen+Selection+formatter+StringMap+ScreenSet). Lay down ALL ~80 cluster types/enums + method signatures as todo!() so the 4-way cycle COMPILES, breaking it the PageList way. No method bodies, no tests yet (those are vt-11). Plan: .port/phase-2/SCREEN_SCC_PLAN.md.',
  phases: [
    { title: 'Structs', detail: 'Selection->Screen->formatter->StringMap->ScreenSet types + todo!() sigs -> crate compiles' },
    { title: 'Verify', detail: 'compiles + existing 819 tests still pass + clippy/fmt/acyclic + all types present' },
  ],
}
const G = '/Users/gael/projects/ghostty', R = '/Users/gael/projects/rustty'
const T = G + '/src/terminal'
const GATES = R + '/.port/tools/gates'
const PLAN = R + '/.port/phase-2/SCREEN_SCC_PLAN.md'
const RULES = 'OBEY: PARITY CONSTITUTION + ' + R + '/.port/PORT_RULES.md. R1 = 100% fidelity (every comment verbatim on the types you define, retarget only). Reference READ-ONLY. READ THE PLAN FIRST: ' + PLAN + ' (type inventory, order, seams, risks). '
  + 'KEY FACTS: `*mut Pin` is the faithful map of Zig `*Pin` (PinPool gives stable pointers — pin.rs track_pin). Zig `[:0]const u8`/`[]const u8` -> `Vec<u8>` (NEVER lossy String); `[]const u21` -> `&[u32]`/`Vec<u32>` (codepoints, not bytes). oniguruma search surface behind `#[cfg(feature="oniguruma")]` (off by default). `TerminalFormatter` -> compiling stub (needs Terminal, vt-12). Screen `kitty_images` -> the existing deferred `kitty::graphics::ImageStorage` stub. ScreenSet `Key` is `#[repr(C)]` (primary=0, alternate=1). No other C-ABI in this cluster. Deps ready: PageList/page/style/point/size/ansi/charsets/hyperlink/sgr/color/cursor/osc/parser/stream/selection_codepoints/sys/rustty-unicode. Keep lib.rs PORT STATUS: stub. Run gates FOREGROUND. Commit trailer "Co-Authored-By: Claude Opus 4.8 (1M context)". '
async function robust(p, o) { let r = await agent(p, o); if (!r) r = await agent(p, { ...o, label: (o.label || 'a') + '-retry' }); return r }
const STEP = { type: 'object', additionalProperties: true, required: ['ok', 'blockers'], properties: { ok: { type: 'boolean' }, builds: { type: 'boolean' }, types_defined: { type: 'integer' }, commit: { type: 'string' }, notes: { type: 'string' }, blockers: { type: 'array', items: { type: 'string' } } } }
const VERIFY = { type: 'object', additionalProperties: true, required: ['verdict', 'blockers'], properties: { verdict: { type: 'string', enum: ['GREEN', 'RED'] }, compiles: { type: 'boolean' }, existing_tests_pass: { type: 'integer' }, cycle_broken: { type: 'boolean' }, missing_types: { type: 'array', items: { type: 'string' } }, notes: { type: 'string' }, blockers: { type: 'array', items: { type: 'string' } } } }

phase('Structs')
const structs = await robust(RULES +
  'TASK (struct-first skeleton — the WHOLE Screen-SCC cluster must COMPILE with todo!() method bodies). Define ALL types/enums + method SIGNATURES (bodies = todo!()) across the 5 stub files, in this order (per the plan):\n'
  + '1. crates/rustty-vt/src/selection.rs <- ' + T + '/Selection.zig: Selection, Bounds (Untracked{start,end:Pin} | Tracked{start,end:*mut Pin}), Order, Adjustment + all 14 method sigs.\n'
  + '2. crates/rustty-vt/src/screen.rs <- ' + T + '/Screen.zig: the Screen struct (fields ref 37-82) + 13 nested types (Dirty, SemanticPrompt+SemanticClick, Cursor [incl page_pin/page_row/page_cell raw ptrs], SavedCursor, CharsetState, Options, Scroll, Resize, SelectionString, SelectLine, LineIterator, PromptClickMove) + 3 inline param structs + all ~50 method sigs (todo!()). kitty_images uses the deferred ImageStorage stub.\n'
  + '3. crates/rustty-vt/src/formatter.rs <- ' + T + '/formatter.zig: ScreenFormatter + Content/Extra/Format/Options/CodepointMap/Replacement/PinMap/PageFormatter/PageListFormatter/TrailingState + method sigs. TerminalFormatter = compiling stub (PORT NOTE: needs Terminal, vt-12).\n'
  + '4. crates/rustty-vt/src/string_map.rs <- ' + T + '/StringMap.zig: StringMap{string:Vec<u8>, map:Vec<Pin>} always; SearchIterator/Match/oni gated #[cfg(feature="oniguruma")].\n'
  + '5. crates/rustty-vt/src/screen_set.rs <- ' + T + '/ScreenSet.zig: ScreenSet + #[repr(C)] Key + method sigs.\n'
  + 'Wire all 5 into lib.rs (mod decls exist). The crate must BUILD (todo!() bodies). DO NOT port method bodies or tests (vt-11). Preserve verbatim the doc/field comments on every type you define. GATES FOREGROUND: cargo build -p rustty-vt (green); clippy -p rustty-vt -- -D warnings (allow clippy::todo + dead_code on the skeleton via #[allow]); fmt --check. Commit "rustty: Screen-SCC struct-first skeleton — Screen+Selection+formatter+StringMap+ScreenSet types (vt-10)". Return ok+builds+types_defined+commit+notes(per-module type list + any forward-ref tricks)+blockers.',
  { schema: STEP, effort: 'high', label: 'structs', phase: 'Structs' })

phase('Verify')
const verify = await robust(RULES +
  'TASK (independent verify — default RED). Structs landed: ' + JSON.stringify(structs && { commit: structs.commit, blockers: structs.blockers }) + '\n'
  + 'Confirm FOREGROUND: cargo build -p rustty-vt (compiles with todo!() bodies); cargo build --workspace (no downstream break); cargo test -p rustty-vt (the existing 819 tests STILL PASS — vt-10 adds types only, no regression; new todo!() methods have no tests yet); clippy -p rustty-vt --all-targets -- -D warnings (allow todo/dead_code on skeleton); fmt --check; ' + GATES + '/check_acyclic.sh (GREEN). CYCLE BROKEN: confirm all 5 modules define their types and the 4-way Screen<->Selection<->formatter<->StringMap cycle compiles (cycle_broken). Check the type inventory vs ' + PLAN + ': all ~80 cluster types present (Selection/Bounds/Order/Adjustment; Screen + its 13 nested; ScreenFormatter + formatter types + TerminalFormatter stub; StringMap + gated search; ScreenSet + repr(C) Key). Confirm faithful skeleton: `*mut Pin` for Tracked, Vec<u8> for byte payloads, oniguruma-gated search, no tests/bodies smuggled in. Verdict GREEN only if compiles AND existing 819 pass AND no cluster type missing AND acyclic. Report verdict + compiles + existing_tests_pass + cycle_broken + missing_types + blockers.',
  { schema: VERIFY, effort: 'high', label: 'verify', phase: 'Verify' })
return { structs, verify }
