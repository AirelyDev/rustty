# Example: Scrollback Compression in C

This example shows how a librustty-vt embedding application can track
compression-relevant terminal activity and perform incremental scrollback
compression after its own idle delay. It also reads the terminal's memory
usage before and after compression to show how much memory was released.

librustty-vt does not create a timer or background thread. The embedding
application remains responsible for scheduling compression and serializing it
with other access to the terminal.

This uses a `build.sh` (invoking `clang`) to build the C program against
`librustty_vt_c`. Rustty emits a standard C library (`librustty_vt_c` +
`include/rustty/vt.h`) that can be used with any C tooling.

## Usage

Run the example:

```shell-session
./build.sh
```
