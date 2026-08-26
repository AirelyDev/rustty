# swift-vt-xcframework

Demonstrates consuming librustty-vt from a Swift Package using the pre-built
XCFramework. Creates a terminal, writes VT sequences into it, and formats the
screen contents as plain text.

This example requires the XCFramework to be built first.

## Building

First, build the XCFramework from the repository root:

```shell-session
cargo run -p xtask -- xcframework-vt
```

Then build and run the Swift package:

```shell-session
cd example/swift-vt-xcframework
swift build
swift run
```

## Notes

The Swift module is `RusttyVt`, declared by the clang module map `xtask`
stages into the framework's `Headers/`. The C symbols keep their canonical
`rustty_*` spelling — Swift imports the module, not a Swift-native wrapper.

Unlike the `c-`/`cpp-` examples this directory has no `build.sh`: SwiftPM is
what it demonstrates. `.port/tools/gates/check_examples.sh` builds and runs it
on macOS as part of the `G-EXAMPLES` gate, and reports it skipped elsewhere.
