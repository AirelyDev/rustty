#!/usr/bin/env bash
# release-local.sh — build, sign and stage a release by hand, then (optionally)
# create a DRAFT GitHub release from it. The hosted pipeline
# (.github/workflows/release-{tag,tip}.yml) is inert and uploads to R2; this is
# the manual path, built from the same tooling (xtask dist, macos/build-rustty*.sh).
#
# Everything lands in dist/release/<version>/ (git-ignored). Nothing is tagged,
# pushed or published unless you run `publish`, and publish only ever makes a
# DRAFT release.
#
#   release-local.sh stage   [--lanes source,macos,lib] [--allow-adhoc] [--distcheck]
#   release-local.sh vm      <ssh-host> [--lanes lib]       build a lane on a VM, fetch the result
#   release-local.sh sign                                   SHA256SUMS + minisign every asset, verify
#   release-local.sh verify                                 re-check checksums, signatures, codesign
#   release-local.sh publish --tag <tag> [--prerelease]     DRAFT release from the staged assets
#
# VERSION. Rustty carries the pinned oracle's version (G-VERSION): 1.3.2-dev at
# this pin, built from git as X.Y.Z-<branch>+<hash> (Ghostty's tip scheme). A
# vX.Y.Z release exists only when the pin is exactly Ghostty's vX.Y.Z; until
# then these are pre-releases. Asset names replace `+` with `-` (GitHub rewrites
# unusual characters in asset names).
#
# SIGNING (see the plan in the session notes):
#   source/lib tarballs, SHA256SUMS   minisign, key ~/.minisign/rustty-release.key
#                                     (checked against the public key in PACKAGING.md)
#   macOS app                         DEVELOPER_ID="Developer ID Application: NAME (TEAM)"
#                                     plus NOTARY_PROFILE=<xcrun notarytool keychain profile>
#                                     for notarization + stapling. Without DEVELOPER_ID
#                                     the app is ad-hoc signed, NOT notarizable, and the
#                                     macos lane refuses to run unless --allow-adhoc.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

MINISIGN_KEY="${MINISIGN_KEY:-$HOME/.minisign/rustty-release.key}"
MINISIGN_PUB="${MINISIGN_PUB:-$HOME/.minisign/rustty-release.pub}"

die() { echo "release-local: $*" >&2; exit 1; }
say() { echo "== $* =="; }

# --- version -----------------------------------------------------------------
# The tarball name carries the full app version (`rustty_version::VERSION_STRING`),
# so build it and read the version back from the name: one source of truth.
full_version() {
  local p
  # A VM build gets the host's exact version (it has no git history to derive one).
  if [ -n "${RUSTTY_VERSION_STRING:-}" ]; then printf '%s' "$RUSTTY_VERSION_STRING"; return; fi
  p="$(cargo run -q -p xtask -- dist 2>/dev/null | tail -1)"
  [ -f "$p" ] || p="$(ls -t dist/rustty-*.tar.gz 2>/dev/null | head -1)"
  [ -f "$p" ] || die "xtask dist produced no tarball"
  basename "$p" .tar.gz | sed 's/^rustty-//'
}
safe() { printf '%s' "$1" | tr '+' '-'; }

stage_dir() { printf '%s/dist/release/%s' "$ROOT" "$(safe "$1")"; }

# Refuse to release an uncommitted tree: the artifacts must be reproducible from
# a commit (the version carries its hash).
require_clean() {
  if [ -n "$(git status --porcelain --untracked-files=no)" ]; then
    die "tracked files are modified; commit or stash first (the version embeds the commit)"
  fi
}

# --- lanes -------------------------------------------------------------------
lane_source() {
  local ver="$1" out="$2"
  say "source tarballs"
  if [ "${DISTCHECK:-0}" = 1 ]; then
    cargo run -q -p xtask -- distcheck
    cargo run -q -p xtask -- distcheck-lib-vt
  fi
  local a b
  a="$(cargo run -q -p xtask -- dist 2>/dev/null | tail -1)"
  b="$(cargo run -q -p xtask -- dist-lib-vt 2>/dev/null | tail -1)"
  [ -f "$a" ] && [ -f "$b" ] || die "xtask dist / dist-lib-vt did not produce tarballs"
  cp "$a" "$out/$(safe "$(basename "$a")")"
  cp "$b" "$out/$(safe "$(basename "$b")")"
}

# The shared and static C library, headers and pkg-config files, laid out by the
# CMake install (the layout xtask/tests/cmake_lib_vt.rs checks), for THIS host.
lane_lib() {
  local ver="$1" out="$2" os arch tmp name
  os="$(uname -s | tr '[:upper:]' '[:lower:]')"
  arch="$(uname -m)"
  [ "$arch" = arm64 ] && arch=aarch64
  tmp="$(mktemp -d)"
  say "librustty-vt for $os-$arch"
  cmake -S . -B "$tmp/build" -DCMAKE_BUILD_TYPE=Release >/dev/null
  cmake --build "$tmp/build" --config Release
  cmake --install "$tmp/build" --prefix "$tmp/prefix" --config Release >/dev/null
  name="librustty-vt-$(safe "$ver")-$os-$arch"
  mkdir -p "$tmp/stage"
  mv "$tmp/prefix" "$tmp/stage/$name"
  tar -C "$tmp/stage" -czf "$out/$name.tar.gz" "$name"
  rm -rf "$tmp"
}

lane_macos() {
  local ver="$1" out="$2" app dmg id="${DEVELOPER_ID:-}"
  [ "$(uname -s)" = Darwin ] || die "the macos lane needs a Mac"
  if [ -z "$id" ] && [ "${ALLOW_ADHOC:-0}" != 1 ]; then
    die "DEVELOPER_ID is unset, so the app would be ad-hoc signed and could not be notarized; set it, or pass --allow-adhoc to ship an un-notarized build deliberately"
  fi
  export CONFIGURATION=Release
  say "build Rustty.app (Release, universal)"
  ./macos/build-rustty.sh
  app="$(cd macos && xcodebuild -project Rustty.xcodeproj -scheme Rustty -configuration Release -showBuildSettings 2>/dev/null |
    awk -F' = ' '/ BUILT_PRODUCTS_DIR = /{d=$2} / FULL_PRODUCT_NAME = /{p=$2} END{print d"/"p}')"
  [ -d "$app" ] || die "built app not found at $app"
  # Same metadata the release workflow patches in before signing seals it.
  local plist="$app/Contents/Info.plist" short commit
  commit="$(git rev-parse --short=9 HEAD)"
  short="$(printf '%s' "$ver" | sed 's/[-+].*//')"
  /usr/libexec/PlistBuddy -c "Set :RusttyCommit $commit" "$plist"
  /usr/libexec/PlistBuddy -c "Set :CFBundleShortVersionString $short" "$plist"
  /usr/libexec/PlistBuddy -c "Delete :SUEnableAutomaticChecks" "$plist" 2>/dev/null || true

  say "codesign (${id:-ad-hoc})"
  DEVELOPER_ID="$id" SKIP_BUILD=1 ./macos/build-rustty-signed.sh

  dmg="$out/Rustty-$(safe "$ver")-macos-universal.dmg"
  say "dmg"
  if command -v create-dmg >/dev/null; then
    local tmp; tmp="$(mktemp -d)"
    if [ -n "$id" ]; then create-dmg --identity="$id" "$app" "$tmp" || true; else create-dmg "$app" "$tmp" || true; fi
    mv "$tmp"/Rustty*.dmg "$dmg"; rm -rf "$tmp"
  else
    local tmp; tmp="$(mktemp -d)"
    cp -R "$app" "$tmp/" && ln -s /Applications "$tmp/Applications"
    hdiutil create -quiet -volname Rustty -srcfolder "$tmp" -format UDZO "$dmg"
    [ -n "$id" ] && codesign --force --sign "$id" --timestamp "$dmg"
    rm -rf "$tmp"
  fi

  if [ -n "$id" ] && [ -n "${NOTARY_PROFILE:-}" ]; then
    say "notarize + staple"
    xcrun notarytool submit "$dmg" --keychain-profile "$NOTARY_PROFILE" --wait
    xcrun stapler staple "$dmg"
    xcrun stapler staple "$app"
  elif [ -n "$id" ]; then
    echo "WARNING: DEVELOPER_ID is set but NOTARY_PROFILE is not; the dmg is signed, NOT notarized" >&2
  fi

  say "zip"
  ditto -c -k --sequesterRsrc --keepParent "$app" "$out/Rustty-$(safe "$ver")-macos-universal.zip"
  [ -d "$app.dSYM" ] && ditto -c -k --keepParent "$app.dSYM" "$out/Rustty-$(safe "$ver")-macos-universal-dsym.zip"
  return 0
}

# --- commands ----------------------------------------------------------------
cmd_stage() {
  local lanes="source,macos,lib" ver out
  while [ $# -gt 0 ]; do
    case "$1" in
      --lanes) lanes="$2"; shift 2 ;;
      --allow-adhoc) ALLOW_ADHOC=1; shift ;;
      --distcheck) DISTCHECK=1; shift ;;
      *) die "stage: unknown argument $1" ;;
    esac
  done
  require_clean
  ver="$(full_version)"
  out="$(stage_dir "$ver")"
  mkdir -p "$out"
  say "version $ver -> $out"
  local l
  for l in ${lanes//,/ }; do
    case "$l" in
      source) lane_source "$ver" "$out" ;;
      lib) lane_lib "$ver" "$out" ;;
      macos) lane_macos "$ver" "$out" ;;
      *) die "unknown lane $l" ;;
    esac
  done
  echo "$ver" >"$out/.version"
  ls -la "$out"
}

# Build a lane on a VM from `git archive HEAD` (the committed tree), then fetch.
cmd_vm() {
  local host="${1:-}" lanes="lib"; shift || true
  [ -n "$host" ] || die "vm: ssh host required (rvm = Linux, rbsd = FreeBSD, rwin = Windows)"
  while [ $# -gt 0 ]; do case "$1" in --lanes) lanes="$2"; shift 2 ;; *) die "vm: unknown $1" ;; esac; done
  require_clean
  ssh -o ConnectTimeout=8 -o BatchMode=yes "$host" true 2>/dev/null || die "cannot reach $host over ssh (is the VM running?)"
  local ver out rdir sha
  ver="$(full_version)"; out="$(stage_dir "$ver")"; mkdir -p "$out"
  sha="$(git rev-parse HEAD)"
  rdir="rustty-release-$sha"
  say "ship $sha to $host"
  git archive --format=tar --prefix="$rdir/" HEAD | ssh "$host" "rm -rf $rdir && tar -xf -"
  say "build on $host"
  ssh "$host" "cd $rdir && git init -q . 2>/dev/null; RUSTTY_VERSION_STRING='$ver' bash .port/tools/release-local.sh stage --lanes $lanes"
  say "fetch"
  scp -q "$host:$rdir/dist/release/$(safe "$ver")/*" "$out/"
  ssh "$host" "rm -rf $rdir"
  ls -la "$out"
}

cmd_sign() {
  local ver out
  ver="$(<"$(ls -d dist/release/*/ | tail -1).version")" || die "nothing staged"
  out="$(stage_dir "$ver")"
  command -v minisign >/dev/null || die "minisign is not installed"
  [ -f "$MINISIGN_KEY" ] || die "no minisign key at $MINISIGN_KEY"
  # The key we sign with must be the one packagers verify against.
  local want have
  want="$(grep -E '^RW[A-Za-z0-9+/=]{50,}$' PACKAGING.md | head -1)"
  have="$(tail -1 "$MINISIGN_PUB")"
  [ "$want" = "$have" ] || die "the public key beside $MINISIGN_KEY does not match the one in PACKAGING.md"
  ( cd "$out" && rm -f SHA256SUMS SHA256SUMS.minisig &&
    shasum -a 256 $(ls | grep -v -e '\.minisig$' -e '^SHA256SUMS$') >SHA256SUMS )
  say "minisign (you will be asked for the key password)"
  local f
  for f in "$out"/*; do
    case "$f" in *.minisig) continue ;; esac
    minisign -S -s "$MINISIGN_KEY" -m "$f" -x "$f.minisig"
  done
  cmd_verify
}

cmd_verify() {
  local out ver
  out="$(ls -d dist/release/*/ | tail -1)"; out="${out%/}"
  [ -f "$out/SHA256SUMS" ] || die "not signed yet (no SHA256SUMS in $out)"
  ( cd "$out" && shasum -a 256 -c SHA256SUMS )
  local f
  for f in "$out"/*; do
    case "$f" in *.minisig|*/SHA256SUMS|*/.version) continue ;; esac
    minisign -V -p "$MINISIGN_PUB" -m "$f" -x "$f.minisig" >/dev/null || die "bad signature on $f"
  done
  minisign -V -p "$MINISIGN_PUB" -m "$out/SHA256SUMS" -x "$out/SHA256SUMS.minisig" >/dev/null || die "bad signature on SHA256SUMS"
  for f in "$out"/*.dmg; do
    [ -e "$f" ] || continue
    codesign --verify --strict "$f" 2>&1 || echo "NOTE: $f is not Developer-ID signed"
    xcrun stapler validate "$f" >/dev/null 2>&1 && echo "stapled: $f" || echo "NOTE: $f is not notarized/stapled"
  done
  echo "verify OK: $out"
}

cmd_publish() {
  local tag="" pre=1
  while [ $# -gt 0 ]; do
    case "$1" in --tag) tag="$2"; shift 2 ;; --prerelease) pre=1; shift ;; --final) pre=0; shift ;; *) die "publish: unknown $1" ;; esac
  done
  [ -n "$tag" ] || die "publish: --tag required"
  require_clean
  git ls-remote --exit-code origin >/dev/null 2>&1 || die "cannot reach origin"
  git branch -r --contains HEAD 2>/dev/null | grep -q . ||
    die "HEAD is not on any remote branch; push it first, GitHub must be able to see the commit the release points at"
  cmd_verify
  local out ver
  out="$(ls -d dist/release/*/ | tail -1)"; out="${out%/}"; ver="$(<"$out/.version")"
  local -a assets=()
  local f
  for f in "$out"/*; do case "$f" in */.version) ;; *) assets+=("$f") ;; esac; done
  local -a flags=(--draft --target "$(git rev-parse HEAD)" --title "Rustty $ver")
  [ "$pre" = 1 ] && flags+=(--prerelease)
  say "creating DRAFT release $tag (${#assets[@]} assets)"
  gh release create "$tag" "${flags[@]}" --notes "Rustty $ver, ported from Ghostty $(grep -m1 -o '"1\.[0-9.]*-dev"' crates/rustty-identity/src/lib.rs | head -1 | tr -d '\"') at $(cut -c1-9 .port/tools/gates/ORACLE_PIN). Verify downloads with SHA256SUMS and the minisign key in PACKAGING.md." "${assets[@]}"
  echo "Draft created. Review it on GitHub, then publish it there."
}

case "${1:-}" in
  stage) shift; cmd_stage "$@" ;;
  vm) shift; cmd_vm "$@" ;;
  sign) shift; cmd_sign "$@" ;;
  verify) shift; cmd_verify "$@" ;;
  publish) shift; cmd_publish "$@" ;;
  *) sed -n '2,25p' "$0"; exit 2 ;;
esac
