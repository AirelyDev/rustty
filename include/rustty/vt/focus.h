/**
 * @file focus.h
 *
 * Focus encoding - encode focus in/out events into terminal escape sequences.
 */

#ifndef RUSTTY_VT_FOCUS_H
#define RUSTTY_VT_FOCUS_H

/** @defgroup focus Focus Encoding
 *
 * Utilities for encoding focus gained/lost events into terminal escape
 * sequences (CSI I / CSI O) for focus reporting mode (mode 1004).
 *
 * ## Basic Usage
 *
 * Use rustty_focus_encode() to encode a focus event into a caller-provided
 * buffer. If the buffer is too small, the function returns
 * RUSTTY_OUT_OF_SPACE and sets the required size in the output parameter.
 *
 * ## Example
 *
 * @snippet c-vt-encode-focus/src/main.c focus-encode
 *
 * @{
 */

#include <stddef.h>
#include <rustty/vt/types.h>

#ifdef __cplusplus
extern "C" {
#endif

/**
 * Focus event types for focus reporting mode (mode 1004).
 */
typedef enum RUSTTY_ENUM_TYPED {
    /** Terminal window gained focus */
    RUSTTY_FOCUS_GAINED = 0,
    /** Terminal window lost focus */
    RUSTTY_FOCUS_LOST = 1,
    RUSTTY_FOCUS_MAX_VALUE = RUSTTY_ENUM_MAX_VALUE,
} RusttyFocusEvent;

/**
 * Encode a focus event into a terminal escape sequence.
 *
 * Encodes a focus gained (CSI I) or focus lost (CSI O) report into the
 * provided buffer.
 *
 * If the buffer is too small, the function returns RUSTTY_OUT_OF_SPACE
 * and writes the required buffer size to @p out_written. The caller can
 * then retry with a sufficiently sized buffer.
 *
 * @param event The focus event to encode
 * @param buf Output buffer to write the encoded sequence into (may be NULL)
 * @param buf_len Size of the output buffer in bytes
 * @param[out] out_written On success, the number of bytes written. On
 *             RUSTTY_OUT_OF_SPACE, the required buffer size.
 * @return RUSTTY_SUCCESS on success, RUSTTY_OUT_OF_SPACE if the buffer
 *         is too small
 */
RUSTTY_API RusttyResult rustty_focus_encode(
    RusttyFocusEvent event,
    char* buf,
    size_t buf_len,
    size_t* out_written);

#ifdef __cplusplus
}
#endif

/** @} */

#endif /* RUSTTY_VT_FOCUS_H */
