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

**Run by CI**, in the `windows` job of `.github/workflows/ci.yml`
(`windows-latest`), which builds `rustty_apprt_c.dll`, compiles this file with
`cl`, runs it, and asserts the output contains `rustty_info:` — running it
without checking what it printed would pass on a DLL that loaded and did
nothing.

Before that job existed this was run by nothing, and no Windows object code was
produced, linked, or executed anywhere in the repository:
`G-WINCHK` (`.port/tools/gates/w2f001_windows_check_gate.sh`) is `cargo check`
from a non-Windows host, and `G-XTARGET`'s `x86_64-pc-windows-msvc` arm skips
off Windows because there is no MSVC to link with. The same job also runs the
`rustty-vt` and `rustty-vt-c` test suites on Windows — the first execution of
the `page_allocator` (Nt{Allocate,Free}VirtualMemory) and `page.rs`
(VirtualAlloc/VirtualFree) arms on the platform they were written for — and
builds for the GNU ABI, the reference's `build-ghostty-windows-gnu`.
