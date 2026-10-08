#!/usr/bin/env bash
# fetch.sh — fetch esctest for esctest-librustty and install it in
# target/share/esctest, beside the target/debug and target/release
# directories that hold the runner, which finds it relative to itself.
#
# PORTS: the `esctest2` dependency of ghostty `test/esctest/build.zig.zon` and
# the `b.installDirectory(...)` of `test/esctest/build.zig` (438d584f0). Cargo
# has no lazy dependency or install step, and the port never fetches from a
# build script, so this is its own command. The commit is upstream's, and the
# tarball is checked against a SHA-256 before anything is extracted (upstream
# checks the Zig package hash of the same tarball).
#
# Usage: test/esctest/fetch.sh
# Honours CARGO_TARGET_DIR, as cargo does. Exit 0 on success (or when the
# pinned commit is already installed), 3 when the download fails (no
# network), 1 on any other failure (a hash mismatch above all).

set -euo pipefail

# The esctest2 commit of 2026-09-13.
COMMIT="2798f12149a19c3295e9b4853ab2da4b2eff1b2b"
URL="https://github.com/ThomasDickey/esctest2/archive/${COMMIT}.tar.gz"
SHA256="04b01a8343045a6801ea9a2a849b339e1b41179197102363a43fab13b25a2e1e"

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
target="${CARGO_TARGET_DIR:-$here/target}"
dest="$target/share/esctest"
stamp="$dest/.esctest2-commit"

if [ -f "$stamp" ] && [ "$(cat "$stamp")" = "$COMMIT" ]; then
  echo "esctest2 $COMMIT is already in $dest"
  exit 0
fi

sha256() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | cut -d' ' -f1
  else
    shasum -a 256 "$1" | cut -d' ' -f1
  fi
}

work="$(mktemp -d "${TMPDIR:-/tmp}/esctest2-fetch.XXXXXX")"
trap 'rm -rf "$work"' EXIT

echo "fetching $URL"
if ! curl -fsSL --retry 3 -o "$work/esctest2.tar.gz" "$URL"; then
  echo "error: could not download $URL" >&2
  exit 3
fi
got="$(sha256 "$work/esctest2.tar.gz")"
if [ "$got" != "$SHA256" ]; then
  echo "error: esctest2 tarball SHA-256 is $got, expected $SHA256" >&2
  exit 1
fi

mkdir "$work/src"
tar -xzf "$work/esctest2.tar.gz" -C "$work/src"
src="$work/src/esctest2-$COMMIT/esctest"
if [ ! -f "$src/esctest.py" ]; then
  echo "error: the tarball has no esctest2-$COMMIT/esctest/esctest.py" >&2
  exit 1
fi
# `.exclude_extensions = &.{".pyc"}`
find "$src" -name '*.pyc' -type f -delete

rm -rf "$dest"
mkdir -p "$(dirname "$dest")"
cp -R "$src" "$dest"
printf '%s\n' "$COMMIT" >"$stamp"
echo "installed esctest2 $COMMIT in $dest"
