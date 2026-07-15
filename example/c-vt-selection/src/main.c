#include <assert.h>
#include <stdio.h>
#include <string.h>
#include <rustty/vt.h>

//! [selection-main]
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
  RusttyFormatterTerminalOptions opts = RUSTTY_INIT_SIZED(RusttyFormatterTerminalOptions);
  opts.emit = RUSTTY_FORMATTER_FORMAT_PLAIN;
  opts.trim = true;
  opts.selection = selection;

  RusttyFormatter formatter;
  RusttyResult result = rustty_formatter_terminal_new(
      NULL, &formatter, terminal, opts);
  assert(result == RUSTTY_SUCCESS);

  uint8_t *buf = NULL;
  size_t len = 0;
  result = rustty_formatter_format_alloc(formatter, NULL, &buf, &len);
  assert(result == RUSTTY_SUCCESS);

  printf("%s: ", label);
  fwrite(buf, 1, len, stdout);
  printf("\n");

  rustty_free(NULL, buf, len);
  rustty_formatter_free(formatter);
}

int main() {
  RusttyTerminal terminal;
  RusttyTerminalOptions opts = {
    .cols = 80,
    .rows = 8,
    .max_scrollback = 0,
  };
  RusttyResult result = rustty_terminal_new(NULL, &terminal, opts);
  assert(result == RUSTTY_SUCCESS);

  // A realistic shell transcript with OSC 133 semantic prompt markers.
  // Rustty uses these markers to distinguish prompt/input from command
  // output for semantic line and output selections.
  vt_write(terminal,
      "\033]133;A\007$ "           // Prompt starts: "$ "
      "\033]133;B\007git status"  // Input starts: "git status"
      "\033]133;C\007\r\n"        // Output starts after Enter
      "On branch main\r\n"
      "nothing to commit, working tree clean");

  RusttySelection selection = RUSTTY_INIT_SIZED(RusttySelection);

  // Double-click style word selection under the cursor.
  RusttyTerminalSelectWordOptions word = RUSTTY_INIT_SIZED(RusttyTerminalSelectWordOptions);
  word.ref = ref_at(terminal, 6, 0); // the "status" in "git status"
  result = rustty_terminal_select_word(terminal, &word, &selection);
  assert(result == RUSTTY_SUCCESS);
  print_selection(terminal, "word", &selection);

  //! [selection-word-between]
  // Double-click-and-drag style selection. Suppose the user double-clicks
  // "git" and drags to "status". The pointer may pass over whitespace, so
  // select the nearest word between the original click and current drag point
  // in both directions, then combine the outer word bounds.
  RusttyGridRef click_ref = ref_at(terminal, 2, 0); // the "git" in "git status"
  RusttyGridRef drag_ref = ref_at(terminal, 6, 0);  // the "status" in "git status"

  RusttyTerminalSelectWordBetweenOptions start_word_opts =
      RUSTTY_INIT_SIZED(RusttyTerminalSelectWordBetweenOptions);
  start_word_opts.start = click_ref;
  start_word_opts.end = drag_ref;

  RusttySelection start_word = RUSTTY_INIT_SIZED(RusttySelection);
  result = rustty_terminal_select_word_between(
      terminal, &start_word_opts, &start_word);
  assert(result == RUSTTY_SUCCESS);

  RusttyTerminalSelectWordBetweenOptions end_word_opts =
      RUSTTY_INIT_SIZED(RusttyTerminalSelectWordBetweenOptions);
  end_word_opts.start = drag_ref;
  end_word_opts.end = click_ref;

  RusttySelection end_word = RUSTTY_INIT_SIZED(RusttySelection);
  result = rustty_terminal_select_word_between(
      terminal, &end_word_opts, &end_word);
  assert(result == RUSTTY_SUCCESS);

  RusttySelection drag_selection = RUSTTY_INIT_SIZED(RusttySelection);
  drag_selection.start = start_word.start;
  drag_selection.end = end_word.end;
  print_selection(terminal, "double-click drag", &drag_selection);
  //! [selection-word-between]

  // Triple-click style line selection. With semantic prompt boundaries enabled,
  // this selects only the input area rather than the leading "$ " prompt.
  RusttyTerminalSelectLineOptions line = RUSTTY_INIT_SIZED(RusttyTerminalSelectLineOptions);
  line.ref = ref_at(terminal, 2, 0); // the "git status" input area
  line.semantic_prompt_boundary = true;
  result = rustty_terminal_select_line(terminal, &line, &selection);
  assert(result == RUSTTY_SUCCESS);
  print_selection(terminal, "line", &selection);

  // Select exactly the command output for the command under the cursor.
  result = rustty_terminal_select_output(
      terminal, ref_at(terminal, 0, 1), &selection);
  assert(result == RUSTTY_SUCCESS);
  print_selection(terminal, "output", &selection);

  // Select all visible content.
  result = rustty_terminal_select_all(terminal, &selection);
  assert(result == RUSTTY_SUCCESS);
  print_selection(terminal, "all", &selection);

  rustty_terminal_free(terminal);
  return 0;
}
//! [selection-main]
