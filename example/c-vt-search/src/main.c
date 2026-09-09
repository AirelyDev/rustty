#include <assert.h>
#include <stdbool.h>
#include <stdio.h>
#include <string.h>
#include <rustty/vt.h>

//! [search-main]
int main() {
  // Create a terminal and fill it with some content to search.
  RusttyTerminal terminal;
  RusttyResult result = rustty_terminal_new(NULL, &terminal, 80, 24);
  assert(result == RUSTTY_SUCCESS);

  const char *lines[] = {
    "$ make test\r\n",
    "compiling module A... ok\r\n",
    "compiling module B... error: missing semicolon\r\n",
    "linking... error: undefined symbol\r\n",
    "$ grep -n ERROR build.log\r\n",
  };
  for (size_t i = 0; i < sizeof(lines) / sizeof(lines[0]); i++) {
    rustty_terminal_vt_write(terminal, (const uint8_t *)lines[i],
                              strlen(lines[i]));
  }

  // The user opened the find bar, so create a search bound to the
  // terminal. It starts idle until it has a needle.
  RusttySearch search;
  result = rustty_search_new(NULL, &search, terminal);
  assert(result == RUSTTY_SUCCESS);

  // The user typed a query. Matching is byte-exact except ASCII
  // letters, which compare case-insensitively, so "error" also finds
  // "ERROR". Retyping just sets the needle again: a changed needle
  // restarts the search and an unchanged one keeps its results.
  RusttyString needle = { (const uint8_t *)"error", 5 };
  result = rustty_search_set(search, RUSTTY_SEARCH_OPT_NEEDLE, &needle);
  assert(result == RUSTTY_SUCCESS);

  // Drive the search. Interactive embedders interleave
  // rustty_search_tick() and rustty_search_feed() with their event
  // loop, but for a one-shot search we can just run it to completion.
  result = rustty_search_run(search);
  assert(result == RUSTTY_SUCCESS);

  // The total match count, for find bar text like "1 of 3".
  size_t total = 0;
  result = rustty_search_get(search, RUSTTY_SEARCH_DATA_TOTAL_MATCHES,
                              &total);
  assert(result == RUSTTY_SUCCESS);
  printf("%zu matches for \"error\"\n", total);

  // The user pressed Enter, so select the next match. Selection starts
  // at the newest match, moves toward older content, and wraps around.
  // This scrolls the viewport to the match if it isn't visible, per
  // the RUSTTY_SEARCH_OPT_SELECT_SCROLL policy.
  while (true) {
    result = rustty_search_set(search, RUSTTY_SEARCH_OPT_SELECT_NEXT, NULL);
    if (result != RUSTTY_SUCCESS) break;

    // Read the selection state in one call. Index 0 is the newest
    // match, so a "k of n" find bar renders index + 1.
    size_t idx = 0;
    RusttySelection match = RUSTTY_INIT_SIZED(RusttySelection);
    const RusttySearchData keys[] = {
      RUSTTY_SEARCH_DATA_SELECTED_INDEX,
      RUSTTY_SEARCH_DATA_SELECTED_MATCH,
    };
    void *values[] = { &idx, &match };
    result = rustty_search_get_multi(
        search, sizeof(keys) / sizeof(keys[0]), keys, values, NULL);
    assert(result == RUSTTY_SUCCESS);
    printf("selected %zu of %zu\n", idx + 1, total);

    // Wrapped back around to the first match: stop.
    if (idx + 1 == total) break;
  }

  // Each frame while the find bar is open, feed to catch up with any
  // terminal changes and then read the viewport matches to draw
  // highlights. The list can include matches just past the viewport
  // when they share a page with it, so convert each endpoint to
  // viewport coordinates and skip matches outside the visible rows.
  result = rustty_search_feed(search);
  assert(result == RUSTTY_SUCCESS);

  RusttySelection viewport_storage[64];
  RusttySelectionBuffer viewport = {
    .ptr = viewport_storage,
    .cap = sizeof(viewport_storage) / sizeof(viewport_storage[0]),
  };
  result = rustty_search_get(search, RUSTTY_SEARCH_DATA_VIEWPORT_MATCHES,
                              &viewport);
  assert(result == RUSTTY_SUCCESS);
  for (size_t i = 0; i < viewport.len; i++) {
    RusttyPointCoordinate start, end;
    if (rustty_terminal_point_from_grid_ref(
            terminal, &viewport_storage[i].start, RUSTTY_POINT_TAG_VIEWPORT,
            &start) != RUSTTY_SUCCESS) continue;
    if (rustty_terminal_point_from_grid_ref(
            terminal, &viewport_storage[i].end, RUSTTY_POINT_TAG_VIEWPORT,
            &end) != RUSTTY_SUCCESS) continue;
    if (start.y >= 24 || end.y >= 24) continue;

    // A real embedder draws a highlight rect from start to end here.
    printf("highlight rows %u-%u, cols %u-%u\n",
           (unsigned)start.y, (unsigned)end.y,
           (unsigned)start.x, (unsigned)end.x);
  }

  // Closing the find bar. The search borrows the terminal, but the
  // two can be freed in either order.
  rustty_search_free(search);
  rustty_terminal_free(terminal);
  return 0;
}
//! [search-main]
