# Example: `rustty-vt` Selection Gestures

This contains a simple example of how to use the `rustty-vt` selection
gesture API from C. It creates synthetic press, drag, release, and deep-press
events and formats the resulting selection snapshots.

This uses a `build.sh` (invoking `clang`) to build the C program against
`librustty_vt_c`. Rustty emits a standard C library (`librustty_vt_c` +
`include/rustty/vt.h`) that can be used with any C tooling.

## Usage

Run the program:

```shell-session
./build.sh
```
