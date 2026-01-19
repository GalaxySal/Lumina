# Lumina Browser

Lumina is a modern, high-performance web browser built with **Tauri v2** and **Blazor WebAssembly**. It focuses on speed, security, and productivity with a native-feeling user interface.

## 🌟 Features

*   **⚡ Blazing Fast:** Powered by Rust and WebView2/WebKit.
*   **🛡️ Secure Sandbox:** Strong isolation for browsing contexts.
*   **⌨️ Command Palette:** (Alt+Space) Quick access to tabs, PWAs, and commands with a beautiful glassmorphism UI.
*   **🔦 Flash Tab:** A floating overlay for quick lookups without leaving your current context.
*   **🔄 Quick-Switch:** Seamlessly switch between active Progressive Web Apps (PWAs).
*   **🛠️ Native IPC:** Optimized Inter-Process Communication bypassing standard CSP restrictions for superior performance.
*   **🎨 Glassmorphism UI:** Modern, translucent visual effects with GPU acceleration.

## 🛠️ Tech Stack

*   **Backend:** Rust (Tauri v2)
*   **Frontend:** Blazor WebAssembly (C# / .NET)
*   **UI:** HTML/CSS (Glassmorphism effects)

## 🚀 Getting Started

### Prerequisites

*   Rust (latest stable)
*   Node.js (for frontend assets if applicable)
*   .NET SDK (for Blazor)
*   Tauri CLI (`cargo install tauri-cli`)

### Development

Clone the repository and run the development server:

```bash
git clone https://github.com/GalaxySal/Lumina.git
cd Lumina
cargo tauri dev
```

### Build

To build for production:

```bash
cargo tauri build
```

## 📄 License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## 👤 Author

**Nazim**
