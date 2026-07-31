#!/usr/bin/env bash
# check_no_test_setenv.sh — GATE G-ENV (no process-environment mutation from
# test code under crates/rustty-os/) [P3-W7 T3, area-E `E15`]
#
# Why this gate exists
# --------------------
# `setenv(3)`/`unsetenv(3)` are NOT thread-safe against a concurrent `getenv(3)`
# in glibc or Apple libc. Zig's test runner executes a file's `test {}` blocks
# SERIALLY, so upstream could get away with it; Rust's `libtest` runs tests in
# PARALLEL by default, so a `setenv` in one test races every `getenv` in every
# other test in the same binary. That is the recorded
# `parallel-cargo-test-exposes-c-init-races` failure class, and Rust 2024 made
# `std::env::set_var` `unsafe` for exactly this reason.
#
# Upstream `e8525c0fd` + `f8e13f31e` removed the need entirely by threading an
# explicit `std.process.Environ.Map` through `desktopEnvironment`, `homedir.home`,
# `homedir.expandHome`, `path.expand`, `file.allocTmpDir` and the `xdg` helpers,
# and rewriting every affected test to build a LOCAL map. This gate keeps the
# hazard from creeping back.
#
# The ONE legitimate in-process mutator is `os/locale.zig` -> `locale.rs`
# (upstream declares its own file-local `extern "c" setenv/unsetenv` there), and
# it is guarded by its own `assert(!cfg!(test))` tripwire, so its NON-test code
# is exempt. Only `#[cfg(test)]` regions are scanned.
#
# Usage:
#   check_no_test_setenv.sh              # scan (binding)
#   check_no_test_setenv.sh --self-test  # prove the gate flips
#
# Exit 0 = GREEN, non-zero = RED.
set -euo pipefail

GATES_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RUSTTY_ROOT="$(cd "$GATES_DIR/../../.." && pwd)"
PY="${PYTHON:-python3}"

SCAN_ROOT="${1:-}"
if [ "$SCAN_ROOT" = "--self-test" ]; then
  echo "=== G-ENV SELF-TEST (synthetic tree with a test-side setenv) ==="
  tmp="$(mktemp -d "${TMPDIR:-/tmp}/genv_selftest.XXXXXX")"
  mkdir -p "$tmp/src"
  cat > "$tmp/src/clean.rs" <<'EOF'
pub fn f() { let _ = unsafe { setenv(std::ptr::null(), std::ptr::null(), 0) }; }
#[cfg(test)]
mod tests {
    #[test]
    fn t() { let m = std::collections::HashMap::<Vec<u8>, Vec<u8>>::new(); assert!(m.is_empty()); }
}
EOF
  ok=1
  echo "--- [clean] expect GREEN ---"
  if "$0" "$tmp" >/dev/null 2>&1; then echo "  -> exit 0 (GREEN) OK"; else echo "  -> unexpected non-zero"; ok=0; fi
  cat > "$tmp/src/dirty.rs" <<'EOF'
#[cfg(test)]
mod tests {
    #[test]
    fn t() { let _ = crate::env::setenv(c"HOME", c"/tmp"); }
}
EOF
  echo "--- [dirty] expect RED ---"
  if "$0" "$tmp" >/dev/null 2>&1; then echo "  -> unexpected exit 0"; ok=0; else echo "  -> non-zero (RED) OK"; fi
  rm -rf "$tmp"
  [ "$ok" = 1 ] && { echo "SELF-TEST: PASS"; exit 0; } || { echo "SELF-TEST: FAIL"; exit 1; }
fi

SCAN_ROOT="${SCAN_ROOT:-$RUSTTY_ROOT/crates/rustty-os}"

echo "=== G-ENV :: no setenv/unsetenv in #[cfg(test)] code under $(basename "$SCAN_ROOT") ==="

"$PY" - "$SCAN_ROOT" <<'PYEOF'
import os
import re
import sys

root = sys.argv[1]
# A `#[cfg(test)]` attribute opens a test region; we track brace depth from the
# item it annotates to its close, and flag any setenv/unsetenv call inside.
CFG_TEST = re.compile(r'^\s*#\[cfg(_attr)?\(\s*test\b')
CALL = re.compile(r'\b(setenv|unsetenv|_putenv_s|set_var|remove_var)\s*\(')

bad = []
for dirpath, _dirs, files in os.walk(root):
    if os.sep + "target" + os.sep in dirpath + os.sep:
        continue
    for fn in files:
        if not fn.endswith(".rs"):
            continue
        path = os.path.join(dirpath, fn)
        lines = open(path, encoding="utf-8").read().split("\n")
        i = 0
        n = len(lines)
        while i < n:
            if not CFG_TEST.match(lines[i]):
                i += 1
                continue
            # find the start of the annotated item's body
            j = i + 1
            while j < n and "{" not in lines[j]:
                # a `#[cfg(test)] use ...;` / `static X = ...;` has no body
                if lines[j].rstrip().endswith(";"):
                    break
                j += 1
            if j >= n or "{" not in lines[j]:
                i = j + 1
                continue
            depth = 0
            k = j
            while k < n:
                stripped = lines[k].split("//", 1)[0]
                depth += stripped.count("{") - stripped.count("}")
                if CALL.search(stripped):
                    bad.append((path, k + 1, lines[k].strip()))
                if depth <= 0 and k > j:
                    break
                k += 1
            i = k + 1

if bad:
    for path, ln, text in bad:
        rel = os.path.relpath(path, root)
        print(f"  RED {rel}:{ln}: {text}", file=sys.stderr)
    print(
        "\nsetenv/unsetenv from #[cfg(test)] code races every concurrent getenv\n"
        "under cargo test's default parallelism. Thread an explicit EnvMap through\n"
        "the function under test instead (oracle e8525c0fd / f8e13f31e).",
        file=sys.stderr,
    )
    sys.exit(1)
print("  GREEN: no setenv/unsetenv/set_var in #[cfg(test)] regions")
sys.exit(0)
PYEOF
