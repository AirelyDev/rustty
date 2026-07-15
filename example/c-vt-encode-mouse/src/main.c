#include <assert.h>
#include <stddef.h>
#include <stdio.h>
#include <string.h>
#include <rustty/vt.h>

//! [mouse-encode]
int main() {
  // Create encoder
  RusttyMouseEncoder encoder;
  RusttyResult result = rustty_mouse_encoder_new(NULL, &encoder);
  assert(result == RUSTTY_SUCCESS);

  // Configure SGR format with normal tracking
  rustty_mouse_encoder_setopt(encoder, RUSTTY_MOUSE_ENCODER_OPT_EVENT,
      &(RusttyMouseTrackingMode){RUSTTY_MOUSE_TRACKING_NORMAL});
  rustty_mouse_encoder_setopt(encoder, RUSTTY_MOUSE_ENCODER_OPT_FORMAT,
      &(RusttyMouseFormat){RUSTTY_MOUSE_FORMAT_SGR});

  // Set terminal geometry for coordinate mapping
  rustty_mouse_encoder_setopt(encoder, RUSTTY_MOUSE_ENCODER_OPT_SIZE,
      &(RusttyMouseEncoderSize){
          .size = sizeof(RusttyMouseEncoderSize),
          .screen_width = 800, .screen_height = 600,
          .cell_width = 10, .cell_height = 20,
      });

  // Create and configure a left button press event
  RusttyMouseEvent event;
  result = rustty_mouse_event_new(NULL, &event);
  assert(result == RUSTTY_SUCCESS);
  rustty_mouse_event_set_action(event, RUSTTY_MOUSE_ACTION_PRESS);
  rustty_mouse_event_set_button(event, RUSTTY_MOUSE_BUTTON_LEFT);
  rustty_mouse_event_set_position(event,
      (RusttyMousePosition){.x = 50.0f, .y = 40.0f});

  // Encode the mouse event
  char buf[128];
  size_t written = 0;
  result = rustty_mouse_encoder_encode(encoder, event,
      buf, sizeof(buf), &written);
  assert(result == RUSTTY_SUCCESS);

  // Use the encoded sequence (e.g., write to terminal)
  fwrite(buf, 1, written, stdout);

  // Cleanup
  rustty_mouse_event_free(event);
  rustty_mouse_encoder_free(encoder);
  return 0;
}
//! [mouse-encode]
