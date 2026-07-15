#include <cassert>
#include <cstdio>
#include <cstring>
#include <rustty/vt.h>

int main() {
  // Create a terminal
  RusttyTerminal terminal;
  RusttyTerminalOptions opts = {
    .cols = 80,
    .rows = 24,
    .max_scrollback = 0,
  };
  RusttyResult result = rustty_terminal_new(nullptr, &terminal, opts);
  assert(result == RUSTTY_SUCCESS);

  // Feed VT data into the terminal
  const char *text = "Hello from C++!\r\n";
  rustty_terminal_vt_write(terminal, reinterpret_cast<const uint8_t *>(text), std::strlen(text));

  text = "\x1b[1;32mGreen Text\x1b[0m\r\n";
  rustty_terminal_vt_write(terminal, reinterpret_cast<const uint8_t *>(text), std::strlen(text));

  text = "\x1b[1;1HTop-left corner\r\n";
  rustty_terminal_vt_write(terminal, reinterpret_cast<const uint8_t *>(text), std::strlen(text));

  // Get the final terminal state as a plain string
  RusttyFormatterTerminalOptions fmt_opts =
      RUSTTY_INIT_SIZED(RusttyFormatterTerminalOptions);
  fmt_opts.emit = RUSTTY_FORMATTER_FORMAT_PLAIN;
  fmt_opts.trim = true;

  RusttyFormatter formatter;
  result = rustty_formatter_terminal_new(nullptr, &formatter, terminal, fmt_opts);
  assert(result == RUSTTY_SUCCESS);

  uint8_t *buf = nullptr;
  size_t len = 0;
  result = rustty_formatter_format_alloc(formatter, nullptr, &buf, &len);
  assert(result == RUSTTY_SUCCESS);

  std::fwrite(buf, 1, len, stdout);
  std::printf("\n");

  rustty_free(nullptr, buf, len);
  rustty_formatter_free(formatter);
  rustty_terminal_free(terminal);
  return 0;
}
