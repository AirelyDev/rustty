# Example: `rustty-vt` Grid Traversal

This contains a simple example of how to use the `rustty-vt` terminal and
grid reference APIs to create a terminal, write content into it, and then
traverse the entire grid cell-by-cell using grid refs to inspect codepoints,
row state, and styles.

This uses a `build.sh` (invoking `clang`) to build the C program against
`librustty_vt_c`. Rustty emits a standard C library (`librustty_vt_c` +
`include/rustty/vt.h`) that can be used with any C tooling.

## Usage

Run the program:

```shell-session
./build.sh
```
