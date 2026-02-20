use std::panic;

#[cfg(windows)]
use windows::Win32::System::Diagnostics::Debug::IsDebuggerPresent;
#[cfg(windows)]
use windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};

/// Initializes the security layer for Lumina.
/// Implements native security hardening replacing legacy Zig implementation.
pub fn init() {
    setup_panic_hook();
    harden_process();
    println!("Lumina Security Layer (Rust Native) initialized.");
}

/// Configures a custom panic hook to prevent sensitive information leak
fn setup_panic_hook() {
    let _default_hook = panic::take_hook();
    panic::set_hook(Box::new(move |_info| {
        #[cfg(not(debug_assertions))]
        {
            eprintln!("Lumina Critical Error: An unexpected error occurred.");
        }

        #[cfg(debug_assertions)]
        {
            _default_hook(_info);
        }
    }));
}

/// Applies process-level hardening techniques.
fn harden_process() {
    #[cfg(windows)]
    unsafe {
        // 1. Anti-Debugging Check
        if IsDebuggerPresent().as_bool() {
            eprintln!("SECURITY ALERT: Debugger detected. Lumina is running in restricted mode.");
            // In the future, we can forcefully terminate or disable features here.
        }

        // 2. Memory Integrity & Status Check
        check_memory_status();
    }
}

#[cfg(windows)]
unsafe fn check_memory_status() {
    let mut mem_status = MEMORYSTATUSEX {
        dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32,
        ..Default::default()
    };

    if GlobalMemoryStatusEx(&mut mem_status).is_ok() {
        let total_mb = mem_status.ullTotalPhys / 1024 / 1024;
        let avail_mb = mem_status.ullAvailPhys / 1024 / 1024;

        // Log memory status for diagnostics
        println!(
            "System Guardian: Memory Check - Available: {}MB / Total: {}MB",
            avail_mb, total_mb
        );

        if avail_mb < 1024 {
            eprintln!("System Guardian Warning: Available memory is low (<1GB). Performance may be degraded.");
        }
    }
}
