/**
 * @file style.h
 *
 * Terminal cell style types.
 */

#ifndef RUSTTY_VT_STYLE_H
#define RUSTTY_VT_STYLE_H

#include <rustty/vt/color.h>
#include <rustty/vt/types.h>
#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/** @defgroup style Style
 *
 * Terminal cell style attributes.
 *
 * A style describes the visual attributes of a terminal cell, including
 * foreground, background, and underline colors, as well as flags for
 * bold, italic, underline, and other text decorations.
 *
 * @{
 */

/**
 * Style identifier type.
 *
 * Used to look up the full style from a grid reference.
 * Obtain this from a cell via RUSTTY_CELL_DATA_STYLE_ID.
 *
 * @ingroup style
 */
typedef uint16_t RusttyStyleId;

/**
 * Style color tags.
 *
 * These values identify the type of color in a style color.
 * Use the tag to determine which field in the color value union to access.
 *
 * @ingroup style
 */
typedef enum RUSTTY_ENUM_TYPED {
  RUSTTY_STYLE_COLOR_NONE = 0,
  RUSTTY_STYLE_COLOR_PALETTE = 1,
  RUSTTY_STYLE_COLOR_RGB = 2,
  RUSTTY_STYLE_COLOR_TAG_MAX_VALUE = RUSTTY_ENUM_MAX_VALUE,
  } RusttyStyleColorTag;

/**
 * Style color value union.
 *
 * Use the tag to determine which field is active.
 *
 * @ingroup style
 */
typedef union {
  RusttyColorPaletteIndex palette;
  RusttyColorRgb rgb;
  uint64_t _padding;
} RusttyStyleColorValue;

/**
 * Style color (tagged union).
 *
 * A color used in a style attribute. Can be unset (none), a palette
 * index, or a direct RGB value.
 *
 * @ingroup style
 */
typedef struct {
  RusttyStyleColorTag tag;
  RusttyStyleColorValue value;
} RusttyStyleColor;

/**
 * Terminal cell style.
 *
 * Describes the complete visual style for a terminal cell, including
 * foreground, background, and underline colors, as well as text
 * decoration flags. The underline field uses the same values as
 * RusttySgrUnderline.
 *
 * This is a sized struct. Use RUSTTY_INIT_SIZED() to initialize it.
 *
 * @ingroup style
 */
typedef struct {
  size_t size;
  RusttyStyleColor fg_color;
  RusttyStyleColor bg_color;
  RusttyStyleColor underline_color;
  bool bold;
  bool italic;
  bool faint;
  bool blink;
  bool inverse;
  bool invisible;
  bool strikethrough;
  bool overline;
  int underline; /**< One of RUSTTY_SGR_UNDERLINE_* values */
} RusttyStyle;

/**
 * Get the default style.
 *
 * Initializes the style to the default values (no colors, no flags).
 *
 * @param style Pointer to the style to initialize
 *
 * @ingroup style
 */
RUSTTY_API void rustty_style_default(RusttyStyle* style);

/**
 * Check if a style is the default style.
 *
 * Returns true if all colors are unset and all flags are off.
 *
 * @param style Pointer to the style to check
 * @return true if the style is the default style
 *
 * @ingroup style
 */
RUSTTY_API bool rustty_style_is_default(const RusttyStyle* style);

#ifdef __cplusplus
}
#endif

/** @} */

#endif /* RUSTTY_VT_STYLE_H */
