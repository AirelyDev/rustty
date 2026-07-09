export const meta = {
  name: 'full-gap-analysis',
  description: 'Leave-no-stones gap analysis: map every ghostty subsystem to rustty status, classify by blocker-class + reuse-vs-port, catch untracked gaps, synthesize a phased completion roadmap',
  phases: [
    { title: 'Inventory', detail: 'per-subsystem ghostty->rustty status sweep (14 agents)' },
    { title: 'Completeness', detail: 'adversarial sweeps: missed files, orphan seams, ledger drift' },
    { title: 'Synthesize', detail: 'merge into a categorized master map + phased roadmap' },
  ],
}

const GHOSTTY = '/Users/gael/projects/ghostty'
const RUSTTY = '/Users/gael/projects/rustty'
const DEFERRALS = `${RUSTTY}/.port/phase-2/DEFERRALS.md`

// ---- shared JSON Schemas -------------------------------------------------
const UNIT = {
  type: 'object',
  additionalProperties: false,
  required: ['ghostty_path', 'status', 'kind', 'blocker_class', 'unblock', 'loc', 'outstanding_tests', 'notes'],
  properties: {
    ghostty_path: { type: 'string', description: 'ghostty source path relative to repo root' },
    rustty_path: { type: 'string', description: 'rustty counterpart path (empty string if none)' },
    status: { enum: ['done', 'partial', 'stub', 'missing', 'reuse-nonzig'], description: 'done=fully ported+tested; partial=some ported, seams remain; stub=skeleton/unimplemented!() only; missing=no rustty counterpart at all; reuse-nonzig=non-Zig source reusable as-is (Swift/C/C++/GLSL/XML)' },
    kind: { enum: ['zig', 'swift', 'c', 'cpp', 'objc', 'glsl', 'metal', 'xml-blp', 'python', 'other'] },
    blocker_class: { enum: ['none', 'macos-windowing', 'macos-swift-app', 'gtk', 'gui-inspector', 'tui-cli', 'platform-linux', 'platform-windows', 'runtime-thread', 'unused-by-ghostty', 'dependency', 'font-platform'], description: 'why it is not yet done; none=feasible headless work still open OR already done' },
    unblock: { type: 'string', description: 'concretely what is required to complete this (env, dep, tier) — 1 phrase' },
    loc: { type: 'number', description: 'approx Zig LOC to port (0 for done/reuse-nonzig)' },
    outstanding_tests: { type: 'number', description: 'approx number of ghostty test{} blocks not yet ported for this unit' },
    notes: { type: 'string', description: 'terse: key seam names, reuse strategy, or evidence' },
  },
}
const INVENTORY_SCHEMA = {
  type: 'object',
  additionalProperties: false,
  required: ['subsystem', 'summary', 'overall_status', 'units', 'missed_or_untracked'],
  properties: {
    subsystem: { type: 'string' },
    summary: { type: 'string', description: '3-5 sentence state of this subsystem: what is done, what remains, dominant blocker-class' },
    overall_status: { enum: ['complete', 'mostly-done', 'partial', 'skeleton', 'not-started'] },
    units: { type: 'array', items: UNIT },
    missed_or_untracked: { type: 'array', items: { type: 'string' }, description: 'ghostty files/features in this subsystem with NO rustty counterpart AND not tracked in DEFERRALS.md — the untracked gaps' },
  },
}

const commonRules = `
REFERENCE (read-only): ghostty Zig 1.3.2-dev at ${GHOSTTY}. Port target: rustty at ${RUSTTY} (crates/ and crates-sys/).
The authoritative deferral ledger is ${DEFERRALS} — grep it for your subsystem's entries to learn WHY things were deferred and what unblocks them.
Method: use Glob/Grep/Bash(ls,wc -l,grep -c) to enumerate ghostty files and count LOC + 'test {' blocks; spot-Read rustty crate files to confirm ported-vs-stub (look for unimplemented!()/todo!() = stub/partial, real bodies + #[test] = done). Do NOT read every file top-to-bottom — sample enough to classify each unit accurately.
CRITICAL classification rules:
- kind 'swift'/'c'/'cpp'/'glsl'/'metal'/'xml-blp' + status 'reuse-nonzig' when the source is NOT Zig and could be linked/reused as-is (the user WILL reuse non-Zig code). Note the reuse strategy + what C-ABI/glue it needs.
- blocker_class 'none' means either already DONE or feasible-headless-and-still-open. Everything not 'none' names the specific gate: macos-windowing (live NSView/Metal layer), macos-swift-app (the Swift AppKit frontend), gtk, gui-inspector (dcimgui), tui-cli (vaxis/zf TUI), platform-linux, platform-windows, runtime-thread (needs render/io threads spinning), font-platform (coretext/fontconfig/freetype face backends), unused-by-ghostty, dependency.
- Be exhaustive: EVERY ghostty file in your scope becomes a unit (or is explicitly folded into a sibling unit's notes). Leave no file unaccounted.
Return ONLY the structured object.`

// ---- 14 inventory subsystems --------------------------------------------
const SUBSYSTEMS = [
  { key: 'app-core', scope: 'ghostty src/App.zig, src/Surface.zig, src/Command.zig, src/global.zig, src/main*.zig (all entry points), src/quirks.zig, src/file_type.zig, src/math.zig, src/fastmem.zig, src/tripwire.zig, src/surface_mouse.zig, src/pty.zig, src/pty.c', rustty: 'rustty-core (App/Surface/Command), rustty-global, rustty bin, rustty-lib' },
  { key: 'apprt-embedded', scope: 'ghostty src/apprt/embedded.zig, src/apprt/surface.zig, src/apprt/action.zig, src/apprt/structs.zig, src/apprt/runtime.zig, src/apprt/none.zig, src/apprt/browser.zig, src/apprt/ipc.zig, src/apprt.zig', rustty: 'rustty-apprt, rustty-apprt-c, rustty-apprt-api' },
  { key: 'apprt-gtk', scope: 'ghostty src/apprt/gtk/** (ALL files+subdirs: App.zig, Surface.zig, class/, ui/, winproto/, ipc/, portal/, css/, key.zig, media.zig, flatpak.zig, cgroup.zig, gsettings.zig, adw_version.zig, gtk_version.zig, and the .blp/xml UI blueprints)', rustty: 'likely NONE yet — gtk frontend not started; check crates for any gtk crate' },
  { key: 'macos-swift-app', scope: 'ghostty macos/Sources/** (the Swift AppKit/SwiftUI frontend: App/, Features/, Ghostty/, Helpers/) and macos/*.plist/entitlements/xcodeproj. Also macos/GhosttyKit.xcframework', rustty: 'NONE (Swift, not ported). Assess REUSE: what libghostty C-ABI symbols the Swift app consumes (grep for ghostty_ in Swift), whether the Swift app can link librustty as-is or with a bridging shim, and what identity-ghostty/symbol-name adaptation R2 implies.' },
  { key: 'renderer', scope: 'ghostty src/renderer/** (Metal.zig, OpenGL.zig, WebGL.zig, generic.zig, Thread.zig, State.zig, Options.zig, Overlay.zig, backend.zig, cell.zig, cursor.zig, image.zig, link.zig, message.zig, row.zig, shadertoy.zig, size.zig, metal/, opengl/, shaders/)', rustty: 'rustty-renderer, rustty-render-types' },
  { key: 'os-platform', scope: 'ghostty src/os/** (ALL: TempDir, args, cf_release_thread, cgroup, dbus, desktop, env, file, flatpak, homedir, hostname, i18n, i18n_locales, kernel_info, locale, mach, macos, main, mouse, open, passwd, path, pipe, resourcesdir, shell, string_encoding, systemd, uri, windows, xdg, wasm)', rustty: 'rustty-os' },
  { key: 'termio', scope: 'ghostty src/termio/** (Exec.zig, Options.zig, Termio.zig, Thread.zig, backend.zig, mailbox.zig, message.zig, shell_integration.zig, stream_handler.zig) and src/shell-integration/', rustty: 'rustty-termio' },
  { key: 'font', scope: 'ghostty src/font/** (Atlas, CodepointMap, CodepointResolver, Collection, DeferredFace, Glyph, Metrics, SharedGrid, SharedGridSet, backend, discovery, embedded, face/ (coretext+freetype+web_canvas), glyf_rasterize, library, opentype/, shape.zig, shaper/ (coretext/harfbuzz/web_canvas/run/noop), sprite/, nerd_font_*)', rustty: 'rustty-font, rustty-font-shaper' },
  { key: 'cli', scope: 'ghostty src/cli/** (action, args, CommaSplitter, Pager, boo, crash_report, diagnostics, edit_config, explain_config, ghostty, help, list_actions, list_colors, list_fonts, list_keybinds, list_themes, new_window, show_config, show_face, ssh, ssh_cache, ssh-cache/, toggle_quick_terminal, tui, validate_config, version)', rustty: 'rustty-cli, rustty-args, rustty-cfg-fmt, rustty-helpgen' },
  { key: 'inspector', scope: 'ghostty src/inspector/** (Inspector.zig, main.zig, units.zig, widgets/, widgets.zig) and the dcimgui integration', rustty: 'rustty-inspector' },
  { key: 'config-input', scope: 'ghostty src/config/** (Config.zig, all value-types, formatter, edit, theme, ...) and src/input/** (all: keybind, key, mouse, Binding, link, ...)', rustty: 'rustty-config, rustty-input, rustty-cfg-fmt' },
  { key: 'terminal-vt', scope: 'ghostty src/terminal/** (ALL — Screen, Page, PageList, Terminal, parser, stream, csi/osc/dcs/apc handlers, kitty/, sgr, color, selection, ...)', rustty: 'rustty-vt, rustty-vt-c, rustty-unicode, rustty-simd, rustty-datastruct', hint: 'This subsystem is LARGE and reportedly mostly DONE. Do NOT emit one unit per file — that overflows the output and fails validation. Instead group into at most ~18 CLUSTER units (e.g. "terminal/page+Screen+PageList", "terminal/Terminal.zig+stream", "terminal/Parser+csi/osc/dcs/apc handlers", "terminal/kitty/*", "terminal/sgr+color+selection+charsets", "terminal/tmux+modes+search"). Each unit ghostty_path = the cluster glob. Confirm ported-vs-stub by spot-grepping rustty-vt for unimplemented!()/todo!() and comparing test{} counts at cluster level. Keep every notes field to <=1 short sentence.'},
  { key: 'deps-runtime', scope: 'vendored deps: libxev (ghostty pkg/ or build.zig.zon dep) -> rustty-xev; z2d -> rustty-z2d; the zig std shims -> rustty-zigstd; oniguruma -> rustty-oniguruma; wuffs -> rustty-wuffs. Assess which parts ghostty actually EXERCISES vs unused subset.', rustty: 'rustty-xev, rustty-z2d, rustty-zigstd, rustty-oniguruma, rustty-wuffs, rustty-synthetic' },
  { key: 'misc-support', scope: 'ghostty src/crash/**, src/build/**, src/build_config.zig, src/benchmark/**, src/main_bench.zig, src/synthetic/**, src/terminfo/**, src/simd/**, src/unicode/**, src/datastruct/**, src/extra/**, src/lib/, src/lib_vt.zig, src/stb/, src/renderer/shaders (if not covered)', rustty: 'rustty-crash, rustty-build-config, rustty-bench, rustty-synthetic, rustty-terminfo, rustty-simd, rustty-unicode, rustty-datastruct, rustty-util, rustty-identity' },
]

phase('Inventory')
const invRaw = await parallel(SUBSYSTEMS.map((s) => () =>
  agent(
    `You are auditing the "${s.key}" subsystem of the ghostty->rustty port for a leave-no-stones gap analysis.\n\nGHOSTTY SCOPE: ${s.scope}\nRUSTTY COUNTERPART(S): ${s.rustty}\n${commonRules}${s.hint ? '\n\n' + s.hint : ''}`,
    { label: `inv:${s.key}`, phase: 'Inventory', agentType: 'Explore', schema: INVENTORY_SCHEMA },
  )
))
const inventories = invRaw.map((r, idx) => (r && r.subsystem)
  ? r
  : { subsystem: SUBSYSTEMS[idx].key, summary: 'AGENT FAILED — resume to retry', overall_status: 'partial', units: (r && r.units) || [], missed_or_untracked: ['<agent-failed>'] })

// ---- completeness critics (barrier: they need the full inventory) --------
phase('Completeness')
const invBlob = JSON.stringify(inventories.map((i) => ({ subsystem: i.subsystem, files: (i.units || []).map((u) => u.ghostty_path), missed: i.missed_or_untracked })))

const CRITIC_SCHEMA = {
  type: 'object', additionalProperties: false,
  required: ['critic', 'findings', 'verdict'],
  properties: {
    critic: { type: 'string' },
    verdict: { enum: ['clean', 'gaps-found'] },
    findings: { type: 'array', items: { type: 'object', additionalProperties: false, required: ['item', 'issue', 'severity'], properties: {
      item: { type: 'string' }, issue: { type: 'string' }, severity: { enum: ['blocker', 'notable', 'minor'] } } } },
  },
}

const critics = await parallel([
  () => agent(
    `Completeness critic A — MISSED FILES. The inventory below claims to cover these ghostty files per subsystem:\n${invBlob}\n\nEnumerate EVERY ghostty source file under ${GHOSTTY}/src (all *.zig) AND ${GHOSTTY}/macos/Sources (all *.swift) with: \`cd ${GHOSTTY} && find src -name '*.zig' | sort\` and \`find macos/Sources -name '*.swift' | sort\`. Diff against the covered set. Report as findings ANY ghostty source file that appears in NO subsystem's file list — these are unaccounted gaps. Group trivially-related files. severity=blocker if it is a substantive un-tracked source file.`,
    { label: 'critic:missed-files', phase: 'Completeness', agentType: 'Explore', schema: CRITIC_SCHEMA },
  ),
  () => agent(
    `Completeness critic B — ORPHAN SEAMS. Run \`cd ${RUSTTY} && grep -rn 'unimplemented!\\|todo!(' crates/*/src --include='*.rs' | grep -v '#\\[cfg(test' \` to find every deferred seam in the rustty code. For each, confirm it is explained by some inventory unit's blocker_class (inventory summary: ${invBlob}). Report as findings any live unimplemented!()/todo!() seam whose reason/blocker is NOT captured in the inventory (an untracked open seam). Also flag any seam that looks feasible-headless (blocker_class would be 'none') and thus is still-open real work. severity=blocker for feasible-but-open seams.`,
    { label: 'critic:orphan-seams', phase: 'Completeness', agentType: 'Explore', schema: CRITIC_SCHEMA },
  ),
  () => agent(
    `Completeness critic C — LEDGER DRIFT. Read ${DEFERRALS} (it is large — grep its '## ' headers and read the ones marked open/RESOLVED). Cross-check its OPEN (not-RESOLVED) deferral items against the inventory (${invBlob}). Report findings where: (a) the ledger lists an OPEN item that no inventory unit reflects, or (b) the inventory marks something done/partial that the ledger still calls fully open (drift). This validates the ledger is current. severity=notable for drift, blocker for a ledger-open item missing from the inventory entirely.`,
    { label: 'critic:ledger-drift', phase: 'Completeness', agentType: 'Explore', schema: CRITIC_SCHEMA },
  ),
])

// ---- synthesis: the phased completion roadmap ----------------------------
phase('Synthesize')
const fullData = JSON.stringify({ inventories, critics: critics.filter(Boolean) })

const ROADMAP_SCHEMA = {
  type: 'object', additionalProperties: false,
  required: ['headline', 'totals', 'workstreams', 'critical_path', 'reuse_opportunities', 'open_questions'],
  properties: {
    headline: { type: 'string', description: '3-6 sentences: honest overall state of the port + what "complete" requires' },
    totals: { type: 'object', additionalProperties: false, required: ['done_pct_estimate', 'remaining_zig_loc_estimate', 'outstanding_tests_estimate'], properties: {
      done_pct_estimate: { type: 'number' }, remaining_zig_loc_estimate: { type: 'number' }, outstanding_tests_estimate: { type: 'number' } } },
    workstreams: { type: 'array', description: 'the completion plan grouped into coherent workstreams (macOS windowing+swift-app, GTK frontend, GUI inspector, TUI/CLI, platform Linux/Windows, runtime-thread, unused-subset, misc feasible-headless). Ordered by recommended sequence.', items: {
      type: 'object', additionalProperties: false, required: ['name', 'blocker_class', 'goal', 'key_units', 'dependencies', 'reuse_strategy', 'effort', 'risk', 'first_milestone'],
      properties: {
        name: { type: 'string' }, blocker_class: { type: 'string' }, goal: { type: 'string' },
        key_units: { type: 'array', items: { type: 'string' } },
        dependencies: { type: 'array', items: { type: 'string' }, description: 'other workstreams or env prerequisites that must land first' },
        reuse_strategy: { type: 'string', description: 'what non-Zig code is reused as-is vs what Zig must be ported' },
        effort: { enum: ['S', 'M', 'L', 'XL'] }, risk: { enum: ['low', 'med', 'high'] },
        first_milestone: { type: 'string', description: 'the concrete first shippable+testable milestone for this workstream' },
      } } },
    critical_path: { type: 'array', items: { type: 'string' }, description: 'ordered list of workstream names on the critical path to a runnable windowed rustty terminal' },
    reuse_opportunities: { type: 'array', items: { type: 'string' }, description: 'concrete non-Zig reuse wins (Swift app, dcimgui, GLSL/Metal shaders, GTK blueprints)' },
    open_questions: { type: 'array', items: { type: 'string' }, description: 'genuine forks needing the user: symbol-identity (rustty_ vs ghostty_ ABI for Swift reuse), macOS-first vs GTK-first, unused-subset port-or-skip, etc.' },
  },
}

const roadmap = await agent(
  `You are the synthesis lead for a leave-no-stones ghostty->rustty gap analysis. Below is the full per-subsystem inventory plus three completeness critics.\n\n${fullData}\n\nProduce the completion ROADMAP. The user's goal: a FULL and COMPLETE port of ghostty from Zig to Rust, REUSING non-Zig code (the Swift macOS app, dcimgui C++, GLSL/Metal shaders, GTK blueprint XML) rather than reimplementing it. Group all remaining work into coherent workstreams by blocker-class, order them by dependency/critical-path toward a runnable WINDOWED rustty terminal (macOS-first is the established strategy — see EMBEDDED_BLUEPRINT), call out every non-Zig reuse opportunity, estimate effort honestly, and surface the genuine forks that need the user's decision. Be precise and complete — this roadmap drives the entire remaining port.`,
  { label: 'synthesize:roadmap', phase: 'Synthesize', schema: ROADMAP_SCHEMA },
)

return { inventories, critics: critics.filter(Boolean), roadmap }
