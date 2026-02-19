mod data;
mod history_manager;
mod security; // Added security module
use history_manager::HistoryManager;
use data::{AppDataStore, HistoryItem, FavoriteItem, AppSettings};
use tauri::{AppHandle, Manager, WebviewUrl, Emitter, Listener, Url};
use futures_util::StreamExt;
use tokio::io::{AsyncWriteExt, AsyncSeekExt};
use std::collections::HashMap;
use std::sync::{Mutex, Arc, OnceLock};
use std::path::PathBuf;
use serde::{Serialize, Deserialize};
use std::fs::OpenOptions;
use adblock::engine::Engine;
use adblock::lists::FilterSet;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState, Modifiers, Code};
use base64::Engine as _;
use mlua::Lua;

static ADBLOCK_ENGINE: OnceLock<Arc<Mutex<Engine>>> = OnceLock::new();
static ADBLOCK_STATS: OnceLock<Arc<Mutex<HashMap<String, u32>>>> = OnceLock::new();

struct LuaState {
    lua: Mutex<Lua>,
}

// 1. Safe Lua Execution (Real Lua 5.4 Runtime)
// This creates a sandboxed Lua environment that can interact with the browser safely.
fn create_lua_runtime() -> Lua {
    // Create a Lua state with safe standard libraries only (Sandbox)
    // We exclude IO, OS, and Package libraries to prevent system access
    // Note: Lua 5.4 has built-in bitwise operators, so BIT library is not needed/available.
    let libs = mlua::StdLib::MATH | mlua::StdLib::STRING | mlua::StdLib::TABLE;
    let lua = Lua::new_with(libs, mlua::LuaOptions::default()).expect("Failed to create Lua state");
    
    // Inject Custom Lumina API
    let _ = lua.load("
        -- Custom Lumina API
        lumina = {
            version = '0.3.6',
            platform = 'windows'
        }
    ").exec();

    lua
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct StoreItem {
    pub id: String,
    pub title: String,
    pub author: String,
    pub description: String,
    pub icon: String,
    pub version: String,
    pub tags: Vec<String>,
    pub verified: bool,
    #[serde(default)]
    pub installed: bool,
    #[serde(default, rename = "comingSoon")]
    pub coming_soon: bool,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct ToastPayload {
    pub message: String,
    pub level: String,
}

#[tauri::command]
fn get_store_items(app: AppHandle) -> Vec<StoreItem> {
    // Prioritize the writable app data store.json so user changes (installs) are reflected
    let app_data_store = app.path().app_data_dir().unwrap_or_default().join("store.json");
    
    let paths = vec![
        app_data_store, // Check writable path FIRST
        PathBuf::from("store.json"),
        PathBuf::from("data/store.json"),
        PathBuf::from("src-tauri/store.json"),
    ];

    for path in paths {
        if path.exists() {
            if let Ok(content) = std::fs::read_to_string(&path) {
                if let Ok(items) = serde_json::from_str::<Vec<StoreItem>>(&content) {
                    return items;
                }
            }
        }
    }
    Vec::new()
}

fn perform_install(app: &AppHandle, id: &str) -> bool {
    // 1. Determine Store Path (Writable)
    let app_data_dir = app.path().app_data_dir().unwrap_or(PathBuf::from("."));
    if !app_data_dir.exists() {
        let _ = std::fs::create_dir_all(&app_data_dir);
    }
    let store_path = app_data_dir.join("store.json");

    // 2. Load Existing Items
    let mut items: Vec<StoreItem> = Vec::new();
    
    // Try to read from writable path first
    if store_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&store_path) {
             if let Ok(parsed) = serde_json::from_str(&content) {
                 items = parsed;
             }
        }
    } else {
        // Fallback to read from read-only locations (like src-tauri/store.json) to init
        let paths = vec![
            PathBuf::from("store.json"),
            PathBuf::from("src-tauri/store.json"),
        ];
        for p in paths {
            if p.exists() {
                if let Ok(content) = std::fs::read_to_string(&p) {
                     if let Ok(parsed) = serde_json::from_str(&content) {
                         items = parsed;
                         break;
                     }
                }
            }
        }
    }

    // 3. Update Item
    let mut found = false;
    for item in &mut items {
        if item.id == id {
            item.installed = true;
            found = true;
            break;
        }
    }

    if found {
        // 4. Save to Writable Path
        if let Ok(json) = serde_json::to_string_pretty(&items) {
            if let Ok(mut file) = OpenOptions::new().write(true).create(true).truncate(true).open(&store_path) {
                if std::io::Write::write_all(&mut file, json.as_bytes()).is_ok() {
                    return true;
                }
            }
        }
    }
    false
}

#[tauri::command]
fn install_package(app: AppHandle, id: String) {
    println!("Lumina Command: Installing {}", id);
    
    if perform_install(&app, &id) {
         let _ = app.emit("toast", ToastPayload {
            message: format!("Modül başarıyla kuruldu: {}", id),
            level: "success".to_string(),
        });
    } else {
        let _ = app.emit("toast", ToastPayload {
            message: format!("Kurulum başarısız: {}", id),
            level: "error".to_string(),
        });
    }
}

#[derive(Clone, serde::Serialize)]
struct AdblockStatsPayload {
    label: String,
    blocked_count: u32,
}

fn check_adblock_url(url: &str, referer: Option<&str>, label: &str, app: &AppHandle) -> bool {
    // 0. Always Allow Internal Protocols
    if url.starts_with("lumina:") || url.starts_with("lumina-app:") {
        return false;
    }

    // 0. Force Block List (Overrides Friendly Policy) - Kills AdMatic & Google Ads on Friendly Sites
    if url.contains("admatic.com.tr") || 
       url.contains("doubleclick.net") || 
       url.contains("googlesyndication.com") || 
       url.contains("adnxs.com") || 
       url.contains("smartadserver.com") ||
       url.contains("criteo.com") ||
       url.contains("rubiconproject.com") ||
       url.contains("pubmatic.com") {
        println!("Lumina Adblock: Forced block on ad domain: {}", url);
        return true;
    }

    // 1. Friendly Domain Policy (Bypass Adblock for Gemini/Google Critical Services)
    if let Some(ref_str) = referer {
         if ref_str.contains("gemini.google.com") || 
            ref_str.contains("accounts.google.com") ||
            ref_str.contains("google.com") ||
            ref_str.contains("youtube.com") ||
            ref_str.contains("transfermarkt") {
              // println!("Lumina Adblock: Bypassing friendly domain: {}", url);
              return false;
         }
    }

    // 1. Check Global Adblock Engine
    if let Some(engine_arc) = ADBLOCK_ENGINE.get() {
        if let Ok(engine) = engine_arc.lock() {
            let check_result = engine.check_network_request(&adblock::request::Request::new(
                url,
                referer.unwrap_or(""), 
                "", // Request type (empty for now)
            ).unwrap());
            
            if check_result.matched {
                println!("Lumina Adblock: Blocked {}", url);
                
                // Increment stats
                if let Some(stats_arc) = ADBLOCK_STATS.get() {
                    if let Ok(mut stats) = stats_arc.lock() {
                        let count = stats.entry(label.to_string()).or_insert(0);
                        *count += 1;
                        
                        // Emit event to frontend (Spawned to avoid blocking the resource request thread)
                        let app_emit = app.clone();
                        let label_emit = label.to_string();
                        let count_emit = *count;
                        tauri::async_runtime::spawn(async move {
                            let _ = app_emit.emit("adblock-stats-update", AdblockStatsPayload {
                                label: label_emit,
                                blocked_count: count_emit,
                            });
                        });
                    }
                }
                
                return true;
            }
        }
    }

    // 2. Fallback to HostBlock List
    if BLOCKED_DOMAINS.iter().any(|d| url.contains(d)) {
        println!("Lumina HostBlock: {}", url);
        // Increment stats (also for host block)
        if let Some(stats_arc) = ADBLOCK_STATS.get() {
            if let Ok(mut stats) = stats_arc.lock() {
                let count = stats.entry(label.to_string()).or_insert(0);
                *count += 1;
                
                // Emit event to frontend (Spawned)
                let app_emit = app.clone();
                let label_emit = label.to_string();
                let count_emit = *count;
                tauri::async_runtime::spawn(async move {
                    let _ = app_emit.emit("adblock-stats-update", AdblockStatsPayload {
                        label: label_emit,
                        blocked_count: count_emit,
                    });
                });
            }
        }
        return true;
    }

    false
}


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadItem {
    pub url: String,
    pub file_name: String,
    pub total_size: u64,
    pub downloaded_size: u64,
    pub path: String,
    pub status: String, // "downloading", "paused", "completed", "failed"
    #[serde(default)]
    pub added_at: i64,
}

pub struct DownloadManager {
    pub downloads: Mutex<HashMap<String, DownloadItem>>,
    pub app_dir: PathBuf,
}

impl DownloadManager {
    pub fn new(app_dir: PathBuf) -> Self {
        let mut manager = Self {
            downloads: Mutex::new(HashMap::new()),
            app_dir: app_dir.clone(),
        };
        manager.load();
        manager
    }

    pub fn load(&mut self) {
        let path = self.app_dir.join("downloads.json");
        if path.exists() {
            if let Ok(content) = std::fs::read_to_string(&path) {
                if let Ok(data) = serde_json::from_str::<HashMap<String, DownloadItem>>(&content) {
                    *self.downloads.lock().unwrap() = data;
                }
            }
        }
    }

    pub fn save(&self) {
        let path = self.app_dir.join("downloads.json");
        let data = self.downloads.lock().unwrap();
        if let Ok(content) = serde_json::to_string_pretty(&*data) {
            // Use OpenOptions (restored)
            if let Ok(mut file) = OpenOptions::new().write(true).create(true).truncate(true).open(path) {
                let _ = std::io::Write::write_all(&mut file, content.as_bytes());
            }
        }
    }
    
    pub fn update_status(&self, url: &str, status: &str) {
        let mut data = self.downloads.lock().unwrap();
        if let Some(item) = data.get_mut(url) {
            item.status = status.to_string();
        }
        drop(data); // unlock before save
        self.save();
    }
    
    pub fn update_progress(&self, url: &str, downloaded: u64, total: u64) {
        let mut data = self.downloads.lock().unwrap();
        if let Some(item) = data.get_mut(url) {
            item.downloaded_size = downloaded;
            item.total_size = total;
        }
        // Don't save on every progress update to avoid IO thrashing
    }

    pub fn get_downloads(&self) -> Vec<DownloadItem> {
        let data = self.downloads.lock().unwrap();
        data.values().cloned().collect()
    }
}

async fn check_and_redirect(webview: tauri::Webview, url: String) {
    if url.starts_with("tauri://") || url.starts_with("file://") || url.starts_with("about:") || url.starts_with("data:") || url.starts_with("lumina://") {
        return;
    }

    // Simple check: try to fetch headers
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .unwrap_or_default();

    match client.get(&url).send().await {
        Ok(_) => {
            // Success or server error (404/500), browser handles it.
            // We only care if we CANNOT reach the server.
        }
        Err(e) => {
            // If it's a builder error, ignore. If it's a request error...
            if e.is_connect() || e.is_timeout() || e.is_body() { // is_body means error reading body? No.
               // is_connect covers DNS, Refused.
               // is_timeout covers timeout.
               println!("Connection failed for {}: {}", url, e);
               
               let err_msg = e.to_string();
               let error_url = format!("tauri://localhost/error.html?url={}&err={}", 
                   urlencoding::encode(&url), 
                   urlencoding::encode(&err_msg));
               
               let _ = webview.eval(format!("window.location.replace('{}')", error_url));
            }
        }
    }
}

struct SidekickState {
    tx: tokio::sync::mpsc::Sender<String>,
}

#[tauri::command]
async fn request_omnibox_suggestions(
    state: tauri::State<'_, SidekickState>, 
    app_data: tauri::State<'_, AppDataStore>,
    history_manager: tauri::State<'_, HistoryManager>,
    query: String
) -> Result<(), String> {
    // 1. Fetch Favorites
    let favorites = {
        let data = app_data.data.lock().unwrap();
        data.favorites.clone()
    };

    // 2. Fetch History (Search or Recent)
    let history_items = if query.is_empty() {
        history_manager.get_recent(10).unwrap_or_default()
    } else {
        history_manager.search(&query).unwrap_or_default()
    };

    // 3. Construct Payload
    let payload = serde_json::json!({
        "type": "omnibox_query",
        "query": query,
        "context": {
            "favorites": favorites,
            "history": history_items
        }
    }).to_string();
    
    state.tx.send(payload).await.map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
async fn navigate(app: AppHandle, label: String, url: String) {
    // println!("Rust: navigating tab {} to {}", label, url);
    // Try to find the webview. If not found, it might be because it was JUST created and not yet in the map.
    // In Tauri v2, add_child returns the webview instance.
    // But navigate is a separate command called from JS, so it relies on AppHandle lookup.
    
    let mut webview = app.get_webview(&label);
    if webview.is_none() {
        // Retry logic for race conditions - Increased to 10x 100ms (1s total)
        for i in 0..10 {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            webview = app.get_webview(&label);
            if webview.is_some() { 
                println!("Rust: webview {} found after retry {}", label, i+1);
                break; 
            }
        }
    }

    if let Some(webview) = webview {
        let _ = webview.set_focus();
        
        // Rewrite lumina:// to lumina-app://localhost/ (standardized) for internal navigation
        let target_url = if url.starts_with("lumina://") {
            url.replace("lumina://", "lumina-app://localhost/")
        } else if url.starts_with("lumina-app://") {
            if !url.contains("lumina-app://localhost/") {
                url.replace("lumina-app://", "lumina-app://localhost/")
            } else {
                url.clone()
            }
        } else {
            url.clone()
        };

        println!("Rust: navigating tab {} to {}", label, target_url); // DEBUG LOG

        // Use eval for navigation
        let _ = webview.eval(format!("window.location.assign('{}')", target_url).as_str());
        
        // Check connection
        let wv = webview.clone();
        let u = target_url.clone();
        tauri::async_runtime::spawn(async move {
            check_and_redirect(wv, u).await;
        });
    } else {
        println!("Rust: webview {} not found via AppHandle lookup (gave up after retries)", label);
    }
}

fn get_internal_page_html(app: &AppHandle, path: &str) -> Option<String> {
    let lumina_style = r#"
        <style>
            :root { --primary: #05B8CC; --bg: #121212; --card: #1e1e1e; --text: #e0e0e0; --text-dim: #a0a0a0; }
            body { font-family: 'Segoe UI', system-ui, sans-serif; padding: 40px; background: var(--bg); color: var(--text); max-width: 900px; margin: 0 auto; }
            h1 { border-bottom: 2px solid #333; padding-bottom: 20px; margin-bottom: 30px; font-weight: 600; color: var(--primary); letter-spacing: 1px; }
            .item { background: var(--card); padding: 15px 20px; margin-bottom: 10px; border-radius: 8px; border-left: 4px solid var(--primary); display: flex; align-items: center; gap: 20px; transition: transform 0.2s; }
            .item:hover { transform: translateX(5px); }
            .time, .meta { color: var(--text-dim); font-size: 0.85em; white-space: nowrap; }
            .title, .filename { font-weight: 500; margin-bottom: 4px; color: #fff; font-size: 1.1em; }
            .url a { color: var(--text-dim); font-size: 0.9em; text-decoration: none; display: block; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
            .url a:hover { color: var(--primary); }
            button { padding: 8px 16px; cursor: pointer; border: 1px solid #333; background: #2d2d2d; border-radius: 6px; color: #fff; transition: all 0.2s; }
            button:hover { background: var(--primary); border-color: var(--primary); color: #000; }
            .empty-state { text-align: center; color: var(--text-dim); padding: 60px; font-size: 1.2em; border: 2px dashed #333; border-radius: 12px; }
            /* Scrollbar */
            ::-webkit-scrollbar { width: 10px; }
            ::-webkit-scrollbar-track { background: var(--bg); }
            ::-webkit-scrollbar-thumb { background: #333; border-radius: 5px; }
            ::-webkit-scrollbar-thumb:hover { background: var(--primary); }
            @keyframes slideIn { from { transform: translateY(100%); opacity: 0; } to { transform: translateY(0); opacity: 1; } }
        </style>
        <script>
            (function() {
                if (window.__TAURI__) {
                    window.__TAURI__.event.listen('lua-bridge-message', (event) => {
                        console.log("Lua Bridge:", event.payload);
                        let el = document.getElementById('bridge-msg');
                        if (!el) {
                            el = document.createElement('div');
                            el.id = 'bridge-msg';
                            el.style.cssText = "position: fixed; bottom: 20px; right: 20px; background: #7C4DFF; color: white; padding: 15px; border-radius: 8px; z-index: 9999; box-shadow: 0 4px 12px rgba(0,0,0,0.3); animation: slideIn 0.3s ease-out; font-weight: 500; display: flex; align-items: center; gap: 10px;";
                            document.body.appendChild(el);
                        }
                        el.innerHTML = "<span>🔮</span> " + event.payload;
                        
                        // Auto hide after 5s
                        if (window.bridgeTimeout) clearTimeout(window.bridgeTimeout);
                        window.bridgeTimeout = setTimeout(() => {
                            if(el) {
                                el.style.opacity = '0';
                                el.style.transform = 'translateY(100%)';
                                setTimeout(() => el.remove(), 300);
                            }
                        }, 5000);
                    });
                }
            })();
        </script>
    "#;

    match path {
        "history" => {
            let history_manager = app.state::<HistoryManager>();
            let history = history_manager.get_recent(100).unwrap_or_default();
            
            let mut items_html = String::new();
            for item in history {
                let date = chrono::DateTime::from_timestamp(item.last_visit, 0)
                    .map(|d| d.with_timezone(&chrono::Local).format("%Y-%m-%d %H:%M").to_string())
                    .unwrap_or_else(|| "Unknown".to_string());

                items_html.push_str(&format!(
                    r#"<div class="item">
                        <div class="time">{}</div>
                        <div class="info">
                            <div class="title">{}</div>
                            <div class="url"><a href="{}">{}</a></div>
                        </div>
                    </div>"#,
                    date, item.title, item.url, item.url
                ));
            }
            
            if items_html.is_empty() {
                items_html = r#"<div class="empty-state">No history yet</div>"#.to_string();
            }

            Some(format!(
                r#"<!DOCTYPE html>
                <html>
                <head>
                    <title>History - Lumina</title>
                    <meta charset="UTF-8">
                    {}
                </head>
                <body>
                    <h1>History</h1>
                    <div id="list">{}</div>
                </body>
                </html>"#,
                lumina_style, items_html
            ))
        },
        "downloads" => {
            let download_manager = app.state::<DownloadManager>();
            let downloads = download_manager.inner().get_downloads();
            
            let mut items_html = String::new();
            for item in downloads.iter().rev() {
                 let finished = item.status == "completed";
                 let status_color = if finished { "#00E676" } else { "#FFAB40" }; // Material Green/Orange
                 let status_text = if finished { "Completed" } else { "Downloading..." };
                 
                 let date = if item.added_at > 0 {
                     chrono::DateTime::from_timestamp(item.added_at, 0)
                         .map(|d| d.with_timezone(&chrono::Local).format("%Y-%m-%d %H:%M").to_string())
                         .unwrap_or_default()
                 } else {
                     "".to_string()
                 };

                 items_html.push_str(&format!(
                    r#"<div class="item" style="border-left-color: {};">
                        <div class="icon" style="font-size: 24px; width: 40px; text-align: center;">⬇️</div>
                        <div class="info">
                            <div class="filename">{}</div>
                            <div class="url"><a href="{}">{}</a></div>
                            <div class="meta" style="color: var(--text-dim);">{} • {} • {}</div>
                        </div>
                        <div class="actions">
                            <button onclick="window.__TAURI__.core.invoke('open_file', {{ path: '{}' }})">Open</button>
                            <button onclick="window.__TAURI__.core.invoke('show_in_folder', {{ path: '{}' }})">Folder</button>
                        </div>
                    </div>"#,
                    status_color,
                    item.file_name, item.url, item.url, 
                    status_text, item.path, date,
                    item.path.replace("\\", "\\\\"), item.path.replace("\\", "\\\\")
                ));
            }

            if items_html.is_empty() {
                items_html = r#"<div class="empty-state">No downloads yet</div>"#.to_string();
            }

            Some(format!(
                r#"<!DOCTYPE html>
                <html>
                <head>
                    <title>Downloads - Lumina</title>
                    <meta charset="UTF-8">
                    {}
                </head>
                <body>
                    <h1>Downloads</h1>
                    <div id="list">{}</div>
                </body>
                </html>"#,
                lumina_style, items_html
            ))
        },
        "favorites" | "bookmarks" => {
            let state = app.state::<AppDataStore>();
            let data = state.data.lock().unwrap();
            let favorites = &data.favorites;
            
            let mut items_html = String::new();
            for item in favorites {
                items_html.push_str(&format!(
                    r#"<div class="item">
                        <div class="icon" style="color: #FFD700; font-size: 24px;">★</div>
                        <div class="info">
                            <div class="filename">{}</div>
                            <div class="url"><a href="{}">{}</a></div>
                        </div>
                        <div class="actions">
                            <button style="border-color: #ef5350; color: #ef5350;" onmouseover="this.style.background='#ef5350'; this.style.color='white'" onmouseout="this.style.background='transparent'; this.style.color='#ef5350'" onclick="window.__TAURI__.core.invoke('remove_favorite', {{ url: '{}' }}).then(() => window.location.reload())">Remove</button>
                        </div>
                    </div>"#,
                    item.title, item.url, item.url, item.url
                ));
            }
            
            if items_html.is_empty() {
                 items_html = r#"<div class="empty-state">No favorites yet</div>"#.to_string();
            }
            
            Some(format!(
                r#"<!DOCTYPE html>
                <html>
                <head>
                    <title>Favorites - Lumina</title>
                    <meta charset="UTF-8">
                    {}
                </head>
                <body>
                    <h1>Favorites</h1>
                    <div id="list">
                        {}
                    </div>
                </body>
                </html>"#,
                lumina_style, items_html
            ))
        },
        "store" => {
            // Lumina Web-Store (No-JS)
            let store_css = r#"
                body { font-family: 'Segoe UI', system-ui, sans-serif; background: #0f172a; color: #e2e8f0; margin: 0; padding: 0; }
                .container { max-width: 1000px; margin: 0 auto; padding: 40px 20px; }
                header { display: flex; align-items: center; justify-content: space-between; margin-bottom: 40px; border-bottom: 1px solid #334155; padding-bottom: 20px; }
                h1 { margin: 0; font-size: 2.5rem; background: linear-gradient(to right, #3b82f6, #10b981); -webkit-background-clip: text; -webkit-text-fill-color: transparent; }
                .tagline { color: #94a3b8; font-size: 1.1rem; }
                .grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(300px, 1fr)); gap: 24px; }
                .card { background: #1e293b; border: 1px solid #334155; border-radius: 12px; padding: 24px; transition: transform 0.2s, border-color 0.2s; position: relative; overflow: hidden; }
                .card:hover { transform: translateY(-4px); border-color: #3b82f6; }
                .card-header { display: flex; align-items: center; gap: 12px; margin-bottom: 16px; }
                .icon { width: 48px; height: 48px; background: #334155; border-radius: 10px; display: flex; align-items: center; justify-content: center; font-size: 24px; }
                .card h3 { margin: 0; font-size: 1.25rem; color: #f8fafc; }
                .author { font-size: 0.875rem; color: #64748b; margin-top: 4px; }
                .desc { color: #cbd5e1; line-height: 1.5; margin-bottom: 20px; font-size: 0.95rem; }
                .meta { display: flex; gap: 12px; font-size: 0.8rem; color: #64748b; margin-bottom: 20px; }
                .tag { background: #334155; padding: 2px 8px; border-radius: 4px; color: #94a3b8; }
                .btn { display: block; text-align: center; background: #3b82f6; color: white; text-decoration: none; padding: 10px; border-radius: 8px; font-weight: 600; transition: background 0.2s; }
                .btn:hover { background: #2563eb; }
                .btn.installed { background: #10b981; pointer-events: none; opacity: 0.8; }
                .badge-verified { color: #10b981; display: inline-flex; align-items: center; gap: 4px; font-size: 0.8rem; margin-left: auto; }
            "#;

            Some(format!(
                r##"<!DOCTYPE html>
                <html>
                <head>
                    <title>Lumina Store</title>
                    <meta charset="UTF-8">
                    <style>{}</style>
                </head>
                <body>
                    <div class="container">
                        <header>
                            <div>
                                <h1>Lumina Store</h1>
                                <div class="tagline">Secure, Sandboxed, No-JS Extensions</div>
                            </div>
                            <div style="text-align: right">
                                <div style="font-size: 0.9rem; color: #94a3b8;">Balance</div>
                                <div style="font-size: 1.2rem; font-weight: bold;">0 LUM</div>
                            </div>
                        </header>

                        <div class="grid">
                            <!-- Item 1: Init Script -->
                            <div class="card">
                                <div class="card-header">
                                    <div class="icon">🚀</div>
                                    <div>
                                        <h3>Dev Starter Pack</h3>
                                        <div class="author">by @safkanyapi</div>
                                    </div>
                                    <div class="badge-verified">✓ Verified</div>
                                </div>
                                <div class="desc">
                                    Essential initialization scripts for Lua development. Includes debug helpers and environment checks.
                                </div>
                                <div class="meta">
                                    <span class="tag">System</span>
                                    <span class="tag">Lua</span>
                                    <span class="tag">v1.0.0</span>
                                </div>
                                <a href="lumina-app://install?id=init-script" class="btn">Install</a>
                            </div>

                            <!-- Item 2: Adblock Plus -->
                            <div class="card">
                                <div class="card-header">
                                    <div class="icon">🛡️</div>
                                    <div>
                                        <h3>AdShield Pro</h3>
                                        <div class="author">by @community</div>
                                    </div>
                                </div>
                                <div class="desc">
                                    Enhanced filter lists for Turkish media sites. Blocks aggressive trackers and mining scripts.
                                </div>
                                <div class="meta">
                                    <span class="tag">Privacy</span>
                                    <span class="tag">Filters</span>
                                    <span class="tag">v2.1.0</span>
                                </div>
                                <a href="lumina-app://install?id=adshield" class="btn">Install</a>
                            </div>

                            <!-- Item 3: Offline AI (Placeholder) -->
                            <div class="card" style="opacity: 0.7; border-style: dashed;">
                                <div class="card-header">
                                    <div class="icon">🧠</div>
                                    <div>
                                        <h3>Local Brain (Phi-2)</h3>
                                        <div class="author">by @lumina_ai</div>
                                    </div>
                                </div>
                                <div class="desc">
                                    Run LLMs locally on your device. Zero data leaves your machine. (Coming Soon)
                                </div>
                                <div class="meta">
                                    <span class="tag">AI</span>
                                    <span class="tag">Experimental</span>
                                </div>
                                <a href="#" class="btn" style="background: #475569; cursor: not-allowed;">Coming Soon</a>
                            </div>
                            
                            <!-- Item 4: Dark Reader -->
                            <div class="card">
                                <div class="card-header">
                                    <div class="icon">🌙</div>
                                    <div>
                                        <h3>Night Owl</h3>
                                        <div class="author">by @nightwalker</div>
                                    </div>
                                </div>
                                <div class="desc">
                                    Forces dark mode on all internal pages and supported websites via CSS injection.
                                </div>
                                <div class="meta">
                                    <span class="tag">Theme</span>
                                    <span class="tag">CSS</span>
                                </div>
                                <a href="lumina-app://install?id=night-owl" class="btn">Install</a>
                            </div>
                        </div>
                    </div>
                </body>
                </html>"##,
                store_css
            ))
        },
        "settings" => {
            let state = app.state::<AppDataStore>();
            let data = state.data.lock().unwrap();
            let settings = &data.settings;
            
            Some(format!(
                r#"<!DOCTYPE html>
                <html>
                <head>
                    <title>Settings</title>
                    <meta charset="UTF-8">
                    <style>
                        body {{ font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Helvetica, Arial, sans-serif; padding: 40px; background: #f9fafb; color: #111827; max-width: 600px; margin: 0 auto; }}
                        h1 {{ border-bottom: 1px solid #e5e7eb; padding-bottom: 20px; margin-bottom: 30px; }}
                        .group {{ background: white; padding: 25px; margin-bottom: 20px; border-radius: 12px; box-shadow: 0 1px 3px rgba(0,0,0,0.1); }}
                        .form-group {{ margin-bottom: 20px; }}
                        .form-group:last-child {{ margin-bottom: 0; }}
                        label {{ display: block; margin-bottom: 8px; font-weight: 500; font-size: 0.95em; color: #374151; }}
                        input[type="text"], select {{ width: 100%; padding: 10px; border: 1px solid #d1d5db; border-radius: 6px; font-size: 1em; box-sizing: border-box; transition: border-color 0.2s; }}
                        input[type="text"]:focus, select:focus {{ outline: none; border-color: #2563eb; ring: 2px solid #bfdbfe; }}
                        .checkbox-group {{ display: flex; align-items: center; }}
                        input[type="checkbox"] {{ width: 18px; height: 18px; margin-right: 10px; }}
                        button {{ background: #2563eb; color: white; border: none; padding: 12px 24px; border-radius: 8px; font-size: 1em; font-weight: 500; cursor: pointer; transition: background 0.2s; width: 100%; margin-top: 10px; }}
                        button:hover {{ background: #1d4ed8; }}
                    </style>
                </head>
                <body>
                    <h1>Settings</h1>
                    <div class="group">
                        <div class="form-group">
                            <label>Homepage URL</label>
                            <input type="text" id="homepage" value="{}">
                        </div>
                        <div class="form-group">
                            <label>Search Engine</label>
                            <select id="search_engine">
                                <option value="google" {}>Google</option>
                                <option value="bing" {}>Bing</option>
                                <option value="duckduckgo" {}>DuckDuckGo</option>
                            </select>
                        </div>
                    </div>
                    
                    <div class="group">
                        <div class="form-group">
                            <label>Theme</label>
                            <select id="theme">
                                <option value="dark" {}>Dark</option>
                                <option value="light" {}>Light</option>
                                <option value="system" {}>System</option>
                            </select>
                        </div>
                        <div class="form-group">
                            <label>Accent Color</label>
                            <input type="text" id="accent_color" value="{}">
                        </div>
                    </div>

                    <div class="group">
                        <div class="form-group checkbox-group">
                            <input type="checkbox" id="vertical_tabs" {}>
                            <label for="vertical_tabs" style="margin-bottom: 0">Vertical Tabs</label>
                        </div>
                        <div class="form-group checkbox-group">
                            <input type="checkbox" id="rounded_corners" {}>
                            <label for="rounded_corners" style="margin-bottom: 0">Rounded Corners</label>
                        </div>
                    </div>

                    <button onclick="save()">Save Settings</button>

                    <script>
                        function save() {{
                            const homepage = document.getElementById('homepage').value;
                            const search_engine = document.getElementById('search_engine').value;
                            const theme = document.getElementById('theme').value;
                            const accent_color = document.getElementById('accent_color').value;
                            const vertical_tabs = document.getElementById('vertical_tabs').checked;
                            const rounded_corners = document.getElementById('rounded_corners').checked;

                            window.__TAURI__.core.invoke('save_settings', {{
                                homepage, 
                                searchEngine: search_engine, 
                                theme, 
                                accentColor: accent_color, 
                                verticalTabs: vertical_tabs, 
                                roundedCorners: rounded_corners
                            }}).then(() => {{
                                alert('Settings saved!');
                            }}).catch(e => {{
                                alert('Error saving settings: ' + e);
                            }});
                        }}
                    </script>
                </body>
                </html>"#,
                settings.homepage,
                if settings.search_engine == "google" { "selected" } else { "" },
                if settings.search_engine == "bing" { "selected" } else { "" },
                if settings.search_engine == "duckduckgo" { "selected" } else { "" },
                if settings.theme == "dark" { "selected" } else { "" },
                if settings.theme == "light" { "selected" } else { "" },
                if settings.theme == "system" { "selected" } else { "" },
                settings.accent_color,
                if settings.vertical_tabs { "checked" } else { "" },
                if settings.rounded_corners { "checked" } else { "" }
            ))
        },
        "network" => {
            Some(r#"<!DOCTYPE html>
                <html>
                <head>
                    <title>Network Manager</title>
                    <meta charset="UTF-8">
                    <style>
                        body { font-family: system-ui, -apple-system, sans-serif; padding: 40px; background: #f9fafb; color: #111827; max-width: 800px; margin: 0 auto; }
                        h1 { border-bottom: 1px solid #e5e7eb; padding-bottom: 20px; margin-bottom: 30px; font-weight: 600; }
                        .card { background: white; padding: 25px; margin-bottom: 20px; border-radius: 12px; box-shadow: 0 1px 3px rgba(0,0,0,0.1); }
                        h2 { margin-top: 0; font-size: 1.2em; color: #374151; border-bottom: 1px solid #f3f4f6; padding-bottom: 10px; margin-bottom: 15px; }
                        .status-item { display: flex; justify-content: space-between; padding: 10px 0; border-bottom: 1px solid #f3f4f6; }
                        .status-item:last-child { border-bottom: none; }
                        .label { font-weight: 500; color: #6b7280; }
                        .value { font-family: monospace; color: #111827; }
                        .form-row { display: flex; gap: 10px; align-items: flex-end; }
                        .input-group { flex: 1; }
                        label { display: block; margin-bottom: 5px; font-size: 0.9em; font-weight: 500; color: #374151; }
                        input, select { width: 100%; padding: 8px 12px; border: 1px solid #d1d5db; border-radius: 6px; box-sizing: border-box; }
                        button { padding: 9px 16px; background: #2563eb; color: white; border: none; border-radius: 6px; cursor: pointer; font-weight: 500; transition: background 0.2s; }
                        button:hover { background: #1d4ed8; }
                        button.secondary { background: white; border: 1px solid #d1d5db; color: #374151; }
                        button.secondary:hover { background: #f3f4f6; }
                        button.danger { background: #dc2626; color: white; border: none; }
                        button.danger:hover { background: #b91c1c; }
                        #server-list { margin-top: 10px; }
                        .empty-list { color: #9ca3af; font-style: italic; padding: 10px 0; }
                    </style>
                </head>
                <body>
                    <h1>Network Manager</h1>
                    
                    <div class="card">
                        <h2>Sidecar Status</h2>
                        <div id="status-display">
                            <div class="status-item">
                                <span class="label">Status</span>
                                <span class="value" id="connection-status">Checking...</span>
                            </div>
                            <div class="status-item">
                                <span class="label">Active Servers</span>
                                <span class="value" id="active-count">0</span>
                            </div>
                        </div>
                    </div>

                    <div class="card">
                        <h2>Active Servers</h2>
                        <div id="server-list">
                            <div class="empty-list">No active servers</div>
                        </div>
                    </div>

                    <div class="card">
                        <h2>Start New Server</h2>
                        <div class="form-row">
                            <div class="input-group">
                                <label>Port</label>
                                <input type="number" id="port-input" value="8080" min="1" max="65535">
                            </div>
                            <div class="input-group">
                                <label>Type</label>
                                <select id="type-input">
                                    <option value="tcp">TCP</option>
                                </select>
                            </div>
                            <button onclick="startServer()">Start Server</button>
                        </div>
                    </div>

                    <script>
                        async function invokeNet(command, payload = {}) {
                            try {
                                const res = await window.__TAURI__.core.invoke('run_networking_command', { 
                                    command: command, 
                                    payload: JSON.stringify(payload) 
                                });
                                return JSON.parse(res);
                            } catch (e) {
                                console.error("Network Error:", e);
                                return { status: "error", message: e };
                            }
                        }

                        async function refreshStatus() {
                            const res = await invokeNet('status');
                            if (res.status === 'ok') {
                                document.getElementById('connection-status').textContent = 'Connected';
                                document.getElementById('connection-status').style.color = '#10b981';
                                
                                const servers = res.data.active_servers || [];
                                document.getElementById('active-count').textContent = servers.length;
                                
                                const list = document.getElementById('server-list');
                                if (servers.length === 0) {
                                    list.innerHTML = '<div class="empty-list">No active servers</div>';
                                } else {
                                    list.innerHTML = servers.map(addr => `
                                        <div class="status-item">
                                            <span class="value">${addr}</span>
                                            <button class="secondary danger" style="padding: 4px 8px; font-size: 0.8em;" onclick="stopServer('${addr}')">Stop</button>
                                        </div>
                                    `).join('');
                                }
                            } else {
                                document.getElementById('connection-status').textContent = 'Error';
                                document.getElementById('connection-status').style.color = '#dc2626';
                            }
                        }

                        async function startServer() {
                            const port = parseInt(document.getElementById('port-input').value);
                            const type = document.getElementById('type-input').value;
                            
                            const res = await invokeNet('start_server', { port, type });
                            if (res.status === 'ok') {
                                alert('Server started!');
                                refreshStatus();
                            } else {
                                alert('Error: ' + res.message);
                            }
                        }

                        async function stopServer(addr) {
                            // Parse port from address (e.g., ":8080")
                            const port = parseInt(addr.replace(':', ''));
                            if (confirm(`Stop server on port ${port}?`)) {
                                const res = await invokeNet('stop_server', { port, type: 'tcp' });
                                if (res.status === 'ok') {
                                    refreshStatus();
                                } else {
                                    alert('Error: ' + res.message);
                                }
                            }
                        }

                        // Initial refresh
                        refreshStatus();
                        
                        // Refresh every 5 seconds
                        setInterval(refreshStatus, 5000);
                    </script>
                </body>
                </html>"#.to_string()
            )
        },
        _ => Some(format!(
            r#"<!DOCTYPE html>
            <html>
            <head>
                <title>404 Not Found</title>
                <meta charset="UTF-8">
                <style>
                    body {{ font-family: system-ui, -apple-system, sans-serif; height: 100vh; display: flex; align-items: center; justify-content: center; background: #f9fafb; color: #374151; margin: 0; }}
                    .container {{ text-align: center; }}
                    h1 {{ font-size: 4em; margin: 0; color: #1f2937; }}
                    p {{ font-size: 1.2em; margin-top: 10px; }}
                </style>
            </head>
            <body>
                <div class="container">
                    <h1>404</h1>
                    <p>Page not found: {}</p>
                </div>
            </body>
            </html>"#,
            path
        ))
    }
}

#[tauri::command]
fn force_internal_navigate(app: AppHandle, label: String, mut url: String) {
    println!("Rust: force_internal_navigate tab {} to {}", label, url);

    // Standardize URL to ensure same-origin (lumina-app://localhost/)
    if url.starts_with("lumina://") {
        url = url.replace("lumina://", "lumina-app://localhost/");
    } else if url.starts_with("lumina-app://") {
         let scheme = "lumina-app://";
         if let Some(rest) = url.strip_prefix(scheme) {
             if !rest.starts_with("localhost/") && rest != "localhost" {
                 // Convert lumina-app://page to lumina-app://localhost/page
                 url = format!("{}localhost/{}", scheme, rest);
             }
         }
    }

    if let Some(webview) = app.get_webview(&label) {
         let _ = webview.set_focus();
         
         // Check if it's an internal page
         let mut is_internal = false;
         let mut internal_html = None;

         if url.starts_with("lumina-app://") || url.starts_with("lumina://") {
            let scheme_strip = if url.starts_with("lumina-app:") { "lumina-app:" } else { "lumina:" };
            let without_scheme = url.strip_prefix(scheme_strip).unwrap_or(&url);
            let without_slashes = without_scheme.trim_start_matches('/');
            let path_and_query = without_slashes.strip_prefix("localhost").unwrap_or(without_slashes);
            let full_path = path_and_query.trim_start_matches('/');
            
            // Split path and query/hash
            let (path, _query) = if let Some(idx) = full_path.find('?') {
                (&full_path[..idx], &full_path[idx..])
            } else if let Some(idx) = full_path.find('#') {
                 (&full_path[..idx], &full_path[idx..])
            } else {
                (full_path, "")
            };
            
            let path = path.trim_end_matches('/');
            
            if let Some(html) = get_internal_page_html(&app, path) {
                is_internal = true;
                internal_html = Some(html);
            }
         }

         if is_internal {
             if let Some(html) = internal_html {
                 // Escape HTML for JS string
                 let escaped_html = html.replace("\\", "\\\\").replace("'", "\\'").replace("\n", "\\n").replace("\r", "");
                 let js = format!(
                     "window.stop(); document.open(); document.write('{}'); document.close(); try {{ history.pushState(null, '', '{}'); }} catch(e) {{ console.warn('PushState failed (likely origin mismatch), but content loaded:', e); }}", 
                     escaped_html, url
                 );
                 let _ = webview.eval(&js);
             }
         } else {
             // Fallback for external URLs or if parsing failed
             let _ = webview.eval(format!("window.location.replace('{}')", url).as_str());
         }
    }
}

#[tauri::command]
fn go_back(app: AppHandle, label: String) {
    if let Some(webview) = app.get_webview(&label) {
        let _ = webview.eval("window.history.back()");
    }
}

#[tauri::command]
fn go_forward(app: AppHandle, label: String) {
    if let Some(webview) = app.get_webview(&label) {
        let _ = webview.eval("window.history.forward()");
    }
}

#[tauri::command]
fn refresh(app: AppHandle, label: String) {
    if let Some(webview) = app.get_webview(&label) {
        let _ = webview.reload();
    }
}

#[tauri::command]
fn add_history_item(state: tauri::State<'_, AppDataStore>, history_manager: tauri::State<'_, HistoryManager>, url: String, title: String) {
    // Legacy JSON store (optional, maybe keep for backup or remove later)
    state.add_history(url.clone(), title.clone());
    state.save();

    // SQLite Store
    if let Err(e) = history_manager.add_visit(url, title) {
        eprintln!("Failed to add history item: {}", e);
    }
}

#[tauri::command]
fn update_history_title(app: AppHandle, history_manager: tauri::State<'_, HistoryManager>, label: String, url: String, title: String) {
    if let Err(e) = history_manager.update_title(url, title.clone()) {
         eprintln!("Failed to update history title: {}", e);
    }
    // Also emit tab-updated so UI reflects the real title
    let _ = app.emit("tab-updated", TabUpdatedPayload { label, title: Some(title), favicon: None });
}

#[tauri::command]
fn search_history(history_manager: tauri::State<'_, HistoryManager>, data_store: tauri::State<'_, AppDataStore>, query: String) -> Vec<history_manager::HistoryItem> {
    if query.starts_with("@b") {
        // Search Bookmarks (Favorites)
        let q = query.replace("@b", "").trim().to_lowercase();
        let favorites = data_store.data.lock().unwrap().favorites.clone();
        favorites.into_iter()
            .filter(|f| f.url.to_lowercase().contains(&q) || f.title.to_lowercase().contains(&q))
            .map(|f| history_manager::HistoryItem {
                url: f.url,
                title: f.title,
                visit_count: 100, // Boost favorites
                last_visit: chrono::Utc::now().timestamp(),
            })
            .collect()
    } else {
        // Search History (default or @h)
        let q = if query.starts_with("@h") {
            query.replace("@h", "").trim().to_string()
        } else {
            query
        };
        
        match history_manager.search(&q) {
            Ok(items) => items,
            Err(e) => {
                eprintln!("Search error: {}", e);
                Vec::new()
            }
        }
    }
}

#[tauri::command]
fn get_history(state: tauri::State<'_, AppDataStore>) -> Vec<HistoryItem> {
    state.data.lock().unwrap().history.clone()
}

#[tauri::command]
fn get_recent_history(history_manager: tauri::State<'_, HistoryManager>) -> Vec<history_manager::HistoryItem> {
    match history_manager.get_recent(50) {
        Ok(items) => items,
        Err(e) => {
            eprintln!("Failed to get recent history: {}", e);
            Vec::new()
        }
    }
}

#[tauri::command]
fn add_favorite(state: tauri::State<'_, AppDataStore>, url: String, title: String) {
    state.add_favorite(url, title);
    state.save();
}

#[tauri::command]
fn remove_favorite(state: tauri::State<'_, AppDataStore>, url: String) {
    state.remove_favorite(url);
    state.save();
}

#[tauri::command]
fn get_favorites(state: tauri::State<'_, AppDataStore>) -> Vec<FavoriteItem> {
    state.data.lock().unwrap().favorites.clone()
}

#[tauri::command]
fn get_settings(state: tauri::State<'_, AppDataStore>) -> AppSettings {
    state.data.lock().unwrap().settings.clone()
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
fn save_settings(state: tauri::State<'_, AppDataStore>, app: AppHandle, homepage: String, search_engine: String, theme: String, accent_color: String, vertical_tabs: bool, rounded_corners: bool) {
    state.update_settings(homepage, search_engine, theme, accent_color, vertical_tabs, rounded_corners);
    state.save();
    let _ = update_layout(app.state::<UiState>(), app.clone(), app.state::<AppDataStore>());
}

#[tauri::command]
fn open_file(_path: String) {
    #[cfg(target_os = "windows")]
    {
        let _ = std::process::Command::new("explorer")
            .arg(_path)
            .spawn();
    }
}

#[tauri::command]
fn show_in_folder(path: String) {
    #[cfg(target_os = "windows")]
    {
        let _ = std::process::Command::new("explorer")
            .args(["/select,", &path])
            .spawn();
    }
    #[cfg(not(target_os = "windows"))]
    let _ = path;
}

#[tauri::command]
fn toggle_reader_mode(app: AppHandle, label: String) {
    if let Some(webview) = app.get_webview(&label) {
        let script = r#"
            (function() {
                if (window.__readerModeActive) {
                    window.location.reload();
                    return;
                }

                function findContent() {
                    const selectors = ['article', 'main', '.content', '#content', '.post', '.entry', '.article', '#article'];
                    for (let sel of selectors) {
                        let el = document.querySelector(sel);
                        if (el && el.innerText.length > 200) return el;
                    }
                    
                    let divs = document.getElementsByTagName('div');
                    let bestDiv = null;
                    let maxP = 0;
                    for (let div of divs) {
                        let pCount = div.getElementsByTagName('p').length;
                        if (pCount > maxP) {
                            maxP = pCount;
                            bestDiv = div;
                        }
                    }
                    return bestDiv || document.body;
                }

                try {
                    const content = findContent().cloneNode(true);
                    
                    const style = `
                        body {
                            background-color: #f4ecd8 !important;
                            color: #333 !important;
                            font-family: Georgia, 'Times New Roman', serif !important;
                            font-size: 20px !important;
                            line-height: 1.6 !important;
                            max-width: 800px !important;
                            margin: 0 auto !important;
                            padding: 40px 20px !important;
                        }
                        img { max-width: 100%; height: auto; display: block; margin: 20px auto; }
                        a { color: #005a9c; text-decoration: underline; }
                        h1, h2, h3, h4 { font-family: Helvetica, Arial, sans-serif; color: #111; margin-top: 1.5em; }
                        p { margin-bottom: 1.5em; }
                        pre, code { background: rgba(0,0,0,0.05); padding: 2px 4px; border-radius: 3px; }
                        blockquote { border-left: 4px solid #ccc; padding-left: 16px; margin-left: 0; color: #555; font-style: italic; }
                        
                        /* Hide everything else */
                        nav, header, footer, aside, .sidebar, .menu, .ad, .ads, .advertisement, iframe, .popup, .modal { display: none !important; }
                    `;

                    document.head.innerHTML = '';
                    const styleEl = document.createElement('style');
                    styleEl.textContent = style;
                    document.head.appendChild(styleEl);

                    document.body.innerHTML = '';
                    document.body.appendChild(content);
                    window.__readerModeActive = true;
                    console.log("Reader mode activated");
                } catch(e) {
                    console.error("Reader mode failed:", e);
                }
            })();
        "#;
        let _ = webview.eval(script);
    }
}

fn calculate_layout(logical_size: tauri::LogicalSize<f64>, vertical_tabs: bool, menu_open: bool, suggestions_height: f64) -> (f64, f64, f64, f64, f64) {
    let top_bar_height = 104.0 + suggestions_height;
    let sidebar_width = 200.0;
    let menu_width = 320.0;
    let toolbar_height = 60.0;

    if vertical_tabs {
        let main_height = logical_size.height;
        let x = sidebar_width;
        let y = toolbar_height; 
        let mut width = logical_size.width - sidebar_width;
        if menu_open { width -= menu_width; }
        if width < 0.0 { width = 0.0; }
        (main_height, x, y, width, logical_size.height - toolbar_height)
    } else {
        let mut width = logical_size.width;
        if menu_open { width -= menu_width; }
        if width < 0.0 { width = 0.0; }
        let main_height = if menu_open { logical_size.height } else { top_bar_height };
        (main_height, 0.0, top_bar_height, width, logical_size.height - top_bar_height)
    }
}

#[tauri::command]
fn update_layout(state: tauri::State<'_, UiState>, app: AppHandle, data_store: tauri::State<'_, AppDataStore>) -> Result<(), String> {
    let menu_open = state.sidebar_open.load(std::sync::atomic::Ordering::Relaxed);
    let suggestions_height = state.suggestions_height.load(std::sync::atomic::Ordering::Relaxed) as f64;
    let vertical_tabs = data_store.data.lock().unwrap().settings.vertical_tabs;
    let main_window = app.get_window("main").ok_or("Main window not found")?;
    let window_size = main_window.inner_size().map_err(|e| e.to_string())?;
    let scale_factor = main_window.scale_factor().map_err(|e| e.to_string())?;
    let logical_size = window_size.to_logical::<f64>(scale_factor);
    
    let (main_height, x, y, width, height) = calculate_layout(logical_size, vertical_tabs, menu_open, suggestions_height);
    
    if let Some(main_webview) = app.get_webview("main") {
        main_webview.set_size(tauri::LogicalSize::new(logical_size.width, main_height)).map_err(|e| e.to_string())?;
        if menu_open { let _ = main_webview.set_focus(); }
    }
    let webviews = app.webviews();
    for webview in webviews {
        let webview_instance = &webview.1;
        if webview_instance.label() != "main" {
            let _ = webview_instance.set_position(tauri::LogicalPosition::new(x, y));
            let _ = webview_instance.set_size(tauri::LogicalSize::new(width, height));
        }
    }
    Ok(())
}

#[tauri::command]
fn set_suggestions_height(state: tauri::State<'_, UiState>, app: AppHandle, data_store: tauri::State<'_, AppDataStore>, height: u32) -> Result<(), String> {
    state.suggestions_height.store(height, std::sync::atomic::Ordering::Relaxed);
    update_layout(state, app, data_store)
}

#[tauri::command]
fn toggle_sidebar(state: tauri::State<'_, UiState>, app: AppHandle, data_store: tauri::State<'_, AppDataStore>, open: bool) -> Result<(), String> {
    state.sidebar_open.store(open, std::sync::atomic::Ordering::Relaxed);
    update_layout(state, app, data_store)
}


#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct TabNavigationPayload {
    label: String,
    url: String,
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct DownloadStartedPayload {
    url: String,
    file_name: String,
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct DownloadFinishedPayload {
    url: String,
    success: bool,
    path: Option<String>,
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct DownloadProgressPayload {
    url: String,
    progress: u64,
    total: u64,
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct TabCreatedPayload {
    label: String,
    url: String,
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct TabUpdatedPayload {
    label: String,
    title: Option<String>,
    favicon: Option<String>,
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct TabClosedPayload {
    label: String,
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct TabPwaPayload {
    label: String,
    icon_url: Option<String>,
}

struct PwaState {
    icons: std::sync::Mutex<std::collections::HashMap<String, String>>,
}

#[tauri::command]
async fn pwa_detected(app: AppHandle, state: tauri::State<'_, PwaState>, label: String, icon_url: Option<String>) -> Result<(), String> {
    if let Some(url) = &icon_url {
        state.icons.lock().unwrap().insert(label.clone(), url.clone());
    }
    app.emit("pwa-can-install", TabPwaPayload { label, icon_url }).map_err(|e| e.to_string())
}

#[tauri::command]
async fn install_pwa(app: AppHandle, state: tauri::State<'_, PwaState>, label: String) -> Result<(), String> {
    // Get stored icon URL if available
    let icon_url = state.icons.lock().unwrap().get(&label).cloned();
    let icon_url_js = if let Some(u) = icon_url {
        format!("'{}'", u)
    } else {
        "null".to_string()
    };

    if let Some(webview) = app.get_webview(&label) {
        let script = format!(r#"
            (async function() {{
                var knownIconUrl = {};
                if (window.deferredPrompt) {{
                    console.log("Triggering PWA install prompt...");
                    window.deferredPrompt.prompt();
                    window.deferredPrompt.userChoice.then((choiceResult) => {{
                        if (choiceResult.outcome === 'accepted') {{
                            console.log('User accepted the install prompt');
                        }} else {{
                            console.log('User dismissed the install prompt');
                        }}
                        window.deferredPrompt = null;
                    }});
                }} else {{
                    console.warn("No deferredPrompt found, falling back to manual PWA window...");
                    var title = document.title || window.location.href;
                    
                    var faviconUrl = knownIconUrl;
                    if (!faviconUrl) {{
                        var links = document.querySelectorAll("link[rel*='icon']");
                        if (links.length > 0) {{
                            faviconUrl = links[0].href;
                        }}
                    }}

                    try {{
                        var args = {{ url: window.location.href, title: title, faviconUrl: faviconUrl }};
                        if (window.__TAURI__ && window.__TAURI__.core) {{
                            await window.__TAURI__.core.invoke('open_pwa_window', args);
                        }} else if (window.__TAURI__ && window.__TAURI__.invoke) {{
                            await window.__TAURI__.invoke('open_pwa_window', args);
                        }} else {{
                             // Fallback to our custom invoke
                             if (typeof invoke === 'function') {{
                                 invoke('open_pwa_window', args);
                             }} else {{
                                 console.error("No invoke mechanism found");
                             }}
                        }}
                    }} catch(e) {{
                        console.error("Failed to open PWA window:", e);
                    }}
                }}
            }})();
        "#, icon_url_js);
        webview.eval(&script).map_err(|e| e.to_string())?;
    }
    Ok(())
}

async fn save_icon(app: &AppHandle, bytes: &[u8]) -> Option<std::path::PathBuf> {
    let app_dir = app.path().app_data_dir().ok()?;
    let icons_dir = app_dir.join("icons");
    if !icons_dir.exists() {
        let _ = std::fs::create_dir_all(&icons_dir);
    }

    // Try to load image to convert to ICO (Lumina v0.2.5 PNG->ICO Converter)
    // We use a blocking task because image decoding/encoding is CPU intensive
    let bytes_vec = bytes.to_vec();
    let icons_dir_clone = icons_dir.clone();
    
    let converted_path = tokio::task::spawn_blocking(move || {
        if let Ok(img) = image::load_from_memory(&bytes_vec) {
            // Resize to 256x256 for Windows compatibility (Standard Large Icon)
            // Windows icons behave best when they are 256x256
            let resized = img.resize(256, 256, image::imageops::FilterType::Lanczos3);
            
            let filename = format!("icon_{}.ico", chrono::Utc::now().timestamp_micros());
            let path = icons_dir_clone.join(&filename);
            
            if let Ok(file) = std::fs::File::create(&path) {
                let mut writer = std::io::BufWriter::new(file);
                // Convert to ICO
                if resized.write_to(&mut writer, image::ImageFormat::Ico).is_ok() {
                    return Some(path);
                }
            }
        }
        None
    }).await.ok().flatten();

    if let Some(path) = converted_path {
        return Some(path);
    }
    
    // Fallback: Just save as is if conversion failed (e.g. SVG or format error)
    // BUT for shortcuts we really want ICO. If we can't make ICO, we might skip returning a path
    // or return it and hope for the best (but likely fail on Windows).
    // Let's try to infer extension.
    let extension = "png"; // Default to png if we can't guess
    
    let filename = format!("icon_{}.{}", chrono::Utc::now().timestamp_micros(), extension);
    let path = icons_dir.join(&filename);
    
    let mut file = tokio::fs::File::create(&path).await.ok()?;
    file.write_all(bytes).await.ok()?;
    
    Some(path)
}

async fn download_icon(app: &AppHandle, url: &str) -> Option<std::path::PathBuf> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/144.0.0.0 Safari/537.36 Edg/144.0.0.0")
        .build()
        .ok()?;
        
    let response = client.get(url).send().await.ok()?;
    let bytes = response.bytes().await.ok()?;
    
    save_icon(app, &bytes).await
}

#[derive(Clone, serde::Serialize)]
struct WindowInfo {
    label: String,
    title: String,
    url: String, // We might not be able to get the exact URL easily without tracking it, but we can try
}

fn sanitize_pwa_label(url: &str) -> String {
    // Extract hostname or use a hash if not parseable
    if let Ok(parsed) = url::Url::parse(url) {
        if let Some(host) = parsed.host_str() {
            let safe_host: String = host.chars()
                .map(|c| if c.is_alphanumeric() { c } else { '-' })
                .collect();
            return format!("pwa-{}", safe_host);
        }
    }
    // Fallback to timestamp if parsing fails
    format!("pwa-{}", chrono::Utc::now().timestamp_micros())
}

fn get_pwa_init_script(label: &str, invoke_key: &str) -> String {
    format!(r#"
        (function() {{
            window.__TAB_LABEL__ = "{}";
            window.__TAURI_INVOKE_KEY__ = "{}";
            
            // Custom IPC for PWA windows
            function invoke(cmd, args) {{
                // Reuse the same logic as create_tab
                if (window.__TAURI__ && window.__TAURI__.core) {{
                    window.__TAURI__.core.invoke(cmd, args).catch(err => console.error("Tauri invoke failed:", err));
                    return;
                }}
                
                if (typeof window.__IPC_COUNTER === 'undefined') window.__IPC_COUNTER = 0;
                window.__IPC_COUNTER = (window.__IPC_COUNTER + 1) % 4000000000;
                var callbackId = window.__IPC_COUNTER;
                var noOp = function(res) {{}};
                
                try {{
                     if (window.__TAURI_INTERNALS__) {{
                         if (!window.__TAURI_INTERNALS__.callbacks) window.__TAURI_INTERNALS__.callbacks = {{}};
                         window.__TAURI_INTERNALS__.callbacks[callbackId] = {{ resolve: noOp, reject: noOp }};
                     }}
                }} catch(e) {{}}
                
                setTimeout(function() {{
                    try {{
                        if (window.__TAURI_INTERNALS__ && window.__TAURI_INTERNALS__.callbacks) {{
                             delete window.__TAURI_INTERNALS__.callbacks[callbackId];
                        }}
                    }} catch(e) {{}}
                }}, 60000);

                var msg = {{
                    cmd: cmd,
                    callback: callbackId, 
                    error: callbackId,
                    payload: args,
                    __TAURI_INVOKE_KEY__: window.__TAURI_INVOKE_KEY__
                }};
                
                if (window.chrome && window.chrome.webview) {{
                    window.chrome.webview.postMessage(msg);
                }} else if (window.webkit && window.webkit.messageHandlers && window.webkit.messageHandlers.ipc) {{
                    window.webkit.messageHandlers.ipc.postMessage(msg);
                }}
            }}

            // Override window.open
            window.open = function(url, target, features) {{
                if (url) {{
                    // Call create_tab directly on the main window via our fixed command
                    invoke('create_tab', {{ label: 'new-tab-' + Date.now() + '-' + Math.floor(Math.random() * 1000), url: url }});
                }}
                return null;
            }};
            
            // Handle _blank links
            document.addEventListener('click', (e) => {{
                let target = e.target;
                while(target && target.tagName !== 'A') target = target.parentElement;
                if (target && target.tagName === 'A' && target.target === '_blank') {{
                    e.preventDefault();
                    invoke('create_tab', {{ label: 'new-tab-' + Date.now() + '-' + Math.floor(Math.random() * 1000), url: target.href }});
                }}
            }}, true);

            document.addEventListener('auxclick', (e) => {{
                if (e.button === 1) {{
                    let target = e.target;
                    while(target && target.tagName !== 'A') target = target.parentElement;
                    if (target && target.tagName === 'A') {{
                        e.preventDefault();
                        invoke('create_tab', {{ label: 'new-tab-' + Date.now() + '-' + Math.floor(Math.random() * 1000), url: target.href }});
                    }}
                }}
            }}, true);

            // Context Menu Override
            document.addEventListener('contextmenu', (e) => {{
                let target = e.target;
                let linkUrl = null;
                while(target && target.tagName !== 'A') target = target.parentElement;
                if (target && target.tagName === 'A' && target.href) {{
                    linkUrl = target.href;
                }}

                if (linkUrl) {{
                    e.preventDefault();
                    e.stopPropagation(); 
                    
                    const existing = document.getElementById('lumina-context-menu');
                    if (existing) existing.remove();

                    const menu = document.createElement('div');
                    menu.id = 'lumina-context-menu';
                    menu.style.position = 'fixed';
                    menu.style.top = e.clientY + 'px';
                    menu.style.left = e.clientX + 'px';
                    menu.style.zIndex = '2147483647'; 
                    menu.style.background = '#292a2d';
                    menu.style.border = '1px solid #3c4043';
                    menu.style.borderRadius = '4px';
                    menu.style.padding = '4px 0';
                    menu.style.color = '#e8eaed';
                    menu.style.fontFamily = 'system-ui, sans-serif';
                    menu.style.fontSize = '13px';
                    menu.style.userSelect = 'none';

                    const createItem = (text, onClick) => {{
                        const item = document.createElement('div');
                        item.innerText = text;
                        item.style.padding = '6px 12px';
                        item.style.cursor = 'pointer';
                        item.onmouseenter = () => item.style.background = '#3c4043';
                        item.onmouseleave = () => item.style.background = 'transparent';
                        item.onclick = (ev) => {{
                            ev.stopPropagation(); 
                            onClick();
                            menu.remove();
                        }};
                        return item;
                    }};

                    menu.appendChild(createItem('Open Link in New Tab', () => {{
                         invoke('create_tab', {{ label: 'tab-' + Date.now() + '-' + Math.floor(Math.random() * 1000), url: linkUrl }});
                    }}));
                    
                    // Add copy link
                    menu.appendChild(createItem('Copy Link Address', () => {{
                         navigator.clipboard.writeText(linkUrl);
                    }}));
                    
                    document.body.appendChild(menu);
                    
                    const closeMenu = () => {{
                        menu.remove();
                        document.removeEventListener('click', closeMenu);
                        document.removeEventListener('contextmenu', closeMenu);
                    }};
                    setTimeout(() => {{
                        document.addEventListener('click', closeMenu);
                        document.addEventListener('contextmenu', (e) => {{
                             if (e.target.closest('#lumina-context-menu')) return;
                             closeMenu();
                        }});
                    }}, 100);
                }}
            }}, true);

        }})();
    "#, label, invoke_key)
}

#[tauri::command]
async fn open_pwa_window(app: AppHandle, url: String, title: String, favicon_url: Option<String>, icon_data: Option<String>) -> Result<(), String> {
    let label = sanitize_pwa_label(&url);
    
    // Check if window already exists
    if let Some(window) = app.get_webview_window(&label) {
        let _ = window.set_focus();
        return Ok(());
    }
    
    // Get Icon Path
    let icon_path = if let Some(data) = icon_data {
        // Decode base64
        if let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(data) {
             save_icon(&app, &bytes).await
        } else {
             None
        }
    } else if let Some(f_url) = favicon_url {
        download_icon(&app, &f_url).await
    } else {
        None
    };

    let icon_path_clone = icon_path.clone();

    // Create Desktop Shortcut
    let _ = create_desktop_shortcut(&title, &url, icon_path);

    let app_clone = app.clone();
    let label_clone = label.clone();

    // Inject PWA script for handling window.open and context menu
    let invoke_key = app.invoke_key();
    let script = get_pwa_init_script(&label, invoke_key);

    let mut builder = tauri::WebviewWindowBuilder::new(&app, &label, tauri::WebviewUrl::External(url.parse().map_err(|e: url::ParseError| e.to_string())?))
        .title(&title)
        .initialization_script(&script);

    if let Some(path) = icon_path_clone {
        if let Ok(img) = image::open(&path) {
             let rgba = img.to_rgba8();
             let (width, height) = rgba.dimensions();
             let rgba_vec = rgba.into_raw();
             let tauri_img = tauri::image::Image::new_owned(rgba_vec, width, height);
             match builder.icon(tauri_img) {
                 Ok(b) => builder = b,
                 Err(e) => return Err(format!("Failed to set window icon: {}", e)),
             }
        }
    }

    #[cfg(target_os = "windows")]
    {
        builder = builder.user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/144.0.0.0 Safari/537.36 Edg/144.0.0.0");
        builder = builder.additional_browser_args("--ignore-certificate-errors");
    }
    #[cfg(target_os = "linux")]
    {
        builder = builder.user_agent("Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/144.0.0.0 Safari/537.36");
    }
    #[cfg(target_os = "macos")]
    {
        builder = builder.user_agent("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/144.0.0.0 Safari/537.36");
    }

    builder.inner_size(1024.0, 768.0)
        .decorations(true) // Enable native window controls (Close, Minimize, Maximize)
        .focused(true)
        .initialization_script(get_lumina_stealth_script())
        .on_web_resource_request(move |request, response| {
            let referer = request.headers().get("referer").and_then(|h| h.to_str().ok());
            if check_adblock_url(&request.uri().to_string(), referer, &label_clone, &app_clone) {
                *response = tauri::http::Response::builder()
                    .status(403)
                    .body(std::borrow::Cow::Owned(Vec::new()))
                    .unwrap();
            }
        })
        .build()
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
fn get_open_windows(app: AppHandle) -> Vec<WindowInfo> {
    let mut windows = Vec::new();
    for (label, window) in app.webview_windows() {
        if label != "main" && !label.starts_with("flash-") {
            let title = window.title().unwrap_or_else(|_| label.clone());
            // For PWAs, we use the window URL which is typically the app URL.
            // Note: This might be the initial URL if navigation handling is complex, 
            // but for single-page apps/PWAs it's usually sufficient.
            windows.push(WindowInfo {
                label,
                title,
                url: window.url().map(|u| u.to_string()).unwrap_or_default(),
            });
        }
    }
    windows
}

#[tauri::command]
fn focus_window(app: AppHandle, label: String) {
    if let Some(window) = app.get_webview_window(&label) {
        let _ = window.set_focus();
        if window.is_minimized().unwrap_or(false) {
            let _ = window.unminimize();
        }
    }
}

const BLOCKED_DOMAINS: &[&str] = &[
    "doubleclick.net",
    "googleadservices.com",
    "googlesyndication.com",
    "adnxs.com",
    "rubiconproject.com",
    "taboola.com",
    "outbrain.com",
    "amazon-adsystem.com",
    "adservice.google.com",
    "moatads.com",
    "criteo.com",
    "pubmatic.com",
    "openx.net",
    "smartadserver.com",
];

#[tauri::command]
fn clean_page(app: AppHandle) {
    let script = r#"
        (function() {
            // Domain Awareness for Clean Page
            const host = window.location.hostname;
            if (host.includes('google.com') || host.includes('gemini') || host.includes('youtube.com')) {
                 console.log("Lumina Clean Page: Friendly domain detected (" + host + "), aborting force clean.");
                 return;
            }

            const elements = document.querySelectorAll('div, iframe, section, aside, span, a, img, button');
            let count = 0;
            elements.forEach(el => {
                const style = window.getComputedStyle(el);
                if (style.position === 'fixed' || style.position === 'absolute') {
                    // Check if it's likely an overlay/ad (high z-index or full width/height)
                    if ((style.zIndex && parseInt(style.zIndex) > 10) || 
                        (el.offsetWidth > window.innerWidth * 0.9 && el.offsetHeight > window.innerHeight * 0.9)) {
                         el.remove();
                         count++;
                    }
                }
            });
            console.log("Lumina Clean Page: Removed " + count + " floating elements.");
        })();
    "#;
    
    for (label, window) in app.webview_windows() {
        if label != "main" {
            let _ = window.eval(script);
        }
    }
}

#[tauri::command]
async fn open_flash_window(app: AppHandle, url: String) -> Result<(), String> {
    let label = format!("flash-{}", chrono::Utc::now().timestamp_micros());
    let app_handle = app.clone();
    let label_clone = label.clone();
    
    let mut builder = tauri::WebviewWindowBuilder::new(&app, &label, tauri::WebviewUrl::External(url.parse().map_err(|e: url::ParseError| e.to_string())?))
        .title("Flash Tab");

    #[cfg(target_os = "windows")]
    {
        builder = builder.user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36 Edg/120.0.0.0");
    }
    #[cfg(target_os = "linux")]
    {
        builder = builder.user_agent("Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36");
    }
    #[cfg(target_os = "macos")]
    {
        builder = builder.user_agent("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36");
    }

    builder.inner_size(800.0, 600.0)
        .decorations(false)
        .always_on_top(true)
        .center()
        .focused(true)
        .skip_taskbar(true)
        .initialization_script(get_lumina_stealth_script())
        .on_web_resource_request(move |request, response| {
            let referer = request.headers().get("referer").and_then(|h| h.to_str().ok());
            if check_adblock_url(&request.uri().to_string(), referer, &label_clone, &app_handle) {
                *response = tauri::http::Response::builder()
                    .status(403)
                    .body(std::borrow::Cow::Owned(Vec::new()))
                    .unwrap();
            }
        })
        .build()
        .map_err(|e| e.to_string())?;
    Ok(())
}

fn get_lumina_stealth_script() -> String {
    r#"
    (function() {
        let host = window.location.hostname;
        
        // Try to get parent host if current host is empty (e.g. about:blank iframe)
        try {
            if ((!host || host === '') && window.parent && window.parent.location && window.parent.location.hostname) {
                host = window.parent.location.hostname;
            }
        } catch(e) {
            // Access denied to parent (cross-origin)
        }
        
        console.log("Lumina Stealth Protocol: Activated on " + host);

        // 0. Monkey-Patch IntersectionObserver to prevent ad script crashes
        const originalObserve = IntersectionObserver.prototype.observe;
        IntersectionObserver.prototype.observe = function(target) {
            if (!target || !(target instanceof Element)) {
                // Silently ignore invalid targets (likely removed ads)
                return;
            }
            return originalObserve.apply(this, arguments);
        };

        // 0.1 Monkey-Patch XHR to fix Transfermarkt malformed URLs
        const originalOpen = XMLHttpRequest.prototype.open;
        XMLHttpRequest.prototype.open = function(method, url, ...args) {
            if (typeof url === 'string' && (window.location.hostname.includes('transfermarkt') || window.location.hostname.includes('mackolik'))) {
                 // Remove spaces (%20) from URL path which break AJAX calls
                 if (url.includes('%20') || url.includes(' ')) {
                     const cleanUrl = url.replace(/%20/g, '').replace(/\s/g, '');
                     console.log("Lumina Fix: Corrected malformed URL", url, "->", cleanUrl);
                     url = cleanUrl;
                 }
            }
            return originalOpen.call(this, method, url, ...args);
        };

        const isFriendly = host.includes('google.com') || host.includes('gemini') || host.includes('youtube.com') || host.includes('transfermarkt') || host.includes('cdn.privacy-mgmt.com') || host.includes('consensu') || host.includes('cmp') || host.includes('quantcast');

        // Force remove aria-hidden from body to prevent accessibility lock
        if (isFriendly) {
            const ariaObserver = new MutationObserver((mutations) => {
                mutations.forEach((mutation) => {
                    if (mutation.type === 'attributes' && mutation.attributeName === 'aria-hidden') {
                        if (document.body.getAttribute('aria-hidden') === 'true') {
                            document.body.removeAttribute('aria-hidden');
                        }
                    }
                });
            });
            if (document.body) {
                ariaObserver.observe(document.body, { attributes: true });
                document.body.removeAttribute('aria-hidden');
            } else {
                document.addEventListener('DOMContentLoaded', () => {
                    ariaObserver.observe(document.body, { attributes: true });
                    document.body.removeAttribute('aria-hidden');
                });
            }
        }

        // 1. CSS Injection Strategy
        // Split into "Core/High-Confidence" (Always Safe) and "Aggressive" (Skip on Friendly)
        
        const coreAdStyles = `
            /* High-Confidence Ad Patterns - Safe to block everywhere */
            iframe[src*="ads"], iframe[id*="google_ads"], iframe[src*="doubleclick"], 
            iframe[src*="amazon-adsystem"], iframe[src*="adnxs"], iframe[src*="teads"],
            
            /* Google & Networks */
            ins.adsbygoogle, div[id^="google_ads_"],
            
            /* Native Ad Widgets */
            div[id*="taboola"], div[class*="taboola"],
            div[id*="outbrain"], div[class*="outbrain"],
            
            /* Specific Ad Iframes */
            iframe[title*="Advertisement"], iframe[title*="reklam"]
            
            { display: none !important; visibility: hidden !important; height: 0 !important; width: 0 !important; pointer-events: none !important; overflow: hidden !important; }
        `;

        const aggressiveAdStyles = `
            /* Common Ad Containers - Risk of False Positives */
            div[class*="ad-"], div[id*="ad-"],
            div[class*="ads-"], div[id*="ads-"],
            div[class*="sponsor"], div[id*="sponsor"],
            div[class*="banner"], div[id*="banner"],
            
            /* Overlays & Popups - Can kill Login Modals */
            div[class*="popup"][class*="ad"], div[class*="modal"][class*="ad"],
            div[id*="popup"][id*="ad"], div[id*="modal"][id*="ad"],
            
            /* Video Ads */
            div[class*="video-ad"], .ad-showing
            
            { display: none !important; visibility: hidden !important; height: 0 !important; width: 0 !important; pointer-events: none !important; overflow: hidden !important; }
        `;
        
        function injectCSS(cssContent) {
            const style = document.createElement('style');
            style.textContent = cssContent;
            const head = document.head || document.documentElement;
            if (head) head.appendChild(style);
        }
        
        function initCSS() {
            // Always inject Core Styles
            injectCSS(coreAdStyles);
            
            // Only inject Aggressive Styles if NOT Friendly
            if (!isFriendly) {
                injectCSS(aggressiveAdStyles);
            } else {
                console.log("Lumina Stealth: Friendly domain (" + host + ") - Skipping aggressive CSS.");
            }
        }
        
        if (document.readyState === 'loading') {
            document.addEventListener('DOMContentLoaded', initCSS);
        } else {
            initCSS();
        }

        // 2. Global Ad-Intervention (Overlay Remover)
        function killAdOverlays() {
            // Safety: On Friendly domains, we ONLY unlock scroll and kill specific IFRAMES.
            // We do NOT kill divs/overlays to avoid breaking login/navigation.
            
            if (isFriendly) {
                // FORCE SCROLL UNLOCK - Fixes "Transfermarkt scroll lock"
                if (document.body) {
                    document.body.style.setProperty('overflow', 'auto', 'important');
                    document.body.style.setProperty('overflow-x', 'auto', 'important');
                    document.body.style.setProperty('overflow-y', 'auto', 'important');
                }
                if (document.documentElement) {
                    document.documentElement.style.setProperty('overflow', 'auto', 'important');
                    document.documentElement.style.setProperty('overflow-x', 'auto', 'important');
                    document.documentElement.style.setProperty('overflow-y', 'auto', 'important');
                }
                
                // Kill only ad iframes (which might be transparent overlays)
                document.querySelectorAll('iframe').forEach(el => {
                     const src = (el.src || '').toLowerCase();
                     const id = (el.id || '').toLowerCase();
                     if (src.includes('ads') || src.includes('doubleclick') || id.includes('google_ads') || src.includes('teads')) {
                         console.log("Lumina Friendly-Kill: Removing ad iframe ->", el);
                         el.remove();
                     }
                });
                return;
            }

            const keywords = ['modal', 'popup', 'overlay', 'interstitial', 'ads', 'promo', 'subscribe', 'sign-up'];
            // Select potential overlays
            const elements = document.querySelectorAll('div, section, aside, iframe');
            
            elements.forEach(el => {
                const id = (el.id || '').toLowerCase();
                const cls = (el.className || '').toString().toLowerCase();
                // Safety check for null style (e.g. detached elements)
                let style;
                try {
                     style = window.getComputedStyle(el);
                } catch(e) { return; }
                
                if (!style) return;
                
                // Check for fixed/absolute positioning + high z-index
                const isFloating = style.position === 'fixed' || style.position === 'absolute';
                const isHighZ = parseInt(style.zIndex) > 50;
                
                // Check for keywords
                const hasKeyword = keywords.some(k => id.includes(k) || cls.includes(k));
                
                if (isFloating && isHighZ && hasKeyword) {
                        console.log("Lumina Ad-Intervention: Killing overlay ->", el);
                        el.remove();
                        // Unlock scroll if blocked by overlay
                        if (document.body) document.body.style.overflow = 'auto';
                        if (document.documentElement) document.documentElement.style.overflow = 'auto';
                        
                        // Try to unlock parent scroll if in iframe
                        try {
                            if (window.parent && window.parent !== window) {
                                if (window.parent.document.body) window.parent.document.body.style.overflow = 'auto';
                                if (window.parent.document.documentElement) window.parent.document.documentElement.style.overflow = 'auto';
                            }
                        } catch(e) {}
                }
                
                // Also kill iframes that are likely ads but missed by CSS
                if (el.tagName === 'IFRAME' && (id.includes('google') || cls.includes('ads'))) {
                    el.remove();
                }
            });
        }

        // 3. Event Listeners (ESC & Periodic)
        document.addEventListener('keydown', (e) => {
            if (e.key === 'Escape') {
                console.log("Lumina: ESC pressed. Triggering Ad-Intervention & Stop.");
                killAdOverlays();
                window.stop(); // Stop infinite scripts
            }
        });

        // Run periodically to catch delayed popups
        setTimeout(killAdOverlays, 2000);
        setTimeout(killAdOverlays, 5000);
        setTimeout(killAdOverlays, 10000);
        // More frequent checks for scroll lock on Friendly domains
        if (isFriendly) {
             setInterval(() => {
                 // Check if body exists before accessing style
                 if (document.body) {
                     const s = window.getComputedStyle(document.body);
                     if (s.overflow === 'hidden' || s.overflowX === 'hidden' || s.overflowY === 'hidden') {
                        document.body.style.setProperty('overflow', 'auto', 'important');
                        document.body.style.setProperty('overflow-x', 'auto', 'important');
                        document.body.style.setProperty('overflow-y', 'auto', 'important');
                     }
                     if (s.position === 'fixed') {
                        document.body.style.setProperty('position', 'static', 'important');
                     }
                 }
                 // Check if documentElement exists before accessing style
                 if (document.documentElement) {
                     const s = window.getComputedStyle(document.documentElement);
                     if (s.overflow === 'hidden' || s.overflowX === 'hidden' || s.overflowY === 'hidden') {
                        document.documentElement.style.setProperty('overflow', 'auto', 'important');
                        document.documentElement.style.setProperty('overflow-x', 'auto', 'important');
                        document.documentElement.style.setProperty('overflow-y', 'auto', 'important');
                     }
                     if (s.position === 'fixed') {
                        document.documentElement.style.setProperty('position', 'static', 'important');
                     }
                 }
             }, 1000);
        }
        
    })();
    "#.to_string()
}

fn create_desktop_shortcut(_name: &str, _url: &str, _icon_path: Option<std::path::PathBuf>) -> std::io::Result<()> {
    #[cfg(target_os = "windows")]
    {
        // Sanitize filename
        let safe_name: String = _name.chars()
            .map(|c| if c.is_alphanumeric() || c == ' ' { c } else { '_' })
            .collect();
        
        let desktop = std::env::var("USERPROFILE").unwrap_or(".".to_string()) + "\\Desktop";
        let path = std::path::Path::new(&desktop).join(format!("{}.lnk", safe_name));
        let exe = std::env::current_exe()?;
        let exe_path = exe.to_str().unwrap();
        
        // Escape quotes for PowerShell
        let safe_url = _url.replace("'", "''");
        
        let mut script = format!(
            "$WshShell = New-Object -comObject WScript.Shell; $Shortcut = $WshShell.CreateShortcut('{}'); $Shortcut.TargetPath = '{}'; $Shortcut.Arguments = '--pwa-url=\"{}\"';",
            path.to_str().unwrap(),
            exe_path,
            safe_url
        );
        
        if let Some(icon) = _icon_path {
            if let Some(icon_str) = icon.to_str() {
                script.push_str(&format!(" $Shortcut.IconLocation = '{}';", icon_str));
            }
        }
        
        script.push_str(" $Shortcut.Save()");
        
        std::process::Command::new("powershell")
            .arg("-NoProfile")
            .arg("-Command")
            .arg(script)
            .output()?;
    }
    Ok(())
}


#[tauri::command]
fn update_tab_info(app: AppHandle, history_manager: tauri::State<'_, HistoryManager>, label: String, title: Option<String>, favicon: Option<String>, url: Option<String>) {
    // If URL and Title are present, update history title (but don't increment visit count)
    if let (Some(u), Some(t)) = (&url, &title) {
         if !u.starts_with("tauri://") && !u.starts_with("about:") {
             let _ = history_manager.update_title(u.clone(), t.clone());
         }
    }
    let _ = app.emit("tab-updated", TabUpdatedPayload { label, title, favicon });
}

struct NetworkSidecarRequest {
    command: String,
    payload: String,
    response_tx: tokio::sync::oneshot::Sender<String>,
}

struct NetworkState {
    tx: tokio::sync::mpsc::Sender<NetworkSidecarRequest>,
}

struct UiState {
    sidebar_open: std::sync::atomic::AtomicBool,
    suggestions_height: std::sync::atomic::AtomicU32,
    current_tab: std::sync::Mutex<Option<String>>,
}



#[tauri::command]
async fn create_tab(state: tauri::State<'_, UiState>, app: AppHandle, data_store: tauri::State<'_, AppDataStore>, label: String, url: String, _window: tauri::Window) -> Result<(), String> {
    // println!("Rust: create_tab called for {} url: {}", label, url);

    // Rewrite lumina:// to lumina-app://localhost/ for internal navigation to avoid OS deep link conflict
    let url = if url.starts_with("lumina://") {
        url.replace("lumina://", "lumina-app://localhost/")
    } else if url.starts_with("lumina-app://") {
         if !url.contains("lumina-app://localhost/") {
             url.replace("lumina-app://", "lumina-app://localhost/")
         } else {
             url
         }
    } else {
        url
    };

    // Ensure we are targeting the main window for the new tab
    let target_window = app.get_window("main").ok_or_else(|| {
        println!("Rust: Main window 'main' not found!");
        "Main window not found".to_string()
    })?;

    if app.get_webview(&label).is_some() {
        // If tab already exists, just switch to it (optional logic)
        println!("Rust: Tab {} already exists", label);
        return Ok(());
    }

    let window_size = target_window.inner_size().map_err(|e| e.to_string())?;
    let scale_factor = target_window.scale_factor().map_err(|e| e.to_string())?;
    let logical_size = window_size.to_logical::<f64>(scale_factor);
    
    let vertical_tabs = data_store.data.lock().unwrap().settings.vertical_tabs;
    let sidebar_open = state.sidebar_open.load(std::sync::atomic::Ordering::Relaxed);
    let suggestions_height = state.suggestions_height.load(std::sync::atomic::Ordering::Relaxed) as f64;
    
    let (main_height, x, y, tab_width, tab_height) = calculate_layout(logical_size, vertical_tabs, sidebar_open, suggestions_height);
    
    // Resize main webview (UI) to cover the top area
    if let Some(main_webview) = app.get_webview("main") {
        let _ = main_webview.set_size(tauri::LogicalSize::new(logical_size.width, main_height));
    }
    
    let app_handle = app.clone();
    let app_handle_dl = app.clone();


    let label_clone = label.clone();
    
    let ad_block_script = get_lumina_stealth_script();

    // Attempt to get invoke key
    println!("Rust: Getting invoke key for {}", label);
    let invoke_key = app.invoke_key();
     
    let info_script = format!(r#"
         (function() {{
             // Prevent execution in subframes (ads, tracking pixels) to stop IPC errors
             try {{
                if (window.self !== window.top) return;
             }} catch(e) {{ return; }}

             // Block execution on known ad domains (even if top-level)
             try {{
                 let host = window.location.hostname;
                 if (host.includes('doubleclick') || host.includes('googlesyndication') || host.includes('adnxs') || host.includes('teads')) return;
             }} catch(e) {{}}

             window.__TAB_LABEL__ = "{}";
             window.__TAURI_INVOKE_KEY__ = "{}";
            
            // Suppress Tauri callback errors caused by our manual IPC
            const originalConsoleError = console.error;
            const originalConsoleWarn = console.warn;
            
            function isTauriCallbackError(args) {{
                if (args.length > 0 && typeof args[0] === 'string') {{
                    return args[0].includes("Couldn't find callback id") || 
                           args[0].includes("[TAURI] Couldn't find callback id");
                }}
                return false;
            }}

            console.error = function(...args) {{
                if (isTauriCallbackError(args)) return;
                originalConsoleError.apply(console, args);
            }};
            
            console.warn = function(...args) {{
                if (isTauriCallbackError(args)) return;
                originalConsoleWarn.apply(console, args);
            }};

            // Custom IPC for our browser tabs via native postMessage
            // This bypasses CSP 'connect-src' and 'frame-src' restrictions.
            function invoke(cmd, args) {{
                // Try Tauri v2 standard invoke first (if available and not blocked)
                if (window.__TAURI__ && window.__TAURI__.core) {{
                    window.__TAURI__.core.invoke(cmd, args).catch(err => console.error("Tauri invoke failed:", err));
                    return;
                }}
                
                // Use a static counter to ensure unique, valid u32 IDs
                if (typeof window.__IPC_COUNTER === 'undefined') {{
                    window.__IPC_COUNTER = 0;
                }}
                window.__IPC_COUNTER = (window.__IPC_COUNTER + 1) % 4000000000;
                var callbackId = window.__IPC_COUNTER;
                
                // Register dummy callback to silence "Couldn't find callback id" errors
                var noOp = function(res) {{}};
                
                try {{
                     if (window.__TAURI_INTERNALS__) {{
                         if (!window.__TAURI_INTERNALS__.callbacks) {{
                             window.__TAURI_INTERNALS__.callbacks = {{}};
                         }}
                         // Tauri v2 style
                         window.__TAURI_INTERNALS__.callbacks[callbackId] = {{
                             resolve: noOp,
                             reject: noOp
                         }};
                     }}
                }} catch(e) {{}}
                
                // Cleanup
                setTimeout(function() {{
                    try {{
                        if (window.__TAURI_INTERNALS__ && window.__TAURI_INTERNALS__.callbacks) {{
                             delete window.__TAURI_INTERNALS__.callbacks[callbackId];
                        }}
                    }} catch(e) {{}}
                }}, 60000); // 60s timeout

                var msg = {{
                    cmd: cmd,
                    callback: callbackId, 
                    error: callbackId,
                    payload: args,
                    __TAURI_INVOKE_KEY__: window.__TAURI_INVOKE_KEY__
                }};
                
                if (window.chrome && window.chrome.webview) {{
                    // WebView2 (Windows)
                    window.chrome.webview.postMessage(msg); // Send object directly for WebView2
                }} else if (window.webkit && window.webkit.messageHandlers && window.webkit.messageHandlers.ipc) {{
                    // WebKit (macOS / Linux)
                    window.webkit.messageHandlers.ipc.postMessage(msg);
                }} else {{
                    console.error("No native IPC found for " + cmd);
                }}
            }}

            // PWA Detection
            window.addEventListener('beforeinstallprompt', (e) => {{
                // Prevent the mini-infobar from appearing on mobile
                e.preventDefault();
                // Stash the event so it can be triggered later.
                window.deferredPrompt = e;
                console.log("PWA beforeinstallprompt fired! Event stashed.");
                invoke('pwa_detected', {{ label: window.__TAB_LABEL__ }});
            }});
            
            window.addEventListener('appinstalled', () => {{
                console.log('PWA was installed');
                window.deferredPrompt = null;
            }});
            
            // Manual PWA Detection (Fallback)
            // Checks for manifest with display: standalone/minimal-ui
            async function checkManifest() {{
                if (window.deferredPrompt) return; // Already detected
                
                var link = document.querySelector("link[rel='manifest']");
                if (link && link.href) {{
                    // Try to fetch in browser first (shares session/cookies, bypasses Cloudflare)
                    try {{
                        console.log("Fetching PWA manifest via JS: " + link.href);
                        let response = await fetch(link.href);
                        if (response.ok) {{
                            let manifest = await response.json();
                            console.log("JS Manifest parsed:", manifest);
                            if (manifest.display && ['standalone', 'minimal-ui', 'fullscreen'].includes(manifest.display)) {{
                                console.log("PWA detected via JS fetch!");
                                
                                // Extract best icon
                                let iconUrl = null;
                                if (manifest.icons && Array.isArray(manifest.icons)) {{
                                    let maxArea = 0;
                                    for (let icon of manifest.icons) {{
                                        if (icon.src && icon.sizes) {{
                                            let sizes = icon.sizes.split(' ');
                                            for (let size of sizes) {{
                                                if (size === 'any') continue;
                                                let parts = size.split('x');
                                                if (parts.length === 2) {{
                                                    let w = parseInt(parts[0]);
                                                    let h = parseInt(parts[1]);
                                                    if (!isNaN(w) && !isNaN(h) && w * h > maxArea) {{
                                                        maxArea = w * h;
                                                        iconUrl = icon.src;
                                                    }}
                                                }}
                                            }}
                                        }} else if (icon.src && !iconUrl) {{
                                            // Fallback if no sizes
                                            iconUrl = icon.src;
                                        }}
                                    }}
                                    
                                    // Resolve relative URL
                                    if (iconUrl) {{
                                        iconUrl = new URL(iconUrl, link.href).href;
                                    }}
                                }}

                                invoke('pwa_detected', {{ label: window.__TAB_LABEL__, iconUrl: iconUrl }});
                                return;
                            }}
                        }}
                    }} catch(e) {{
                        console.error("Browser fetch failed, falling back to Rust:", e);
                    }}

                    // Fallback to Rust (bypasses CORS/CSP if browser fetch failed)
                    invoke('check_pwa_manifest', {{ label: window.__TAB_LABEL__, url: link.href }});
                }}
            }}
            
            // Run check on load and DOMContentLoaded, avoiding loops
            window.addEventListener('load', checkManifest);
            if (document.readyState === 'complete' || document.readyState === 'interactive') {{
                 checkManifest();
            }}

            function getFavicon() {{
                let link = document.querySelector("link[rel*='icon']") || document.querySelector("link[rel='shortcut icon']");
                return link ? link.href : "";
            }}

            function logVisit() {{
                if (window.location.protocol.startsWith('http')) {{
                     invoke('add_history_item', {{
                         url: window.location.href,
                         title: document.title || window.location.href
                     }});
                }}
            }}

            function updateInfo() {{
                 let title = document.title;
                 let favicon = getFavicon();
                 invoke('update_tab_info', {{
                     label: window.__TAB_LABEL__,
                     title: title,
                     favicon: favicon,
                     url: window.location.href
                 }});
            }}
            
            // Observer for head changes (title, favicon)
            function initObserver() {{
                var target = document.head || document.querySelector('head') || document.documentElement;
                if (target) {{
                    try {{
                        new MutationObserver(updateInfo).observe(target, {{ subtree: true, childList: true, attributes: true }});
                    }} catch(e) {{
                        console.error("MutationObserver init failed:", e);
                    }}
                }}
            }}

            // Handle new tab requests
            window.open = function(url, target, features) {{
                if (url) {{
                    window.__TAURI__.event.emit('request-new-tab', {{ label: 'new-tab', url: url }});
                }}
                return null;
            }};
            
            document.addEventListener('click', (e) => {{
                let target = e.target;
                while(target && target.tagName !== 'A') target = target.parentElement;
                if (target && target.tagName === 'A' && target.target === '_blank') {{
                    e.preventDefault();
                    window.__TAURI__.event.emit('request-new-tab', {{ label: 'new-tab', url: target.href }});
                }}
            }}, true);

            document.addEventListener('auxclick', (e) => {{
                if (e.button === 1) {{
                    let target = e.target;
                    while(target && target.tagName !== 'A') target = target.parentElement;
                    if (target && target.tagName === 'A') {{
                        e.preventDefault();
                        window.__TAURI__.event.emit('request-new-tab', {{ label: 'new-tab', url: target.href }});
                    }}
                }}
            }}, true);

            // Custom Context Menu
            document.addEventListener('contextmenu', (e) => {{
                // Check if target is link
                let target = e.target;
                let linkUrl = null;
                while(target && target.tagName !== 'A') target = target.parentElement;
                if (target && target.tagName === 'A' && target.href) {{
                    linkUrl = target.href;
                }}

                if (linkUrl) {{
                    e.preventDefault();
                    e.stopPropagation(); // Stop propagation immediately
                    
                    // Remove existing menu
                    const existing = document.getElementById('lumina-context-menu');
                    if (existing) existing.remove();

                    const menu = document.createElement('div');
                    menu.id = 'lumina-context-menu';
                    menu.style.position = 'fixed';
                    menu.style.top = e.clientY + 'px';
                    menu.style.left = e.clientX + 'px';
                    menu.style.zIndex = '2147483647'; // Max z-index
                    menu.style.background = '#292a2d';
                    menu.style.border = '1px solid #3c4043';
                    menu.style.borderRadius = '4px';
                    menu.style.padding = '4px 0';
                    menu.style.boxShadow = '0 2px 6px rgba(0,0,0,0.3)';
                    menu.style.minWidth = '150px';
                    menu.style.color = '#e8eaed';
                    menu.style.fontFamily = 'system-ui, -apple-system, sans-serif';
                    menu.style.fontSize = '13px';
                    menu.style.userSelect = 'none';

                    const createItem = (text, onClick) => {{
                        const item = document.createElement('div');
                        item.innerText = text;
                        item.style.padding = '6px 12px';
                        item.style.cursor = 'pointer';
                        item.onmouseenter = () => item.style.background = '#3c4043';
                        item.onmouseleave = () => item.style.background = 'transparent';
                        item.onclick = (ev) => {{
                            ev.stopPropagation(); 
                            onClick();
                            menu.remove();
                        }};
                        return item;
                    }};

                    menu.appendChild(createItem('Open Link in New Tab', () => {{
                         let uniqueLabel = 'tab-' + Date.now() + '-' + Math.floor(Math.random() * 1000000);
                         invoke('create_tab', {{ label: uniqueLabel, url: linkUrl }});
                    }}));
                    
                    // Separator
                    const sep = document.createElement('div');
                    sep.style.height = '1px';
                    sep.style.background = '#3c4043';
                    sep.style.margin = '4px 0';
                    menu.appendChild(sep);

                    menu.appendChild(createItem('Back', () => window.history.back()));
                    menu.appendChild(createItem('Forward', () => window.history.forward()));
                    menu.appendChild(createItem('Reload', () => window.location.reload()));

                    document.body.appendChild(menu);
                    
                    // Close on click outside
                    const closeMenu = () => {{
                        menu.remove();
                        document.removeEventListener('click', closeMenu);
                        document.removeEventListener('contextmenu', closeMenu);
                    }};
                    // Delay slightly to avoid immediate trigger
                    setTimeout(() => {{
                        document.addEventListener('click', closeMenu);
                        document.addEventListener('contextmenu', (e) => {{
                             if (e.target.closest('#lumina-context-menu')) return;
                             closeMenu();
                        }});
                    }}, 100);
                }}
            }}, true); // Use Capture phase to preempt site scripts

            if (document.body || document.head || document.documentElement) {{
                initObserver();
            }} else {{
                document.addEventListener('DOMContentLoaded', initObserver);
            }}
            
            // Initial call
            if (document.readyState === 'complete' || document.readyState === 'interactive') {{
                updateInfo();
                logVisit();
            }} else {{
                window.addEventListener('DOMContentLoaded', updateInfo);
                window.addEventListener('load', () => {{ updateInfo(); logVisit(); }});
            }}
        }})();
    "#, label_clone, invoke_key);

    let full_script = format!("{}\n{}", ad_block_script, info_script);

    let url_parsed = match url.parse() {
        Ok(u) => u,
        Err(e) => return Err(format!("Invalid URL: {}", e)),
    };

    let app_clone_adblock = app.clone();
    let label_clone_adblock = label.clone();

    // println!("Rust: Creating WebviewBuilder for {}", label);
    let mut builder = tauri::webview::WebviewBuilder::new(&label, WebviewUrl::External(url_parsed));
    
    #[cfg(target_os = "windows")]
    {
         // Chrome Extensions Support (Windows)
         let mut args = Vec::new();
         args.push("--ignore-certificate-errors".to_string());
         
         // Load unpacked extensions if available
         if let Some(ext_path) = get_extension_path(&app_handle_dl) {
             if let Ok(entries) = std::fs::read_dir(&ext_path) {
                 let paths: Vec<String> = entries
                     .filter_map(|e| e.ok())
                     .filter(|e| e.path().is_dir())
                     .map(|e| e.path().to_string_lossy().into_owned())
                     .collect();
                 
                 if !paths.is_empty() {
                     let ext_arg = format!("--load-extension={}", paths.join(","));
                     println!("Lumina Extensions: Loading {}", ext_arg);
                     args.push(ext_arg);
                 }
             }
         }
         
         for arg in args {
            builder = builder.additional_browser_args(&arg);
         }

         builder = builder.user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36 Edg/120.0.0.0");
    }
    #[cfg(target_os = "linux")]
    {
        builder = builder.user_agent("Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36");
    }
    #[cfg(target_os = "macos")]
    {
        builder = builder.user_agent("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36");
    }

    builder = builder.initialization_script(&full_script)
        .on_web_resource_request(move |request, response| {
             // Lumina Stealth: Rust-side Ad/Tracker Blocking
             let referer = request.headers().get("referer").and_then(|h| h.to_str().ok());
             if check_adblock_url(&request.uri().to_string(), referer, &label_clone_adblock, &app_clone_adblock) {
                   *response = tauri::http::Response::builder()
                    .status(403)
                    .body(std::borrow::Cow::Owned(Vec::new()))
                    .unwrap();
            }
        })
        .on_download(move |_webview, event| {
            match event {
                tauri::webview::DownloadEvent::Requested { url, destination: _ } => {
                    println!("Download requested: {}", url);
                    let url_str = url.to_string();
                    let mut file_name = url.as_str().split('/').next_back().unwrap_or("file").to_string();
                    if file_name.is_empty() {
                        file_name = "downloaded_file".to_string();
                    }
                    let app = app_handle_dl.clone();
                    
                    tauri::async_runtime::spawn(async move {
                         download_file(app, url_str, file_name).await;
                    });
                    false // Suppress native download
                }
                _ => true
            }
        })

        .on_navigation(move |url: &Url| {
            // println!("Navigation: {} -> {}", label_clone, url);
            
            // Explicitly allow lumina-app scheme to bypass some restrictions
            if url.scheme() == "lumina-app" {
                 println!("Navigation ALLOWED (internal): {}", url);
                 return true;
            }

            let _ = app_handle.emit("tab-navigation", TabNavigationPayload {
                label: label_clone.clone(),
                url: url.to_string(),
            });
            
            true
        });

    // Use add_child to create the webview inside the existing window
    // println!("Rust: Attempting to add_child for {}", label);
    // Explicitly handle the Result immediately to catch panic or error
     let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
         // println!("Rust: Inside catch_unwind, calling add_child");
         target_window.add_child(
             builder,
             tauri::LogicalPosition::new(x, y),
             tauri::LogicalSize::new(tab_width, tab_height),
         )
     }));

    match res {
        Ok(inner_res) => {
             match inner_res {
                Ok(webview) => {
                    // println!("Rust: Tab {} created successfully. Actual label: {}", label, webview.label());
                    // Force tracking of the new webview
                    // In Tauri v2, add_child might not immediately register the webview in the app handle's map
                    // But the webview object returned is valid.
                    
                    // Position and size are already set by add_child

                    // New tab is created. 
                    
                    // Optimization: Hide previous tab immediately to prevent stacking/flicker
                    {
                        let mut current = state.current_tab.lock().unwrap();
                        if let Some(ref old_label) = *current {
                             if let Some(old_webview) = app.get_webview(old_label) {
                                 let _ = old_webview.hide();
                             }
                        }
                        *current = Some(label.clone());
                    }

                    let _ = webview.show();
                    let _ = webview.set_focus();

                    #[cfg(target_os = "windows")]
                    {
                        use windows::Win32::Foundation::HWND;
                        use windows::Win32::UI::WindowsAndMessaging::{SetWindowPos, HWND_TOP, SWP_NOMOVE, SWP_NOSIZE, SWP_SHOWWINDOW};
                        
                        // Force Z-Order to Top
                        if let Ok(hwnd_isize) = webview.window().hwnd() {
                             // Tauri v2 returns HWND as isize on Windows
                             let hwnd = HWND(hwnd_isize.0 as isize);
                             unsafe {
                                 let _ = SetWindowPos(hwnd, HWND_TOP, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW);
                             }
                        }
                    }
                    
                    // Explicitly ensure navigation (fix for WebView2 Source being null)
                    // This forces the webview to navigate even if the builder initialization missed it
                    // Using eval since load_url is not available on Webview struct in this context
                    let json_url = serde_json::to_string(&url).unwrap_or_else(|_| format!("'{}'", url));
                    let nav_script = format!("window.location.replace({})", json_url);
                    if let Err(e) = webview.eval(&nav_script) {
                        println!("Rust: Failed to force load url via eval: {}", e);
                    }

                    // Force resize to ensure visibility (Fix for black screen / 0x0 size)
                    let _ = webview.set_size(tauri::LogicalSize::new(tab_width, tab_height));
                    // Force position to ensure it doesn't overlap with the top bar (Fix for production layout issue)
                    let _ = webview.set_position(tauri::LogicalPosition::new(x, y));

                    let _ = app.emit::<TabCreatedPayload>("tab-created", TabCreatedPayload {
                        label: label.clone(),
                        url: url.clone(),
                    });

                    // Initial check
                    let wv = webview.clone();
                    let u = url.clone();
                    tauri::async_runtime::spawn(async move {
                        check_and_redirect(wv, u).await;
                    });
                },
                Err(e) => {
                    println!("Rust: Error creating tab {}: {:?}", label, e);
                    return Err(format!("Failed to create tab: {:?}", e));
                }
            }
        },
        Err(payload) => {
             println!("Rust: add_child PANICKED for {}: {:?}", label, payload);
             return Err("add_child panicked".to_string());
        }
    }
    
    Ok(())
}

#[tauri::command]
fn switch_tab(app: AppHandle, state: tauri::State<'_, UiState>, label: String) {
    println!("Switching to tab: {}", label);
    
    let mut current = state.current_tab.lock().unwrap();
    
    // Optimization: Only hide the previously active tab instead of iterating all webviews
    if let Some(ref old_label) = *current {
        if old_label != &label {
            if let Some(old_webview) = app.get_webview(old_label) {
                let _ = old_webview.hide();
            }
        }
    } else {
        // Fallback: If no current tab tracked yet (first switch), hide all others
        let webviews = app.webviews();
        for webview in webviews {
            let webview_instance = &webview.1; 
            if webview_instance.label() != "main" && webview_instance.label() != label {
                let _ = webview_instance.hide();
            }
        }
    }
    
    // Show the new tab
    if let Some(webview) = app.get_webview(&label) {
        let _ = webview.show();
        let _ = webview.set_focus();

        #[cfg(target_os = "windows")]
        {
            use windows::Win32::Foundation::HWND;
            use windows::Win32::UI::WindowsAndMessaging::{SetWindowPos, HWND_TOP, SWP_NOMOVE, SWP_NOSIZE, SWP_SHOWWINDOW};
            
            // Force Z-Order to Top
             if let Ok(hwnd_isize) = webview.window().hwnd() {
                     let hwnd = HWND(hwnd_isize.0 as isize);
                     unsafe {
                         let _ = SetWindowPos(hwnd, HWND_TOP, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW);
                     }
             }
        }
    }
    
    // Update state
    *current = Some(label);
}

#[tauri::command]
fn close_tab(app: AppHandle, label: String) {
    if let Some(webview) = app.get_webview(&label) {
        let _ = webview.close();
        let _ = app.emit("tab-closed", TabClosedPayload { label });
    }
}

#[tauri::command]
async fn init_browser(app: AppHandle, window: tauri::Window) {
    // This function is kept for backward compatibility or initial setup
    // But mostly we will use create_tab now.
    // Let's just resize the main webview here to be safe.
    
    let window_size = window.inner_size().unwrap();
    let scale_factor = window.scale_factor().unwrap();
    let logical_size = window_size.to_logical::<f64>(scale_factor);
    let top_ui_height = 104.0;
    
    if let Some(main_webview) = app.get_webview("main") {
        let _ = main_webview.set_size(tauri::LogicalSize::new(logical_size.width, top_ui_height));
    }
}

async fn download_file(app: AppHandle, url: String, file_name: String) {
    let download_dir = app.path().download_dir().unwrap_or(std::path::PathBuf::from("downloads"));
    if !download_dir.exists() {
        let _ = tokio::fs::create_dir_all(&download_dir).await;
    }
    let path = download_dir.join(&file_name);
    let path_str = path.to_string_lossy().to_string();

    // Use DownloadManager
    let manager = app.state::<DownloadManager>();
    
    // Check existing file size
    let mut downloaded: u64 = 0;
    if path.exists() {
        if let Ok(metadata) = tokio::fs::metadata(&path).await {
             downloaded = metadata.len();
        }
    }

    // Register
    {
        let mut data = manager.downloads.lock().unwrap();
        data.insert(url.clone(), DownloadItem {
            url: url.clone(),
            file_name: file_name.clone(),
            total_size: 0,
            downloaded_size: downloaded,
            path: path_str.clone(),
            status: "downloading".to_string(),
            added_at: chrono::Utc::now().timestamp(),
        });
    }
    manager.save();

    let _ = app.emit("download-started", DownloadStartedPayload {
        url: url.clone(),
        file_name: file_name.clone(),
    });

    let client = reqwest::Client::new();
    let mut request = client.get(&url);
    
    if downloaded > 0 {
        request = request.header("Range", format!("bytes={}-", downloaded));
    }

    match request.send().await {
        Ok(res) => {
            let status = res.status();
            let total_size = res.content_length().unwrap_or(0) + downloaded;
            
            let mut file;
            if status == reqwest::StatusCode::PARTIAL_CONTENT {
                 match tokio::fs::OpenOptions::new().create(true).append(true).open(&path).await {
                    Ok(mut f) => {
                        // Use AsyncSeekExt (restored)
                        let _ = f.seek(std::io::SeekFrom::End(0)).await;
                        file = f;
                    }
                    Err(e) => {
                         println!("Failed to open file for append: {}", e);
                         manager.update_status(&url, "failed");
                         let _ = app.emit("download-finished", DownloadFinishedPayload {
                            url: url.clone(),
                            success: false,
                            path: None,
                        });
                        return;
                    }
                 }
            } else {
                downloaded = 0;
                match tokio::fs::File::create(&path).await {
                    Ok(f) => file = f,
                    Err(e) => {
                         println!("Failed to create file: {}", e);
                         manager.update_status(&url, "failed");
                         let _ = app.emit("download-finished", DownloadFinishedPayload {
                            url: url.clone(),
                            success: false,
                            path: None,
                        });
                        return;
                    }
                }
            }

            let mut stream = res.bytes_stream();
            let mut last_save = std::time::Instant::now();

            while let Some(item) = stream.next().await {
                match item {
                    Ok(chunk) => {
                        if (file.write_all(&chunk).await).is_err() {
                             manager.update_status(&url, "failed");
                             return;
                        }
                        downloaded += chunk.len() as u64;
                        manager.update_progress(&url, downloaded, total_size);
                        
                        if last_save.elapsed().as_secs() > 5 {
                            manager.save();
                            last_save = std::time::Instant::now();
                        }

                        let _ = app.emit("download-progress", DownloadProgressPayload {
                            url: url.clone(),
                            progress: downloaded,
                            total: total_size,
                        });
                    }
                    Err(_) => {
                         manager.update_status(&url, "failed");
                         return;
                    }
                }
            }
            
            // Ensure file is written and closed
            let _ = file.sync_all().await;
            drop(file);

            manager.update_status(&url, "completed");
            manager.save();

            let _ = app.emit("download-finished", DownloadFinishedPayload {
                url: url.clone(),
                success: true,
                path: Some(path_str),
            });
        }
        Err(_) => {
            manager.update_status(&url, "failed");
             let _ = app.emit("download-finished", DownloadFinishedPayload {
                url: url.clone(),
                success: false,
                path: None,
            });
        }
    }
}

#[tauri::command]
fn get_downloads(app: AppHandle) -> Vec<DownloadItem> {
    let manager = app.state::<DownloadManager>();
    let data = manager.downloads.lock().unwrap();
    data.values().cloned().collect()
}

#[tauri::command]
async fn resume_download(app: AppHandle, url: String) -> Result<(), String> {
    let manager = app.state::<DownloadManager>();
    let item = {
        let data = manager.downloads.lock().unwrap();
        data.get(&url).cloned()
    };
    
    if let Some(item) = item {
        download_file(app, item.url, item.file_name).await;
        Ok(())
    } else {
        Err("Download not found".to_string())
    }
}

#[tauri::command]
async fn check_pwa_manifest(app: AppHandle, state: tauri::State<'_, PwaState>, label: String, url: String) -> Result<(), String> {
    println!("Checking PWA manifest for {}: {}", label, url);
    let client = reqwest::Client::new();
    match client.get(&url)
        .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/144.0.0.0 Safari/537.36 Edg/144.0.0.0")
        .send()
        .await 
    {
        Ok(res) => {
            let status = res.status();
            println!("Manifest fetch status: {}", status);
            
            let text = res.text().await.unwrap_or_default();
            // println!("Manifest raw content: {}", text); // Uncomment for full debug if needed

            if let Ok(manifest) = serde_json::from_str::<serde_json::Value>(&text) {
                 println!("PWA Manifest fetched for {}: {:?}", label, manifest);
                 if let Some(display) = manifest.get("display").and_then(|v: &serde_json::Value| v.as_str()) {
                     println!("PWA Manifest display mode: {}", display);
                     if display == "standalone" || display == "minimal-ui" || display == "fullscreen" {
                         println!("PWA Manifest confirmed via Rust for {}", label);
                         
                         // Find best icon
                         let mut best_icon_url = None;
                         let mut max_area = 0;
                         if let Some(icons) = manifest.get("icons").and_then(|v| v.as_array()) {
                             for icon in icons {
                                 if let Some(src) = icon.get("src").and_then(|v| v.as_str()) {
                                     if let Some(sizes) = icon.get("sizes").and_then(|v| v.as_str()) {
                                         for size in sizes.split_whitespace() {
                                             if size == "any" { continue; }
                                             if let Some((w, h)) = size.split_once('x') {
                                                 if let (Ok(w), Ok(h)) = (w.parse::<i32>(), h.parse::<i32>()) {
                                                     if w * h > max_area {
                                                         max_area = w * h;
                                                         best_icon_url = Some(src.to_string());
                                                     }
                                                 }
                                             }
                                         }
                                     }
                                     // Fallback if no sizes and we haven't found a better one yet
                                     if best_icon_url.is_none() {
                                         best_icon_url = Some(src.to_string());
                                     }
                                 }
                             }
                         }
                         
                         // Resolve relative URL
                         let final_icon_url = if let Some(u) = best_icon_url {
                             if let Ok(base) = url::Url::parse(&url) {
                                 if let Ok(joined) = base.join(&u) {
                                     Some(joined.to_string())
                                 } else {
                                     Some(u)
                                 }
                             } else {
                                 Some(u)
                             }
                         } else {
                             None
                         };

                         if let Some(u) = &final_icon_url {
                              state.icons.lock().unwrap().insert(label.clone(), u.clone());
                         }

                         let _ = app.emit("pwa-can-install", TabPwaPayload { label, icon_url: final_icon_url });
                     }
                 } else {
                     println!("PWA Manifest missing 'display' field or invalid.");
                 }
            } else {
                println!("Failed to parse PWA manifest JSON. Raw content start: {:.200}", text);
            }
        }
        Err(e) => println!("Failed to fetch manifest: {}", e),
    }
    Ok(())
}

#[tauri::command]
async fn run_kip_code(app: tauri::AppHandle, code: String) -> Result<String, String> {
    use tauri_plugin_shell::ShellExt;
    use tauri_plugin_shell::process::CommandEvent;

    let sidecar = app.shell().sidecar("kip-lang")
        .map_err(|e| e.to_string())?;

    let (mut rx, mut child) = sidecar
        .spawn()
        .map_err(|e| e.to_string())?;

    // Send code + exit command to ensure the sidecar processes and terminates
    let input = format!("{}\nexit\n", code);
    child.write(input.as_bytes()).map_err(|e| e.to_string())?;

    let mut output = String::new();
    while let Some(event) = rx.recv().await {
        match event {
            CommandEvent::Stdout(line) => {
                let text = String::from_utf8_lossy(&line);
                output.push_str(&text);
            }
            CommandEvent::Stderr(line) => {
                let text = String::from_utf8_lossy(&line);
                println!("Kip Stderr: {}", text);
            }
            CommandEvent::Terminated(_) => {
                break;
            }
            _ => {}
        }
    }
    
    Ok(output)
}

#[tauri::command]
async fn run_networking_command(state: tauri::State<'_, NetworkState>, command: String, payload: String) -> Result<String, String> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    state.tx.send(NetworkSidecarRequest {
        command,
        payload,
        response_tx: tx
    }).await.map_err(|e| e.to_string())?;

    rx.await.map_err(|e| e.to_string())
}

#[tauri::command]
fn run_sidekick(app: tauri::AppHandle) -> Result<String, String> {
    use tauri_plugin_shell::ShellExt;
    
    let sidecar = app.shell().sidecar("lumina-sidekick")
        .map_err(|e| e.to_string())?;

    let (mut _rx, _child) = sidecar
        .spawn()
        .map_err(|e| e.to_string())?;

    Ok("Sidekick started".to_string())
}

#[tauri::command]
fn run_lua_code(app: AppHandle, code: String) -> Result<String, String> {
    let state = app.state::<LuaState>();
    let result = {
        if let Ok(lua) = state.lua.lock() {
            lua.load(&code).eval::<String>().map_err(|e| e.to_string())
        } else {
            Err("Failed to lock Lua state".to_string())
        }
    };
    result
}

// 2. Chrome Extension Support (Windows Only)
// Allows loading unpacked extensions from a specific directory
#[cfg(target_os = "windows")]
fn get_extension_path(app: &AppHandle) -> Option<PathBuf> {
    if let Ok(app_data) = app.path().app_data_dir() {
        let extensions_dir = app_data.join("extensions");
        if !extensions_dir.exists() {
            let _ = std::fs::create_dir_all(&extensions_dir);
        }
        Some(extensions_dir)
    } else {
        None
    }
}

// === New Browser Feature Commands ===

#[tauri::command]
fn set_zoom_level(history_manager: tauri::State<'_, HistoryManager>, domain: String, zoom: i32) -> Result<(), String> {
    history_manager.set_zoom_level(&domain, zoom).map_err(|e| e.to_string())
}

#[tauri::command]
fn get_zoom_level(history_manager: tauri::State<'_, HistoryManager>, domain: String) -> Result<i32, String> {
    history_manager.get_zoom_level(&domain).map_err(|e| e.to_string())
}

#[tauri::command]
fn set_cookie(history_manager: tauri::State<'_, HistoryManager>, domain: String, name: String, value: String, expires: Option<i64>, path: Option<String>, secure: bool, http_only: bool) -> Result<(), String> {
    let p = path.unwrap_or_else(|| "/".to_string());
    let cookie = history_manager::CookieItem {
        domain,
        name,
        value,
        expires,
        path: p,
        secure,
        http_only,
    };
    history_manager.set_cookie(cookie).map_err(|e| e.to_string())
}

#[tauri::command]
fn get_cookies(history_manager: tauri::State<'_, HistoryManager>, domain: String) -> Result<Vec<history_manager::CookieItem>, String> {
    history_manager.get_cookies(&domain).map_err(|e| e.to_string())
}

#[tauri::command]
fn delete_cookie(history_manager: tauri::State<'_, HistoryManager>, domain: String, name: String) -> Result<(), String> {
    history_manager.delete_cookie(&domain, &name).map_err(|e| e.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    #[cfg(target_os = "linux")]
    std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");

    let builder = tauri::Builder::default();

    #[cfg(target_os = "windows")]
    {
        // Enable WebView2 features for extensions
        // Note: This requires the correct WebView2 runtime and potentially additional configuration
        // Tauri v2 exposes some of this via WebviewWindowBuilder, but global settings are limited.
        // We will try to set the user data folder which is often required for extensions.
    }

    builder
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_deep_link::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new().with_handler(|app, shortcut, event| {
                if event.state() == ShortcutState::Pressed && shortcut.matches(Modifiers::CONTROL, Code::Space) {
                    if let Some(window) = app.get_webview_window("main") {
                        if window.is_visible().unwrap_or(false) {
                            // If window is visible, we toggle the command palette UI instead of hiding the window
                            let _ = window.emit("toggle-command-palette", ());
                            let _ = window.set_focus();
                        } else {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                }
            }).build()
        )
        .register_uri_scheme_protocol("lumina-app", move |ctx, request| {
            let uri = request.uri().to_string();
            println!("Lumina-App Protocol Handler: {}", uri); // DEBUG LOG

            // Robust parsing: handle lumina-app://path or lumina-app:path or lumina-app://localhost/path
            // 1. Strip scheme
            let without_scheme = uri.strip_prefix("lumina-app:").unwrap_or(&uri);
            // 2. Strip leading slashes (//)
            let without_slashes = without_scheme.trim_start_matches('/');
            // 3. Strip 'localhost' if present
            let path_and_query = without_slashes.strip_prefix("localhost").unwrap_or(without_slashes);
            // 4. Clean up path
            let full_path = path_and_query.trim_start_matches('/');
            
            // Split path and query/hash
            let (path, query) = if let Some(idx) = full_path.find('?') {
                (&full_path[..idx], &full_path[idx..])
            } else if let Some(idx) = full_path.find('#') {
                 (&full_path[..idx], &full_path[idx..])
            } else {
                (full_path, "")
            };
            
            let path = path.trim_end_matches('/');

            // Store Installation Handler
            if path == "install" {
                 let id = if let Some(idx) = query.find("id=") {
                     let rest = &query[idx + 3..];
                     rest.split('&').next().unwrap_or(rest)
                 } else {
                     "unknown"
                 };
                 
                 println!("Lumina Store: Installing {}", id);
                 
                 let success = perform_install(ctx.app_handle(), id);
                 
                 let (title, message, color) = if success {
                     ("Installation Complete", format!("Package <strong>{}</strong> has been successfully installed.", id), "#10b981")
                 } else {
                     ("Installation Failed", format!("Failed to install package <strong>{}</strong>.", id), "#ef4444")
                 };

                 let success_html = format!(r#"
                    <!DOCTYPE html>
                    <html>
                    <head>
                        <title>{}</title>
                        <meta charset="UTF-8">
                        <style>
                            body {{ font-family: 'Segoe UI', system-ui, sans-serif; background: #0f172a; color: #e2e8f0; margin: 0; display: flex; align-items: center; justify-content: center; height: 100vh; }}
                            .card {{ background: #1e293b; padding: 40px; border-radius: 16px; text-align: center; border: 1px solid #334155; box-shadow: 0 10px 25px -5px rgba(0, 0, 0, 0.5); animation: popIn 0.3s cubic-bezier(0.175, 0.885, 0.32, 1.275); }}
                            @keyframes popIn {{ from {{ transform: scale(0.8); opacity: 0; }} to {{ transform: scale(1); opacity: 1; }} }}
                            h1 {{ color: {}; margin: 0 0 16px 0; font-size: 2rem; }}
                            p {{ color: #94a3b8; margin-bottom: 24px; }}
                            .btn {{ background: #3b82f6; color: white; text-decoration: none; padding: 10px 24px; border-radius: 8px; font-weight: 600; transition: background 0.2s; display: inline-block; }}
                            .btn:hover {{ background: #2563eb; }}
                        </style>
                    </head>
                    <body>
                        <div class="card">
                            <div style="font-size: 4rem; margin-bottom: 10px;">{}</div>
                            <h1>{}</h1>
                            <p>{}</p>
                            <a href="lumina-app://store" class="btn">Return to Store</a>
                        </div>
                    </body>
                    </html>
                 "#, title, color, if success { "🎉" } else { "⚠️" }, title, message);
                 
                 // Emit Toast for feedback in main window too
                 let _ = ctx.app_handle().emit("toast", ToastPayload {
                     message: if success { format!("Sidekick modülü kuruldu: {}", id) } else { format!("Kurulum hatası: {}", id) },
                     level: if success { "success".to_string() } else { "error".to_string() },
                 });

                 return tauri::http::Response::builder()
                    .status(200)
                    .header("Content-Type", "text/html; charset=utf-8")
                    .header("Access-Control-Allow-Origin", "*")
                    .body(success_html.into_bytes())
                    .unwrap();
            }

            println!("Lumina-App Path: {}", path); // DEBUG LOG

            if let Some(html) = get_internal_page_html(ctx.app_handle(), path) {
                tauri::http::Response::builder()
                    .status(200)
                    .header("Content-Type", "text/html; charset=utf-8")
                    .header("Access-Control-Allow-Origin", "*")
                    .body(html.into_bytes())
                    .unwrap()
            } else {
                println!("Lumina-App: Unknown path {}", path);
                tauri::http::Response::builder()
                    .status(404)
                    .header("Content-Type", "text/html; charset=utf-8")
                    .header("Access-Control-Allow-Origin", "*")
                    .body(format!("<h1>404 Not Found</h1><p>Path: {}</p>", path).into_bytes())
                    .unwrap()
            }
        })
        .manage(UiState { 
            sidebar_open: std::sync::atomic::AtomicBool::new(false),
            suggestions_height: std::sync::atomic::AtomicU32::new(0),
            current_tab: std::sync::Mutex::new(None),
        })
        .manage(PwaState { icons: std::sync::Mutex::new(std::collections::HashMap::new()) })
        .setup(|app| {
            // Initialize Lua (Real Runtime)
            app.manage(LuaState { lua: Mutex::new(create_lua_runtime()) });

            // Load scripts/init.lua if exists
            let lua_state = app.state::<LuaState>();
            if let Ok(lua) = lua_state.lua.lock() {
                // Try to find init.lua in current dir (dev) or app data dir (prod)
                let paths = vec![
                    std::path::PathBuf::from("scripts/init.lua"),
                    app.path().app_data_dir().unwrap_or_default().join("scripts/init.lua"),
                ];

                for path in paths {
                    if path.exists() {
                        if let Ok(script) = std::fs::read_to_string(&path) {
                            println!("Executing Lua script: {:?}", path);
                            if let Err(e) = lua.load(&script).exec() {
                                eprintln!("Error executing Lua script {:?}: {}", path, e);
                            } else {
                                break; // Loaded successfully, stop looking
                            }
                        }
                    }
                }
            }

            // Initialize Sidekick Communication Channel
            let (sidekick_tx, mut sidekick_rx) = tokio::sync::mpsc::channel::<String>(32);
            app.manage(SidekickState { tx: sidekick_tx });

            // Start Lumina Sidekick (GUI) with output capture for "The Bridge"
            let sidekick_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                use tauri_plugin_shell::ShellExt;
                use tauri_plugin_shell::process::CommandEvent;
                
                loop {
                    println!("Starting Lumina Sidekick...");
                    let sidecar = match sidekick_handle.shell().sidecar("lumina-sidekick") {
                        Ok(s) => s,
                        Err(e) => {
                            eprintln!("Failed to create Sidekick sidecar command: {}", e);
                            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                            continue;
                        }
                    };

                    let (mut rx, mut child) = match sidecar.spawn() {
                        Ok(v) => v,
                        Err(e) => {
                            eprintln!("Failed to spawn Sidekick process: {}", e);
                            if e.to_string().contains("740") {
                                eprintln!("CRITICAL: Sidekick requires elevation (Admin rights). Stopping retry loop.");
                                break;
                            }
                            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                            continue;
                        }
                    };

                    println!("Lumina Sidekick started successfully.");

                    loop {
                        tokio::select! {
                            msg_opt = sidekick_rx.recv() => {
                                match msg_opt {
                                    Some(msg) => {
                                        let input = format!("{}\n", msg);
                                        if let Err(e) = child.write(input.as_bytes()) {
                                            eprintln!("Failed to write to Sidekick: {}", e);
                                            break; 
                                        }
                                    }
                                    None => {
                                        println!("Sidekick input channel closed. Stopping Sidekick.");
                                        return; 
                                    }
                                }
                            }
                            event_opt = rx.recv() => {
                                match event_opt {
                                    Some(event) => {
                                        match event {
                                            CommandEvent::Stdout(line_bytes) => {
                                                let line = String::from_utf8_lossy(&line_bytes);
                                                if line.starts_with("LUA:") {
                                                    let script = line.trim_start_matches("LUA:").trim().to_string();
                                                    println!("Bridge: Executing Lua from Sidekick: {}", script);
                                                    
                                                    if let Some(state) = sidekick_handle.try_state::<LuaState>() {
                                                        if let Ok(lua) = state.lua.lock() {
                                                            match lua.load(&script).eval::<String>() {
                                                                Ok(res) => {
                                                                    let _ = sidekick_handle.emit("lua-bridge-message", res);
                                                                }
                                                                Err(e) => {
                                                                    eprintln!("Lua Bridge Error: {}", e);
                                                                }
                                                            }
                                                        }
                                                    }
                                                } else if line.starts_with("OMNIBOX_RESULTS:") {
                                                    let json_str = line.trim_start_matches("OMNIBOX_RESULTS:").trim();
                                                    let _ = sidekick_handle.emit("omnibox-results", json_str);
                                                }
                                            }
                                            CommandEvent::Stderr(line_bytes) => {
                                                let line = String::from_utf8_lossy(&line_bytes);
                                                eprintln!("Sidekick Stderr: {}", line);
                                            }
                                            CommandEvent::Terminated(t) => {
                                                println!("Sidekick terminated: {:?}", t);
                                                break; 
                                            }
                                            _ => {}
                                        }
                                    }
                                    None => break, 
                                }
                            }
                        }
                    }
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                }
            });

            // Initialize Network Sidecar
            let (tx, mut rx) = tokio::sync::mpsc::channel::<NetworkSidecarRequest>(32);
            app.manage(NetworkState { tx });
            
            let app_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                use tauri_plugin_shell::ShellExt;
                use tauri_plugin_shell::process::CommandEvent;

                // Start sidecar loop
                loop {
                    println!("Starting Lumina-Net Sidecar...");
                    let sidecar = match app_handle.shell().sidecar("lumina-net") {
                        Ok(s) => s,
                        Err(e) => {
                            eprintln!("Failed to create sidecar command: {}", e);
                            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                            continue;
                        }
                    };

                    let (mut sidecar_rx, mut sidecar_child) = match sidecar.spawn() {
                        Ok(v) => v,
                        Err(e) => {
                            eprintln!("Failed to spawn sidecar: {}", e);
                            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                            continue;
                        }
                    };

                    let mut current_response_tx: Option<tokio::sync::oneshot::Sender<String>> = None;

                    loop {
                        tokio::select! {
                            req_opt = rx.recv() => {
                                match req_opt {
                                    Some(req) => {
                                         current_response_tx = Some(req.response_tx);
                                         let request_json = serde_json::json!({
                                            "command": req.command,
                                            "payload": serde_json::from_str::<serde_json::Value>(&req.payload).unwrap_or(serde_json::Value::Null)
                                         });
                                         let input = format!("{}\n", request_json);
                                         if let Err(e) = sidecar_child.write(input.as_bytes()) {
                                             eprintln!("Failed to write to sidecar: {}", e);
                                             break; 
                                         }
                                    }
                                    None => return, 
                                }
                            }
                            event_opt = sidecar_rx.recv() => {
                                match event_opt {
                                    Some(event) => {
                                        match event {
                                            CommandEvent::Stdout(line) => {
                                                let text = String::from_utf8_lossy(&line).to_string();
                                                if let Some(tx) = current_response_tx.take() {
                                                    let _ = tx.send(text);
                                                }
                                            }
                                            CommandEvent::Stderr(line) => {
                                                eprintln!("Lumina-Net Stderr: {}", String::from_utf8_lossy(&line));
                                            }
                                            CommandEvent::Terminated(t) => {
                                                println!("Lumina-Net terminated: {:?}", t);
                                                break; 
                                            }
                                            _ => {}
                                        }
                                    }
                                    None => break, 
                                }
                            }
                        }
                    }
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                }
            });

            // Initialize Rust Native Security Layer
            security::init();

            // Deep Link Registration
            #[cfg(any(windows, target_os = "linux"))]
            {
                use tauri_plugin_deep_link::DeepLinkExt;
                let _ = app.deep_link().register_all();
            }

            // Register Global Shortcut
            #[cfg(desktop)]
            {
                let _ = app.handle().global_shortcut().unregister_all();
                if let Err(e) = app.handle().global_shortcut().register("Ctrl+Space") {
                    println!("Warning: Failed to register global shortcut 'Ctrl+Space': {}", e);
                }
            }

            // Initialize Adblock Engine
            tauri::async_runtime::spawn(async move {
                println!("Initializing Adblock Engine...");
                let mut filter_set = FilterSet::new(true);
                
                // Fallback/Basic Rules
                let basic_rules = vec![
                    "||doubleclick.net^", "||googlesyndication.com^", "||adnxs.com^",
                    "||taboola.com^", "||outbrain.com^", "||adservice.google.com^",
                    "/ads.js", "/ad-", "-ad-"
                ];
                filter_set.add_filters(&basic_rules, adblock::lists::ParseOptions::default());

                // Fetch EasyList
                match reqwest::get("https://easylist.to/easylist/easylist.txt").await {
                    Ok(resp) => {
                         if let Ok(text) = resp.text().await {
                             println!("Downloaded EasyList, parsing...");
                             filter_set.add_filters(text.lines().collect::<Vec<_>>(), adblock::lists::ParseOptions::default());
                         }
                    },
                    Err(e) => println!("Failed to fetch EasyList: {}", e),
                }

                let engine = Engine::from_filter_set(filter_set, true);
                let _ = ADBLOCK_ENGINE.set(Arc::new(Mutex::new(engine)));
                println!("Adblock Engine Ready.");
            });

            // Check for PWA args
            let args: Vec<String> = std::env::args().collect();
            let mut pwa_url = None;
            for arg in args {
                if arg.starts_with("--pwa-url=") {
                    pwa_url = Some(arg.replace("--pwa-url=", "").replace("\"", ""));
                }
            }

            if let Some(url) = pwa_url {
                 let label = format!("pwa-{}", chrono::Utc::now().timestamp_micros());
                 if let Ok(parsed_url) = url.parse() {
                     let app_handle = app.handle().clone();
                     let label_clone = label.clone();
                     
                     let invoke_key = app.handle().invoke_key();
                     let pwa_script = get_pwa_init_script(&label, invoke_key);

                     let mut builder = tauri::WebviewWindowBuilder::new(app, &label, tauri::WebviewUrl::External(parsed_url))
                        .title("PWA");

                     #[cfg(target_os = "windows")]
                     {
                         builder = builder.user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/144.0.0.0 Safari/537.36 Edg/144.0.0.0");
                     }
                     #[cfg(target_os = "linux")]
                     {
                         builder = builder.user_agent("Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/144.0.0.0 Safari/537.36");
                     }
                     #[cfg(target_os = "macos")]
                     {
                         builder = builder.user_agent("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/144.0.0.0 Safari/537.36");
                     }

                     let _ = builder.inner_size(1024.0, 768.0)
                        .decorations(true)
                        .focused(true)
                        .initialization_script(get_lumina_stealth_script())
                        .initialization_script(&pwa_script)
                        .on_web_resource_request(move |request, response| {
                            let referer = request.headers().get("referer").and_then(|h| h.to_str().ok());
                            if check_adblock_url(&request.uri().to_string(), referer, &label_clone, &app_handle) {
                                *response = tauri::http::Response::builder()
                                    .status(403)
                                    .body(std::borrow::Cow::Owned(Vec::new()))
                                    .unwrap();
                            }
                        })
                        .build();
                 }
                 if let Some(main) = app.get_webview_window("main") {
                     let _ = main.close();
                 }
            }

            let app_dir = app.path().app_data_dir().unwrap();
            if !app_dir.exists() {
                let _ = std::fs::create_dir_all(&app_dir);
            }
            app.manage(AppDataStore::new(app_dir.clone()));
            app.manage(DownloadManager::new(app_dir.clone()));
            app.manage(HistoryManager::new(app_dir));

            // Tray Setup
            let quit_i = tauri::menu::MenuItem::with_id(app, "quit", "Çıkış", true, None::<&str>)?;
            let show_i = tauri::menu::MenuItem::with_id(app, "show", "Göster", true, None::<&str>)?;
            let menu = tauri::menu::Menu::with_items(app, &[&show_i, &quit_i])?;

            let _tray = tauri::tray::TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
                .tooltip("Lumina Browser")
                .menu(&menu)
                .on_menu_event(|app: &AppHandle, event| {
                    match event.id().as_ref() {
                        "quit" => app.exit(0),
                        "show" => {
                             if let Some(window) = app.get_webview_window("main") {
                                 let _ = window.show();
                                 let _ = window.set_focus();
                             }
                        }
                        _ => {}
                    }
                })
                .on_tray_icon_event(|tray: &tauri::tray::TrayIcon, event| {
                     if let tauri::tray::TrayIconEvent::Click { .. } = event {
                         let app = tray.app_handle();
                         if let Some(window) = app.get_webview_window("main") {
                             let _ = window.show();
                             let _ = window.set_focus();
                         }
                     }
                })
                .build(app)?;

            // Use Listener (restored)
            app.listen("debug-event", |event| {
                println!("Debug event received: {:?}", event);
            });

            Ok(())
        })
        .on_window_event(|window, event| {
            match event {
                tauri::WindowEvent::CloseRequested { .. } => {
                     // Allow window to close (and app to exit if it's the last window)
                     // let _ = window.hide();
                     // api.prevent_close();
                }
                tauri::WindowEvent::Resized(size) => {
                    if window.label() == "main" {
                         let scale_factor = window.scale_factor().unwrap_or(1.0);
                         let logical_size = size.to_logical::<f64>(scale_factor);
                         
                         let state = window.app_handle().state::<UiState>();
                         let sidebar_open = state.sidebar_open.load(std::sync::atomic::Ordering::Relaxed);
                         let suggestions_height = state.suggestions_height.load(std::sync::atomic::Ordering::Relaxed) as f64;
                         
                         let data_store = window.app_handle().state::<AppDataStore>();
                         let vertical_tabs = data_store.data.lock().unwrap().settings.vertical_tabs;

                         let (main_height, x, y, width, height) = calculate_layout(logical_size, vertical_tabs, sidebar_open, suggestions_height);
                         
                         // Resize main webview (UI)
                         if let Some(main_webview) = window.app_handle().get_webview("main") {
                             let _ = main_webview.set_size(tauri::LogicalSize::new(logical_size.width, main_height));
                         }
    
                         // Resize ALL other webviews (browser tabs)
                         let webviews = window.app_handle().webviews();
                         
                         for webview in webviews {
                             let webview_instance = &webview.1;
                             if webview_instance.label() != "main" {
                                 let _ = webview_instance.set_position(tauri::LogicalPosition::new(x, y));
                                 let _ = webview_instance.set_size(tauri::LogicalSize::new(width, height));
                             }
                         }
                    }
                }
                _ => {}
            }
        })
        .invoke_handler(tauri::generate_handler![
            // New Feature Commands
            set_zoom_level,
            get_zoom_level,
            set_cookie,
            get_cookies,
            delete_cookie,
            navigate, 
            force_internal_navigate,
            go_back, 
            go_forward, 
            refresh, 
            init_browser, 
            create_tab, 
            switch_tab, 
            close_tab, 
            update_tab_info, 
            add_history_item, 
            get_history, 
            get_recent_history,
            update_history_title,
            search_history,
            add_favorite, 
            remove_favorite, 
            get_favorites, 
            toggle_sidebar, 
            set_suggestions_height,
            get_settings, 
            save_settings, 
            open_file, 
            show_in_folder, 
            toggle_reader_mode, 
            get_downloads, 
            resume_download, 
            pwa_detected, 
            install_pwa, 
            check_pwa_manifest, 
            open_pwa_window,
            get_open_windows,
            focus_window,
            open_flash_window,
            clean_page,
            run_kip_code,
            run_networking_command,
            run_sidekick,
            request_omnibox_suggestions,
            run_lua_code,
            get_store_items,
            install_package
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
