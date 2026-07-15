# Example: `rustty-vt` Tracked Grid References

This contains a simple example of how to use the `rustty-vt` terminal and
tracked grid reference APIs to keep a long-lived reference to a cell as the
terminal scrolls, detect when that reference loses its meaningful location,
and move the same tracked handle to a new point.

This uses a `build.sh` (invoking `clang`) to build the C program against
`librustty_vt_c`. Rustty emits a standard C library (`librustty_vt_c` +
`include/rustty/vt.h`) that can be used with any C tooling.

## Usage

Run the program:

```shell-session
./build.sh
```
