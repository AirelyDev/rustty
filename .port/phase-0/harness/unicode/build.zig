const std = @import("std");

// Harness-only build for the rustty-unicode differential oracle (ORACLE #2).
// Builds `uucode-oracle` against uucode 0.2.0 @ 2826a37a with ghostty's
// uucode_config.zig (copied verbatim from the reference pin 20abdb50a). R3: this tooling lives in the harness; no rustty
// Cargo crate depends on uucode/ghostty.
pub fn build(b: *std.Build) void {
    const exe = b.addExecutable(.{
        .name = "uucode-oracle",
        .root_module = b.createModule(.{
            .root_source_file = b.path("uucode_oracle.zig"),
            .target = b.graph.host,
        }),
        .use_llvm = true,
    });

    if (b.lazyDependency("uucode", .{
        .target = b.graph.host,
        .build_config_path = b.path("uucode_config.zig"),
    })) |dep| {
        exe.root_module.addImport("uucode", dep.module("uucode"));
    }

    b.installArtifact(exe);
}
