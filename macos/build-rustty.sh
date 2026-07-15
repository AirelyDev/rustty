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
# zlib is now self-contained: rustty's vendored libz.a builds FULL zlib
# (inflate + deflate; xtask P3 vendored deflate.c/trees.c/compress.c) and xtask
# folds it into libRusttyKit.a, so sentry's `deflate` resolves from the app's
# own archive — no system `-lz` needed.
# -lintl: gettext runtime. STILL required — rustty's vendored gettext-sys is the
# localename slice only (no dgettext/bindtextdomain/textdomain). Folding full
# libintl is a large vendor change; see DEFERRALS.md ("self-contained libintl").
cd macos
exec xcodebuild -project Rustty.xcodeproj -scheme Rustty -configuration Debug \
  -destination 'platform=macOS,arch=arm64' \
  CODE_SIGN_IDENTITY="" CODE_SIGNING_REQUIRED=NO CODE_SIGNING_ALLOWED=NO \
  OTHER_LDFLAGS="-lstdc++ -L/opt/homebrew/opt/gettext/lib -lintl" \
  "$@"
