# Example: `rustty-vt` Color Scheme Report Encoding

This contains a simple example of how to use the color scheme report
encoder (`rustty_color_scheme_report_encode`) to encode a CSI ? 997 n
color scheme report escape sequence with the `rustty-vt` C library.

This uses a `build.sh` (invoking `clang`) to build the C program against
`librustty_vt_c`. Rustty emits a standard C library (`librustty_vt_c` +
`include/rustty/vt.h`) that can be used with any C tooling.

## Usage

Run the program:

```shell-session
./build.sh
```
