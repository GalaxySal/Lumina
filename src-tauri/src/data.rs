use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct HistoryItem {
    pub url: String,
    pub title: String,
    pub timestamp: i64,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct FavoriteItem {
    pub url: String,
    pub title: String,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct AppSettings {
    pub homepage: String,
    pub search_engine: String, // "google", "bing", "duckduckgo"
    pub theme: String, // "dark", "light", "system"
    pub accent_color: String, // Hex color e.g., "#3b82f6"
    pub vertical_tabs: bool,
    pub rounded_corners: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            homepage: "https://www.google.com".to_string(),
            search_engine: "google".to_string(),
            theme: "dark".to_string(),
            accent_color: "#3b82f6".to_string(),
            vertical_tabs: false,
            rounded_corners: true,
        }
    }
}

#[derive(Clone, Serialize, Deserialize, Debug, Default)]
pub struct AppData {
    pub history: Vec<HistoryItem>,
    pub favorites: Vec<FavoriteItem>,
    #[serde(default)]
    pub settings: AppSettings,
}

pub struct AppDataStore {
    pub data: Mutex<AppData>,
    pub file_path: PathBuf,
}

impl AppDataStore {
    pub fn new(app_dir: PathBuf) -> Self {
        let file_path = app_dir.join("browser_data.json");
        let data = if file_path.exists() {
            let content = fs::read_to_string(&file_path).unwrap_or_default();
            serde_json::from_str(&content).unwrap_or_default()
        } else {
            AppData::default()
        };

        Self {
            data: Mutex::new(data),
            file_path,
        }
    }

    pub fn save(&self) {
        let data = self.data.lock().unwrap();
        let content = serde_json::to_string_pretty(&*data).unwrap();
        let _ = fs::write(&self.file_path, content);
    }

    pub fn add_history(&self, url: String, title: String) {
        let mut data = self.data.lock().unwrap();
        // Remove duplicate if exists (simple logic: move to top)
        if let Some(pos) = data.history.iter().position(|x| x.url == url) {
            data.history.remove(pos);
        }
        
        data.history.insert(0, HistoryItem {
            url,
            title,
            timestamp: chrono::Utc::now().timestamp(),
        });
        
        // Limit history to 100 items
        if data.history.len() > 100 {
            data.history.truncate(100);
        }
    }

    pub fn add_favorite(&self, url: String, title: String) {
        let mut data = self.data.lock().unwrap();
        if !data.favorites.iter().any(|x| x.url == url) {
            data.favorites.push(FavoriteItem { url, title });
        }
    }

    pub fn remove_favorite(&self, url: String) {
        let mut data = self.data.lock().unwrap();
        if let Some(pos) = data.favorites.iter().position(|x| x.url == url) {
            data.favorites.remove(pos);
        }
    }
    
    pub fn update_settings(&self, homepage: String, search_engine: String, theme: String, accent_color: String, vertical_tabs: bool, rounded_corners: bool) {
        let mut data = self.data.lock().unwrap();
        data.settings.homepage = homepage;
        data.settings.search_engine = search_engine;
        data.settings.theme = theme;
        data.settings.accent_color = accent_color;
        data.settings.vertical_tabs = vertical_tabs;
        data.settings.rounded_corners = rounded_corners;
    }
}
