/*
 * Audit-only compatibility shim for .port/phase-0/harness/ref_driver/vt_driver.c.
 *
 * The checked-in driver still uses the pre-03d5fa268 constructor shape:
 *   ghostty_terminal_new(allocator, terminal, GhosttyTerminalOptions)
 * Current Ghostty and Rustty both use:
 *   *_terminal_new(allocator, terminal, cols, rows)
 *
 * Include this header before the driver (`clang -include ...`) to prove that
 * the differential comparison can still be run after adapting only that stale
 * call boundary. The differential fixtures always pass scrollback=0, so the
 * removed max_scrollback field is intentionally ignored here. This is not a
 * production compatibility layer or a proposed fix.
 */
#ifndef RUSTTY_CODEX_VT_DRIVER_API_SHIM_H
#define RUSTTY_CODEX_VT_DRIVER_API_SHIM_H

#include <ghostty/vt.h>

typedef struct GhosttyTerminalOptions {
  uint16_t cols;
  uint16_t rows;
  size_t max_scrollback;
} GhosttyTerminalOptions;

static inline GhosttyResult rustty_codex_terminal_new(
    const GhosttyAllocator *allocator,
    GhosttyTerminal *terminal,
    GhosttyTerminalOptions options) {
  (void)options.max_scrollback;
  return ghostty_terminal_new(allocator, terminal, options.cols, options.rows);
}

#define ghostty_terminal_new(allocator, terminal, options) \
  rustty_codex_terminal_new((allocator), (terminal), (options))

#endif
