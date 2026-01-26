const std = @import("std");

pub fn build(b: *std.Build) void {
    // ReleaseSmall is crucial for the <1MB requirement
    const target = b.standardTargetOptions(.{});
    const optimize = b.standardOptimizeOption(.{ .preferred_optimize_mode = .ReleaseSmall });

    const exe = b.addExecutable(.{
        .name = "lumina-sentinel",
        .root_source_file = b.path("main.zig"),
        .target = target,
        .optimize = optimize,
    });
    // For older Zig versions or specific environments where root_module is tricky in init
    exe.root_module.strip = true; 

    b.installArtifact(exe);
}
