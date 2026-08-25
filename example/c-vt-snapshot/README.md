# Example: Terminal Snapshots in C

This example creates a terminal with continuation tracking, encodes its full
state, and restores the snapshot using both the one-shot and incremental C
decoder APIs. The incremental path uses a synchronous `RusttyReader` callback
and reports each restored history page.

This uses a `build.sh` (invoking `clang`) to build the C program against
`librustty_vt_c`. Rustty emits a standard C library (`librustty_vt_c` +
`include/rustty/vt.h`) that can be used with any C tooling.

## Usage

Run the example:

```shell-session
./build.sh
```
