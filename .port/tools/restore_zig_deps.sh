#!/usr/bin/env bash
# WS19 T1 (N4): restore the three vendored zig-dep source corpora that gate G1b
# (`.port/tools/gates/check_test_counts.py`) greps as its ZIG reference side.
#
# WHY THIS EXISTS
# ---------------
# G1b's zig side reads three EXTERNAL, read-only checkouts under `~/.cache/zig/p/`
# (R3: the oracle and its deps are references, never build inputs). `~/.cache/zig/p/`
# is a CACHE — a `zig` upgrade or a plain `rm -rf ~/.cache` evicts it. When it is gone
# the gate exits 3 BLOCKED (never a silent green: a missing reference would count zig=0
# and turn every mapped module VACUOUSLY green, R5). This script is the documented way
# back to rc=0.
#
# DIGEST POLICY
# -------------
# The pin is zig's OWN package content hash, which is literally the package directory
# name (`libxev-0.0.0-86vtc4Ic…`) that `check_test_counts.py` hardcodes at :162 (libxev),
# :183 (zf) and :200 (vaxis). `zig fetch` recomputes that hash over the unpacked tree and
# prints it; we require it to equal the pin byte-for-byte and FAIL LOUDLY otherwise. We
# never fall back to an unverified fetch — an unverified corpus would make G1b's zig
# counts accident-controlled, which is strictly worse than BLOCKED.
#
# The tarball URLs come from the oracle's read-only `build.zig.zon.json`. NOTE that that
# file's `hash:` field is a Nix NAR hash of the UNPACKED tree, not a sha256 of the
# tarball — do not try to verify the tarball against it; it will never match (verified
# 2026-07-29: expected sha256-1B9oJEx…, tarball is sha256-YAPqa5b…).
#
# The URLs and pins are hardcoded ON PURPOSE and are NOT env-overridable:
# `.port/tools/ci.sh:114-129` records why an env-redirectable reference path was killed
# as a spoof hole (the probe passes against a fake dir while the gate counts zig=0
# everywhere -> vacuous green, R5).
#
# Usage:
#   bash .port/tools/restore_zig_deps.sh                 # fetch what is missing, then run G1b
#   bash .port/tools/restore_zig_deps.sh --verify-only    # presence check only; no fetch
#                                                         # (this is what ci.sh calls)
set -euo pipefail

ROOT="$(git -C "$(dirname "${BASH_SOURCE[0]}")" rev-parse --show-toplevel)"
CACHE="${HOME}/.cache/zig/p"

VERIFY_ONLY=0
[ "${1:-}" = "--verify-only" ] && VERIFY_ONLY=1

# name|pinned package-dir (== zig's content hash, pinned by check_test_counts.py)|url
PKGS=(
  "libxev|libxev-0.0.0-86vtc4IcEwCqEYxEYoN_3KXmc6A9VLcm22aVImfvecYs|https://deps.files.ghostty.org/libxev-34fa50878aec6e5fa8f532867001ab3c36fae23e.tar.gz"
  "zf|zf-0.10.3-OIRy8RuJAACKA3Lohoumrt85nRbHwbpMcUaLES8vxDnh|https://deps.files.ghostty.org/zf-3c52637b7e937c5ae61fd679717da3e276765b23.tar.gz"
  "vaxis|vaxis-0.5.1-BWNV_LosCQAGmCCNOLljCIw6j6-yt53tji6n6rwJ2BhS|https://deps.files.ghostty.org/vaxis-7dbb9fd3122e4ffad262dd7c151d80d863b68558.tar.gz"
)

# The exact files G1b will read out of each corpus. A directory that exists but is
# missing these is NOT "present" — that is the incomplete-corpus case the gate's
# BLOCKED arm names explicitly.
sentinels_for() {
  case "$1" in
    libxev) printf '%s\n' src/queue.zig src/queue_mpsc.zig src/heap.zig src/darwin.zig \
              src/watcher/async.zig src/watcher/process.zig src/watcher/stream.zig \
              src/watcher/timer.zig src/watcher/common.zig src/backend/kqueue.zig \
              src/backend/epoll.zig src/backend/io_uring.zig src/api.zig src/backend.zig \
              src/main.zig src/loop.zig src/ThreadPool.zig src/dynamic.zig \
              src/linux/timerfd.zig ;;
    zf)     printf '%s\n' src/zf/zf.zig src/zf/filter.zig ;;
    vaxis)  printf '%s\n' src/widgets/TextInput.zig ;;
  esac
}

present() { # $1=pkgdir $2=name
  local dir="$CACHE/$1" f
  [ -d "$dir" ] || return 1
  while read -r f; do [ -f "$dir/$f" ] || return 1; done < <(sentinels_for "$2")
  return 0
}

echo "== restore_zig_deps: G1b zig-reference corpora =="
echo "cache root: $CACHE"

missing=()
for spec in "${PKGS[@]}"; do
  IFS='|' read -r name pkgdir url <<<"$spec"
  if present "$pkgdir" "$name"; then
    echo "  OK      $name  -> $CACHE/$pkgdir"
  else
    echo "  MISSING $name  -> $CACHE/$pkgdir"
    missing+=("$spec")
  fi
done

if [ "$VERIFY_ONLY" -eq 1 ]; then
  if [ "${#missing[@]}" -gt 0 ]; then
    echo "VERIFY-ONLY: ${#missing[@]} corpus/corpora absent or incomplete."
    echo "Restore with: bash .port/tools/restore_zig_deps.sh"
    exit 1
  fi
  echo "VERIFY-ONLY: all corpora present and complete."
  exit 0
fi

if [ "${#missing[@]}" -eq 0 ]; then
  echo "nothing to fetch (idempotent: present, complete corpora are left alone)"
else
  if ! command -v zig >/dev/null 2>&1; then
    echo "zig is not on PATH; it is REQUIRED here." >&2
    echo "The digest pin is zig's own package content hash, and only 'zig fetch' can" >&2
    echo "compute it. Installing an unverified corpus is not an option (an unverified" >&2
    echo "zig reference makes G1b's counts untrustworthy, which is worse than BLOCKED)." >&2
    exit 2
  fi
  for spec in "${missing[@]}"; do
    IFS='|' read -r name pkgdir url <<<"$spec"
    echo "-- fetching $name"
    echo "   url: $url"
    got="$(zig fetch "$url")"
    if [ "$got" != "$pkgdir" ]; then
      echo "DIGEST MISMATCH for $name" >&2
      echo "  expected (pinned by check_test_counts.py): $pkgdir" >&2
      echo "  got      (zig fetch content hash):        $got" >&2
      echo "REFUSING an unverified corpus. If upstream legitimately moved, the gate's" >&2
      echo "pin must be updated deliberately — never silently accept a new hash here." >&2
      exit 2
    fi
    echo "   digest OK (zig content hash == pin): $got"
    if ! present "$pkgdir" "$name"; then
      echo "FETCHED but sentinel files are missing under $CACHE/$pkgdir" >&2
      echo "(the package layout changed; fix the paths, do NOT relax the gate)" >&2
      exit 2
    fi
    echo "   installed: $CACHE/$pkgdir"
  done
fi

echo
echo "== paths check_test_counts.py will read =="
for spec in "${PKGS[@]}"; do
  IFS='|' read -r name pkgdir url <<<"$spec"
  while read -r f; do echo "  $CACHE/$pkgdir/$f"; done < <(sentinels_for "$name")
done

echo
echo "== re-running G1b =="
set +e
python3 "$ROOT/.port/tools/gates/check_test_counts.py"
rc=$?
set -e
echo "check_test_counts.py rc=$rc"
exit "$rc"
