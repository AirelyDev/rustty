#include <stddef.h>
#include <stdio.h>
#include <string.h>
#include <rustty/vt.h>

int main() {
  RusttyOscParser parser;
  if (rustty_osc_new(NULL, &parser) != RUSTTY_SUCCESS) {
    return 1;
  }
  
  // Setup change window title command to change the title to "hello"
  rustty_osc_next(parser, '0');
  rustty_osc_next(parser, ';');
  const char *title = "hello";
  for (size_t i = 0; i < strlen(title); i++) {
    rustty_osc_next(parser, title[i]);
  }
  
  // End parsing and get command
  RusttyOscCommand command = rustty_osc_end(parser, 0);
  
  // Get and print command type
  RusttyOscCommandType type = rustty_osc_command_type(command);
  printf("Command type: %d\n", type);
  
  // Extract and print the title
  if (rustty_osc_command_data(command, RUSTTY_OSC_DATA_CHANGE_WINDOW_TITLE_STR, &title)) {
    printf("Extracted title: %s\n", title);
  } else {
    printf("Failed to extract title\n");
  }
  
  rustty_osc_free(parser);
  return 0;
}
