/**
 * @file terminal.h
 *
 * Complete terminal emulator state and rendering.
 */

#ifndef RUSTTY_VT_TERMINAL_H
#define RUSTTY_VT_TERMINAL_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include <rustty/vt/types.h>
#include <rustty/vt/allocator.h>
#include <rustty/vt/device.h>
#include <rustty/vt/modes.h>
#include <rustty/vt/size_report.h>
#include <rustty/vt/grid_ref.h>
#include <rustty/vt/kitty_graphics.h>
#include <rustty/vt/screen.h>
#include <rustty/vt/point.h>
#include <rustty/vt/selection.h>
#include <rustty/vt/style.h>

#ifdef __cplusplus
extern "C" {
#endif

/** @defgroup terminal Terminal
 *
 * Complete terminal emulator state and rendering.
 *
 * A terminal instance manages the full emulator state including the screen,
 * scrollback, cursor, styles, modes, and VT stream processing.
 *
 * Once a terminal session is up and running, you can configure a key encoder
 * to write keyboard input via rustty_key_encoder_setopt_from_terminal().
 *
 * ### Example: VT stream processing
 * @snippet c-vt-stream/src/main.c vt-stream-init
 * @snippet c-vt-stream/src/main.c vt-stream-write
 *
 * ## Effects
 *
 * By default, the terminal sequence processing with rustty_terminal_vt_write() 
 * only process sequences that directly affect terminal state and 
 * ignores sequences that have side effect behavior or require responses.
 * These sequences include things like bell characters, title changes, device
 * attributes queries, and more. To handle these sequences, the embedder
 * must configure "effects."
 *
 * Effects are callbacks that the terminal invokes in response to VT
 * sequences processed during rustty_terminal_vt_write(). They let the
 * embedding application react to terminal-initiated events such as bell
 * characters, title changes, device status report responses, and more.
 *
 * Each effect is registered with rustty_terminal_set() using the
 * corresponding `RusttyTerminalOption` identifier. A `NULL` value
 * pointer clears the callback and disables the effect.
 *
 * A userdata pointer can be attached via `RUSTTY_TERMINAL_OPT_USERDATA`
 * and is passed to every callback, allowing callers to route events
 * back to their own application state without global variables.
 * You cannot specify different userdata for different callbacks.
 *
 * All callbacks are invoked synchronously during
 * rustty_terminal_vt_write(). Callbacks **must not** call
 * rustty_terminal_vt_write() on the same terminal (no reentrancy).
 * And callbacks must be very careful to not block for too long or perform 
 * expensive operations, since they are blocking further IO processing.
 *
 * The available effects are:
 *
 * | Option                                  | Callback Type                     | Trigger                                   |
 * |-----------------------------------------|-----------------------------------|-------------------------------------------|
 * | `RUSTTY_TERMINAL_OPT_WRITE_PTY`        | `RusttyTerminalWritePtyFn`       | Query responses written back to the pty   |
 * | `RUSTTY_TERMINAL_OPT_BELL`             | `RusttyTerminalBellFn`           | BEL character (0x07)                      |
 * | `RUSTTY_TERMINAL_OPT_TITLE_CHANGED`    | `RusttyTerminalTitleChangedFn`   | Title change via OSC 0 / OSC 2            |
 * | `RUSTTY_TERMINAL_OPT_PWD_CHANGED`      | `RusttyTerminalPwdChangedFn`     | Pwd change via OSC 7 / OSC 9 / OSC 1337   |
 * | `RUSTTY_TERMINAL_OPT_ENQUIRY`          | `RusttyTerminalEnquiryFn`        | ENQ character (0x05)                      |
 * | `RUSTTY_TERMINAL_OPT_XTVERSION`        | `RusttyTerminalXtversionFn`      | XTVERSION query (CSI > q)                 |
 * | `RUSTTY_TERMINAL_OPT_SIZE`             | `RusttyTerminalSizeFn`           | XTWINOPS size query (CSI 14/16/18 t)      |
 * | `RUSTTY_TERMINAL_OPT_COLOR_SCHEME`     | `RusttyTerminalColorSchemeFn`    | Color scheme query (CSI ? 996 n)          |
 * | `RUSTTY_TERMINAL_OPT_DEVICE_ATTRIBUTES`| `RusttyTerminalDeviceAttributesFn`| Device attributes query (CSI c / > c / = c)|
 *
 * ### Defining a write_pty callback
 * @snippet c-vt-effects/src/main.c effects-write-pty
 *
 * ### Defining a bell callback
 * @snippet c-vt-effects/src/main.c effects-bell
 *
 * ### Defining a title_changed callback
 * @snippet c-vt-effects/src/main.c effects-title-changed
 *
 * ### Registering effects and processing VT data
 * @snippet c-vt-effects/src/main.c effects-register
 *
 * ## Color Theme
 *
 * The terminal maintains a set of colors used for rendering: a foreground
 * color, a background color, a cursor color, and a 256-color palette. Each
 * of these has two layers: a **default** value set by the embedder, and an
 * **override** value that programs running in the terminal can set via OSC
 * escape sequences (e.g. OSC 10/11/12 for foreground/background/cursor,
 * OSC 4 for individual palette entries).
 *
 * ### Default Colors
 *
 * Use rustty_terminal_set() with the color options to configure the
 * default colors. These represent the theme or configuration chosen by
 * the embedder. Passing `NULL` clears the default, leaving the color
 * unset.
 *
 * | Option                                  | Input Type              | Description                          |
 * |-----------------------------------------|-------------------------|--------------------------------------|
 * | `RUSTTY_TERMINAL_OPT_COLOR_FOREGROUND` | `RusttyColorRgb*`      | Default foreground color             |
 * | `RUSTTY_TERMINAL_OPT_COLOR_BACKGROUND` | `RusttyColorRgb*`      | Default background color             |
 * | `RUSTTY_TERMINAL_OPT_COLOR_CURSOR`     | `RusttyColorRgb*`      | Default cursor color                 |
 * | `RUSTTY_TERMINAL_OPT_COLOR_PALETTE`    | `RusttyColorRgb[256]*` | Default 256-color palette            |
 *
 * For the palette, passing `NULL` resets to the built-in default palette.
 * The palette set operation preserves any per-index OSC overrides that
 * programs have applied; only unmodified indices are updated.
 *
 * ### Reading colors
 *
 * Use rustty_terminal_get() to read colors. There are two variants for
 * each color: the **effective** value (which returns the OSC override if
 * one is active, otherwise the default) and the **default** value (which
 * ignores any OSC overrides).
 *
 * | Data                                              | Output Type             | Description                                    |
 * |---------------------------------------------------|-------------------------|------------------------------------------------|
 * | `RUSTTY_TERMINAL_DATA_COLOR_FOREGROUND`          | `RusttyColorRgb*`      | Effective foreground (override or default)      |
 * | `RUSTTY_TERMINAL_DATA_COLOR_BACKGROUND`          | `RusttyColorRgb*`      | Effective background (override or default)      |
 * | `RUSTTY_TERMINAL_DATA_COLOR_CURSOR`              | `RusttyColorRgb*`      | Effective cursor (override or default)          |
 * | `RUSTTY_TERMINAL_DATA_COLOR_PALETTE`             | `RusttyColorRgb[256]*` | Current palette (with any OSC overrides)        |
 * | `RUSTTY_TERMINAL_DATA_COLOR_FOREGROUND_DEFAULT`  | `RusttyColorRgb*`      | Default foreground only (ignores OSC override)  |
 * | `RUSTTY_TERMINAL_DATA_COLOR_BACKGROUND_DEFAULT`  | `RusttyColorRgb*`      | Default background only (ignores OSC override)  |
 * | `RUSTTY_TERMINAL_DATA_COLOR_CURSOR_DEFAULT`      | `RusttyColorRgb*`      | Default cursor only (ignores OSC override)      |
 * | `RUSTTY_TERMINAL_DATA_COLOR_PALETTE_DEFAULT`     | `RusttyColorRgb[256]*` | Default palette only (ignores OSC overrides)    |
 *
 * For foreground, background, and cursor colors, the getters return
 * `RUSTTY_NO_VALUE` if no color is configured (neither a default nor an
 * OSC override). The palette getters always succeed since the palette
 * always has a value (the built-in default if nothing else is set).
 *
 * ### Setting a color theme
 * @snippet c-vt-colors/src/main.c colors-set-defaults
 *
 * ### Reading effective and default colors
 * @snippet c-vt-colors/src/main.c colors-read
 *
 * ### Full example with OSC overrides
 * @snippet c-vt-colors/src/main.c colors-main
 *
 * @{
 */

/**
 * Terminal initialization options.
 *
 * @ingroup terminal
 */
typedef struct {
  /** Terminal width in cells. Must be greater than zero. */
  uint16_t cols;

  /** Terminal height in cells. Must be greater than zero. */
  uint16_t rows;

  /** Maximum number of lines to keep in scrollback history. */
  size_t max_scrollback;

  // TODO: Consider ABI compatibility implications of this struct.
  // We may want to artificially pad it significantly to support
  // future options.
} RusttyTerminalOptions;

/**
 * Scroll viewport behavior tag.
 *
 * @ingroup terminal
 */
typedef enum RUSTTY_ENUM_TYPED {
  /** Scroll to the top of the scrollback. */
  RUSTTY_SCROLL_VIEWPORT_TOP,

  /** Scroll to the bottom (active area). */
  RUSTTY_SCROLL_VIEWPORT_BOTTOM,

  /** Scroll by a delta amount (up is negative). */
  RUSTTY_SCROLL_VIEWPORT_DELTA,
  RUSTTY_SCROLL_VIEWPORT_MAX_VALUE = RUSTTY_ENUM_MAX_VALUE,
} RusttyTerminalScrollViewportTag;

/**
 * Scroll viewport value.
 *
 * @ingroup terminal
 */
typedef union {
  /** Scroll delta (only used with RUSTTY_SCROLL_VIEWPORT_DELTA). Up is negative. */
  intptr_t delta;

  /** Padding for ABI compatibility. Do not use. */
  uint64_t _padding[2];
} RusttyTerminalScrollViewportValue;

/**
 * Tagged union for scroll viewport behavior.
 *
 * @ingroup terminal
 */
typedef struct {
  RusttyTerminalScrollViewportTag tag;
  RusttyTerminalScrollViewportValue value;
} RusttyTerminalScrollViewport;

/**
 * Terminal screen identifier.
 *
 * Identifies which screen buffer is active in the terminal.
 *
 * @ingroup terminal
 */
typedef enum RUSTTY_ENUM_TYPED {
  /** The primary (normal) screen. */
  RUSTTY_TERMINAL_SCREEN_PRIMARY = 0,

  /** The alternate screen. */
  RUSTTY_TERMINAL_SCREEN_ALTERNATE = 1,
  RUSTTY_TERMINAL_SCREEN_MAX_VALUE = RUSTTY_ENUM_MAX_VALUE,
} RusttyTerminalScreen;

/**
 * Visual style of the terminal cursor.
 *
 * @ingroup terminal
 */
typedef enum RUSTTY_ENUM_TYPED {
  /** Bar cursor (DECSCUSR 5, 6). */
  RUSTTY_TERMINAL_CURSOR_STYLE_BAR = 0,

  /** Block cursor (DECSCUSR 1, 2). */
  RUSTTY_TERMINAL_CURSOR_STYLE_BLOCK = 1,

  /** Underline cursor (DECSCUSR 3, 4). */
  RUSTTY_TERMINAL_CURSOR_STYLE_UNDERLINE = 2,

  /** Hollow block cursor. */
  RUSTTY_TERMINAL_CURSOR_STYLE_BLOCK_HOLLOW = 3,
  RUSTTY_TERMINAL_CURSOR_STYLE_MAX_VALUE = RUSTTY_ENUM_MAX_VALUE,
} RusttyTerminalCursorStyle;

/**
 * Scrollbar state for the terminal viewport.
 *
 * Represents the scrollable area dimensions needed to render a scrollbar.
 *
 * @ingroup terminal
 */
typedef struct {
  /** Total size of the scrollable area in rows. */
  uint64_t total;

  /** Offset into the total area that the viewport is at. */
  uint64_t offset;

  /** Length of the visible area in rows. */
  uint64_t len;
} RusttyTerminalScrollbar;

/**
 * Callback function type for bell.
 *
 * Called when the terminal receives a BEL character (0x07).
 *
 * @param terminal The terminal handle
 * @param userdata The userdata pointer set via RUSTTY_TERMINAL_OPT_USERDATA
 *
 * @ingroup terminal
 */
typedef void (*RusttyTerminalBellFn)(RusttyTerminal terminal,
                                      void* userdata);

/**
 * Callback function type for color scheme queries (CSI ? 996 n).
 *
 * Called when the terminal receives a color scheme device status report
 * query. Return true and fill *out_scheme with the current color scheme,
 * or return false to silently ignore the query.
 *
 * @param terminal The terminal handle
 * @param userdata The userdata pointer set via RUSTTY_TERMINAL_OPT_USERDATA
 * @param[out] out_scheme Pointer to store the current color scheme
 * @return true if the color scheme was filled, false to ignore the query
 *
 * @ingroup terminal
 */
typedef bool (*RusttyTerminalColorSchemeFn)(RusttyTerminal terminal,
                                             void* userdata,
                                             RusttyColorScheme* out_scheme);

/**
 * Callback function type for device attributes queries (DA1/DA2/DA3).
 *
 * Called when the terminal receives a device attributes query (CSI c,
 * CSI > c, or CSI = c). Return true and fill *out_attrs with the
 * response data, or return false to silently ignore the query.
 *
 * The terminal uses whichever sub-struct (primary, secondary, tertiary)
 * matches the request type, but all three should be filled for simplicity.
 *
 * @param terminal The terminal handle
 * @param userdata The userdata pointer set via RUSTTY_TERMINAL_OPT_USERDATA
 * @param[out] out_attrs Pointer to store the device attributes response
 * @return true if attributes were filled, false to ignore the query
 *
 * @ingroup terminal
 */
typedef bool (*RusttyTerminalDeviceAttributesFn)(RusttyTerminal terminal,
                                                   void* userdata,
                                                   RusttyDeviceAttributes* out_attrs);

/**
 * Callback function type for enquiry (ENQ, 0x05).
 *
 * Called when the terminal receives an ENQ character. Return the
 * response bytes as a RusttyString. The memory must remain valid
 * until the callback returns. Return a zero-length string to send
 * no response.
 *
 * @param terminal The terminal handle
 * @param userdata The userdata pointer set via RUSTTY_TERMINAL_OPT_USERDATA
 * @return The response bytes to write back to the pty
 *
 * @ingroup terminal
 */
typedef RusttyString (*RusttyTerminalEnquiryFn)(RusttyTerminal terminal,
                                                   void* userdata);

/**
 * Callback function type for size queries (XTWINOPS).
 *
 * Called in response to XTWINOPS size queries (CSI 14/16/18 t).
 * Return true and fill *out_size with the current terminal geometry,
 * or return false to silently ignore the query.
 *
 * @param terminal The terminal handle
 * @param userdata The userdata pointer set via RUSTTY_TERMINAL_OPT_USERDATA
 * @param[out] out_size Pointer to store the terminal size information
 * @return true if size was filled, false to ignore the query
 *
 * @ingroup terminal
 */
typedef bool (*RusttyTerminalSizeFn)(RusttyTerminal terminal,
                                      void* userdata,
                                      RusttySizeReportSize* out_size);

/**
 * Callback function type for title_changed.
 *
 * Called when the terminal title changes via escape sequences
 * (e.g. OSC 0 or OSC 2). The new title can be queried from the
 * terminal after the callback returns.
 *
 * @param terminal The terminal handle
 * @param userdata The userdata pointer set via RUSTTY_TERMINAL_OPT_USERDATA
 *
 * @ingroup terminal
 */
typedef void (*RusttyTerminalTitleChangedFn)(RusttyTerminal terminal,
                                              void* userdata);

/**
 * Callback function type for pwd_changed.
 *
 * Called when the terminal pwd (current working directory) changes via
 * escape sequences: OSC 7 (file:// URI), OSC 9 (ConEmu CurrentDir), or
 * OSC 1337 CurrentDir (iTerm2). Use rustty_terminal_get() with
 * RUSTTY_TERMINAL_DATA_PWD inside the callback to read the new value.
 *
 * The terminal stores whatever bytes the shell emitted, without parsing.
 * That means for OSC 7 the value is the raw URI (typically file://...);
 * for OSC 9/OSC 1337 it is typically a bare path. The embedder is
 * responsible for decoding any URI scheme or host if it cares about them.
 *
 * The callback also fires when the shell clears the pwd (e.g. an empty
 * OSC 7). In that case RUSTTY_TERMINAL_DATA_PWD returns a zero-length
 * string.
 *
 * @param terminal The terminal handle
 * @param userdata The userdata pointer set via RUSTTY_TERMINAL_OPT_USERDATA
 *
 * @ingroup terminal
 */
typedef void (*RusttyTerminalPwdChangedFn)(RusttyTerminal terminal,
                                            void* userdata);

/**
 * Callback function type for write_pty.
 *
 * Called when the terminal needs to write data back to the pty, for
 * example in response to a device status report or mode query. The
 * data is only valid for the duration of the call; callers must copy
 * it if it needs to persist.
 *
 * @param terminal The terminal handle
 * @param userdata The userdata pointer set via RUSTTY_TERMINAL_OPT_USERDATA
 * @param data Pointer to the response bytes
 * @param len Length of the response in bytes
 *
 * @ingroup terminal
 */
typedef void (*RusttyTerminalWritePtyFn)(RusttyTerminal terminal,
                                          void* userdata,
                                          const uint8_t* data,
                                          size_t len);

/**
 * Callback function type for XTVERSION.
 *
 * Called when the terminal receives an XTVERSION query (CSI > q).
 * Return the version string (e.g. "myterm 1.0") as a RusttyString.
 * The memory must remain valid until the callback returns. Return a
 * zero-length string to report the default "librustty" version.
 *
 * @param terminal The terminal handle
 * @param userdata The userdata pointer set via RUSTTY_TERMINAL_OPT_USERDATA
 * @return The version string to report
 *
 * @ingroup terminal
 */
typedef RusttyString (*RusttyTerminalXtversionFn)(RusttyTerminal terminal,
                                                     void* userdata);

/**
 * Terminal option identifiers.
 *
 * These values are used with rustty_terminal_set() to configure
 * terminal callbacks and associated state.
 *
 * @ingroup terminal
 */
typedef enum RUSTTY_ENUM_TYPED {
  /**
   * Opaque userdata pointer passed to all callbacks.
   *
   * Input type: void*
   */
  RUSTTY_TERMINAL_OPT_USERDATA = 0,

  /**
   * Callback invoked when the terminal needs to write data back
   * to the pty (e.g. in response to a DECRQM query or device
   * status report). Set to NULL to ignore such sequences.
   *
   * Input type: RusttyTerminalWritePtyFn
   */
  RUSTTY_TERMINAL_OPT_WRITE_PTY = 1,

  /**
   * Callback invoked when the terminal receives a BEL character
   * (0x07). Set to NULL to ignore bell events.
   *
   * Input type: RusttyTerminalBellFn
   */
  RUSTTY_TERMINAL_OPT_BELL = 2,

  /**
   * Callback invoked when the terminal receives an ENQ character
   * (0x05). Set to NULL to send no response.
   *
   * Input type: RusttyTerminalEnquiryFn
   */
  RUSTTY_TERMINAL_OPT_ENQUIRY = 3,

  /**
   * Callback invoked when the terminal receives an XTVERSION query
   * (CSI > q). Set to NULL to report the default "librustty" string.
   *
   * Input type: RusttyTerminalXtversionFn
   */
  RUSTTY_TERMINAL_OPT_XTVERSION = 4,

  /**
   * Callback invoked when the terminal title changes via escape
   * sequences (e.g. OSC 0 or OSC 2). Set to NULL to ignore title
   * change events.
   *
   * Input type: RusttyTerminalTitleChangedFn
   */
  RUSTTY_TERMINAL_OPT_TITLE_CHANGED = 5,

  /**
   * Callback invoked in response to XTWINOPS size queries
   * (CSI 14/16/18 t). Set to NULL to silently ignore size queries.
   *
   * Input type: RusttyTerminalSizeFn
   */
  RUSTTY_TERMINAL_OPT_SIZE = 6,

  /**
   * Callback invoked in response to a color scheme device status
   * report query (CSI ? 996 n). Return true and fill the out pointer
   * to report the current scheme, or return false to silently ignore.
   * Set to NULL to ignore color scheme queries.
   *
   * Input type: RusttyTerminalColorSchemeFn
   */
  RUSTTY_TERMINAL_OPT_COLOR_SCHEME = 7,

  /**
   * Callback invoked in response to a device attributes query
   * (CSI c, CSI > c, or CSI = c). Return true and fill the out
   * pointer with response data, or return false to silently ignore.
   * Set to NULL to ignore device attributes queries.
   *
   * Input type: RusttyTerminalDeviceAttributesFn
   */
  RUSTTY_TERMINAL_OPT_DEVICE_ATTRIBUTES = 8,

  /**
   * Set the terminal title manually.
   *
   * The string data is copied into the terminal. A NULL value pointer
   * clears the title (equivalent to setting an empty string).
   *
   * Input type: RusttyString*
   */
  RUSTTY_TERMINAL_OPT_TITLE = 9,

  /**
   * Set the terminal working directory manually.
   *
   * The string data is copied into the terminal. A NULL value pointer
   * clears the pwd (equivalent to setting an empty string).
   *
   * Input type: RusttyString*
   */
  RUSTTY_TERMINAL_OPT_PWD = 10,

  /**
   * Set the default foreground color.
   *
   * A NULL value pointer clears the default (unset).
   *
   * Input type: RusttyColorRgb*
   */
  RUSTTY_TERMINAL_OPT_COLOR_FOREGROUND = 11,

  /**
   * Set the default background color.
   *
   * A NULL value pointer clears the default (unset).
   *
   * Input type: RusttyColorRgb*
   */
  RUSTTY_TERMINAL_OPT_COLOR_BACKGROUND = 12,

  /**
   * Set the default cursor color.
   *
   * A NULL value pointer clears the default (unset).
   *
   * Input type: RusttyColorRgb*
   */
  RUSTTY_TERMINAL_OPT_COLOR_CURSOR = 13,

  /**
   * Set the default 256-color palette.
   *
   * The value must point to an array of exactly 256 RusttyColorRgb values.
   * A NULL value pointer resets to the built-in default palette.
   *
   * Input type: RusttyColorRgb[256]*
   */
  RUSTTY_TERMINAL_OPT_COLOR_PALETTE = 14,

  /**
   * Set the Kitty image storage limit in bytes.
   *
   * Applied to all initialized screens (primary and alternate).
   * A value of zero disables the Kitty graphics protocol entirely,
   * deleting all stored images and placements. A NULL value pointer
   * is equivalent to zero (disables). Has no effect when Kitty graphics
   * are disabled at build time.
   *
   * Input type: uint64_t*
   */
  RUSTTY_TERMINAL_OPT_KITTY_IMAGE_STORAGE_LIMIT = 15,

  /**
   * Enable or disable Kitty image loading via the file medium.
   *
   * A NULL value pointer is a no-op. Has no effect when Kitty graphics
   * are disabled at build time.
   *
   * Input type: bool*
   */
  RUSTTY_TERMINAL_OPT_KITTY_IMAGE_MEDIUM_FILE = 16,

  /**
   * Enable or disable Kitty image loading via the temporary file medium.
   *
   * A NULL value pointer is a no-op. Has no effect when Kitty graphics
   * are disabled at build time.
   *
   * Input type: bool*
   */
  RUSTTY_TERMINAL_OPT_KITTY_IMAGE_MEDIUM_TEMP_FILE = 17,

  /**
   * Enable or disable Kitty image loading via the shared memory medium.
   *
   * A NULL value pointer is a no-op. Has no effect when Kitty graphics
   * are disabled at build time.
   *
   * Input type: bool*
   */
  RUSTTY_TERMINAL_OPT_KITTY_IMAGE_MEDIUM_SHARED_MEM = 18,

  /**
   * Set the maximum bytes the APC handler will buffer for all protocols.
   * This prevents malicious input from causing unbounded memory allocation.
   * A NULL value pointer removes all overrides, reverting to the built-in
   * defaults.
   *
   * Input type: size_t*
   */
  RUSTTY_TERMINAL_OPT_APC_MAX_BYTES = 19,

  /**
   * Set the maximum bytes the APC handler will buffer for Kitty graphics
   * protocol data. A NULL value pointer removes the override, reverting
   * to the built-in default.
   *
   * Input type: size_t*
   */
  RUSTTY_TERMINAL_OPT_APC_MAX_BYTES_KITTY = 20,

  /**
   * Set the active screen selection.
   *
   * The value must point to a RusttySelection whose grid references are
   * valid for this terminal's active screen at the time of the call. The
   * terminal copies the selection immediately and converts it to
   * terminal-owned tracked state, so the RusttySelection struct and its
   * untracked grid references do not need to outlive this call.
   *
   * Passing NULL clears the active screen selection.
   *
   * Input type: RusttySelection*
   */
  RUSTTY_TERMINAL_OPT_SELECTION = 21,

  /**
   * Set the default cursor style used by DECSCUSR reset (CSI 0 q).
   *
   * A NULL value pointer resets to the built-in default block cursor.
   *
   * Input type: RusttyTerminalCursorStyle*
   */
  RUSTTY_TERMINAL_OPT_DEFAULT_CURSOR_STYLE = 22,

  /**
   * Set whether the default cursor should blink when reset by DECSCUSR
   * (CSI 0 q).
   *
   * A NULL value pointer resets to the built-in default of not blinking.
   *
   * Input type: bool*
   */
  RUSTTY_TERMINAL_OPT_DEFAULT_CURSOR_BLINK = 23,

  /**
   * Enable or disable Glyph Protocol APC handling.
   *
   * When disabled, Glyph Protocol APC sequences are ignored and no
   * support/query/register/clear responses are emitted. Disabling also clears
   * the terminal session's glyph glossary. A NULL value pointer is a no-op.
   *
   * Input type: bool*
   */
  RUSTTY_TERMINAL_OPT_GLYPH_PROTOCOL = 24,

  /**
   * Callback invoked when the terminal pwd changes via escape
   * sequences (OSC 7, OSC 9, or OSC 1337 CurrentDir). Set to NULL
   * to ignore pwd change events.
   *
   * Input type: RusttyTerminalPwdChangedFn
   */
  RUSTTY_TERMINAL_OPT_PWD_CHANGED = 25,
  RUSTTY_TERMINAL_OPT_MAX_VALUE = RUSTTY_ENUM_MAX_VALUE,
} RusttyTerminalOption;

/**
 * Terminal data types.
 *
 * These values specify what type of data to extract from a terminal
 * using `rustty_terminal_get`.
 *
 * @ingroup terminal
 */
typedef enum RUSTTY_ENUM_TYPED {
  /** Invalid data type. Never results in any data extraction. */
  RUSTTY_TERMINAL_DATA_INVALID = 0,

  /**
   * Terminal width in cells.
   *
   * Output type: uint16_t *
   */
  RUSTTY_TERMINAL_DATA_COLS = 1,

  /**
   * Terminal height in cells.
   *
   * Output type: uint16_t *
   */
  RUSTTY_TERMINAL_DATA_ROWS = 2,

  /**
   * Cursor column position (0-indexed).
   *
   * Output type: uint16_t *
   */
  RUSTTY_TERMINAL_DATA_CURSOR_X = 3,

  /**
   * Cursor row position within the active area (0-indexed).
   *
   * Output type: uint16_t *
   */
  RUSTTY_TERMINAL_DATA_CURSOR_Y = 4,

  /**
   * Whether the cursor has a pending wrap (next print will soft-wrap).
   *
   * Output type: bool *
   */
  RUSTTY_TERMINAL_DATA_CURSOR_PENDING_WRAP = 5,

  /**
   * The currently active screen.
   *
   * Output type: RusttyTerminalScreen *
   */
  RUSTTY_TERMINAL_DATA_ACTIVE_SCREEN = 6,

  /**
   * Whether the cursor is visible (DEC mode 25).
   *
   * Output type: bool *
   */
  RUSTTY_TERMINAL_DATA_CURSOR_VISIBLE = 7,

  /**
   * Current Kitty keyboard protocol flags.
   *
   * Output type: RusttyKittyKeyFlags * (uint8_t *)
   */
  RUSTTY_TERMINAL_DATA_KITTY_KEYBOARD_FLAGS = 8,

  /**
   * Scrollbar state for the terminal viewport.
   *
   * This may be expensive to calculate depending on where the viewport
   * is (arbitrary pins are expensive). The caller should take care to only
   * call this as needed and not too frequently.
   *
   * Output type: RusttyTerminalScrollbar *
   */
  RUSTTY_TERMINAL_DATA_SCROLLBAR = 9,

  /**
   * The current SGR style of the cursor.
   *
   * This is the style that will be applied to newly printed characters.
   *
   * Output type: RusttyStyle *
   */
  RUSTTY_TERMINAL_DATA_CURSOR_STYLE = 10,

  /**
   * Whether any mouse tracking mode is active.
   *
   * Returns true if any of the mouse tracking modes (X10, normal, button,
   * or any-event) are enabled.
   *
   * Output type: bool *
   */
  RUSTTY_TERMINAL_DATA_MOUSE_TRACKING = 11,

  /**
   * The terminal title as set by escape sequences (e.g. OSC 0/2).
   *
   * Returns a borrowed string. The pointer is valid until the next call
   * to rustty_terminal_vt_write() or rustty_terminal_reset(). An empty
   * string (len=0) is returned when no title has been set.
   *
   * Output type: RusttyString *
   */
  RUSTTY_TERMINAL_DATA_TITLE = 12,

  /**
   * The terminal's current working directory as set by escape sequences
   * (e.g. OSC 7).
   *
   * Returns a borrowed string. The pointer is valid until the next call
   * to rustty_terminal_vt_write() or rustty_terminal_reset(). An empty
   * string (len=0) is returned when no pwd has been set.
   *
   * Output type: RusttyString *
   */
  RUSTTY_TERMINAL_DATA_PWD = 13,

  /**
   * The total number of rows in the active screen including scrollback.
   *
   * Output type: size_t *
   */
  RUSTTY_TERMINAL_DATA_TOTAL_ROWS = 14,

  /**
   * The number of scrollback rows (total rows minus viewport rows).
   *
   * Output type: size_t *
   */
  RUSTTY_TERMINAL_DATA_SCROLLBACK_ROWS = 15,

  /**
   * The total width of the terminal in pixels.
   *
   * This is cols * cell_width_px as set by rustty_terminal_resize().
   *
   * Output type: uint32_t *
   */
  RUSTTY_TERMINAL_DATA_WIDTH_PX = 16,

  /**
   * The total height of the terminal in pixels.
   *
   * This is rows * cell_height_px as set by rustty_terminal_resize().
   *
   * Output type: uint32_t *
   */
  RUSTTY_TERMINAL_DATA_HEIGHT_PX = 17,

  /**
   * The effective foreground color (override or default).
   *
   * Returns RUSTTY_NO_VALUE if no foreground color is set.
   *
   * Output type: RusttyColorRgb *
   */
  RUSTTY_TERMINAL_DATA_COLOR_FOREGROUND = 18,

  /**
   * The effective background color (override or default).
   *
   * Returns RUSTTY_NO_VALUE if no background color is set.
   *
   * Output type: RusttyColorRgb *
   */
  RUSTTY_TERMINAL_DATA_COLOR_BACKGROUND = 19,

  /**
   * The effective cursor color (override or default).
   *
   * Returns RUSTTY_NO_VALUE if no cursor color is set.
   *
   * Output type: RusttyColorRgb *
   */
  RUSTTY_TERMINAL_DATA_COLOR_CURSOR = 20,

  /**
   * The current 256-color palette.
   *
   * Output type: RusttyColorRgb[256] *
   */
  RUSTTY_TERMINAL_DATA_COLOR_PALETTE = 21,

  /**
   * The default foreground color (ignoring any OSC override).
   *
   * Returns RUSTTY_NO_VALUE if no default foreground color is set.
   *
   * Output type: RusttyColorRgb *
   */
  RUSTTY_TERMINAL_DATA_COLOR_FOREGROUND_DEFAULT = 22,

  /**
   * The default background color (ignoring any OSC override).
   *
   * Returns RUSTTY_NO_VALUE if no default background color is set.
   *
   * Output type: RusttyColorRgb *
   */
  RUSTTY_TERMINAL_DATA_COLOR_BACKGROUND_DEFAULT = 23,

  /**
   * The default cursor color (ignoring any OSC override).
   *
   * Returns RUSTTY_NO_VALUE if no default cursor color is set.
   *
   * Output type: RusttyColorRgb *
   */
  RUSTTY_TERMINAL_DATA_COLOR_CURSOR_DEFAULT = 24,

  /**
   * The default 256-color palette (ignoring any OSC overrides).
   *
   * Output type: RusttyColorRgb[256] *
   */
  RUSTTY_TERMINAL_DATA_COLOR_PALETTE_DEFAULT = 25,

  /**
   * The Kitty image storage limit in bytes for the active screen.
   *
   * A value of zero means the Kitty graphics protocol is disabled.
   * Returns RUSTTY_NO_VALUE when Kitty graphics are disabled at build time.
   *
   * Output type: uint64_t *
   */
  RUSTTY_TERMINAL_DATA_KITTY_IMAGE_STORAGE_LIMIT = 26,

  /**
   * Whether the file medium is enabled for Kitty image loading on the
   * active screen.
   *
   * Returns RUSTTY_NO_VALUE when Kitty graphics are disabled at build time.
   *
   * Output type: bool *
   */
  RUSTTY_TERMINAL_DATA_KITTY_IMAGE_MEDIUM_FILE = 27,

  /**
   * Whether the temporary file medium is enabled for Kitty image loading
   * on the active screen.
   *
   * Returns RUSTTY_NO_VALUE when Kitty graphics are disabled at build time.
   *
   * Output type: bool *
   */
  RUSTTY_TERMINAL_DATA_KITTY_IMAGE_MEDIUM_TEMP_FILE = 28,

  /**
   * Whether the shared memory medium is enabled for Kitty image loading
   * on the active screen.
   *
   * Returns RUSTTY_NO_VALUE when Kitty graphics are disabled at build time.
   *
   * Output type: bool *
   */
  RUSTTY_TERMINAL_DATA_KITTY_IMAGE_MEDIUM_SHARED_MEM = 29,

  /**
   * The Kitty graphics image storage for the active screen.
   *
   * Returns a borrowed pointer to the image storage. The pointer is valid
   * until the next mutating terminal call (e.g. rustty_terminal_vt_write()
   * or rustty_terminal_reset()).
   *
   * Returns RUSTTY_NO_VALUE when Kitty graphics are disabled at build time.
   *
   * Output type: RusttyKittyGraphics *
   */
  RUSTTY_TERMINAL_DATA_KITTY_GRAPHICS = 30,

  /**
   * The active screen's current selection.
   *
   * On success, writes an untracked snapshot of the terminal-owned selection
   * to the caller-provided RusttySelection. The RusttySelection struct is
   * caller-owned and may be kept, but the grid references inside it are
   * untracked borrowed references into the active screen. They are only valid
   * until the next mutating terminal call, such as rustty_terminal_set(),
   * rustty_terminal_vt_write(), rustty_terminal_resize(), or
   * rustty_terminal_reset().
   *
   * Returns RUSTTY_NO_VALUE when there is no active selection.
   *
   * Output type: RusttySelection *
   */
  RUSTTY_TERMINAL_DATA_SELECTION = 31,

  /**
   * Whether the viewport is currently pinned to the active area.
   *
   * This is true when the viewport is following the active terminal area,
   * and false when the user has scrolled into history.
   *
   * Output type: bool *
   */
  RUSTTY_TERMINAL_DATA_VIEWPORT_ACTIVE = 32,
  RUSTTY_TERMINAL_DATA_MAX_VALUE = RUSTTY_ENUM_MAX_VALUE,
} RusttyTerminalData;

/**
 * Create a new terminal instance.
 *
 * @param allocator Pointer to allocator, or NULL to use the default allocator
 * @param terminal Pointer to store the created terminal handle
 * @param options Terminal initialization options
 * @return RUSTTY_SUCCESS on success, or an error code on failure
 *
 * @ingroup terminal
 */
RUSTTY_API RusttyResult rustty_terminal_new(const RusttyAllocator* allocator,
                                   RusttyTerminal* terminal,
                                   RusttyTerminalOptions options);

/**
 * Free a terminal instance.
 *
 * Releases all resources associated with the terminal. After this call,
 * the terminal handle becomes invalid and must not be used.
 *
 * @param terminal The terminal handle to free (may be NULL)
 *
 * @ingroup terminal
 */
RUSTTY_API void rustty_terminal_free(RusttyTerminal terminal);

/**
 * Perform a full reset of the terminal (RIS).
 *
 * Resets all terminal state back to its initial configuration, including
 * modes, scrollback, scrolling region, and screen contents. The terminal
 * dimensions are preserved.
 *
 * @param terminal The terminal handle (may be NULL, in which case this is a no-op)
 *
 * @ingroup terminal
 */
RUSTTY_API void rustty_terminal_reset(RusttyTerminal terminal);

/**
 * Resize the terminal to the given dimensions.
 *
 * Changes the number of columns and rows in the terminal. The primary
 * screen will reflow content if wraparound mode is enabled; the alternate
 * screen does not reflow. If the dimensions are unchanged, this is a no-op.
 *
 * This also updates the terminal's pixel dimensions (used for image
 * protocols and size reports), disables synchronized output mode (allowed
 * by the spec so that resize results are shown immediately), and sends an
 * in-band size report if mode 2048 is enabled.
 *
 * @param terminal The terminal handle (NULL returns RUSTTY_INVALID_VALUE)
 * @param cols New width in cells (must be greater than zero)
 * @param rows New height in cells (must be greater than zero)
 * @param cell_width_px Width of a single cell in pixels
 * @param cell_height_px Height of a single cell in pixels
 * @return RUSTTY_SUCCESS on success, or an error code on failure
 *
 * @ingroup terminal
 */
RUSTTY_API RusttyResult rustty_terminal_resize(RusttyTerminal terminal,
                                      uint16_t cols,
                                      uint16_t rows,
                                      uint32_t cell_width_px,
                                      uint32_t cell_height_px);

/**
 * Set an option on the terminal.
 *
 * Configures terminal callbacks and associated state such as the
 * write_pty callback and userdata pointer. The value is passed
 * directly for pointer types (callbacks, userdata) or as a pointer
 * to the value for non-pointer types (e.g. RusttyString*).
 * NULL clears the option to its default.
 *
 * Callbacks are invoked synchronously during rustty_terminal_vt_write().
 * Callbacks must not call rustty_terminal_vt_write() on the same
 * terminal (no reentrancy).
 *
 * @param terminal The terminal handle (may be NULL, in which case this is a no-op)
 * @param option The option to set
 * @param value Pointer to the value to set (type depends on the option),
 *              or NULL to clear the option
 *
 * @ingroup terminal
 */
RUSTTY_API RusttyResult rustty_terminal_set(RusttyTerminal terminal,
                                   RusttyTerminalOption option,
                                   const void* value);

/**
 * Write VT-encoded data to the terminal for processing.
 *
 * Feeds raw bytes through the terminal's VT stream parser, updating
 * terminal state accordingly. By default, sequences that require output
 * (queries, device status reports) are silently ignored. Use
 * rustty_terminal_set() with RUSTTY_TERMINAL_OPT_WRITE_PTY to install
 * a callback that receives response data.
 *
 * This never fails. Any erroneous input or errors in processing the
 * input are logged internally but do not cause this function to fail
 * because this input is assumed to be untrusted and from an external
 * source; so the primary goal is to keep the terminal state consistent and 
 * not allow malformed input to corrupt or crash.
 *
 * @param terminal The terminal handle
 * @param data Pointer to the data to write
 * @param len Length of the data in bytes
 *
 * @ingroup terminal
 */
RUSTTY_API void rustty_terminal_vt_write(RusttyTerminal terminal,
                                const uint8_t* data,
                                size_t len);

/**
 * Scroll the terminal viewport.
 *
 * Scrolls the terminal's viewport according to the given behavior.
 * When using RUSTTY_SCROLL_VIEWPORT_DELTA, set the delta field in
 * the value union to specify the number of rows to scroll (negative
 * for up, positive for down). For other behaviors, the value is ignored.
 *
 * @param terminal The terminal handle (may be NULL, in which case this is a no-op)
 * @param behavior The scroll behavior as a tagged union
 *
 * @ingroup terminal
 */
RUSTTY_API void rustty_terminal_scroll_viewport(RusttyTerminal terminal,
                                       RusttyTerminalScrollViewport behavior);

/**
 * Get the current value of a terminal mode.
 *
 * Returns the value of the mode identified by the given mode.
 *
 * @param terminal The terminal handle (NULL returns RUSTTY_INVALID_VALUE)
 * @param mode The mode identifying the mode to query
 * @param[out] out_value On success, set to true if the mode is set, false
 *             if it is reset
 * @return RUSTTY_SUCCESS on success, RUSTTY_INVALID_VALUE if the terminal
 *         is NULL or the mode does not correspond to a known mode
 *
 * @ingroup terminal
 */
RUSTTY_API RusttyResult rustty_terminal_mode_get(RusttyTerminal terminal,
                                        RusttyMode mode,
                                        bool* out_value);

/**
 * Set the value of a terminal mode.
 *
 * Sets the mode identified by the given mode to the specified value.
 *
 * @param terminal The terminal handle (NULL returns RUSTTY_INVALID_VALUE)
 * @param mode The mode identifying the mode to set
 * @param value true to set the mode, false to reset it
 * @return RUSTTY_SUCCESS on success, RUSTTY_INVALID_VALUE if the terminal
 *         is NULL or the mode does not correspond to a known mode
 *
 * @ingroup terminal
 */
RUSTTY_API RusttyResult rustty_terminal_mode_set(RusttyTerminal terminal,
                                         RusttyMode mode,
                                         bool value);

/**
 * Get data from a terminal instance.
 *
 * Extracts typed data from the given terminal based on the specified
 * data type. The output pointer must be of the appropriate type for the
 * requested data kind. Valid data types and output types are documented
 * in the `RusttyTerminalData` enum.
 *
 * @param terminal The terminal handle (may be NULL)
 * @param data The type of data to extract
 * @param out Pointer to store the extracted data (type depends on data parameter)
 * @return RUSTTY_SUCCESS on success, RUSTTY_INVALID_VALUE if the terminal
 *         is NULL or the data type is invalid
 *
 * @ingroup terminal
 */
RUSTTY_API RusttyResult rustty_terminal_get(RusttyTerminal terminal,
                                    RusttyTerminalData data,
                                    void *out);

/**
 * Get multiple data fields from a terminal in a single call.
 *
 * This is an optimization over calling rustty_terminal_get()
 * repeatedly, particularly useful in environments with high per-call
 * overhead such as FFI or Cgo.
 *
 * Each element in the keys array specifies a data kind, and the
 * corresponding element in the values array receives the result.
 * The type of each values[i] pointer must match the output type
 * documented for keys[i].
 *
 * Processing stops at the first error; on success out_written
 * is set to count, on error it is set to the index of the
 * failing key (i.e. the number of values successfully written).
 *
 * @param terminal The terminal handle (may be NULL)
 * @param count Number of key/value pairs
 * @param keys Array of data kinds to query
 * @param values Array of output pointers (types must match each key's
 *               documented output type)
 * @param[out] out_written On return, receives the number of values
 *             successfully written (may be NULL)
 * @return RUSTTY_SUCCESS if all queries succeed
 *
 * @ingroup terminal
 */
RUSTTY_API RusttyResult rustty_terminal_get_multi(RusttyTerminal terminal,
                                    size_t count,
                                    const RusttyTerminalData* keys,
                                    void** values,
                                    size_t* out_written);

/**
 * Resolve a point in the terminal grid to a grid reference.
 *
 * Resolves the given point (which can be in active, viewport, screen,
 * or history coordinates) to a grid reference for that location. Use
 * rustty_grid_ref_cell() and rustty_grid_ref_row() to extract the cell
 * and row.
 *
 * Lookups using the `active` and `viewport` tags are fast. The `screen`
 * and `history` tags may require traversing the full scrollback page list
 * to resolve the y coordinate, so they can be expensive for large
 * scrollback buffers.
 *
 * This function isn't meant to be used as the core of render loop. It
 * isn't built to sustain the framerates needed for rendering large screens.
 * Use the render state API for that. This API is instead meant for less
 * strictly performance-sensitive use cases.
 *
 * @param terminal The terminal handle (NULL returns RUSTTY_INVALID_VALUE)
 * @param point The point specifying which cell to look up
 * @param[out] out_ref On success, set to the grid reference at the given point (may be NULL)
 * @return RUSTTY_SUCCESS on success, RUSTTY_INVALID_VALUE if the terminal
 *         is NULL or the point is out of bounds
 *
 * @ingroup terminal
 */
RUSTTY_API RusttyResult rustty_terminal_grid_ref(RusttyTerminal terminal,
                                        RusttyPoint point,
                                        RusttyGridRef *out_ref);

/**
 * Create an owned tracked grid reference for a terminal point.
 *
 * This is the tracked variant of rustty_terminal_grid_ref(). The returned
 * handle follows the referenced cell as the terminal's page list is modified:
 * scrolling, pruning, resize/reflow, and other page-list operations update the
 * tracked reference automatically.
 *
 * The reference is attached to the terminal screen/page-list that is active at
 * creation time.
 *
 * If the point is outside the requested coordinate space, this returns
 * RUSTTY_INVALID_VALUE and writes NULL to out_ref.
 *
 * The returned handle must be freed with rustty_tracked_grid_ref_free(). If
 * the terminal is freed first, the handle remains valid only for
 * tracked-grid-ref APIs: it reports no value and can still be freed.
 *
 * @param terminal Terminal instance.
 * @param point Point to track.
 * @param[out] out_ref On success, receives the tracked reference handle.
 * @return RUSTTY_SUCCESS on success, RUSTTY_INVALID_VALUE if terminal,
 *         point, or out_ref is invalid, or RUSTTY_OUT_OF_MEMORY if allocation
 *         fails.
 *
 * @ingroup terminal
 */
RUSTTY_API RusttyResult rustty_terminal_grid_ref_track(
    RusttyTerminal terminal,
    RusttyPoint point,
    RusttyTrackedGridRef *out_ref);

/**
 * Convert a grid reference back to a point in the given coordinate system.
 *
 * This is the inverse of rustty_terminal_grid_ref(): given a grid reference,
 * it returns the x/y coordinates in the requested coordinate system (active,
 * viewport, screen, or history).
 *
 * The grid reference must have been obtained from the same terminal instance.
 * Like all grid references, it is only valid until the next mutating terminal
 * call.
 *
 * Not every grid reference is representable in every coordinate system. For
 * example, a cell in scrollback history cannot be expressed in active
 * coordinates, and a cell that has scrolled off the visible area cannot be
 * expressed in viewport coordinates. In these cases, the function returns
 * RUSTTY_NO_VALUE.
 *
 * @param terminal The terminal handle (NULL returns RUSTTY_INVALID_VALUE)
 * @param ref Pointer to the grid reference to convert
 * @param tag The target coordinate system
 * @param[out] out On success, set to the coordinate in the requested system (may be NULL)
 * @return RUSTTY_SUCCESS on success, RUSTTY_INVALID_VALUE if the terminal
 *         or ref is NULL/invalid, RUSTTY_NO_VALUE if the ref falls outside
 *         the requested coordinate system
 *
 * @ingroup terminal
 */
RUSTTY_API RusttyResult rustty_terminal_point_from_grid_ref(
    RusttyTerminal terminal,
    const RusttyGridRef *ref,
    RusttyPointTag tag,
    RusttyPointCoordinate *out);

/** @} */

#ifdef __cplusplus
}
#endif

#endif /* RUSTTY_VT_TERMINAL_H */
