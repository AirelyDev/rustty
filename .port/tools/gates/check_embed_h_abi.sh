#!/usr/bin/env bash
#
# check_embed_h_abi.sh — the embedding-header (rustty-embed.h) ABI-SIZE gate.
#
# WHY THIS EXISTS
# ---------------
# The macOS/iOS Swift app compiles against the frozen umbrella header
# `include/rustty-embed.h` (the pinned upstream `ghostty.h`, token-renamed). Every
# aggregate it declares must have a rustty `#[repr(C)]` type with byte-identical
# layout — otherwise the app reads corrupt data, and for a struct RETURNED BY
# VALUE a size mismatch flips the AAPCS64 return convention (>16 bytes → hidden
# x8 sret pointer vs ≤16 bytes → registers) and CRASHES on the first call.
#
# That is exactly the bug this gate was built after: `rustty_config_trigger`
# returned `rustty_input_trigger_s`, whose Rust `TriggerC` embedded the non-repr(C)
# `Mods` struct → 20 bytes vs the header's 12 → sret crash → blank menu shortcuts.
# The existing `abi_layout.rs` freeze never covered it (it is a rustty-config /
# rustty-input type, outside that test's curated list). See
# `.port/phase-2/MACOS_UI_PARITY_GAPS.md`.
#
# WHAT IT CHECKS
# --------------
#   (a) Enumerates every aggregate typedef (`_s`/`_u`) in the umbrella header and
#       clang-probes its authoritative C `sizeof`/`_Alignof` (the ABI the app sees).
#   (b) Enumerates every `RUSTTY_API` function that RETURNS an aggregate BY VALUE —
#       the hard-crash class — and REQUIRES each such return type to carry a Rust
#       size-freeze annotation `// @embed-abi <header_name> <bytes>` (next to a
#       compile-time `const _: () = assert!(size_of::<T>() == bytes)`), and that
#       the annotated size equals the clang-probed header size.
#       A missing annotation is the coverage gap that let TriggerC through → FAIL.
#   (b2) FIELD OFFSETS. Every Rust `// @embed-abi-field <c_struct> <c_field>
#       <offset>` annotation (paired with a compile-time `offset_of!` freeze) is
#       cross-checked against the header's `offsetof(c_struct, c_field)` — catching
#       a same-total-size field TRANSPOSITION that (b)'s sizeof check misses. No
#       exported reflection symbol is added (that would break the rustty-embed.h ==
#       ghostty.h symbol freeze / gate A3), so the Rust side is const-assert
#       freezes enforced by `cargo test --workspace`, and this gate is the header
#       half.
#   (c) Prints a full coverage ledger for ALL aggregates (frozen / unfrozen) so the
#       long tail of pointer-passed structs is visible even though only the
#       by-value-return class is hard-gated here.
#
# Rust freezes live at their definition sites (grep '@embed-abi'):
#   rustty_input_trigger_s   crates/rustty-input/src/binding/trigger.rs
#   rustty_diagnostic_s      crates/rustty-config/src/c_api.rs
#   rustty_info_s            crates/rustty-apprt-c/src/lib.rs
#   rustty_string_s / _surface_config_s / _surface_size_s
#                            crates/rustty-apprt-c/tests/abi_layout.rs
# The `const` asserts make Rust drift a HARD BUILD FAILURE; this gate makes a
# header/annotation drift a HARD CI FAILURE. Together: header == annotation == Rust.
#
# Exit codes (ci.sh run_gate contract): 0 PASS · 1 FAIL · 3 BLOCKED (tooling/inputs)
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
PARITY_ENV="$ROOT/.port/tools/parity_names.env"
[ -f "$PARITY_ENV" ] || { echo "BLOCKED: $PARITY_ENV not found"; exit 3; }
# shellcheck source=/dev/null
source "$PARITY_ENV"
: "${PORT_UMBRELLA:?PORT_UMBRELLA not set}"
UMB="$ROOT/$PORT_UMBRELLA"
[ -f "$UMB" ] || { echo "BLOCKED: umbrella header $UMB missing"; exit 3; }
command -v clang >/dev/null 2>&1 || { echo "BLOCKED: clang not found"; exit 3; }

TMP="$(mktemp -d)"; trap 'rm -rf "$TMP"' EXIT
INCDIR="$(dirname "$UMB")"; HDR="$(basename "$UMB")"
fail=0

# --- (a) enumerate aggregates + clang-probe their C sizeof/alignof -----------
# (portable to bash 3.2 — no `mapfile`)
AGGS=(); while IFS= read -r a; do AGGS+=("$a"); done < <(grep -oE '\} [a-z_]+_(s|u)\b' "$UMB" | sed 's/} //' | sort -u)
[ "${#AGGS[@]}" -gt 0 ] || { echo "BLOCKED: no aggregates parsed from $HDR"; exit 3; }

{
  echo "#include \"$HDR\""
  echo '#include <stdio.h>'
  echo 'int main(void){'
  for a in "${AGGS[@]}"; do
    printf '  printf("%s %%zu %%zu\\n", sizeof(%s), _Alignof(%s));\n' "$a" "$a" "$a"
  done
  echo '  return 0; }'
} > "$TMP/probe.c"

if ! clang -I "$INCDIR" "$TMP/probe.c" -o "$TMP/probe" 2>"$TMP/cc.err"; then
  echo "BLOCKED: clang could not compile the header probe:"; sed 's/^/  /' "$TMP/cc.err"; exit 3
fi
"$TMP/probe" > "$TMP/csizes.txt"   # lines: "<name> <size> <align>"
csize() { awk -v n="$1" '$1==n{print $2}' "$TMP/csizes.txt"; }

# --- gather Rust @embed-abi freeze annotations ------------------------------
# format anywhere in crates/**.rs:  // @embed-abi <header_name> <bytes>
grep -rhoE '@embed-abi +[a-z_]+_(s|u) +[0-9]+' "$ROOT/crates" 2>/dev/null \
  | awk '{print $2, $3}' | sort -u > "$TMP/frozen.txt"
frozen_size() { awk -v n="$1" '$1==n{print $2}' "$TMP/frozen.txt"; }

# --- (b) hard-gate the by-value-return class --------------------------------
BYVAL=(); while IFS= read -r a; do BYVAL+=("$a"); done < <(grep -E 'RUSTTY_API +[a-z_]+_(s|u) +[a-z_]+ *\(' "$UMB" \
  | sed -E 's/^RUSTTY_API +([a-z_]+_(s|u)) .*/\1/' | sort -u)

echo "== by-value-return ABI types (hard-gated crash class) =="
for a in "${BYVAL[@]}"; do
  cs="$(csize "$a")"; fs="$(frozen_size "$a")"
  if [ -z "$cs" ]; then echo "  ?? $a — not found in header probe (name drift?)"; fail=1; continue; fi
  if [ -z "$fs" ]; then
    echo "  FAIL $a — header sizeof=$cs but NO Rust @embed-abi freeze (coverage gap)"; fail=1; continue
  fi
  if [ "$cs" != "$fs" ]; then
    echo "  FAIL $a — header sizeof=$cs != Rust freeze=$fs (layout drift)"; fail=1; continue
  fi
  warn=""; [ "$cs" -gt 16 ] && warn="  <-- WARN >16B: returned via x8 sret; must match exactly"
  echo "  ok   $a — $cs bytes (frozen)$warn"
done

# --- (b2) field-offset cross-check ------------------------------------------
# Every Rust `@embed-abi-field <c_struct> <c_field> <offset>` annotation (paired
# with a compile-time `offset_of!` freeze that cargo enforces) is cross-checked
# against the header's own `offsetof(c_struct, c_field)`. This catches a
# same-total-size field TRANSPOSITION that the sizeof check in (b) misses — the
# residual the embedding gate previously left open (no exported type_json symbol
# is added, preserving the rustty-embed.h == ghostty.h symbol freeze / gate A3).
grep -rhoE '@embed-abi-field +[a-z_]+_(s|u) +[a-z_]+ +[0-9]+' "$ROOT/crates" 2>/dev/null \
  | awk '{print $2, $3, $4}' | sort -u > "$TMP/fields.txt"

nfields=$(wc -l < "$TMP/fields.txt" | tr -d ' ')
echo
echo "== field-offset cross-check ($nfields annotated fields) =="
if [ "$nfields" -gt 0 ]; then
  {
    echo "#include \"$HDR\""
    echo '#include <stddef.h>'
    echo '#include <stdio.h>'
    echo 'int main(void){'
    while read -r st fld off; do
      [ -n "$st" ] || continue
      printf '  printf("%s %s %%zu\\n", offsetof(%s, %s));\n' "$st" "$fld" "$st" "$fld"
    done < "$TMP/fields.txt"
    echo '  return 0; }'
  } > "$TMP/offprobe.c"
  if ! clang -I "$INCDIR" "$TMP/offprobe.c" -o "$TMP/offprobe" 2>"$TMP/off.err"; then
    echo "  BLOCKED: offsetof probe did not compile (C field-name drift?):"; sed 's/^/    /' "$TMP/off.err"; fail=1
  else
    "$TMP/offprobe" > "$TMP/coff.txt"   # lines: "<struct> <field> <c_offset>"
    bad=0
    while read -r st fld off; do
      [ -n "$st" ] || continue
      coff="$(awk -v s="$st" -v f="$fld" '$1==s && $2==f{print $3}' "$TMP/coff.txt")"
      if [ -z "$coff" ]; then
        echo "  FAIL $st.$fld — no header offset (C field-name drift?)"; bad=1; fail=1
      elif [ "$coff" != "$off" ]; then
        echo "  FAIL $st.$fld — header offset=$coff != Rust freeze=$off (field transposition)"; bad=1; fail=1
      fi
    done < "$TMP/fields.txt"
    [ "$bad" -eq 0 ] && echo "  ok — all $nfields field offsets match the header"
  fi
fi

# --- (c) full coverage ledger (report-only for pointer-passed aggregates) ----
echo
echo "== full aggregate coverage ledger (${#AGGS[@]} types; only by-value returns are gated) =="
covered=0
for a in "${AGGS[@]}"; do
  cs="$(csize "$a")"; fs="$(frozen_size "$a")"
  tag="unfrozen"; [ -n "$fs" ] && { tag="frozen=$fs"; covered=$((covered+1)); }
  printf "  %-42s C=%-4s %s\n" "$a" "${cs:-?}" "$tag"
done
echo "  ($covered/${#AGGS[@]} aggregates have a Rust @embed-abi freeze)"

echo
if [ "$fail" -ne 0 ]; then
  echo "check_embed_h_abi: FAIL — an ABI-by-value-return type is uncovered or drifted."
  exit 1
fi
echo "check_embed_h_abi: PASS — every by-value-return ABI type is frozen and matches the header."
exit 0
