#!/bin/bash
# Build the rustty macOS terminal app. Produces a runnable Ghostty.app whose core
# is rustty's ported Rust terminal engine (Branch A: ghostty_* ABI aliases).
#
# Prereqs (once): `cargo run -p xtask -- xcframework` (the GhosttyKit.xcframework)
# and `cargo run -p xtask -- resources` (zig-out/share/*). This script runs both.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

echo "== 1/3 packaging GhosttyKit.xcframework =="
cargo run -q -p xtask -- xcframework
rm -rf macos/GhosttyKit.xcframework
cp -R dist/GhosttyKit.xcframework macos/GhosttyKit.xcframework

echo "== 2/3 generating app resources (zig-out/share) =="
cargo run -q -p xtask -- resources

echo "== 3/3 xcodebuild =="
# -lz  : full system zlib (rustty's vendored libz.a is inflate-only; sentry needs deflate).
# -lintl: gettext runtime (rustty's vendored gettext-sys is the localename slice only).
# Both are follow-ups to fold into the self-contained lib (see MILESTONE.md).
cd macos
exec xcodebuild -project Ghostty.xcodeproj -scheme Ghostty -configuration Debug \
  -destination 'platform=macOS,arch=arm64' \
  CODE_SIGN_IDENTITY="" CODE_SIGNING_REQUIRED=NO CODE_SIGNING_ALLOWED=NO \
  OTHER_LDFLAGS="-lstdc++ -lz -L/opt/homebrew/opt/gettext/lib -lintl" \
  "$@"
