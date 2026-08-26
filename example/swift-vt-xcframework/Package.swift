// swift-tools-version: 5.9
import PackageDescription

// PORTS: ghostty example/swift-vt-xcframework/Package.swift.
//
// The binaryTarget path differs: ghostty's XCFramework lands in
// `zig-out/lib/ghostty-vt.xcframework` because `zig build` owns that tree.
// rustty has no zig-out for build products — `cargo run -p xtask --
// xcframework-vt` writes `dist/rustty-vt.xcframework`, alongside the
// RusttyKit.xcframework the macOS app consumes.
let package = Package(
    name: "swift-vt-xcframework",
    platforms: [.macOS(.v13)],
    targets: [
        .executableTarget(
            name: "swift-vt-xcframework",
            dependencies: ["RusttyVt"],
            path: "Sources"
        ),
        .binaryTarget(
            name: "RusttyVt",
            path: "../../dist/rustty-vt.xcframework"
        ),
    ]
)
