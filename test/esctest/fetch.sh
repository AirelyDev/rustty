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
# cargo resolves a relative CARGO_TARGET_DIR against the working directory.
case "$target" in /*) ;; *) target="$PWD/$target" ;; esac
dest="$target/share/esctest"
# The stamp holds the commit and a digest of the installed tree, so a tree
# that was changed after the install is installed again, not trusted.
stamp="$dest/.esctest2-commit"

# sha256 [FILE]: the hex SHA-256 of FILE, or of stdin.
sha256() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$@" | cut -d' ' -f1
  else
    shasum -a 256 "$@" | cut -d' ' -f1
  fi
}

# tree_digest DIR: one SHA-256 over every file's path and SHA-256, leaving out
# the stamp and the bytecode Python writes when esctest runs.
tree_digest() {
  (
    cd "$1" || exit 1
    find . -type f ! -name .esctest2-commit ! -name '*.pyc' ! -path '*/__pycache__/*' -print |
      LC_ALL=C sort | while IFS= read -r f; do
      printf '%s  %s\n' "$(sha256 "$f")" "$f"
    done
  ) | sha256
}

if [ -f "$stamp" ] && [ "$(sed -n 1p "$stamp")" = "$COMMIT" ] &&
  [ "$(sed -n 2p "$stamp")" = "$(tree_digest "$dest")" ]; then
  echo "esctest2 $COMMIT is already in $dest"
  exit 0
fi
if [ -d "$dest" ]; then
  echo "$dest is missing, from another commit or changed since it was installed; installing again"
fi

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
digest="$(tree_digest "$dest")"
printf '%s\n%s\n' "$COMMIT" "$digest" >"$stamp"
echo "installed esctest2 $COMMIT in $dest"
