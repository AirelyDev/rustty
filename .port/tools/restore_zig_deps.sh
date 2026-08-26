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
# R3 SCOPE — VERIFICATION INPUT ONLY, NEVER A BUILD DEPENDENCY
# ------------------------------------------------------------
# These corpora exist solely so G1b has a zig-side reference to count against. NOTHING
# in any `Cargo.toml` or any `build.rs` in this repository may read `~/.cache/zig`; the
# rustty build graph is fully self-contained (every codec, UCD corpus and asset is
# vendored in-tree). A `build.rs` reaching into `~/.cache/zig` would be an R3 violation.
#   Audit with:  grep -rn 'cache/zig' --include=build.rs --include=Cargo.toml .
#
# DIGEST POLICY — TWO INDEPENDENT PINS
# -----------------------------------
# (1) PRIMARY, toolchain-free: a baked SHA-256 of each TARBALL, computed once from the
#     downloaded bytes on 2026-07-29 and hardcoded in PKGS below. A restore script
#     without a digest check is a supply-chain hole — the gate would then count
#     `test {}` blocks out of whatever the network happened to serve.
# (2) SECONDARY, opportunistic: zig's OWN package content hash, which is literally the
#     package directory name (`libxev-0.0.0-86vtc4Ic…`) that `check_test_counts.py`
#     hardcodes at :162 (libxev), :183 (zf) and :200 (vaxis). When `zig` is on PATH we
#     cross-check it and FAIL LOUDLY on a mismatch. It is a *cross-check*, not the only
#     gate, for the reason in the next paragraph.
#
# WHY (1) WAS ADDED (WS12 T3, 2026-07-29 — a real reproducibility defect):
#   This script originally had ONLY pin (2) and hard-`exit 2`ed when `zig` was absent.
#   The machine has since moved to **Zig 0.16.0**, whose `zig fetch` REFUSES to run
#   unless invoked from inside a Zig project ("error: no build.zig file found, in the
#   current directory or any parent directories") — so the sole restore path was dead on
#   arrival regardless of whether zig was installed. Verified 2026-07-29. The hash
#   ALGORITHM is unchanged: run from a throwaway minimal project, 0.16.0 prints exactly
#   `vaxis-0.5.1-BWNV_LosCQAGmCCNOLljCIw6j6-yt53tji6n6rwJ2BhS`, matching the pin. Hence
#   the fix is (a) a zig-free SHA-256 route so a restore needs no Zig toolchain at all,
#   and (b) running `zig fetch` from a scratch project dir so the cross-check works
#   under 0.16.0. Both pins TIGHTEN the check; neither relaxes it (R5).
#   We still never install an unverified corpus — an unverified zig reference would make
#   G1b's counts accident-controlled, which is strictly worse than BLOCKED.
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

# name|pinned package-dir (== zig's content hash, pinned by check_test_counts.py)|url|tarball sha256
PKGS=(
  "libxev|libxev-0.0.0-86vtc4IcEwCqEYxEYoN_3KXmc6A9VLcm22aVImfvecYs|https://deps.files.ghostty.org/libxev-34fa50878aec6e5fa8f532867001ab3c36fae23e.tar.gz|6003ea6b96e4a518a128f932327d79a11bd30996b13b73baeb29916379487dd7"
  "zf|zf-0.10.3-OIRy8RuJAACKA3Lohoumrt85nRbHwbpMcUaLES8vxDnh|https://deps.files.ghostty.org/zf-3c52637b7e937c5ae61fd679717da3e276765b23.tar.gz|3b015d928af04e9e26272bc15eb4dbb4d9a9d469eb6d290a0ddae673b77c4568"
  "vaxis|vaxis-0.5.1-BWNV_LosCQAGmCCNOLljCIw6j6-yt53tji6n6rwJ2BhS|https://deps.files.ghostty.org/vaxis-7dbb9fd3122e4ffad262dd7c151d80d863b68558.tar.gz|2e72332bc89c5b541ec6e6bd48769e1f3fb757c4006f3d1af940b54f9b088ef6"
)

sha256_of() {
  if command -v shasum >/dev/null 2>&1; then shasum -a 256 "$1" | awk '{print $1}'
  else sha256sum "$1" | awk '{print $1}'; fi
}

# `zig fetch` cross-check (pin 2). Zig 0.16.0 refuses to run outside a Zig project, so
# invoke it from a throwaway minimal one. Echoes the computed content hash on success and
# nothing at all when zig is unusable — an ABSENT cross-check never counts as a pass; the
# caller only ever compares a NON-EMPTY result against the pin.
zig_content_hash() {
  local url="$1" probe
  command -v zig >/dev/null 2>&1 || return 0
  probe="$(mktemp -d "${TMPDIR:-/tmp}/restore_zig_deps_probe.XXXXXX")" || return 0
  printf 'pub fn build(b: *@import("std").Build) void { _ = b; }\n' >"$probe/build.zig"
  printf '.{ .name = .probe, .version = "0.0.0", .fingerprint = 0x1234567812345678, .paths = .{""} }\n' >"$probe/build.zig.zon"
  ( cd "$probe" && zig fetch "$url" 2>/dev/null | tail -1 )
  rm -rf "$probe"
}

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
    # WS12 T3: was `src/widgets/TextInput.zig` ALONE, which let this script report
    # "present and complete" for a corpus the gate would still BLOCK on. Now the full
    # 17-file list G1b actually reads (check_test_counts.py MAP["vaxis"]).
    vaxis)  printf '%s\n' src/Cell.zig src/Key.zig src/Mouse.zig src/GraphemeCache.zig \
              src/Image.zig src/gwidth.zig src/unicode.zig src/Screen.zig \
              src/InternalScreen.zig src/Window.zig src/Parser.zig src/event.zig \
              src/ctlseqs.zig src/queue.zig src/tty.zig src/Loop.zig \
              src/widgets/TextInput.zig ;;
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
  IFS='|' read -r name pkgdir url want <<<"$spec"
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
  mkdir -p "$CACHE"
  if [ ! -w "$CACHE" ]; then
    echo "REFUSING to run: $CACHE is not writable." >&2
    exit 2
  fi
  for spec in "${missing[@]}"; do
    IFS='|' read -r name pkgdir url want <<<"$spec"
    echo "-- fetching $name"
    echo "   url: $url"

    tmpdir="$(mktemp -d "${TMPDIR:-/tmp}/restore_zig_deps.XXXXXX")"
    tarball="$tmpdir/pkg.tar.gz"
    if ! curl -m 300 -fsSL -o "$tarball" "$url"; then
      echo "DOWNLOAD FAILED for $name ($url)" >&2
      rm -rf "$tmpdir"; exit 2
    fi

    # PIN 1 (primary, toolchain-free): SHA-256 of the tarball bytes.
    got_sha="$(sha256_of "$tarball")"
    if [ "$got_sha" != "$want" ]; then
      echo "TARBALL SHA-256 MISMATCH for $name" >&2
      echo "  expected (baked pin): $want" >&2
      echo "  got      (downloaded): $got_sha" >&2
      echo "REFUSING an unverified corpus. If upstream legitimately re-rolled the" >&2
      echo "tarball, the pin must be updated deliberately — never silently accept one." >&2
      rm -rf "$tmpdir"; exit 2
    fi
    echo "   digest OK (tarball sha256 == baked pin): $got_sha"

    # PIN 2 (cross-check): zig's own package content hash == the pinned package dir
    # name. Skipped, never silently passed, when zig is unavailable/unusable.
    got_zig="$(zig_content_hash "$url")"
    if [ -n "$got_zig" ]; then
      if [ "$got_zig" != "$pkgdir" ]; then
        echo "ZIG CONTENT-HASH MISMATCH for $name" >&2
        echo "  expected (pinned by check_test_counts.py): $pkgdir" >&2
        echo "  got      (zig fetch content hash):         $got_zig" >&2
        rm -rf "$tmpdir"; exit 2
      fi
      echo "   cross-check OK (zig content hash == pin): $got_zig"
    else
      echo "   cross-check SKIPPED (zig unavailable) — pin 1 above still binds"
    fi

    # INSTALL. Two routes, in preference order:
    #  (a) zig's own installation. `zig_content_hash` above ran a real `zig fetch`,
    #      which — having matched the pin — has ALREADY unpacked the package into
    #      $CACHE/$pkgdir. Adopt it verbatim so the on-disk layout is byte-identical to
    #      what a normal `zig build` would produce.
    #      In practice (a) rarely fires: once zig has seen a package, its own manifest
    #      records it and `zig fetch` will NOT re-extract into a directory a human
    #      deleted by hand — observed 2026-07-29 restoring `zf`, which fell through to
    #      (b) even with the cross-check green. Hence (b) is the load-bearing route and
    #      is tested as such; (a) is kept because it is the exactly-faithful layout when
    #      it does apply.
    #  (b) toolchain-free tar extraction, for a machine with no usable Zig. NOTE the
    #      one difference, stated rather than glossed: `zig fetch` PRUNES the package to
    #      its `build.zig.zon` `paths` set, whereas tar keeps the whole tarball, so (b)
    #      leaves extras (`.github/`, `README.md`, `src/tui/`, …) that (a) omits. Every
    #      file G1b reads is byte-identical either way (verified 2026-07-29 by
    #      `diff -r` between the two layouts: no content differences, extras only), so
    #      the gate's counts are unaffected.
    if [ -n "$got_zig" ] && present "$pkgdir" "$name"; then
      echo "   installed by zig fetch (pruned-to-paths layout): $CACHE/$pkgdir"
      rm -rf "$tmpdir"
    else
      if ! tar xzf "$tarball" -C "$tmpdir"; then
        echo "EXTRACT FAILED for $name" >&2
        rm -rf "$tmpdir"; exit 2
      fi
      # Each of these tarballs contains exactly one top-level directory.
      top="$(find "$tmpdir" -mindepth 1 -maxdepth 1 -type d | head -1)"
      if [ -z "$top" ]; then
        echo "UNEXPECTED TARBALL LAYOUT for $name (no single top-level directory)" >&2
        rm -rf "$tmpdir"; exit 2
      fi
      rm -rf "${CACHE:?}/${pkgdir:?}"
      mv "$top" "$CACHE/$pkgdir"
      rm -rf "$tmpdir"
      echo "   installed by tar extraction (full-tarball layout): $CACHE/$pkgdir"
    fi

    if ! present "$pkgdir" "$name"; then
      echo "INSTALLED but sentinel files are missing under $CACHE/$pkgdir" >&2
      echo "(the package layout changed; fix the paths, do NOT relax the gate)" >&2
      exit 2
    fi
  done
fi

echo
echo "== paths check_test_counts.py will read =="
for spec in "${PKGS[@]}"; do
  IFS='|' read -r name pkgdir url want <<<"$spec"
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
