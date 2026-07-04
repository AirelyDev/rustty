export const meta = {
  name: 'core-wave1',
  description: 'Core port wave 1: chunks C0-C4 of the reviewed CORE_BLUEPRINT (seam prep + type-Core decision gate, inspector Event, surface_mouse, renderer_seam placeholders, App.zig + TestRuntime + seam tests). Sequential, per-chunk commits, halt on block.',
  phases: [
    { title: 'C0-seams' },
    { title: 'C1-inspector' },
    { title: 'C2-surface-mouse' },
    { title: 'C3-renderer-seam' },
    { title: 'C4-app' },
  ],
}

const RESULT_SCHEMA = {
  type: 'object', additionalProperties: false,
  required: ['chunk', 'ok', 'tests_passing', 'commit', 'blockers', 'notes'],
  properties: {
    chunk: { type: 'string' }, ok: { type: 'boolean' },
    tests_passing: { type: 'number', description: 'tests passing in the touched crate(s)' },
    commit: { type: 'string' }, blockers: { type: 'array', items: { type: 'string' } }, notes: { type: 'string' },
  },
}

const COMMON = `OBEY the rustty PARITY CONSTITUTION + /Users/gael/projects/rustty/.port/PORT_RULES.md. R1 (every reference test ported; test COUNT is a contract), R3 (byte-faithful, every line + comment; byte-slices -> &[u8]/&str never lossy String), R5 (never weaken/skip). Reference READ-ONLY at /Users/gael/projects/ghostty (Zig 0.15.2).

FIRST read the authoritative, adversarially-reviewed design: /Users/gael/projects/rustty/.port/phase-2/CORE_BLUEPRINT.md — you implement ONE chunk of its section-6 plan. Follow its signatures, seams, [ADV-FIX] annotations, and deferral doctrine exactly; where the reference reveals unpinned detail, use faithful judgment. Also check the blueprint for a "C0 DECISION" appendix (chunks after C0 must follow it).

GATES (FOREGROUND, ALL green before commit — never commit red):
- cargo build --workspace clean; cargo test on every touched crate all pass; NO regression elsewhere (spot-check rustty-vt default, rustty-termio 44, rustty-apprt-api 20, rustty-renderer 19, rustty-crash 12)
- cargo clippy --all-targets -D warnings on touched crates; cargo fmt --check
- bash .port/tools/gates/check_acyclic.sh GREEN
- python3 .port/tools/gates/check_test_counts.py GREEN (add MAP entries your chunk owes per blueprint section 7; never fabricate)
- scoped-G3b: distinctive reference comments preserved (missing=0) for each .zig range your chunk covers
- Update .port/phase-2/DEFERRALS.md for any deferral created/resolved
COMMIT: only your files + shared gate/manifest edits. Trailer line:
Co-Authored-By: Claude Opus 4.8 (1M context)
If hard-blocked: do NOT fabricate — commit what is cleanly green, report the blocker precisely.`

const CHUNKS = [
  {
    phase: 'C0-seams', label: 'C0',
    prompt: `CHUNK C0 — seam prep (blueprint section-6 row C0; sections 1, 3, 5 give the designs). Deliverables:
1. NEW crate crates/rustty-xev: move the type definitions from crates/rustty-termio/src/xev_seam.rs verbatim (termio's module becomes \`pub use rustty_xev::*\`); add Clone+Copy to Async (Zig copies the handle). Register workspace member + deps. It may dep rustty-global only (blueprint 1.2 note: xev_seam imports rustty_global::xev — verify and keep downward-safe). This is D6's future home (libxev port lands here later) — document that in the crate doc.
2. rustty-zigstd guard-less \`thread::Mutex\` (lock/try_lock/unlock, no RAII guard; std Mutex<bool>+Condvar; no poisoning) + 2-3 rustty-only unit tests (lock/unlock across threads, try_lock, unlock-from-callee). Then retype rustty-renderer's State.mutex from std Mutex to *mut rustty_zigstd::thread::Mutex per blueprint section 3 (touches renderer state.rs + termio queueMessage/mailbox call sites — keep everything green).
3. apprt-api growth (sections 2+3): add the App trait methods (config, keyboard_layout) and Surface trait methods (type Core, core, rt_app, close, get_content_scale, get_size, get_title, supports_clipboard, clipboard_request -> Result<bool,_> [ADV-FIX 1], set_clipboard, get_cursor_pos, default_termio_env) with the blueprint's exact signatures + reference line comments. Change AppMailbox: drop type Timeout/type Size for the concrete rustty_datastruct::blocking_queue::{Timeout, Size}; push takes Message<'static> (verify all existing termio pushes stay green — blueprint pre-verified payloads are owned).
4. rustty-renderer: retype thread::DerivedConfig from uninhabited enum{} to an INHABITED struct with unimplemented!() methods [ADV-FIX 2]; add Message::initChangeConfig<C> placeholder (generic C, documented — renderer->config cycles); keep Renderer::init placeholder signature carrying alloc [ADV-FIX 3] if/where sketched.
5. Add rustty-core's missing Cargo deps [ADV-FIX 5]: rustty-util, rustty-unicode, rustty-oniguruma, rustty-datastruct, rustty-zigstd, rustty-xev (plus whatever it already has).
6. DECISION GATE: prototype the \`type Core\` self-referential bound (section 1 option 1: impl<A> ... where A::Surface: Surface<App=A, Core=Surface<A>>) in a minimal compile test inside rustty-core (a #[cfg(test)] or small module with a dummy A). If the trait solver accepts it, adopt option 1; if it fights, adopt option 2 (core_ptr() opaque cast). APPEND a "## C0 DECISION" section to /Users/gael/projects/rustty/.port/phase-2/CORE_BLUEPRINT.md recording which option + evidence, and include it in your commit — later chunks follow it.
Commit: "rustty: core C0 — xev crate, zigstd Mutex, apprt-api growth methods, mailbox Timeout/Size, type-Core decision"`,
  },
  {
    phase: 'C1-inspector', label: 'C1',
    prompt: `CHUNK C1 — rustty-inspector minimal surface (blueprint section 4 + row C1). Port into crates/rustty-inspector: ghostty src/inspector/key.zig (or widgets/key.zig — locate it) Event + EventRing (= CircBuf<Event>) + label(), and main.zig's KeyEvent re-export. PARTIAL-FILE port [ADV-FIX 7]: the ImGui-dependent section (render/imgui widgets) defers with verbatim comment bodies; only the pure-data Event/EventRing/label surface ports live (deps: rustty-input/vt KeyEvent + rustty-datastruct CircBuf; dcimgui NOT touched). Add the uninhabited \`pub enum Inspector {}\` placeholder (precedent: renderer::state::Inspector) with the Phase-3 doc. Wire inspector's Cargo deps (input+datastruct+vt as needed — acyclic). ~150 loc, 0 owed tests; scoped-G3b on the ported portion; MAP entry if the gate maps inspector files (0). Commit: "rustty: core C1 — inspector KeyEvent Event/EventRing (partial-file, ImGui deferred) + Inspector placeholder"`,
  },
  {
    phase: 'C2-surface-mouse', label: 'C2',
    prompt: `CHUNK C2 — core surface_mouse.rs (blueprint row C2). Port ghostty src/surface_mouse.zig (336 loc) in FULL into crates/rustty-core/src/surface_mouse.rs including the 1 reference test "keyToMouseShape" (13 sub-cases — one #[test] or 13, rust >= zig). Wire rustty-core's lib.rs mod. MAP entries: core_surface_mouse (1), and pre-register core_app (0) + core_surface (0) per blueprint section 7 if the gate accepts mappings for not-yet-existing rust files (if not, add them in their own chunks). Commit: "rustty: core C2 — surface_mouse (1 test)"`,
  },
  {
    phase: 'C3-renderer-seam', label: 'C3',
    prompt: `CHUNK C3 — core renderer_seam.rs placeholders (blueprint section 5 + row C3, ~200 loc). Create crates/rustty-core/src/renderer_seam.rs: inhabited \`Renderer\` placeholder with the section-5 method signatures (surfaceInit/init(alloc, opts) [ADV-FIX 3]/finalizeSurfaceInit/threadEnter/drawFrame/deinit — all unimplemented!(), documented as Phase-3 R-d dependency placeholders, NOT stubbed reference logic) + \`Thread<A: apprt_api::App>\` with the faithful renderer/Thread.zig field shape (xev handles from rustty-xev; surface *mut A::Surface; renderer *mut Renderer; state *mut renderer State; mailbox *mut renderer thread::Mailbox (REAL); app_mailbox core app::Mailbox<A> — NOTE: app.rs lands in C4; if the Mailbox type is needed first, declare the minimal app module skeleton (struct Mailbox<A>) here per the blueprint signatures and let C4 fill it, documenting the split) + Options struct + Flags + init/threadMain/deinit unimplemented!(). Follow the C0 DECISION for any type Core usage. Everything compiles workspace-green. Commit: "rustty: core C3 — renderer_seam placeholders (Renderer, Thread<A>)"`,
  },
  {
    phase: 'C4-app', label: 'C4',
    prompt: `CHUNK C4 — core app.rs: ALL of App.zig (604 loc) per blueprint section 1 signatures (App<A>, Message<A> + unsafe impl Send with the documented threading contract, NewWindow<A>, Queue<A> = BlockingQueue<Message<A>,64>, Mailbox<A> with manual Clone/Copy impls (NO A: Copy bound) [ADV-FIX 6]) + create/destroy/tick/updateConfig/addSurface/deleteSurface/focusSurface/surfaceFromMessage/hasSurface/redrawSurface/focusEvent/colorSchemeEvent/keyEvent/performAction(s)/performAllAction + the Wasm block (cfg-gated or documented-deferred as the reference build gates it). Follow the C0 DECISION for the core()/type Core edge (App.zig sites 140/193/231/285/477/502/511 [ADV-FIX 4]). NOTE Surface<A> itself lands in C5+ — where App needs the Surface type, declare the minimal surface module skeleton (struct Surface<A> with only the fields App actually touches… NO: per the blueprint the skeleton is C5's job; instead declare an opaque forward type only if unavoidable and document the split — prefer implementing App generically such that it compiles against a minimal \`pub struct Surface<A>\` stub declared in surface.rs with a doc comment saying C5 fills it; keep field access confined to what you can faithfully declare now, e.g. via methods deferred to C5+ with unimplemented!() where App calls into Surface internals). Use judgment to keep every ported App line faithful and the workspace green.
PLUS blueprint section 7's test glue: #[cfg(test)] mod test_runtime (TestApp/TestSurface implementing the apprt-api seam traits, recording perform_action keys in Mutex<Vec<_>>, counting wakeups — documented rustty-only, NOT a port) and the ~8-12 rustty-only seam tests listed there (App create/deinit; Mailbox push wakes + Size; drainMailbox dispatch incl .quit short-circuit; focusEvent dedup; colorSchemeEvent conditional-state + reload_config{soft:true} + same-scheme no-op; addSurface/deleteSurface quit_timer; deleteSurface clears focused_surface; needsConfirmQuit; keyEvent release/unfocused/app-scoped-leaf; performAllChainedAction continues past failures). Respect [ADV-FIX 2] — Surface<A>/Thread<A> must be inhabitable for these tests, or use the documented dangling-pointer discipline where construction is impossible. MAP: core_app (0 zig; rustty-only extras noted). Scoped-G3b App.zig missing=0. Commit: "rustty: core C4 — App.zig (mailbox, dispatch, seam tests via TestRuntime)"`,
  },
]

const results = []
for (const c of CHUNKS) {
  phase(c.phase)
  let r
  try {
    r = await agent(`${COMMON}\n\n${c.prompt}\n\nReturn the structured result (chunk, ok, tests_passing, commit, blockers, notes).`,
      { label: c.label, phase: c.phase, schema: RESULT_SCHEMA })
  } catch (e) {
    r = { chunk: c.phase, ok: false, tests_passing: 0, commit: '', blockers: ['threw: ' + String(e)], notes: '' }
  }
  if (!r) r = { chunk: c.phase, ok: false, tests_passing: 0, commit: '', blockers: ['agent returned null'], notes: '' }
  results.push(r)
  log(`${c.phase}: ${r.ok ? 'OK ' + r.commit + ' (' + r.tests_passing + 't)' : 'BLOCKED — halting wave'}`)
  if (!r.ok) break
}
return { results }
