#!/usr/bin/env bash
# GATE G-XTASK — the xtask subcommands nothing else exercises.
#
# `xtask` IS rustty's `build.zig` (R4: it replaces `src/build/`'s 34 Zig steps).
# Grepping every `xtask -- <cmd>` invocation across `.port/tools/` and
# `.github/workflows/` before this gate existed turned up: dist, dist-lib-vt,
# distcheck, header-gen, header-gen-apprt, release-info, update-translations,
# xcframework-vt. Missing from that list — and therefore run by NOTHING:
#
#   resources                  <- the big one
#   gen                        <- the +gen-* data CLI
#   wasm-patch-growable-table
#   xcframework                <- the APP bundle (only the -vt variant was gated)
#
# `resources` is the one that matters. It generates ~1,200 files — man pages,
# the compiled terminfo database, 32 .mo catalogues, themes, shell integration,
# and the Linux desktop/dbus/systemd units — and its only callers were the
# Flatpak and Snap manifests. In the reference this whole surface is part of the
# default `zig build` install and is covered by build-linux/build-macos; here it
# sat behind zero gates while being release-critical.
#
# ── WHAT IT CHECKS ───────────────────────────────────────────────────────────
# 1. `resources` for the HOST, with per-family floors (not just "exit 0"): a
#    generator that silently emits an empty tree would otherwise pass. It also
#    asserts NO website data (`-Demit-webdata` defaults to false).
# 2. `resources --target-os linux --optimize ReleaseFast`, which additionally
#    stages the desktop-app resources. Pure file generation, so it runs from any
#    host — which means the Linux packaging layout is checked on the macOS leg
#    too. `--optimize ReleaseFast` because the app id follows the optimize mode
#    (a Debug run installs `io.airely.rustty-debug`, as the reference does), and
#    `--emit-webdata true`, whose three .mdx files are the positive half of
#    arm 1's absence check.
# 2b. `resources --target-os linux` with the default optimize (Debug) installs
#    the `-debug` app id and not the release one (N371, end to end).
# 3. `resources --target-os linux --flatpak --optimize ReleaseFast`, the arm the
#    Flatpak manifest actually passes: the D-Bus service is swapped and icons
#    capped at 512.
# 4. `gen` for every documented action, each asserted non-empty.
# 5. `wasm-patch-growable-table` end to end over a hand-built minimal module,
#    asserting the table limits flag flips 0x01 -> 0x00 and the max is dropped.
#    The patch FUNCTION has unit tests; this covers the subcommand plumbing
#    (argv, read, write) that they do not reach.
# 6. `xcframework` (the app bundle) under RUSTTY_XTASK_XCFRAMEWORK=1 — a
#    multi-arch build of rustty-apprt-c, too slow for every run, and reported
#    skipped BY NAME when off. CI does not set it: the `macos-app` job in
#    ci.yml builds the bundle outright as the first step of building the app,
#    which is stronger coverage than this arm, so setting it here would pay for
#    the same multi-arch build twice. Set it locally to check the bundle
#    without going through Xcode.
#
# Exit contract (`.port/tools/CI.md`): 0 = pass, 1 = red, 3 = BLOCKED.
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$ROOT" || exit 1

command -v cargo >/dev/null 2>&1 || { echo "G-XTASK BLOCKED: cargo not installed"; exit 3; }

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

RAN=0
FAILED=0
ok()  { echo "  ok      $1"; RAN=$((RAN+1)); }
bad() { echo "  FAIL    $1"; RAN=$((RAN+1)); FAILED=1; }

XTASK=(cargo run -q -p xtask --)

# `resources` writes to zig-out/share, the path the reference uses and the
# packaging manifests copy from. Save and restore whatever is there so the gate
# is not destructive to a working tree mid-package.
SHARE="$ROOT/zig-out/share"
STASH=""
if [ -d "$SHARE" ]; then
  STASH="$WORK/share-stash"
  mv "$SHARE" "$STASH"
fi
# Invoked from the EXIT trap below, which shellcheck cannot see.
# shellcheck disable=SC2329
restore_share() {
  rm -rf "$SHARE"
  if [ -n "$STASH" ] && [ -d "$STASH" ]; then
    mkdir -p "$(dirname "$SHARE")"
    mv "$STASH" "$SHARE"
  fi
}
trap 'restore_share; rm -rf "$WORK"' EXIT

# count_at <glob-dir> — regular files under a directory, 0 if absent.
count_at() { [ -d "$1" ] && find "$1" -type f | wc -l | tr -d ' ' || echo 0; }

# ── 1. resources, host layout ────────────────────────────────────────────────
echo "=== G-XTASK 1/6 :: resources (host) ==="
rm -rf "$SHARE"
if ! "${XTASK[@]}" resources > "$WORK/res.log" 2>&1; then
  tail -20 "$WORK/res.log" | sed 's/^/      /'
  bad "resources (host) — command failed"
else
  # Per-family floors. The .mo floor is derived from po/*.po rather than
  # hardcoded so adding a translation cannot silently skip compilation.
  npo="$(find "$ROOT/po" -name '*.po' | wc -l | tr -d ' ')"
  nmo="$(find "$SHARE/locale" -name '*.mo' 2>/dev/null | wc -l | tr -d ' ')"
  nman="$(count_at "$SHARE/man")"
  nthemes="$(count_at "$SHARE/rustty/themes")"
  nshell="$(count_at "$SHARE/rustty/shell-integration")"
  nterm="$(count_at "$SHARE/terminfo")"
  errs=0
  [ "$nmo" -ge "$npo" ]   || { echo "      locale: $nmo .mo files for $npo catalogues"; errs=1; }
  [ "$nman" -ge 2 ]       || { echo "      man: $nman page(s), expected >= 2 (rustty.1, rustty.5)"; errs=1; }
  [ "$nthemes" -ge 100 ]  || { echo "      themes: $nthemes, expected >= 100"; errs=1; }
  [ "$nshell" -ge 4 ]     || { echo "      shell-integration: $nshell file(s), expected >= 4"; errs=1; }
  [ "$nterm" -ge 3 ]      || { echo "      terminfo: $nterm file(s), expected >= 3 (source, termcap, compiled db)"; errs=1; }
  # `-Demit-webdata` defaults to false (Config.zig:531-535): no website data
  # unless asked for. Arm 2 asks for it, so this pair is differential.
  [ ! -e "$SHARE/rustty/webdata" ] \
    || { echo "      webdata emitted without --emit-webdata (the reference default is off)"; errs=1; }
  # R2: nothing may be emitted under the reference's identity.
  if find "$SHARE" -name 'ghostty*' -print -quit 2>/dev/null | grep -q .; then
    echo "      R2: ghostty-named output under $SHARE:"
    find "$SHARE" -name 'ghostty*' | head -3 | sed 's/^/        /'
    errs=1
  fi
  if [ "$errs" -eq 0 ]; then
    ok "resources (host) — $nman man, $nmo/$npo locale, $nthemes themes, $nterm terminfo, $nshell shell-integration"
  else
    bad "resources (host) — output tree incomplete"
  fi
fi

# ── 2. resources, linux layout ───────────────────────────────────────────────
echo "=== G-XTASK 2/6 :: resources --target-os linux ==="
rm -rf "$SHARE"
if ! "${XTASK[@]}" resources --target-os linux --prefix /usr --optimize ReleaseFast --emit-webdata true > "$WORK/res-linux.log" 2>&1; then
  tail -20 "$WORK/res-linux.log" | sed 's/^/      /'
  bad "resources (linux) — command failed"
else
  errs=0
  for f in \
    "applications/io.airely.rustty.desktop" \
    "dbus-1/services/io.airely.rustty.service" \
    "metainfo/io.airely.rustty.metainfo.xml"
  do
    [ -f "$SHARE/$f" ] || { echo "      missing share/$f"; errs=1; }
  done
  nmdx="$(find "$SHARE/rustty/webdata" -name '*.mdx' 2>/dev/null | wc -l | tr -d ' ')"
  [ "$nmdx" -eq 3 ] || { echo "      --emit-webdata true: $nmdx .mdx file(s), expected 3"; errs=1; }
  nicons="$(find "$SHARE/icons" -name '*.png' 2>/dev/null | wc -l | tr -d ' ')"
  [ "$nicons" -ge 6 ] || { echo "      icons: $nicons png(s), expected >= 6 hicolor sizes"; errs=1; }
  # The Exec= line must carry the prefix that was asked for.
  if [ -f "$SHARE/applications/io.airely.rustty.desktop" ] \
     && ! grep -q "^Exec=/usr/bin/rustty" "$SHARE/applications/io.airely.rustty.desktop"; then
    echo "      desktop Exec= does not carry the --prefix that was passed:"
    grep '^Exec=' "$SHARE/applications/io.airely.rustty.desktop" | sed 's/^/        /'
    errs=1
  fi
  # These two are the POSITIVE half of the flatpak differential in arm 3: the
  # plain arm MUST have the systemd wiring and a >512 icon, so arm 3's
  # "absent" assertions cannot pass vacuously.
  if ! grep -q '^SystemdService=' "$SHARE/dbus-1/services/io.airely.rustty.service" 2>/dev/null; then
    echo "      the plain D-Bus service is missing SystemdService="; errs=1
  fi
  [ -f "$SHARE/systemd/user/app-io.airely.rustty.service" ] \
    || { echo "      the plain arm did not install the systemd user unit"; errs=1; }
  nbig="$(find "$SHARE/icons" \( -path '*1024*' -o -path '*2048*' \) -name '*.png' 2>/dev/null | wc -l | tr -d ' ')"
  [ "$nbig" -ge 1 ] || { echo "      the plain arm emitted no icon above 512 to cap"; errs=1; }
  if [ "$errs" -eq 0 ]; then
    ok "resources (linux) — desktop + dbus(+systemd) + metainfo + $nicons icons ($nbig above 512)"
  else
    bad "resources (linux) — desktop-app resources incomplete"
  fi
fi

# ── 2b. resources, linux layout, DEFAULT optimize (N371 end to end) ─────────
# `xtask resources` defaults to `--optimize Debug`, like `zig build`, and the
# reference derives the Linux app id from the optimize mode
# (GhosttyResources.zig:268-280). So a plain run with no `--optimize` and no
# `--debug` must install the `-debug` desktop/metainfo/D-Bus names and NOT the
# release ones; arm 2 (ReleaseFast) is the positive half. This drives the
# real argument parsing and generation, not just `linux_app_is_debug`.
echo "=== G-XTASK 2b/6 :: resources --target-os linux (default optimize -> -debug id) ==="
rm -rf "$SHARE"
if ! "${XTASK[@]}" resources --target-os linux --prefix /usr > "$WORK/res-linux-dbg.log" 2>&1; then
  tail -20 "$WORK/res-linux-dbg.log" | sed 's/^/      /'
  bad "resources (linux, default optimize) — command failed"
else
  errs=0
  for f in \
    "applications/io.airely.rustty-debug.desktop" \
    "metainfo/io.airely.rustty-debug.metainfo.xml" \
    "dbus-1/services/io.airely.rustty-debug.service"
  do
    [ -f "$SHARE/$f" ] || { echo "      missing share/$f"; errs=1; }
  done
  [ ! -e "$SHARE/applications/io.airely.rustty.desktop" ] \
    || { echo "      a Debug run installed the RELEASE desktop id"; errs=1; }
  if [ "$errs" -eq 0 ]; then
    ok "resources (linux, default optimize) — the -debug app id, as the reference derives it"
  else
    bad "resources (linux, default optimize) — the app id did not follow the optimize mode"
  fi
fi

# ── 3. resources, flatpak arm ────────────────────────────────────────────────
echo "=== G-XTASK 3/6 :: resources --target-os linux --flatpak ==="
rm -rf "$SHARE"
if ! "${XTASK[@]}" resources --target-os linux --prefix /app --flatpak --optimize ReleaseFast > "$WORK/res-fp.log" 2>&1; then
  tail -20 "$WORK/res-fp.log" | sed 's/^/      /'
  bad "resources (flatpak) — command failed"
else
  errs=0
  svc="$SHARE/dbus-1/services/io.airely.rustty.service"
  [ -f "$svc" ] || { echo "      missing the D-Bus service file"; errs=1; }
  # What --flatpak actually changes (dist/linux/dbus.service.flatpak.in vs
  # dbus.service.in): the flatpak template DROPS the `SystemdService=` line,
  # because a flatpak has no systemd user unit to activate. Arm 2 asserts the
  # plain template HAS it, so this pair is differential — neither arm can pass
  # by generating nothing.
  if [ -f "$svc" ] && grep -q '^SystemdService=' "$svc"; then
    echo "      the --flatpak D-Bus template was not swapped in (SystemdService= still present):"
    sed 's/^/        /' "$svc" | head -5
    errs=1
  fi
  # ...and no systemd user unit is installed at all.
  if [ -e "$SHARE/systemd" ]; then
    echo "      --flatpak installed a systemd unit tree"; errs=1
  fi
  # Icons capped at 512 — no 1024/2048 under flatpak (arm 2 asserts the plain
  # arm emits at least one, so there is genuinely something to cap).
  big="$(find "$SHARE/icons" \( -path '*1024*' -o -path '*2048*' \) -name '*.png' 2>/dev/null | wc -l | tr -d ' ')"
  [ "$big" -eq 0 ] || { echo "      $big icon(s) above the 512 flatpak cap"; errs=1; }
  if [ "$errs" -eq 0 ]; then
    ok "resources (flatpak) — D-Bus template swapped, no systemd unit, icons capped at 512"
  else
    bad "resources (flatpak) — the --flatpak arm did not take effect"
  fi
fi
rm -rf "$SHARE"

# ── 4. gen, every action ─────────────────────────────────────────────────────
echo "=== G-XTASK 4/6 :: gen <+action> ==="
GEN_ACTIONS=(bash fish zsh sublime vim-syntax vim-ftdetect vim-ftplugin vim-compiler terminfo)
gen_bad=0
for a in "${GEN_ACTIONS[@]}"; do
  if ! "${XTASK[@]}" gen "+$a" > "$WORK/gen-$a.out" 2>"$WORK/gen-$a.err"; then
    tail -5 "$WORK/gen-$a.err" | sed 's/^/      /'
    echo "      FAIL gen +$a"
    gen_bad=1
    continue
  fi
  sz="$(wc -c < "$WORK/gen-$a.out" | tr -d ' ')"
  # Anti-vacuity: exit 0 with empty stdout is the failure mode that matters.
  [ "$sz" -gt 32 ] || { echo "      gen +$a emitted $sz bytes"; gen_bad=1; }
done
if [ "$gen_bad" -eq 0 ]; then
  ok "gen — all ${#GEN_ACTIONS[@]} actions emitted non-empty output"
else
  bad "gen — at least one action failed or emitted nothing"
fi

# ── 5. wasm-patch-growable-table, end to end ─────────────────────────────────
echo "=== G-XTASK 5/6 :: wasm-patch-growable-table ==="
# A minimal module: magic + version, then section id 4 (table) carrying one
# funcref table (0x70) with flags=0x01 (has max), min=1, max=1.
printf '\x00\x61\x73\x6d\x01\x00\x00\x00\x04\x05\x01\x70\x01\x01\x01' > "$WORK/in.wasm"
if ! "${XTASK[@]}" wasm-patch-growable-table "$WORK/in.wasm" "$WORK/out.wasm" > "$WORK/wasm.log" 2>&1; then
  tail -10 "$WORK/wasm.log" | sed 's/^/      /'
  bad "wasm-patch-growable-table — command failed"
else
  got="$(od -An -tx1 -v "$WORK/out.wasm" | tr -d ' \n')"
  # flags 0x01 -> 0x00 and the max byte dropped, so the section payload shrinks
  # from 5 bytes to 4: 04 04 01 70 00 01.
  want="0061736d010000000404017000 01"
  want="${want// /}"
  if [ "$got" = "$want" ]; then
    ok "wasm-patch-growable-table — limits flag 0x01 -> 0x00, max dropped"
  else
    echo "      got:  $got"
    echo "      want: $want"
    bad "wasm-patch-growable-table — unexpected output bytes"
  fi
fi

# ── 6. xcframework (the app bundle), opt-in ──────────────────────────────────
echo "=== G-XTASK 6/6 :: xcframework (app bundle) ==="
if [ "$(uname -s)" != "Darwin" ]; then
  echo "  skipped xcframework (macOS only)"
elif [ "${RUSTTY_XTASK_XCFRAMEWORK:-0}" != "1" ]; then
  echo "  skipped xcframework (set RUSTTY_XTASK_XCFRAMEWORK=1; multi-arch build of rustty-apprt-c)"
else
  if ! "${XTASK[@]}" xcframework > "$WORK/xcf.log" 2>&1; then
    tail -20 "$WORK/xcf.log" | sed 's/^/      /'
    bad "xcframework — command failed"
  else
    # `macos/`, not `dist/`: that is where Rustty.xcodeproj links it from and
    # where the reference emits it (GhosttyXCFramework.zig:41).
    bundle="$ROOT/macos/RusttyKit.xcframework"
    if [ -f "$bundle/Info.plist" ]; then
      ok "xcframework — $bundle built"
    else
      bad "xcframework — exited 0 but $bundle/Info.plist is absent"
    fi
  fi
fi

echo "--- G-XTASK summary: $RAN check(s) ran ---"
if [ "$RAN" -eq 0 ]; then
  echo "G-XTASK BLOCKED: nothing ran."
  exit 3
fi
if [ "$FAILED" -ne 0 ]; then
  echo "G-XTASK RED: see above." >&2
  exit 1
fi
echo "G-XTASK GREEN ($RAN check(s) clean)."
exit 0
