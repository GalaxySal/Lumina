const std = @import("std");

pub fn build(b: *std.Build) void {
    // ReleaseSmall is crucial for the <1MB requirement
    const target = b.standardTargetOptions(.{});
    const optimize = b.standardOptimizeOption(.{ .preferred_optimize_mode = .ReleaseSmall });

    const exe = b.addExecutable(.{
        .name = "lumina-sentinel",
        .root_module = b.createModule(.{
            .root_source_file = b.path("main.zig"),
            .target = target,
            .optimize = optimize,
            .strip = true, // Remove symbols to reduce size
        }),
    });

    b.installArtifact(exe);
}
