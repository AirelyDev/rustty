#!/bin/bash
# WS9 acceptance gate — prove the macOS app is SELF-CONTAINED for gettext.
#
# INTEGRATION_PARITY §2.3 required that the shipped archive stop depending on
# Homebrew's libintl. This gate proves it the only way that cannot be faked:
# it makes Homebrew's libintl UNLINKABLE, then does a full clean build of
# Rustty.app and asserts the gettext symbols still resolve from rustty's own
# vendored archive.
#
# How the "hide" works (and why it is surgical):
#   /opt/homebrew/opt/gettext        -> ../Cellar/gettext/<v>
#   /opt/homebrew/lib/libintl.dylib  -> ../Cellar/gettext/<v>/lib/libintl.dylib
#   /opt/homebrew/lib/libintl.a      -> ../Cellar/gettext/<v>/lib/libintl.a
# Renaming the two Cellar files dangles every one of those paths at once, so
# `-lintl` cannot resolve anywhere on the host. `libintl.8.dylib` is left in
# place because Homebrew's `msgfmt` loads it by ABSOLUTE path — and `msgfmt` is
# a legitimate BUILD-TIME TOOL dependency (`xtask resources` compiles po/*.po;
# the oracle's GhosttyI18n.zig needs it too). WS9's claim is about what the
# shipped binary LINKS, not about which tools build it.
#
# The renames are reverted by an EXIT trap, so ^C or a mid-build failure still
# leaves the host exactly as it was.
#
# Usage:  bash .port/tools/gates/no_homebrew_gettext_gate.sh
# Exit 0 = self-contained. Any non-zero = the gate failed; read the message.
set -uo pipefail

ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"
cd "$ROOT"

fail() { echo "GATE FAIL: $*" >&2; exit 1; }
ok()   { echo "  ok: $*"; }

# ---------------------------------------------------------------- 0. static
echo "== 0/5 static checks: no source references Homebrew gettext for LINKING =="
# Only EFFECTIVE flags count, so strip `#` / `//` comment lines first (both files
# retain explanatory comments that mention `-lintl` by name on purpose).
strip_comments() { sed -e 's/[[:space:]]*#.*$//' -e 's|[[:space:]]*//.*$||' "$1"; }
for f in macos/build-rustty.sh crates/*/build.rs crates-sys/*/build.rs; do
  if strip_comments "$f" | grep -q -- "-lintl\|rustc-link-lib=dylib=intl\|opt/gettext/lib"; then
    fail "$f still passes a Homebrew gettext link flag"
  fi
done
ok "no -lintl link flag in any build script or in build-rustty.sh"

# --------------------------------------------------------------- 1. locate
echo "== 1/5 locating Homebrew's libintl =="
CELLAR_LIB=""
for prefix in /opt/homebrew /usr/local; do
  cand="$prefix/opt/gettext/lib"
  if [ -e "$cand/libintl.dylib" ] || [ -e "$cand/libintl.a" ]; then
    # Resolve through the `opt` symlink to the real Cellar directory so the
    # rename touches the file, not a symlink chain.
    CELLAR_LIB="$(cd "$cand" && pwd -P)"
    break
  fi
done
if [ -z "$CELLAR_LIB" ]; then
  echo "  note: no Homebrew gettext found on this host — the build is already"
  echo "        running in the hermetic condition this gate creates."
else
  ok "found Homebrew libintl at $CELLAR_LIB"
fi

# ------------------------------------------------------------------ 2. hide
HIDDEN=()
restore() {
  for f in "${HIDDEN[@]:-}"; do
    [ -n "$f" ] || continue
    if [ -e "$f.ws9-hidden" ] || [ -L "$f.ws9-hidden" ]; then
      mv "$f.ws9-hidden" "$f" && echo "  restored $f"
    fi
  done
}
trap restore EXIT

echo "== 2/5 hiding Homebrew's linker-facing libintl =="
if [ -n "$CELLAR_LIB" ]; then
  for f in "$CELLAR_LIB/libintl.dylib" "$CELLAR_LIB/libintl.a"; do
    if [ -e "$f" ] || [ -L "$f" ]; then
      mv "$f" "$f.ws9-hidden" || fail "could not rename $f (is $CELLAR_LIB writable?)"
      HIDDEN+=("$f")
      echo "  hid $f"
    fi
  done
  # Prove the hide actually took: a trivial -lintl link must now FAIL.
  probe="$(mktemp -d)"
  printf 'int main(void){return 0;}\n' > "$probe/p.c"
  if clang -o "$probe/p" "$probe/p.c" -L/opt/homebrew/lib -L"$CELLAR_LIB" -lintl 2>/dev/null; then
    rm -rf "$probe"
    fail "-lintl STILL links after hiding; the gate would prove nothing"
  fi
  rm -rf "$probe"
  ok "-lintl is now unresolvable host-wide"
  # msgfmt must survive — it is a build-time tool, not a link dependency.
  command -v msgfmt >/dev/null 2>&1 || PATH="/opt/homebrew/opt/gettext/bin:$PATH"
  msgfmt --version >/dev/null 2>&1 || fail "msgfmt broke; the hide was too broad"
  ok "msgfmt still runs (build-time tool preserved)"
fi

# ----------------------------------------------------------------- 3. build
echo "== 3/5 clean build of Rustty.app with libintl hidden =="
rm -rf target/aarch64-apple-darwin/xcframework
if ! bash macos/build-rustty.sh > /tmp/ws9-gate-build.log 2>&1; then
  tail -40 /tmp/ws9-gate-build.log >&2
  fail "build-rustty.sh failed without Homebrew gettext (see /tmp/ws9-gate-build.log)"
fi
grep -q "BUILD SUCCEEDED" /tmp/ws9-gate-build.log \
  || fail "xcodebuild did not report BUILD SUCCEEDED (see /tmp/ws9-gate-build.log)"
ok "build-rustty.sh succeeded with no Homebrew libintl available"

# --------------------------------------------------------------- 4. archive
echo "== 4/5 symbol assertions on the combined archive =="
LIB="target/aarch64-apple-darwin/xcframework/libRusttyKit.a"
[ -f "$LIB" ] || fail "combined archive missing at $LIB"
# Capture nm's output ONCE into files rather than piping it into `grep -q`:
# under `set -o pipefail`, `grep -q` exits at the first match and SIGPIPEs nm,
# so the pipeline reports failure even when the symbol IS present.
NM_DEFINED="$(mktemp)"; NM_UNDEF="$(mktemp)"
nm -g "$LIB" 2>/dev/null > "$NM_DEFINED"
nm -u "$LIB" 2>/dev/null > "$NM_UNDEF"
for sym in _dgettext _bindtextdomain _textdomain _deflate; do
  grep -q " T $sym\$" "$NM_DEFINED" || fail "$LIB does not DEFINE $sym"
  ok "$LIB defines $sym"
done
# NOTE on the undefined check: `nm -u` on a STATIC ARCHIVE reports each member's
# undefined symbols separately, including references that another member of the
# SAME archive satisfies (every zlib caller lists `_deflate` as U). So a bare
# `nm -u | grep` can never be empty and would prove nothing. The meaningful
# condition — "this archive needs the symbol from OUTSIDE itself" — is
# referenced-somewhere AND defined-nowhere, which is what we assert.
for sym in _dgettext _bindtextdomain _textdomain _deflate; do
  if grep -qx "$sym" "$NM_UNDEF" && ! grep -q " T $sym\$" "$NM_DEFINED"; then
    fail "$LIB references $sym but no member defines it"
  fi
done
ok "no externally-unsatisfied _dgettext/_bindtextdomain/_textdomain/_deflate"
rm -f "$NM_DEFINED" "$NM_UNDEF"
AR_TOC="$(mktemp)"
ar -t "$LIB" > "$AR_TOC"
head -1 "$AR_TOC" | grep -q "__.SYMDEF" \
  || fail "$LIB has no symbol table (ranlib normalization did not run)"
rm -f "$AR_TOC"
ok "combined archive carries a symbol table"

# ------------------------------------------------------------------ 5. app
echo "== 5/5 symbol assertions on the linked app binary =="
BUILT="$(xcodebuild -project macos/Rustty.xcodeproj -scheme Rustty -configuration Debug \
  -showBuildSettings 2>/dev/null | awk -F' = ' '/ BUILT_PRODUCTS_DIR /{print $2; exit}')"
MACOS_DIR="$BUILT/Rustty.app/Contents/MacOS"
BIN="$MACOS_DIR/rustty"
[ -x "$BIN" ] || fail "app binary not found at $BIN"

# In Debug, Xcode's "debug dylib" mode makes `rustty` a ~40KB STUB that merely
# loads @rpath/rustty.debug.dylib — the ~40MB image that actually contains
# libRusttyKit.a. Asserting only on the stub would pass vacuously: the stub
# links nothing but libSystem no matter what libintl does. So assert on EVERY
# Mach-O image the app ships, and separately require that whichever image
# carries the archive genuinely DEFINES the gettext symbols.
IMAGES=("$BIN")
[ -f "$MACOS_DIR/rustty.debug.dylib" ] && IMAGES+=("$MACOS_DIR/rustty.debug.dylib")

# Same SIGPIPE/pipefail caution as section 4: capture, then grep.
DEFINED_SOMEWHERE=0
for img in "${IMAGES[@]}"; do
  name="$(basename "$img")"
  OTOOL_L="$(mktemp)"; BIN_UNDEF="$(mktemp)"; BIN_DEF="$(mktemp)"
  otool -L "$img" > "$OTOOL_L"
  nm -u "$img" 2>/dev/null > "$BIN_UNDEF"
  nm "$img" 2>/dev/null > "$BIN_DEF"
  grep -qi "intl" "$OTOOL_L" && fail "$img links a libintl dylib"
  ok "$name links no libintl dylib"
  grep -qi "homebrew\|/usr/local/opt" "$OTOOL_L" && fail "$img links a Homebrew dylib"
  ok "$name links no Homebrew dylib"
  for sym in _dgettext _bindtextdomain _textdomain; do
    grep -qx "$sym" "$BIN_UNDEF" && fail "$img has undefined $sym"
  done
  ok "$name has no undefined gettext symbols"
  if grep -qE " [Tt] _dgettext\$" "$BIN_DEF" \
     && grep -qE " [Tt] _bindtextdomain\$" "$BIN_DEF" \
     && grep -qE " [Tt] _textdomain\$" "$BIN_DEF"; then
    DEFINED_SOMEWHERE=1
    ok "$name DEFINES _dgettext/_bindtextdomain/_textdomain (vendored libintl is in the image)"
  fi
  rm -f "$OTOOL_L" "$BIN_UNDEF" "$BIN_DEF"
done
[ "$DEFINED_SOMEWHERE" -eq 1 ] \
  || fail "no shipped image DEFINES the gettext symbols — the assertions above were vacuous"

echo
echo "GATE PASS: Rustty.app builds and links with NO Homebrew gettext available."
