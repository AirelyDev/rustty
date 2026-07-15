# Example: VT Stream Processing in C

This contains a simple example of how to use `rustty_terminal_vt_write`
to parse and process VT sequences in C. This is the C equivalent of
the `zig-vt-stream` example, ideal for read-only terminal applications
such as replay tooling, CI log viewers, and PaaS builder output.

This uses a `build.sh` (invoking `clang`) to build the C program against
`librustty_vt_c`. Rustty emits a standard C library (`librustty_vt_c` +
`include/rustty/vt.h`) that can be used with any C tooling.

## Usage

Run the program:

```shell-session
./build.sh
```
