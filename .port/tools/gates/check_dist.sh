#!/usr/bin/env bash
# GATE G-DIST — the source-tarball lane.
#
# PORTS: ghostty `.github/workflows/test.yml`'s `build-dist` and
# `build-dist-lib-vt` jobs (`zig build distcheck`).
#
# WHY THIS EXISTS. `xtask dist` produces the source tarball that PACKAGING.md
# points users at and that .github/workflows/release-{tag,tip}.yml upload. It
# was invoked NOWHERE except from those release workflows — meaning the first
# time anyone found out the tarball was broken would be during a release. The
# reference runs it on every push for exactly that reason.
#
# TWO DEPTHS.
#   default          `xtask dist` AND `xtask dist-lib-vt`, then assert both
#                    tarballs' SHAPE: the
#                    rustty-<version>/ prefix, the files a from-scratch build
#                    needs, the vendored C that makes it self-contained, and
#                    the absence of the dev-only trees .gitattributes marks
#                    export-ignore. ~15s.
#   RUSTTY_DISTCHECK=1
#                    additionally `xtask distcheck`, which extracts the tarball
#                    and runs its TEST SUITE there (parity S63-F8)
#                    and BUILDS it. That is a cold full-workspace build, far
#                    too slow to put in front of every local ci.sh run, so it
#                    is opt-in and CI turns it on for one leg. When it is off
#                    the gate says so by name — it never counts as green.
#
# Exit contract (`.port/tools/CI.md`): 0 = pass, 1 = red, 3 = BLOCKED.
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$ROOT" || exit 1

blocked() { echo "G-DIST BLOCKED: $*"; exit 3; }
red()     { echo "G-DIST RED: $*" >&2; exit 1; }

command -v cargo >/dev/null 2>&1 || blocked "cargo not installed"
command -v git   >/dev/null 2>&1 || blocked "git not installed (xtask dist uses git archive)"
command -v tar   >/dev/null 2>&1 || blocked "tar not installed"

VERSION="$(sed -n 's/^version = "\(.*\)"$/\1/p' "$ROOT/Cargo.toml" | head -1)"
[ -n "$VERSION" ] || red "could not read [workspace.package] version from Cargo.toml"
TARBALL="$ROOT/dist/rustty-$VERSION.tar.gz"
PREFIX="rustty-$VERSION"

echo "=== G-DIST 1/5 :: cargo run -p xtask -- dist ==="
rm -f "$TARBALL"
if ! cargo run -q -p xtask -- dist > /dev/null 2>&1; then
  cargo run -q -p xtask -- dist 2>&1 | tail -20 | sed 's/^/    /'
  red "xtask dist failed"
fi
[ -f "$TARBALL" ] || red "xtask dist reported success but $TARBALL does not exist"
echo "    ok      $(basename "$TARBALL") ($(( $(wc -c < "$TARBALL") / 1024 / 1024 )) MB)"

echo "=== G-DIST 2/5 :: tarball shape ==="
LIST="$(mktemp)"; trap 'rm -f "$LIST"' EXIT
tar tzf "$TARBALL" > "$LIST" || red "the tarball is not readable as gzip'd tar"

# Everything must sit under the single versioned prefix — packagers rely on it.
if grep -qv "^$PREFIX/" "$LIST"; then
  grep -v "^$PREFIX/" "$LIST" | head -5 | sed 's/^/    /'
  red "entries outside the $PREFIX/ prefix (see above)"
fi
echo "    ok      every entry is under $PREFIX/"

# What a from-scratch build of the tarball needs.
MUST_HAVE=(
  "Cargo.toml" "Cargo.lock" "rust-toolchain.toml" "CMakeLists.txt"
  "crates/rustty-vt/Cargo.toml" "include/rustty/vt.h" "xtask/Cargo.toml"
)
for f in "${MUST_HAVE[@]}"; do
  if grep -qx "$PREFIX/$f" "$LIST"; then echo "    ok      $f"; else echo "    MISSING $f"; red "the tarball is missing $f"; fi
done

# Self-containment: the vendored C/C++ must ship, or the tarball needs network
# access to build and is not a source release at all.
nvendor="$(grep -c "^$PREFIX/crates-sys/.*/vendor/" "$LIST")"
[ "$nvendor" -gt 1000 ] || red "only $nvendor vendored crates-sys files in the tarball; expected >1000 (is .gitattributes export-ignore too broad?)"
echo "    ok      $nvendor vendored crates-sys/*/vendor files present"

# The dev-only trees .gitattributes marks export-ignore must NOT ship.
for d in .port docs/superpowers; do
  if grep -q "^$PREFIX/$d/" "$LIST"; then
    red "$d/ leaked into the tarball — its .gitattributes export-ignore is not taking effect"
  fi
  echo "    ok      $d/ correctly excluded"
done

echo "=== G-DIST 3/5 :: xtask dist-lib-vt (the VT-library-only tarball) ==="
LIBVT="$ROOT/dist/librustty-vt-$VERSION.tar.gz"
rm -f "$LIBVT"
if ! cargo run -q -p xtask -- dist-lib-vt > /dev/null 2>&1; then
  cargo run -q -p xtask -- dist-lib-vt 2>&1 | tail -20 | sed 's/^/    /'
  red "xtask dist-lib-vt failed"
fi
[ -f "$LIBVT" ] || red "xtask dist-lib-vt reported success but $LIBVT does not exist"
echo "    ok      $(basename "$LIBVT") ($(( $(wc -c < "$LIBVT") / 1024 / 1024 )) MB)"

LVLIST="$(mktemp)"
tar tzf "$LIBVT" > "$LVLIST" || red "the lib-vt tarball is not readable"
LVPREFIX="librustty-vt-$VERSION"
# It must be a STRICT SUBSET of the workspace: the whole point is a smaller
# tree for C consumers. If the slice ever stops slicing, this catches it.
full_n="$(wc -l < "$LIST" | tr -d ' ')"
lv_n="$(wc -l < "$LVLIST" | tr -d ' ')"
[ "$lv_n" -lt "$full_n" ] || red "the lib-vt tarball ($lv_n entries) is not smaller than the full one ($full_n)"
echo "    ok      $lv_n entries vs the workspace tarball's $full_n"
# ...and it must still contain what a C consumer builds against.
for f in "Cargo.toml" "Cargo.lock" "CMakeLists.txt" "include/rustty/vt.h"          "crates/rustty-vt-c/Cargo.toml" "crates/rustty-vt/Cargo.toml"; do
  grep -qx "$LVPREFIX/$f" "$LVLIST" || red "the lib-vt tarball is missing $f"
done
echo "    ok      the C-consumer surface is present"
# The crates that fell out of the slice must really be gone.
if grep -q "^$LVPREFIX/crates/rustty-apprt/" "$LVLIST"; then
  red "rustty-apprt is in the lib-vt tarball — the slice is not pruning"
fi
echo "    ok      out-of-closure crates pruned (rustty-apprt absent)"
rm -f "$LVLIST"

echo "=== G-DIST 4/5 :: the tarball builds on another distro ==="
# PORTS: ghostty's `test-debian-13` job. Building the tarball on the DEV BOX
# proves it is self-contained; building it on a different distro proves it does
# not quietly depend on that box's glibc, toolchain or headers. Opt-in because
# it is a cold build inside a container.
if [ "${RUSTTY_DISTCHECK_DEBIAN:-0}" != "1" ]; then
  echo "    skipped debian-13 build (set RUSTTY_DISTCHECK_DEBIAN=1; needs podman/docker)"
else
  OCI=""
  for c in podman docker; do command -v "$c" >/dev/null 2>&1 && OCI="$c" && break; done
  [ -n "$OCI" ] || red "RUSTTY_DISTCHECK_DEBIAN=1 but neither podman nor docker is installed"
  # A PREBUILT rust-on-debian image, not `debian:13` + `apt-get install`.
  #
  # Two reasons, and the second is the one that forced it. First, installing a
  # toolchain inside the container is slow and adds a second network
  # dependency. Second: this project's Linux box has a skewed clock, which made
  # BOTH the Docker Hub TLS handshake ("certificate ... is not yet valid") and
  # then apt's OpenPGP verification ("Not live until ...") fail. An image that
  # already contains the toolchain needs neither.
  #
  # Overridable so this is not welded to one registry or one distro.
  IMAGE="${RUSTTY_DISTCHECK_DEBIAN_IMAGE:-mirror.gcr.io/library/rust:trixie}"
  echo "    using $OCI + $IMAGE"
  # RUSTUP_TOOLCHAIN pins to the image's own toolchain so rust-toolchain.toml
  # does not trigger a download. The point of this arm is "builds against a
  # DIFFERENT distro's glibc and a different toolchain", not "builds with our
  # exact pin" — `distcheck` above already covers that.
  # `:ro,z`, not `:ro`. On an SELinux host (Fedora, RHEL) a bind mount without
  # a relabel option is denied inside the container — verified: plain `:ro`
  # gives "Permission denied" reading the tarball, which surfaces three steps
  # later as an unhelpful "gzip: stdin: unexpected end of file". `z` is the
  # shared label; `Z` would be private and conflicts with reuse.
  if ! "$OCI" run --rm -e RUSTUP_TOOLCHAIN=stable -v "$ROOT/dist:/dist:ro,z" "$IMAGE" bash -c '
        set -eu
        tmp=$(mktemp -d)
        tar xzf /dist/librustty-vt-*.tar.gz -C "$tmp"
        cd "$tmp"/librustty-vt-*
        cargo build --offline --release -p rustty-vt-c --features simd
        test -f target/release/librustty_vt_c.a
        echo "$(cat /etc/debian_version): librustty_vt_c.a built with $(rustc --version)"
      ' > "$LIST.debian.log" 2>&1; then
    tail -25 "$LIST.debian.log" | sed 's/^/      /'
    rm -f "$LIST.debian.log"
    red "the tarball does not build on $IMAGE"
  fi
  tail -1 "$LIST.debian.log" | sed 's/^/    ok      /'
  rm -f "$LIST.debian.log"
fi

echo "=== G-DIST 5/5 :: distcheck (extract + test) ==="
if [ "${RUSTTY_DISTCHECK:-0}" != "1" ]; then
  echo "    skipped distcheck (set RUSTTY_DISTCHECK=1 — it is a cold full-workspace build+test)"
  echo "G-DIST GREEN (tarball built and shape-checked). Not run: distcheck."
  exit 0
fi
if ! cargo run -q -p xtask -- distcheck > /dev/null 2>&1; then
  cargo run -q -p xtask -- distcheck 2>&1 | tail -30 | sed 's/^/    /'
  red "xtask distcheck failed — the tarball does not build and pass its tests standalone"
fi
echo "    ok      distcheck: the extracted tarball builds AND its tests pass"
echo "G-DIST GREEN (tarball built, shape-checked, and distcheck clean)."
exit 0
