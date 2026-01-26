# Lumina Browser

![Lumina Logo](lumina_logo.png)

**Lumina** is a next-generation, high-performance web browser architecture exploring the limits of **Polyglot Engineering**. Built on **Tauri v2**, it orchestrates a symphony of languages—**Rust**, **C#**, **Go**, **Python**, **Zig**, and **Haskell**—to deliver speed, security, and a unique developer experience.

## 🌟 Key Features

### 🚀 Polyglot Architecture

Lumina isn't just a browser; it's a multi-language runtime environment:

- **Rust (Core):** Powered by Tauri v2 for secure, memory-safe system interactions and window management.
- **C# / Blazor (UI):** A rich, component-based frontend running in WebAssembly with direct native interop.
- **Python (Sidekick):** A local intelligence unit (`lumina-sidekick`) handling heavy lifting, AI tasks, and system automation.
- **Go (Networking):** A high-concurrency sidecar (`lumina-net`) handling complex network operations and custom protocols.
- **Zig (Sentinel):** `lumina-sentinel`, a standalone, ultra-lightweight CLI for system auditing, security verification, and cleanup.
- **Kip (Scripting):** A dual-implementation (Rust/Haskell) experimental language for browser automation and scripting.

### 🛡️ Security First & "Safkan Yapı"

Lumina follows the "Purebred Structure" (Safkan Yapı) philosophy: **No Node.js** in the core logic.

- **Zero-JS-File Policy:** Strict CSP enforcement; no external JavaScript files allowed.
- **Lumina Sentinel:** A dedicated Zig tool to audit environment integrity, verify binaries, and purge Chromium/Google tracking data from the system.
- **Granular ACL:** Permission scopes defined down to specific IPC commands.
- **Sandboxed Contexts:** Strong isolation for browsing tabs and PWA instances.

### 🎨 Modern Experience

- **Glassmorphism UI:** Translucent, GPU-accelerated visual effects.
- **Command Palette (Alt+Space):** Instant access to tabs, commands, and history.
- **Flash Tab:** Floating overlay for quick lookups without context switching.
- **Vertical Tabs (Zen Mode):** Optimized screen real estate for wide displays.
- **OmniBox Smart Search:** Intelligent address bar powered by Python Sidekick, offering real-time math, time, history, and favorite suggestions.
- **Chrome Extensions (Windows):** Support for loading unpacked Chrome extensions for enhanced browsing.
- **Lua Scripting:** Built-in sandboxed Lua 5.4 runtime for safe browser automation and extension.

## 🛠️ Tech Stack

| Component | Language / Tech | Role |
| --- | --- | --- |
| **Core** | Rust (Tauri v2) | System Backend, Window Manager |
| **Frontend** | C# (Blazor WASM) | User Interface, Component Logic |
| **Intelligence** | Python | Local AI & Automation (`lumina-sidekick`) |
| **Networking** | Go | High-perf Networking (`lumina-net`) |
| **Security Tool** | Zig | System Audit & Cleanup (`lumina-sentinel`) |
| **Scripting** | Rust / Haskell | Kip Language Runtime (`src-kip`) |

## 🚀 Getting Started

### Prerequisites

Ensure you have the following installed:

- **Rust:** Latest stable (`rustup update`)
- **.NET SDK:** .NET 8.0 or later
- **Python:** 3.10+ (for Sidekick)
- **Go:** 1.21+ (for networking sidecar)
- **Zig:** Master/Latest (for Sentinel)
- **Tauri CLI:** `cargo install tauri-cli`

### Installation & Development

1. **Clone the repository:**

   ```bash
   git clone https://github.com/GalaxySal/Lumina.git
   cd Lumina
   ```

2. **Run the development server:**
   This command compiles the sidecars (Go/Python/Rust/Zig) and starts the app.

   ```bash
   cargo tauri dev
   ```

### Building for Production

To build the optimized release bundle (including Sentinel):

```bash
cargo tauri build
```

*Note: The build pipeline automatically handles cross-language compilation and integrates `lumina-sentinel` into the release assets.*

## 📂 Project Structure

```text
Lumina/
├── src/            # Blazor WebAssembly Frontend (C#)
├── src-tauri/      # Tauri Core (Rust) & Sidecar Binaries
├── src-sidekick/   # Intelligence Sidecar (Python)
├── src-go/         # Networking Sidecar (Go)
├── src-zig/        # Lumina Sentinel (Zig Standalone Tool)
├── src-kip/        # Kip Language Implementation (Rust/Haskell)
└── .github/        # CI/CD Workflows (Polyglot Pipeline)
```

## 📄 License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## 👤 Author

- **Nazim** - *Polyglot Architect*
