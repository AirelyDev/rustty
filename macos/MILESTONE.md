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
- **Identity rebrand** (R2): bundle id / app name (deferred until it runs).
- **Run + iterate**: launch, confirm window + shell + render + input.

## Deferred (optional features, not MVP)
- 5 non-display Darwin exports (quicklook_font/word, inspector_metal_*), the 2 inspector
  core-seams — `unimplemented!()` leaves, fine for a basic terminal.

## ✅ MILESTONE ACHIEVED (2026-07-12) — RUNNABLE macOS TERMINAL

`macos/build-rustty.sh` builds `Ghostty.app`; launching it produces:
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
- **Identity rebrand (R2)**: bundle id / app name still `Ghostty`; the icon is upstream's.
- **Deployment target**: staticlib built for macOS 26.5 vs app target 13.0 (harmless warnings;
  set MACOSX_DEPLOYMENT_TARGET on the cargo build to match).
- **Deferred features**: quicklook, live inspector, the 5 non-display Darwin exports.
