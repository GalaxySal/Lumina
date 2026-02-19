# Lumina Browser

![Lumina Logo](lumina-logo.png)

[![Rust](https://img.shields.io/badge/Rust-Core-orange?logo=rust)](https://www.rust-lang.org/)
[![Tauri](https://img.shields.io/badge/Tauri-v2-blue?logo=tauri)](https://tauri.app/)
[![.NET 8](https://img.shields.io/badge/.NET-8.0-purple?logo=dotnet)](https://dotnet.microsoft.com/)
[![Blazor](https://img.shields.io/badge/Blazor-WASM-512BD4?logo=blazor)](https://dotnet.microsoft.com/apps/aspnet/web-apps/blazor)
[![Python](https://img.shields.io/badge/Python-Sidekick-yellow?logo=python)](https://www.python.org/)
[![Go](https://img.shields.io/badge/Go-Networking-00ADD8?logo=go)](https://go.dev/)
[![Haskell](https://img.shields.io/badge/Haskell-Scripting-5D4F85?logo=haskell)](https://www.haskell.org/)
[![License](https://img.shields.io/badge/License-MIT-green)](LICENSE)

**Lumina** is a next-generation, high-performance web browser architecture exploring the limits of **Polyglot Engineering**. Built on **Tauri v2**, it orchestrates a symphony of languages—**Rust**, **C#**, **Go**, **Python**, and **Haskell**—to deliver speed, security, and a unique developer experience.

## 🌟 Key Features

### 🚀 Polyglot Architecture

Lumina isn't just a browser; it's a multi-language runtime environment:

- **Rust (Core):** Powered by Tauri v2 for secure, memory-safe system interactions and window management.
- **C# / Blazor (UI):** A rich, component-based frontend running in WebAssembly with direct native interop.
- **Python (Sidekick):** A local intelligence unit (`lumina-sidekick`) handling heavy lifting, AI tasks, and system automation.
- **Go (Networking):** A high-concurrency sidecar (`lumina-net`) handling complex network operations and custom protocols.
- **Kip (Scripting):** A dual-implementation (Rust/Haskell) experimental language for browser automation and scripting.

### 🛡️ Security First & "Safkan Yapı"

Lumina follows the "Purebred Structure" (Safkan Yapı) philosophy: **No Node.js** in the core logic.

- **Zero-JS-File Policy:** Strict CSP enforcement; no external JavaScript files allowed.
- **Granular ACL:** Permission scopes defined down to specific IPC commands.
- **Sandboxed Contexts:** Strong isolation for browsing tabs and PWA instances.

### 🎨 Modern Experience

- **Glassmorphism UI:** Translucent, GPU-accelerated visual effects (Acrylic/Mica).
- **Command Palette (Alt+Space):** Instant access to tabs, commands, and history.
- **Flash Tab:** Floating overlay for quick lookups without context switching.
- **Vertical Tabs (Zen Mode):** Optimized screen real estate for wide displays.
- **Native Screenshot:** Capture and save screenshots directly via Command Palette without extensions.
- **Text Scaling:** Adjust UI text size (100%-200%) via Flags for better readability.

### 🧠 Intelligent Omnibox

- **Smart Search:** Intelligent address bar powered by Python Sidekick.
- **Built-in Calculator:** Perform math operations directly in the address bar (e.g., `(12*5)+50`).
- **Real-time Suggestions:** History, favorites, and navigation heuristics.

### ⚙️ Power User Tools

- **Lumina Flags (`lumina://flags`):** Advanced configuration page to toggle experimental features (like Firefox's `about:config`).
- **Tab Pinning:** Pin essential tabs to keep them compact and accessible.
- **Web-Store (`lumina-app://store`):** Secure environment for discovering extensions.
- **Lua Scripting:** Built-in sandboxed Lua 5.4 runtime for safe browser automation.

## 🛠️ Tech Stack

| Component | Language / Tech | Role | Details |
| --- | --- | --- | --- |
| **Core** | Rust (Tauri v2) | System Backend | Window Manager, IPC, File System |
| **Frontend** | C# (Blazor WASM) | User Interface | .NET 8.0, Razor Components |
| **Intelligence** | Python 3.10+ | AI Sidecar | `lumina-sidekick`, Math, Heuristics |
| **Networking** | Go 1.25+ | Net Sidecar | `lumina-net`, TCP/UDP Sockets |
| **Scripting** | Rust / Haskell | Kip Language | `src-kip`, Parser/Interpreter |

## 🚀 Getting Started

### Prerequisites

Ensure you have the following installed:

- **Rust:** Latest stable (`rustup update`)
- **.NET SDK:** .NET 8.0 or later
- **Python:** 3.10+ (for Sidekick)
- **Go:** 1.21+ (for networking sidecar)
- **Tauri CLI:** `cargo install tauri-cli`

### Installation & Development

1. **Clone the repository:**

   ```bash
   git clone https://github.com/GalaxySal/Lumina.git
   cd Lumina
   ```

2. **Run the development server:**
   This command compiles the sidecars (Go/Python/Rust) and starts the app.

   ```bash
   cargo tauri dev
   ```

### Building for Production

To build the optimized release bundle:

```bash
cargo tauri build
```

*Note: The build pipeline automatically handles cross-language compilation.*

> **⚠️ TODO: Enable Updater Plugin**
> 
> The self-update feature (Tauri Updater) is currently disabled. To enable automatic updates in production, configure the updater plugin in `src-tauri/tauri.conf.json` and set up a signed release distribution endpoint. This is scheduled for implementation in v0.4.0.

## 📂 Project Structure

```text
Lumina/
├── src/            # Blazor WebAssembly Frontend (C#)
├── src-tauri/      # Tauri Core (Rust) & Sidecar Binaries
├── src-sidekick/   # Intelligence Sidecar (Python)
├── src-go/         # Networking Sidecar (Go)
├── src-kip/        # Kip Language Implementation (Rust/Haskell)
└── .github/        # CI/CD Workflows (Polyglot Pipeline)
```

## 📄 License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## 👤 Author

- **Nazim** - *Polyglot Architect*
