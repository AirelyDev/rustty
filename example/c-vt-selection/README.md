# Example: `rustty-vt` Selection

This contains a simple example of how to use the `rustty-vt` terminal,
grid reference, selection, and formatter APIs to derive selections such as a
word, semantic command line, command output, and all visible content.

This uses a `build.sh` (invoking `clang`) to build the C program against
`librustty_vt_c`. Rustty emits a standard C library (`librustty_vt_c` +
`include/rustty/vt.h`) that can be used with any C tooling.

## Usage

Run the program:

```shell-session
./build.sh
```
