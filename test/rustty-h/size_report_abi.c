/* Pins the by-value `SizeReportSize` ABI of `rustty_size_report_encode` as
 * declared by the generated full header `include/rustty.h` (NEW-FINDINGS
 * N529). cbindgen once declared that function over `mouse_encode::Size`
 * (size, screen_*, cell_*, padding_*) because two rustty-vt-c types shared the
 * name `Size`; a caller built from `rustty.h` then passed the wrong struct.
 * The initializer below names the REAL fields, so the old header fails to
 * compile, and the encoded bytes prove the library reads the same layout.
 *
 * Expected output is the reference's `encode_mode_2048` test vector
 * (src/terminal/c/size_report.zig): rows 24, columns 80, 9x18 cells.
 */
#include <rustty.h>
#include <stdio.h>
#include <string.h>

int main(void) {
  SizeReportSize size = {.rows = 24, .columns = 80, .cell_width = 9, .cell_height = 18};
  uint8_t buf[64];
  uintptr_t written = 0;
  /* style 0 == RUSTTY_SIZE_REPORT_STYLE_MODE_2048 */
  int rc = rustty_size_report_encode(0, size, buf, sizeof buf, &written);
  const char expect[] = "\x1B[48;24;80;432;720t";
  if (rc != 0) {
    fprintf(stderr, "rustty_size_report_encode returned %d\n", rc);
    return 1;
  }
  if (written != sizeof expect - 1 || memcmp(buf, expect, written) != 0) {
    fprintf(stderr, "unexpected encoding (%zu bytes)\n", (size_t)written);
    return 1;
  }
  return 0;
}
