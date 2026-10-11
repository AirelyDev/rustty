#!/usr/bin/env bash
# GATE G-TGTDIR — the gates work with a non-default CARGO_TARGET_DIR (P8 CI
# speed-up, controller input from a live run).
#
# Setting CARGO_TARGET_DIR for ci.sh broke G-EXAMPLES, D11r, A5, G-VTFEAT and
# G-XCF: each built rustty-vt-c with cargo (which honours the variable) and then
# read `<repo>/target/release/...` literally, so the examples failed to link
# (`ld: library 'rustty_vt_c' not found`), and G-LINT linted the custom target
# dir through prettier and typos. The gates now read cargo's own
# `target_directory` (_target_dir.sh); xtask, which folds archives it finds
# under `<root>/target`, runs under `env -u CARGO_TARGET_DIR`; G-LINT skips every
# target root. This gate keeps it that way:
#
#   1  STATIC. No registered gate runner (the scripts ci.sh's registry runs)
#      and no example/*/build.sh reads a literal target/<profile> artifact path
#      outside a comment. ALLOWED lists the exceptions, each with its reason;
#      a stale ALLOWED row is RED. Control: a fixture with the old D11r line.
#   2  LINT. check_lints.sh's shellcheck and typos lanes, in a fixture checkout
#      whose ./target-alt/ and CARGO_TARGET_DIR hold a shellcheck warning and a
#      typo: GREEN. Control: the same files in ./notarget/ are RED (the lanes
#      are live). The prettier lane uses the same exclusion list.
#   3  DYNAMIC. D11r, A5, G-ABISCHEMA and example/c-vt run with
#      CARGO_TARGET_DIR set to a dir that is not <repo>/target, and pass; the
#      library they used is the one cargo wrote there. Control: D11r with its
#      old literal library path, pointed at a dir cargo did not write, is RED.
#
# CARGO_TARGET_DIR: ci.sh gives this gate its own (dir=tgtdir) in a parallel
# run; otherwise it uses target/tgtdir-check. Exit: 0 GREEN, 1 RED, 3 BLOCKED
# (no cargo / python3, or no C compiler for arm 3).
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$ROOT" || exit 1
GATES="$ROOT/.port/tools/gates"
command -v cargo >/dev/null 2>&1 || { echo "G-TGTDIR BLOCKED: cargo not installed"; exit 3; }
command -v python3 >/dev/null 2>&1 || { echo "G-TGTDIR BLOCKED: python3 not installed"; exit 3; }

fail=0
ARM2_SKIPPED=0
ARM3_SKIPPED=0
# TGTDIR_ARMS="1 2": run only those arms (no builds). A subset is never GREEN:
# it ends BLOCKED, and ci.sh names TGTDIR_ARMS as a narrowing variable.
ARMS="${TGTDIR_ARMS:-1 2 3}"
want_arm() { case " $ARMS " in *" $1 "*) return 0 ;; esac; return 1; }
ok()  { echo "  ok    $1"; }
bad() { echo "  FAIL  $1"; fail=1; }

# ---- arm 1: static ----------------------------------------------------------
# ALLOWED: "<repo-relative file>|<reason>".
ALLOWED=(
  ".port/tools/gates/check_dist.sh|its target/release read runs inside the Debian container, on a tarball freshly extracted there, where no CARGO_TARGET_DIR is set"
  ".port/tools/gates/check_perf_fence.sh|builds every artifact it measures under env -u CARGO_TARGET_DIR, so the default dir is where they are"
)
# static_scan FILE... -> prints "file:line: text" for each literal artifact read
static_scan() {
  python3 - "$@" <<'PY'
import re, sys
# (?<!...): not a longer word, and not the shell variable $target / ${target}.
pat = re.compile(r'''(?<![\w${-])target/(debug|release|release-fast|wasm32-[a-z0-9-]+/(debug|release))\b'''
                 r'''|["']target["']\s*\)?\s*/\s*["'](debug|release)["']''')
for f in sys.argv[1:]:
    try:
        lines = open(f, encoding="utf-8").read().split("\n")
    except OSError:
        continue
    for n, l in enumerate(lines, 1):
        s = l.strip()
        if s.startswith("#") or s.startswith("//"):
            continue
        if pat.search(l):
            print("%s:%d: %s" % (f, n, s[:140]))
PY
}
W="$(mktemp -d "${TMPDIR:-/tmp}/tgtdir.XXXXXX")"
if want_arm 1; then
echo "=== G-TGTDIR arm 1: no literal target/<profile> artifact path in the gates or examples ==="
files="$(sed -n 's/^  "[A-Za-z0-9-]*|[A-Z-]*|[a-z]*|[^|]*|\(.*\)"$/\1/p' "$ROOT/.port/tools/ci.sh" \
  | grep -oE '\$(GATES|ROOT)/[^ ]+\.(sh|py)' | sed "s#^\$GATES/#.port/tools/gates/#; s#^\$ROOT/##" | sort -u)"
# Also: the tools ci.sh runs from its own body rather than the registry (the
# --webgl lane's wasm checks), and every tracked Node tool under .port/tools.
# The Node tools read artifacts through _target_dir.mjs; before that they
# spelled `../../target/...`, which no arm saw (A5's struct-layout checker
# failed to link under a non-default CARGO_TARGET_DIR for that reason).
files="$files
$(grep -oE '\$ROOT/\.port/tools/[A-Za-z0-9_./-]+\.(sh|py|mjs)' "$ROOT/.port/tools/ci.sh" | sed 's#^\$ROOT/##')
$(cd "$ROOT" && git ls-files '.port/tools/*.mjs' '.port/tools/*.js' '.port/tools/**/*.mjs' '.port/tools/**/*.js')"
files="$(printf '%s\n' "$files" | grep . | sort -u)"
# This gate's own fixture strings are not reads; it is not scanned.
files="$(printf '%s\n' "$files" | grep -vx '.port/tools/gates/check_target_dir.sh')
$(ls example/*/build.sh 2>/dev/null)"
nfiles="$(printf '%s\n' "$files" | grep -c .)"
[ "$nfiles" -gt 50 ] || bad "arm 1 found only $nfiles file(s) to scan (registry parse broke?)"
# shellcheck disable=SC2086  # repo-relative paths without spaces
hits="$(static_scan $files)"
unallowed=""; used=""
while IFS= read -r h; do
  [ -n "$h" ] || continue
  f="${h%%:*}"; a=0
  for e in "${ALLOWED[@]}"; do [ "${e%%|*}" = "$f" ] && { a=1; used="$used $f"; }; done
  [ "$a" = 1 ] || unallowed="$unallowed
    $h"
done <<EOF
$hits
EOF
if [ -z "$unallowed" ]; then ok "arm 1: $nfiles files, no literal artifact path outside ALLOWED"
else bad "arm 1: literal target/<profile> artifact paths (read cargo's target_directory instead, _target_dir.sh):$unallowed"; fi
for e in "${ALLOWED[@]}"; do
  case " $used " in *" ${e%%|*} "*) ;; *) bad "arm 1: stale ALLOWED row ${e%%|*} (it has no literal path any more)" ;; esac
done
printf '%s\n' '"$CC" -I"$root/include" -L"$root/target/release" -lrustty_vt_c d11r.c -o d11r' > "$W/old_d11r.sh"
printf '%s\n' '        lib = ROOT / "target" / "debug" / name' > "$W/old_vtfeat.py"
printf '%s\n' 'const UU = new URL("../../target/wasm32-unknown-unknown/release/rustty_vt_c.wasm", import.meta.url);' > "$W/old_roundtrip.mjs"
printf '%s\n' '  `${ROOT}/target/release`,' > "$W/old_layouts.mjs"
if [ "$(static_scan "$W/old_d11r.sh" "$W/old_vtfeat.py" "$W/old_roundtrip.mjs" "$W/old_layouts.mjs" | grep -c .)" = 4 ]; then ok "arm 1 control: the old D11r, G-VTFEAT, wasm-vt-roundtrip and struct-layout lines are each flagged"
else bad "arm 1 control: the old literal lines were not all flagged"; fi
case "$files" in *.port/tools/wasm-vt-roundtrip.mjs*) ok "arm 1: the Node tools are in the scanned set" ;; *) bad "arm 1: the Node tools are not in the scanned set" ;; esac
fi

# ---- arm 2: G-LINT skips target roots ----------------------------------------
if want_arm 2; then
echo "=== G-TGTDIR arm 2: G-LINT skips every target root, and only target roots ==="
lint_fixture() {  # lint_fixture DIR SUBDIR — a checkout with bad files in SUBDIR
  local d="$1"
  mkdir -p "$d/.port/tools/gates" "$d/src" "$d/$2"
  cp "$GATES/check_lints.sh" "$GATES/_target_dir.sh" "$d/.port/tools/gates/"
  cp "$ROOT/.shellcheckrc" "$d/" 2>/dev/null
  printf '[package]\nname = "fx"\nversion = "0.1.0"\nedition = "2021"\n[lib]\npath = "src/lib.rs"\n' > "$d/Cargo.toml"
  : > "$d/src/lib.rs"
  printf '#!/usr/bin/env bash\ncd /tmp\n' > "$d/$2/bad.sh"            # SC2164 (warning)
  printf 'teh quick fox\n' > "$d/$2/bad.txt"                           # a typo
}
if command -v shellcheck >/dev/null 2>&1 && command -v typos >/dev/null 2>&1; then
  lint_fixture "$W/lint" target-alt
  # A nested cargo target root (CACHEDIR.TAG), as a harness crate's target/.
  mkdir -p "$W/lint/sub/target"; printf 'Signature: 8a477f597d28d172789f06886806bc55\n' > "$W/lint/sub/target/CACHEDIR.TAG"
  printf '#!/usr/bin/env bash\ncd /tmp\n' > "$W/lint/sub/target/bad.sh"; printf 'teh\n' > "$W/lint/sub/target/bad.txt"
  mkdir -p "$W/lint/out-ctd"
  printf '#!/usr/bin/env bash\ncd /tmp\n' > "$W/lint/out-ctd/bad.sh"; printf 'teh\n' > "$W/lint/out-ctd/bad.txt"
  if out="$(cd "$W/lint" && CARGO_TARGET_DIR="$W/lint/out-ctd" bash .port/tools/gates/check_lints.sh shellcheck typos 2>&1)"; then
    ok "arm 2: bad files in ./target-alt/, a CACHEDIR.TAG root (./sub/target/) and CARGO_TARGET_DIR (./out-ctd/) are not linted"
  else bad "arm 2: G-LINT linted a target root:"; printf '%s\n' "$out" | tail -8 | sed 's/^/        /'; fi
  # Control (M3): a source dir merely NAMED target (no CACHEDIR.TAG, not at
  # the root) is linted like any other.
  lint_fixture "$W/lint-ctl" src/target
  if out="$(cd "$W/lint-ctl" && bash .port/tools/gates/check_lints.sh shellcheck typos 2>&1)"; then
    bad "arm 2 control: the same bad files outside a target root went unflagged (lanes not live)"
  elif printf '%s\n' "$out" | grep -q 'FAIL    shellcheck' && printf '%s\n' "$out" | grep -q 'FAIL    typos'; then
    ok "arm 2 control: the same files in a plain ./src/target/ dir fail shellcheck and typos"
  else bad "arm 2 control: unexpected output"; printf '%s\n' "$out" | tail -8 | sed 's/^/        /'; fi
else
  echo "  skip  arm 2: shellcheck or typos not installed (G-LINT reports them skipped by name)"
  ARM2_SKIPPED=1
fi

fi

# ---- arm 3: the gates under a non-default CARGO_TARGET_DIR -------------------
if want_arm 3; then
echo "=== G-TGTDIR arm 3: D11r, A5, G-ABISCHEMA, example/c-vt with a non-default CARGO_TARGET_DIR ==="
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target/tgtdir-check}"
mkdir -p "$CARGO_TARGET_DIR"
CARGO_TARGET_DIR="$(cd "$CARGO_TARGET_DIR" && pwd)"
if [ "$CARGO_TARGET_DIR" = "$ROOT/target" ]; then bad "arm 3: CARGO_TARGET_DIR is the default dir; nothing would be proven"; fi
echo "  CARGO_TARGET_DIR=$CARGO_TARGET_DIR"
run_arm() {  # run_arm NAME CMD...
  local name="$1" out rc; shift
  out="$("$@" 2>&1)"; rc=$?
  if [ "$rc" = 0 ]; then ok "arm 3: $name passes"
  elif [ "$rc" = 3 ]; then echo "  skip  arm 3: $name BLOCKED here: $(printf '%s\n' "$out" | grep -m1 BLOCKED)"; ARM3_SKIPPED=1
  else bad "arm 3: $name (exit $rc)"; printf '%s\n' "$out" | tail -8 | sed 's/^/        /'; fi
}
run_arm "D11r" sh "$GATES/check_closed_enum_abi_runtime.sh"
run_arm "A5" sh "$GATES/check_rustty_vt_headers.sh"
run_arm "G-ABISCHEMA" bash "$GATES/check_abi_schema.sh"
run_arm "example/c-vt" sh "$ROOT/example/c-vt/build.sh"
lib="$(ls "$CARGO_TARGET_DIR"/release/librustty_vt_c.dylib "$CARGO_TARGET_DIR"/release/librustty_vt_c.so 2>/dev/null | head -1)"
if [ -n "$lib" ] && [ -f "$lib" ]; then ok "arm 3: the library they used is cargo's, in CARGO_TARGET_DIR ($lib)"
else bad "arm 3: no librustty_vt_c in $CARGO_TARGET_DIR/release"; fi
# Control: D11r as it was (a literal "$root/target/release"), pointed at a dir
# cargo did not write, must fail to link.
mkdir -p "$W/ctl/.port/tools/gates" "$W/ctl/empty/release"
sed -e "s#^root=\$(cd \"\$(dirname \"\$0\")/../../..\" && pwd); cd \"\$root\"#root=\"$ROOT\"; cd \"\$root\"#" \
    -e 's#-L"$rel"#-L"$root/target/release"#' -e 's#DYLD_LIBRARY_PATH="$rel" LD_LIBRARY_PATH="$rel"#DYLD_LIBRARY_PATH="$root/target/release" LD_LIBRARY_PATH="$root/target/release"#' \
    -e "s#\"\$root/target/release\"#\"$W/ctl/empty/release\"#g" \
    "$GATES/check_closed_enum_abi_runtime.sh" > "$W/ctl/.port/tools/gates/d11r_literal.sh"
if grep -q "$W/ctl/empty/release" "$W/ctl/.port/tools/gates/d11r_literal.sh" \
   && grep -q "^root=\"$ROOT\"" "$W/ctl/.port/tools/gates/d11r_literal.sh"; then
  if sh "$W/ctl/.port/tools/gates/d11r_literal.sh" >"$W/ctl.log" 2>&1; then bad "arm 3 control: D11r with a literal path to a dir cargo did not write stayed GREEN"
  else ok "arm 3 control: D11r with its old literal library path fails ($(grep -m1 -iE "not found|no such|error" "$W/ctl.log" | cut -c1-80))"; fi
else bad "arm 3 control: could not build the literal-path copy of D11r"; fi

fi

if [ "$fail" -ne 0 ]; then echo "G-TGTDIR RED (fixtures in $W)"; exit 1; fi
if [ "$ARMS" != "1 2 3" ]; then echo "G-TGTDIR BLOCKED: TGTDIR_ARMS=\"$ARMS\" ran a subset; never a full GREEN"; exit 3; fi
if [ "$ARM3_SKIPPED" = 1 ]; then echo "G-TGTDIR BLOCKED: arm 3 sub-run(s) BLOCKED above, so arm 3 is unproven here"; exit 3; fi
if [ "$ARM2_SKIPPED" = 1 ]; then echo "G-TGTDIR BLOCKED: arms 1 and 3 GREEN, arm 2 could not run (no shellcheck or typos)"; exit 3; fi
echo "G-TGTDIR GREEN (gates read cargo's target dir; G-LINT skips target roots)"
exit 0
