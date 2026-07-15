# Example: `rustty-vt` Static Linking

This contains a simple example of how to statically link the `rustty-vt`
C library with a C program using the `rustty-vt-static` artifact. It is
otherwise identical to the `c-vt` example.

This uses a `build.sh` (invoking `clang`) to build the C program against
`librustty_vt_c`. Rustty emits a standard C library (`librustty_vt_c` +
`include/rustty/vt.h`) that can be used with any C tooling.

## Usage

Run the program:

```shell-session
./build.sh
```
