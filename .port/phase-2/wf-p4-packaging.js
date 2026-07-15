export const meta = {
  name: 'p4-packaging',
  description: 'Plan P4: packaging robustness — explicit archive dep set + ranlib normalization, universal arm64+x86_64 slice, code signing + notarization path',
  whenToUse: 'macOS full-functionality plan Phase 4 (xtask + build-rustty.sh; disjoint from inspector/apprt work)',
  phases: [{ title: 'Packaging', detail: 'P4.1 archive-set+ranlib, P4.2 universal, P4.3 signing — 4.1→4.2 sequential (both edit xcframework), 4.3 parallel' }],
}
const OUT = {
  type: 'object', required: ['task', 'status', 'evidence'],
  properties: {
    task: { type: 'string' }, status: { type: 'string', enum: ['done', 'partial', 'blocked'] },
    evidence: { type: 'string' }, committed: { type: 'boolean' }, commit_ref: { type: 'string' }, residual: { type: 'string' }, blocker: { type: 'string' },
  },
}
const COMMON = `
REPO: /Users/gael/projects/rustty. Oracle (READ-ONLY): /Users/gael/projects/ghostty @ 4749c4e9 (src/build/{GhosttyXCFramework,XCFrameworkStep,LibtoolStep,CombineArchivesStep,GhosttyLib}.zig is the recipe). Plan: docs/superpowers/plans/2026-07-12-macos-app-full-functionality.md. The xcframework packaging is xtask/src/main.rs xcframework(); the reproducible build is macos/build-rustty.sh. Combined lib is now self-contained for zlib (deflate defined); gettext still -lintl. Concurrent agents own crates/rustty-{apprt-c,inspector,core,apprt,renderer} — do NOT touch those. RULES: R1/R3; do NOT break the working build (verify cargo run -p xtask -- xcframework succeeds after each change). Git: explicit paths, index.lock retry, no push/reset/stash. Return via StructuredOutput; COMMIT own files.`

phase('Packaging')
// 4.1 and 4.2 both edit xcframework() -> one agent does both sequentially to avoid self-conflict; 4.3 (signing) is separate files.
const combined = agent(`${COMMON}
TASK P4.1 + P4.2 (both edit xtask xcframework(), so do them together, sequentially):
P4.1 (INTEGRATION_PARITY §2.4/§2.5): (a) replace the filesystem-glob archive discovery in xcframework() with an EXPLICIT set derived from cargo build -p rustty-apprt-c --profile xcframework --features identity-ghostty --target <target> --message-format=json — parse the JSON records (compiler-artifact / build-script "linked-libs"/"outputs", or the .a paths in the artifact filenames) to get the real C-dep archive set, instead of globbing target/.../build/*/out/*.a (which risks missed-nested / stale-duplicate archives). (b) Before libtool, copy each archive to a temp path and run /usr/bin/ranlib on it (so newer-Xcode libtool doesn't silently drop 64-bit members).
P4.2 (universal slice): add the x86_64-apple-darwin target build (rustup target add x86_64-apple-darwin first if needed) + lipo -create the per-arch combined archives into a universal lib, staged as the slice macos-arm64_x86_64 (matching ghostty's GhosttyKit.xcframework/macos-arm64_x86_64). If x86_64 target can't be added/built in this env, keep arm64-only and DEFER universal with a precise DEFERRALS entry (the exact rustup + lipo steps) — do NOT break the arm64 build.
Verify after each: cargo run -p xtask -- xcframework succeeds; nm the combined lib still has _ghostty_app_new + _deflate + stable symbol counts; plutil -p dist/GhosttyKit.xcframework/Info.plist shows the slice(s); lipo -info the lib shows the arch(es). Commit ("xtask: explicit archive set (cargo json) + ranlib normalization + universal slice (P4.1+P4.2)" or accurate variant if universal deferred).`,
  { label: 'P4.1+4.2 archive/universal', phase: 'Packaging', schema: OUT })

const signing = agent(`${COMMON}
TASK P4.3 (code signing + notarization path): the current build disables signing (CODE_SIGNING_ALLOWED=NO in build-rustty.sh). Add a SIGNED build variant WITHOUT breaking the unsigned local build: (a) a new script macos/build-rustty-signed.sh (or a --sign flag in build-rustty.sh) that signs the app + framework with a Developer ID (env DEVELOPER_ID) OR ad-hoc "-" for local, using the rustty entitlements (macos/*.entitlements). Verify codesign --verify --deep --strict passes on an ad-hoc-signed local build (ad-hoc works without a cert). (b) DOCUMENT (do NOT run without credentials) the notarization path in macos/MILESTONE.md or a macos/DISTRIBUTION.md: the exact xcrun notarytool submit + xcrun stapler staple commands + the required Developer ID Application cert + hardened-runtime entitlement. Do NOT touch xtask (another agent owns it). Commit ("macos: signed build variant (ad-hoc verified) + documented notarization path (P4.3)").`,
  { label: 'P4.3 signing', phase: 'Packaging', schema: OUT })

const res = (await parallel([() => combined, () => signing])).filter(Boolean)
log(`P4: ${res.filter(r => r.status === 'done').length}/2 done`)
return { items: res.map(r => ({ t: r.task, s: r.status, ref: r.commit_ref, residual: r.residual })), summary: res.map(r => `${(r.task||'').slice(0,22)}:${r.status}`).join(' | ') }
