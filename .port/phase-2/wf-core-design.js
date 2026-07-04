export const meta = {
  name: 'core-design',
  description: 'Design blueprint for rustty-core (Surface.zig 6036 loc / App.zig 604 / surface_mouse 336) + adversarial verification of the blueprint against the reference and the Cargo graph',
  phases: [
    { title: 'Design' },
    { title: 'Verify' },
  ],
}

const CONTEXT = `Project: rustty — faithful Zig->Rust port of ghostty. Reference READ-ONLY at /Users/gael/projects/ghostty (Zig 0.15.2); port at /Users/gael/projects/rustty. Rules: /Users/gael/projects/rustty/.port/PORT_RULES.md (R1 test-count contract; R3 byte-faithful, every line+comment; byte-slices -> &[u8]/&str never lossy String; R5 never weaken). Binding gates: check_acyclic.sh (Tarjan over crates/*/Cargo.toml), check_test_counts.py, scoped-G3b comment coverage.

Current state (all committed, gate-green): rustty-vt (2019t crown jewel), vt-c, config (189t), font stack, input (113t), os (incl pty.rs), renderer (19t headless: cell/cursor/state/shaders/Health/shadertoy/link + message.rs + Thread::Mailbox=BlockingQueue<Message,64> + DerivedConfig placeholders), rustty-apprt-api (20t — the generic App/Surface/CoreSurfaceHandle/Target trait seam with documented GROWTH POINTs in seam.rs; AppMailbox with opaque Timeout/Size assoc types in surface.rs), rustty-crash (12t — incl. trait ThreadStateSurface {screen_size/grid_size/cell_size -> (u32,u32)} whose concrete impl awaits core's Surface), rustty-termio (44t — Options/Termio_impl/backend/mailbox/thread shapes + Command + Exec + shell_integration + full stream_handler as vt Handler impl; generic over M: AppMailbox; xev event-loop paths D6-deferred with verbatim bodies; mailbox::send caller-held-mutex pattern deferred "decided at D6/core"; stream_handler surfaceMessageWriter/rendererMessageWriter deferred on core's opaque Timeout). rustty-inspector is a 9-loc STUB (dcimgui-sys vendored; inspector logic is Phase 3). D6 = xev/libxev deferred (Zig source, will be ported as rustty-xev when an end-to-end shell milestone forces it). rustty-core is a ~9-loc stub whose Cargo.toml already deps termio/apprt-api/etc (read it).

Blueprint docs to read: /Users/gael/projects/rustty/.port/phase-2/INTEGRATION_TIER_BLUEPRINT.md (the tier just landed), /Users/gael/projects/rustty/.port/phase-2/NEXT_PHASE_PLAN.md (CORE section), /Users/gael/projects/rustty/crates/rustty-apprt-api/src/{seam.rs,surface.rs}, /Users/gael/projects/rustty/.port/phase-0/plan/PORT_PLAN.md (crate 24).`

phase('Design')
const blueprint = await agent(`${CONTEXT}

You are the ARCHITECT. Produce a CONCISE, decision-oriented implementation blueprint for porting rustty-core: src/Surface.zig (6036 loc, 0 tests), src/App.zig (604 loc, 0 tests), src/surface_mouse.zig (336 loc, 1 test). Ground every claim by READING the actual reference files and the current crates' code — do not guess. Core is weakly test-gated (1 test total), so the blueprint + G3b comment coverage + chunked review ARE the quality gate; be precise.

The blueprint must cover:
1. **Genericity strategy (the make-or-break decision).** apprt-api committed to GENERIC (not dyn) App/Surface traits, faithful to Zig's comptime single-backend. How does the type parameter thread through core? e.g. \`pub struct App<A: apprt_api::App>\` / \`pub struct Surface<A: apprt_api::App>\` holding rt_app: *mut A / rt_surface: *mut A::Surface — trace where the parameter must propagate (termio::Termio is generic over M: AppMailbox; does core's Surface own a Termio<CoreMailbox<A>>? what is the concrete mailbox type?) and where it STOPS. Flag any explosion risk (e.g. does Surface's generic force rustty-vt types to be generic? — it must NOT). Give the exact top-level type signatures.
2. **apprt-api growth points — the exact additive trait-method list.** Grep ALL \`rt_app.\`/\`rt_surface.\`/\`self.rt_\` method calls in Surface.zig + App.zig, and apprt.App/apprt.Surface usages, and produce the precise Rust signatures to ADD to apprt-api::seam's App and Surface traits (additive, R1-safe). Include performAction's concrete call shape from core's side, clipboard (setClipboard/getClipboard/clipboardRequest), cursor pos, size, title, close, defaultTermioEnv, keyboardLayout, mouse shape/visibility, IME, etc. — whatever the reference actually calls.
3. **The App Mailbox.** App.zig's Mailbox type (read it — likely BlockingQueue(Message, N)) and core's surface Mailbox pairing (&CoreSurface + App::Mailbox per the apprt-api blueprint). Resolve apprt-api's opaque AppMailbox::Timeout/Size associated types concretely, and state whether the termio deferrals this unblocks (mailbox::send unlock pattern, stream_handler surfaceMessageWriter/rendererMessageWriter) can now be un-deferred in the core effort or stay deferred (justify).
4. **Inspector seam.** What does Surface.zig actually use from inspector/main.zig (grep it)? Recommend: minimal type port into rustty-inspector stub vs a core-local trait seam vs deferral with verbatim bodies. rustty-inspector must stay Phase-3 for its real logic.
5. **Renderer/GPU-coupled paths.** Surface spawns renderer.Thread and drives the GPU renderer (Phase-3). Which Surface subsystems are GPU-coupled and get documented deferrals (verbatim bodies) vs which are portable now (renderer::State updates, size math, cell/link usage are already-ported headless surfaces)? Same for xev/event-loop touches (D6): enumerate exactly which Surface/App functions hit them.
6. **Chunked execution plan.** 6k loc in one agent pass is unreviewable. Propose the skeleton->fill chunking (the proven cluster discipline): chunk list with file/function ranges, per-chunk scoped-G3b, suggested order, rough size each. Include surface_mouse.zig (1 test) and the un-defers that land with core: crash's ThreadStateSurface concrete impl, apprt-api's newConfig + Mailbox struct, termio Timeout resolution (if per item 3).
7. **Test accounting.** core_surface_mouse (1) MAP entry + any other MAP entries; enumerate recommended rustty-only seam tests (headless: construct App<NoneRuntime>-style fake runtime? — decide whether a minimal test-only fake apprt runtime impl in core's tests is warranted and faithful, since ghostty's own none.zig exists — read apprt/none.zig and say whether porting it INTO rustty-apprt now (19 loc) as the test backend is the right move).
8. **Risks/ambiguities** with recommended resolutions.

Write the blueprint to /Users/gael/projects/rustty/.port/phase-2/CORE_BLUEPRINT.md (do NOT commit it; I will after verification) and ALSO return it as your final text.`, { label: 'architect', phase: 'Design' })

phase('Verify')
const verdict = await agent(`${CONTEXT}

You are the ADVERSARIAL REVIEWER of a core-port blueprint. The blueprint is at /Users/gael/projects/rustty/.port/phase-2/CORE_BLUEPRINT.md (uncommitted working file — read it). Your job: try to BREAK it against the actual reference and repo. Independently verify (do not trust the blueprint's claims):
1. **Cycle traps**: does any proposed placement/dep create a cycle in the real Cargo graph (run check_acyclic mentally over the proposed edges; core deps termio/apprt-api/renderer/crash/config/input/vt/os/font...)? Does the genericity plan force an illegal edge (e.g. apprt-api needing a core type)?
2. **Growth-point completeness**: grep Surface.zig/App.zig yourself for EVERY rt_app./rt_surface./apprt. usage and diff against the blueprint's trait-method list. List anything missed or with a wrong signature (esp. error types, allocator params, optional/comptime-gated methods).
3. **Genericity feasibility**: pick the 3 hairiest concrete uses (e.g. Surface stored in App's surface list, Target<'s,C> at performAction call sites, termio Termio<M> ownership + stream_handler generic chain) and check the proposed signatures actually typecheck conceptually — name any variance/lifetime/object-safety landmine.
4. **Deferral honesty**: are the proposed GPU/xev/inspector deferrals really 0-test and cleanly separable at the proposed function boundaries? Any proposed-portable function that actually pulls a GPU/xev dependency transitively?
5. **Test accounting**: verify surface_mouse.zig has exactly 1 test and the none.zig recommendation is sound (read apprt/none.zig — what does it actually contain?).
6. **Anything else that would sink the port** (missed imports like oniguruma/pty usage in Surface, the inspector grep, wasm paths in App.zig).
Do NOT edit the blueprint. Return a numbered findings list, each: CONFIRMED-ISSUE (with evidence: file:line) / FALSE-ALARM (why the blueprint is right) / RECOMMENDATION. End with an overall verdict: ACCEPT / ACCEPT-WITH-FIXES (list the fixes) / REDESIGN.`, { label: 'adversary', phase: 'Verify' })

return { blueprint_len: (blueprint || '').length, verdict }
