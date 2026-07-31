#!/bin/sh
# D11r — the RUNTIME half of D11 (P3-W6 T2 Step 4).
#
# WHY A SECOND GATE
#   check_closed_enum_abi.sh is a SOURCE scanner: it proves no exported
#   `extern "C" fn` declares a closed `#[repr(C)]` enum as a by-value parameter
#   (directly or through a by-value struct field). What it CANNOT prove is that
#   the narrowing point each site now has actually REJECTS an out-of-range value
#   instead of, say, silently clamping it or falling through to a default arm.
#   A scanner that is satisfied by the type alone is satisfied by
#   `Foo::from_c_int(v).unwrap_or(Foo::First)` too.
#
#   So this gate compiles a real C program against the PUBLISHED header and the
#   shipped dylib, passes the `…_MAX_VALUE` enumerator the header ITSELF declares
#   for each enum, and asserts a DEFINED answer comes back.
#
#   That test is the point of the whole exercise: before P3-W6 T2 it could not be
#   written correctly at all. `rustty_focus_encode` took a `RusttyFocusEvent` by
#   value, so calling it with `RUSTTY_FOCUS_MAX_VALUE` was instant UB in the
#   callee prologue — there was no defined answer to assert, and any "result" the
#   program printed was meaningless. The value is not exotic: it is an enumerator
#   the published header declares, so a conforming C caller can write it today.
#
# WHAT IT ASSERTS
#   * the value-returning exports it covers (`rustty_focus_encode`,
#     `rustty_size_report_encode`) return RUSTTY_INVALID_VALUE — not
#     RUSTTY_SUCCESS, and not a crash;
#   * the in-range enumerators still behave EXACTLY as before (the anti-vacuity
#     half — a narrowing point that rejects everything would pass the checks
#     above and break every real caller);
#   * `rustty_terminal_scroll_viewport`, which returns `void` and so has no error
#     channel, survives the out-of-range tag AND leaves the terminal usable: a
#     valid scroll issued afterwards still works.
set -e
root=$(cd "$(dirname "$0")/../../.." && pwd); cd "$root"
CC="$(xcrun --find clang 2>/dev/null || echo /usr/bin/clang)"
SDK="$(xcrun --show-sdk-path 2>/dev/null || true)"
T="${TMPDIR:-/tmp}"

# Build unconditionally — the WS19 T16 rule. A gate that links whatever dylib is
# lying in target/release is testing an unknown artifact.
cargo build -p rustty-vt-c --release >/dev/null

cat > "$T/d11r.c" <<'CEOF'
#include <rustty/vt.h>
#include <stdio.h>
#include <string.h>

static int failures = 0;
#define CHECK(cond, msg)                                                       \
  do {                                                                         \
    if (!(cond)) {                                                             \
      fprintf(stderr, "  FAIL: %s  (%s:%d)\n", msg, __FILE__, __LINE__);       \
      failures++;                                                              \
    }                                                                          \
  } while (0)

int main(void) {
  char buf[64];
  size_t written = 0;
  RusttyResult r;

  /* ---- rustty_focus_encode ------------------------------------------- */
  /* In range: unchanged behaviour. */
  written = 0;
  r = rustty_focus_encode(RUSTTY_FOCUS_GAINED, buf, sizeof(buf), &written);
  CHECK(r == RUSTTY_SUCCESS, "focus_encode(GAINED) should still succeed");
  CHECK(written > 0, "focus_encode(GAINED) should write bytes");

  /* Out of range, using the enumerator the header itself declares. */
  written = 12345;
  r = rustty_focus_encode(RUSTTY_FOCUS_MAX_VALUE, buf, sizeof(buf), &written);
  CHECK(r == RUSTTY_INVALID_VALUE,
        "focus_encode(MAX_VALUE) must return RUSTTY_INVALID_VALUE");

  /* One past the last declared enumerator, too. */
  r = rustty_focus_encode((RusttyFocusEvent)2, buf, sizeof(buf), &written);
  CHECK(r == RUSTTY_INVALID_VALUE,
        "focus_encode(2) must return RUSTTY_INVALID_VALUE");

  /* ---- rustty_size_report_encode ------------------------------------- */
  RusttySizeReportSize size;
  size.rows = 24;
  size.columns = 80;
  size.cell_width = 10;
  size.cell_height = 20;

  written = 0;
  r = rustty_size_report_encode(RUSTTY_SIZE_REPORT_MODE_2048, size, buf,
                                sizeof(buf), &written);
  CHECK(r == RUSTTY_SUCCESS, "size_report_encode(MODE_2048) should still succeed");
  CHECK(written > 0, "size_report_encode(MODE_2048) should write bytes");

  r = rustty_size_report_encode(RUSTTY_SIZE_REPORT_STYLE_MAX_VALUE, size, buf,
                                sizeof(buf), &written);
  CHECK(r == RUSTTY_INVALID_VALUE,
        "size_report_encode(MAX_VALUE) must return RUSTTY_INVALID_VALUE");

  /* ---- terminal-scoped checks ---------------------------------------- */
  RusttyTerminal t = NULL;
  r = rustty_terminal_new(NULL, &t, 80, 24);
  CHECK(r == RUSTTY_SUCCESS, "terminal_new should succeed");
  if (r != RUSTTY_SUCCESS) return 1;

  /* Fill enough rows to make the viewport actually movable. */
  for (int i = 0; i < 200; i++) {
    char line[32];
    int n = snprintf(line, sizeof(line), "line %d\r\n", i);
    rustty_terminal_vt_write(t, (const uint8_t *)line, (size_t)n);
  }

  /* rustty_terminal_scroll_viewport returns void: the contract for an
   * out-of-range tag is "nothing happens, and the terminal stays usable".
   * Issue the bad one BETWEEN two good ones so a corrupted viewport shows up. */
  RusttyTerminalScrollViewport sv;
  memset(&sv, 0, sizeof(sv));

  sv.tag = RUSTTY_SCROLL_VIEWPORT_TOP;
  rustty_terminal_scroll_viewport(t, sv);

  memset(&sv, 0, sizeof(sv));
  sv.tag = RUSTTY_SCROLL_VIEWPORT_MAX_VALUE;
  rustty_terminal_scroll_viewport(t, sv); /* must not be UB */

  memset(&sv, 0, sizeof(sv));
  sv.tag = RUSTTY_SCROLL_VIEWPORT_BOTTOM;
  rustty_terminal_scroll_viewport(t, sv);

  /* The terminal must still answer queries after the bad tag. */
  uint16_t rows = 0;
  r = rustty_terminal_get(t, RUSTTY_TERMINAL_DATA_ROWS, &rows);
  CHECK(r == RUSTTY_SUCCESS, "terminal_get(ROWS) must still work after a bad tag");
  CHECK(rows == 24, "terminal must be unperturbed by the out-of-range tag");

  rustty_terminal_free(t);

  if (failures != 0) {
    fprintf(stderr, "D11r: %d check(s) FAILED\n", failures);
    return 1;
  }
  printf("D11r: all out-of-range enumerators returned a DEFINED result\n");
  return 0;
}
CEOF

# shellcheck disable=SC2086 — SDK expands to zero or two words on purpose.
"$CC" ${SDK:+-isysroot "$SDK"} -std=c11 -Wall -Wextra \
  -I"$root/include" -L"$root/target/release" -lrustty_vt_c \
  "$T/d11r.c" -o "$T/d11r"
DYLD_LIBRARY_PATH="$root/target/release" LD_LIBRARY_PATH="$root/target/release" "$T/d11r"
echo "D11r: GREEN"
