const std = @import("std");
pub fn build(b: *std.Build) void {
    const dep = b.lazyDependency("uucode", .{
        .target = b.graph.host,
        .build_config_path = b.path("uucode_config.zig"),
    }) orelse return;
    const exe = b.addExecutable(.{
        .name = "vaxis-probe",
        .root_module = b.createModule(.{
            .root_source_file = b.path("probe.zig"),
            .target = b.graph.host,
        }),
        .use_llvm = true,
    });
    exe.root_module.addImport("uucode", dep.module("uucode"));
    b.installArtifact(exe);
}
