#include <assert.h>
#include <stdio.h>
#include <string.h>
#include <rustty/vt.h>

int main(void) {
  //! [vt-stream-init]
  // Create a terminal
  RusttyTerminal terminal;
  RusttyResult result = rustty_terminal_new(NULL, &terminal, 80, 24);
  assert(result == RUSTTY_SUCCESS);
  //! [vt-stream-init]

  //! [vt-stream-write]
  // Feed VT data into the terminal
  const char *text = "Hello, World!\r\n";
  rustty_terminal_vt_write(terminal, (const uint8_t *)text, strlen(text));

  // ANSI color codes: ESC[1;32m = bold green, ESC[0m = reset
  text = "\x1b[1;32mGreen Text\x1b[0m\r\n";
  rustty_terminal_vt_write(terminal, (const uint8_t *)text, strlen(text));

  // Cursor positioning: ESC[1;1H = move to row 1, column 1
  text = "\x1b[1;1HTop-left corner\r\n";
  rustty_terminal_vt_write(terminal, (const uint8_t *)text, strlen(text));

  // Cursor movement: ESC[5B = move down 5 lines
  text = "\x1b[5B";
  rustty_terminal_vt_write(terminal, (const uint8_t *)text, strlen(text));
  text = "Moved down!\r\n";
  rustty_terminal_vt_write(terminal, (const uint8_t *)text, strlen(text));

  // Erase line: ESC[2K = clear entire line
  text = "\x1b[2K";
  rustty_terminal_vt_write(terminal, (const uint8_t *)text, strlen(text));
  text = "New content\r\n";
  rustty_terminal_vt_write(terminal, (const uint8_t *)text, strlen(text));

  // Multiple lines
  text = "Line A\r\nLine B\r\nLine C\r\n";
  rustty_terminal_vt_write(terminal, (const uint8_t *)text, strlen(text));
  //! [vt-stream-write]

  //! [vt-stream-read]
  // Get the final terminal state as a plain string using the formatter
  RusttyFormatterTerminalOptions fmt_opts =
      RUSTTY_INIT_SIZED(RusttyFormatterTerminalOptions);
  fmt_opts.emit = RUSTTY_FORMATTER_FORMAT_PLAIN;
  fmt_opts.trim = true;

  RusttyFormatter formatter;
  result = rustty_formatter_terminal_new(NULL, &formatter, terminal, fmt_opts);
  assert(result == RUSTTY_SUCCESS);

  uint8_t *buf = NULL;
  size_t len = 0;
  result = rustty_formatter_format_alloc(formatter, NULL, &buf, &len);
  assert(result == RUSTTY_SUCCESS);

  fwrite(buf, 1, len, stdout);
  printf("\n");

  rustty_free(NULL, buf, len);
  rustty_formatter_free(formatter);
  //! [vt-stream-read]

  rustty_terminal_free(terminal);
  return 0;
}
