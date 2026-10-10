/* P8-W5 T4 / DECISIONS P1: multi-byte UTF-8 through rustty_vt_c.dll.
 *
 * `c-vt-effects` writes only ASCII, but the failure that
 * `src/lib/windows_dll.zig` works around happens on the first multi-byte
 * UTF-8 write (simdutf's static kernels unconstructed in a Windows DLL).
 * This program writes 2-, 3- and 4-byte sequences (and a wide cluster) and
 * reads the cursor back, so a crash or a wrong column shows up here. */
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <rustty/vt.h>

int main(void) {
  RusttyTerminal terminal = NULL;
  if (rustty_terminal_new(NULL, &terminal, 80, 24) != RUSTTY_SUCCESS) {
    fprintf(stderr, "terminal_new failed\n");
    return 1;
  }
  /* h, e-acute (2 bytes), l, l, o, space, three CJK (3 bytes each, wide),
   * space, emoji (4 bytes, wide) = 6 + 6 + 1 + 2 = 15 columns. */
  const char* text =
      "h\xC3\xA9llo \xE6\x97\xA5\xE6\x9C\xAC\xE8\xAA\x9E \xF0\x9F\x98\x80";
  rustty_terminal_vt_write(terminal, (const uint8_t*)text, strlen(text));
  uint16_t col = 0;
  if (rustty_terminal_get(terminal, RUSTTY_TERMINAL_DATA_CURSOR_X, &col) !=
      RUSTTY_SUCCESS) {
    fprintf(stderr, "cursor_x failed\n");
    return 1;
  }
  printf("utf-8 write ok, cursor_x=%u (expected 15)\n", (unsigned)col);
  rustty_terminal_free(terminal);
  return col == 15 ? 0 : 1;
}
