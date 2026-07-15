# Example: `rustty-vt` Terminal Effects

This contains a simple example of how to register and use terminal
effect callbacks (`write_pty`, `bell`, `title_changed`) with the
`rustty-vt` C library.

This uses a `build.sh` (invoking `clang`) to build the C program against
`librustty_vt_c`. Rustty emits a standard C library (`librustty_vt_c` +
`include/rustty/vt.h`) that can be used with any C tooling.

## Usage

Run the program:

```shell-session
./build.sh
```
