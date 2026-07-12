#!/usr/bin/env bash
# check_stale_comments.sh — DETECTOR D9: stale-comment contract check.
#
# Spec: BUN_PORT_LEARNINGS.md §4 D9 (+ PROPOSED R7 "Comments" bullet).
#   "Wherever a Zig builtin was replaced by a Rust primitive with DIFFERENT
#    failure semantics (reinterpretSlice->cast_slice, catch unreachable->expect,
#    wrapping/saturating ops, &mut->raw), the adjacent ported comment must state
#    the RUST primitive's contract; any comment of the form 'len/align is
#    guaranteed X' near a panicking cast requires a test asserting X."
#
# THE HAZARD (cross-cutting, invisible to a diff): the code was translated
# faithfully but the *comment* still describes the OLD (Zig) primitive's
# behavior. Zig `reinterpretSlice` silently drops a trailing odd byte;
# `bytemuck::cast_slice` PANICS on a length/align mismatch. Zig `catch
# unreachable` is UB-on-error in ReleaseFast; Rust `.expect(..)` always panics.
# `+%` wraps; a bare `+` debug-panics. A comment that still says "silently
# drops" / "wraps" / "never fails" next to the panicking Rust primitive is a
# LIE that will mislead the next porter — exactly the class this detector kills.
#
# WHAT IT IS: a GREP HEURISTIC that flags CANDIDATE stale comments for human
# review (it cannot decide intent — false positives are expected and are
# dispositioned in the review ledger). It is deliberately advisory-first:
#   * default run  -> print every candidate + the count, EXIT 0 (progress).
#   * --strict     -> EXIT 1 if any candidate lacks a ledger disposition.
#   * ledger SEALED (see below) -> the default run also binds (progress->binding
#     flip, exactly like the other PROGRESS gates: green while the review is
#     open, RED the instant a new un-dispositioned candidate appears after the
#     ledger is sealed). This is R5-correct — the gate is never weakened, it
#     starts advisory and tightens as the review completes.
#
# REVIEW LEDGER: .port/audits/stale-comments-D9.tsv
#   TSV, one row per reviewed candidate:  file <TAB> line <TAB> disposition <TAB> note
#   disposition in { contract-ok | false-positive | needs-fix }.
#   A `# STATUS: SEALED` line in the ledger flips the default run to binding.
# CHECKLIST RULE: .port/audits/D9_STALE_COMMENT_CHECKLIST.md (the process rule
#   a reviewer applies to each flagged candidate).
#
# TWO CANDIDATE CLASSES:
#   STALE-SEMANTIC   a panic/raw primitive (cast_slice, .expect, wrapping_*,
#                    saturating_*, &mut->*mut) whose adjacent comment describes
#                    NON-panic / old-Zig behavior and does NOT state the Rust
#                    primitive's contract.
#   GUARANTEE-TEST   a "len/align is guaranteed X" comment adjacent to a
#                    PANICKING cast (cast_slice/align_to/bytes_of): D9 requires a
#                    test asserting X. Flagged for a reviewer to confirm the test
#                    exists (auto-detecting "a test asserting X" is out of scope
#                    for a grep heuristic).
#
# Exit: 0 GREEN/advisory-progress | 1 RED (strict or sealed: un-dispositioned
#       candidate) | 2 usage | 3 BLOCKED (tooling/paths missing — WARNING).
set -uo pipefail

GATES_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$GATES_DIR/../../.." && pwd)"
LEDGER="$ROOT/.port/audits/stale-comments-D9.tsv"
WINDOW="${D9_WINDOW:-3}"     # comment-scan window (lines above the primitive)

usage() {
  cat >&2 <<EOF
usage: check_stale_comments.sh [--strict] [--list] [--self-test] [--help]
  (no args)    advisory scan of crates/*/src; print candidates + count; exit 0
               (exit 1 iff the ledger is SEALED and a candidate is un-dispositioned)
  --strict     exit 1 if ANY candidate lacks a ledger disposition
  --list       print candidates only (machine form: file:line:CLASS)
  --self-test  run the embedded fixtures (prove flag + clear behaviour)
EOF
}

# ---- the heuristic (awk over one file) ------------------------------------
# Emits: <file>\t<line>\t<CLASS>\t<primitive-snippet>\t<comment-snippet>
scan_file() {
  local f="$1"
  awk -v FN="$f" -v WIN="$WINDOW" '
    # A comment line?  (leading // or /// or //! or a trailing // on code)
    function comment_of(s,   p) { p=index(s,"//"); return (p>0)? substr(s,p) : "" }

    # Regexes ----------------------------------------------------------------
    # Panic/raw Rust primitives whose Zig origin had different failure semantics.
    function is_prim(s) {
      return (s ~ /cast_slice|cast_slice_mut|[Bb]ytes_of|align_to(_mut)?\(/) ||
             (s ~ /\.expect\(/) ||
             (s ~ /wrapping_[a-z_]+\(/) || (s ~ /saturating_[a-z_]+\(/) ||
             (s ~ /&mut [^;]*as \*mut|as_mut_ptr\(\)/)
    }
    # Panicking CAST specifically (for the guarantee/test rule).
    function is_cast(s) {
      return (s ~ /cast_slice|cast_slice_mut|align_to(_mut)?\(|[Bb]ytes_of/)
    }
    # Comment describes OLD / non-panic behaviour (Zig semantics left stale).
    function stale(c) {
      return c ~ /silently|wraps around|wrap on overflow|wraps on overflow|catch unreachable|never fails|cannot fail|can.t fail|infallible|no bounds check|drops? (the |a |trailing|any |an )|reinterpretSlice|@bitCast|@ptrCast|@truncate|@intCast/
    }
    # Comment already states the RUST primitive contract -> clears a stale hit.
    function contract(c) {
      return c ~ /panic|assert|debug_assert|\.expect|must be|must have|must not|requires|REQUIRES|invariant|SAFETY|bit-preserving|same bits|identical bit|will abort|\.unwrap|exact-len|checked cast|is checked|bounds-checked/
    }
    # "len/align is guaranteed X" style comment.
    function guarantee_len(c) {
      return (c ~ /guarantee[ds]?/ && c ~ /len|length|align|multiple|divisibl|even byte|byte multiple|4-byte|2-byte/) ||
             ((c ~ /len|length|align|multiple/) && c ~ /guarantee/)
    }

    { L[NR]=$0 }
    END {
      for (i=1;i<=NR;i++) {
        line=L[i]
        # strip the leading "// ..." only-comment lines from primitive matching
        codepart=line
        if (line ~ /^[[:space:]]*\/\//) codepart=""   # pure comment line, no code
        if (codepart=="" ) continue

        if (is_prim(codepart)) {
          # gather window comments: WIN lines above + trailing comment on line i
          winc=""
          for (j=i-WIN; j<=i; j++) { if (j<1) continue; c=comment_of(L[j]); if (c!="") winc=winc " " c }
          if (winc=="") continue
          # STALE-SEMANTIC: stale marker present, contract marker absent
          if (stale(winc) && !contract(winc)) {
            snip=codepart; sub(/^[[:space:]]+/,"",snip)
            cs=winc; sub(/^ /,"",cs)
            printf "%s\t%d\t%s\t%.80s\t%.100s\n", FN, i, "STALE-SEMANTIC", snip, cs
            continue
          }
          # GUARANTEE-TEST: only for panicking casts
          if (is_cast(codepart) && guarantee_len(winc)) {
            snip=codepart; sub(/^[[:space:]]+/,"",snip)
            cs=winc; sub(/^ /,"",cs)
            printf "%s\t%d\t%s\t%.80s\t%.100s\n", FN, i, "GUARANTEE-TEST", snip, cs
          }
        }
      }
    }
  ' "$f"
}

collect() {
  # All Rust sources under the workspace crates (skip target/ and generated).
  local dirs=("$ROOT/crates" "$ROOT/crates-sys")
  local files=()
  local d
  for d in "${dirs[@]}"; do
    [ -d "$d" ] || continue
    while IFS= read -r f; do files+=("$f"); done < <(find "$d" -type f -name '*.rs' -not -path '*/target/*')
  done
  [ "${#files[@]}" -gt 0 ] || { echo "BLOCKED: no Rust sources found under crates/" >&2; return 3; }
  local f
  for f in "${files[@]}"; do scan_file "$f"; done
}

# Is (relpath,line) dispositioned in the ledger?  echoes disposition or "".
ledger_disp() {
  local rel="$1" ln="$2"
  [ -f "$LEDGER" ] || { echo ""; return; }
  awk -F'\t' -v R="$rel" -v N="$ln" '
    /^[[:space:]]*#/ {next} NF<3 {next}
    $1==R && $2==N { print $3; found=1; exit }
    END { if(!found) print "" }
  ' "$LEDGER"
}

ledger_sealed() {
  [ -f "$LEDGER" ] && grep -qE '^[[:space:]]*#[[:space:]]*STATUS:[[:space:]]*SEALED' "$LEDGER"
}

run_scan() {
  local mode="$1"   # advisory | strict | list
  local raw; raw="$(collect)"; local rc=$?
  [ "$rc" -eq 3 ] && return 3

  local n_stale=0 n_guar=0 n_undisp=0 n_ok=0 n_fp=0 n_fix=0
  local out_stale="" out_guar="" out_undisp=""
  while IFS=$'\t' read -r file ln cls prim cmt; do
    [ -n "$file" ] || continue
    local rel="${file#$ROOT/}"
    local disp; disp="$(ledger_disp "$rel" "$ln")"
    case "$cls" in STALE-SEMANTIC) n_stale=$((n_stale+1));; GUARANTEE-TEST) n_guar=$((n_guar+1));; esac
    case "$disp" in
      contract-ok)     n_ok=$((n_ok+1));;
      false-positive)  n_fp=$((n_fp+1));;
      needs-fix)       n_fix=$((n_fix+1)); out_undisp+="  [needs-fix]   $rel:$ln  $cls"$'\n'; n_undisp=$((n_undisp+1));;
      "")              n_undisp=$((n_undisp+1)); out_undisp+="  [UN-REVIEWED] $rel:$ln  $cls"$'\n';;
    esac
    if [ "$mode" = "list" ]; then echo "$rel:$ln:$cls"; continue; fi
    local rec="  $rel:$ln
     primitive: $prim
     comment  : $cmt"
    if [ "$cls" = "STALE-SEMANTIC" ]; then out_stale+="$rec"$'\n'; else out_guar+="$rec"$'\n'; fi
  done <<< "$raw"

  [ "$mode" = "list" ] && return 0

  echo "=================================================================="
  echo " D9 stale-comment contract check  (heuristic; candidates for review)"
  echo " ledger: ${LEDGER#$ROOT/}$( ledger_sealed && echo '  [SEALED]' || echo '  [open]')"
  echo "=================================================================="
  echo
  echo "-- CLASS 1: STALE-SEMANTIC (comment describes old/non-panic behaviour"
  echo "            next to a panicking/raw Rust primitive; no Rust contract) --"
  [ -n "$out_stale" ] && printf '%s' "$out_stale" || echo "  (none)"
  echo
  echo "-- CLASS 2: GUARANTEE-TEST (len/align 'guaranteed' comment next to a"
  echo "            panicking cast — REQUIRES a test asserting the guarantee) --"
  [ -n "$out_guar" ] && printf '%s' "$out_guar" || echo "  (none)"
  echo
  echo "------------------------------------------------------------------"
  echo "CANDIDATES: $((n_stale+n_guar))   (STALE-SEMANTIC=$n_stale, GUARANTEE-TEST=$n_guar)"
  echo "LEDGER    : contract-ok=$n_ok  false-positive=$n_fp  needs-fix=$n_fix  un-reviewed=$((n_undisp-n_fix))"
  if [ "$n_undisp" -gt 0 ]; then
    echo
    echo "UN-DISPOSITIONED (review + record in ${LEDGER#$ROOT/}):"
    printf '%s' "$out_undisp"
  fi
  echo "------------------------------------------------------------------"

  # Exit policy.
  if [ "$mode" = "strict" ] || ledger_sealed; then
    if [ "$n_undisp" -gt 0 ]; then
      echo ">> RED: $n_undisp candidate(s) lack a ledger disposition ($([ "$mode" = strict ] && echo --strict || echo 'ledger SEALED'))." >&2
      return 1
    fi
    echo ">> GREEN: every candidate is dispositioned in the ledger."
    return 0
  fi
  echo ">> ADVISORY (progress): $((n_stale+n_guar)) candidate(s) printed; not failing CI."
  echo "   Review each per .port/audits/D9_STALE_COMMENT_CHECKLIST.md and record"
  echo "   its disposition in ${LEDGER#$ROOT/}. Seal the ledger to make D9 bind."
  return 0
}

# ---- embedded self-test ----------------------------------------------------
self_test() {
  local tmp; tmp="$(mktemp -d)"; trap 'rm -rf "$tmp"' RETURN
  local fx="$tmp/fixture.rs"
  cat > "$fx" <<'RS'
// case A (SHOULD FLAG): comment still claims the Zig silent-drop behaviour.
// reinterpretSlice silently drops a trailing odd byte here.
let words: &[u16] = bytemuck::cast_slice(buf);

// case B (SHOULD FLAG): comment claims the lookup never fails.
// this lookup never fails for a valid id.
let v = map.get(&id).expect("id present");

// case C (SHOULD CLEAR): comment states the Rust panic contract.
// bytemuck::cast_slice PANICS unless buf.len() is a multiple of 2; asserted below.
let half: &[u16] = bytemuck::cast_slice(buf);

// case D (SHOULD FLAG, GUARANTEE-TEST): guaranteed-len near a panicking cast.
// buf.len() is guaranteed to be a multiple of 4 by the caller.
let px: &[u32] = bytemuck::cast_slice(buf);

// case E (SHOULD NOT FLAG): plain expect, no stale/guarantee comment nearby.
let x = opt.expect("present");
RS
  echo "self-test fixture: $fx"
  local res; res="$(scan_file "$fx")"
  echo "--- raw candidates ---"; printf '%s\n' "$res"
  local ok=0 fail=0
  chk() { # desc  expect-present(1/0)  grep-pattern
    local desc="$1" want="$2" pat="$3" got=0
    printf '%s' "$res" | grep -qE "$pat" && got=1
    if [ "$got" = "$want" ]; then echo "  PASS: $desc"; ok=$((ok+1)); else echo "  FAIL: $desc (want=$want got=$got)"; fail=$((fail+1)); fi
  }
  echo "--- assertions ---"
  chk "A cast_slice+silently-drops flagged"     1 'STALE-SEMANTIC.*cast_slice'
  chk "B .expect+never-fails flagged"           1 'STALE-SEMANTIC.*expect'
  chk "C contract-stating comment cleared"      0 $'\t9\t'
  chk "D guaranteed-len near cast flagged"      1 'GUARANTEE-TEST'
  chk "E plain expect not flagged"              0 $'\t22\t'
  echo "self-test: $ok passed, $fail failed"
  [ "$fail" -eq 0 ]
}

# ---- dispatch --------------------------------------------------------------
case "${1:-}" in
  ""|--advisory) run_scan advisory; exit $? ;;
  --strict)      run_scan strict;   exit $? ;;
  --list)        run_scan list;     exit $? ;;
  --self-test)   self_test;         exit $? ;;
  -h|--help)     usage;             exit 0  ;;
  *)             echo "unknown arg: $1" >&2; usage; exit 2 ;;
esac
