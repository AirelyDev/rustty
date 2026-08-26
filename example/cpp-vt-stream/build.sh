#!/bin/sh
# Build + run this rustty c-vt example (the rustty analog of ghostty's `zig build
# run`): compile the ported C++ against librustty_vt_c under the oracle-parity
# profile (`identity-oracle` + `simd`, so build-info/version match the reference).
set -e
here=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)
root=$(CDPATH='' cd -- "$here/../.." && pwd)
CC="$(xcrun --find clang++ 2>/dev/null || echo /usr/bin/clang++)"
SDK="$(xcrun --show-sdk-path 2>/dev/null)"
# WS19 T16 (audit row N7): build UNCONDITIONALLY. This was
# `[ -f "$lib" ] || ( ... cargo build ... )`, which accepted whatever
# librustty_vt_c.dylib happened to be lying in target/release — including one built
# from different sources, a different feature set, or a different profile. The example
# then compiled against the CURRENT include/rustty.h and linked a STALE library: either
# a link error that looks like a header bug, or worse, a silent ABI mismatch. cargo is
# already incremental, so the "avoid rebuilding" motive the guard served is void; a
# no-op rebuild costs a fraction of a second. Identical change in all 23
# example/*/build.sh.
( cd "$root" && cargo build -p rustty-vt-c --release --features "identity-oracle simd" >/dev/null )
out="${TMPDIR:-/tmp}/$(basename "$here")"
"$CC" ${SDK:+-isysroot "$SDK"} -std=c++17 -Wall -I"$root/include" -L"$root/target/release" -lrustty_vt_c "$here"/src/*.cpp -o "$out"
DYLD_LIBRARY_PATH="$root/target/release" "$out"
