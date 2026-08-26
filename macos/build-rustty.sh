#!/bin/bash
# Build the rustty macOS terminal app. Produces a runnable Rustty.app whose core
# is rustty's ported Rust terminal engine (Branch A: ghostty_* ABI aliases).
#
# Prereqs (once): `cargo run -p xtask -- xcframework` (the RusttyKit.xcframework)
# and `cargo run -p xtask -- resources` (zig-out/share/*). This script runs both.
#
# CONFIGURATION selects the Xcode build configuration and defaults to `Debug`
# (local development). The release pipeline — .github/workflows/release-tag.yml
# and release-tip.yml, which build the shipped app through THIS script rather
# than duplicating its xcodebuild invocation — sets CONFIGURATION=Release.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

CONFIGURATION="${CONFIGURATION:-Debug}"

echo "== 1/3 packaging RusttyKit.xcframework =="
# `xtask xcframework` now writes STRAIGHT to macos/RusttyKit.xcframework — the
# path Rustty.xcodeproj links and the one the reference emits to
# (GhosttyXCFramework.zig:41 `.out_path = "macos/GhosttyKit.xcframework"`).
#
# This step used to build into dist/ and then `cp -R` it here. That copy was
# the only thing refreshing macos/, so the bundle was current exactly when this
# script had been run and stale otherwise — and a plain `xcodebuild` (what the
# reference's CI does, and what ci.yml's macos-app job does) silently linked
# whatever stale copy was lying around. On this machine that copy was 69 header
# lines behind and the app failed with 18 "cannot find X in scope" errors.
# Having one location instead of two removes the failure mode entirely.
cargo run -q -p xtask -- xcframework

echo "== 2/3 generating app resources (zig-out/share) =="
cargo run -q -p xtask -- resources

echo "== 3/3 xcodebuild =="
# The combined archive is fully self-contained — no system or Homebrew link
# flags for zlib or gettext:
#   * zlib: rustty's vendored libz.a builds FULL zlib (inflate + deflate; xtask
#     P3 vendored deflate.c/trees.c/compress.c), so sentry's `deflate` resolves
#     from the app's own archive — no `-lz`.
#   * gettext: rustty's vendored gettext-sys builds the FULL macOS libintl (WS9
#     vendored the reference's whole pkg/libintl `srcs` set), so
#     `dgettext`/`bindtextdomain`/`textdomain` resolve from the app's own
#     archive — no `-L/opt/homebrew/opt/gettext/lib -lintl`.
# `-lstdc++` stays: it is a toolchain runtime (the C++ -sys crates), not a
# third-party library rustty could vendor.
# Guard: `.port/tools/gates/no_homebrew_gettext_gate.sh` proves this by building
# with Homebrew's libintl hidden.
cd macos
exec xcodebuild -project Rustty.xcodeproj -scheme Rustty -configuration "$CONFIGURATION" \
  -destination 'platform=macOS,arch=arm64' \
  CODE_SIGN_IDENTITY="" CODE_SIGNING_REQUIRED=NO CODE_SIGNING_ALLOWED=NO \
  OTHER_LDFLAGS="-lstdc++" \
  "$@"
