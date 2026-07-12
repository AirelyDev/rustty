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
