#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <rustty/vt.h>

//! [effects-write-pty]
void on_write_pty(RusttyTerminal terminal,
                  void* userdata,
                  const uint8_t* data,
                  size_t len) {
  (void)terminal;
  (void)userdata;
  printf("  write_pty (%zu bytes): ", len);
  fwrite(data, 1, len, stdout);
  printf("\n");
}
//! [effects-write-pty]

//! [effects-bell]
void on_bell(RusttyTerminal terminal, void* userdata) {
  (void)terminal;
  int* count = (int*)userdata;
  (*count)++;
  printf("  bell! (count=%d)\n", *count);
}
//! [effects-bell]

//! [effects-title-changed]
void on_title_changed(RusttyTerminal terminal, void* userdata) {
  (void)userdata;
  // Query the cursor position to confirm the terminal processed the
  // title change (the title itself is tracked by the embedder via the
  // OSC parser or its own state).
  uint16_t col = 0;
  rustty_terminal_get(terminal, RUSTTY_TERMINAL_DATA_CURSOR_X, &col);
  printf("  title changed (cursor at col %u)\n", col);
}
//! [effects-title-changed]

//! [effects-register]
int main() {
  // Create a terminal
  RusttyTerminal terminal = NULL;
  RusttyTerminalOptions opts = {
    .cols = 80,
    .rows = 24,
    .max_scrollback = 0,
  };
  if (rustty_terminal_new(NULL, &terminal, opts) != RUSTTY_SUCCESS) {
    fprintf(stderr, "Failed to create terminal\n");
    return 1;
  }

  // Set up userdata — a simple bell counter
  int bell_count = 0;
  rustty_terminal_set(terminal, RUSTTY_TERMINAL_OPT_USERDATA, &bell_count);

  // Register effect callbacks
  rustty_terminal_set(terminal, RUSTTY_TERMINAL_OPT_WRITE_PTY,
      (const void *)on_write_pty);
  rustty_terminal_set(terminal, RUSTTY_TERMINAL_OPT_BELL,
      (const void *)on_bell);
  rustty_terminal_set(terminal, RUSTTY_TERMINAL_OPT_TITLE_CHANGED,
      (const void *)on_title_changed);

  // Feed VT data that triggers effects:

  // 1. Bell (BEL = 0x07)
  printf("Sending BEL:\n");
  const uint8_t bel = 0x07;
  rustty_terminal_vt_write(terminal, &bel, 1);

  // 2. Title change (OSC 2 ; <title> ST)
  printf("Sending title change:\n");
  const char* title_seq = "\x1B]2;Hello Effects\x1B\\";
  rustty_terminal_vt_write(terminal, (const uint8_t*)title_seq,
                            strlen(title_seq));

  // 3. Device status report (DECRQM for wraparound mode ?7)
  //    triggers write_pty with the response
  printf("Sending DECRQM query:\n");
  const char* decrqm = "\x1B[?7$p";
  rustty_terminal_vt_write(terminal, (const uint8_t*)decrqm,
                            strlen(decrqm));

  // 4. Another bell to show the counter increments
  printf("Sending another BEL:\n");
  rustty_terminal_vt_write(terminal, &bel, 1);

  printf("Total bells: %d\n", bell_count);

  rustty_terminal_free(terminal);
  return 0;
}
//! [effects-register]
