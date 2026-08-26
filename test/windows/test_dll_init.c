/*
 * Minimal reproducer for the rustty-internal DLL CRT initialization issue.
 *
 * PORTS: ghostty test/windows/test_dll_init.c.
 *
 * PORT NOTE — the defect this guards against is Zig-specific, and rustty does
 * not have it. The reference's crash ("access violation writing
 * 0x0000000000000024") came from Zig's `_DllMainCRTStartup` not initializing
 * the MSVC C runtime for DLL targets, which its DllMain had to fix by calling
 * `__vcrt_initialize` / `__acrt_initialize`. rustc emits a normal MSVC DLL
 * entry point, so the CRT is initialized for us.
 *
 * The test is ported anyway, and it is not redundant: what it actually asserts
 * is that `librustty_apprt_c.dll` LOADS at runtime and that a by-value struct
 * return across the C ABI works — the same property the A4/embed-ABI gate
 * covers on macOS, checked on Windows where the calling convention differs.
 *
 * Build:  clang test_dll_init.c -o test_dll_init.exe        (MSVC target)
 *   or:   zig cc test_dll_init.c -o test_dll_init.exe -target native-native-msvc
 * Run:    copy ..\..\target\release\rustty_apprt_c.dll . && test_dll_init.exe
 *
 * Expected output:
 *   rustty_info: <version string>
 */

#include <stdio.h>
#include <windows.h>

/* Mirrors `rustty_info_s` in include/rustty-embed.h. Declared locally rather
 * than including the header so the test exercises the ABI as an outside
 * consumer sees it, exactly as the reference's copy does. */
typedef struct {
    int build_mode;
    const char *version;
    size_t version_len;
} rustty_info_s;

typedef rustty_info_s (*rustty_info_fn)(void);

int main(void) {
    HMODULE dll = LoadLibraryA("rustty_apprt_c.dll");
    if (!dll) {
        fprintf(stderr, "LoadLibrary failed: %lu\n", GetLastError());
        return 1;
    }

    rustty_info_fn info_fn = (rustty_info_fn)GetProcAddress(dll, "rustty_info");
    if (!info_fn) {
        fprintf(stderr, "GetProcAddress(rustty_info) failed: %lu\n", GetLastError());
        return 1;
    }

    rustty_info_s info = info_fn();
    fprintf(stderr, "rustty_info: %.*s\n", (int)info.version_len, info.version);

    /* Skip FreeLibrary -- rustty's global state cleanup and CRT teardown
     * ordering is not yet handled. The OS reclaims everything on process
     * exit. */
    return 0;
}
