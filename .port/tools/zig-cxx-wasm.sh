#!/bin/sh
# WASM C cross-compiler wrapper: routes cc-crate invocations to zig's bundled
# clang + wasi sysroot. The cc crate appends a clang-style `--target=wasm32-wasip1`
# which zig's target parser rejects (UnknownOperatingSystem); strip any such flag
# and force zig's own `-target wasm32-wasi`. Used only for wasm32-wasip1 via
# CC_wasm32_wasip1 (native builds untouched).
set -- "$@"
i=0
for a do
  case "$a" in
    --target=*) ;;
    *) i=$((i+1)); eval "arg_$i=\$a" ;;
  esac
done
set --
j=1
while [ "$j" -le "$i" ]; do
  eval "set -- \"\$@\" \"\$arg_$j\""
  j=$((j+1))
done
exec zig c++ -target wasm32-wasi "$@"
