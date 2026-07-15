# Example: `ghostty-vt` Kitty Graphics Protocol

This contains a simple example of how to use the system interface
(`rustty_sys_set`) to install a PNG decoder callback, then send
a Kitty Graphics Protocol image via `rustty_terminal_vt_write`.

This uses a `build.zig` and `Zig` to build the C program so that we
can reuse a lot of our build logic and depend directly on our source
tree, but Rustty emits a standard C library that can be used with any
C tooling.

## Usage

Run the program:

```shell-session
./build.sh
```
