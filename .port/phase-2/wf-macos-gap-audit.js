export const meta = {
  name: 'macos-gap-audit',
  description: 'Audit EVERY deferred/offscreen-only/unimplemented gap on the live macOS-app critical path (launch → render → type → resize → close), so the full scope to a fully-functional windowed terminal is known before fixing',
  whenToUse: 'After finding W-ATTACH (blank window) — map all remaining gaps, not one screenshot at a time',
  phases: [
    { title: 'Audit', detail: '6 parallel auditors over render / reachable-seams / font / input-actions / lifecycle / event-loop' },
    { title: 'Synthesize', detail: 'prioritized gap list + fix plan for a fully-functional macOS app' },
  ],
}

const GAPS = {
  type: 'object',
  required: ['area', 'coverage', 'gaps'],
  properties: {
    area: { type: 'string' },
    coverage: { type: 'string', description: 'what code paths you traced; honest about what you did/didn\'t reach' },
    gaps: {
      type: 'array',
      items: {
        type: 'object',
        required: ['severity', 'title', 'file', 'detail', 'reachability'],
        properties: {
          severity: { type: 'string', enum: ['blocker', 'high', 'medium', 'low'], description: 'blocker = breaks a fully-functional terminal (no render/no input/crash on normal use); high = major feature broken; medium/low = polish' },
          title: { type: 'string' },
          file: { type: 'string', description: 'file:line' },
          detail: { type: 'string', description: 'what is deferred/stubbed/no-op + what it should do (cite the preserved Zig body / ghostty reference if present)' },
          reachability: { type: 'string', description: 'is it reached on the NORMAL macOS app path (launch/render/type/resize/close)? ALWAYS / ON-FEATURE (splits/clipboard/etc.) / NEVER — and does it crash (panic=abort) or silently no-op?' },
          fix_sketch: { type: 'string' },
          deferral_ref: { type: 'string', description: 'DEFERRALS.md entry / W-ATTACH / Task-N if already tracked' },
        },
      },
    },
  },
}

const COMMON = `
CONTEXT: rustty is a full-fidelity Zig→Rust port of Ghostty. A macOS Swift app (macos/) now LINKS + LAUNCHES against rustty's core: window opens, shell spawns, keystrokes reach the shell — BUT THE WINDOW IS BLANK (no terminal content renders). Root cause of the blank: the Metal renderer is built via Metal::init_offscreen (generic.rs:1840, layer=None → present() no-ops); the windowed Metal::init(view) host-attach exists but the renderer_seam W3 wiring never routes the NSView into it (tracked: DEFERRALS W-ATTACH / plan Task 23). The shipped profile is panic=abort, so any REACHED unimplemented!() ABORTS (crashes) rather than no-ops.
YOUR JOB: audit YOUR area for EVERY deferred / offscreen-only / stubbed / unimplemented gap on the LIVE macOS-app critical path, so we know the FULL scope to a fully-functional windowed terminal before fixing. This is READ-ONLY: grep/read only, do NOT edit/build/commit. Oracle /Users/gael/projects/ghostty @ 4749c4e9 (read-only) — cite the reference where a stub preserves the Zig body as a comment.
KEY SIGNALS to hunt: unimplemented!()/todo!() REACHABLE on the normal path (launch/render/type/resize/close/scroll/select); comments like "offscreen", "never exercised on-screen", "no-op", "windowing-host seam", "deferred", "W1-7 DIVERGENCE", "golden/test only", "stub"; init_offscreen vs windowed init; no-op present/draw; DEFERRALS.md entries touching your area.
For each gap: severity (blocker=no fully-functional terminal), file:line, what it should do, REACHABILITY (always/on-feature/never + crash-vs-noop), fix sketch, and any existing deferral ref. Be exhaustive; a missed blocker means another blank-screenshot surprise. Return via StructuredOutput.`

phase('Audit')
const auditors = [
  { key: 'render-pipeline', prompt: `${COMMON}
AREA: the full RENDER pipeline — surface → render Thread → drawFrame → present, on the LIVE windowed path. Trace: (1) W-ATTACH itself — the renderer_seam::Renderer::init / generic.rs:1840 init_offscreen vs the windowed Metal::init(view); how/where the NSView should be routed (the W3 wiring gap) and everything downstream that depends on layer=Some. (2) The render THREAD: is it actually spawned and running a draw loop for a windowed surface? loop_enter / CVDisplayLink / displayCallback — wired or stubbed? (crates/rustty-renderer/src/{generic.rs,thread.rs,metal/metal_impl.rs}, rustty-core/src/surface.rs). (3) drawFrame: does it rebuild cells + encode GPU commands + present, or is any of it offscreen-only/no-op? (4) present/swap-chain/setSurface: the layer=None no-op arm and everything gated on Some. (5) set_display_id → does the display callback actually drive frames once wired? Enumerate every gap that stops pixels reaching the window AND everything that would break AFTER W-ATTACH is wired.` },
  { key: 'reachable-seams', prompt: `${COMMON}
AREA: REACHABILITY of every live unimplemented!()/todo!() on the macOS app path. Enumerate all unimplemented!()/todo!() in crates/rustty-{apprt-c,core,renderer,xev,font,font-shaper,termio,vaxis} (grep -rn). For EACH, determine: is it REACHED during normal macOS app operation — launch, first render, typing, output, scroll, selection, resize, content-scale change, color-scheme change, window close, quit? Since the shipped lib is panic=abort, a REACHED one CRASHES the app. Classify each: ALWAYS-reached (blocker — app crashes on that action) / ON-FEATURE (splits, tabs, quicklook, inspector, clipboard — high if the Swift app triggers it during normal use) / NEVER (safe, deferred correctly). Cross-ref DEFERRALS.md. The output is the list of "what will crash the app when the user does X".` },
  { key: 'font-glyph', prompt: `${COMMON}
AREA: FONT / glyph / text rendering — does rustty actually rasterize glyphs and get them into the GPU atlas for on-screen display, or is any of it offscreen-only/stubbed/deferred? Trace: font discovery (does it find a real system font on macOS, or is CoreText/fontconfig discovery deferred? rustty-font-shaper had "Linux fontconfig deferred" — check the macOS CoreText path), glyph rasterization (freetype/coretext), the glyph atlas upload to Metal, shaping (harfbuzz), the sprite/box-drawing font, cursor glyph. If fonts don't rasterize or the atlas isn't uploaded on the live path, the terminal renders blank even AFTER W-ATTACH. Cite file:line + reference. rustty-font, rustty-font-shaper, rustty-renderer glyph/atlas paths.` },
  { key: 'input-actions', prompt: `${COMMON}
AREA: INPUT + the ghostty_* ACTION callbacks the Swift app invokes. (1) Key/mouse encoding: keystrokes reach the shell (verified) — but is the full input path (key_encode, mods, mouse, scroll, IME/preedit) wired or are parts stubbed? (2) The action callback: the Swift app calls back via the action fn-ptr for MANY actions (set_title works). Enumerate the ghostty_action_* the Swift app triggers during NORMAL use (render_inspector, set_title, mouse_shape, mouse_visibility, clipboard, ring_bell, desktop_notification, cell_size, resize, initial_size, ...) and check each rustty performAction arm: wired / no-op / unimplemented!() (crash). (3) The reverse: exports the Swift calls (surface_key/mouse_button/scroll/text/preedit/set_focus/...) — wired or stubbed? rustty-core surface.rs performAction + rustty-apprt-c. A stubbed common action = broken or crashing app.` },
  { key: 'lifecycle-resize-color', prompt: `${COMMON}
AREA: surface LIFECYCLE + resize/scale/color/config on the live path. Check each is wired (not no-op/unimplemented): ghostty_surface_set_size (window resize → grid resize → renderer resize), set_content_scale (Retina/HiDPI — CRITICAL: wrong scale = blurry or 0-size render), set_color_scheme (light/dark), the config → renderer (colors/theme/font-size/padding), surface_new init sizing (the 800x600 default + first size), needs_confirm_quit/close. A broken set_content_scale or set_size on the live path can produce a blank/0-size/garbage render even with W-ATTACH wired. rustty-apprt-c + rustty-core surface.rs + rustty-renderer.` },
  { key: 'event-loop-io', prompt: `${COMMON}
AREA: the EVENT LOOP (xev) + termio + the render/io thread orchestration on the live macOS path. The shell spawns and echoes (termio works) — but audit the FULL loop: (1) xev (rustty-xev) — is the kqueue/mach event loop fully driving on the app path, or are parts stubbed (14 xev seams noted earlier — which are reached)? (2) the render thread's event loop (wakeup/mailbox/timer) — does it actually tick and request frames? (3) termio read/write threads feeding the terminal screen that the renderer reads. (4) the coupling: PTY output → terminal screen → renderer dirty → drawFrame → present. Any broken link here means output doesn't reach the screen. Which links are wired vs stubbed/deferred? rustty-xev, rustty-termio, rustty-core.` },
]
const results = await parallel(auditors.map(a => () => agent(a.prompt, { label: `audit:${a.key}`, phase: 'Audit', schema: GAPS })))
const good = results.filter(Boolean)
const allGaps = good.flatMap(r => r.gaps.map(g => ({ ...g, area: r.area })))
const blockers = allGaps.filter(g => g.severity === 'blocker')
log(`Audit: ${good.length}/${auditors.length} areas, ${allGaps.length} gaps (${blockers.length} blockers)`)

phase('Synthesize')
const synth = await agent(`${COMMON.split('YOUR JOB')[0]}
You are synthesizing a macOS-app gap audit into an actionable scope + fix plan. Inputs: ${good.length} auditors' gap lists (JSON below).

GAPS:
${JSON.stringify(good, null, 1)}

Write /Users/gael/projects/rustty/.port/phase-2/MACOS_APP_GAPS.md:
1. **Verdict** — one paragraph: how far is the app from a fully-functional windowed terminal, and the critical-path blockers in order.
2. **Blocker chain** — the ordered list of gaps that MUST be fixed for pixels-on-screen + no-crash normal use (W-ATTACH first, then whatever the render pipeline needs after it, then any ALWAYS-reached unimplemented!() that would crash). For each: file:line, what to wire, fix sketch, dependency order.
3. **High/medium/low ledger** — on-feature crashes (splits/clipboard/etc.), polish, grouped.
4. **Fix plan** — a phased sequence (P1 = get pixels rendering + no crash on launch/type/resize/close; P2 = common features; P3 = polish), noting what parallelizes.
5. **Honest scope estimate** — is a fully-functional terminal a small wiring fix (W-ATTACH + a few) or a substantial chunk (whole render loop unported)?
Then return a <=400-word executive summary: the blocker chain, the scope estimate, and whether W-ATTACH alone gets visible pixels.`,
  { label: 'synthesize', phase: 'Synthesize' })

return { areas: good.length, gaps: allGaps.length, blockers: blockers.length, summary: synth, doc: '.port/phase-2/MACOS_APP_GAPS.md' }
