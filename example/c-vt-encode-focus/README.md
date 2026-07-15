# Example: `rustty-vt` Encode Focus

This contains a simple example of how to use the `rustty-vt` focus
encoding API to encode focus gained/lost events into escape sequences.

This uses a `build.sh` (invoking `clang`) to build the C program against
`librustty_vt_c`. Rustty emits a standard C library (`librustty_vt_c` +
`include/rustty/vt.h`) that can be used with any C tooling.

## Usage

Run the program:

```shell-session
./build.sh
```
