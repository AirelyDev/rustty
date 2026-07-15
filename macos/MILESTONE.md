# macOS Swift-app milestone — progress

## ✅ DONE
- **Phase A** — ABI prep: symbol manifest (90 exports), `ghostty_*` alias layer (Branch A),
  ghostty.h ABI-layout gate, `rt_surface` pin proof. Plus the ABI-completion pass
  (config/main_c/benchmark aliases) — all 84 Swift-referenced `ghostty_*` functions resolve.
- **B1** — `cargo run -p xtask -- xcframework` packages rustty-apprt-c (identity-ghostty,
  panic=abort profile) into `dist/GhosttyKit.xcframework` (macos-arm64, 96 ghostty_* symbols,
  rustty's vendored ghostty.h).
- **B2** — the reused ghostty Swift app imported to `macos/` (Branch A, byte-identical), rustty's
  xcframework swapped in.
- **B3 (partial)** — **THE APP LINKS**: `xcodebuild` produces ZERO `Undefined symbol ghostty_*`
  errors. rustty's ported terminal core is now linked into the reused AppKit frontend. This retired
  the milestone's biggest technical risk (Branch A symbol resolution).

## ⏳ REMAINING to a running window
- **Resource bundle**: the Xcode Copy-Resources phase needs `zig-out/share/{terminfo,ghostty,
  vim,nvim,fish,zsh,bash-completion,man,locale,bat}` — data ghostty's `zig build` produces.
  rustty has the generators (rustty-terminfo, rustty-extra, rustty-helpgen) but no xtask pipeline
  yet. terminfo is runtime-critical; the rest are build-phase folders. → xtask `resources`.
- **Identity rebrand** (R2, then full rebrand): DONE — app is named "Rustty" everywhere
  user-visible and the app bundle id is `io.airely.rustty(.debug)`. See `REBRAND.md`. The
  R2-era carve-out (GhosttyKit module + `ghostty_*` symbols + on-disk `Ghostty.app`/executable
  name kept for the Branch-A link) no longer applies: the project is fully on `RusttyKit` /
  `rustty_*` and builds `Rustty.app`/`rustty`.
- **Run + iterate**: launch, confirm window + shell + render + input.

## Deferred (optional features, not MVP)
- 5 non-display Darwin exports (quicklook_font/word, inspector_metal_*), the 2 inspector
  core-seams — `unimplemented!()` leaves, fine for a basic terminal.

## ✅ MILESTONE ACHIEVED (2026-07-12) — RUNNABLE macOS TERMINAL

`macos/build-rustty.sh` builds `Rustty.app` (product name, executable, and framework are now
fully `Rustty`/`rustty`/`RusttyKit` — see `REBRAND.md` for the full-rebrand landing); launching it
produces:
- **A terminal window** (AppKit surface up — verified: `count of windows` = 1).
- **A live shell**: the app spawns `/usr/bin/login → /bin/zsh` in a pty via rustty's
  ported termio (verified in the process tree). The full stack works end-to-end:
  reused Swift frontend → rustty `ghostty_*` C-ABI → surface → termio → pty → shell → renderer.

This is the payoff: rustty's ported Rust terminal engine, linked into the reused ghostty
AppKit app, running as a real macOS terminal.

### Follow-ups (non-blocking refinements)
- **Self-contained lib**: currently links system `-lz` + Homebrew `-lintl` because rustty's
  vendored `libz.a` is inflate-only (no `deflate`; sentry needs it) and `gettext-sys` is the
  localename slice only (no `dgettext`). Fold full zlib + gettext into the combined lib to
  drop the system/Homebrew link deps (VALIDATION_REPORT_2 flagged the gettext slice).
- **Identity rebrand**: DONE, full — user-visible naming (app presents as "Rustty"; bundle id
  `io.airely.rustty`), the app **icon** (`images/Rustty.icon`, `ASSETCATALOG_COMPILER_APPICON_NAME
  = Rustty`), and the on-disk product/executable name (`Rustty.app`/`rustty`) are all rebranded —
  see `REBRAND.md`. Secondary component bundle ids are `io.airely.rustty*` (dock-tile plugin, unit/
  UI test bundles) — no `com.mitchellh.*`/`Ghostty` remnants left on the macOS side.
- **Deployment target**: staticlib built for macOS 26.5 vs app target 13.0 (harmless warnings;
  set MACOSX_DEPLOYMENT_TARGET on the cargo build to match).
- **Deferred features**: quicklook, live inspector, the 5 non-display Darwin exports.

## ✅ CORRECTED (2026-07-12) — the window ACTUALLY RENDERS now (W-ATTACH fixed)

**Correction to the claim above:** the first "RUNNABLE" entry was premature — the window opened
but was BLANK (renderer presented to a detached offscreen IOSurface; I inferred rendering from the
process tree instead of looking). A 6-agent critical-path audit (`.port/phase-2/MACOS_APP_GAPS.md`)
found W-ATTACH was the SINGLE blocker to pixels. Fixed in commit 96b0116 (route the host NSView
into the renderer's windowed layer-attach at finalizeSurfaceInit).

**Now verified by screenshot:** the terminal window renders the shell prompt, block cursor, and
glyphs; typed input echoes and is interactive. A genuinely-working macOS terminal:
window + render + shell + input, all driven by rustty's ported Rust core.

### Remaining (from the gap audit — non-blocking for a usable terminal)
- **Verify (should auto-work with W-ATTACH):** live window resize tracking the layer bounds (B4),
  present-error propagation (B2), display-callback redraw on resize (B3). Un-ignore
  `macos_window_host.rs::hosts_view_layer_and_presents_live_frame`.
- **P2 on-feature crashers (panic=abort):** `quicklook_font`/`quicklook_word` abort on Look-Up /
  force-click / accessibility; the inspector aborts if opened. Cheap interim: swap the
  `unimplemented!()` leaves for graceful null/false returns, then port the bodies.
- **P3 polish:** autoreleasepool shim, self-contained zlib+gettext lib, R2 identity rebrand, the
  deployment-target warning.
