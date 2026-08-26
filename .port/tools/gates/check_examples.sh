#!/usr/bin/env bash
# GATE G-EXAMPLES — build (and where possible run) every example/ program.
#
# PORTS: ghostty `.github/workflows/test.yml`'s `list-examples` +
# `build-examples-zig` / `build-examples-cmake` / `build-examples-swift` jobs.
# The reference enumerates example/ at CI time and builds each one; rustty
# checked in 32 example programs and built none of them.
#
# example/README.md already claims "CI discovers examples by globbing
# `example/*/build.sh`, so a new example needs no workflow edit" — this gate is
# what makes that sentence true. It globs, so adding an example needs no edit
# here either.
#
# THREE FAMILIES, discovered not listed:
#   example/*/build.sh          compile the C/C++ sources against the published
#                               artifacts and RUN the result. 24 of them.
#   example/c-vt-cmake*/        CMakeLists-driven; they demonstrate the
#                               FetchContent/find_package integration itself,
#                               so they are configured + built through cmake.
#                               The -cross one needs a non-host Rust target
#                               installed and reports skipped without it.
#   example/wasm-*/             browser lanes; covered by ci.sh's opt-in
#                               `--webgl` lane, not here. Listed as skipped so
#                               the omission is visible.
#
# Exit contract (`.port/tools/CI.md`): 0 = pass, 1 = red, 3 = BLOCKED.
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$ROOT" || exit 1

command -v cargo >/dev/null 2>&1 || { echo "G-EXAMPLES BLOCKED: cargo not installed"; exit 3; }

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

RAN=0
FAILED=0
SKIPPED=""
FAILED_NAMES=""

ok()   { echo "  ok      $1"; RAN=$((RAN+1)); }
bad()  { echo "  FAIL    $1"; RAN=$((RAN+1)); FAILED=1; FAILED_NAMES="$FAILED_NAMES $1"; }
skip() { echo "  skipped $1 ($2)"; SKIPPED="$SKIPPED $1"; }

# --- family 1: build.sh examples --------------------------------------------
echo "=== G-EXAMPLES 1/3 :: example/*/build.sh (compile + run) ==="
found_sh=0
for f in "$ROOT"/example/*/build.sh; do
  [ -f "$f" ] || continue
  found_sh=$((found_sh+1))
  name="$(basename "$(dirname "$f")")"
  if sh "$f" > "$WORK/$name.log" 2>&1; then
    ok "$name"
  else
    tail -20 "$WORK/$name.log" | sed 's/^/          /'
    bad "$name"
  fi
done
if [ "$found_sh" -eq 0 ]; then
  echo "G-EXAMPLES BLOCKED: no example/*/build.sh matched — the glob this gate"
  echo "  and example/README.md both rely on has stopped finding anything."
  exit 3
fi

# --- family 2: CMake examples ------------------------------------------------
echo "=== G-EXAMPLES 2/3 :: example/c-vt-cmake* (cmake configure + build) ==="
if ! command -v cmake >/dev/null 2>&1; then
  skip "c-vt-cmake*" "cmake not installed"
else
  for d in "$ROOT"/example/c-vt-cmake*/; do
    [ -f "$d/CMakeLists.txt" ] || continue
    name="$(basename "$d")"

    # The cross example builds for a NON-host triple; without that target
    # installed via rustup it cannot work, and reporting red for a missing
    # toolchain would be a lie about the example.
    extra=()
    if [ "$name" = "c-vt-cmake-cross" ]; then
      # Mirror the example's own RUST_TARGET derivation (its CMakeLists.txt
      # normalises arm64/ARM64 -> aarch64 and AMD64 -> x86_64, then picks an
      # OS different from the host).
      arch="$(uname -m)"
      case "$arch" in
        x86_64|AMD64)         arch="x86_64" ;;
        aarch64|arm64|ARM64)  arch="aarch64" ;;
      esac
      case "$(uname -s)" in
        Linux)  want="$arch-pc-windows-gnu" ;;
        Darwin) want="$arch-unknown-linux-gnu" ;;
        *)      want="" ;;
      esac
      if [ -z "$want" ] || ! rustup target list --installed 2>/dev/null | grep -qx "$want"; then
        skip "$name" "rust target ${want:-unknown} not installed (rustup target add $want)"
        continue
      fi
      extra=(-DRUST_TARGET="$want")
    fi

    # For the cross example, build only the LIBRARY target.
    #
    # The final executable link additionally needs GNU gettext's libintl FOR
    # THE TARGET: crates/rustty-os/src/i18n.rs declares
    # `_libintl_locale_name_canonicalize` extern on non-macOS, exactly as the
    # reference's src/os/i18n.zig does, and that symbol is a gnulib internal
    # exported by GNU libintl — NOT by glibc. The reference's Linux builds
    # resolve it because its Nix devShell puts gettext on the link line; zig's
    # bundled Linux sysroot ships no GNU libintl, so the executable link cannot
    # complete on a plain cross host. That is an environment prerequisite, not
    # something rustty's build files control.
    #
    # The LIBRARY half is rustty's, and it is where every cross defect found so
    # far lived (the cc-rs compiler/ar/ranlib environment, the clang-style
    # --target= flag zig rejects, the CMake function-scope include leak, and
    # the missing non-Windows link interface). That is what this gate binds.
    build_args=()
    if [ "$name" = "c-vt-cmake-cross" ]; then
      build_args=(--target cargo_build_lib_vt_cross)
    fi

    bd="$WORK/build-$name"
    # `${arr[@]+"${arr[@]}"}`, not `"${arr[@]}"`: bash 3.2 (what macOS ships)
    # treats an empty array as unset under `set -u` and aborts.
    if cmake -B "$bd" -S "$d" -DFETCHCONTENT_SOURCE_DIR_RUSTTY="$ROOT" ${extra[@]+"${extra[@]}"} \
         > "$WORK/$name.log" 2>&1 \
       && cmake --build "$bd" ${build_args[@]+"${build_args[@]}"} >> "$WORK/$name.log" 2>&1; then
      # Run it when it is a host binary (the cross one is not runnable here).
      if [ "$name" = "c-vt-cmake-cross" ]; then
        ok "$name (library built for $want, not run — cross target)"
      else
        bin="$(find "$bd" -maxdepth 1 -type f -perm -u+x ! -name '*.cmake' ! -name 'Makefile' 2>/dev/null | head -1)"
        if [ -n "$bin" ] && "$bin" > "$WORK/$name.run.log" 2>&1; then
          ok "$name -> $(head -1 "$WORK/$name.run.log")"
        elif [ -n "$bin" ]; then
          tail -10 "$WORK/$name.run.log" | sed 's/^/          /'
          bad "$name (built but did not run)"
        else
          ok "$name (built; no runnable binary found)"
        fi
      fi
    else
      tail -20 "$WORK/$name.log" | sed 's/^/          /'
      bad "$name"
    fi
  done
fi

# --- family 3: wasm examples -------------------------------------------------
echo "=== G-EXAMPLES 3/3 :: example/wasm-* ==="
for d in "$ROOT"/example/wasm-*/; do
  [ -d "$d" ] || continue
  skip "$(basename "$d")" "browser lane — run 'bash .port/tools/ci.sh --webgl'"
done

echo "--- G-EXAMPLES summary: $RAN built, $(printf '%s' "$SKIPPED" | wc -w | tr -d ' ') skipped ---"
if [ "$FAILED" -ne 0 ]; then
  echo "G-EXAMPLES RED:$FAILED_NAMES"
  exit 1
fi
echo "G-EXAMPLES GREEN ($RAN example(s) built).${SKIPPED:+ Not built:$SKIPPED}"
exit 0
