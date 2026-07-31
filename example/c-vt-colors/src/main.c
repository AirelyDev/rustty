#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <rustty/vt.h>

//! [colors-set-defaults]
/// Set up a dark color theme with custom palette entries.
void set_color_theme(RusttyTerminal terminal) {
  // Set default foreground (light gray) and background (dark)
  RusttyColorRgb fg = { .r = 0xDD, .g = 0xDD, .b = 0xDD };
  RusttyColorRgb bg = { .r = 0x1E, .g = 0x1E, .b = 0x2E };
  RusttyColorRgb cursor = { .r = 0xF5, .g = 0xE0, .b = 0xDC };

  rustty_terminal_set(terminal, RUSTTY_TERMINAL_OPT_COLOR_FOREGROUND, &fg);
  rustty_terminal_set(terminal, RUSTTY_TERMINAL_OPT_COLOR_BACKGROUND, &bg);
  rustty_terminal_set(terminal, RUSTTY_TERMINAL_OPT_COLOR_CURSOR, &cursor);

  // Set a custom palette — start from the built-in default and override
  // the first 8 entries with a custom dark theme.
  RusttyColorRgb palette[256];
  rustty_terminal_get(terminal, RUSTTY_TERMINAL_DATA_COLOR_PALETTE, palette);

  palette[RUSTTY_COLOR_NAMED_BLACK]   = (RusttyColorRgb){ 0x45, 0x47, 0x5A };
  palette[RUSTTY_COLOR_NAMED_RED]     = (RusttyColorRgb){ 0xF3, 0x8B, 0xA8 };
  palette[RUSTTY_COLOR_NAMED_GREEN]   = (RusttyColorRgb){ 0xA6, 0xE3, 0xA1 };
  palette[RUSTTY_COLOR_NAMED_YELLOW]  = (RusttyColorRgb){ 0xF9, 0xE2, 0xAF };
  palette[RUSTTY_COLOR_NAMED_BLUE]    = (RusttyColorRgb){ 0x89, 0xB4, 0xFA };
  palette[RUSTTY_COLOR_NAMED_MAGENTA] = (RusttyColorRgb){ 0xF5, 0xC2, 0xE7 };
  palette[RUSTTY_COLOR_NAMED_CYAN]    = (RusttyColorRgb){ 0x94, 0xE2, 0xD5 };
  palette[RUSTTY_COLOR_NAMED_WHITE]   = (RusttyColorRgb){ 0xBA, 0xC2, 0xDE };

  rustty_terminal_set(terminal, RUSTTY_TERMINAL_OPT_COLOR_PALETTE, palette);
}
//! [colors-set-defaults]

//! [colors-read]
/// Print the effective and default values for a color, showing how
/// OSC overrides layer on top of defaults.
void print_color(RusttyTerminal terminal,
                 const char* name,
                 RusttyTerminalData effective_data,
                 RusttyTerminalData default_data) {
  RusttyColorRgb color;

  RusttyResult res = rustty_terminal_get(terminal, effective_data, &color);
  if (res == RUSTTY_SUCCESS) {
    printf("  %-12s effective: #%02X%02X%02X", name, color.r, color.g, color.b);
  } else {
    printf("  %-12s effective: (not set)", name);
  }

  res = rustty_terminal_get(terminal, default_data, &color);
  if (res == RUSTTY_SUCCESS) {
    printf("  default: #%02X%02X%02X\n", color.r, color.g, color.b);
  } else {
    printf("  default: (not set)\n");
  }
}

void print_all_colors(RusttyTerminal terminal, const char* label) {
  printf("%s:\n", label);
  print_color(terminal, "foreground",
      RUSTTY_TERMINAL_DATA_COLOR_FOREGROUND,
      RUSTTY_TERMINAL_DATA_COLOR_FOREGROUND_DEFAULT);
  print_color(terminal, "background",
      RUSTTY_TERMINAL_DATA_COLOR_BACKGROUND,
      RUSTTY_TERMINAL_DATA_COLOR_BACKGROUND_DEFAULT);
  print_color(terminal, "cursor",
      RUSTTY_TERMINAL_DATA_COLOR_CURSOR,
      RUSTTY_TERMINAL_DATA_COLOR_CURSOR_DEFAULT);

  // Show palette index 0 (black) as an example
  RusttyColorRgb palette[256];
  rustty_terminal_get(terminal, RUSTTY_TERMINAL_DATA_COLOR_PALETTE, palette);
  printf("  %-12s effective: #%02X%02X%02X", "palette[0]",
      palette[0].r, palette[0].g, palette[0].b);

  rustty_terminal_get(terminal, RUSTTY_TERMINAL_DATA_COLOR_PALETTE_DEFAULT,
      palette);
  printf("  default: #%02X%02X%02X\n", palette[0].r, palette[0].g, palette[0].b);
}
//! [colors-read]

//! [colors-main]
int main() {
  // Create a terminal
  RusttyTerminal terminal = NULL;
  if (rustty_terminal_new(NULL, &terminal, 80, 24) != RUSTTY_SUCCESS) {
    fprintf(stderr, "Failed to create terminal\n");
    return 1;
  }

  // Before setting any colors, everything is unset
  print_all_colors(terminal, "Before setting defaults");

  // Set our color theme defaults
  set_color_theme(terminal);
  print_all_colors(terminal, "\nAfter setting defaults");

  // Simulate an OSC override (e.g. a program running inside the
  // terminal changes the foreground via OSC 10)
  const char* osc_fg = "\x1B]10;rgb:FF/00/00\x1B\\";
  rustty_terminal_vt_write(terminal, (const uint8_t*)osc_fg,
                            strlen(osc_fg));
  print_all_colors(terminal, "\nAfter OSC foreground override");

  // Clear the foreground default — the OSC override is still active
  rustty_terminal_set(terminal, RUSTTY_TERMINAL_OPT_COLOR_FOREGROUND, NULL);
  print_all_colors(terminal, "\nAfter clearing foreground default");

  rustty_terminal_free(terminal);
  return 0;
}
//! [colors-main]
