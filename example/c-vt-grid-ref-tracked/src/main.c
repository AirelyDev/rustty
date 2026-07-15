#include <assert.h>
#include <stdbool.h>
#include <stdio.h>
#include <string.h>
#include <rustty/vt.h>

//! [grid-ref-tracked]
static uint32_t codepoint_at_tracked_ref(RusttyTrackedGridRef tracked) {
  RusttyGridRef snapshot = RUSTTY_INIT_SIZED(RusttyGridRef);
  RusttyResult result = rustty_tracked_grid_ref_snapshot(tracked, &snapshot);
  assert(result == RUSTTY_SUCCESS);

  RusttyCell cell;
  result = rustty_grid_ref_cell(&snapshot, &cell);
  assert(result == RUSTTY_SUCCESS);

  bool has_text = false;
  rustty_cell_get(cell, RUSTTY_CELL_DATA_HAS_TEXT, &has_text);
  assert(has_text);

  uint32_t codepoint = 0;
  rustty_cell_get(cell, RUSTTY_CELL_DATA_CODEPOINT, &codepoint);
  return codepoint;
}

int main() {
  RusttyTerminal terminal;
  RusttyTerminalOptions opts = {
    .cols = 8,
    .rows = 3,
    .max_scrollback = 100,
  };
  RusttyResult result = rustty_terminal_new(NULL, &terminal, opts);
  assert(result == RUSTTY_SUCCESS);

  const char *text = "alpha\r\n"
                     "bravo\r\n"
                     "charlie";
  rustty_terminal_vt_write(
      terminal, (const uint8_t *)text, strlen(text));

  RusttyTrackedGridRef tracked = NULL;
  RusttyPoint alpha = {
    .tag = RUSTTY_POINT_TAG_ACTIVE,
    .value = { .coordinate = { .x = 0, .y = 0 } },
  };
  result = rustty_terminal_grid_ref_track(terminal, alpha, &tracked);
  assert(result == RUSTTY_SUCCESS);

  // Writing another line scrolls the original "alpha" row into scrollback.
  // The tracked ref still follows the same cell.
  const char *more = "\r\ndelta";
  rustty_terminal_vt_write(
      terminal, (const uint8_t *)more, strlen(more));

  assert(rustty_tracked_grid_ref_has_value(tracked));
  printf("tracked codepoint after scroll: %c\n",
      (char)codepoint_at_tracked_ref(tracked));

  RusttyPointCoordinate screen = {0};
  result = rustty_tracked_grid_ref_point(
      tracked, RUSTTY_POINT_TAG_SCREEN, &screen);
  assert(result == RUSTTY_SUCCESS);
  printf("tracked screen point: %u,%u\n", screen.x, screen.y);

  // Resetting the terminal discards the old grid contents. The tracked
  // handle remains valid, but no longer has a meaningful location.
  rustty_terminal_reset(terminal);
  assert(!rustty_tracked_grid_ref_has_value(tracked));

  RusttyGridRef discarded = RUSTTY_INIT_SIZED(RusttyGridRef);
  result = rustty_tracked_grid_ref_snapshot(tracked, &discarded);
  assert(result == RUSTTY_NO_VALUE);

  // The same handle can be moved to a new point after it loses its value.
  const char *replacement = "echo";
  rustty_terminal_vt_write(
      terminal, (const uint8_t *)replacement, strlen(replacement));

  RusttyPoint echo = {
    .tag = RUSTTY_POINT_TAG_ACTIVE,
    .value = { .coordinate = { .x = 0, .y = 0 } },
  };
  result = rustty_tracked_grid_ref_set(tracked, terminal, echo);
  assert(result == RUSTTY_SUCCESS);
  assert(rustty_tracked_grid_ref_has_value(tracked));
  printf("tracked codepoint after reset/set: %c\n",
      (char)codepoint_at_tracked_ref(tracked));

  rustty_tracked_grid_ref_free(tracked);
  rustty_terminal_free(terminal);
  return 0;
}
//! [grid-ref-tracked]
