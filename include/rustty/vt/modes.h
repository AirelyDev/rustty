/**
 * @file modes.h
 *
 * Terminal mode utilities - pack and unpack ANSI/DEC mode identifiers.
 */

#ifndef RUSTTY_VT_MODES_H
#define RUSTTY_VT_MODES_H

/** @defgroup modes Mode Utilities
 *
 * Utilities for working with terminal modes. A mode is a compact
 * 16-bit representation of a terminal mode identifier that encodes both
 * the numeric mode value (up to 15 bits) and whether the mode is an ANSI
 * mode or a DEC private mode (?-prefixed).
 *
 * The packed layout (least-significant bit first) is:
 * - Bits 0–14: mode value (u15)
 * - Bit 15: ANSI flag (0 = DEC private mode, 1 = ANSI mode)
 *
 * ## Example
 *
 * @snippet c-vt-modes/src/main.c modes-pack-unpack
 *
 * ## DECRPM Report Encoding
 *
 * Use rustty_mode_report_encode() to encode a DECRPM response into a
 * caller-provided buffer:
 *
 * @snippet c-vt-modes/src/main.c modes-decrpm
 *
 * @{
 */

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#include <rustty/vt/types.h>

#ifdef __cplusplus
extern "C" {
#endif

/** @name ANSI Modes
 * Modes for standard ANSI modes.
 * @{
 */
#define RUSTTY_MODE_KAM              (rustty_mode_new(2, true))    /**< Keyboard action (disable keyboard) */
#define RUSTTY_MODE_INSERT           (rustty_mode_new(4, true))    /**< Insert mode */
#define RUSTTY_MODE_SRM              (rustty_mode_new(12, true))   /**< Send/receive mode */
#define RUSTTY_MODE_LINEFEED         (rustty_mode_new(20, true))   /**< Linefeed/new line mode */
/** @} */

/** @name DEC Private Modes
 * Modes for DEC private modes (?-prefixed).
 * @{
 */
#define RUSTTY_MODE_DECCKM           (rustty_mode_new(1, false))   /**< Cursor keys */
#define RUSTTY_MODE_132_COLUMN       (rustty_mode_new(3, false))   /**< 132/80 column mode */
#define RUSTTY_MODE_SLOW_SCROLL      (rustty_mode_new(4, false))   /**< Slow scroll */
#define RUSTTY_MODE_REVERSE_COLORS   (rustty_mode_new(5, false))   /**< Reverse video */
#define RUSTTY_MODE_ORIGIN           (rustty_mode_new(6, false))   /**< Origin mode */
#define RUSTTY_MODE_WRAPAROUND       (rustty_mode_new(7, false))   /**< Auto-wrap mode */
#define RUSTTY_MODE_AUTOREPEAT       (rustty_mode_new(8, false))   /**< Auto-repeat keys */
#define RUSTTY_MODE_X10_MOUSE        (rustty_mode_new(9, false))   /**< X10 mouse reporting */
#define RUSTTY_MODE_CURSOR_BLINKING  (rustty_mode_new(12, false))  /**< Cursor blink */
#define RUSTTY_MODE_CURSOR_VISIBLE   (rustty_mode_new(25, false))  /**< Cursor visible (DECTCEM) */
#define RUSTTY_MODE_ENABLE_MODE_3    (rustty_mode_new(40, false))  /**< Allow 132 column mode */
#define RUSTTY_MODE_REVERSE_WRAP     (rustty_mode_new(45, false))  /**< Reverse wrap */
#define RUSTTY_MODE_ALT_SCREEN_LEGACY (rustty_mode_new(47, false)) /**< Alternate screen (legacy) */
#define RUSTTY_MODE_KEYPAD_KEYS      (rustty_mode_new(66, false))  /**< Application keypad */
#define RUSTTY_MODE_BACKARROW_KEY_MODE (rustty_mode_new(67, false))  /**< Backarrow key mode (DECBKM) */
#define RUSTTY_MODE_LEFT_RIGHT_MARGIN (rustty_mode_new(69, false)) /**< Left/right margin mode */
#define RUSTTY_MODE_NORMAL_MOUSE     (rustty_mode_new(1000, false)) /**< Normal mouse tracking */
#define RUSTTY_MODE_BUTTON_MOUSE     (rustty_mode_new(1002, false)) /**< Button-event mouse tracking */
#define RUSTTY_MODE_ANY_MOUSE        (rustty_mode_new(1003, false)) /**< Any-event mouse tracking */
#define RUSTTY_MODE_FOCUS_EVENT      (rustty_mode_new(1004, false)) /**< Focus in/out events */
#define RUSTTY_MODE_UTF8_MOUSE       (rustty_mode_new(1005, false)) /**< UTF-8 mouse format */
#define RUSTTY_MODE_SGR_MOUSE        (rustty_mode_new(1006, false)) /**< SGR mouse format */
#define RUSTTY_MODE_ALT_SCROLL       (rustty_mode_new(1007, false)) /**< Alternate scroll mode */
#define RUSTTY_MODE_URXVT_MOUSE      (rustty_mode_new(1015, false)) /**< URxvt mouse format */
#define RUSTTY_MODE_SGR_PIXELS_MOUSE (rustty_mode_new(1016, false)) /**< SGR-Pixels mouse format */
#define RUSTTY_MODE_NUMLOCK_KEYPAD   (rustty_mode_new(1035, false)) /**< Ignore keypad with NumLock */
#define RUSTTY_MODE_ALT_ESC_PREFIX   (rustty_mode_new(1036, false)) /**< Alt key sends ESC prefix */
#define RUSTTY_MODE_ALT_SENDS_ESC    (rustty_mode_new(1039, false)) /**< Alt sends escape */
#define RUSTTY_MODE_REVERSE_WRAP_EXT (rustty_mode_new(1045, false)) /**< Extended reverse wrap */
#define RUSTTY_MODE_ALT_SCREEN       (rustty_mode_new(1047, false)) /**< Alternate screen */
#define RUSTTY_MODE_SAVE_CURSOR      (rustty_mode_new(1048, false)) /**< Save cursor (DECSC) */
#define RUSTTY_MODE_ALT_SCREEN_SAVE  (rustty_mode_new(1049, false)) /**< Alt screen + save cursor + clear */
#define RUSTTY_MODE_BRACKETED_PASTE  (rustty_mode_new(2004, false)) /**< Bracketed paste mode */
#define RUSTTY_MODE_SYNC_OUTPUT      (rustty_mode_new(2026, false)) /**< Synchronized output */
#define RUSTTY_MODE_GRAPHEME_CLUSTER (rustty_mode_new(2027, false)) /**< Grapheme cluster mode */
#define RUSTTY_MODE_COLOR_SCHEME_REPORT (rustty_mode_new(2031, false)) /**< Report color scheme */
#define RUSTTY_MODE_VISIBILITY_REPORT (rustty_mode_new(2033, false)) /**< Report terminal visibility */
#define RUSTTY_MODE_IN_BAND_RESIZE   (rustty_mode_new(2048, false)) /**< In-band size reports */
#define RUSTTY_MODE_PASTE_EVENTS     (rustty_mode_new(5522, false)) /**< Kitty clipboard protocol paste events */
/** @} */

/**
 * A packed 16-bit terminal mode.
 *
 * Encodes a mode value (bits 0–14) and an ANSI flag (bit 15) into a
 * single 16-bit integer. Use the inline helper functions to construct
 * and inspect modes rather than manipulating bits directly.
 */
typedef uint16_t RusttyMode;

/**
 * Create a mode from a mode value and ANSI flag.
 *
 * @param value The numeric mode value (0–32767)
 * @param ansi true for an ANSI mode, false for a DEC private mode
 * @return The packed mode
 *
 * @ingroup modes
 */
static inline RusttyMode rustty_mode_new(uint16_t value, bool ansi) {
    return (RusttyMode)((value & 0x7FFF) | ((uint16_t)ansi << 15));
}

/**
 * Extract the numeric mode value from a mode.
 *
 * @param mode The mode
 * @return The mode value (0–32767)
 *
 * @ingroup modes
 */
static inline uint16_t rustty_mode_value(RusttyMode mode) {
    return mode & 0x7FFF;
}

/**
 * Check whether a mode represents an ANSI mode.
 *
 * @param mode The mode
 * @return true if this is an ANSI mode, false if it is a DEC private mode
 *
 * @ingroup modes
 */
static inline bool rustty_mode_ansi(RusttyMode mode) {
    return (mode >> 15) != 0;
}

/**
 * DECRPM report state values.
 *
 * These correspond to the Ps2 parameter in a DECRPM response
 * sequence (CSI ? Ps1 ; Ps2 $ y).
 */
typedef enum RUSTTY_ENUM_TYPED {
    /** Mode is not recognized */
    RUSTTY_MODE_REPORT_NOT_RECOGNIZED = 0,
    /** Mode is set (enabled) */
    RUSTTY_MODE_REPORT_SET = 1,
    /** Mode is reset (disabled) */
    RUSTTY_MODE_REPORT_RESET = 2,
    /** Mode is permanently set */
    RUSTTY_MODE_REPORT_PERMANENTLY_SET = 3,
    /** Mode is permanently reset */
    RUSTTY_MODE_REPORT_PERMANENTLY_RESET = 4,
    RUSTTY_MODE_REPORT_MAX_VALUE = RUSTTY_ENUM_MAX_VALUE,
} RusttyModeReportState;

/**
 * Encode a DECRPM (DEC Private Mode Report) response sequence.
 *
 * Writes a mode report escape sequence into the provided buffer.
 * The generated sequence has the form:
 * - DEC private mode: CSI ? Ps1 ; Ps2 $ y
 * - ANSI mode:        CSI Ps1 ; Ps2 $ y
 *
 * If the buffer is too small, the function returns RUSTTY_OUT_OF_SPACE
 * and writes the required buffer size to @p out_written. The caller can
 * then retry with a sufficiently sized buffer.
 *
 * @param mode The mode identifying the mode to report on
 * @param state The report state for this mode
 * @param buf Output buffer to write the encoded sequence into (may be NULL)
 * @param buf_len Size of the output buffer in bytes
 * @param[out] out_written On success, the number of bytes written. On
 *             RUSTTY_OUT_OF_SPACE, the required buffer size.
 * @return RUSTTY_SUCCESS on success, RUSTTY_OUT_OF_SPACE if the buffer
 *         is too small
 */
RUSTTY_API RusttyResult rustty_mode_report_encode(
    RusttyMode mode,
    RusttyModeReportState state,
    char* buf,
    size_t buf_len,
    size_t* out_written);

#ifdef __cplusplus
}
#endif

/** @} */

#endif /* RUSTTY_VT_MODES_H */
