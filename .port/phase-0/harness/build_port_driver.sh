#!/usr/bin/env bash
# build_port_driver.sh — Compile the PORT-side VT oracle (the SAME vt_driver.c
# the reference uses) against rustty's own librustty_vt_c static library, and
# the PORT-side c-vt example binaries for the cvt golden corpus (G4 leg B).
#
# This is the sibling of build_ref_driver.sh. Both compile the IDENTICAL C
# source (ref_driver/vt_driver.c) so the only thing that differs between the two
# binaries is which VT implementation they link:
#   * vt_driver_ref  -> ghostty's libghostty-vt.a   (the external oracle)
#   * vt_driver_port -> rustty's  librustty_vt_c.a  (the port under test)
#
# SYMBOL IDENTITY (the locked `ghostty_*`-aliases fork): rustty's canonical C
# ABI is `rustty_*`. The shared C sources reference `ghostty_*`. We do NOT fork
# the C sources; instead we generate a linker alias list mapping every exported
# `_rustty_*` symbol to its `_ghostty_*` alias (ld64 `-alias_list`) — the
# "generated alias file" glue of the locked fork, kept harness-local.
#
# IMPORTANT (R3): rustty NEVER depends on the ghostty tree as a build/run dep.
# This script reads the ghostty include layout and example sources ONLY as the
# read-only external oracle for the LOCAL-ONLY differential gate G4 (G4 is
# skipped on CI precisely because the oracle is not shipped there). The header
# read is what proves the port's ABI is reference-identical: the same
# `ghostty/vt.h` compiles against either library.
#
# IDENTITY (R2): the reference lib is built with simd=true (the zig default;
# the c-vt-build-info golden records "SIMD: enabled"), so the port driver build
# enables the matching `rustty-vt/simd` feature — harness build-config parity.
# NOTE: the `identity-ghostty` feature exists today only on rustty-identity /
# rustty-terminfo and is NOT yet plumbed into rustty-vt/rustty-vt-c (version
# identity tokens such as VERSION_STRING still come from CARGO_PKG_VERSION).
# Any resulting identity-token divergence is a REAL G4 finding against the
# identity plumbing, not something this script papers over.
#
# NOTE: in this environment `cc` is aliased to the Claude CLI, so we resolve a
# real C compiler explicitly (xcrun clang on macOS), exactly like the ref build.
set -euo pipefail

HARNESS_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RUSTTY_ROOT="${RUSTTY_ROOT:-/Users/gael/projects/rustty}"
GHOSTTY="${GHOSTTY:-/Users/gael/projects/ghostty}"
OUT="$HARNESS_DIR/bin/vt_driver_port"
CVT_OUT_DIR="$HARNESS_DIR/bin/cvt"

# --- Locate rustty's librustty_vt_c.a (built from rustty-vt-c, staticlib). ----
# Cargo emits staticlibs under target/<profile>/. We accept an explicit override
# and otherwise build + probe the conventional locations.
LIBA="${RUSTTY_VT_LIB:-}"
if [ -z "$LIBA" ]; then
  if command -v cargo >/dev/null 2>&1 && [ -f "$RUSTTY_ROOT/Cargo.toml" ]; then
    echo "build_port_driver: cargo build --release -p rustty-vt-c --features rustty-vt/simd ..." >&2
    # simd mirrors the reference build config (see header note). identity-ghostty
    # is intentionally NOT passed: the feature is not plumbed to rustty-vt-c yet.
    ( cd "$RUSTTY_ROOT" && cargo build --release -p rustty-vt-c --features rustty-vt/simd ) >&2 || \
      echo "build_port_driver: cargo build failed; probing for an existing staticlib anyway." >&2
  fi
  for cand in \
    "$RUSTTY_ROOT/target/release/librustty_vt_c.a" \
    "$RUSTTY_ROOT/target/debug/librustty_vt_c.a"; do
    if [ -f "$cand" ]; then LIBA="$cand"; break; fi
  done
fi

# --- Locate the C ABI header dir (`ghostty/vt.h` layout). ---------------------
# Preference order: an explicit override; a port-emitted identity header
# (cbindgen under the identity profile, when that lands); the read-only
# reference include tree (source `include/`, then `zig-out/include`), which is
# the ABI-parity oracle: the port must link and behave under the REFERENCE
# header, byte-for-byte.
INC="${RUSTTY_VT_INCLUDE:-}"
if [ -z "$INC" ]; then
  for cand in \
    "$RUSTTY_ROOT/target/include" \
    "$GHOSTTY/include" \
    "$GHOSTTY/zig-out/include"; do
    if [ -f "$cand/ghostty/vt.h" ]; then INC="$cand"; break; fi
  done
fi

# --- Honest blocker if prerequisites are absent (no fabrication, R5). ---------
if [ -z "$LIBA" ] || [ ! -f "$LIBA" ]; then
  cat >&2 <<EOF
build_port_driver: BLOCKED — no rustty librustty_vt_c.a available.
  cargo build --release -p rustty-vt-c did not produce a staticlib (and no
  RUSTTY_VT_LIB override was given). Probed:
    RUSTTY_VT_LIB=${RUSTTY_VT_LIB:-<unset>}
    target/{release,debug}/librustty_vt_c.a
EOF
  exit 11
fi

if [ -z "$INC" ] || [ ! -f "$INC/ghostty/vt.h" ]; then
  cat >&2 <<EOF
build_port_driver: BLOCKED — no C ABI header (ghostty/vt.h) found.
  The port driver compiles the shared vt_driver.c against the reference header
  layout (ABI parity, R2). Set RUSTTY_VT_INCLUDE to a dir containing
  ghostty/vt.h. Probed:
    RUSTTY_VT_INCLUDE=${RUSTTY_VT_INCLUDE:-<unset>}
    target/include, $GHOSTTY/include, $GHOSTTY/zig-out/include
EOF
  exit 12
fi

# --- Resolve a real C compiler (avoid the `cc` alias), like the ref build. ----
SYSROOT=""
if command -v xcrun >/dev/null 2>&1; then
  CC="$(xcrun --find clang)"
  CXX="$(xcrun --find clang++)"
  SYSROOT="$(xcrun --show-sdk-path 2>/dev/null || true)"
elif [ -x /usr/bin/clang ]; then
  CC=/usr/bin/clang
  CXX=/usr/bin/clang++
else
  echo "ERROR: no real C compiler found (xcrun clang / /usr/bin/clang)" >&2
  exit 10
fi
SYSROOT_FLAG=()
if [ -n "$SYSROOT" ]; then SYSROOT_FLAG=(-isysroot "$SYSROOT"); fi

mkdir -p "$HARNESS_DIR/bin"

# --- Generate the ghostty_* alias list from the staticlib's rustty_* ABI. -----
# One line per exported symbol: "_rustty_X _ghostty_X". This is the generated
# alias glue of the locked identity fork; it can never go stale because it is
# derived from the staticlib being linked, on every build.
ALIAS_LIST="$HARNESS_DIR/bin/.rustty_ghostty_aliases.txt"
nm -gU "$LIBA" 2>/dev/null \
  | awk '$3 ~ /^_rustty_/ {print $3, "_ghostty_" substr($3, 9)}' \
  | sort -u > "$ALIAS_LIST"
ALIAS_COUNT="$(wc -l < "$ALIAS_LIST" | tr -d ' ')"
if [ "$ALIAS_COUNT" -eq 0 ]; then
  cat >&2 <<EOF
build_port_driver: BLOCKED — $LIBA exports no unmangled _rustty_* symbols.
  The staticlib is a scaffold stub (no C ABI). With no port driver, the
  empty-workspace gate keeps measuring 100% divergence (the soundness control).
EOF
  exit 13
fi

# --- Extra libs the Rust staticlib needs (rustty-os i18n pulls gettext). ------
INTL_LIBS=()
if [ -f /opt/homebrew/opt/gettext/lib/libintl.a ]; then
  INTL_LIBS=(/opt/homebrew/opt/gettext/lib/libintl.a -liconv)
else
  INTL_LIBS=(-lintl -liconv)
fi

echo "CC=$CC"
echo "INC=$INC"
echo "LIBA=$LIBA   (rustty port; rustty-vt/simd on, matching the ref build)"
echo "ALIASES=$ALIAS_COUNT rustty_* -> ghostty_* (ld64 -alias_list)"
echo "OUT=$OUT"

# Same compile/link recipe as the reference, only the linked library differs
# (plus the alias glue + the gettext/iconv deps the Rust lib needs).
if ! "$CC" -std=c11 -O2 -g \
  "${SYSROOT_FLAG[@]}" \
  -I"$INC" \
  "$HARNESS_DIR/ref_driver/vt_driver.c" \
  "$LIBA" \
  -Wl,-alias_list,"$ALIAS_LIST" \
  "${INTL_LIBS[@]}" \
  -framework CoreFoundation -framework CoreText -framework CoreGraphics \
  -framework Foundation \
  -o "$OUT" 2>"$HARNESS_DIR/bin/.port_driver_link.log"; then
  cat "$HARNESS_DIR/bin/.port_driver_link.log" >&2
  rm -f "$OUT"
  cat >&2 <<EOF
build_port_driver: BLOCKED — link against $LIBA failed.
  The shared vt_driver.c could not link against the port staticlib via the
  generated ghostty_* alias list. Missing rustty_* exports are an R1/ABI
  finding, not something to paper over. See the log above.
EOF
  exit 13
fi

echo "OK: built $OUT"
"$OUT" --help >/dev/null 2>&1 || true
file "$OUT" 2>/dev/null || true

# --- PORT-side c-vt example binaries (G4 leg B: the cvt golden corpus). -------
# Each example under the read-only oracle tree ($GHOSTTY/example/<case>/src) is
# a self-contained program with its VT input baked in; the corpus golden
# (.port/phase-0/corpora/cvt/<case>.golden) is the reference build's combined
# stdout+stderr. We compile the SAME example source against the PORT staticlib
# so run_differential.sh can byte-diff the port's combined output per case.
# Oracle absent (CI) => skipped with a warning; G4's cvt leg then reports the
# missing binaries honestly (never a vacuous green).
EXAMPLES_DIR="${GHOSTTY_EXAMPLES:-$GHOSTTY/example}"
CVT_CASES=(
  c-vt c-vt-build-info c-vt-colors c-vt-effects c-vt-encode-focus
  c-vt-encode-key c-vt-encode-mouse c-vt-formatter c-vt-grid-ref-tracked
  c-vt-grid-traverse c-vt-kitty-graphics c-vt-modes c-vt-paste c-vt-render
  c-vt-selection c-vt-selection-gesture c-vt-sgr c-vt-size-report c-vt-static
  c-vt-stream cpp-vt-stream
)
if [ ! -d "$EXAMPLES_DIR" ]; then
  echo "WARN: oracle example dir missing ($EXAMPLES_DIR); skipping port-side cvt example builds." >&2
else
  mkdir -p "$CVT_OUT_DIR"
  built=0
  failed=0
  for case_name in "${CVT_CASES[@]}"; do
    src="$EXAMPLES_DIR/$case_name/src/main.c"
    COMP=("$CC" -std=c11)
    if [ ! -f "$src" ]; then
      src="$EXAMPLES_DIR/$case_name/src/main.cpp"
      COMP=("$CXX" -std=c++17)
    fi
    if [ ! -f "$src" ]; then
      echo "WARN: no source for cvt example $case_name (probed src/main.c, src/main.cpp)" >&2
      failed=$((failed + 1))
      continue
    fi
    if "${COMP[@]}" -O2 -g \
      "${SYSROOT_FLAG[@]}" \
      -I"$INC" \
      "$src" \
      "$LIBA" \
      -Wl,-alias_list,"$ALIAS_LIST" \
      "${INTL_LIBS[@]}" \
      -framework CoreFoundation -framework CoreText -framework CoreGraphics \
      -framework Foundation \
      -o "$CVT_OUT_DIR/$case_name" 2>"$CVT_OUT_DIR/.$case_name.link.log"; then
      built=$((built + 1))
    else
      # An example that fails to link is a REAL finding (missing/incompatible
      # C ABI surface); keep the log, delete any stale binary, keep going so
      # the gate can report every unbuilt case.
      rm -f "$CVT_OUT_DIR/$case_name"
      echo "LINK-FAIL: cvt example $case_name (see $CVT_OUT_DIR/.$case_name.link.log)" >&2
      failed=$((failed + 1))
    fi
  done
  echo "OK: built $built/${#CVT_CASES[@]} port-side cvt example binaries into $CVT_OUT_DIR ($failed failed)"
fi
