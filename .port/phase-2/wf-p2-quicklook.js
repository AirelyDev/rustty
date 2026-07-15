export const meta = {
  name: 'p2-quicklook',
  description: 'Plan P2.1/P2.2: port the quicklook_word (objc-free) + quicklook_font (CTFont) features faithfully from ghostty, replacing the P2 crash-guard sentinels',
  whenToUse: 'Executes docs/superpowers/plans/2026-07-12-macos-app-full-functionality.md P2.1+P2.2',
  phases: [{ title: 'Quicklook', detail: 'P2.1 word (objc-free), P2.2 font (CTFont) — parallel' }],
}
const OUT = {
  type: 'object', required: ['task', 'status', 'evidence'],
  properties: {
    task: { type: 'string' }, status: { type: 'string', enum: ['done', 'partial', 'blocked'] },
    evidence: { type: 'string' }, committed: { type: 'boolean' }, commit_ref: { type: 'string' }, residual: { type: 'string' }, blocker: { type: 'string' },
  },
}
const COMMON = `
REPO: /Users/gael/projects/rustty. Oracle (READ-ONLY): /Users/gael/projects/ghostty @ 4749c4e9 — the src/apprt/embedded.zig quicklook exports are your reference; port them faithfully (R1: every line, comments preserved as ported comments). The macOS app renders; the quicklook features currently return the P2 crash-guard sentinel (commit a290323) instead of the real body. Only rustty-apprt-c/src/lib.rs quicklook exports are in scope; a concurrent agent owns rustty-renderer/metal + the macos_window_host test harness — do NOT touch those. RULES: R1 fidelity; panic=abort (recoverable → sentinel, asserts abort); ownership-transfer returns (Text/CTFont) follow invariants I6 (host frees via ghostty_surface_free_text) and I16 (CTFont +1 the host releases). Git: explicit paths, index.lock retry, no push/reset/stash. Return via StructuredOutput; COMMIT own files.`

phase('Quicklook')
const tasks = [
  {
    key: 'p2.1-word', label: 'P2.1 quicklook_word',
    prompt: `${COMMON}
TASK P2.1: port rustty_surface_quicklook_word (crates/rustty-apprt-c/src/lib.rs, currently the P2 sentinel ~:2705). Reference: ghostty embedded.zig ghostty_surface_quicklook_word — objc-FREE: lock renderer_state.mutex, selectWord at the cursor, readTextLocked, return a Text (ownership-transfers to the host; host frees via ghostty_surface_free_text, I6). All three (renderer_state.mutex, selectWord, readTextLocked) are already ported in rustty — call them. Replace the P2 sentinel with the faithful body + the ported Zig comments. Verify: cargo build -p rustty-apprt-c green; add/extend a KAT that drives quicklook_word over a surface with a word under the cursor and asserts the returned Text is non-null with the expected word (or, if a full surface KAT is impractical in-crate, a focused unit test of the selectWord+readTextLocked path). Remove the now-stale P2 DEFERRALS entry for quicklook_word. Commit ("apprt-c: port quicklook_word (objc-free) — Look-Up word gesture functional (P2.1)").`,
  },
  {
    key: 'p2.2-font', label: 'P2.2 quicklook_font',
    prompt: `${COMMON}
TASK P2.2: port rustty_surface_quicklook_font (crates/rustty-apprt-c/src/lib.rs, currently the P2 sentinel ~:2634). Reference: ghostty embedded.zig ghostty_surface_quicklook_font (:2179 — copyWithAttributes returns a +1 CTFontRef the host releases, I16). Port faithfully using objc2 / objc2-core-text / core-foundation CFRetained with MATCHING ownership (the returned CTFont is +1; the host releases it — do NOT over/under-retain). Main-thread-affine (I5); the returned handle is !Send. Replace the P2 sentinel with the faithful body + ported comments. Verify: cargo build -p rustty-apprt-c green (may need objc2-core-text as a dep — add it if the font-attributes API needs it; check what rustty-font already uses for CoreText). A KAT is hard (needs a real font grid + Metal) — at minimum a compile + a focused test of the CTFont retain/release balance if feasible, else document why the KAT needs the app + verify via the app in the final shakedown. Remove the stale P2 DEFERRALS entry for quicklook_font. Commit ("apprt-c: port quicklook_font (CTFont +1) — Look-Up/accessibility font query functional (P2.2)"). If the CoreText font-copy API isn't reachable without a large new binding, report partial with exactly what's needed.`,
  },
]
const res = (await parallel(tasks.map(t => () => agent(t.prompt, { label: t.label, phase: 'Quicklook', schema: OUT })))).filter(Boolean)
log(`P2 quicklook: ${res.filter(r => r.status === 'done').length}/${tasks.length} done`)
return { items: res.map(r => ({ t: r.task, s: r.status, ref: r.commit_ref, residual: r.residual })), summary: res.map(r => `${(r.task||'').slice(0,18)}:${r.status}`).join(' | ') }
