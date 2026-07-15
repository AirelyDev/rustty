#include <assert.h>
#include <stdbool.h>
#include <stdio.h>
#include <string.h>
#include <rustty/vt.h>

/// Helper: resolve a style color to an RGB value using the palette.
static RusttyColorRgb resolve_color(RusttyStyleColor color,
                                     const RusttyRenderStateColors* colors,
                                     RusttyColorRgb fallback) {
  switch (color.tag) {
    case RUSTTY_STYLE_COLOR_RGB:
      return color.value.rgb;
    case RUSTTY_STYLE_COLOR_PALETTE:
      return colors->palette[color.value.palette];
    default:
      return fallback;
  }
}

int main(void) {
  RusttyResult result;

  //! [render-state-update]
  // Create a terminal and render state, then update the render state
  // from the terminal. The render state captures a snapshot of everything
  // needed to draw a frame.
  RusttyTerminal terminal = NULL;
  RusttyTerminalOptions terminal_opts = {
      .cols = 40,
      .rows = 5,
      .max_scrollback = 10000,
  };
  result = rustty_terminal_new(NULL, &terminal, terminal_opts);
  assert(result == RUSTTY_SUCCESS);

  RusttyRenderState render_state = NULL;
  result = rustty_render_state_new(NULL, &render_state);
  assert(result == RUSTTY_SUCCESS);

  // Feed some styled content into the terminal.
  const char* content =
      "Hello, \033[1;32mworld\033[0m!\r\n"     // bold green "world"
      "\033[4munderlined\033[0m text\r\n"       // underlined text
      "\033[38;2;255;128;0morange\033[0m\r\n";  // 24-bit orange fg
  rustty_terminal_vt_write(
      terminal, (const uint8_t*)content, strlen(content));

  // Select "underlined" on the second row. Render state exposes this
  // later as a row-local selected cell range.
  RusttyGridRef selection_start = RUSTTY_INIT_SIZED(RusttyGridRef);
  RusttyPoint selection_start_pt = {
      .tag = RUSTTY_POINT_TAG_ACTIVE,
      .value = { .coordinate = { .x = 0, .y = 1 } },
  };
  result = rustty_terminal_grid_ref(
      terminal, selection_start_pt, &selection_start);
  assert(result == RUSTTY_SUCCESS);

  RusttyGridRef selection_end = RUSTTY_INIT_SIZED(RusttyGridRef);
  RusttyPoint selection_end_pt = {
      .tag = RUSTTY_POINT_TAG_ACTIVE,
      .value = { .coordinate = { .x = 9, .y = 1 } },
  };
  result = rustty_terminal_grid_ref(terminal, selection_end_pt, &selection_end);
  assert(result == RUSTTY_SUCCESS);

  RusttySelection selection = RUSTTY_INIT_SIZED(RusttySelection);
  selection.start = selection_start;
  selection.end = selection_end;
  result = rustty_terminal_set(
      terminal, RUSTTY_TERMINAL_OPT_SELECTION, &selection);
  assert(result == RUSTTY_SUCCESS);

  result = rustty_render_state_update(render_state, terminal);
  assert(result == RUSTTY_SUCCESS);
  //! [render-state-update]

  //! [render-dirty-check]
  // Check the global dirty state to decide how much work the renderer
  // needs to do. After rendering, reset it to false.
  RusttyRenderStateDirty dirty;
  result = rustty_render_state_get(
      render_state, RUSTTY_RENDER_STATE_DATA_DIRTY, &dirty);
  assert(result == RUSTTY_SUCCESS);

  switch (dirty) {
    case RUSTTY_RENDER_STATE_DIRTY_FALSE:
      printf("Frame is clean, nothing to draw.\n");
      break;
    case RUSTTY_RENDER_STATE_DIRTY_PARTIAL:
      printf("Partial redraw needed.\n");
      break;
    case RUSTTY_RENDER_STATE_DIRTY_FULL:
      printf("Full redraw needed.\n");
      break;
  }
  //! [render-dirty-check]

  //! [render-colors]
  // Retrieve colors (background, foreground, palette) from the render
  // state. These are needed to resolve palette-indexed cell colors.
  RusttyRenderStateColors colors =
      RUSTTY_INIT_SIZED(RusttyRenderStateColors);
  result = rustty_render_state_colors_get(render_state, &colors);
  assert(result == RUSTTY_SUCCESS);

  printf("Background: #%02x%02x%02x\n",
         colors.background.r, colors.background.g, colors.background.b);
  printf("Foreground: #%02x%02x%02x\n",
         colors.foreground.r, colors.foreground.g, colors.foreground.b);
  //! [render-colors]

  //! [render-cursor]
  // Read cursor position and visual style from the render state.
  bool cursor_visible = false;
  rustty_render_state_get(
      render_state, RUSTTY_RENDER_STATE_DATA_CURSOR_VISIBLE,
      &cursor_visible);

  bool cursor_in_viewport = false;
  rustty_render_state_get(
      render_state, RUSTTY_RENDER_STATE_DATA_CURSOR_VIEWPORT_HAS_VALUE,
      &cursor_in_viewport);

  if (cursor_visible && cursor_in_viewport) {
    uint16_t cx, cy;
    rustty_render_state_get(
        render_state, RUSTTY_RENDER_STATE_DATA_CURSOR_VIEWPORT_X, &cx);
    rustty_render_state_get(
        render_state, RUSTTY_RENDER_STATE_DATA_CURSOR_VIEWPORT_Y, &cy);

    RusttyRenderStateCursorVisualStyle style;
    rustty_render_state_get(
        render_state, RUSTTY_RENDER_STATE_DATA_CURSOR_VISUAL_STYLE,
        &style);

    const char* style_name = "unknown";
    switch (style) {
      case RUSTTY_RENDER_STATE_CURSOR_VISUAL_STYLE_BAR:
        style_name = "bar";
        break;
      case RUSTTY_RENDER_STATE_CURSOR_VISUAL_STYLE_BLOCK:
        style_name = "block";
        break;
      case RUSTTY_RENDER_STATE_CURSOR_VISUAL_STYLE_UNDERLINE:
        style_name = "underline";
        break;
      case RUSTTY_RENDER_STATE_CURSOR_VISUAL_STYLE_BLOCK_HOLLOW:
        style_name = "hollow";
        break;
    }
    printf("Cursor at (%u, %u), style: %s\n", cx, cy, style_name);
  }
  //! [render-cursor]

  //! [render-row-iterate]
  // Iterate rows via the row iterator. For each dirty row, iterate its
  // cells, read codepoints/graphemes and styles, and emit ANSI-colored
  // output as a simple "renderer".
  RusttyRenderStateRowIterator row_iter = NULL;
  result = rustty_render_state_row_iterator_new(NULL, &row_iter);
  assert(result == RUSTTY_SUCCESS);

  result = rustty_render_state_get(
      render_state, RUSTTY_RENDER_STATE_DATA_ROW_ITERATOR, &row_iter);
  assert(result == RUSTTY_SUCCESS);

  RusttyRenderStateRowCells cells = NULL;
  result = rustty_render_state_row_cells_new(NULL, &cells);
  assert(result == RUSTTY_SUCCESS);

  int row_index = 0;
  while (rustty_render_state_row_iterator_next(row_iter)) {
    // Check per-row dirty state; a real renderer would skip clean rows.
    bool row_dirty = false;
    rustty_render_state_row_get(
        row_iter, RUSTTY_RENDER_STATE_ROW_DATA_DIRTY, &row_dirty);

    printf("Row %2d [%s]: ", row_index,
           row_dirty ? "dirty" : "clean");

    // Query the row-local selection range. Rows without a selection return
    // RUSTTY_NO_VALUE; selected rows return inclusive start/end columns.
    RusttyRenderStateRowSelection row_selection =
        RUSTTY_INIT_SIZED(RusttyRenderStateRowSelection);
    result = rustty_render_state_row_get(
        row_iter, RUSTTY_RENDER_STATE_ROW_DATA_SELECTION, &row_selection);
    assert(result == RUSTTY_SUCCESS || result == RUSTTY_NO_VALUE);
    if (result == RUSTTY_SUCCESS) {
      printf("selection=%u..%u ",
             row_selection.start_x, row_selection.end_x);
    }

    // Get cells for this row (reuses the same cells handle).
    result = rustty_render_state_row_get(
        row_iter, RUSTTY_RENDER_STATE_ROW_DATA_CELLS, &cells);
    assert(result == RUSTTY_SUCCESS);

    while (rustty_render_state_row_cells_next(cells)) {
      // Get the grapheme length; 0 means the cell is empty.
      uint32_t grapheme_len = 0;
      rustty_render_state_row_cells_get(
          cells, RUSTTY_RENDER_STATE_ROW_CELLS_DATA_GRAPHEMES_LEN,
          &grapheme_len);

      if (grapheme_len == 0) {
        putchar(' ');
        continue;
      }

      // Read the style for this cell. Returns the default style for
      // cells that have no explicit styling.
      RusttyStyle style = RUSTTY_INIT_SIZED(RusttyStyle);
      rustty_render_state_row_cells_get(
          cells, RUSTTY_RENDER_STATE_ROW_CELLS_DATA_STYLE, &style);

      // Resolve foreground color for this cell.
      RusttyColorRgb fg =
          resolve_color(style.fg_color, &colors, colors.foreground);

      // Emit ANSI true-color escape for the foreground.
      printf("\033[38;2;%u;%u;%um", fg.r, fg.g, fg.b);
      if (style.bold) printf("\033[1m");
      if (style.underline) printf("\033[4m");

      // Read grapheme codepoints into a buffer and print them.
      // The buffer must be at least grapheme_len elements.
      uint32_t codepoints[16];
      uint32_t len = grapheme_len < 16 ? grapheme_len : 16;
      rustty_render_state_row_cells_get(
          cells, RUSTTY_RENDER_STATE_ROW_CELLS_DATA_GRAPHEMES_BUF,
          codepoints);

      for (uint32_t i = 0; i < len; i++) {
        // Simple ASCII print; a real renderer would handle UTF-8.
        if (codepoints[i] < 128)
          putchar((char)codepoints[i]);
        else
          printf("U+%04X", codepoints[i]);
      }

      printf("\033[0m");  // Reset style after each cell.
    }

    printf("\n");

    // Clear per-row dirty flag after "rendering" it.
    bool clean = false;
    rustty_render_state_row_set(
        row_iter, RUSTTY_RENDER_STATE_ROW_OPTION_DIRTY, &clean);

    row_index++;
  }
  //! [render-row-iterate]

  //! [render-dirty-reset]
  // After finishing the frame, reset the global dirty state so the next
  // update can report changes accurately.
  RusttyRenderStateDirty clean_state = RUSTTY_RENDER_STATE_DIRTY_FALSE;
  result = rustty_render_state_set(
      render_state, RUSTTY_RENDER_STATE_OPTION_DIRTY, &clean_state);
  assert(result == RUSTTY_SUCCESS);
  //! [render-dirty-reset]

  // Cleanup
  rustty_render_state_row_cells_free(cells);
  rustty_render_state_row_iterator_free(row_iter);
  rustty_render_state_free(render_state);
  rustty_terminal_free(terminal);
  return 0;
}
