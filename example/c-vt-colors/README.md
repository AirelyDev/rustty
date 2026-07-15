# Example: `rustty-vt` Terminal Colors

This contains a simple example of how to set default terminal colors,
read effective and default color values, and observe how OSC overrides
layer on top of defaults using the `rustty-vt` C library.

This uses a `build.sh` (invoking `clang`) to build the C program against
`librustty_vt_c`. Rustty emits a standard C library (`librustty_vt_c` +
`include/rustty/vt.h`) that can be used with any C tooling.

## Usage

Run the program:

```shell-session
./build.sh
```
