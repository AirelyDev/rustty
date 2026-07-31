#include <assert.h>
#include <stdbool.h>
#include <stdio.h>
#include <string.h>
#include <rustty/vt.h>

//! [selection-gesture-main]
static void vt_write(RusttyTerminal terminal, const char *s) {
  rustty_terminal_vt_write(terminal, (const uint8_t *)s, strlen(s));
}

static RusttyGridRef ref_at(RusttyTerminal terminal, uint16_t x, uint16_t y) {
  RusttyGridRef ref = RUSTTY_INIT_SIZED(RusttyGridRef);
  RusttyPoint point = {
    .tag = RUSTTY_POINT_TAG_ACTIVE,
    .value = { .coordinate = { .x = x, .y = y } },
  };

  RusttyResult result = rustty_terminal_grid_ref(terminal, point, &ref);
  assert(result == RUSTTY_SUCCESS);
  return ref;
}

static void print_selection(
    RusttyTerminal terminal,
    const char *label,
    const RusttySelection *selection) {
  RusttyTerminalSelectionFormatOptions opts =
      RUSTTY_INIT_SIZED(RusttyTerminalSelectionFormatOptions);
  opts.emit = RUSTTY_FORMATTER_FORMAT_PLAIN;
  opts.trim = true;
  opts.selection = selection;

  uint8_t *buf = NULL;
  size_t len = 0;
  RusttyResult result = rustty_terminal_selection_format_alloc(
      terminal, NULL, opts, &buf, &len);
  assert(result == RUSTTY_SUCCESS);

  printf("%s: ", label);
  fwrite(buf, 1, len, stdout);
  printf("\n");

  rustty_free(NULL, buf, len);
}

static RusttySelectionGestureEvent new_event(
    RusttySelectionGestureEventType type) {
  RusttySelectionGestureEvent event = NULL;
  RusttyResult result = rustty_selection_gesture_event_new(NULL, &event, type);
  assert(result == RUSTTY_SUCCESS);
  return event;
}

int main() {
  RusttyTerminal terminal;
  RusttyResult result = rustty_terminal_new(NULL, &terminal, 20, 4);
  assert(result == RUSTTY_SUCCESS);

  vt_write(terminal, "hello world\r\nsecond line");

  RusttySelectionGesture gesture = NULL;
  result = rustty_selection_gesture_new(NULL, &gesture);
  assert(result == RUSTTY_SUCCESS);

  RusttySelectionGestureEvent press =
      new_event(RUSTTY_SELECTION_GESTURE_EVENT_TYPE_PRESS);
  RusttySelectionGestureEvent drag =
      new_event(RUSTTY_SELECTION_GESTURE_EVENT_TYPE_DRAG);
  RusttySelectionGestureEvent release =
      new_event(RUSTTY_SELECTION_GESTURE_EVENT_TYPE_RELEASE);
  RusttySelectionGestureEvent deep_press =
      new_event(RUSTTY_SELECTION_GESTURE_EVENT_TYPE_DEEP_PRESS);

  RusttySelectionGestureGeometry geometry = {
    .columns = 20,
    .cell_width = 10,
    .padding_left = 0,
    .screen_height = 40,
  };

  // Press in the first cell. A normal single press records the click anchor but
  // doesn't produce a selection yet, so we discard the optional output.
  RusttyGridRef press_ref = ref_at(terminal, 0, 0);
  result = rustty_selection_gesture_event_set(
      press, RUSTTY_SELECTION_GESTURE_EVENT_OPT_REF, &press_ref);
  assert(result == RUSTTY_SUCCESS);

  RusttySurfacePosition press_pos = { .x = 2, .y = 8 };
  result = rustty_selection_gesture_event_set(
      press, RUSTTY_SELECTION_GESTURE_EVENT_OPT_POSITION, &press_pos);
  assert(result == RUSTTY_SUCCESS);

  result = rustty_selection_gesture_event(
      gesture, terminal, press, NULL);
  assert(result == RUSTTY_NO_VALUE);

  // Drag across "hello". The drag event returns a selection snapshot that the
  // embedder can apply to its UI, copy, or format immediately.
  RusttyGridRef drag_ref = ref_at(terminal, 4, 0);
  result = rustty_selection_gesture_event_set(
      drag, RUSTTY_SELECTION_GESTURE_EVENT_OPT_REF, &drag_ref);
  assert(result == RUSTTY_SUCCESS);

  RusttySurfacePosition drag_pos = { .x = 46, .y = 8 };
  result = rustty_selection_gesture_event_set(
      drag, RUSTTY_SELECTION_GESTURE_EVENT_OPT_POSITION, &drag_pos);
  assert(result == RUSTTY_SUCCESS);

  result = rustty_selection_gesture_event_set(
      drag, RUSTTY_SELECTION_GESTURE_EVENT_OPT_GEOMETRY, &geometry);
  assert(result == RUSTTY_SUCCESS);

  RusttySelection selection = RUSTTY_INIT_SIZED(RusttySelection);
  result = rustty_selection_gesture_event(
      gesture, terminal, drag, &selection);
  assert(result == RUSTTY_SUCCESS);
  print_selection(terminal, "drag", &selection);

  // Release updates gesture state but never produces a selection.
  result = rustty_selection_gesture_event_set(
      release, RUSTTY_SELECTION_GESTURE_EVENT_OPT_REF, &drag_ref);
  assert(result == RUSTTY_SUCCESS);
  result = rustty_selection_gesture_event(
      gesture, terminal, release, NULL);
  assert(result == RUSTTY_NO_VALUE);

  bool dragged = false;
  result = rustty_selection_gesture_get(
      gesture, terminal, RUSTTY_SELECTION_GESTURE_DATA_DRAGGED, &dragged);
  assert(result == RUSTTY_SUCCESS);
  printf("dragged: %s\n", dragged ? "true" : "false");

  // Deep press uses the active click anchor to select the surrounding word.
  rustty_selection_gesture_reset(gesture, terminal);
  RusttyGridRef world_ref = ref_at(terminal, 6, 0);
  result = rustty_selection_gesture_event_set(
      press, RUSTTY_SELECTION_GESTURE_EVENT_OPT_REF, &world_ref);
  assert(result == RUSTTY_SUCCESS);
  result = rustty_selection_gesture_event(
      gesture, terminal, press, NULL);
  assert(result == RUSTTY_NO_VALUE);

  result = rustty_selection_gesture_event(
      gesture, terminal, deep_press, &selection);
  assert(result == RUSTTY_SUCCESS);
  print_selection(terminal, "deep press", &selection);

  rustty_selection_gesture_event_free(deep_press);
  rustty_selection_gesture_event_free(release);
  rustty_selection_gesture_event_free(drag);
  rustty_selection_gesture_event_free(press);
  rustty_selection_gesture_free(gesture, terminal);
  rustty_terminal_free(terminal);
  return 0;
}
//! [selection-gesture-main]
