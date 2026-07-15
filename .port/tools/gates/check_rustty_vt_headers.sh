#!/bin/sh
# ABI-fidelity gate for the curated rustty/vt.h family.
# Spec: docs/superpowers/specs/2026-07-15-rustty-c-header-family-design.md
set -e
root=$(cd "$(dirname "$0")/../../.." && pwd); cd "$root"
lib=target/release/librustty_vt_c.dylib
[ -f "$lib" ] || cargo build -p rustty-vt-c --release >/dev/null 2>&1
[ -f target/wasm32-unknown-unknown/release/rustty_vt_c.wasm ] || cargo build -p rustty-vt-c --target wasm32-unknown-unknown --release >/dev/null 2>&1
CC="$(xcrun --find clang 2>/dev/null || echo /usr/bin/clang)"
T="${TMPDIR:-/tmp}"

echo "== (a) headers compile =="
printf '#include <rustty/vt.h>\nint main(void){return 0;}\n' > "$T/vt_probe.c"
"$CC" -fsyntax-only -Wall -Iinclude "$T/vt_probe.c"
echo "  OK"

echo "== (b) every declared+cbindgen rustty_* symbol is a real export =="
grep -rhoE 'rustty_[a-z0-9_]+' include/rustty/vt.h include/rustty/vt | sort -u > "$T/vt_decl.txt"
wasm=target/wasm32-unknown-unknown/release/rustty_vt_c.wasm
{ nm -gjU "$lib" | sed 's/^_//' | grep '^rustty_'; \
  node -e 'const{readFileSync}=require("fs");WebAssembly.compile(readFileSync(process.argv[1])).then(m=>WebAssembly.Module.exports(m).forEach(e=>console.log(e.name)))' "$wasm"; \
} | grep '^rustty_' | sort -u > "$T/vt_exp.txt"
grep -oE 'rustty_[a-z0-9_]+' include/rustty.h | sort -u > "$T/vt_cbindgen.txt"
missing=$(comm -23 "$(comm -12 "$T/vt_decl.txt" "$T/vt_cbindgen.txt" > "$T/vt_both.txt"; echo "$T/vt_both.txt")" "$T/vt_exp.txt")
if [ -n "$missing" ]; then echo "  MISMATCH — declared+cbindgen but NOT exported:"; echo "$missing" | sed 's/^/    /'; exit 1; fi
echo "  OK ($(wc -l < "$T/vt_both.txt" | tr -d ' ') symbols cross-checked)"

echo "== (c) struct layouts match rustty_type_json() =="
node .port/tools/check-vt-struct-layouts.mjs

echo "VT-HEADERS: GREEN"
