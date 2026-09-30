/**
 * @file render.h
 *
 * Render state for creating high performance renderers.
 */

#ifndef RUSTTY_VT_RENDER_H
#define RUSTTY_VT_RENDER_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include <rustty/vt/allocator.h>
#include <rustty/vt/color.h>
#include <rustty/vt/terminal.h>
#include <rustty/vt/types.h>

#ifdef __cplusplus
extern "C" {
#endif

/** @defgroup render Render State
 *
 * Represents the state required to render a visible screen (a viewport)
 * of a terminal instance. This is stateful and optimized for repeated
 * updates from a single terminal instance and only updating dirty regions
 * of the screen.
 *
 * The key design principle of this API is that it only needs read/write
 * access to the terminal instance during the update call. This allows
 * the render state to minimally impact terminal IO performance and also
 * allows the renderer to be safely multi-threaded (as long as a lock is 
 * held during the update call to ensure exclusive access to the terminal 
 * instance).
 *
 * The basic usage of this API is:
 *
 *   1. Create an empty render state
 *   2. Update it from a terminal instance whenever you need.
 *   3. Read from the render state to get the data needed to draw your frame.
 *
 * ## Two-Phase Updates
 *
 * For callers that synchronize terminal access (e.g. a renderer thread
 * sharing a lock with an IO thread), the update can be split into two
 * phases to minimize the time the terminal must be held exclusively:
 * rustty_render_state_begin_update requires terminal access, while
 * rustty_render_state_end_update completes any deferred work using only
 * memory owned by the render state. A typical renderer would lock, begin
 * the update, unlock, and then end the update while the IO thread is free
 * to continue modifying the terminal. rustty_render_state_update is a
 * convenience that performs both phases in one call.
 *
 * ## Dirty Tracking
 *
 * Dirty tracking is a key feature of the render state that allows renderers
 * to efficiently determine what parts of the screen have changed and only 
 * redraw changed regions.
 *
 * The render state API keeps track of dirty state at two independent layers:
 * a global dirty state that indicates whether the entire frame is clean, 
 * partially dirty, or fully dirty, and a per-row dirty state that allows 
 * tracking which rows in a partially dirty frame have changed. 
 *
 * The user of the render state API is expected to unset both of these.
 * The `update` call does not unset dirty state, it only updates it. After
 * successfully rendering a complete frame, use rustty_render_state_clean()
 * to unset both layers in one call. The granular setters remain available
 * for callers that only consume part of a frame.
 *
 * An extremely important detail: setting one dirty state doesn't unset
 * the other. For example, setting the global dirty state to false does not
 * reset the row-level dirty flags. So, the caller of the render state API must
 * be careful to manage both layers of dirty state correctly. 
 *
 * ## Render Holds (Synchronized Output)
 *
 * A program can ask the terminal to stop updating the screen while it
 * draws a frame, so the user never sees a half-drawn one. Today programs
 * do this with synchronized output (DEC private mode 2026).
 *
 * rustty_render_state_update() does not check for this. It always
 * captures the terminal as it is right now, so a renderer that updates
 * on every draw will show half-drawn frames. To avoid that:
 *
 *   1. Set a RUSTTY_TERMINAL_OPT_RENDER_HOLD callback on the terminal.
 *   2. When the callback reports that a hold began, update the render
 *      state from within the callback. This captures the frame the
 *      program wants left on screen.
 *   3. Don't update the render state again until the callback reports
 *      that the hold ended. You can keep drawing it in the meantime.
 *   4. End the hold yourself if it lasts too long (one second is a
 *      common limit) so a misbehaving program can't freeze the screen.
 *
 * See RusttyTerminalRenderHoldFn for the details and a complete example.
 *
 * ## Examples
 *
 * ### Creating and updating render state
 * @snippet c-vt-render/src/main.c render-state-update
 *
 * ### Checking dirty state
 * @snippet c-vt-render/src/main.c render-dirty-check
 *
 * ### Reading colors
 * @snippet c-vt-render/src/main.c render-colors
 *
 * ### Reading cursor state
 * @snippet c-vt-render/src/main.c render-cursor
 *
 * ### Iterating rows and cells
 * @snippet c-vt-render/src/main.c render-row-iterate
 *
 * ### Resetting dirty state after rendering
 * @snippet c-vt-render/src/main.c render-dirty-reset
 *
 * @{
 */

/**
 * Dirty state of a render state after update.
 *
 * @ingroup render
 */
typedef enum RUSTTY_ENUM_TYPED {
  /** Not dirty at all; rendering can be skipped. */
  RUSTTY_RENDER_STATE_DIRTY_FALSE = 0,

  /** Some rows changed; renderer can redraw incrementally. */
  RUSTTY_RENDER_STATE_DIRTY_PARTIAL = 1,

  /** Global state changed; renderer should redraw everything. */
  RUSTTY_RENDER_STATE_DIRTY_FULL = 2,
  RUSTTY_RENDER_STATE_DIRTY_MAX_VALUE = RUSTTY_ENUM_MAX_VALUE,
} RusttyRenderStateDirty;

/**
 * Visual style of the cursor.
 *
 * @ingroup render
 */
typedef enum RUSTTY_ENUM_TYPED {
  /** Bar cursor (DECSCUSR 5, 6). */
  RUSTTY_RENDER_STATE_CURSOR_VISUAL_STYLE_BAR = 0,

  /** Block cursor (DECSCUSR 1, 2). */
  RUSTTY_RENDER_STATE_CURSOR_VISUAL_STYLE_BLOCK = 1,

  /** Underline cursor (DECSCUSR 3, 4). */
  RUSTTY_RENDER_STATE_CURSOR_VISUAL_STYLE_UNDERLINE = 2,

  /** Hollow block cursor. */
  RUSTTY_RENDER_STATE_CURSOR_VISUAL_STYLE_BLOCK_HOLLOW = 3,
  RUSTTY_RENDER_STATE_CURSOR_VISUAL_STYLE_MAX_VALUE = RUSTTY_ENUM_MAX_VALUE,
} RusttyRenderStateCursorVisualStyle;

/**
 * Queryable data kinds for rustty_render_state_get().
 *
 * @ingroup render
 */
typedef enum RUSTTY_ENUM_TYPED {
  /** Invalid / sentinel value. */
  RUSTTY_RENDER_STATE_DATA_INVALID = 0,

  /** Viewport width in cells (uint16_t). */
  RUSTTY_RENDER_STATE_DATA_COLS = 1,

  /** Viewport height in cells (uint16_t). */
  RUSTTY_RENDER_STATE_DATA_ROWS = 2,

  /** Current dirty state (RusttyRenderStateDirty). */
  RUSTTY_RENDER_STATE_DATA_DIRTY = 3,

  /** Populate a pre-allocated RusttyRenderStateRowIterator with row data
   *  from the render state (RusttyRenderStateRowIterator). Row data is
   *  only valid as long as the underlying render state is not updated.
   *  It is unsafe to use row data after updating the render state.
   *  */
  RUSTTY_RENDER_STATE_DATA_ROW_ITERATOR = 4,

  /** Default/current background color (RusttyColorRgb). */
  RUSTTY_RENDER_STATE_DATA_COLOR_BACKGROUND = 5,

  /** Default/current foreground color (RusttyColorRgb). */
  RUSTTY_RENDER_STATE_DATA_COLOR_FOREGROUND = 6,

  /** Cursor color when explicitly set by terminal state (RusttyColorRgb).
   *  Returns RUSTTY_INVALID_VALUE if no explicit cursor color is set;
   *  use COLOR_CURSOR_HAS_VALUE to check first. */
  RUSTTY_RENDER_STATE_DATA_COLOR_CURSOR = 7,

  /** Whether an explicit cursor color is set (bool). */
  RUSTTY_RENDER_STATE_DATA_COLOR_CURSOR_HAS_VALUE = 8,

  /** The active 256-color palette (RusttyColorRgb[256]). */
  RUSTTY_RENDER_STATE_DATA_COLOR_PALETTE = 9,

  /** The visual style of the cursor (RusttyRenderStateCursorVisualStyle). */
  RUSTTY_RENDER_STATE_DATA_CURSOR_VISUAL_STYLE = 10,

  /** Whether the cursor is visible based on terminal modes (bool). */
  RUSTTY_RENDER_STATE_DATA_CURSOR_VISIBLE = 11,

  /** Whether the cursor should blink based on terminal modes (bool). */
  RUSTTY_RENDER_STATE_DATA_CURSOR_BLINKING = 12,

  /** Whether the cursor is at a password input field (bool). */
  RUSTTY_RENDER_STATE_DATA_CURSOR_PASSWORD_INPUT = 13,

  /** Whether the cursor is visible within the viewport (bool).
   *  If false, the cursor viewport position values are undefined. */
  RUSTTY_RENDER_STATE_DATA_CURSOR_VIEWPORT_HAS_VALUE = 14,

  /** Cursor viewport x position in cells (uint16_t).
   *  Only valid when CURSOR_VIEWPORT_HAS_VALUE is true. */
  RUSTTY_RENDER_STATE_DATA_CURSOR_VIEWPORT_X = 15,

  /** Cursor viewport y position in cells (uint16_t).
   *  Only valid when CURSOR_VIEWPORT_HAS_VALUE is true. */
  RUSTTY_RENDER_STATE_DATA_CURSOR_VIEWPORT_Y = 16,

  /** Whether the cursor is on the tail of a wide character (bool).
   *  Only valid when CURSOR_VIEWPORT_HAS_VALUE is true. */
  RUSTTY_RENDER_STATE_DATA_CURSOR_VIEWPORT_WIDE_TAIL = 17,

  /** All cursor state in one sized struct (RusttyRenderStateCursor).
   *  Initialize the output with RUSTTY_INIT_SIZED before querying. */
  RUSTTY_RENDER_STATE_DATA_CURSOR = 18,

  /** All render-state colors in one sized struct (RusttyRenderStateColors).
   *  Initialize the output with RUSTTY_INIT_SIZED before querying. */
  RUSTTY_RENDER_STATE_DATA_COLORS = 19,
  RUSTTY_RENDER_STATE_DATA_MAX_VALUE = RUSTTY_ENUM_MAX_VALUE,
} RusttyRenderStateData;

/**
 * Settable options for rustty_render_state_set().
 *
 * @ingroup render
 */
typedef enum RUSTTY_ENUM_TYPED {
  /** Set dirty state (RusttyRenderStateDirty). */
  RUSTTY_RENDER_STATE_OPTION_DIRTY = 0,
  RUSTTY_RENDER_STATE_OPTION_MAX_VALUE = RUSTTY_ENUM_MAX_VALUE,
} RusttyRenderStateOption;

/**
 * Queryable data kinds for rustty_render_state_row_get().
 *
 * @ingroup render
 */
typedef enum RUSTTY_ENUM_TYPED {
  /** Invalid / sentinel value. */
  RUSTTY_RENDER_STATE_ROW_DATA_INVALID = 0,

  /** Whether the current row is dirty (bool). */
  RUSTTY_RENDER_STATE_ROW_DATA_DIRTY = 1,

  /** The raw row value (RusttyRow). */
  RUSTTY_RENDER_STATE_ROW_DATA_RAW = 2,

  /** Populate a pre-allocated RusttyRenderStateRowCells with cell data for
   *  the current row (RusttyRenderStateRowCells). Cell data is only 
   *  valid as long as the underlying render state is not updated. 
   *  It is unsafe to use cell data after updating the render state. */
  RUSTTY_RENDER_STATE_ROW_DATA_CELLS = 3,

  /** Row-local selected cell range (RusttyRenderStateRowSelection). */
  RUSTTY_RENDER_STATE_ROW_DATA_SELECTION = 4,

  /** A borrowed view of the raw cell values for the current row
   *  (RusttyCellsView). One value per column, identical to querying
   *  RUSTTY_RENDER_STATE_ROW_CELLS_DATA_RAW for each cell. The view
   *  is only valid as long as the underlying render state is not
   *  updated; it is unsafe to use after updating the render state.
   *
   *  This is the bulk alternative to iterating cells one at a time.
   *  It lets callers with expensive call boundaries (e.g. WebAssembly
   *  embedders) read an entire row with a single call.
   *
   *  Bit positions aren't protected by ABI, so callers should parse them
   *  out of the manifest from `rustty_type_json`. Callers with access
   *  to the C header or without high FFI costs should use `rustty_cell_get`.
   */
  RUSTTY_RENDER_STATE_ROW_DATA_CELLS_RAW = 5,
  RUSTTY_RENDER_STATE_ROW_DATA_MAX_VALUE = RUSTTY_ENUM_MAX_VALUE,
} RusttyRenderStateRowData;

/**
 * Settable options for rustty_render_state_row_set().
 *
 * @ingroup render
 */
typedef enum RUSTTY_ENUM_TYPED {
  /** Set dirty state for the current row (bool). */
  RUSTTY_RENDER_STATE_ROW_OPTION_DIRTY = 0,
  RUSTTY_RENDER_STATE_ROW_OPTION_MAX_VALUE = RUSTTY_ENUM_MAX_VALUE,
} RusttyRenderStateRowOption;

/**
 * Row-local selection range.
 *
 * This struct uses the sized-struct ABI pattern. Initialize with
 * RUSTTY_INIT_SIZED(RusttyRenderStateRowSelection) before querying
 * RUSTTY_RENDER_STATE_ROW_DATA_SELECTION.
 *
 * Querying RUSTTY_RENDER_STATE_ROW_DATA_SELECTION returns RUSTTY_NO_VALUE
 * if the current row does not intersect the current selection.
 *
 * @ingroup render
 */
typedef struct {
  /** Size of this struct in bytes. Must be set to sizeof(RusttyRenderStateRowSelection). */
  size_t size;

  /** Start column of the row-local selection range, inclusive. */
  uint16_t start_x;

  /** End column of the row-local selection range, inclusive. */
  uint16_t end_x;
} RusttyRenderStateRowSelection;

/**
 * Render-state cursor information.
 *
 * This struct uses the sized-struct ABI pattern. Initialize with
 * RUSTTY_INIT_SIZED(RusttyRenderStateCursor) before querying
 * RUSTTY_RENDER_STATE_DATA_CURSOR.
 *
 * When viewport_has_value is false, viewport_x, viewport_y, and wide_tail
 * contain undefined data and must not be read.
 *
 * @ingroup render
 */
typedef struct {
  /** Size of this struct in bytes. Must be set to sizeof(RusttyRenderStateCursor). */
  size_t size;

  /** Whether the cursor is visible within the viewport. */
  bool viewport_has_value;

  /** Cursor viewport x position in cells. */
  uint16_t viewport_x;

  /** Cursor viewport y position in cells. */
  uint16_t viewport_y;

  /** Whether the cursor is on the tail of a wide character. */
  bool wide_tail;

  /** Whether the cursor is visible based on terminal modes. */
  bool visible;

  /** Whether the cursor should blink based on terminal modes. */
  bool blinking;

  /** Whether the cursor is at a password input field. */
  bool password_input;

  /** The visual style of the cursor. */
  RusttyRenderStateCursorVisualStyle visual_style;
} RusttyRenderStateCursor;

/**
 * Render-state color information.
 *
 * This struct uses the sized-struct ABI pattern. Initialize with
 * RUSTTY_INIT_SIZED(RusttyRenderStateColors) before querying
 * RUSTTY_RENDER_STATE_DATA_COLORS.
 *
 * Example:
 * @code
 * RusttyRenderStateColors colors = RUSTTY_INIT_SIZED(RusttyRenderStateColors);
 * RusttyResult result = rustty_render_state_get(
 *     state, RUSTTY_RENDER_STATE_DATA_COLORS, &colors);
 * @endcode
 *
 * @ingroup render
 */
typedef struct {
  /** Size of this struct in bytes. Must be set to sizeof(RusttyRenderStateColors). */
  size_t size;

  /** The default/current background color for the render state. */
  RusttyColorRgb background;

  /** The default/current foreground color for the render state. */
  RusttyColorRgb foreground;

  /** The cursor color when explicitly set by terminal state. */
  RusttyColorRgb cursor;

  /** 
   * True when cursor contains a valid explicit cursor color value. 
   * If this is false, the cursor color should be ignored; it will 
   * contain undefined data.
   * */
  bool cursor_has_value;

  /** The active 256-color palette for this render state. */
  RusttyColorRgb palette[256];
} RusttyRenderStateColors;

/**
 * Create a new render state instance.
 *
 * @param allocator Pointer to allocator, or NULL to use the default allocator
 * @param state Pointer to store the created render state handle
 * @return RUSTTY_SUCCESS on success, RUSTTY_OUT_OF_MEMORY on allocation
 * failure
 *
 * @ingroup render
 */
RUSTTY_API RusttyResult rustty_render_state_new(const RusttyAllocator* allocator,
                                       RusttyRenderState* state);

/**
 * Free a render state instance.
 *
 * Releases all resources associated with the render state. After this call,
 * the render state handle becomes invalid.
 *
 * @param state The render state handle to free (may be NULL)
 *
 * @ingroup render
 */
RUSTTY_API void rustty_render_state_free(RusttyRenderState state);

/**
 * Update a render state instance from a terminal.
 *
 * This consumes terminal/screen dirty state in the same way as the internal
 * render state update path.
 *
 * This is a convenience function that performs a full update in one call,
 * equivalent to rustty_render_state_begin_update immediately followed by
 * rustty_render_state_end_update. Callers that hold a lock over the
 * terminal state should prefer calling the two phases directly so that the
 * lock is only held for the begin phase.
 *
 * @param state The render state handle (NULL returns RUSTTY_INVALID_VALUE)
 * @param terminal The terminal handle to read from (NULL returns RUSTTY_INVALID_VALUE)
 * @return RUSTTY_SUCCESS on success, RUSTTY_INVALID_VALUE if `state` or
 * `terminal` is NULL, RUSTTY_OUT_OF_MEMORY if updating the state requires
 * allocation and that allocation fails
 *
 * @ingroup render
 */
RUSTTY_API RusttyResult rustty_render_state_update(RusttyRenderState state,
                                          RusttyTerminal terminal);

/**
 * Begin an update of a render state instance from a terminal.
 *
 * Every begin must be completed with a rustty_render_state_end_update call
 * before the render state is read.
 *
 * This two-phase structure exists for callers that synchronize access to the
 * terminal state (e.g. with a lock shared with an IO thread): only this
 * function requires terminal access, so a caller can hold its lock for this
 * call only and then call rustty_render_state_end_update after releasing
 * it. The end phase exclusively reads and writes memory owned by the render
 * state, so it is safe to call while the terminal is being modified.
 *
 * Work that doesn't require terminal access may be deferred to the end phase
 * to keep this call (and therefore lock hold time) as short as possible.
 * Callers must treat the render state as incomplete until
 * rustty_render_state_end_update is called.
 *
 * This consumes terminal/screen dirty state in the same way as the internal
 * render state update path.
 *
 * @param state The render state handle (NULL returns RUSTTY_INVALID_VALUE)
 * @param terminal The terminal handle to read from (NULL returns RUSTTY_INVALID_VALUE)
 * @return RUSTTY_SUCCESS on success, RUSTTY_INVALID_VALUE if `state` or
 * `terminal` is NULL, RUSTTY_OUT_OF_MEMORY if updating the state requires
 * allocation and that allocation fails
 *
 * @ingroup render
 */
RUSTTY_API RusttyResult rustty_render_state_begin_update(RusttyRenderState state,
                                                RusttyTerminal terminal);

/**
 * Complete a prior rustty_render_state_begin_update call by performing any
 * deferred work.
 *
 * This only reads and writes memory owned by the render state, so it is safe
 * to call while the terminal is being modified (no terminal synchronization
 * is required). Calling this without a prior begin is a safe no-op.
 *
 * @param state The render state handle (NULL returns RUSTTY_INVALID_VALUE)
 * @return RUSTTY_SUCCESS on success, RUSTTY_INVALID_VALUE if `state` is
 * NULL
 *
 * @ingroup render
 */
RUSTTY_API RusttyResult rustty_render_state_end_update(RusttyRenderState state);

/**
 * Mark all dirty render-state data as consumed.
 *
 * This sets the global dirty state to RUSTTY_RENDER_STATE_DIRTY_FALSE and
 * clears every per-row dirty flag. It is idempotent and does not modify cell
 * contents or dirty state owned by the terminal. Call this only after a
 * complete frame has been rendered successfully; partial consumers should
 * use rustty_render_state_set() and rustty_render_state_row_set() instead.
 *
 * @param state The render state handle (NULL returns RUSTTY_INVALID_VALUE)
 * @return RUSTTY_SUCCESS on success, RUSTTY_INVALID_VALUE if `state` is
 *         NULL
 *
 * @ingroup render
 */
RUSTTY_API RusttyResult rustty_render_state_clean(RusttyRenderState state);

/**
 * Get a value from a render state.
 *
 * The `out` pointer must point to a value of the type corresponding to the
 * requested data kind (see RusttyRenderStateData).
 *
 * @param state The render state handle (NULL returns RUSTTY_INVALID_VALUE)
 * @param data The data kind to query
 * @param[out] out Pointer to receive the queried value
 * @return RUSTTY_SUCCESS on success, RUSTTY_INVALID_VALUE if `state` or
 *         `out` is NULL, `data` is not a recognized enum value, or a sized
 *         output struct is smaller than `sizeof(size_t)`
 *
 * @ingroup render
 */
RUSTTY_API RusttyResult rustty_render_state_get(RusttyRenderState state,
                                        RusttyRenderStateData data,
                                        void* out);

/**
 * Get multiple data fields from a render state in a single call.
 *
 * Each element in the keys array specifies a data kind, and the
 * corresponding element in the values array receives the result.
 *
 * Processing stops at the first error; on success out_written
 * is set to count, on error it is set to the index of the
 * failing key (i.e. the number of values successfully written).
 *
 * @param state The render state handle (NULL returns RUSTTY_INVALID_VALUE)
 * @param count Number of key/value pairs
 * @param keys Array of data kinds to query
 * @param values Array of output pointers (types must match each key's
 *               documented output type)
 * @param[out] out_written On return, receives the number of values
 *             successfully written (may be NULL)
 * @return RUSTTY_SUCCESS if all queries succeed
 *
 * @ingroup render
 */
RUSTTY_API RusttyResult rustty_render_state_get_multi(
    RusttyRenderState state,
    size_t count,
    const RusttyRenderStateData* keys,
    void** values,
    size_t* out_written);

/**
 * Set an option on a render state.
 *
 * The `value` pointer must point to a value of the type corresponding to the
 * requested option kind (see RusttyRenderStateOption).
 *
 * @param state The render state handle (NULL returns RUSTTY_INVALID_VALUE)
 * @param option The option to set
 * @param[in] value Pointer to the value to set (NULL returns
 *            RUSTTY_INVALID_VALUE)
 * @return RUSTTY_SUCCESS on success, RUSTTY_INVALID_VALUE if `state` or
 *         `value` is NULL
 *
 * @ingroup render
 */
RUSTTY_API RusttyResult rustty_render_state_set(RusttyRenderState state,
                                       RusttyRenderStateOption option,
                                       const void* value);

/**
 * Create a new row iterator instance.
 *
 * All fields except the allocator are left undefined until populated
 * via rustty_render_state_get() with
 * RUSTTY_RENDER_STATE_DATA_ROW_ITERATOR.
 *
 * @param allocator Pointer to allocator, or NULL to use the default allocator
 * @param[out] out_iterator On success, receives the created iterator handle
 * @return RUSTTY_SUCCESS on success, RUSTTY_OUT_OF_MEMORY on allocation
 *         failure
 *
 * @ingroup render
 */
RUSTTY_API RusttyResult rustty_render_state_row_iterator_new(
    const RusttyAllocator* allocator,
    RusttyRenderStateRowIterator* out_iterator);

/**
 * Free a render-state row iterator.
 *
 * @param iterator The iterator handle to free (may be NULL)
 *
 * @ingroup render
 */
RUSTTY_API void rustty_render_state_row_iterator_free(RusttyRenderStateRowIterator iterator);

/**
 * Move a render-state row iterator to the next row.
 *
 * Rows are visited contiguously in ascending viewport order, starting at
 * y = 0. Returns true if the iterator moved successfully and row data is
 * available to read at the new position.
 *
 * @param iterator The iterator handle to advance (may be NULL)
 * @return true if advanced to the next row, false if `iterator` is
 *         NULL or if the iterator has reached the end
 *
 * @ingroup render
 */
RUSTTY_API bool rustty_render_state_row_iterator_next(RusttyRenderStateRowIterator iterator);

/**
 * Move a render-state row iterator to the next row requiring a redraw.
 *
 * If the global dirty state is RUSTTY_RENDER_STATE_DIRTY_FALSE, this returns
 * false. If it is RUSTTY_RENDER_STATE_DIRTY_PARTIAL, clean rows are skipped.
 * If it is RUSTTY_RENDER_STATE_DIRTY_FULL, every remaining row is returned
 * regardless of its per-row dirty flag. Rows are returned in ascending
 * viewport order. This function does not clear any dirty state.
 *
 * @param iterator The iterator handle to advance (NULL returns false)
 * @param[out] out_y Receives the viewport y coordinate when true is returned
 *                   (NULL returns false); it is not modified when false is
 *                   returned
 * @return true if advanced to a row requiring a redraw, false if an argument
 *         is NULL or the iterator has reached the end of the effective dirty
 *         rows
 *
 * @ingroup render
 */
RUSTTY_API bool rustty_render_state_row_iterator_next_dirty(
    RusttyRenderStateRowIterator iterator,
    uint16_t* out_y);

/**
 * Get a value from the current row in a render-state row iterator.
 *
 * The `out` pointer must point to a value of the type corresponding to the
 * requested data kind (see RusttyRenderStateRowData).
 * Call rustty_render_state_row_iterator_next() or
 * rustty_render_state_row_iterator_next_dirty() at least once before
 * calling this function.
 *
 * @param iterator The iterator handle to query (NULL returns RUSTTY_INVALID_VALUE)
 * @param data The data kind to query
 * @param[out] out Pointer to receive the queried value
 * @return RUSTTY_SUCCESS on success, RUSTTY_INVALID_VALUE if
 *         `iterator` is NULL or the iterator is not positioned on a row
 *
 * @ingroup render
 */
RUSTTY_API RusttyResult rustty_render_state_row_get(
    RusttyRenderStateRowIterator iterator,
    RusttyRenderStateRowData data,
    void* out);

/**
 * Get multiple data fields from the current row in a single call.
 *
 * Each element in the keys array specifies a data kind, and the
 * corresponding element in the values array receives the result.
 *
 * Processing stops at the first error; on success out_written
 * is set to count, on error it is set to the index of the
 * failing key (i.e. the number of values successfully written).
 *
 * @param iterator The iterator handle (NULL returns RUSTTY_INVALID_VALUE)
 * @param count Number of key/value pairs
 * @param keys Array of data kinds to query
 * @param values Array of output pointers (types must match each key's
 *               documented output type)
 * @param[out] out_written On return, receives the number of values
 *             successfully written (may be NULL)
 * @return RUSTTY_SUCCESS if all queries succeed
 *
 * @ingroup render
 */
RUSTTY_API RusttyResult rustty_render_state_row_get_multi(
    RusttyRenderStateRowIterator iterator,
    size_t count,
    const RusttyRenderStateRowData* keys,
    void** values,
    size_t* out_written);

/**
 * Set an option on the current row in a render-state row iterator.
 *
 * The `value` pointer must point to a value of the type corresponding to the
 * requested option kind (see RusttyRenderStateRowOption).
 * Call rustty_render_state_row_iterator_next() or
 * rustty_render_state_row_iterator_next_dirty() at least once before
 * calling this function.
 *
 * @param iterator The iterator handle to update (NULL returns RUSTTY_INVALID_VALUE)
 * @param option The option to set
 * @param[in] value Pointer to the value to set (NULL returns
 *            RUSTTY_INVALID_VALUE)
 * @return RUSTTY_SUCCESS on success, RUSTTY_INVALID_VALUE if
 *         `iterator` is NULL or the iterator is not positioned on a row
 *
 * @ingroup render
 */
RUSTTY_API RusttyResult rustty_render_state_row_set(
    RusttyRenderStateRowIterator iterator,
    RusttyRenderStateRowOption option,
    const void* value);

/**
 * Create a new row cells instance.
 *
 * All fields except the allocator are left undefined until populated
 * via rustty_render_state_row_get() with
 * RUSTTY_RENDER_STATE_ROW_DATA_CELLS.
 *
 * You can reuse this value repeatedly with rustty_render_state_row_get() to 
 * avoid allocating a new cells container for every row.
 *
 * @param allocator Pointer to allocator, or NULL to use the default allocator
 * @param[out] out_cells On success, receives the created row cells handle
 * @return RUSTTY_SUCCESS on success, RUSTTY_OUT_OF_MEMORY on allocation
 *         failure
 *
 * @ingroup render
 */
RUSTTY_API RusttyResult rustty_render_state_row_cells_new(
    const RusttyAllocator* allocator,
    RusttyRenderStateRowCells* out_cells);

/**
 * Queryable data kinds for rustty_render_state_row_cells_get().
 *
 * @ingroup render
 */
typedef enum RUSTTY_ENUM_TYPED {
  /** Invalid / sentinel value. */
  RUSTTY_RENDER_STATE_ROW_CELLS_DATA_INVALID = 0,

  /** The raw cell value (RusttyCell). */
  RUSTTY_RENDER_STATE_ROW_CELLS_DATA_RAW = 1,

  /** The style for the current cell (RusttyStyle). */
  RUSTTY_RENDER_STATE_ROW_CELLS_DATA_STYLE = 2,

  /** The total number of grapheme codepoints including the base codepoint
   *  (uint32_t). Returns 0 if the cell has no text. */
  RUSTTY_RENDER_STATE_ROW_CELLS_DATA_GRAPHEMES_LEN = 3,

  /** Write grapheme codepoints into a caller-provided buffer (uint32_t*).
   *  The buffer must be at least graphemes_len elements. The base codepoint
   *  is written first, followed by any extra codepoints. */
  RUSTTY_RENDER_STATE_ROW_CELLS_DATA_GRAPHEMES_BUF = 4,

  /** The resolved background color of the cell (RusttyColorRgb).
   *  Flattens the three possible sources: content-tag bg_color_rgb,
   *  content-tag bg_color_palette (looked up in the palette), or the
   *  style's bg_color. Returns RUSTTY_INVALID_VALUE if the cell has
   *  no background color, in which case the caller should use whatever
   *  default background color it wants (e.g. the terminal background). */
  RUSTTY_RENDER_STATE_ROW_CELLS_DATA_BG_COLOR = 5,

  /** The resolved foreground color of the cell (RusttyColorRgb).
   *  Resolves palette indices through the palette. Bold color handling
   *  is not applied; the caller should handle bold styling separately.
   *  Returns RUSTTY_INVALID_VALUE if the cell has no explicit foreground
   *  color, in which case the caller should use whatever default foreground
   *  color it wants (e.g. the terminal foreground). */
  RUSTTY_RENDER_STATE_ROW_CELLS_DATA_FG_COLOR = 6,

  /** Whether the cell is contained within the current selection (bool).
   *  This returns true when the cell's column is within the current row's
   *  row-local selection range, and false otherwise. Rendering policy for
   *  selected cells (colors, inversion, etc.) is left to the caller.
   *
   *  Renderers that can draw cells in spans may be more efficient querying
   *  RUSTTY_RENDER_STATE_ROW_DATA_SELECTION once per row and applying that
   *  range directly, avoiding one C API call per cell for selection state. */
  RUSTTY_RENDER_STATE_ROW_CELLS_DATA_SELECTED = 7,

  /** Whether the cell has any explicit styling (bool).
   *  This is equivalent to querying the raw cell's
   *  RUSTTY_CELL_DATA_HAS_STYLING value, but avoids materializing the raw
   *  RusttyCell for renderers that only need to know whether fetching the
   *  full style is necessary. */
  RUSTTY_RENDER_STATE_ROW_CELLS_DATA_HAS_STYLING = 8,

  /**
   * Encode the current cell's full grapheme cluster as UTF-8 into a
   * caller-provided buffer (RusttyBuffer).
   *
   * The base codepoint is encoded first, followed by any extra grapheme
   * codepoints. Returns RUSTTY_SUCCESS with len=0 when the cell has no text.
   *
   * If ptr is NULL or cap is too small for a non-empty cell, returns
   * RUSTTY_OUT_OF_SPACE without writing any bytes and sets len to the required
   * buffer size in bytes.
   */
  RUSTTY_RENDER_STATE_ROW_CELLS_DATA_GRAPHEMES_UTF8 = 9,
  RUSTTY_RENDER_STATE_ROW_CELLS_DATA_MAX_VALUE = RUSTTY_ENUM_MAX_VALUE,
} RusttyRenderStateRowCellsData;

/**
 * Move a render-state row cells iterator to the next cell.
 *
 * Returns true if the iterator moved successfully and cell data is
 * available to read at the new position.
 *
 * @param cells The row cells handle to advance (may be NULL)
 * @return true if advanced to the next cell, false if `cells` is
 *         NULL or if the iterator has reached the end
 *
 * @ingroup render
 */
RUSTTY_API bool rustty_render_state_row_cells_next(RusttyRenderStateRowCells cells);

/**
 * Move a render-state row cells iterator to a specific column.
 *
 * Positions the iterator at the given x (column) index so that
 * subsequent reads return data for that cell.
 *
 * @param cells The row cells handle to reposition (NULL returns
 *        RUSTTY_INVALID_VALUE)
 * @param x The zero-based column index to select
 * @return RUSTTY_SUCCESS on success, RUSTTY_INVALID_VALUE if `cells`
 *         is NULL or `x` is out of range
 *
 * @ingroup render
 */
RUSTTY_API RusttyResult rustty_render_state_row_cells_select(
    RusttyRenderStateRowCells cells, uint16_t x);

/**
 * Get a value from the current cell in a render-state row cells iterator.
 *
 * The `out` pointer must point to a value of the type corresponding to the
 * requested data kind (see RusttyRenderStateRowCellsData).
 * Call rustty_render_state_row_cells_next() or
 * rustty_render_state_row_cells_select() at least once before
 * calling this function.
 *
 * @param cells The row cells handle to query (NULL returns RUSTTY_INVALID_VALUE)
 * @param data The data kind to query
 * @param[out] out Pointer to receive the queried value
 * @return RUSTTY_SUCCESS on success, RUSTTY_INVALID_VALUE if
 *         `cells` is NULL or the iterator is not positioned on a cell
 *
 * @ingroup render
 */
RUSTTY_API RusttyResult rustty_render_state_row_cells_get(
    RusttyRenderStateRowCells cells,
    RusttyRenderStateRowCellsData data,
    void* out);

/**
 * Get multiple data fields from the current cell in a single call.
 *
 * Each element in the keys array specifies a data kind, and the
 * corresponding element in the values array receives the result.
 *
 * Processing stops at the first error; on success out_written
 * is set to count, on error it is set to the index of the
 * failing key (i.e. the number of values successfully written).
 *
 * @param cells The row cells handle (NULL returns RUSTTY_INVALID_VALUE)
 * @param count Number of key/value pairs
 * @param keys Array of data kinds to query
 * @param values Array of output pointers (types must match each key's
 *               documented output type)
 * @param[out] out_written On return, receives the number of values
 *             successfully written (may be NULL)
 * @return RUSTTY_SUCCESS if all queries succeed
 *
 * @ingroup render
 */
RUSTTY_API RusttyResult rustty_render_state_row_cells_get_multi(
    RusttyRenderStateRowCells cells,
    size_t count,
    const RusttyRenderStateRowCellsData* keys,
    void** values,
    size_t* out_written);

/**
 * Free a row cells instance.
 *
 * @param cells The row cells handle to free (may be NULL)
 *
 * @ingroup render
 */
RUSTTY_API void rustty_render_state_row_cells_free(RusttyRenderStateRowCells cells);

/** @} */

#ifdef __cplusplus
}
#endif

#endif /* RUSTTY_VT_RENDER_H */
