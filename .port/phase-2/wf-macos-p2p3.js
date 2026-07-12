export const meta = {
  name: 'macos-p2p3',
  description: 'The macOS milestone remaining items: verify W-ATTACH via the windowed-present KAT, P2 crash-guards (quicklook/inspector graceful returns), P3 identity rebrand + deployment-target + self-contained lib',
  whenToUse: 'After the macOS terminal renders — close out the documented follow-ups',
  phases: [
    { title: 'Items', detail: 'Verify-KAT, P2 crash-guards, P3 rebrand, P3 xtask (deptarget + self-contained) — parallel' },
  ],
}

const OUT = {
  type: 'object',
  required: ['task', 'status', 'evidence'],
  properties: {
    task: { type: 'string' }, status: { type: 'string', enum: ['done', 'partial', 'blocked'] },
    evidence: { type: 'string' }, committed: { type: 'boolean' }, commit_ref: { type: 'string' },
    residual: { type: 'string' }, blocker: { type: 'string' },
  },
}
const COMMON = `
REPO: /Users/gael/projects/rustty. Oracle /Users/gael/projects/ghostty @ 4749c4e9 (READ-ONLY, never modify). The macOS Swift app (macos/) now renders a live terminal (W-ATTACH landed, commit 96b0116). Rebuild command: bash macos/build-rustty.sh. Rules: R1 fidelity (port faithfully; preserve ghostty behavior + comments); R2 rustty identity (but C-ABI symbol names STAY ghostty_* — Branch A); R5 never weaken a gate. The shipped lib is panic=abort (assert-class aborts like Zig's trap). Git: explicit paths, index.lock retry (sleep 3, ≤10x), no push/reset/stash. Return via StructuredOutput; COMMIT own files.`

phase('Items')
const items = [
  {
    key: 'verify-kat', label: 'verify W-ATTACH via windowed-present KAT',
    prompt: `${COMMON}
TASK: verify W-ATTACH by un-ignoring + running the windowed-present KAT on this Metal host. crates/rustty-apprt-c/tests/macos_window_host.rs has KATs (e.g. hosts_view_layer_and_presents_live_frame at ~:329, set_display_id one) that were #[ignore]'d citing the (now-fixed) host-attach gap. (1) Read them; the one asserting "host view is not layer-hosting" (:329) should now PASS since attach_view does setLayer:/setWantsLayer:. Un-ignore it (remove the #[cfg_attr(miri,ignore)]/#[ignore] tied to W-ATTACH; keep any legit miri-FFI ignore). (2) Run: cargo test -p rustty-apprt-c --test macos_window_host 2>&1. (3) If it PASSES → W-ATTACH verified end-to-end; commit the un-ignore ("apprt-c: un-ignore the windowed-present KAT — passes after W-ATTACH"). If it FAILS → that's a real signal about the attach; capture the failure verbatim and report status=blocked (do NOT re-ignore to hide it, R5). (4) Also verify present-error propagation + resize: grep the present() Some-arm + surface_size Some-arm (metal_impl.rs) and confirm they're live (behavioral read, not a run). Report the KAT result verbatim.`,
  },
  {
    key: 'p2-crashguards', label: 'P2 crash-guard the quicklook/inspector leaves',
    prompt: `${COMMON}
TASK (P2): the on-feature crashers. Under panic=abort, these unimplemented!() leaves ABORT the app when the user triggers the feature: rustty_surface_quicklook_font, rustty_surface_quicklook_word (Look-Up / force-click / accessibility), and the inspector metal exports / activateInspector (surface.rs:2843) if the inspector is opened. INTERIM graceful-degrade (per PREPORT_SWIFT_APP §4 abort policy — a recoverable no-op is faithful for a not-yet-ported FEATURE, and strictly better than aborting the whole app): replace each REACHABLE-on-feature unimplemented!() with the export's error-sentinel return (null for the CTFont/Text returns, false/no-op for void) + a log::warn!("<feature> not yet ported") + a TODO(port) with a DEFERRALS entry. Do NOT touch normal-path code, and do NOT change the Zig-faithful behavior of already-ported paths — ONLY the feature leaves that would abort. Enumerate what you changed (grep the quicklook/inspector exports first). Verify: cargo build -p rustty-apprt-c green; the changed exports return a sentinel instead of unimplemented!(). Add DEFERRALS entries. Commit ("apprt-c: P2 crash-guard quicklook/inspector feature leaves — graceful sentinel instead of abort").`,
  },
  {
    key: 'p3-rebrand', label: 'P3 R2 identity rebrand — visible name "Rustty"',
    prompt: `${COMMON}
TASK (P3, R2 identity — USER PRIORITY): re-title the macOS app so it is named "Rustty" EVERYWHERE THE USER SEES IT (the menu-bar app menu currently reads "Ghostty", plus the Dock name, window title, About panel, and app name). This is the sanctioned R2 identity exception — you MAY change identity strings even though Branch A keeps Swift byte-identical elsewhere. HARD CONSTRAINT: do NOT rename the GhosttyKit module or any ghostty_* symbol / #import (those are the Branch-A ABI — renaming breaks the link), and do NOT change Swift LOGIC — only user-visible name strings + bundle identity.
Cover ALL of these:
1. macos/Ghostty-Info.plist: CFBundleName + CFBundleDisplayName → "Rustty"; CFBundleIdentifier com.mitchellh.ghostty → the rustty id (match the config dir already in use, ~/Library/Application Support/io.airely.rustty → io.airely.rustty, or the rustty-identity crate's canonical id — check crates/rustty-identity); display strings like "New $(…)Ghostty…"/"New Ghostty Tab/Window" → Rustty.
2. The Xcode PRODUCT_NAME / the .app product: the menu-bar app menu name = the app's CFBundleName/product. Set PRODUCT_NAME (or CFBundleName) so the built app + its menu read "Rustty". If renaming the product to Rustty.app is clean, do it via the build setting (NOT by renaming the .xcodeproj/scheme, which risks the build — keep the project file named Ghostty.xcodeproj + scheme Ghostty, just change the PRODUCT_NAME/bundle name). If the app menu name comes from a hardcoded Swift string or a localization (Localizable.strings / an NSApplication name), update THOSE display strings to Rustty too (grep macos/Sources + macos for user-facing "Ghostty" display strings — About window, app name constants, notification titles — and change the DISPLAY ones only, leaving symbol/module/URL-scheme/bundle-path references alone).
3. entitlements: app-group / keychain-group ids that embed com.mitchellh.ghostty.
4. build-rustty.sh: if PRODUCT_NAME changes the built .app name (e.g. Rustty.app), update the launch/find path there + in macos/MILESTONE.md.
Document every change + what you deliberately left as ghostty (GhosttyKit module, ghostty_* symbols, project/scheme file names) in macos/REBRAND.md. Verify plutil -lint macos/Ghostty-Info.plist passes. Do NOT rebuild (another agent owns the build); the build-verify happens after. Commit ("macos: R2 rebrand — app is named Rustty everywhere user-visible (menu/Dock/title/About); GhosttyKit module + ghostty_* symbols kept for Branch-A link").`,
  },
  {
    key: 'p3-xtask', label: 'P3 xtask: deployment-target + self-contained lib',
    prompt: `${COMMON}
TASK (P3): two xtask/build refinements (xtask/src/main.rs xcframework()). (1) DEPLOYMENT TARGET: the staticlib builds for macOS 26.5 vs the app's 13.0 target → "built for newer macOS" ld warnings. Fix: set MACOSX_DEPLOYMENT_TARGET (match the app's target — read it from macos/Ghostty.xcodeproj or Info.plist LSMinimumSystemVersion, likely 13.0) as an env on the cargo build Command in xcframework() so rustc/clang target that version. Verify the warnings are gone (or reduced) on the next xcframework build. (2) SELF-CONTAINED LIB (evaluate, then do or defer honestly): the app currently links system -lz + Homebrew -lintl because rustty's vendored libz.a is INFLATE-ONLY (no deflate; sentry needs it) and gettext-sys is the localename slice only (no dgettext). Assess: can zlib-sys (crates-sys/zlib-sys/build.rs) be configured to build FULL zlib (incl. deflate.o), and gettext-sys to provide dgettext/bindtextdomain/textdomain? If a small build.rs change gives full zlib → do it (then the combined archive is self-contained for zlib, drop -lz from build-rustty.sh). If gettext's full libintl is a big vendor change → DEFER with a DEFERRALS entry + keep -lintl, documenting exactly what's needed. Report what you did vs deferred. Do NOT break the current working build — if unsure, keep the system links and just fix deployment-target. Commit your changes ("xtask: P3 — deployment-target pin + self-contained zlib (gettext deferred)" or accurate variant).`,
  },
]
const res = (await parallel(items.map(t => () => agent(t.prompt, { label: t.label, phase: 'Items', schema: OUT })))).filter(Boolean)
log(`Items: ${res.filter(r => r.status === 'done').length}/${items.length} done`)
return { items: res.map(r => ({ t: r.task, s: r.status, ref: r.commit_ref, residual: r.residual })), summary: res.map(r => `${(r.task||'').slice(0,24)}:${r.status}`).join(' | ') }
