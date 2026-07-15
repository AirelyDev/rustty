/**
 * @file screen.h
 *
 * Terminal screen cell and row types.
 */

#ifndef RUSTTY_VT_SCREEN_H
#define RUSTTY_VT_SCREEN_H

#include <stdbool.h>
#include <stdint.h>
#include <rustty/vt/color.h>
#include <rustty/vt/types.h>

#ifdef __cplusplus
extern "C" {
#endif

/** @defgroup screen Screen
 *
 * Terminal screen cell and row types.
 *
 * These types represent the contents of a terminal screen. A RusttyCell
 * is a single grid cell and a RusttyRow is a single row. Both are opaque
 * values whose fields are accessed via rustty_cell_get() and
 * rustty_row_get() respectively.
 *
 * @{
 */

/**
 * Opaque cell value.
 *
 * Represents a single terminal cell. The internal layout is opaque and
 * must be queried via rustty_cell_get(). Obtain cell values from
 * terminal query APIs.
 *
 * @ingroup screen
 */
typedef uint64_t RusttyCell;

/**
 * Opaque row value.
 *
 * Represents a single terminal row. The internal layout is opaque and
 * must be queried via rustty_row_get(). Obtain row values from
 * terminal query APIs.
 *
 * @ingroup screen
 */
typedef uint64_t RusttyRow;

/**
 * Cell content tag.
 *
 * Describes what kind of content a cell holds.
 *
 * @ingroup screen
 */
typedef enum RUSTTY_ENUM_TYPED {
  /** A single codepoint (may be zero for empty). */
  RUSTTY_CELL_CONTENT_CODEPOINT = 0,

  /** A codepoint that is part of a multi-codepoint grapheme cluster. */
  RUSTTY_CELL_CONTENT_CODEPOINT_GRAPHEME = 1,

  /** No text; background color from palette. */
  RUSTTY_CELL_CONTENT_BG_COLOR_PALETTE = 2,

  /** No text; background color as RGB. */
  RUSTTY_CELL_CONTENT_BG_COLOR_RGB = 3,
  RUSTTY_CELL_CONTENT_TAG_MAX_VALUE = RUSTTY_ENUM_MAX_VALUE,
} RusttyCellContentTag;

/**
 * Cell wide property.
 *
 * Describes the width behavior of a cell.
 *
 * @ingroup screen
 */
typedef enum RUSTTY_ENUM_TYPED {
  /** Not a wide character, cell width 1. */
  RUSTTY_CELL_WIDE_NARROW = 0,

  /** Wide character, cell width 2. */
  RUSTTY_CELL_WIDE_WIDE = 1,

  /** Spacer after wide character. Do not render. */
  RUSTTY_CELL_WIDE_SPACER_TAIL = 2,

  /** Spacer at end of soft-wrapped line for a wide character. */
  RUSTTY_CELL_WIDE_SPACER_HEAD = 3,
  RUSTTY_CELL_WIDE_MAX_VALUE = RUSTTY_ENUM_MAX_VALUE,
} RusttyCellWide;

/**
 * Semantic content type of a cell.
 *
 * Set by semantic prompt sequences (OSC 133) to distinguish between
 * command output, user input, and shell prompt text.
 *
 * @ingroup screen
 */
typedef enum RUSTTY_ENUM_TYPED {
  /** Regular output content, such as command output. */
  RUSTTY_CELL_SEMANTIC_OUTPUT = 0,

  /** Content that is part of user input. */
  RUSTTY_CELL_SEMANTIC_INPUT = 1,

  /** Content that is part of a shell prompt. */
  RUSTTY_CELL_SEMANTIC_PROMPT = 2,
  RUSTTY_CELL_SEMANTIC_MAX_VALUE = RUSTTY_ENUM_MAX_VALUE,
} RusttyCellSemanticContent;

/**
 * Cell data types.
 *
 * These values specify what type of data to extract from a cell
 * using `rustty_cell_get`.
 *
 * @ingroup screen
 */
typedef enum RUSTTY_ENUM_TYPED {
  /** Invalid data type. Never results in any data extraction. */
  RUSTTY_CELL_DATA_INVALID = 0,

  /**
   * The codepoint of the cell (0 if empty or bg-color-only).
   *
   * Output type: uint32_t *
   */
  RUSTTY_CELL_DATA_CODEPOINT = 1,

  /**
   * The content tag describing what kind of content is in the cell.
   *
   * Output type: RusttyCellContentTag *
   */
  RUSTTY_CELL_DATA_CONTENT_TAG = 2,

  /**
   * The wide property of the cell.
   *
   * Output type: RusttyCellWide *
   */
  RUSTTY_CELL_DATA_WIDE = 3,

  /**
   * Whether the cell has text to render.
   *
   * Output type: bool *
   */
  RUSTTY_CELL_DATA_HAS_TEXT = 4,

  /**
   * Whether the cell has non-default styling.
   *
   * Output type: bool *
   */
  RUSTTY_CELL_DATA_HAS_STYLING = 5,

  /**
   * The style ID for the cell (for use with style lookups).
   *
   * Output type: uint16_t *
   */
  RUSTTY_CELL_DATA_STYLE_ID = 6,

  /**
   * Whether the cell has a hyperlink.
   *
   * Output type: bool *
   */
  RUSTTY_CELL_DATA_HAS_HYPERLINK = 7,

  /**
   * Whether the cell is protected.
   *
   * Output type: bool *
   */
  RUSTTY_CELL_DATA_PROTECTED = 8,

  /**
   * The semantic content type of the cell (from OSC 133).
   *
   * Output type: RusttyCellSemanticContent *
   */
  RUSTTY_CELL_DATA_SEMANTIC_CONTENT = 9,

  /**
   * The palette index for the cell's background color.
   * Only valid when content_tag is RUSTTY_CELL_CONTENT_BG_COLOR_PALETTE.
   *
   * Output type: RusttyColorPaletteIndex *
   */
  RUSTTY_CELL_DATA_COLOR_PALETTE = 10,

  /**
   * The RGB value for the cell's background color.
   * Only valid when content_tag is RUSTTY_CELL_CONTENT_BG_COLOR_RGB.
   *
   * Output type: RusttyColorRgb *
   */
  RUSTTY_CELL_DATA_COLOR_RGB = 11,
  RUSTTY_CELL_DATA_MAX_VALUE = RUSTTY_ENUM_MAX_VALUE,
} RusttyCellData;

/**
 * Row semantic prompt state.
 *
 * Indicates whether any cells in a row are part of a shell prompt,
 * as reported by OSC 133 sequences.
 *
 * @ingroup screen
 */
typedef enum RUSTTY_ENUM_TYPED {
  /** No prompt cells in this row. */
  RUSTTY_ROW_SEMANTIC_NONE = 0,

  /** Prompt cells exist and this is a primary prompt line. */
  RUSTTY_ROW_SEMANTIC_PROMPT = 1,

  /** Prompt cells exist and this is a continuation line. */
  RUSTTY_ROW_SEMANTIC_PROMPT_CONTINUATION = 2,
  RUSTTY_ROW_SEMANTIC_MAX_VALUE = RUSTTY_ENUM_MAX_VALUE,
} RusttyRowSemanticPrompt;

/**
 * Row data types.
 *
 * These values specify what type of data to extract from a row
 * using `rustty_row_get`.
 *
 * @ingroup screen
 */
typedef enum RUSTTY_ENUM_TYPED {
  /** Invalid data type. Never results in any data extraction. */
  RUSTTY_ROW_DATA_INVALID = 0,

  /**
   * Whether this row is soft-wrapped.
   *
   * Output type: bool *
   */
  RUSTTY_ROW_DATA_WRAP = 1,

  /**
   * Whether this row is a continuation of a soft-wrapped row.
   *
   * Output type: bool *
   */
  RUSTTY_ROW_DATA_WRAP_CONTINUATION = 2,

  /**
   * Whether any cells in this row have grapheme clusters.
   *
   * Output type: bool *
   */
  RUSTTY_ROW_DATA_GRAPHEME = 3,

  /**
   * Whether any cells in this row have styling (may have false positives).
   *
   * Output type: bool *
   */
  RUSTTY_ROW_DATA_STYLED = 4,

  /**
   * Whether any cells in this row have hyperlinks (may have false positives).
   *
   * Output type: bool *
   */
  RUSTTY_ROW_DATA_HYPERLINK = 5,

  /**
   * The semantic prompt state of this row.
   *
   * Output type: RusttyRowSemanticPrompt *
   */
  RUSTTY_ROW_DATA_SEMANTIC_PROMPT = 6,

  /**
   * Whether this row contains a Kitty virtual placeholder.
   *
   * Output type: bool *
   */
  RUSTTY_ROW_DATA_KITTY_VIRTUAL_PLACEHOLDER = 7,

  /**
   * Whether this row is dirty and requires a redraw.
   *
   * Output type: bool *
   */
  RUSTTY_ROW_DATA_DIRTY = 8,
  RUSTTY_ROW_DATA_MAX_VALUE = RUSTTY_ENUM_MAX_VALUE,
} RusttyRowData;

/**
 * Get data from a cell.
 *
 * Extracts typed data from the given cell based on the specified
 * data type. The output pointer must be of the appropriate type for the
 * requested data kind. Valid data types and output types are documented
 * in the `RusttyCellData` enum.
 *
 * @param cell The cell value
 * @param data The type of data to extract
 * @param out Pointer to store the extracted data (type depends on data parameter)
 * @return RUSTTY_SUCCESS on success, RUSTTY_INVALID_VALUE if the
 *         data type is invalid
 *
 * @ingroup screen
 */
RUSTTY_API RusttyResult rustty_cell_get(RusttyCell cell,
                                RusttyCellData data,
                                void *out);

/**
 * Get multiple data fields from a cell in a single call.
 *
 * Each element in the keys array specifies a data kind, and the
 * corresponding element in the values array receives the result.
 *
 * Processing stops at the first error; on success out_written
 * is set to count, on error it is set to the index of the
 * failing key (i.e. the number of values successfully written).
 *
 * @param cell The cell value
 * @param count Number of key/value pairs
 * @param keys Array of data kinds to query
 * @param values Array of output pointers (types must match each key's
 *               documented output type)
 * @param[out] out_written On return, receives the number of values
 *             successfully written (may be NULL)
 * @return RUSTTY_SUCCESS if all queries succeed
 *
 * @ingroup screen
 */
RUSTTY_API RusttyResult rustty_cell_get_multi(RusttyCell cell,
                                     size_t count,
                                     const RusttyCellData* keys,
                                     void** values,
                                     size_t* out_written);

/**
 * Get data from a row.
 *
 * Extracts typed data from the given row based on the specified
 * data type. The output pointer must be of the appropriate type for the
 * requested data kind. Valid data types and output types are documented
 * in the `RusttyRowData` enum.
 *
 * @param row The row value
 * @param data The type of data to extract
 * @param out Pointer to store the extracted data (type depends on data parameter)
 * @return RUSTTY_SUCCESS on success, RUSTTY_INVALID_VALUE if the
 *         data type is invalid
 *
 * @ingroup screen
 */
RUSTTY_API RusttyResult rustty_row_get(RusttyRow row,
                               RusttyRowData data,
                               void *out);

/**
 * Get multiple data fields from a row in a single call.
 *
 * Each element in the keys array specifies a data kind, and the
 * corresponding element in the values array receives the result.
 *
 * Processing stops at the first error; on success out_written
 * is set to count, on error it is set to the index of the
 * failing key (i.e. the number of values successfully written).
 *
 * @param row The row value
 * @param count Number of key/value pairs
 * @param keys Array of data kinds to query
 * @param values Array of output pointers (types must match each key's
 *               documented output type)
 * @param[out] out_written On return, receives the number of values
 *             successfully written (may be NULL)
 * @return RUSTTY_SUCCESS if all queries succeed
 *
 * @ingroup screen
 */
RUSTTY_API RusttyResult rustty_row_get_multi(RusttyRow row,
                                    size_t count,
                                    const RusttyRowData* keys,
                                    void** values,
                                    size_t* out_written);

/** @} */

#ifdef __cplusplus
}
#endif

#endif /* RUSTTY_VT_SCREEN_H */
