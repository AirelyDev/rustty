/* rustty-os: os_log_with_type is a C MACRO (it captures a compile-time format
 * string), so it cannot be bound over FFI directly. This shim is the rustty
 * analogue of ghostty's `zig_os_log_with_type` C helper (pkg/macos): it fixes
 * the format to "%{public}s" and forwards a pre-formatted message, exactly what
 * pkg/macos/os/log.zig's `Log.log` does (allocPrint then forward). */
#include <os/log.h>
#include <stdint.h>

void rustty_os_log_with_type(void *log, uint8_t type, const char *msg) {
    os_log_with_type((os_log_t)log, (os_log_type_t)type, "%{public}s", msg);
}
