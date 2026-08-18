/**
 * @file point.h
 *
 * Terminal point types for referencing locations in the terminal grid.
 */

#ifndef RUSTTY_VT_POINT_H
#define RUSTTY_VT_POINT_H

#include <stdint.h>

#include <rustty/vt/types.h>

#ifdef __cplusplus
extern "C" {
#endif

/** @defgroup point Point
 *
 * Types for referencing x/y positions in the terminal grid under
 * different coordinate systems (active area, viewport, full screen,
 * scrollback history).
 *
 * @{
 */

/**
 * A coordinate in the terminal grid.
 *
 * @ingroup point
 */
typedef struct {
  /** Column (0-indexed). */
  uint16_t x;

  /** Row (0-indexed). May exceed page size for screen/history tags. */
  uint32_t y;
} RusttyPointCoordinate;

/**
 * Point reference tag.
 *
 * Determines which coordinate system a point uses.
 *
 * @ingroup point
 */
typedef enum RUSTTY_ENUM_TYPED {
  /** Active area where the cursor can move. */
  RUSTTY_POINT_TAG_ACTIVE = 0,

  /** Visible viewport (changes when scrolled). */
  RUSTTY_POINT_TAG_VIEWPORT = 1,

  /** Full screen including scrollback. */
  RUSTTY_POINT_TAG_SCREEN = 2,

  /** Scrollback history only (before active area). */
  RUSTTY_POINT_TAG_HISTORY = 3,
  RUSTTY_POINT_TAG_MAX_VALUE = RUSTTY_ENUM_MAX_VALUE,
  } RusttyPointTag;

/**
 * Point value union.
 *
 * @ingroup point
 */
typedef union {
  /** Coordinate (used for all tag variants). */
  RusttyPointCoordinate coordinate;

  /** Padding for ABI compatibility. Do not use. */
  uint64_t _padding[2];
} RusttyPointValue;

/**
 * Tagged union for a point in the terminal grid.
 *
 * @ingroup point
 */
typedef struct {
  RusttyPointTag tag;
  RusttyPointValue value;
} RusttyPoint;

/** @} */

#ifdef __cplusplus
}
#endif

#endif /* RUSTTY_VT_POINT_H */
