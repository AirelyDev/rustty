#!/bin/bash
# Build a SIGNED rustty macOS app (Rustty.app). This is the distribution-oriented
# counterpart to the unsigned local build in build-rustty.sh (which passes
# CODE_SIGNING_ALLOWED=NO).
#
# It builds the app unsigned (reusing build-rustty.sh), then re-signs the whole
# bundle INSIDE-OUT with codesign + the hardened runtime, exactly as ghostty's
# release-tag.yml workflow does (oracle: .github/workflows/release-tag.yml
# "Codesign app bundle"). Signing order matters: nested Mach-O (Sparkle XPC
# services, Autoupdate, Updater.app, the framework, the Dock-tile plugin, the
# bundled dylibs) must be signed before the enclosing app bundle.
#
# IDENTITY:
#   - DEVELOPER_ID unset (default)  -> ad-hoc "-"  (works with NO Apple cert;
#                                      this is what CI/local verification uses).
#   - DEVELOPER_ID="Developer ID Application: NAME (TEAMID)" -> real signing,
#                                      the identity notarization requires.
#
# Both paths apply the HARDENED RUNTIME (-o runtime) — mandatory for
# notarization, and harmless (still statically verifiable) for an ad-hoc build.
#
# ENTITLEMENTS: defaults to macos/Rustty.entitlements (the hardened-runtime
#   entitlement set ghostty notarizes with). Override with ENTITLEMENTS=... .
#   macos/RusttyReleaseLocal.entitlements additionally carries
#   com.apple.security.cs.disable-library-validation, useful for running an
#   ad-hoc build that loads the bundle's own dylibs.
#
# SKIP_BUILD=1 re-signs an already-built app without rebuilding.
#
# Verify locally (no cert needed):
#   ./macos/build-rustty-signed.sh
#   # -> ends by running `codesign --verify --deep --strict` on the app.
#
# Notarization (needs a Developer ID cert + credentials) is DOCUMENTED, not run,
# in macos/DISTRIBUTION.md.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

IDENTITY="${DEVELOPER_ID:--}"
ENTITLEMENTS="${ENTITLEMENTS:-$ROOT/macos/Rustty.entitlements}"
# Must match the configuration build-rustty.sh built (it honors the same
# variable): the release workflows set CONFIGURATION=Release, local dev gets
# the Debug default.
CONFIGURATION="${CONFIGURATION:-Debug}"

if [ "$IDENTITY" = "-" ]; then
  echo "== signing identity: ad-hoc (\"-\") — set DEVELOPER_ID for a real cert =="
  # Ad-hoc signatures cannot be timestamped, and an offline/local run must not
  # block on Apple's timestamp service.
  TS_FLAG=(--timestamp=none)
else
  echo "== signing identity: $IDENTITY =="
  # A REAL Developer ID signature MUST carry a secure timestamp: Apple rejects
  # notarization submissions without one ("The signature does not include a
  # secure timestamp"), which would then fail `stapler staple` in the release
  # workflows. Bare `--timestamp` selects Apple's default timestamp authority,
  # i.e. the same signature the oracle's inline block produces by omitting the
  # flag (ghostty .github/workflows/release-tag.yml:229-237). It is spelled
  # explicitly rather than left empty because this script runs under `set -u`
  # on macOS's bash 3.2, where expanding an empty array is an error.
  TS_FLAG=(--timestamp)
fi
echo "== entitlements: $ENTITLEMENTS =="

# 1) Build the app unsigned (produces the bundle in DerivedData).
if [ "${SKIP_BUILD:-0}" = "1" ]; then
  echo "== SKIP_BUILD=1 — re-signing the existing build =="
else
  echo "== building unsigned app via build-rustty.sh =="
  "$ROOT/macos/build-rustty.sh"
fi

# 2) Locate the built app bundle.
APP="$(cd "$ROOT/macos" && xcodebuild -project Rustty.xcodeproj -scheme Rustty \
  -configuration "$CONFIGURATION" -showBuildSettings 2>/dev/null \
  | awk -F' = ' '/ BUILT_PRODUCTS_DIR = /{d=$2} / FULL_PRODUCT_NAME = /{p=$2} END{print d"/"p}')"
if [ ! -d "$APP" ]; then
  echo "!! built app not found at: $APP" >&2
  exit 1
fi
echo "== app: $APP =="

# codesign wrapper: force, hardened runtime, verbose.
sign() { /usr/bin/codesign --force --options runtime "${TS_FLAG[@]}" \
  --sign "$IDENTITY" "$@"; }

echo "== 1/3 signing nested code (inside-out) =="
FW="$APP/Contents/Frameworks/Sparkle.framework"
if [ -d "$FW" ]; then
  # Sparkle's XPC services + helpers. They live under Versions/<letter>; resolve
  # via the "Current" symlink so we don't hardcode the version letter. The
  # binaries in Versions must be signed as real files (not through the symlink).
  V="$FW/Versions/Current"
  for x in "$V/XPCServices/Downloader.xpc" "$V/XPCServices/Installer.xpc" \
           "$V/Autoupdate" "$V/Updater.app"; do
    [ -e "$x" ] && sign "$x"
  done
  sign "$FW"
fi

# Dock-tile plugin.
for p in "$APP"/Contents/PlugIns/*.plugin; do
  [ -e "$p" ] && sign "$p"
done

# Bundled dylibs loaded by the main executable (hardened runtime seals these).
for d in "$APP"/Contents/MacOS/*.dylib; do
  [ -e "$d" ] && sign "$d"
done

echo "== 2/3 signing the app bundle (with entitlements) =="
/usr/bin/codesign --force --options runtime "${TS_FLAG[@]}" \
  --entitlements "$ENTITLEMENTS" --sign "$IDENTITY" "$APP"

echo "== 3/3 verifying (codesign --verify --deep --strict) =="
codesign --verify --deep --strict --verbose=2 "$APP"
echo ""
codesign -dv --verbose=2 "$APP" 2>&1 | sed 's/^/   /'
echo ""
echo "== SIGNED OK: $APP =="
if [ "$IDENTITY" = "-" ]; then
  echo "   (ad-hoc — NOT notarizable; use DEVELOPER_ID + macos/DISTRIBUTION.md to notarize)"
fi
