const std = @import("std");

pub fn build(b: *std.Build) void {
    const target = b.standardTargetOptions(.{});
    const optimize = b.standardOptimizeOption(.{});

    const ghostty_src = "/Users/gael/projects/ghostty/src";

    const exe = b.addExecutable(.{
        .name = "zigbench",
        .root_module = b.createModule(.{
            .root_source_file = b.path("src/main.zig"),
            .target = target,
            .optimize = optimize,
        }),
    });

    const terminal = b.createModule(.{
        .root_source_file = .{ .cwd_relative = ghostty_src ++ "/lib_vt.zig" },
        .target = target,
        .optimize = optimize,
    });

    terminal.addOptions("terminal_options", buildTerminalOptions(b));
    exe.root_module.addImport("terminal", terminal);

    b.installArtifact(exe);

    const run_cmd = b.addRunArtifact(exe);
    if (b.args) |args| run_cmd.addArgs(args);
    const run_step = b.step("run", "Run the bench");
    run_step.dependOn(&run_cmd.step);
}

fn buildTerminalOptions(b: *std.Build) *std.Build.Step.Options {
    const Artifact = enum { ghostty, lib };
    const opts = b.addOptions();
    opts.addOption(Artifact, "artifact", .lib);
    opts.addOption(bool, "c_abi", false);
    opts.addOption(bool, "oniguruma", false);
    opts.addOption(bool, "simd", false);
    opts.addOption(bool, "slow_runtime_safety", false);
    opts.addOption(bool, "kitty_graphics", true);
    opts.addOption(bool, "tmux_control_mode", false);
    opts.addOption([]const u8, "version_string", "1.3.2-dev");
    opts.addOption(usize, "version_major", 1);
    opts.addOption(usize, "version_minor", 3);
    opts.addOption(usize, "version_patch", 2);
    opts.addOption(?[]const u8, "version_pre", "dev");
    opts.addOption(?[]const u8, "version_build", null);
    return opts;
}
