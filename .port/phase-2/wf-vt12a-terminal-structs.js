export const meta = {
  name: 'vt12a-terminal-structs',
  description: 'rustty-vt vt-12a: struct-first skeleton for Terminal + stream_terminal (the last giant, mutual cycle). Lay down the Terminal struct + 8 nested types + Flags + GlyphGlossary stub + 66 method sigs, AND the stream_terminal Handler struct + Effects + Stream alias + impl stream::Handler (all ~93 methods todo!()), so both files COMPILE. No method bodies, no tests (vt-12b/c). Plan: .port/phase-2/TERMINAL_PLAN.md.',
  phases: [
    { title: 'Structs', detail: 'Terminal + stream_terminal types + ~160 todo!() sigs -> both files compile' },
    { title: 'Verify', detail: 'compiles + existing 992 tests pass + clippy/fmt/acyclic + all types present' },
  ],
}
const G = '/Users/gael/projects/ghostty', R = '/Users/gael/projects/rustty'
const T = G + '/src/terminal'
const GATES = R + '/.port/tools/gates'
const PLAN = R + '/.port/phase-2/TERMINAL_PLAN.md'
const RULES = 'OBEY: PARITY CONSTITUTION + ' + R + '/.port/PORT_RULES.md. R1 = 100% fidelity (every comment verbatim on the types you define, retarget only). Reference READ-ONLY ' + T + '/{Terminal.zig,stream_terminal.zig}. READ THE PLAN FIRST: ' + PLAN + '. '
  + 'KEY FACTS: Handler.terminal is `*mut Terminal` (raw ptr, NOT &mut — Stream owns Handler which aliases Terminal; mirrors screen_set::ScreenSet `*mut Screen`); access via `active_mut()`/`unsafe{&mut *self.terminal}`. Zig `[:0]const u8`/ArrayList(u8) byte payloads (pwd/title) -> Vec<u8> with trailing-0 sentinel (NEVER lossy String); getters return Option<&[u8]> (NUL-stripped). apc/glyph DEFERRED (font-blocked): apc_handler + GlyphGlossary + glyphProtocol are ZST/no-op STUBS (continuing deferral — record in DEFERRALS.md). ScrollViewport is a plain Rust enum (its TaggedUnion C-ABI -> rustty-vt-c, deferred). Effects = 9 Option<fn ptr>, default readonly=all None. kitty graphics forwards to Screen.kitty_images (deferred ImageStorage stub). Deps ready: Screen/ScreenSet/Selection/formatter/StringMap/modes/charsets/Tabstops/sgr/color/osc/kitty/hyperlink/mouse/point/size/device_*/size_report + the Stream<H:Handler> trait (stream.rs:228). Keep lib.rs PORT STATUS: stub. Run gates FOREGROUND. Commit trailer "Co-Authored-By: Claude Opus 4.8 (1M context)". '
async function robust(p, o) { let r = await agent(p, o); if (!r) r = await agent(p, { ...o, label: (o.label || 'a') + '-retry' }); return r }
const STEP = { type: 'object', additionalProperties: true, required: ['ok', 'blockers'], properties: { ok: { type: 'boolean' }, builds: { type: 'boolean' }, types_defined: { type: 'integer' }, sigs_defined: { type: 'integer' }, commit: { type: 'string' }, notes: { type: 'string' }, blockers: { type: 'array', items: { type: 'string' } } } }
const VERIFY = { type: 'object', additionalProperties: true, required: ['verdict', 'blockers'], properties: { verdict: { type: 'string', enum: ['GREEN', 'RED'] }, compiles: { type: 'boolean' }, existing_tests_pass: { type: 'integer' }, cycle_broken: { type: 'boolean' }, missing_types: { type: 'array', items: { type: 'string' } }, notes: { type: 'string' }, blockers: { type: 'array', items: { type: 'string' } } } }

phase('Structs')
const structs = await robust(RULES +
  'TASK (struct-first skeleton — Terminal + stream_terminal must COMPILE with todo!() bodies). '
  + '(1) crates/rustty-vt/src/terminal.rs <- ' + T + '/Terminal.zig: the Terminal struct (14 fields per Terminal.zig:44-134) + 8 nested types (Colors, Dirty[plain struct of bools], ScrollingRegion, Options, SemanticPrompt, ScrollViewport[PLAIN enum Top/Bottom/Delta(isize)], DeccolmMode, SwitchScreenMode) + Flags + the 3-state mouse_shift_capture enum + a GlyphGlossary ZST stub (apc deferred) + ALL 66 method signatures (60 pub + 6 priv) as `todo!()` + the active_mut() helper. '
  + '(2) crates/rustty-vt/src/stream_terminal.rs <- ' + T + '/stream_terminal.zig: the Handler struct (terminal:*mut Terminal, effects:Effects, apc_handler[ZST stub], default_cursor fields) + Effects struct (9 Option<fn ptr> + readonly const) + `pub type Stream<\'a> = stream::Stream<\'a, Handler<\'a>>` + Handler::{init,deinit,vt} sigs + `impl stream::Handler for Handler` with ALL ~93 trait methods present (bodies todo!()). '
  + 'The mutual cycle (Terminal names stream_terminal types via vtStream/vtHandler; Handler names Terminal) compiles because both land together in one crate. Wire both into lib.rs. DO NOT port method bodies or tests (vt-12b/c). Preserve verbatim doc/field comments on every type. Record the apc/glyph + ScrollViewport-C-ABI stubs as deferrals in ' + R + '/.port/phase-2/DEFERRALS.md. '
  + 'GATES FOREGROUND: cargo build -p rustty-vt (green, todo!() bodies); clippy -p rustty-vt -- -D warnings (allow clippy::todo + dead_code on the skeleton via #[allow]); fmt --check. Commit "rustty: Terminal + stream_terminal struct-first skeleton (vt-12a)". Return ok+builds+types_defined+sigs_defined+commit+notes(per-file type list + the *mut Terminal seam approach)+blockers.',
  { schema: STEP, effort: 'high', label: 'structs', phase: 'Structs' })

phase('Verify')
const verify = await robust(RULES +
  'TASK (independent verify — default RED). Structs landed: ' + JSON.stringify(structs && { commit: structs.commit, blockers: structs.blockers }) + '\n'
  + 'Confirm FOREGROUND: cargo build -p rustty-vt (compiles with todo!() bodies); cargo build --workspace (no downstream break); cargo test -p rustty-vt (existing 992 STILL PASS — vt-12a adds types only, no regression; new todo!() have no tests); clippy -p rustty-vt --all-targets -- -D warnings (allow todo/dead_code on skeleton); fmt --check; ' + GATES + '/check_acyclic.sh (GREEN). CYCLE BROKEN: Terminal<->stream_terminal compiles (Handler impls stream::Handler; terminal:*mut Terminal; Terminal::vtStream/vtHandler name Stream/Handler). Check type inventory vs ' + PLAN + ': Terminal + 8 nested + Flags + mouse_shift_capture + GlyphGlossary stub; Handler + Effects + Stream alias + the ~93-method impl stream::Handler. Confirm faithful skeleton: *mut Terminal (not &mut), Vec<u8>+NUL for pwd/title, ScrollViewport plain enum, apc/glyph ZST stubs recorded in DEFERRALS.md, no tests/bodies smuggled. Verdict GREEN only if compiles AND existing 992 pass AND no type missing AND acyclic. Report verdict + compiles + existing_tests_pass + cycle_broken + missing_types + blockers.',
  { schema: VERIFY, effort: 'high', label: 'verify', phase: 'Verify' })
return { structs, verify }
