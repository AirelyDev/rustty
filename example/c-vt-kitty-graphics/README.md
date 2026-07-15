# Example: `rustty-vt` Kitty Graphics Protocol

This contains a simple example of how to use the system interface
(`rustty_sys_set`) to install a PNG decoder callback, then send
a Kitty Graphics Protocol image via `rustty_terminal_vt_write`.

This uses a `build.sh` (invoking `clang`) to build the C program against
`librustty_vt_c`. Rustty emits a standard C library (`librustty_vt_c` +
`include/rustty/vt.h`) that can be used with any C tooling.

## Usage

Run the program:

```shell-session
./build.sh
```
