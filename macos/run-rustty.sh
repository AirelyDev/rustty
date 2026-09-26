#!/bin/bash
# Build and run the rustty macOS app. PORTS the `open` step of
# src/build/GhosttyXcodebuild.zig:126-163 (`zig build run` on macOS), S64-F7.
#
# Builds through build-rustty.sh (same CONFIGURATION / XCFRAMEWORK_TARGET
# variables), disables state restoration in the built bundle, then runs the
# app binary with logs forced to stderr and the launch source set to
# `zig_run`, forwarding this script's arguments to the app.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

CONFIGURATION="${CONFIGURATION:-Debug}"
export CONFIGURATION

"$ROOT/macos/build-rustty.sh"

cd "$ROOT/macos"
APP="$(env -i HOME="$HOME" PATH="$PATH" xcodebuild -project Rustty.xcodeproj -scheme Rustty \
  -configuration "$CONFIGURATION" -showBuildSettings 2>/dev/null \
  | awk -F' = ' '/ BUILT_PRODUCTS_DIR = /{d=$2} / FULL_PRODUCT_NAME = /{p=$2} END{print d"/"p}')"

# We'll have to change this to `Set` if we ever put this into our Info.plist.
/usr/libexec/PlistBuddy -c "Add :NSQuitAlwaysKeepsWindows bool false" "$APP/Contents/Info.plist"

cd "$ROOT"
# This overrides our default behavior and forces logs to show up on stderr
# (in addition to the centralized macOS log).
export RUSTTY_LOG="stderr,macos"
# Configure how we're launching
export RUSTTY_MAC_LAUNCH_SOURCE="zig_run"
exec "$APP/Contents/MacOS/rustty" "$@"
