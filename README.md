# Lumina Browser

![Lumina Logo](lumina_logo.png)

**Lumina** is a next-generation, high-performance web browser architecture exploring the limits of **Polyglot Engineering**. Built on **Tauri v2**, it orchestrates a symphony of languages—**Rust**, **C#**, **Go**, **Zig**, and **Haskell**—to deliver speed, security, and a unique developer experience.

## 🌟 Key Features

### 🚀 Polyglot Architecture

Lumina isn't just a browser; it's a multi-language runtime environment:

- **Rust (Core):** Powered by Tauri v2 for secure, memory-safe system interactions and window management.
- **C# / Blazor (UI):** A rich, component-based frontend running in WebAssembly with direct native interop.
- **Go (Networking):** A high-concurrency sidecar (`lumina-net`) handling complex network operations and custom protocols.
- **Zig (Optimization):** Integrated for FFI hardening and low-level optimizations in the build pipeline.
- **Kip (Scripting):** A dual-implementation (Rust/Haskell) experimental language for browser automation and scripting.

### 🛡️ Security First

- **Zero-JS-File Policy:** Strict CSP enforcement; no external JavaScript files allowed.
- **Granular ACL:** Permission scopes defined down to specific IPC commands (e.g., `core:app:default`).
- **Sandboxed Contexts:** Strong isolation for browsing tabs and PWA instances.

### 🎨 Modern Experience

- **Glassmorphism UI:** Translucent, GPU-accelerated visual effects.
- **Command Palette (Alt+Space):** Instant access to tabs, commands, and history.
- **Flash Tab:** Floating overlay for quick lookups without context switching.
- **Vertical Tabs (Zen Mode):** Optimized screen real estate for wide displays.

## 🛠️ Tech Stack

| Component | Language / Tech | Role |
| --- | --- | --- |
| **Core** | Rust (Tauri v2) | System Backend, Window Manager |
| **Frontend** | C# (Blazor WASM) | User Interface, Component Logic |
| **Sidecar** | Go | High-perf Networking (`lumina-net`) |
| **Scripting** | Rust / Haskell | Kip Language Runtime (`src-kip`) |
| **Build/FFI** | Zig | Security Hardening & Native Modules |

## 🚀 Getting Started

### Prerequisites

Ensure you have the following installed:

- **Rust:** Latest stable (`rustup update`)
- **.NET SDK:** .NET 8.0 or later
- **Go:** 1.21+ (for networking sidecar)
- **Zig:** 0.11.0+ (for build integration)
- **Tauri CLI:** `cargo install tauri-cli`
- **Node.js:** (Optional, for specific frontend assets)

### Installation & Development

1. **Clone the repository:**

   ```bash
   git clone https://github.com/GalaxySal/Lumina.git
   cd Lumina
   ```

2. **Run the development server:**
   This command compiles the sidecars (Go/Rust/Zig) and starts the app.

   ```bash
   cargo tauri dev
   ```

### Building for Production

To build the optimized release bundle:

```bash
cargo tauri build
```

*Note: The build script automatically handles cross-language compilation.*

## 📂 Project Structure

```text
Lumina/
├── src/            # Blazor WebAssembly Frontend (C#)
├── src-tauri/      # Tauri Core (Rust) & Sidecar Binaries
├── src-go/         # Networking Sidecar (Go)
├── src-kip/        # Kip Language Implementation (Rust/Haskell)
├── src-zig/        # Native Layer (Zig)
└── .github/        # CI/CD Workflows (Polyglot Pipeline)
```

## 📄 License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## 👤 Author

- **Nazim** - *Polyglot Architect*
