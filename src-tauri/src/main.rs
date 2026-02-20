#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    tauri_browser_lib::run()
}
