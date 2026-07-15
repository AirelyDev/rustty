#include <stdio.h>
#include <rustty/vt.h>

//! [focus-encode]
int main() {
  char buf[8];
  size_t written = 0;

  RusttyResult result = rustty_focus_encode(
      RUSTTY_FOCUS_GAINED, buf, sizeof(buf), &written);

  if (result == RUSTTY_SUCCESS) {
    printf("Encoded %zu bytes: ", written);
    fwrite(buf, 1, written, stdout);
    printf("\n");
  }

  return 0;
}
//! [focus-encode]
