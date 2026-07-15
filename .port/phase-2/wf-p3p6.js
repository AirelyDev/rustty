export const meta = {
  name: 'p3p6-selfcontained-polish',
  description: 'Plan P3.1 (self-contained full zlib+gettext) + P6.1 (iOS host_view arm) + P6.2 (stale renderer doc) — disjoint from the running quicklook/apprt-c work',
  whenToUse: 'macOS full-functionality plan, disjoint batch after P0',
  phases: [{ title: 'Batch', detail: 'P3.1 self-contained lib, P6.1 iOS host_view, P6.2 stale doc — parallel' }],
}
const OUT = {
  type: 'object', required: ['task', 'status', 'evidence'],
  properties: {
    task: { type: 'string' }, status: { type: 'string', enum: ['done', 'partial', 'blocked'] },
    evidence: { type: 'string' }, committed: { type: 'boolean' }, commit_ref: { type: 'string' }, residual: { type: 'string' }, blocker: { type: 'string' },
  },
}
const COMMON = `
REPO: /Users/gael/projects/rustty. Oracle (READ-ONLY): /Users/gael/projects/ghostty @ 4749c4e9. Plan: docs/superpowers/plans/2026-07-12-macos-app-full-functionality.md. The macOS app renders. Concurrent agents own rustty-apprt-c/src/lib.rs (quicklook) — do NOT touch it. RULES: R1 fidelity; R3 self-contained (vendor C sources, same libs — full zlib/gettext, no pure-Rust sub); R5. Git: explicit paths, index.lock retry, no push/reset/stash. Return via StructuredOutput; COMMIT own files.`

phase('Batch')
const tasks = [
  {
    key: 'p3.1-selfcontained', label: 'P3.1 self-contained zlib+gettext',
    prompt: `${COMMON}
TASK P3.1: make the shipped lib self-contained so the app drops system -lz + Homebrew -lintl. Currently rustty's vendored libz.a is INFLATE-ONLY (no deflate.o; sentry needs deflate) and gettext-sys is the gnulib localename slice only (no dgettext/bindtextdomain/textdomain). (1) ZLIB: reconfigure crates-sys/zlib-sys/build.rs to compile the FULL zlib (add the deflate + compress sources it currently omits — check the build.rs source list vs the vendored zlib vendor/ dir); rebuild + nm the produced libz.a for _deflate (defined T, not U). (2) GETTEXT: extend crates-sys/gettext-sys to build the full libintl runtime (dgettext/bindtextdomain/textdomain) from its vendored gettext sources — R3 same C lib; if the full gettext-runtime is a large vendor addition, do the zlib half NOW and DEFER gettext with a precise DEFERRALS entry (what sources are needed) + keep -lintl. (3) After zlib (and gettext if done): cargo run -p xtask -- xcframework; nm the combined dist lib for the now-defined symbols; remove the corresponding flag(s) from macos/build-rustty.sh OTHER_LDFLAGS (drop -lz if zlib self-contained; drop -L/opt/homebrew…/-lintl if gettext self-contained). Do NOT break the build — verify the combined lib still has all symbols. Commit ("sys: full deflate-zlib [+ gettext] — self-contained xcframework, drop -lz [/-lintl] (P3.1)"). Report exactly what's now self-contained vs deferred.`,
  },
  {
    key: 'p6.1-ios-hostview', label: 'P6.1 iOS host_view arm',
    prompt: `${COMMON}
TASK P6.1: fix the dropped iOS uiview mapping in host_view (INTEGRATION_PARITY §2.6, §2.9; R1). crates/rustty-apprt/src/embedded.rs — the ApprtSurface::host_view() impl currently returns null for the iOS arm; ghostty's Metal.init handles both nsview (macOS) and uiview (iOS). Add the iOS arm: match &self.platform { Platform::MacOS(m) => m.nsview, Platform::IOS(i) => i.uiview } (use the actual iOS field name — read the Platform::IOS variant + the MacOS/IOS structs; MacOS is a ZST on non-darwin so the field access must be cfg-correct). If the Platform::MacOS/IOS field access does not compile off-darwin (§2.9), #[cfg(target_os=…)]-gate the arms so the fn compiles on all targets. Verify: cargo build -p rustty-apprt; and cargo check -p rustty-apprt (a non-macos target if one is installed — else note the cfg is structurally correct). Commit ("apprt: host_view iOS uiview arm + non-darwin cfg (R1 parity) (P6.1)").`,
  },
  {
    key: 'p6.2-staledoc', label: 'P6.2 stale renderer doc',
    prompt: `${COMMON}
TASK P6.2: correct the stale doc block at crates/rustty-renderer/src/generic.rs:11-38 (MACOS_APP_GAPS §3 LOW). It claims DerivedConfig::init / setScreenSize / changeConfig / setFontGrid are unimplemented seams — all four are LANDED & real, and W-ATTACH (the actual last render gap) is now fixed (commit 96b0116). Rewrite the DEFERRED note to reflect reality: those four are implemented; there is no remaining windowed-render seam (W-ATTACH resolved). Preserve any genuinely-still-deferred item. Do NOT change code, only the doc comment. Verify: cargo build -p rustty-renderer green (comment-only). Commit ("renderer: correct stale doc block — the four listed seams are landed, W-ATTACH resolved (P6.2)").`,
  },
]
const res = (await parallel(tasks.map(t => () => agent(t.prompt, { label: t.label, phase: 'Batch', schema: OUT })))).filter(Boolean)
log(`P3.1/P6: ${res.filter(r => r.status === 'done').length}/${tasks.length} done`)
return { items: res.map(r => ({ t: r.task, s: r.status, ref: r.commit_ref, residual: r.residual })), summary: res.map(r => `${(r.task||'').slice(0,20)}:${r.status}`).join(' | ') }
