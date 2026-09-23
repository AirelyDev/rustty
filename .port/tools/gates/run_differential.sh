#!/usr/bin/env bash
# run_differential.sh — GATE G4 (differential VT clean) — thin runner.
#
# DoP id:  G4  (DEFINITION_OF_PARITY §1, "Differential VT clean")
# Drives:  .port/phase-0/harness/  (the differential VT comparator + drivers)
#          .port/phase-0/corpora/cvt/  (the c-vt-* golden corpus)
# Pass:    zero byte diffs across BOTH legs:
#            leg A — every harness fixture (`harness/fixtures/*.vt`) fed through
#                    vt_driver_ref vs vt_driver_port via comparator.py
#                    (identical input + grid/format options; canonical
#                    Screen/PageList serialization byte-diffed), and
#            leg B — every c-vt golden case (`corpora/cvt/*.golden`): the
#                    PORT-built example binary's combined stdout+stderr must
#                    byte-match the stored reference golden (and its exit code
#                    must match the stored .exit).
#
# ARMING: the gate arms itself the moment the port driver exists —
#   PORT_DRIVER="${RUSTTY_VT_DRIVER:-$HARNESS/bin/vt_driver_port}"
# (build it with `.port/phase-0/harness/build_port_driver.sh`). While it does
# NOT exist, the gate runs in progress mode: it executes the harness self-test
# (gate_empty_divergence.sh) which PROVES the harness correctly detects total
# divergence against the empty port, and exits 0 (harness wired, port empty) —
# it does NOT fabricate a parity green. Once armed, a present-but-diverging
# port is RED (exit 1), NEVER a warning, and gate_empty_divergence.sh (whose
# assert-100%-divergence contract is only meaningful against an EMPTY port) is
# not run at all.
#
# DIAGNOSTICS: every failing case writes a Bun-style
#   $HARNESS/results/diag/<case>.diag
# recording the failure class (crash / hang / diverge / unbuilt), exit codes,
# and the first diverging byte offset with hex context — the fixer's runtime
# evidence.
#
# Usage:
#   run_differential.sh                 # auto: real gate if port driver exists, else progress
#   run_differential.sh --progress      # force progress (harness self-test) mode
#   run_differential.sh --self-test     # self-test: wiring + RED-flip injection proof
#   run_differential.sh --self-test-oracle  # hermetic: the staleness guard BLOCKS on
#                                       # an absent or off-pin oracle, and GIT_* cannot
#                                       # move its HEAD check (run by G-ORACLE-SH-SELF)
#   RUSTTY_VT_DRIVER=/path/to/bin run_differential.sh   # point at the port driver
#
# Exit 0 = GREEN (real) or HARNESS-WIRED (progress); non-zero = RED / BLOCKED.
set -euo pipefail

GATES_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PORT_DIR="$(cd "$GATES_DIR/../.." && pwd)"
REPO_ROOT="$(cd "$PORT_DIR/.." && pwd)"

# Port<->oracle name map (rebrand Task 0, .port/tools/PARITY_NAMING.md). The
# port driver var below stays RUSTTY_VT_DRIVER (already the port name); this
# source line is here so future edits to this runner never have to hardcode
# either side's name again.
# shellcheck source=/dev/null
source "$REPO_ROOT/.port/tools/parity_names.env"

HARNESS="$PORT_DIR/phase-0/harness"
# .port/phase-0/corpora/cvt/*.golden — the ORACLE golden corpus (captured
# ghostty output, carve-out: stays ghostty, never renamed by the rebrand).
CORPUS="$PORT_DIR/phase-0/corpora/cvt"
PY="${PYTHON:-python3}"

MODE="auto"
case "${1:-}" in
  --progress)  MODE="progress" ;;
  --self-test) MODE="self-test" ;;
  --print-oracle) MODE="print-oracle" ;;
  --self-test-oracle) MODE="self-test-oracle" ;;
  "" )         MODE="auto" ;;
  *) echo "run_differential.sh: unknown arg '${1:-}'" >&2; exit 2 ;;
esac

# The read-only oracle checkout (its zig-out/include feeds the driver-source
# staleness guard): `ghostty` beside the MAIN checkout, resolved by the shared
# shell resolver (the twin of `_gatelib.oracle_root`). R5: no environment
# override — the former `GHOSTTY` could point the guard at any tree — and
# `GIT_*` cannot redirect it. `--print-oracle` prints the result and exits;
# G-ORACLE-SH-SELF (`_oracle_root.sh --self-test`) runs a copy of this gate that
# way with the old override and a GIT_DIR decoy set.
# shellcheck source=.port/tools/gates/_oracle_root.sh
source "$REPO_ROOT/.port/tools/gates/_oracle_root.sh" || { echo "BLOCKED: cannot source _oracle_root.sh"; exit 3; }
GHOSTTY="$(rustty_oracle_root "$REPO_ROOT")"
if [ "$MODE" = "print-oracle" ]; then
  echo "$GHOSTTY"
  exit 0
fi
# The one checked-in pin; the staleness guard requires the oracle's HEAD to be
# exactly this commit. `rustty_oracle_pin` returns 1 on a missing or malformed
# file, which `set -e` would turn into a silent exit 1; the `|| ORACLE_PIN=""`
# keeps the run going so the guard reports it as BLOCKED (exit 3) with a reason.
# `--self-test-oracle` arms d-h run this exact line in a copy of the gate.
ORACLE_PIN="$(rustty_oracle_pin)" || ORACLE_PIN=""

echo "=== G4 :: differential VT clean (runner) ==="

# --- BLOCKED checks: harness + corpus must exist ---
blocked=0
if [ ! -d "$HARNESS" ]; then echo "BLOCKED: harness dir missing ($HARNESS)"; blocked=1; fi
if [ ! -d "$CORPUS" ]; then echo "BLOCKED: cvt corpus dir missing ($CORPUS)"; blocked=1; fi
if [ ! -f "$HARNESS/comparator.py" ]; then echo "BLOCKED: comparator.py missing"; blocked=1; fi
if [ "$blocked" = "1" ]; then exit 3; fi

REF_DRIVER="$HARNESS/bin/vt_driver_ref"
PORT_DRIVER="${RUSTTY_VT_DRIVER:-$HARNESS/bin/vt_driver_port}"
DIAG_DIR="$HARNESS/results/diag"

# ---------------------------------------------------------------------------
# Driver-source staleness guard (F-016): the shared vt_driver.c must still
# COMPILE against the pinned oracle headers. A pin advance that changes the C
# ABI out from under the driver source must be RED here, never a silent skip
# and never stale binaries voting green (same lesson as the dc054bb G6 fix:
# a broken probe is a silent skip). It needs the oracle AT THE PIN (HEAD ==
# `ORACLE_PIN`) with its built `zig-out/include`, plus a real C compiler. When
# any of those is missing the guard cannot check, and a guard that cannot
# check is BLOCKED (exit 3 from `enforce_driver_source_fresh`), never skipped.
#   $1 = path to the driver source to check (parameterized for the self-test)
# Returns 0 fresh / 1 stale / 2 probe-unavailable (the caller BLOCKS on 2).
check_driver_source_fresh() {
  local src="${1:-$HARNESS/ref_driver/vt_driver.c}"
  local inc="$GHOSTTY/zig-out/include" why
  # `zig-out/include` is build output, so HEAD == pin does not prove the
  # headers were built from the pin; it does rule out an oracle on another
  # commit, which is the case a pin advance produces.
  why="$(rustty_oracle_pin_status "$GHOSTTY" "$ORACLE_PIN")" \
    || { echo "    staleness guard: $why"; return 2; }
  [ -d "$inc" ] || { echo "    staleness guard: oracle include dir missing ($inc)"; return 2; }
  # `cc` is aliased in this environment; resolve a real clang explicitly.
  local cc="" sysroot_flag=()
  if command -v xcrun >/dev/null 2>&1; then
    cc="$(xcrun --find clang 2>/dev/null || true)"
    local sysroot
    sysroot="$(xcrun --show-sdk-path 2>/dev/null || true)"
    [ -n "$sysroot" ] && sysroot_flag=(-isysroot "$sysroot")
  fi
  [ -n "$cc" ] || { [ -x /usr/bin/clang ] && cc=/usr/bin/clang; }
  [ -n "$cc" ] || { echo "    staleness guard: no real C compiler found"; return 2; }
  if "$cc" -std=c11 -fsyntax-only "${sysroot_flag[@]}" -I"$inc" "$src" 2>"$DIAG_DIR/.staleness.err"; then
    rm -f "$DIAG_DIR/.staleness.err"
    return 0
  fi
  return 1
}

# Enforce the guard for a gate run (auto/progress; self-test proves the flip
# itself). Stale source is BLOCKED exit 3: the binaries on disk can no longer
# be rebuilt from source, so any green they produce is unre-derivable.
enforce_driver_source_fresh() {
  mkdir -p "$DIAG_DIR"
  local rc=0 guard_out
  guard_out="$(check_driver_source_fresh)" || rc=$?
  [ -z "$guard_out" ] || printf '%s\n' "$guard_out"
  case "$rc" in
    0) echo "--- staleness guard: vt_driver.c compiles against the pinned oracle headers: OK ---" ;;
    1)
      echo "BLOCKED: vt_driver.c no longer compiles against the pinned oracle headers"
      echo "         ($GHOSTTY/zig-out/include) — the drivers cannot be rebuilt from"
      echo "         source; fix the driver for the new ABI before trusting this gate."
      sed 's/^/    | /' "$DIAG_DIR/.staleness.err" | head -20
      exit 3
      ;;
    2)
      # R5: the guard could not check, so the gate cannot vouch for the
      # drivers. Before R13-GF3 this printed "skipped" and the run went on to
      # a green HARNESS-WIRED wherever the oracle was absent.
      # The reason goes on the BLOCKED line itself: ci.sh copies that line into
      # its WARNINGS summary.
      echo "BLOCKED: G4 staleness guard: ${guard_out#*staleness guard: }"
      echo "         The guard needs the oracle at the pin (ORACLE_PIN) with a built"
      echo "         zig-out/include and a C compiler. This is NOT a pass."
      exit 3
      ;;
  esac
}

# ---------------------------------------------------------------------------
# Diag helper: write $DIAG_DIR/<case>.diag with the failure class, exit codes,
# and the first diverging byte offset + hex context of ref vs port output.
#   $1=case  $2=class(crash|hang|diverge|unbuilt)  $3=summary
#   $4=ref-output-file (or "")  $5=port-output-file (or "")
# ---------------------------------------------------------------------------
write_diag() {
  local case_name="$1" class="$2" summary="$3" ref_out="${4:-}" port_out="${5:-}"
  mkdir -p "$DIAG_DIR"
  local diag="$DIAG_DIR/$case_name.diag"
  {
    echo "case:    $case_name"
    echo "class:   $class"
    echo "summary: $summary"
    if [ -n "$ref_out" ] && [ -n "$port_out" ] && [ -f "$ref_out" ] && [ -f "$port_out" ]; then
      # First diverging byte (1-based from cmp; also handles length-only diffs).
      local cmp_line off
      cmp_line="$(cmp "$ref_out" "$port_out" 2>&1 || true)"
      echo "cmp:     ${cmp_line:-<identical>}"
      echo "sizes:   ref=$(wc -c < "$ref_out" | tr -d ' ')B port=$(wc -c < "$port_out" | tr -d ' ')B"
      # (BSD sed has no \| alternation; "char"/"byte" both match [a-z]*.)
      off="$(printf '%s' "$cmp_line" | sed -n 's/.*differ: *[a-z]* *\([0-9][0-9]*\).*/\1/p')"
      if [ -z "$off" ]; then
        # EOF-on-one-side (common prefix): first divergence is right past the
        # shorter output's end.
        local la lb
        la="$(wc -c < "$ref_out" | tr -d ' ')"
        lb="$(wc -c < "$port_out" | tr -d ' ')"
        off="$(( (la < lb ? la : lb) + 1 ))"
      fi
      local start=$(( off > 33 ? off - 33 : 0 ))
      echo "--- ref bytes @ ${start} (first divergence at byte ${off}) ---"
      xxd -s "$start" -l 96 "$ref_out" || true
      echo "--- port bytes @ ${start} ---"
      xxd -s "$start" -l 96 "$port_out" || true
    fi
  } > "$diag"
  echo "    diag: $diag"
}

# ---------------------------------------------------------------------------
# Real gate, leg A: comparator over every harness fixture.
# Prints per-case results; returns non-zero if any case fails.
#   $1=port-driver
# ---------------------------------------------------------------------------
run_fixture_leg() {
  local port_driver="$1"
  local fail=0 fx case_name json cls summary ref_json port_json
  echo "--- leg A: comparator over $(ls "$HARNESS"/fixtures/*.vt | wc -l | tr -d ' ') fixtures (ref vs port, byte-exact) ---"
  for fx in "$HARNESS"/fixtures/*.vt; do
    case_name="fixture-$(basename "${fx%.vt}")"
    json="$DIAG_DIR/.${case_name}.json"
    mkdir -p "$DIAG_DIR"
    if "$PY" "$HARNESS/comparator.py" "$fx" \
         --ref "$REF_DRIVER" --port "$port_driver" \
         --json --show-diff >"$json" 2>"$DIAG_DIR/.${case_name}.stderr"; then
      printf '    OK   %s\n' "$case_name"
      rm -f "$DIAG_DIR/$case_name.diag" "$json" "$DIAG_DIR/.${case_name}.stderr"
    else
      fail=1
      # Classify from the comparator's JSON result (never mask its stderr).
      cls="$("$PY" -c '
import json,sys
r = json.load(open(sys.argv[1]))
p = r["port"]
if p["timed_out"]: print("hang")
elif p["exit_code"] is None or p["exit_code"] != r["reference"]["exit_code"]: print("crash")
else: print("diverge")
' "$json" 2>/dev/null || echo diverge)"
      printf '    %-7s %s (byte_divergence=%s%%)\n' "$(printf '%s' "$cls" | tr '[:lower:]' '[:upper:]')" "$case_name" \
        "$("$PY" -c 'import json,sys; print(json.load(open(sys.argv[1]))["byte_divergence_pct"])' "$json" 2>/dev/null || echo '?')"
      sed 's/^/      | /' "$DIAG_DIR/.${case_name}.stderr" | head -40
      # Re-run both drivers on the same input/options to capture the raw
      # outputs for the byte-context diag (the comparator does not persist them).
      ref_json="$DIAG_DIR/.${case_name}.ref.out"; port_json="$DIAG_DIR/.${case_name}.port.out"
      "$REF_DRIVER"  --cols 80 --rows 24 --scrollback 0 --format plain --trim "$fx" >"$ref_json" 2>/dev/null || true
      "$port_driver" --cols 80 --rows 24 --scrollback 0 --format plain --trim "$fx" >"$port_json" 2>/dev/null || true
      summary="comparator non-identical on $(basename "$fx") ($cls)"
      write_diag "$case_name" "$cls" "$summary" "$ref_json" "$port_json"
    fi
  done
  return $fail
}

# ---------------------------------------------------------------------------
# Real gate, leg B: port-built c-vt example binaries vs the stored goldens.
# The golden is the reference build's combined stdout+stderr; the port binary
# (built by build_port_driver.sh into $HARNESS/bin/cvt/<case>) must byte-match
# it, and its exit code must match the stored .exit. A missing port binary is
# RED (unbuilt), never a skip.
# ---------------------------------------------------------------------------
run_cvt_golden_leg() {
  local fail=0 golden case_name bin out ec want_ec cls
  echo "--- leg B: $(ls "$CORPUS"/*.golden | wc -l | tr -d ' ') c-vt golden cases (port example binary vs stored golden, byte-exact) ---"
  for golden in "$CORPUS"/*.golden; do
    case_name="$(basename "${golden%.golden}")"
    bin="$HARNESS/bin/cvt/$case_name"
    mkdir -p "$DIAG_DIR"
    if [ ! -x "$bin" ]; then
      fail=1
      printf '    UNBUILT %s (no port example binary: %s)\n' "$case_name" "$bin"
      write_diag "$case_name" "unbuilt" "port example binary missing/unlinkable; see build_port_driver.sh output ($bin)" "" ""
      continue
    fi
    out="$DIAG_DIR/.${case_name}.port.out"
    # Run with a hang guard; capture combined stdout+stderr in arrival order
    # (exactly how the golden was captured by run-cvt-corpora.sh).
    set +e
    "$PY" -c '
import subprocess, sys
try:
    p = subprocess.run([sys.argv[1]], stdout=open(sys.argv[2], "wb"),
                       stderr=subprocess.STDOUT, timeout=30)
    sys.exit(p.returncode)
except subprocess.TimeoutExpired:
    sys.exit(124)
' "$bin" "$out"
    ec=$?
    set -e
    want_ec="$(cat "$CORPUS/$case_name.exit" 2>/dev/null || echo 0)"
    if [ "$ec" = "124" ]; then
      fail=1
      printf '    HANG    %s (timeout after 30s)\n' "$case_name"
      write_diag "$case_name" "hang" "port example binary timed out after 30s (ref exit=$want_ec)" "$golden" "$out"
    elif ! cmp -s "$out" "$golden" || [ "$ec" != "$want_ec" ]; then
      fail=1
      if [ "$ec" != "$want_ec" ] && [ "$ec" -ge 128 ]; then cls="crash"; else cls="diverge"; fi
      printf '    %-7s %s (exit %s vs ref %s; %s)\n' "$(printf '%s' "$cls" | tr '[:lower:]' '[:upper:]')" "$case_name" "$ec" "$want_ec" "$(cmp "$golden" "$out" 2>&1 | head -1 || true)"
      write_diag "$case_name" "$cls" "combined output differs from golden (port exit=$ec, ref exit=$want_ec)" "$golden" "$out"
    else
      printf '    OK      %s\n' "$case_name"
      rm -f "$DIAG_DIR/$case_name.diag" "$out"
    fi
  done
  return $fail
}

# ---------------------------------------------------------------------------
# Self-test: wiring + the R5 RED-flip injection proof. Independent of the
# port's parity state: uses the REFERENCE driver as a byte-exact stand-in
# port, then corrupts one byte of its output and asserts the gate machinery
# flips RED; restores and asserts GREEN.
# ---------------------------------------------------------------------------
run_self_test() {
  echo "--- runner self-test: wiring + flippability ---"
  local ok=1
  # 1. comparator must be present/runnable
  if "$PY" "$HARNESS/comparator.py" --help >/dev/null 2>&1; then
    echo "  comparator.py runnable: OK"
  else
    echo "  comparator.py present (--help non-zero, acceptable)"
  fi
  # 2. corpus has golden files
  local n
  n="$(ls "$CORPUS"/*.golden 2>/dev/null | wc -l | tr -d ' ')"
  echo "  cvt golden cases: $n"
  [ "$n" -gt 0 ] || { echo "  FAIL: no golden cases"; ok=0; }
  # 3. the empty-divergence harness self-test exists (progress-mode flip proof)
  if [ -x "$HARNESS/gate_empty_divergence.sh" ]; then
    echo "  harness flip-proof gate present: OK"
  else
    echo "  WARN: gate_empty_divergence.sh absent (harness self-test missing)"
  fi
  # 4. RED-flip injection (comparator leg): ref-vs-ref must be GREEN; ref vs a
  #    one-byte-corrupted copy of the same output must be RED (exit 1).
  if [ -x "$REF_DRIVER" ]; then
    local fx tmpd
    # shellcheck disable=SC2010  # fixture names are known-safe (no newlines).
    fx="$(ls "$HARNESS"/fixtures/*.vt | grep -v empty.vt | head -1)"
    tmpd="$(mktemp -d)"
    # Stand-in "port": the reference driver itself (a byte-exact port).
    if "$PY" "$HARNESS/comparator.py" "$fx" --ref "$REF_DRIVER" --port "$REF_DRIVER" >/dev/null 2>&1; then
      echo "  injection control (ref vs ref => GREEN): OK"
    else
      echo "  FAIL: comparator not identical on ref vs ref"; ok=0
    fi
    # Corrupting wrapper: run the ref driver, flip one byte of its output.
    cat > "$tmpd/corrupt_port.sh" <<EOF
#!/usr/bin/env bash
"$REF_DRIVER" "\$@" | "$PY" -c '
import sys
data = bytearray(sys.stdin.buffer.read())
if data: data[len(data)//2] ^= 0x01
sys.stdout.buffer.write(bytes(data))'
EOF
    chmod +x "$tmpd/corrupt_port.sh"
    if "$PY" "$HARNESS/comparator.py" "$fx" --ref "$REF_DRIVER" --port "$tmpd/corrupt_port.sh" >/dev/null 2>&1; then
      echo "  FAIL: one-byte corruption NOT detected (gate would not flip RED)"; ok=0
    else
      echo "  injection (1 corrupted byte => RED exit 1): OK"
    fi
    # Restore proof: the uncorrupted stand-in is GREEN again.
    if "$PY" "$HARNESS/comparator.py" "$fx" --ref "$REF_DRIVER" --port "$REF_DRIVER" >/dev/null 2>&1; then
      echo "  injection restore (uncorrupted => GREEN exit 0): OK"
    else
      echo "  FAIL: restore did not go green"; ok=0
    fi
    rm -rf "$tmpd"
  else
    echo "  FAIL: reference driver missing ($REF_DRIVER) — cannot prove the flip"; ok=0
  fi
  # 5. Staleness-guard flip proof (F-016): the CURRENT driver source must pass
  #    the guard; a copy with a resurrected pre-03d5fa268 call (the removed
  #    Options-struct 3-arg ghostty_terminal_new) must flip it to STALE.
  local stale_rc
  stale_rc=0; check_driver_source_fresh >/dev/null || stale_rc=$?
  if [ "$stale_rc" = "0" ]; then
    echo "  staleness guard (current vt_driver.c => fresh): OK"
    local tmps
    tmps="$(mktemp -d)"
    sed 's/ghostty_terminal_new(NULL, &terminal, cols, rows)/ghostty_terminal_new(NULL, \&terminal, (GhosttyTerminalOptions){0})/' \
      "$HARNESS/ref_driver/vt_driver.c" > "$tmps/stale_driver.c"
    stale_rc=0; check_driver_source_fresh "$tmps/stale_driver.c" >/dev/null 2>&1 || stale_rc=$?
    if [ "$stale_rc" = "1" ]; then
      echo "  staleness guard injection (old-ABI source => STALE): OK"
    else
      echo "  FAIL: staleness guard did NOT flag the old-ABI driver source (rc=$stale_rc)"; ok=0
    fi
    rm -rf "$tmps" "$DIAG_DIR/.staleness.err"
  elif [ "$stale_rc" = "2" ]; then
    echo "  staleness guard: probe unavailable here (oracle include/compiler missing); flip proof skipped"
  else
    echo "  FAIL: staleness guard reports the CURRENT vt_driver.c as stale"; ok=0
  fi
  # 6. RED-flip injection (golden leg): a copied golden must cmp-equal itself;
  #    corrupting one byte must be detected; restoring must be clean again.
  local g tmpg
  g="$(ls "$CORPUS"/*.golden 2>/dev/null | head -1)"
  if [ -n "$g" ]; then
    tmpg="$(mktemp)"
    cp "$g" "$tmpg"
    if cmp -s "$g" "$tmpg"; then
      "$PY" -c '
import sys
p = sys.argv[1]
data = bytearray(open(p, "rb").read())
if data: data[len(data)//2] ^= 0x01
open(p, "wb").write(bytes(data))' "$tmpg"
      if cmp -s "$g" "$tmpg"; then
        echo "  FAIL: golden-leg corruption NOT detected"; ok=0
      else
        cp "$g" "$tmpg"
        if cmp -s "$g" "$tmpg"; then
          echo "  injection (golden leg corrupt->RED, restore->GREEN): OK"
        else
          echo "  FAIL: golden-leg restore not clean"; ok=0
        fi
      fi
    else
      echo "  FAIL: golden copy does not cmp-equal itself"; ok=0
    fi
    rm -f "$tmpg"
  else
    echo "  FAIL: no golden available for the golden-leg flip proof"; ok=0
  fi
  # 7. The oracle-handling arms (also run alone by G-ORACLE-SH-SELF).
  run_self_test_oracle || ok=0
  if [ "$ok" = "1" ]; then echo "SELF-TEST: PASS"; return 0; else echo "SELF-TEST: FAIL"; return 1; fi
}

# ---------------------------------------------------------------------------
# Hermetic self-test of the staleness guard's oracle handling (R5). Each arm
# runs the guard in a subshell against a throwaway fixture, with GHOSTTY,
# ORACLE_PIN and DIAG_DIR set INSIDE that subshell — none of them is read from
# the environment by the gate itself.
#   a. oracle absent            -> enforce exits 3 (BLOCKED), not "skipped"
#   b. oracle off the pin       -> enforce exits 3 (BLOCKED)
#   c. oracle at the pin, with a GIT_DIR + GIT_WORK_TREE decoy exported whose
#      HEAD is elsewhere -> the HEAD check still reads the fixture oracle
#   d. ORACLE_PIN NOT set by the arm: the pin this run read from the checked-in
#      file must be the one the guard enforces (off-pin fixture -> BLOCKED)
#   e-k. a copy of the gate run end to end in a fixture checkout, so the
#      production pin-read line itself runs: ORACLE_PIN naming another commit,
#      naming the oracle's HEAD, missing, empty, 39 hex, upper-case (j2: with
#      no F, which a locale-collated `[a-f]` accepted), CRLF ->
#      each BLOCKED exit 3 with its reason (never a silent `set -e` exit 1)
# ---------------------------------------------------------------------------
run_self_test_oracle() {
  echo "--- G4 staleness-guard oracle handling (hermetic) ---"
  local ok=1 tmp rc out pin decoy_head
  tmp="$(mktemp -d "${TMPDIR:-/tmp}/g4_oracle.XXXXXX")" || { echo "  FAIL: mktemp"; return 1; }
  pin="$(rustty_fixture_repo "$tmp/ghostty" README "at the pin")" \
    && decoy_head="$(rustty_fixture_repo "$tmp/decoy" README "decoy")" \
    || { echo "  FAIL: could not build the fixture repositories"; rm -rf "$tmp"; return 1; }
  mkdir -p "$tmp/ghostty/zig-out/include"
  printf 'int g4_selftest;\n' > "$tmp/probe.c"
  mkdir -p "$tmp/diag"

  rc=0
  out="$(GHOSTTY="$tmp/absent" ORACLE_PIN="$pin" DIAG_DIR="$tmp/diag"; enforce_driver_source_fresh 2>&1)" || rc=$?  # R5-selftest-fixture
  if [ "$rc" = "3" ] && printf '%s' "$out" | grep -q "no oracle checkout at $tmp/absent"; then
    echo "  ok   a. absent oracle => BLOCKED exit 3"
  else
    echo "  FAIL a. absent oracle gave rc=$rc, want 3 BLOCKED:"; printf '%s\n' "$out" | sed 's/^/       | /'; ok=0
  fi

  rc=0
  out="$(GHOSTTY="$tmp/ghostty" ORACLE_PIN="$decoy_head" DIAG_DIR="$tmp/diag"; enforce_driver_source_fresh 2>&1)" || rc=$?  # R5-selftest-fixture
  if [ "$rc" = "3" ] && printf '%s' "$out" | grep -q "not the pin $decoy_head"; then
    echo "  ok   b. off-pin oracle => BLOCKED exit 3"
  else
    echo "  FAIL b. off-pin oracle gave rc=$rc, want 3 BLOCKED:"; printf '%s\n' "$out" | sed 's/^/       | /'; ok=0
  fi

  rc=0
  out="$(export GIT_DIR="$tmp/decoy/.git" GIT_WORK_TREE="$tmp/decoy"
         GHOSTTY="$tmp/ghostty" ORACLE_PIN="$pin" DIAG_DIR="$tmp/diag"  # R5-selftest-fixture
         check_driver_source_fresh "$tmp/probe.c" 2>&1)" || rc=$?
  if printf '%s' "$out" | grep -Eq "staleness guard: .*(pin|git checkout|oracle checkout)"; then
    echo "  FAIL c. a GIT_DIR decoy moved the HEAD check:"; printf '%s\n' "$out" | sed 's/^/       | /'; ok=0
  elif [ "$rc" = "0" ] || { [ "$rc" = "2" ] && printf '%s' "$out" | grep -q "no real C compiler"; }; then
    echo "  ok   c. GIT_DIR + GIT_WORK_TREE decoy does not move the HEAD check (rc=$rc)"
  else
    echo "  FAIL c. unexpected rc=$rc:"; printf '%s\n' "$out" | sed 's/^/       | /'; ok=0
  fi

  # d. the PRODUCTION pin: nothing sets ORACLE_PIN here, so the value this run
  #    read from the checked-in ORACLE_PIN stays in force. The fixture oracle's
  #    HEAD is never the real pin, so the guard must BLOCK naming that pin.
  local real_pin
  real_pin="$(rustty_oracle_pin)" || real_pin=""
  rc=0
  out="$(GHOSTTY="$tmp/ghostty" DIAG_DIR="$tmp/diag"; enforce_driver_source_fresh 2>&1)" || rc=$?  # R5-selftest-fixture
  if [ -n "$real_pin" ] && [ "$rc" = "3" ] && printf '%s' "$out" | grep -q "not the pin $real_pin"; then
    echo "  ok   d. the checked-in pin is in force: off-pin fixture => BLOCKED exit 3"
  else
    echo "  FAIL d. with the checked-in pin (${real_pin:-unreadable}) an off-pin oracle gave rc=$rc, want 3 'not the pin <ORACLE_PIN>':"
    printf '%s\n' "$out" | sed 's/^/       | /'; ok=0
  fi

  # e-k. a COPY of this gate, run end to end (`--progress`) in a fixture checkout
  #    with its own ORACLE_PIN and a fixture oracle beside it, from a cwd outside
  #    it, with the retired GHOSTTY override and a GIT_DIR decoy exported. This
  #    runs the gate's own pin-read line, so a pin taken from anywhere but
  #    ORACLE_PIN (the oracle's HEAD, a hardcoded SHA) or a pin read that kills
  #    the run under `set -e` goes RED here.
  local co lay_head copy_rc copy_out pinfile malformed
  co="$tmp/layout/rustty"
  mkdir -p "$co/.port/tools/gates" "$co/.port/phase-0/harness" "$co/.port/phase-0/corpora/cvt"
  if ! { cp "$GATES_DIR/run_differential.sh" "$GATES_DIR/_oracle_root.sh" "$co/.port/tools/gates/" &&
    cp "$REPO_ROOT/.port/tools/parity_names.env" "$co/.port/tools/" &&
    : >"$co/.port/phase-0/harness/comparator.py" &&
    rustty_fixture_git -C "$co" init -q >/dev/null 2>&1 &&
    lay_head="$(rustty_fixture_repo "$tmp/layout/ghostty" README "layout oracle")"; }; then
    echo "  FAIL e-k. could not build the fixture checkout"; rm -rf "$tmp"; return 1
  fi
  pinfile="$co/.port/tools/gates/ORACLE_PIN"
  copy_env() ( # the copy's environment: no inherited GIT_*, then the decoys
    cd "$tmp" || exit 1
    for _v in $(compgen -e); do
      case "$_v" in GIT_*) unset "$_v" ;; esac
    done
    export GHOSTTY="$tmp/decoy" GIT_DIR="$tmp/decoy/.git" GIT_WORK_TREE="$tmp/decoy"  # R5-selftest-fixture
    "$@"
  )
  run_copy() { # sets copy_rc / copy_out
    copy_rc=0
    copy_out="$(copy_env bash "$co/.port/tools/gates/run_differential.sh" --progress 2>&1)" || copy_rc=$?
  }
  pin_arm() { # pin_arm <label> <want-substring>
    if [ "$copy_rc" = "3" ] && [[ "$copy_out" == *"$2"* ]]; then
      echo "  ok   $1 => BLOCKED exit 3"
    else
      echo "  FAIL $1 gave rc=$copy_rc, want 3 with '$2':"
      printf '%s\n' "$copy_out" | sed 's/^/       | /'; ok=0
    fi
  }
  printf '%s\n' "$decoy_head" >"$pinfile"; run_copy
  pin_arm "e. copy: ORACLE_PIN names another commit" "at $lay_head, not the pin $decoy_head"
  printf '%s\n' "$lay_head" >"$pinfile"; run_copy
  pin_arm "f. copy: ORACLE_PIN names the oracle's HEAD (pin check passes; no zig-out/include)" \
    "oracle include dir missing (" # (the resolved path is canonical: /private/var on macOS)
  [[ "$copy_out" == *"/layout/ghostty/zig-out/include)"* ]] ||
    { echo "  FAIL f. the guard did not name the fixture checkout's sibling oracle"; ok=0; }
  malformed="ORACLE_PIN is missing or not a 40-hex commit"
  rm -f "$pinfile"; run_copy
  pin_arm "g. copy: ORACLE_PIN missing" "$malformed"
  : >"$pinfile"; run_copy
  pin_arm "h. copy: ORACLE_PIN empty" "$malformed"
  printf '%s\n' "${lay_head:0:39}" >"$pinfile"; run_copy
  pin_arm "i. copy: ORACLE_PIN 39 hex" "$malformed"
  printf '%s\n' "$lay_head" | tr 'a-f' 'A-F' >"$pinfile"; run_copy
  pin_arm "j. copy: ORACLE_PIN upper-case" "$malformed"
  # j2. upper-case with no `F`: a locale-collated `[a-f]` range accepts it.
  printf '%s\n' "$lay_head" | tr 'a-f' 'A-F' | tr 'F' 'E' >"$pinfile"; run_copy
  pin_arm "j2. copy: ORACLE_PIN upper-case without an F" "$malformed"
  printf '%s\r\n' "$lay_head" >"$pinfile"; run_copy
  pin_arm "k. copy: ORACLE_PIN with a CRLF line ending" "$malformed"

  rm -rf "$tmp"
  if [ "$ok" = "1" ]; then echo "G4 ORACLE SELF-TEST: PASS"; return 0; fi
  echo "G4 ORACLE SELF-TEST: FAIL"; return 1
}

if [ "$MODE" = "self-test" ]; then
  run_self_test
  exit $?
fi
if [ "$MODE" = "self-test-oracle" ]; then
  run_self_test_oracle
  exit $?
fi

# ---------------------------------------------------------------------------
# REAL (armed) gate: the port driver exists — run both legs, byte-exact.
# Any diff is RED (exit 1). gate_empty_divergence.sh is NOT run here: its
# assert-100%-divergence contract is only meaningful against an empty port.
# ---------------------------------------------------------------------------
if [ "$MODE" = "auto" ] && [ -x "$PORT_DRIVER" ]; then
  echo "--- REAL differential gate (port driver: $PORT_DRIVER) ---"
  if [ ! -x "$REF_DRIVER" ]; then
    echo "BLOCKED: reference driver missing ($REF_DRIVER); run build_ref_driver.sh"
    exit 3
  fi
  enforce_driver_source_fresh
  fail=0
  run_fixture_leg "$PORT_DRIVER" || fail=1
  run_cvt_golden_leg || fail=1
  if [ "$fail" = "0" ]; then
    echo "GREEN: byte-exact across all fixtures and all cvt golden cases (G4)"
    exit 0
  else
    echo "RED: differential divergence(s) found — see per-case lines above and $DIAG_DIR/*.diag (G4)"
    exit 1
  fi
fi

# --- progress mode: harness self-test proves the harness is wired ---
# gate_empty_divergence.sh's assert-100%-divergence contract is only meaningful
# against an EMPTY port, so it runs ONLY here (never on the armed branch). The
# port_hook stub auto-forwards to bin/vt_driver_port when it exists, so we pin
# RUSTTY_VT_DRIVER to a non-executable path for the sub-run: forced --progress
# stays a pure harness-soundness proof (empty stub => 100% divergence) even
# after the port driver has been built.
echo "--- PROGRESS mode: port driver not armed for this run ($PORT_DRIVER) ---"
echo "    G4 cannot be parity-GREEN without the port driver; running the"
echo "    harness self-test (proves the comparator detects total divergence)."
enforce_driver_source_fresh
if [ -x "$HARNESS/gate_empty_divergence.sh" ]; then
  if RUSTTY_VT_DRIVER="/nonexistent/empty-port-stub" bash "$HARNESS/gate_empty_divergence.sh"; then
    echo "HARNESS-WIRED (port empty): comparator self-test PASS; G4 in progress mode"
    exit 0
  else
    echo "BLOCKED: harness self-test FAILED — the differential harness is broken"
    exit 3
  fi
else
  # fall back to the runner self-test (wiring assertion only)
  run_self_test && {
    echo "HARNESS-WIRED (port empty): runner wiring OK; G4 in progress mode"
    exit 0
  }
  exit 3
fi
