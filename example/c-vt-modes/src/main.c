#include <stdio.h>
#include <rustty/vt.h>

//! [modes-pack-unpack]
void modes_example() {
  // Create a mode for DEC mode 25 (cursor visible)
  RusttyMode tag = rustty_mode_new(25, false);
  printf("value=%u ansi=%d packed=0x%04x\n",
      rustty_mode_value(tag),
      rustty_mode_ansi(tag),
      tag);

  // Create a mode for ANSI mode 4 (insert mode)
  RusttyMode ansi_tag = rustty_mode_new(4, true);
  printf("value=%u ansi=%d packed=0x%04x\n",
      rustty_mode_value(ansi_tag),
      rustty_mode_ansi(ansi_tag),
      ansi_tag);
}
//! [modes-pack-unpack]

//! [modes-decrpm]
void decrpm_example() {
  char buf[32];
  size_t written = 0;

  // Encode a report that DEC mode 25 (cursor visible) is set
  RusttyResult result = rustty_mode_report_encode(
      RUSTTY_MODE_CURSOR_VISIBLE,
      RUSTTY_MODE_REPORT_SET,
      buf, sizeof(buf), &written);

  if (result == RUSTTY_SUCCESS) {
    printf("Encoded %zu bytes: ", written);
    fwrite(buf, 1, written, stdout);
    printf("\n");  // prints: ESC[?25;1$y
  }
}
//! [modes-decrpm]

int main() {
  modes_example();
  decrpm_example();
  return 0;
}
