#!/bin/bash
# Build the rustty macOS terminal app. Produces a runnable Rustty.app whose core
# is rustty's ported Rust terminal engine (Branch A: ghostty_* ABI aliases).
#
# Prereqs (once): `cargo run -p xtask -- xcframework` (the RusttyKit.xcframework)
# and `cargo run -p xtask -- resources` (zig-out/share/*). This script runs both.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

echo "== 1/3 packaging RusttyKit.xcframework =="
cargo run -q -p xtask -- xcframework
rm -rf macos/RusttyKit.xcframework
cp -R dist/RusttyKit.xcframework macos/RusttyKit.xcframework

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
exec xcodebuild -project Rustty.xcodeproj -scheme Rustty -configuration Debug \
  -destination 'platform=macOS,arch=arm64' \
  CODE_SIGN_IDENTITY="" CODE_SIGNING_REQUIRED=NO CODE_SIGNING_ALLOWED=NO \
  OTHER_LDFLAGS="-lstdc++" \
  "$@"
