#include <stdio.h>
#include <rustty/vt.h>

//! [size-report-encode]
int main() {
  RusttySizeReportSize size = {
    .rows = 24,
    .columns = 80,
    .cell_width = 9,
    .cell_height = 18,
  };

  char buf[64];
  size_t written = 0;

  RusttyResult result = rustty_size_report_encode(
      RUSTTY_SIZE_REPORT_MODE_2048, size, buf, sizeof(buf), &written);

  if (result == RUSTTY_SUCCESS) {
    printf("Encoded %zu bytes: ", written);
    fwrite(buf, 1, written, stdout);
    printf("\n");
  }

  return 0;
}
//! [size-report-encode]
