#!/bin/bash
# wincheck.sh — host-side type-check/lint of a crate's Windows (#[cfg(windows)] /
# #[cfg(target_os = "windows")]) code paths.
#
# WHY: this machine has no Windows SDK / MSVC CRT and no mingw cross toolchain, so any
# crate with a `cc`-driven -sys dependency fails `cargo check --target
# x86_64-pc-windows-msvc` inside the C build script, before rustc ever sees the Rust
# code (see .port/phase-2/VERIFICATION_DEBT.md rows V9/V10/V11 for the exact errors).
#
# `cargo check`/`cargo clippy` never LINK, so the Windows Rust code can still be fully
# type-checked and linted here, by rewriting the cfg PREDICATES (not whole attributes,
# so rustfmt's line wrapping cannot defeat the rewrite) in a scratch copy of the crate:
#
#     target_os = "macos"    ->  target_os = "wasi"     (false on this host)
#     target_os = "windows"  ->  target_os = "macos"    (true  on this host)
#     bare `windows`         ->  `unix`                 (true  on this host)
#     target_arch = "wasm32" ->  unchanged              (false on this host)
#
# i.e. this host is made to look exactly like a Windows target to every cfg in the
# crate. Real predicates are used (rather than cfg(all())/cfg(any())) so clippy's
# non_minimal_cfg lint does not fire on the harness's own rewrites.
#
# This proves types, borrows, lints and dead-code for the Windows arms. It does NOT
# prove Win32 struct layouts (they are #[repr(C)] declarations checked by no linker and
# no run) and resolves no kernel32 symbol. The real bar is Recipe A in
# VERIFICATION_DEBT.md.
#
# NOTE: the host must be macOS for the mapping above to hold (it relies on
# target_os = "macos" being the host's own os). On a Linux host, swap the two
# target_os values for "linux".
#
# USAGE:
#     bash .port/phase-2/wincheck.sh "$PWD" rustty-vaxis
#     cargo clippy --manifest-path /tmp/wincheck-rustty-vaxis/Cargo.toml \
#         --all-targets -- -D warnings
#
# The scratch package's Cargo.toml is generated below (the workspace's deps are
# inlined) because the scratch dir is outside the workspace.
set -euo pipefail
REPO="$1"
CRATE="$2"          # rustty-vaxis | rustty-font-shaper
OUT="/tmp/wincheck-$CRATE"

rm -rf "$OUT"
mkdir -p "$OUT"
cp -R "$REPO/crates/$CRATE/src" "$OUT/src"
[ -f "$REPO/crates/$CRATE/build.rs" ] && cp "$REPO/crates/$CRATE/build.rs" "$OUT/build.rs"
[ -d "$REPO/crates/$CRATE/ucd" ] && cp -R "$REPO/crates/$CRATE/ucd" "$OUT/ucd"
[ -d "$REPO/crates/$CRATE/testdata" ] && cp -R "$REPO/crates/$CRATE/testdata" "$OUT/testdata"
[ -d "$REPO/crates/$CRATE/vendor" ] && cp -R "$REPO/crates/$CRATE/vendor" "$OUT/vendor"
[ -d "$REPO/crates/$CRATE/tests" ] && cp -R "$REPO/crates/$CRATE/tests" "$OUT/tests"

find "$OUT" -name '*.rs' -print0 | xargs -0 perl -0pi -e '
  s/target_os = "macos"/target_os = "wasi"/g;
  s/target_os = "windows"/target_os = "macos"/g;
  s/cfg!\(windows\)/true/g;
  s/cfg\(windows\)/cfg(unix)/g;
  s/cfg_attr\(windows,/cfg_attr(unix,/g;
  s/not\(windows\)/not(unix)/g;
  s/\(windows,/(unix,/g;
'

case "$CRATE" in
  rustty-vaxis)
    cat > "$OUT/Cargo.toml" <<EOF
[package]
name = "rustty-vaxis"
version = "0.1.0"
edition = "2021"
build = "build.rs"

[dependencies]
libc = "0.2"
rustty-zigimg = { path = "$REPO/crates/rustty-zigimg" }

[dev-dependencies]
rustty-wuffs = { path = "$REPO/crates/rustty-wuffs" }
allocator-api2 = "0.2"

[workspace]
EOF
    ;;
  rustty-font-shaper)
    # Mirrors crates/rustty-font-shaper/Cargo.toml's [dependencies], MINUS the
    # target tables that are off for Windows (fontconfig-sys, the objc2 frameworks)
    # — which is the point: if a Windows arm reached them, this would not compile.
    cat > "$OUT/Cargo.toml" <<EOF
[package]
name = "rustty-font-shaper"
version = "0.1.0"
edition = "2021"
build = "build.rs"

[dependencies]
rustty-vt = { path = "$REPO/crates/rustty-vt" }
rustty-font = { path = "$REPO/crates/rustty-font" }
rustty-render-types = { path = "$REPO/crates/rustty-render-types" }
rustty-util = { path = "$REPO/crates/rustty-util" }
rustty-unicode = { path = "$REPO/crates/rustty-unicode" }
rustty-datastruct = { path = "$REPO/crates/rustty-datastruct" }
allocator-api2 = "0.2"
freetype-sys = { path = "$REPO/crates-sys/freetype-sys" }
harfbuzz-sys = { path = "$REPO/crates-sys/harfbuzz-sys" }

[build-dependencies]
cc = "1"

[workspace]
EOF
    ;;
  *)
    echo "wincheck.sh: no manifest recipe for $CRATE" >&2
    exit 1
    ;;
esac

echo "prepared $OUT"
