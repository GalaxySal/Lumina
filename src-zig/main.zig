const std = @import("std");
const windows = std.os.windows;

// --- Win32 Definitions ---
// Zig Standard Library might have some, but defining explicitly ensures control.
const MEMORYSTATUSEX = extern struct {
    dwLength: u32,
    dwMemoryLoad: u32,
    ullTotalPhys: u64,
    ullAvailPhys: u64,
    ullTotalPageFile: u64,
    ullAvailPageFile: u64,
    ullTotalVirtual: u64,
    ullAvailVirtual: u64,
    ullAvailExtendedVirtual: u64,
};

extern "kernel32" fn GlobalMemoryStatusEx(lpBuffer: *MEMORYSTATUSEX) callconv(.winapi) c_int;
// -------------------------

// Lumina Sentinel [v0.2] - Purebred System Guardian
// -----------------------------------------------

pub fn main() !void {
    const stdout_file = std.io.getStdOut().writer();
    var bw = std.io.bufferedWriter(stdout_file);
    const stdout = bw.writer();

    defer bw.flush() catch {};
    
    // Print Banner
    try stdout.print("\nLUMA SENTINEL [v0.2] - Purebred System Guardian\n", .{});
    try stdout.print("-----------------------------------------------\n\n", .{});

    // Parse Arguments
    var gpa = std.heap.GeneralPurposeAllocator(.{}){};
    defer _ = gpa.deinit();
    const allocator = gpa.allocator();

    const args = try std.process.argsAlloc(allocator);
    defer std.process.argsFree(allocator, args);

    if (args.len < 2) {
        try printUsage(stdout);
        return;
    }

    const command = args[1];

    if (std.mem.eql(u8, command, "check")) {
        try runCheck(stdout);
    } else if (std.mem.eql(u8, command, "install")) {
        try runInstall(stdout);
    } else if (std.mem.eql(u8, command, "purge")) {
        try runPurge(stdout, allocator);
    } else {
        try stdout.print("Unknown command: {s}\n", .{command});
        try printUsage(stdout);
    }
}

fn printUsage(writer: anytype) !void {
    try writer.print("Usage: sentinel [command]\n\n", .{});
    try writer.print("Commands:\n", .{});
    try writer.print("  check    - Audit system requirements (Real RAM, OS, WebView2)\n", .{});
    try writer.print("  install  - Verify Lumina binaries existence\n", .{});
    try writer.print("  purge    - Clean WebView2 user data (EBWebView)\n", .{});
}

fn runCheck(writer: anytype) !void {
    try writer.print("[*] Running System Audit...\n", .{});

    // 1. Check RAM (Real Win32 API)
    var memStatus: MEMORYSTATUSEX = undefined;
    memStatus.dwLength = @sizeOf(MEMORYSTATUSEX);
    
    if (GlobalMemoryStatusEx(&memStatus) != 0) {
        const total_gb = memStatus.ullTotalPhys / (1024 * 1024 * 1024);
        try writer.print("    - RAM: {d} GB Total [OK]\n", .{total_gb});
    } else {
        try writer.print("    - RAM: Check Failed (Win32 Error)\n", .{});
    }

    // 2. OS Check
    const arch = @import("builtin").cpu.arch;
    try writer.print("    - Architecture: {s} [OK]\n", .{@tagName(arch)});
    
    // 3. WebView2 Check (File Existence Heuristic)
    const wv2_path_x86 = "C:\\Program Files (x86)\\Microsoft\\EdgeWebView\\Application";
    const wv2_path_64 = "C:\\Program Files\\Microsoft\\EdgeWebView\\Application";
    
    var found_wv2 = false;
    
    if (std.fs.accessAbsolute(wv2_path_x86, .{})) |_| {
        found_wv2 = true;
    } else |_| {
        if (std.fs.accessAbsolute(wv2_path_64, .{})) |_| {
             found_wv2 = true;
        } else |_| {}
    }

    if (found_wv2) {
        try writer.print("    - WebView2 Runtime: Detected [OK]\n", .{});
    } else {
        try writer.print("    - WebView2 Runtime: NOT DETECTED [WARNING]\n", .{});
    }

    try writer.print("[+] System Audit Complete.\n", .{});
}

fn runInstall(writer: anytype) !void {
    try writer.print("[*] Verifying Installation (Dev/Prod Mode)...\n", .{});
    
    const cwd = std.fs.cwd();
    
    // Paths to check (Relative to src-zig or Project Root)
    // We try to find them in expected locations.
    const files = [_]struct{ name: []const u8, path: []const u8 }{
        // Assuming we run from src-zig during dev
        .{ .name = "Lumina Browser", .path = "../src-tauri/target/release/lumina-browser.exe" },
        .{ .name = "Sidekick (Python)", .path = "../src-tauri/binaries/lumina-sidekick-x86_64-pc-windows-msvc.exe" },
        .{ .name = "Lumina Net", .path = "../src-tauri/binaries/lumina-net-x86_64-pc-windows-msvc.exe" },
    };

    var all_ok = true;
    for (files) |f| {
        cwd.access(f.path, .{}) catch {
            try writer.print("    [MISSING] {s}\n", .{f.name});
            all_ok = false;
            continue;
        };
        try writer.print("    [OK] {s}\n", .{f.name});
    }

    if (all_ok) {
        try writer.print("[+] Installation Verified. All components ready.\n", .{});
    } else {
        try writer.print("[-] Installation Incomplete. Some files are missing.\n", .{});
    }
}

fn runPurge(writer: anytype, allocator: std.mem.Allocator) !void {
    try writer.print("[*] Purging User Data (Privacy & Reset)...\n", .{});

    var env_map = try std.process.getEnvMap(allocator);
    defer env_map.deinit();

    const local_app_data = env_map.get("LOCALAPPDATA");
    if (local_app_data) |path| {
        // Path: %LOCALAPPDATA%\com.nazim.lumina-browser\EBWebView
        const app_dir = try std.fs.path.join(allocator, &[_][]const u8{ path, "com.nazim.lumina-browser" });
        defer allocator.free(app_dir);
        
        const webview_dir = try std.fs.path.join(allocator, &[_][]const u8{ app_dir, "EBWebView" });
        defer allocator.free(webview_dir);

        try writer.print("    - Target: {s}\n", .{webview_dir});

        // Check existence
        std.fs.accessAbsolute(webview_dir, .{}) catch {
            try writer.print("    - Target not found (already clean).\n", .{});
            return;
        };

        // Delete
        std.fs.deleteTreeAbsolute(webview_dir) catch |err| {
             try writer.print("    [ERROR] Failed to delete: {s}\n", .{@errorName(err)});
             // Sometimes file lock issues prevent deletion.
             try writer.print("    (Note: Ensure Lumina is closed before purging)\n", .{});
             return;
        };

        try writer.print("    [CLEANED] EBWebView data purged successfully.\n", .{});

    } else {
        try writer.print("    [ERROR] LOCALAPPDATA environment variable not found.\n", .{});
    }
}
