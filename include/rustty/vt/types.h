/**
 * @file types.h
 *
 * Common types, macros, and utilities for librustty-vt.
 */

#ifndef RUSTTY_VT_TYPES_H
#define RUSTTY_VT_TYPES_H

#include <limits.h>
#include <stddef.h>
#include <stdint.h>

// Symbol visibility for shared library builds. On Windows, functions
// are exported from the DLL when building and imported when consuming.
// On other platforms with GCC/Clang, functions are marked with default
// visibility so they remain accessible when the library is built with
// -fvisibility=hidden. For static library builds, define RUSTTY_STATIC
// before including this header to make this a no-op.
#ifndef RUSTTY_API
#if defined(RUSTTY_STATIC)
  #define RUSTTY_API
#elif defined(_WIN32) || defined(_WIN64)
  #ifdef RUSTTY_BUILD_SHARED
    #define RUSTTY_API __declspec(dllexport)
  #else
    #define RUSTTY_API __declspec(dllimport)
  #endif
#elif defined(__GNUC__) && __GNUC__ >= 4
  #define RUSTTY_API __attribute__((visibility("default")))
#else
  #define RUSTTY_API
#endif
#endif

/**
 * Enum int-sizing helpers.
 *
 * The Zig side backs all C enums with c_int, so the C declarations
 * must use int as their underlying type to maintain ABI compatibility.
 *
 * C++11 and C23 support explicit enum underlying types with
 * `enum : int { ... }`. Clang and GCC 13+ also support this syntax as
 * an extension in older C language modes, so use it when available.
 *
 * Other pre-C23 C compilers are free to choose any type that can
 * represent all values (C11 §6.7.2.2). For those compilers, we add an
 * INT_MAX sentinel as the last entry so the compatible type must be
 * able to represent INT_MAX. The exact compatible type and its
 * signedness remain implementation-defined in this fallback.
 *
 * INT_MAX is used rather than a fixed constant like 0xFFFFFFFF
 * because enum constants must have type int in pre-C23 C. Values above
 * INT_MAX are a constraint violation there; compilers that accept them
 * may interpret them as negative values via two's complement, which can
 * collide with legitimate negative enum values.
 *
 * Usage:
 * @code
 * typedef enum RUSTTY_ENUM_TYPED {
 *     FOO_A = 0,
 *     FOO_B = 1,
 *     FOO_MAX_VALUE = RUSTTY_ENUM_MAX_VALUE,
 * } Foo;
 * @endcode
 */
#if defined(__cplusplus) && \
    (__cplusplus >= 201103L || (defined(_MSC_VER) && _MSC_VER >= 1700))
#define RUSTTY_ENUM_TYPED : int
#elif defined(__STDC_VERSION__) && __STDC_VERSION__ >= 202311L
#define RUSTTY_ENUM_TYPED : int
#elif defined(__clang__)
  #if __has_extension(c_fixed_enum)
    #define RUSTTY_ENUM_TYPED : int
  #else
    #define RUSTTY_ENUM_TYPED
  #endif
#elif defined(__GNUC__) && __GNUC__ >= 13
#define RUSTTY_ENUM_TYPED : int
#else
#define RUSTTY_ENUM_TYPED
#endif
#define RUSTTY_ENUM_MAX_VALUE INT_MAX

/**
 * Result codes for librustty-vt operations.
 */
typedef enum RUSTTY_ENUM_TYPED {
    /** Operation completed successfully */
    RUSTTY_SUCCESS = 0,
    /** Operation failed due to failed allocation */
    RUSTTY_OUT_OF_MEMORY = -1,
    /** Operation failed due to invalid value */
    RUSTTY_INVALID_VALUE = -2,
    /** Operation failed because the provided buffer was too small */
    RUSTTY_OUT_OF_SPACE = -3,
    /** The requested value has no value */
    RUSTTY_NO_VALUE = -4,
    RUSTTY_RESULT_MAX_VALUE = RUSTTY_ENUM_MAX_VALUE,
} RusttyResult;

/* ---- Opaque handles ---- */

/**
 * Opaque handle to a terminal instance.
 *
 * @ingroup terminal
 */
typedef struct RusttyTerminalImpl* RusttyTerminal;

/**
 * Opaque handle to a tracked grid reference.
 *
 * A tracked grid reference is owned by the caller and must be freed with
 * rustty_tracked_grid_ref_free(). If the terminal that created it is freed
 * first, the handle remains valid only for tracked-grid-ref APIs: it reports no
 * value and can still be freed.
 *
 * @ingroup grid_ref
 */
typedef struct RusttyTrackedGridRefImpl* RusttyTrackedGridRef;

/**
 * Opaque handle to a Kitty graphics image storage.
 *
 * Obtained via rustty_terminal_get() with
 * RUSTTY_TERMINAL_DATA_KITTY_GRAPHICS. The pointer is borrowed from
 * the terminal and remains valid until the next mutating terminal call
 * (e.g. rustty_terminal_vt_write() or rustty_terminal_reset()).
 *
 * @ingroup kitty_graphics
 */
typedef struct RusttyKittyGraphicsImpl* RusttyKittyGraphics;

/**
 * Opaque handle to a Kitty graphics image.
 *
 * Obtained via rustty_kitty_graphics_image() with an image ID. The
 * pointer is borrowed from the storage and remains valid until the next
 * mutating terminal call.
 *
 * @ingroup kitty_graphics
 */
typedef const struct RusttyKittyGraphicsImageImpl* RusttyKittyGraphicsImage;

/**
 * Opaque handle to a Kitty graphics placement iterator.
 *
 * @ingroup kitty_graphics
 */
typedef struct RusttyKittyGraphicsPlacementIteratorImpl* RusttyKittyGraphicsPlacementIterator;

/**
 * Opaque handle to a render state instance.
 *
 * @ingroup render
 */
typedef struct RusttyRenderStateImpl* RusttyRenderState;

/**
 * Opaque handle to a render-state row iterator.
 *
 * @ingroup render
 */
typedef struct RusttyRenderStateRowIteratorImpl* RusttyRenderStateRowIterator;

/**
 * Opaque handle to render-state row cells.
 *
 * @ingroup render
 */
typedef struct RusttyRenderStateRowCellsImpl* RusttyRenderStateRowCells;

/**
 * Opaque handle to an SGR parser instance.
 *
 * This handle represents an SGR (Select Graphic Rendition) parser that can
 * be used to parse SGR sequences and extract individual text attributes.
 *
 * @ingroup sgr
 */
typedef struct RusttySgrParserImpl* RusttySgrParser;

/**
 * Opaque handle to a formatter instance.
 *
 * @ingroup formatter
 */
typedef struct RusttyFormatterImpl* RusttyFormatter;

/**
 * Opaque handle to an OSC parser instance.
 *
 * This handle represents an OSC (Operating System Command) parser that can
 * be used to parse the contents of OSC sequences.
 *
 * @ingroup osc
 */
typedef struct RusttyOscParserImpl* RusttyOscParser;

/**
 * Opaque handle to a single OSC command.
 *
 * This handle represents a parsed OSC (Operating System Command) command.
 * The command can be queried for its type and associated data.
 *
 * @ingroup osc
 */
typedef struct RusttyOscCommandImpl* RusttyOscCommand;

/* ---- Common value types ---- */

/**
 * Terminal content output format.
 *
 * @ingroup formatter
 */
typedef enum RUSTTY_ENUM_TYPED {
  /** Plain text (no escape sequences). */
  RUSTTY_FORMATTER_FORMAT_PLAIN,

  /** VT sequences preserving colors, styles, URLs, etc. */
  RUSTTY_FORMATTER_FORMAT_VT,

  /** HTML with inline styles. */
  RUSTTY_FORMATTER_FORMAT_HTML,
  RUSTTY_FORMATTER_FORMAT_MAX_VALUE = RUSTTY_ENUM_MAX_VALUE,
} RusttyFormatterFormat;

/**
 * A borrowed byte string (pointer + length).
 *
 * The memory is not owned by this struct. The pointer is only valid
 * for the lifetime documented by the API that produces or consumes it.
 */
typedef struct {
  /** Pointer to the string bytes. */
  const uint8_t* ptr;

  /** Length of the string in bytes. */
  size_t len;
} RusttyString;

/**
 * A caller-provided byte buffer.
 *
 * APIs that write to this type use `len` for the number of bytes written on
 * RUSTTY_SUCCESS and the required byte capacity on RUSTTY_OUT_OF_SPACE.
 */
typedef struct {
  /** Destination buffer for bytes. May be NULL when cap is 0 to query required size. */
  uint8_t* ptr;

  /** Capacity of ptr in bytes. */
  size_t cap;

  /** Bytes written on success, or required byte capacity on RUSTTY_OUT_OF_SPACE. */
  size_t len;
} RusttyBuffer;

/**
 * A surface-space position in pixels.
 *
 * This is not a terminal grid coordinate. It represents an x/y position in the
 * rendered surface coordinate space, with (0, 0) at the top-left of the
 * surface.
 */
typedef struct {
  /** X position in surface pixels. */
  double x;

  /** Y position in surface pixels. */
  double y;
} RusttySurfacePosition;

/**
 * A borrowed list of Unicode scalar values.
 *
 * Values are encoded as uint32_t scalar values. The memory is not owned by this
 * struct. The pointer is only valid for the lifetime documented by the API that
 * consumes or produces it.
 *
 * APIs may document special handling for NULL + len 0, such as “use defaults”.
 */
typedef struct {
  /** Pointer to Unicode scalar values. */
  const uint32_t* ptr;

  /** Number of entries in ptr. */
  size_t len;
} RusttyCodepoints;

/**
 * Initialize a sized struct to zero and set its size field.
 *
 * Sized structs use a `size` field as the first member for ABI
 * compatibility. This macro zero-initializes the struct and sets the
 * size field to `sizeof(type)`, which allows the library to detect
 * which version of the struct the caller was compiled against.
 *
 * @param type The struct type to initialize
 * @return A zero-initialized struct with the size field set
 *
 * Example:
 * @code
 * RusttyFormatterTerminalOptions opts = RUSTTY_INIT_SIZED(RusttyFormatterTerminalOptions);
 * opts.emit = RUSTTY_FORMATTER_FORMAT_PLAIN;
 * opts.trim = true;
 * @endcode
 */
#ifdef __cplusplus
#define RUSTTY_INIT_SIZED(type)                                      \
  ([]() noexcept {                                                    \
    type value{};                                                     \
    value.size = sizeof(value);                                       \
    return value;                                                     \
  }())
#else
#define RUSTTY_INIT_SIZED(type) \
  ((type){ .size = sizeof(type) })
#endif

/**
 * Return the versioned librustty-vt C type manifest for the current target.
 *
 * The manifest defines all the public types available in the linked
 * build. The types contain their layouts, enum values, union fields, and more.
 *
 * Language bindings, such as WebAssembly hosts, should obtain offsets,
 * sizes, alignments, array shapes, enum constants, and tagged-union arms from
 * this manifest rather than hardcoding them. Consumers should reject unknown
 * schema versions and verify the descriptors they require at initialization.
 *
 * Packed type descriptors define fields using `lsb` and `width`. `lsb` is
 * relative to bit zero of the containing numerical value; for nested packed
 * layouts it is relative to the immediate containing field. Tagged packed
 * unions select an inline arm layout using the named tag field. These layouts
 * describe the current linked build and are not a cross-version stability
 * promise.
 *
 * The formal format is defined by the
 * <a href="types.schema.json">librustty-vt ABI manifest JSON Schema</a>.
 *
 * Example (abbreviated):
 * @code{.json}
 * {
 *   "schema": 1,
 *   "abi": {
 *     "target": "wasm32", "os": "freestanding", "environment": "none",
 *     "pointer_size": 4, "usize_size": 4, "max_alignment": 16,
 *     "endian": "little"
 *   },
 *   "types": {
 *     "RusttyRenderStateData": {
 *       "kind": "enum", "size": 4, "align": 4,
 *       "underlying": "i32", "prefix": "RUSTTY_RENDER_STATE_DATA_",
 *       "values": { "INVALID": 0, "DIRTY": 3, "MAX_VALUE": 2147483647 }
 *     },
 *     "RusttyStyleColor": {
 *       "kind": "struct", "size": 16, "align": 8,
 *       "fields": {
 *         "tag": { "offset": 0, "size": 4,
 *                  "type": "RusttyStyleColorTag" },
 *         "value": { "offset": 8, "size": 8,
 *                    "type": "RusttyStyleColorValue", "tag": "tag",
 *                    "arms": { "NONE": null, "PALETTE": "palette",
 *                              "RGB": "rgb" } }
 *       }
 *     }
 *   }
 * }
 * @endcode
 *
 * The returned pointer is valid for the lifetime of the process.
 *
 * @return Pointer to the null-terminated JSON string.
 */
RUSTTY_API const char *rustty_type_json(void);

#endif /* RUSTTY_VT_TYPES_H */
