#!/usr/bin/env bash
# hw_gl_probe.sh — WS17 V2: is this box a GENUINELY accelerated GL >= 4.3-core host?
#
# `.port/phase-2/VERIFICATION_DEBT.md` V2 (bare-metal GPU rendering/perf sign-off) is
# blocked on hardware, not on code. This script is the turnkey check so that whoever
# eventually has a GPU box can answer it in one command instead of re-deriving the
# criteria. It exits 0 ONLY when all three hold:
#
#   1. `glxinfo -B` reports `Accelerated: yes`;
#   2. the renderer is not a known software rasterizer (llvmpipe / softpipe / swrast /
#      "Software Rasterizer");
#   3. `Max core profile version` >= rustty's floor of **4.3**
#      (`crates/rustty-renderer/src/opengl/gl/sys.rs` MIN_VERSION_MAJOR/MINOR).
#
# WHY 4.3-CORE IS THE LINE, and why virgl did not clear it: when this host DID run virgl
# (2026-07-17, recorded in `crates/rustty-renderer/src/opengl/gl/sys.rs`), it reported
# `GL_RENDERER = virgl (ANGLE (Apple, Apple M4 Pro, OpenGL 4.1 Metal))`,
# `GL_VERSION = 2.1`, and **max core profile version 0.0** — every core-profile request
# from 3.2 to 4.6 failed `EGL_BAD_MATCH`. A probe that only checked "is there a GPU name"
# would have called that a pass, which is exactly the false-pass this script exists to
# prevent.
#
# NON-VACUITY (the part that makes this trustworthy). A gate whose PASS arm has never
# been observed is indistinguishable from a gate that always fails. So the script can be
# pointed at a captured `glxinfo -B` instead of running one:
#
#   RUSTTY_HW_GL_PROBE_FIXTURE=<file> .port/tools/gates/hw_gl_probe.sh
#
# and two fixtures ship beside it:
#   fixtures/glxinfo-hw-radeonsi.txt  -> synthetic ACCELERATED output, must PASS (exit 0)
#   fixtures/glxinfo-llvmpipe.txt     -> software output, must FAIL (exit 1)
#
# `--self-test` runs both and verifies each arm behaves as declared, so the PASS arm is
# proven reachable on a software-only box. The radeonsi fixture is SYNTHETIC and labelled
# as such: it proves the script's logic, never that any hardware was tested.
#
# EXIT CODES
#   0 = accelerated GL >= 4.3 core (V2 can be executed on this box)
#   1 = software / insufficient GL (V2 cannot be executed here)
#   2 = cannot tell (glxinfo missing, no display, unparseable output) — NEVER a pass
set -uo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
FIXTURE_DIR="$HERE/fixtures"
FLOOR_MAJOR=4
FLOOR_MINOR=3

# --- self-test ----------------------------------------------------------------
if [ "${1:-}" = "--self-test" ]; then
  echo "== hw_gl_probe.sh self-test (proves BOTH arms are reachable) =="
  rc_all=0

  for spec in "glxinfo-hw-radeonsi.txt:0:PASS" "glxinfo-llvmpipe.txt:1:FAIL"; do
    f="${spec%%:*}"; rest="${spec#*:}"; want="${rest%%:*}"; label="${rest#*:}"
    path="$FIXTURE_DIR/$f"
    if [ ! -f "$path" ]; then
      echo "  RED: missing fixture $path — the self-test cannot bind"
      rc_all=1; continue
    fi
    RUSTTY_HW_GL_PROBE_FIXTURE="$path" "$0" > /tmp/hwglprobe-self.$$ 2>&1
    got=$?
    if [ "$got" -eq "$want" ]; then
      echo "  OK   $f -> exit $got (expected $label)"
    else
      echo "  RED  $f -> exit $got (expected $want / $label)"
      sed 's/^/         /' /tmp/hwglprobe-self.$$
      rc_all=1
    fi
    rm -f /tmp/hwglprobe-self.$$
  done

  if [ "$rc_all" -eq 0 ]; then
    echo "SELF-TEST GREEN: the PASS arm is reachable and the FAIL arm fires."
    echo "NOTE: the passing fixture is SYNTHETIC. It proves this script's logic, NOT that"
    echo "      any accelerated hardware was tested. V2 stays open until a real box exits 0."
  else
    echo "SELF-TEST RED: this probe cannot be trusted until both arms behave."
  fi
  exit "$rc_all"
fi

# --- gather -------------------------------------------------------------------
if [ -n "${RUSTTY_HW_GL_PROBE_FIXTURE:-}" ]; then
  [ -f "$RUSTTY_HW_GL_PROBE_FIXTURE" ] || { echo "INCONCLUSIVE: no such fixture: $RUSTTY_HW_GL_PROBE_FIXTURE"; exit 2; }
  out="$(cat "$RUSTTY_HW_GL_PROBE_FIXTURE")"
  source_desc="fixture $RUSTTY_HW_GL_PROBE_FIXTURE"
else
  command -v glxinfo >/dev/null 2>&1 || { echo "INCONCLUSIVE: glxinfo not installed (Fedora: glx-utils)"; exit 2; }
  [ -n "${DISPLAY:-}${WAYLAND_DISPLAY:-}" ] || { echo "INCONCLUSIVE: no DISPLAY/WAYLAND_DISPLAY — glxinfo has nothing to query"; exit 2; }
  out="$(glxinfo -B 2>&1)"
  [ -n "$out" ] || { echo "INCONCLUSIVE: glxinfo -B produced no output"; exit 2; }
  source_desc="live glxinfo -B"
fi

# Field extraction. Two rules, both learned from `--self-test` catching this script
# getting them wrong:
#   * glxinfo INDENTS the GLX_MESA_query_renderer fields, so anchor with `^[[:space:]]*`
#     and not with `^` alone (an unanchored `.*Accelerated:` is worse still — it matched
#     the word inside the fixture's own explanatory header);
#   * capture only the value shape expected (a word / a dotted number) and stop, so a
#     line of prose that happens to mention a field name cannot be read as its value.
renderer="$(printf '%s\n' "$out" | sed -n 's/^OpenGL renderer string:[[:space:]]*//p' | head -1)"
accel="$(printf '%s\n' "$out" | sed -n 's/^[[:space:]]*Accelerated:[[:space:]]*\([A-Za-z]*\).*/\1/p' | head -1)"
core="$(printf '%s\n' "$out" | sed -n 's/^[[:space:]]*Max core profile version:[[:space:]]*\([0-9][0-9.]*\).*/\1/p' | head -1)"
device="$(printf '%s\n' "$out" | sed -n 's/^[[:space:]]*Device:[[:space:]]*//p' | head -1)"

echo "source            : $source_desc"
echo "OpenGL renderer   : ${renderer:-<unparsed>}"
echo "Device            : ${device:-<unparsed>}"
echo "Accelerated       : ${accel:-<unparsed>}"
echo "Max core profile  : ${core:-<unparsed>} (rustty floor ${FLOOR_MAJOR}.${FLOOR_MINOR})"
echo

# Unparseable is INCONCLUSIVE, never a pass: a glxinfo whose format drifted must not be
# read as "accelerated".
if [ -z "$renderer" ] || [ -z "$accel" ] || [ -z "$core" ]; then
  echo "INCONCLUSIVE: could not parse renderer / Accelerated / Max core profile version."
  echo "Do NOT treat this as a pass — fix the parse."
  exit 2
fi

fails=0

case "$accel" in
  yes) echo "  ok   Accelerated: yes" ;;
  *)   echo "  FAIL Accelerated: $accel — this is not a hardware-accelerated stack"; fails=1 ;;
esac

case "$(printf '%s' "$renderer" | tr '[:upper:]' '[:lower:]')" in
  *llvmpipe*|*softpipe*|*swrast*|*"software rasterizer"*|*"kms swrast"*)
    echo "  FAIL renderer is a software rasterizer: $renderer"; fails=1 ;;
  *)
    echo "  ok   renderer is not a known software rasterizer" ;;
esac

# `Max core profile version` is `MAJOR.MINOR`. `0.0` is what the virgl measurement
# produced, and it must fail.
cmaj="${core%%.*}"; cmin="${core#*.}"; cmin="${cmin%%[^0-9]*}"
case "$cmaj$cmin" in
  ''|*[!0-9]*) echo "  FAIL Max core profile version is not numeric: $core"; fails=1 ;;
  *)
    if [ "$cmaj" -gt "$FLOOR_MAJOR" ] || { [ "$cmaj" -eq "$FLOOR_MAJOR" ] && [ "$cmin" -ge "$FLOOR_MINOR" ]; }; then
      echo "  ok   core profile $core >= ${FLOOR_MAJOR}.${FLOOR_MINOR}"
    else
      echo "  FAIL core profile $core < ${FLOOR_MAJOR}.${FLOOR_MINOR} — rustty's renderer cannot create its context"
      fails=1
    fi ;;
esac

echo
if [ "$fails" -eq 0 ]; then
  echo "HW GL PROBE: PASS — accelerated GL >= ${FLOOR_MAJOR}.${FLOOR_MINOR} core."
  echo "V2 CAN be executed on this box. Next: run the renderer KATs and the perf suite here,"
  echo "then discharge VERIFICATION_DEBT.md V2 with the captured output."
  exit 0
fi
echo "HW GL PROBE: FAIL — this box cannot host the V2 bare-metal GPU sign-off."
exit 1
