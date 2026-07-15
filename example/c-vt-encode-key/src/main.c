#include <assert.h>
#include <stddef.h>
#include <stdio.h>
#include <string.h>
#include <rustty/vt.h>

//! [key-encode]
int main() {
  // Create encoder
  RusttyKeyEncoder encoder;
  RusttyResult result = rustty_key_encoder_new(NULL, &encoder);
  assert(result == RUSTTY_SUCCESS);

  // Enable Kitty keyboard protocol with all features
  rustty_key_encoder_setopt(encoder, RUSTTY_KEY_ENCODER_OPT_KITTY_FLAGS,
                             &(uint8_t){RUSTTY_KITTY_KEY_ALL});

  // Create and configure key event for Ctrl+C press
  RusttyKeyEvent event;
  result = rustty_key_event_new(NULL, &event);
  assert(result == RUSTTY_SUCCESS);
  rustty_key_event_set_action(event, RUSTTY_KEY_ACTION_PRESS);
  rustty_key_event_set_key(event, RUSTTY_KEY_C);
  rustty_key_event_set_mods(event, RUSTTY_MODS_CTRL);

  // Encode the key event
  char buf[128];
  size_t written = 0;
  result = rustty_key_encoder_encode(encoder, event, buf, sizeof(buf), &written);
  assert(result == RUSTTY_SUCCESS);

  // Use the encoded sequence (e.g., write to terminal)
  fwrite(buf, 1, written, stdout);

  // Cleanup
  rustty_key_event_free(event);
  rustty_key_encoder_free(encoder);
  return 0;
}
//! [key-encode]
