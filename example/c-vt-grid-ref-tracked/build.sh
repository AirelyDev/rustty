#!/bin/sh
# Build + run this rustty c-vt example (the rustty analog of ghostty's `zig build
# run`): compile the ported C against librustty_vt_c under the oracle-parity
# profile (`identity-oracle` + `simd`, so build-info/version match the reference).
set -e
here=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
root=$(CDPATH= cd -- "$here/../.." && pwd)
CC="$(xcrun --find clang 2>/dev/null || echo /usr/bin/clang)"
SDK="$(xcrun --show-sdk-path 2>/dev/null)"
lib="$root/target/release/librustty_vt_c.dylib"
[ -f "$lib" ] || ( cd "$root" && cargo build -p rustty-vt-c --release --features "identity-oracle simd" >/dev/null )
out="${TMPDIR:-/tmp}/$(basename "$here")"
"$CC" ${SDK:+-isysroot "$SDK"} -Wall -I"$root/include" -L"$root/target/release" -lrustty_vt_c "$here"/src/*.c -o "$out"
DYLD_LIBRARY_PATH="$root/target/release" "$out"
