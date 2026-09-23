#!/usr/bin/env bash
#
# menu-shortcut-parity.sh — cross-implementation parity gate for the macOS
# menu keyboard shortcuts.
#
# The macOS app populates every menu-item key-equivalent AT RUNTIME by calling
# the embedding C-ABI `<impl>_config_trigger(cfg, action, len)` for each action
# string in AppDelegate.syncMenuShortcuts(...). If that call resolves a
# different trigger (or crashes) in rustty vs the ghostty reference, the visible
# menu shortcuts diverge.
#
# This harness compiles a tiny C oracle against BOTH prebuilt libraries and
# resolves the SAME action list through each `config_trigger`, then diffs the
# results (status / tag / key codepoint / modifiers). Any divergence — or a
# rustty crash — is a parity FAIL.
#
# Prereqs (already produced by the normal builds):
#   - ghostty: macos/GhosttyKit.xcframework/macos-arm64_x86_64/ghostty-internal.a
#   - rustty:  macos/RusttyKit.xcframework/macos-arm64/{libRusttyKit.a,Headers/rustty.h}
#
# Usage:  .port/tools/macos-parity/menu-shortcut-parity.sh [--print-oracle]
set -uo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RUSTTY_ROOT="$(cd "$HERE/../../.." && pwd)"
# The reference checkout: `ghostty` beside the MAIN checkout, resolved by the
# shared shell resolver (the twin of `_gatelib.oracle_root`). The former
# `GHOSTTY_ROOT` override is gone (R5: nothing caller-settable points a parity
# check at a different oracle), and so is its default `$RUSTTY_ROOT/../ghostty`,
# which from a git worktree names `.claude/worktrees/ghostty`. `--print-oracle`
# prints the result and exits; G-ORACLE-SH-SELF (`_oracle_root.sh --self-test`)
# runs a copy of this script that way with the old override set.
# shellcheck source=.port/tools/gates/_oracle_root.sh
source "$RUSTTY_ROOT/.port/tools/gates/_oracle_root.sh" || { echo "PARITY FAIL: cannot source _oracle_root.sh" >&2; exit 1; }
GHOSTTY_ROOT="$(rustty_oracle_root "$RUSTTY_ROOT")"
if [ "${1:-}" = "--print-oracle" ]; then
  echo "$GHOSTTY_ROOT"
  exit 0
fi
OUT="$(mktemp -d)"; trap 'rm -rf "$OUT"' EXIT

# Frameworks both static libs pull in.
FW=(-framework Foundation -framework CoreText -framework CoreGraphics
    -framework CoreFoundation -framework AppKit -framework Metal -framework QuartzCore
    -framework IOKit -framework Carbon -framework CoreVideo -framework AudioToolbox
    -framework CoreServices -framework SystemConfiguration -framework Security
    -framework UniformTypeIdentifiers -framework IOSurface -framework MetalKit
    -framework CoreMedia -framework AVFoundation -framework GameController)
INTL_DIR="$(dirname "$(find /opt/homebrew /usr/local -name 'libintl.dylib' 2>/dev/null | head -1)")"

fail() { echo "PARITY FAIL: $*" >&2; exit 1; }

# --- ghostty oracle ---
G_LIB="$GHOSTTY_ROOT/macos/GhosttyKit.xcframework/macos-arm64_x86_64/ghostty-internal.a"
[ -f "$G_LIB" ] || fail "missing ghostty lib $G_LIB (build the reference app first)"
clang -arch arm64 -I "$GHOSTTY_ROOT/include" "$HERE/ghostty_trigger_oracle.c" "$G_LIB" \
  "${FW[@]}" -lc++ -lz -o "$OUT/g_oracle" || fail "ghostty oracle link failed"
"$OUT/g_oracle" > "$OUT/g.txt" || fail "ghostty oracle CRASHED (exit $?)"

# --- rustty oracle ---
R_XCF="$RUSTTY_ROOT/macos/RusttyKit.xcframework/macos-arm64"
[ -f "$R_XCF/libRusttyKit.a" ] || fail "missing rustty lib $R_XCF/libRusttyKit.a (build RusttyKit first)"
clang -arch arm64 -I "$R_XCF/Headers" "$HERE/rustty_trigger_oracle.c" "$R_XCF/libRusttyKit.a" \
  "${FW[@]}" ${INTL_DIR:+-L"$INTL_DIR" -lintl} -lc++ -lz -o "$OUT/r_oracle" || fail "rustty oracle link failed"
"$OUT/r_oracle" > "$OUT/r.txt" || fail "rustty oracle CRASHED (exit $?) — likely a config_trigger C-ABI mismatch (see MACOS_UI_PARITY_GAPS.md)"

# --- normalize + diff (mask low-4 mod bits, drop the ascii-char glyph) ---
norm() { awk '{
    m=$0; gsub(/mods=0x0*/,"mods=0x",m);
    # keep: action status tag key-token(codepoint or physical id), and masked mods
    printf "%s\n", m
}' "$1" | sed -E "s/ '.'$//"; }

if diff <(norm "$OUT/r.txt") <(norm "$OUT/g.txt") > "$OUT/diff.txt"; then
  n=$(wc -l < "$OUT/r.txt" | tr -d ' ')
  echo "menu-shortcut parity OK — $n actions resolve byte-identically (rustty == ghostty)"
  exit 0
else
  echo "PARITY FAIL: rustty and ghostty resolve different triggers:" >&2
  cat "$OUT/diff.txt" >&2
  exit 1
fi
