mod data;
use data::{AppDataStore, HistoryItem, FavoriteItem, AppSettings};
use tauri::{AppHandle, Manager, WebviewUrl, WebviewBuilder, Emitter, Listener, Url};
use futures_util::StreamExt;
use tokio::io::{AsyncWriteExt, AsyncSeekExt};
use std::collections::HashMap;
use std::sync::Mutex;
use std::path::PathBuf;
use serde::{Serialize, Deserialize};
use std::fs::OpenOptions;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadItem {
    pub url: String,
    pub file_name: String,
    pub total_size: u64,
    pub downloaded_size: u64,
    pub path: String,
    pub status: String, // "downloading", "paused", "completed", "failed"
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
}

async fn check_and_redirect(webview: tauri::Webview, url: String) {
    if url.starts_with("tauri://") || url.starts_with("file://") || url.starts_with("about:") || url.starts_with("data:") {
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

#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[tauri::command]
fn navigate(app: AppHandle, label: String, url: String) {
    println!("Rust: navigating tab {} to {}", label, url);
    if let Some(webview) = app.get_webview(&label) {
        let _ = webview.set_focus();
        // Use eval for navigation as load_url is not available on Webview struct directly in this version
        let _ = webview.eval(format!("window.location.assign('{}')", url));
        
        // Check connection
        let wv = webview.clone();
        let u = url.clone();
        tauri::async_runtime::spawn(async move {
            check_and_redirect(wv, u).await;
        });
    } else {
        println!("Rust: webview {} not found", label);
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
fn add_history_item(state: tauri::State<'_, AppDataStore>, url: String, title: String) {
    state.add_history(url, title);
    state.save();
}

#[tauri::command]
fn get_history(state: tauri::State<'_, AppDataStore>) -> Vec<HistoryItem> {
    state.data.lock().unwrap().history.clone()
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
fn show_in_folder(_path: String) {
    #[cfg(target_os = "windows")]
    {
        let _ = std::process::Command::new("explorer")
            .args(["/select,", &_path])
            .spawn();
    }
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

fn calculate_layout(logical_size: tauri::LogicalSize<f64>, vertical_tabs: bool, menu_open: bool) -> (f64, f64, f64, f64, f64) {
    let top_bar_height = 90.0;
    let sidebar_width = 200.0;
    let menu_width = 320.0;
    let toolbar_height = 50.0;

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
    let vertical_tabs = data_store.data.lock().unwrap().settings.vertical_tabs;
    let main_window = app.get_window("main").ok_or("Main window not found")?;
    let window_size = main_window.inner_size().map_err(|e| e.to_string())?;
    let scale_factor = main_window.scale_factor().map_err(|e| e.to_string())?;
    let logical_size = window_size.to_logical::<f64>(scale_factor);
    
    let (main_height, x, y, width, height) = calculate_layout(logical_size, vertical_tabs, menu_open);
    
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
}

#[tauri::command]
async fn pwa_detected(app: AppHandle, label: String) -> Result<(), String> {
    app.emit("pwa-can-install", TabPwaPayload { label }).map_err(|e| e.to_string())
}

#[tauri::command]
async fn install_pwa(app: AppHandle, label: String) -> Result<(), String> {
    if let Some(webview) = app.get_webview(&label) {
        let script = r#"
            (async function() {
                if (window.deferredPrompt) {
                    console.log("Triggering PWA install prompt...");
                    window.deferredPrompt.prompt();
                    window.deferredPrompt.userChoice.then((choiceResult) => {
                        if (choiceResult.outcome === 'accepted') {
                            console.log('User accepted the install prompt');
                        } else {
                            console.log('User dismissed the install prompt');
                        }
                        window.deferredPrompt = null;
                    });
                } else {
                    console.warn("No deferredPrompt found, falling back to manual PWA window...");
                    var title = document.title || window.location.href;
                    
                    var faviconUrl = null;
                    var links = document.querySelectorAll("link[rel*='icon']");
                    if (links.length > 0) {
                        faviconUrl = links[0].href;
                    }

                    try {
                        var args = { url: window.location.href, title: title, faviconUrl: faviconUrl };
                        if (window.__TAURI__ && window.__TAURI__.core) {
                            await window.__TAURI__.core.invoke('open_pwa_window', args);
                        } else if (window.__TAURI__ && window.__TAURI__.invoke) {
                            await window.__TAURI__.invoke('open_pwa_window', args);
                        } else {
                             // Fallback to our custom invoke
                             if (typeof invoke === 'function') {
                                 invoke('open_pwa_window', args);
                             } else {
                                 console.error("No invoke mechanism found");
                             }
                        }
                    } catch(e) {
                        console.error("Failed to open PWA window:", e);
                    }
                }
            })();
        "#;
        webview.eval(script).map_err(|e| e.to_string())?;
    }
    Ok(())
}

async fn download_icon(app: &AppHandle, url: &str) -> Option<std::path::PathBuf> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .ok()?;
        
    let response = client.get(url).send().await.ok()?;
    let bytes = response.bytes().await.ok()?;
    
    let app_dir = app.path().app_data_dir().ok()?;
    let icons_dir = app_dir.join("icons");
    if !icons_dir.exists() {
        let _ = std::fs::create_dir_all(&icons_dir);
    }

    // Try to load image to convert to ICO (Lumina v0.2.5 PNG->ICO Converter)
    // We use a blocking task because image decoding/encoding is CPU intensive
    let bytes_clone = bytes.clone();
    let icons_dir_clone = icons_dir.clone();
    
    let converted_path = tokio::task::spawn_blocking(move || {
        if let Ok(img) = image::load_from_memory(&bytes_clone) {
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
    let extension = if url.to_lowercase().ends_with(".ico") {
        "ico"
    } else if url.to_lowercase().ends_with(".svg") {
        "svg"
    } else {
        "png" 
    };
    
    let filename = format!("icon_{}.{}", chrono::Utc::now().timestamp_micros(), extension);
    let path = icons_dir.join(&filename);
    
    let mut file = tokio::fs::File::create(&path).await.ok()?;
    file.write_all(&bytes).await.ok()?;
    
    Some(path)
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

#[tauri::command]
async fn open_pwa_window(app: AppHandle, url: String, title: String, favicon_url: Option<String>) -> Result<(), String> {
    let label = sanitize_pwa_label(&url);
    
    // Check if window already exists
    if let Some(window) = app.get_webview_window(&label) {
        let _ = window.set_focus();
        return Ok(());
    }
    
    // Download icon if available
    let icon_path = if let Some(f_url) = favicon_url {
        download_icon(&app, &f_url).await
    } else {
        None
    };

    // Create Desktop Shortcut
    let _ = create_desktop_shortcut(&title, &url, icon_path);

    // Inject a small script to ensure window.close() works if needed, 
    // but primarily we rely on native decorations for controls.
    // We also ensure the window is focused.
    tauri::WebviewWindowBuilder::new(&app, &label, tauri::WebviewUrl::External(url.parse().map_err(|e: url::ParseError| e.to_string())?))
        .title(&title)
        .inner_size(1024.0, 768.0)
        .decorations(true) // Enable native window controls (Close, Minimize, Maximize)
        .focused(true)
        .initialization_script(get_lumina_stealth_script())
        .on_web_resource_request(|request, response| {
            let url = request.uri().to_string();
            if BLOCKED_DOMAINS.iter().any(|d| url.contains(d)) {
                println!("Lumina HostBlock: {}", url);
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
            // We assume the label contains the "pwa-" prefix or is a tab ID
            // For PWAs, we don't have the exact current URL stored in the window object easily accessible 
            // without querying the webview, which is async.
            // For now, we'll return the label as a proxy or use a stored map if we had one.
            // But for "Switch to", title is most important.
            windows.push(WindowInfo {
                label,
                title,
                url: "".to_string(), // TODO: Store initial URL or query it
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
    
    tauri::WebviewWindowBuilder::new(&app, &label, tauri::WebviewUrl::External(url.parse().map_err(|e: url::ParseError| e.to_string())?))
        .title("Flash Tab")
        .inner_size(800.0, 600.0)
        .decorations(false)
        .always_on_top(true)
        .center()
        .focused(true)
        .skip_taskbar(true)
        .initialization_script(get_lumina_stealth_script())
        .on_web_resource_request(|request, response| {
            let url = request.uri().to_string();
            if BLOCKED_DOMAINS.iter().any(|d| url.contains(d)) {
                println!("Lumina HostBlock: {}", url);
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
        console.log("Lumina Stealth Protocol: Activated");

        // 1. CSS Injection for Ad Blocking
        const style = document.createElement('style');
        style.textContent = `
            /* Core Ad Patterns */
            iframe[src*="ads"], iframe[id*="google_ads"], iframe[src*="doubleclick"], 
            iframe[src*="amazon-adsystem"], iframe[src*="adnxs"],
            
            /* Common Ad Containers */
            div[class*="ad-"], div[id*="ad-"],
            div[class*="ads-"], div[id*="ads-"],
            div[class*="sponsor"], div[id*="sponsor"],
            div[class*="banner"], div[id*="banner"],
            
            /* Google & Networks */
            ins.adsbygoogle, div[id^="google_ads_"],
            
            /* Native Ad Widgets */
            div[id*="taboola"], div[class*="taboola"],
            div[id*="outbrain"], div[class*="outbrain"],
            
            /* Overlays & Popups */
            div[class*="popup"][class*="ad"], div[class*="modal"][class*="ad"],
            div[id*="popup"][id*="ad"], div[id*="modal"][id*="ad"],
            
            /* Video Ads */
            div[class*="video-ad"], .ad-showing
            
            { display: none !important; visibility: hidden !important; height: 0 !important; width: 0 !important; pointer-events: none !important; overflow: hidden !important; }
        `;
        
        function injectCSS() {
            const head = document.head || document.documentElement;
            if (head) head.appendChild(style);
        }
        
        if (document.readyState === 'loading') {
            document.addEventListener('DOMContentLoaded', injectCSS);
        } else {
            injectCSS();
        }

        // 2. Global Ad-Intervention (Overlay Remover)
        function killAdOverlays() {
            // Lumina Friendly Exception: Skip aggressive removal on Google and Gemini
            const host = window.location.hostname;
            if (host.includes('google.com') || host.includes('gemini') || host.includes('youtube.com')) {
                console.log("Lumina Stealth: Friendly domain detected (" + host + "), skipping aggressive overlay removal.");
                return;
            }

            const keywords = ['modal', 'popup', 'overlay', 'interstitial', 'ads', 'promo', 'subscribe', 'sign-up'];
            // Select potential overlays
            const elements = document.querySelectorAll('div, section, aside, iframe');
            
            elements.forEach(el => {
                const id = (el.id || '').toLowerCase();
                const cls = (el.className || '').toString().toLowerCase();
                const style = window.getComputedStyle(el);
                
                // Check for fixed/absolute positioning + high z-index
                const isFloating = style.position === 'fixed' || style.position === 'absolute';
                const isHighZ = parseInt(style.zIndex) > 50;
                
                // Check for keywords
                const hasKeyword = keywords.some(k => id.includes(k) || cls.includes(k));
                
                if (isFloating && isHighZ && hasKeyword) {
                     console.log("Lumina Ad-Intervention: Killing overlay ->", el);
                     el.remove();
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
fn update_tab_info(app: AppHandle, label: String, title: Option<String>, favicon: Option<String>) {
    let _ = app.emit("tab-updated", TabUpdatedPayload { label, title, favicon });
}

struct UiState {
    sidebar_open: std::sync::atomic::AtomicBool,
}



#[tauri::command]
async fn create_tab(state: tauri::State<'_, UiState>, app: AppHandle, data_store: tauri::State<'_, AppDataStore>, label: String, url: String, window: tauri::Window) -> Result<(), String> {
    if app.get_webview(&label).is_some() {
        // If tab already exists, just switch to it (optional logic)
        return Ok(());
    }

    let window_size = window.inner_size().map_err(|e| e.to_string())?;
    let scale_factor = window.scale_factor().map_err(|e| e.to_string())?;
    let logical_size = window_size.to_logical::<f64>(scale_factor);
    
    let vertical_tabs = data_store.data.lock().unwrap().settings.vertical_tabs;
    let sidebar_open = state.sidebar_open.load(std::sync::atomic::Ordering::Relaxed);
    
    let (main_height, x, y, tab_width, tab_height) = calculate_layout(logical_size, vertical_tabs, sidebar_open);
    
    // Resize main webview (UI) to cover the top area
    if let Some(main_webview) = app.get_webview("main") {
        let _ = main_webview.set_size(tauri::LogicalSize::new(logical_size.width, main_height));
    }
    
    let app_handle = app.clone();
    let app_handle_dl = app.clone();


    let label_clone = label.clone();
    
    let ad_block_script = format!("{}\n{}", get_lumina_stealth_script(), r#"
        (function() {
            // Helper to generate UUID for new tabs
            function generateUUID() {
                if (typeof crypto !== 'undefined' && crypto.randomUUID) {
                    return crypto.randomUUID();
                }
                return 'xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx'.replace(/[xy]/g, function(c) {
                    var r = Math.random() * 16 | 0, v = c == 'x' ? r : (r & 0x3 | 0x8);
                    return v.toString(16);
                });
            }

            // Override window.open to open in new tab via Rust
            window.open = function(url, target, features) {
                console.log("Intercepted window.open: " + url);
                if (url) {
                    try {
                        url = new URL(url, window.location.href).href;
                    } catch(e) {
                        console.error("Invalid URL:", url);
                        return null;
                    }
                    
                    if (window.__TAURI__) {
                        window.__TAURI__.core.invoke('create_tab', { 
                            label: 'tab-' + generateUUID(), 
                            url: url 
                        }).catch(err => console.error("Failed to create tab:", err));
                    }
                }
                return null;
            };

            // Intercept links with target="_blank"
            document.addEventListener('click', function(e) {
                let target = e.target;
                while (target && target.tagName !== 'A') {
                    target = target.parentElement;
                }
                if (target && target.tagName === 'A' && target.target === '_blank') {
                    e.preventDefault();
                    console.log("Intercepted target=_blank link: " + target.href);
                    if (window.__TAURI__) {
                        window.__TAURI__.core.invoke('create_tab', { 
                            label: 'tab-' + generateUUID(), 
                            url: target.href 
                        }).catch(err => console.error("Failed to create tab:", err));
                    }
                }
            }, true);
        })();
    "#);

    // Attempt to get invoke key
     let invoke_key = app.invoke_key();
     
     let info_script = format!(r#"
         (function() {{
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
                    window.chrome.webview.postMessage(JSON.stringify(msg));
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
            function checkManifest() {{
                if (window.deferredPrompt) return; // Already detected
                
                var link = document.querySelector("link[rel='manifest']");
                if (link && link.href) {{
                    // Send to Rust to check (bypasses CSP and CORS)
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

            function updateInfo() {{
                 let title = document.title;
                 let favicon = getFavicon();
                 invoke('update_tab_info', {{
                     label: window.__TAB_LABEL__,
                     title: title,
                     favicon: favicon
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

            if (document.body || document.head || document.documentElement) {{
                initObserver();
            }} else {{
                document.addEventListener('DOMContentLoaded', initObserver);
            }}
            
            // Initial call
            if (document.readyState === 'complete' || document.readyState === 'interactive') {{
                updateInfo();
            }} else {{
                window.addEventListener('DOMContentLoaded', updateInfo);
                window.addEventListener('load', updateInfo);
            }}
        }})();
    "#, label_clone, invoke_key);

    let full_script = format!("{}\n{}", ad_block_script, info_script);

    let url_parsed = match url.parse() {
        Ok(u) => u,
        Err(e) => return Err(format!("Invalid URL: {}", e)),
    };

    let builder = WebviewBuilder::new(&label, WebviewUrl::External(url_parsed))
        .initialization_script(&full_script)
        .on_web_resource_request(|request, response| {
             // Lumina Stealth: Rust-side Ad/Tracker Blocking
             let url = request.uri().to_string();
             if BLOCKED_DOMAINS.iter().any(|d| url.contains(d)) {
                   println!("Lumina Stealth blocked: {}", url);
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
            println!("Navigation: {} -> {}", label_clone, url);
            let _ = app_handle.emit("tab-navigation", TabNavigationPayload {
                label: label_clone.clone(),
                url: url.to_string(),
            });

            // Check connection
            let app = app_handle.clone();
            let l = label_clone.clone();
            let u = url.to_string();
            tauri::async_runtime::spawn(async move {
                 // Slight delay to ensure webview is ready if this is initial nav
                 tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                 if let Some(wv) = app.get_webview(&l) {
                     check_and_redirect(wv, u).await;
                 }
            });

            true
        });
    
    let res = window.add_child(
        builder,
        tauri::LogicalPosition::new(x, y),
        tauri::LogicalSize::new(tab_width, tab_height),
    );
    
    match res {
        Ok(webview) => {
            // New tab is created. 
            // We should hide other tabs (browser webviews) if we want to simulate "opening" this one foreground.
            // But 'switch_tab' should handle visibility. 
            // For now, let's just make sure this one is shown.
            let _ = webview.show();
            let _ = webview.set_focus();
            let _ = app.emit("tab-created", TabCreatedPayload {
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
        Err(e) => println!("Error creating tab {}: {:?}", label, e),
    }
    
    Ok(())
}

#[tauri::command]
fn switch_tab(app: AppHandle, label: String) {
    println!("Switching to tab: {}", label);
    
    // Hide all webviews that are NOT the main UI and NOT the target tab
    // Note: We need a way to identify "browser tabs". 
    // Convention: anything that isn't "main" is a browser tab.
    
    let webviews = app.webviews();
    for webview in webviews {
        // Handle both Vec<Webview> and HashMap iteration (tuple) just in case
        // But since compiler says it is a tuple (String, Webview), we treat it as such.
        // Wait, we can't easily handle both. We must match the compiler error.
        // The error says: no method named `label` found for tuple `(std::string::String, tauri::Webview)`
        // This means 'webview' variable IS the tuple.
        let webview_instance = &webview.1; 
        
        let wv_label = webview_instance.label();
        if wv_label == "main" {
            continue;
        }
        
        if wv_label == label {
            let _ = webview_instance.show();
            let _ = webview_instance.set_focus();
        } else {
            let _ = webview_instance.hide();
        }
    }
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
    let top_ui_height = 90.0;
    
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
async fn check_pwa_manifest(app: AppHandle, label: String, url: String) -> Result<(), String> {
    println!("Checking PWA manifest for {}: {}", label, url);
    match reqwest::get(&url).await {
        Ok(res) => {
            if let Ok(manifest) = res.json::<serde_json::Value>().await {
                 if let Some(display) = manifest.get("display").and_then(|v: &serde_json::Value| v.as_str()) {
                     if display == "standalone" || display == "minimal-ui" || display == "fullscreen" {
                         println!("PWA Manifest confirmed via Rust for {}", label);
                         let _ = app.emit("pwa-can-install", TabPwaPayload { label });
                     }
                 }
            }
        }
        Err(e) => println!("Failed to fetch manifest: {}", e),
    }
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(UiState { sidebar_open: std::sync::atomic::AtomicBool::new(false) })
        .setup(|app| {
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
                     let _ = tauri::WebviewWindowBuilder::new(app, &label, tauri::WebviewUrl::External(parsed_url))
                        .title("PWA")
                        .inner_size(1024.0, 768.0)
                        .decorations(true)
                        .focused(true)
                        .initialization_script(get_lumina_stealth_script())
                        .on_web_resource_request(|request, response| {
                            let url = request.uri().to_string();
                            if BLOCKED_DOMAINS.iter().any(|d| url.contains(d)) {
                                println!("Lumina HostBlock: {}", url);
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
            app.manage(DownloadManager::new(app_dir));

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
                tauri::WindowEvent::CloseRequested { api, .. } => {
                    let _ = window.hide();
                    api.prevent_close();
                }
                tauri::WindowEvent::Resized(size) => {
                    if window.label() == "main" {
                         let scale_factor = window.scale_factor().unwrap_or(1.0);
                         let logical_size = size.to_logical::<f64>(scale_factor);
                         let top_ui_height = 90.0;
                         
                         let state = window.app_handle().state::<UiState>();
                         let sidebar_open = state.sidebar_open.load(std::sync::atomic::Ordering::Relaxed);
                         
                         let main_height = if sidebar_open { logical_size.height } else { top_ui_height };
                         
                         // Resize main webview (UI)
                         if let Some(main_webview) = window.app_handle().get_webview("main") {
                             let _ = main_webview.set_size(tauri::LogicalSize::new(logical_size.width, main_height));
                         }
    
                         // Resize ALL other webviews (browser tabs)
                         let webviews = window.app_handle().webviews();
                         let sidebar_width = if sidebar_open { 320.0 } else { 0.0 };
                         let new_width = logical_size.width - sidebar_width;
                         
                         for webview in webviews {
                             let webview_instance = &webview.1;
                             if webview_instance.label() != "main" {
                                 let _ = webview_instance.set_position(tauri::LogicalPosition::new(0.0, top_ui_height));
                                 let _ = webview_instance.set_size(tauri::LogicalSize::new(new_width, logical_size.height - top_ui_height));
                             }
                         }
                    }
                }
                _ => {}
            }
        })
        .invoke_handler(tauri::generate_handler![
            greet, 
            navigate, 
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
            add_favorite, 
            remove_favorite, 
            get_favorites, 
            toggle_sidebar, 
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
            clean_page
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
