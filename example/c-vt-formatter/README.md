# Example: `rustty-vt` Terminal Formatter

This contains a simple example of how to use the `rustty-vt` terminal and
formatter APIs to create a terminal, write VT-encoded content into it, and
format the screen contents as plain text.

This uses a `build.sh` (invoking `clang`) to build the C program against
`librustty_vt_c`. Rustty emits a standard C library (`librustty_vt_c` +
`include/rustty/vt.h`) that can be used with any C tooling.

## Usage

Run the program:

```shell-session
./build.sh
```
