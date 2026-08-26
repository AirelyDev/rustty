# Windows Tests

Manual test programs for Windows-specific functionality.

PORTS: ghostty `test/windows/`.

## test_dll_init.c

Loads `rustty_apprt_c.dll` at runtime and calls `rustty_info` to verify the
DLL loads and that a by-value struct return across the C ABI works.

The reference's version of this test guards a Zig-specific defect (its
`_DllMainCRTStartup` did not initialize the MSVC C runtime for DLL targets).
rustc emits a normal MSVC DLL entry point, so rustty never had that bug — see
the PORT NOTE at the top of the source for why the test is still worth having.

### Build

First build the DLL, then compile the test:

```
cargo build --release -p rustty-apprt-c
clang test_dll_init.c -o test_dll_init.exe
```

### Run

```
copy ..\..\target\release\rustty_apprt_c.dll .
test_dll_init.exe
```

Expected output:

```
rustty_info: 0.1.0
```

### Status

Not run by any gate. `G-WINCHK` (`.port/tools/gates/w2f001_windows_check_gate.sh`)
covers Windows at `cargo check` level from a non-Windows host; actually running
this needs a Windows machine or runner, which the project does not have yet.
