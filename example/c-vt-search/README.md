# Example: `rustty-vt` Terminal Search

This contains a simple example of how to use the `rustty-vt` search
API from C. It writes content into a terminal, searches it for a
string, navigates between the matches like a find bar, and reads the
viewport matches an embedder would use to draw highlights.

This uses a `build.sh` (invoking `clang`) to build the C program against
`librustty_vt_c`. Rustty emits a standard C library (`librustty_vt_c` +
`include/rustty/vt.h`) that can be used with any C tooling.

## Usage

Run the program:

```shell-session
./build.sh
```
