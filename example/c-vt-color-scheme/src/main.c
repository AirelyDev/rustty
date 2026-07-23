#include <stdio.h>
#include <rustty/vt.h>

//! [color-scheme-report-encode]
int main() {
  char buf[16];
  size_t written = 0;

  RusttyResult result = rustty_color_scheme_report_encode(
      RUSTTY_COLOR_SCHEME_DARK, buf, sizeof(buf), &written);

  if (result == RUSTTY_SUCCESS) {
    printf("Encoded %zu bytes: ", written);
    fwrite(buf, 1, written, stdout);
    printf("\n");
  }

  return 0;
}
//! [color-scheme-report-encode]
