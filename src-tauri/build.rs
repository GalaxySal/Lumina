use std::process::Command;
use std::env;
use std::path::Path;

fn main() {
    println!("cargo:rustc-check-cfg=cfg(zig_enabled)");

    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let zig_dir = Path::new(&manifest_dir).parent().unwrap().join("src-zig");
    
    println!("cargo:rerun-if-changed={}", zig_dir.join("main.zig").display());
    println!("cargo:rerun-if-changed={}", zig_dir.join("build.zig").display());

    // Attempt to build Zig project
    if zig_dir.exists() {
        let status = Command::new("zig")
            .args(["build", "-Doptimize=ReleaseFast"])
            .current_dir(&zig_dir)
            .status();

        match status {
            Ok(s) if s.success() => {
                let zig_out = zig_dir.join("zig-out/lib");
                println!("cargo:rustc-link-search=native={}", zig_out.display());
                println!("cargo:rustc-link-lib=static=lumina_zig");
                println!("cargo:rustc-cfg=zig_enabled");
            }
            Ok(_) => {
                println!("cargo:warning=Zig build command failed. Ensure 'zig' is in PATH and project is valid.");
            }
            Err(_) => {
                println!("cargo:warning=Zig toolchain not found. Skipping Zig integration.");
            }
        }
    }

    tauri_build::build()
}
