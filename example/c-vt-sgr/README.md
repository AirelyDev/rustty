# Example: `rustty-vt` SGR Parser

This contains a simple example of how to use the `rustty-vt` SGR parser
to parse terminal styling sequences and extract text attributes.

This example demonstrates parsing a complex SGR sequence from Kakoune that
includes curly underline, RGB foreground/background colors, and RGB underline
color with mixed semicolon and colon separators.

This uses a `build.sh` (invoking `clang`) to build the C program against
`librustty_vt_c`. Rustty emits a standard C library (`librustty_vt_c` +
`include/rustty/vt.h`) that can be used with any C tooling.

## Usage

Run the program:

```shell-session
./build.sh
```
