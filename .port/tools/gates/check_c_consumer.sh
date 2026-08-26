#!/usr/bin/env bash
# GATE G-CCONSUMER — the C/CMake/pkg-config consumer lane.
#
# PORTS: ghostty `.github/workflows/test.yml`'s `build-cmake` and
# `test-lib-vt-pkgconfig` jobs, merged. The reference builds libghostty-vt via
# CMake, checks the installed artifacts, then compiles a trivial C program
# against `pkg-config --cflags --libs` twice — once shared, once static — and
# runs it.
#
# WHY THIS EXISTS. rustty's CMakeLists.txt (400+ lines), its
# dist/cmake/rustty-vt-config.cmake.in, its install rules and its pkg-config
# files are the entire story for C and CMake consumers, and until this gate
# NOTHING exercised any of it: `cargo test` never invokes cmake, and the
# examples that do were not built by CI either. The first run of this gate
# found three real defects (a missing pkg-config pair, an unresolvable
# `OnigEncodingKOI8` in the static archive, and a CMake custom command that
# never re-invoked cargo), which is roughly what an unexercised 400-line build
# file should be expected to accumulate.
#
# WHAT IT ASSERTS
#   1. `cmake -B <dir>` configures and `cmake --build` builds.
#   2. `cmake --install --prefix P` produces: the shared library, the static
#      archive, the public headers, the CMake package config, and BOTH
#      pkg-config files.
#   3. Both .pc files pass `pkg-config --validate`.
#   4. The smoke test compiles, links and RUNS against the shared .pc.
#   5. The smoke test compiles, links and RUNS against the static .pc, and the
#      resulting binary does not depend on the shared library.
#
# Exit contract (`.port/tools/CI.md`): 0 = pass, 1 = red, 3 = BLOCKED.
# BLOCKED when cmake, pkg-config, or a C compiler is unavailable — never a
# green over a lane that did not run.
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$ROOT" || exit 1

CC_BIN="${CC:-cc}"
SRC="$ROOT/test/pkgconfig/test_librustty_vt.c"

blocked() { echo "G-CCONSUMER BLOCKED: $*"; exit 3; }
red()     { echo "G-CCONSUMER RED: $*" >&2; exit 1; }

command -v cmake      >/dev/null 2>&1 || blocked "cmake not installed"
command -v pkg-config >/dev/null 2>&1 || blocked "pkg-config/pkgconf not installed"
command -v "$CC_BIN"  >/dev/null 2>&1 || blocked "no C compiler ($CC_BIN)"
[ -f "$SRC" ]                         || red     "missing smoke-test source $SRC"

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
BUILD="$WORK/build"
PREFIX="$WORK/prefix"

echo "=== G-CCONSUMER 1/5 :: cmake configure ==="
cmake -B "$BUILD" -S "$ROOT" > "$WORK/configure.log" 2>&1 \
  || { sed 's/^/    /' "$WORK/configure.log"; red "cmake configure failed"; }

echo "=== G-CCONSUMER 2/5 :: cmake build ==="
cmake --build "$BUILD" > "$WORK/build.log" 2>&1 \
  || { sed 's/^/    /' "$WORK/build.log"; red "cmake build failed"; }

echo "=== G-CCONSUMER 3/5 :: cmake install + artifact check ==="
cmake --install "$BUILD" --prefix "$PREFIX" > "$WORK/install.log" 2>&1 \
  || { sed 's/^/    /' "$WORK/install.log"; red "cmake install failed"; }

# Shared-library suffix differs per platform; cargo emits no versioned soname.
case "$(uname -s)" in
  Darwin) SHLIB="librustty_vt_c.dylib" ;;
  *)      SHLIB="librustty_vt_c.so"    ;;
esac

REQUIRED=(
  "lib/$SHLIB"
  "lib/librustty_vt_c.a"
  "include/rustty/vt.h"
  "lib/cmake/rustty-vt/rustty-vt-config.cmake"
  "lib/cmake/rustty-vt/rustty-vt-config-version.cmake"
  "share/pkgconfig/librustty-vt.pc"
  "share/pkgconfig/librustty-vt-static.pc"
)
missing=0
for f in "${REQUIRED[@]}"; do
  if [ -e "$PREFIX/$f" ]; then
    echo "    ok      $f"
  else
    echo "    MISSING $f"; missing=1
  fi
done
[ "$missing" -eq 0 ] || red "the install tree is incomplete (see above)"

echo "=== G-CCONSUMER 4/5 :: pkg-config metadata ==="
export PKG_CONFIG_PATH="$PREFIX/share/pkgconfig"
for pc in librustty-vt librustty-vt-static; do
  pkg-config --validate "$pc" || red "$pc failed pkg-config --validate"
  echo "    ok      $pc validates"
done
echo "    Cflags(shared): $(pkg-config --cflags librustty-vt)"
echo "    Libs(shared):   $(pkg-config --libs librustty-vt)"
echo "    Libs(static):   $(pkg-config --libs --static librustty-vt-static)"

echo "=== G-CCONSUMER 5/5 :: compile, link and RUN the smoke test ==="

# Shared. -rpath so the freshly installed dylib is findable without touching
# the environment's library search path.
# shellcheck disable=SC2046  # pkg-config output is an intentional word-split.
"$CC_BIN" -o "$WORK/t_shared" "$SRC" \
  $(pkg-config --cflags --libs librustty-vt) \
  -Wl,-rpath,"$PREFIX/lib" > "$WORK/cc_shared.log" 2>&1 \
  || { grep -v '^ld: warning' "$WORK/cc_shared.log" | sed 's/^/    /'; red "shared link failed"; }
"$WORK/t_shared" > "$WORK/run_shared.log" 2>&1 \
  || { sed 's/^/    /' "$WORK/run_shared.log"; red "the shared-linked binary did not run"; }
echo "    ok      shared:  $(cat "$WORK/run_shared.log")"

# Static. The archive must link into a plain C program using nothing but what
# the .pc advertises.
# shellcheck disable=SC2046  # pkg-config output is an intentional word-split.
"$CC_BIN" -o "$WORK/t_static" "$SRC" \
  $(pkg-config --cflags --libs --static librustty-vt-static) > "$WORK/cc_static.log" 2>&1 \
  || { grep -v '^ld: warning' "$WORK/cc_static.log" | sed 's/^/    /'; red "static link failed"; }
"$WORK/t_static" > "$WORK/run_static.log" 2>&1 \
  || { sed 's/^/    /' "$WORK/run_static.log"; red "the static-linked binary did not run"; }
echo "    ok      static:  $(cat "$WORK/run_static.log")"

# ...and it must not have quietly picked up the shared library instead.
if [ "$(uname -s)" = "Darwin" ]; then deps="$(otool -L "$WORK/t_static" 2>/dev/null)"
else deps="$(ldd "$WORK/t_static" 2>/dev/null)"; fi
if printf '%s' "$deps" | grep -q 'librustty_vt_c'; then
  printf '%s\n' "$deps" | sed 's/^/    /'
  red "the static-linked binary still depends on the shared library"
fi
echo "    ok      static binary is self-contained"

echo "G-CCONSUMER GREEN: cmake build+install, both .pc files, and both link modes verified."
exit 0
