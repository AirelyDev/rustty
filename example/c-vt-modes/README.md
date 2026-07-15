# Example: `rustty-vt` Mode Utilities

This contains a simple example of how to use the `rustty-vt` mode
utilities to pack and unpack terminal mode identifiers and encode
DECRPM responses.

This uses a `build.sh` (invoking `clang`) to build the C program against
`librustty_vt_c`. Rustty emits a standard C library (`librustty_vt_c` +
`include/rustty/vt.h`) that can be used with any C tooling.

## Usage

Run the program:

```shell-session
./build.sh
```
