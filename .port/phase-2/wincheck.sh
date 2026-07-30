#!/bin/bash
# wincheck.sh — host-side type-check/lint of a crate's #[cfg(windows)] code paths.
#
# WHY: this machine has no Windows SDK / MSVC CRT and no mingw cross toolchain, so any
# crate with a `cc`-driven -sys dependency fails `cargo check --target
# x86_64-pc-windows-msvc` inside the C build script, before rustc ever sees the Rust
# code (see .port/phase-2/VERIFICATION_DEBT.md rows V9/V10/V11 for the exact errors).
#
# `cargo check`/`cargo clippy` never LINK, so the Windows Rust code can still be fully
# type-checked and linted here by forcing the cfg arms:
#     ON  -> cfg(unix)                 (true  on this host)
#     OFF -> cfg(target_os = "wasi")   (false on this host)
# A real cfg predicate is used rather than cfg(all())/cfg(any()) so clippy's
# non_minimal_cfg lint does not fire on the harness's own rewrites.
#
# This proves types, borrows, lints and dead-code. It does NOT prove Win32 struct
# layouts (they are #[repr(C)] declarations checked by no linker and no run) and
# resolves no kernel32 symbol. The real bar is Recipe A in VERIFICATION_DEBT.md.
#
# USAGE:
#     bash .port/phase-2/wincheck.sh "$PWD" rustty-vaxis
#     cargo clippy --manifest-path /tmp/wincheck-rustty-vaxis/Cargo.toml \
#         --all-targets -- -D warnings
#
# The scratch package's Cargo.toml is generated below (workspace lints/deps are
# inlined) because the scratch dir is outside the workspace.
set -euo pipefail
REPO="$1"
CRATE="$2"          # rustty-vaxis | rustty-font-shaper
OUT="/tmp/wincheck-$CRATE"

ON='#[cfg(unix)]'
OFF='#[cfg(target_os = "wasi")]'

rm -rf "$OUT"
mkdir -p "$OUT"
cp -R "$REPO/crates/$CRATE/src" "$OUT/src"
[ -f "$REPO/crates/$CRATE/build.rs" ] && cp "$REPO/crates/$CRATE/build.rs" "$OUT/build.rs"
[ -d "$REPO/crates/$CRATE/ucd" ] && cp -R "$REPO/crates/$CRATE/ucd" "$OUT/ucd"
[ -d "$REPO/crates/$CRATE/testdata" ] && cp -R "$REPO/crates/$CRATE/testdata" "$OUT/testdata"
[ -d "$REPO/crates/$CRATE/tests" ] && cp -R "$REPO/crates/$CRATE/tests" "$OUT/tests"

find "$OUT" -name '*.rs' -print0 | ON="$ON" OFF="$OFF" xargs -0 perl -0pi -e '
  my $on = $ENV{ON}; my $off = $ENV{OFF};
  s/#\[cfg\(all\(not\(windows\), not\(test\)\)\)\]/$off/g;
  s/#\[cfg\(all\(test, not\(windows\)\)\)\]/$off/g;
  s/#\[cfg\(not\(windows\)\)\]/$off/g;
  s/#\[cfg_attr\(windows, allow\(dead_code\)\)\]/#[allow(dead_code)]/g;
  s/#\[cfg_attr\(windows, allow\(unused\)\)\]/#[allow(unused)]/g;
  s/#\[cfg\(all\(windows, not\(test\)\)\)\]/$on/g;
  s/#\[cfg\(windows\)\]/$on/g;
  s/#\[cfg\(not\(target_os = "windows"\)\)\]/$off/g;
  s/#\[cfg\(all\(target_os = "windows", not\(target_arch = "wasm32"\)\)\)\]/$on/g;
  s/#\[cfg\(target_os = "windows"\)\]/$on/g;
  s/#\[cfg\(all\(not\(target_os = "macos"\), not\(target_arch = "wasm32"\), not\(target_os = "windows"\)\)\)\]/$off/g;
  s/#\[cfg\(all\(not\(target_os = "macos"\), not\(target_arch = "wasm32"\)\)\)\]/$off/g;
  s/#\[cfg\(not\(any\(target_os = "windows", target_os = "macos", target_arch = "wasm32"\)\)\)\]/$off/g;
  s/#\[cfg\(not\(any\(target_os = "macos", target_arch = "wasm32", target_os = "windows"\)\)\)\]/$off/g;
  s/#\[cfg\(target_os = "macos"\)\]/$off/g;
  s/#\[cfg\(not\(target_os = "macos"\)\)\]/$on/g;
  s/#\[cfg\(not\(any\(target_os = "macos", target_arch = "wasm32"\)\)\)\]/$on/g;
  s/#\[cfg\(any\(target_os = "macos", target_arch = "wasm32"\)\)\]/$off/g;
  s/#\[cfg\(target_arch = "wasm32"\)\]/$off/g;
  s/#\[cfg\(not\(target_arch = "wasm32"\)\)\]/$on/g;
  s/cfg!\(windows\)/true/g;
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
    # Mirror crates/rustty-font-shaper/Cargo.toml's [dependencies]/[dev-dependencies].
    "$REPO/.port/phase-2/wincheck-manifest.sh" "$REPO" "$CRATE" > "$OUT/Cargo.toml"
    ;;
  *)
    echo "wincheck.sh: no manifest recipe for $CRATE" >&2
    exit 1
    ;;
esac

echo "prepared $OUT"
