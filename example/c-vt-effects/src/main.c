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

//! [effects-clipboard-write]
RusttyClipboardWriteResult on_clipboard_write(
    RusttyTerminal terminal,
    void* userdata,
    const RusttyClipboardWrite* write) {
  (void)terminal;
  (void)userdata;

  printf("  clipboard write (location=%d, contents=%zu)\n",
         (int)write->location, write->contents_len);
  if (write->contents_len == 0) {
    printf("    clear\n");
  }

  for (size_t i = 0; i < write->contents_len; i++) {
    const RusttyClipboardContent* content = &write->contents[i];
    printf("    ");
    if (content->mime.len > 0) {
      fwrite(content->mime.ptr, 1, content->mime.len, stdout);
    }
    printf(" (%zu bytes): ", content->data.len);
    if (content->data.len > 0) {
      fwrite(content->data.ptr, 1, content->data.len, stdout);
    }
    printf("\n");
  }

  return RUSTTY_CLIPBOARD_WRITE_RESULT_SUCCESS;
}
//! [effects-clipboard-write]

//! [effects-desktop-notification]
void on_desktop_notification(
    RusttyTerminal terminal,
    void* userdata,
    const RusttyTerminalDesktopNotification* notification) {
  (void)terminal;
  (void)userdata;

  // Sized-struct contract: only read fields present within `size`. Asserting
  // the exact size here is what proves the library and this header agree —
  // a by-value ABI mismatch shows up as a wrong `size`, not as a crash.
  if (notification->size != sizeof(RusttyTerminalDesktopNotification)) {
    fprintf(stderr, "  desktop notification: BAD size %zu (expected %zu)\n",
            notification->size, sizeof(RusttyTerminalDesktopNotification));
    return;
  }

  printf("  desktop notification (size=%zu, title=%zu, body=%zu)\n",
         notification->size, notification->title.len, notification->body.len);
  printf("    title: ");
  if (notification->title.len > 0) {
    fwrite(notification->title.ptr, 1, notification->title.len, stdout);
  }
  printf("\n    body: ");
  if (notification->body.len > 0) {
    fwrite(notification->body.ptr, 1, notification->body.len, stdout);
  }
  printf("\n");
}
//! [effects-desktop-notification]

//! [effects-progress-report]
void on_progress_report(RusttyTerminal terminal,
                        void* userdata,
                        const RusttyTerminalProgressReport* report) {
  (void)terminal;
  (void)userdata;

  if (report->size != sizeof(RusttyTerminalProgressReport)) {
    fprintf(stderr, "  progress report: BAD size %zu (expected %zu)\n",
            report->size, sizeof(RusttyTerminalProgressReport));
    return;
  }

  // `progress` is -1 when the running program omitted the percentage.
  printf("  progress report (size=%zu, state=%d, progress=%d)\n",
         report->size, (int)report->state, (int)report->progress);
}
//! [effects-progress-report]

//! [effects-register]
int main() {
  // Create a terminal
  RusttyTerminal terminal = NULL;
  if (rustty_terminal_new(NULL, &terminal, 80, 24) != RUSTTY_SUCCESS) {
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
  rustty_terminal_set(terminal, RUSTTY_TERMINAL_OPT_CLIPBOARD_WRITE,
      (const void *)on_clipboard_write);
  rustty_terminal_set(terminal, RUSTTY_TERMINAL_OPT_DESKTOP_NOTIFICATION,
      (const void *)on_desktop_notification);
  rustty_terminal_set(terminal, RUSTTY_TERMINAL_OPT_PROGRESS_REPORT,
      (const void *)on_progress_report);

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

  // 4. Clipboard write (OSC 52 ; c ; <base64 data> ST)
  printf("Sending clipboard write:\n");
  const char* clipboard_seq =
      "\x1B]52;c;SGVsbG8gY2xpcGJvYXJk\x1B\\";
  rustty_terminal_vt_write(terminal, (const uint8_t*)clipboard_seq,
                            strlen(clipboard_seq));

  // 5. Desktop notification (OSC 777 ; notify ; <title> ; <body> BEL).
  //    OSC 9 is the title-less form.
  printf("Sending desktop notifications:\n");
  const char* osc9 = "\x1B]9;Build complete\x1B\\";
  rustty_terminal_vt_write(terminal, (const uint8_t*)osc9, strlen(osc9));
  const char* osc777 = "\x1B]777;notify;Rustty;Needs attention\x07";
  rustty_terminal_vt_write(terminal, (const uint8_t*)osc777, strlen(osc777));

  // 6. Progress report (OSC 9 ; 4 ; <state> ; <percent> ST). The second one
  //    omits the percentage, which the callback sees as -1.
  printf("Sending progress reports:\n");
  const char* progress_set = "\x1B]9;4;1;42\x1B\\";
  rustty_terminal_vt_write(terminal, (const uint8_t*)progress_set,
                            strlen(progress_set));
  const char* progress_indeterminate = "\x1B]9;4;3\x1B\\";
  rustty_terminal_vt_write(terminal, (const uint8_t*)progress_indeterminate,
                            strlen(progress_indeterminate));

  // 7. Another bell to show the counter increments
  printf("Sending another BEL:\n");
  rustty_terminal_vt_write(terminal, &bel, 1);

  printf("Total bells: %d\n", bell_count);

  rustty_terminal_free(terminal);
  return 0;
}
//! [effects-register]
