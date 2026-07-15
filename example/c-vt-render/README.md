# Example: `rustty-vt` Render State

This contains an example of how to use the `rustty-vt` render-state API
to create a render state, update it from terminal content, iterate rows
and cells, read styles and colors, inspect cursor and row-local selection
state, and manage dirty tracking.

This uses a `build.sh` (invoking `clang`) to build the C program against
`librustty_vt_c`. Rustty emits a standard C library (`librustty_vt_c` +
`include/rustty/vt.h`) that can be used with any C tooling.

## Usage

Run the program:

```shell-session
./build.sh
```
