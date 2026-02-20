use rusqlite::{params, Connection, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct HistoryItem {
    pub url: String,
    pub title: String,
    pub visit_count: i64,
    pub last_visit: i64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CookieItem {
    pub domain: String,
    pub name: String,
    pub value: String,
    pub expires: Option<i64>,
    pub path: String,
    pub secure: bool,
    pub http_only: bool,
}

#[allow(dead_code)]
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FormDataItem {
    pub field_name: String,
    pub field_value: String,
    pub domain: String,
    pub last_used: i64,
    pub use_count: i64,
}

#[allow(dead_code)]
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WebStorageItem {
    pub domain: String,
    pub key: String,
    pub value: String,
    pub storage_type: String, // "localStorage" or "sessionStorage"
}

#[allow(dead_code)]
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ZoomLevel {
    pub domain: String,
    pub zoom: i32, // percentage (100 = 100%)
}

pub struct HistoryManager {
    db_path: PathBuf,
}

impl HistoryManager {
    pub fn new(app_data_dir: PathBuf) -> Self {
        let db_path = app_data_dir.join("history.db");
        let manager = Self { db_path };
        if let Err(e) = manager.init() {
            eprintln!("Failed to initialize history database: {}", e);
        }
        manager
    }

    fn connect(&self) -> Result<Connection> {
        Connection::open(&self.db_path)
    }

    fn init(&self) -> Result<()> {
        let conn = self.connect()?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS history (
                id INTEGER PRIMARY KEY,
                url TEXT NOT NULL UNIQUE,
                title TEXT,
                visit_count INTEGER DEFAULT 1,
                last_visit INTEGER
            )",
            [],
        )?;

        // Cookies table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS cookies (
                id INTEGER PRIMARY KEY,
                domain TEXT NOT NULL,
                name TEXT NOT NULL,
                value TEXT,
                expires INTEGER,
                path TEXT DEFAULT '/',
                secure BOOLEAN DEFAULT 0,
                http_only BOOLEAN DEFAULT 0,
                created_at INTEGER,
                UNIQUE(domain, name, path)
            )",
            [],
        )?;

        // Form data table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS form_data (
                id INTEGER PRIMARY KEY,
                field_name TEXT NOT NULL,
                field_value TEXT,
                domain TEXT NOT NULL,
                last_used INTEGER,
                use_count INTEGER DEFAULT 1,
                UNIQUE(field_name, field_value, domain)
            )",
            [],
        )?;

        // Web storage (localStorage/sessionStorage)
        conn.execute(
            "CREATE TABLE IF NOT EXISTS web_storage (
                id INTEGER PRIMARY KEY,
                domain TEXT NOT NULL,
                key TEXT NOT NULL,
                value TEXT,
                storage_type TEXT DEFAULT 'localStorage',
                last_modified INTEGER,
                UNIQUE(domain, key, storage_type)
            )",
            [],
        )?;

        // Zoom levels per domain
        conn.execute(
            "CREATE TABLE IF NOT EXISTS zoom_levels (
                id INTEGER PRIMARY KEY,
                domain TEXT NOT NULL UNIQUE,
                zoom INTEGER DEFAULT 100
            )",
            [],
        )?;

        Ok(())
    }

    pub fn add_visit(&self, url: String, title: String) -> Result<()> {
        let conn = self.connect()?;
        let now = chrono::Utc::now().timestamp();

        // Upsert logic
        // SQLite has ON CONFLICT DO UPDATE
        conn.execute(
            "INSERT INTO history (url, title, visit_count, last_visit) 
             VALUES (?1, ?2, 1, ?3)
             ON CONFLICT(url) DO UPDATE SET 
                visit_count = visit_count + 1,
                last_visit = excluded.last_visit,
                title = excluded.title",
            params![url, title, now],
        )?;
        Ok(())
    }

    pub fn search(&self, query: &str) -> Result<Vec<HistoryItem>> {
        let conn = self.connect()?;
        let mut stmt = conn.prepare(
            "SELECT url, title, visit_count, last_visit FROM history 
             WHERE url LIKE ?1 OR title LIKE ?1 
             ORDER BY visit_count DESC, last_visit DESC 
             LIMIT 20",
        )?;

        let pattern = format!("%{}%", query);
        let rows = stmt.query_map(params![pattern], |row| {
            Ok(HistoryItem {
                url: row.get(0)?,
                title: row.get(1)?,
                visit_count: row.get(2)?,
                last_visit: row.get(3)?,
            })
        })?;

        let mut items = Vec::new();
        for row in rows {
            items.push(row?);
        }
        Ok(items)
    }

    pub fn get_recent(&self, limit: i64) -> Result<Vec<HistoryItem>> {
        let conn = self.connect()?;
        let mut stmt = conn.prepare(
            "SELECT url, title, visit_count, last_visit FROM history 
             ORDER BY last_visit DESC 
             LIMIT ?1",
        )?;

        let rows = stmt.query_map(params![limit], |row| {
            Ok(HistoryItem {
                url: row.get(0)?,
                title: row.get(1)?,
                visit_count: row.get(2)?,
                last_visit: row.get(3)?,
            })
        })?;

        let mut items = Vec::new();
        for row in rows {
            items.push(row?);
        }
        Ok(items)
    }

    pub fn update_title(&self, url: String, title: String) -> Result<()> {
        let conn = self.connect()?;
        conn.execute(
            "UPDATE history SET title = ?2 WHERE url = ?1",
            params![url, title],
        )?;
        Ok(())
    }

    // ============= COOKIES =============
    pub fn set_cookie(&self, cookie: CookieItem) -> Result<()> {
        let conn = self.connect()?;
        let now = chrono::Utc::now().timestamp();
        conn.execute(
            "INSERT INTO cookies (domain, name, value, expires, path, secure, http_only, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(domain, name, path) DO UPDATE SET value = excluded.value, expires = excluded.expires",
            params![cookie.domain, cookie.name, cookie.value, cookie.expires, cookie.path, cookie.secure, cookie.http_only, now],
        )?;
        Ok(())
    }

    pub fn get_cookies(&self, domain: &str) -> Result<Vec<CookieItem>> {
        let conn = self.connect()?;
        let now = chrono::Utc::now().timestamp();
        let mut stmt = conn.prepare(
            "SELECT domain, name, value, expires, path, secure, http_only FROM cookies 
             WHERE domain = ?1 AND (expires IS NULL OR expires > ?2)",
        )?;

        let cookies = stmt.query_map(params![domain, now], |row| {
            Ok(CookieItem {
                domain: row.get(0)?,
                name: row.get(1)?,
                value: row.get(2)?,
                expires: row.get(3)?,
                path: row.get(4)?,
                secure: row.get(5)?,
                http_only: row.get(6)?,
            })
        })?;

        let mut result = Vec::new();
        for cookie in cookies {
            result.push(cookie?);
        }
        Ok(result)
    }

    pub fn delete_cookie(&self, domain: &str, name: &str) -> Result<()> {
        let conn = self.connect()?;
        conn.execute(
            "DELETE FROM cookies WHERE domain = ?1 AND name = ?2",
            params![domain, name],
        )?;
        Ok(())
    }

    // ============= FORM DATA =============
    #[allow(dead_code)]
    pub fn save_form_data(&self, item: FormDataItem) -> Result<()> {
        let conn = self.connect()?;
        let now = chrono::Utc::now().timestamp();
        conn.execute(
            "INSERT INTO form_data (field_name, field_value, domain, last_used, use_count)
             VALUES (?1, ?2, ?3, ?4, 1)
             ON CONFLICT(field_name, field_value, domain) DO UPDATE SET use_count = use_count + 1, last_used = ?4",
            params![item.field_name, item.field_value, item.domain, now],
        )?;
        Ok(())
    }

    #[allow(dead_code)]
    pub fn get_form_suggestions(&self, field_name: &str, domain: &str) -> Result<Vec<String>> {
        let conn = self.connect()?;
        let mut stmt = conn.prepare(
            "SELECT DISTINCT field_value FROM form_data 
             WHERE field_name = ?1 AND domain = ?2
             ORDER BY use_count DESC, last_used DESC 
             LIMIT 10",
        )?;

        let values = stmt.query_map(params![field_name, domain], |row| row.get(0))?;

        let mut result = Vec::new();
        for val in values {
            result.push(val?);
        }
        Ok(result)
    }

    // ============= WEB STORAGE =============
    #[allow(dead_code)]
    pub fn set_web_storage(
        &self,
        domain: &str,
        key: &str,
        value: &str,
        storage_type: &str,
    ) -> Result<()> {
        let conn = self.connect()?;
        let now = chrono::Utc::now().timestamp();
        conn.execute(
            "INSERT INTO web_storage (domain, key, value, storage_type, last_modified)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(domain, key, storage_type) DO UPDATE SET value = excluded.value, last_modified = ?5",
            params![domain, key, value, storage_type, now],
        )?;
        Ok(())
    }

    #[allow(dead_code)]
    pub fn get_web_storage(
        &self,
        domain: &str,
        storage_type: &str,
    ) -> Result<Vec<(String, String)>> {
        let conn = self.connect()?;
        let mut stmt = conn.prepare(
            "SELECT key, value FROM web_storage WHERE domain = ?1 AND storage_type = ?2",
        )?;

        let items = stmt.query_map(params![domain, storage_type], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })?;

        let mut result = Vec::new();
        for item in items {
            result.push(item?);
        }
        Ok(result)
    }

    // ============= ZOOM LEVELS =============
    pub fn set_zoom_level(&self, domain: &str, zoom: i32) -> Result<()> {
        let conn = self.connect()?;
        conn.execute(
            "INSERT INTO zoom_levels (domain, zoom) VALUES (?1, ?2)
             ON CONFLICT(domain) DO UPDATE SET zoom = ?2",
            params![domain, zoom],
        )?;
        Ok(())
    }

    pub fn get_zoom_level(&self, domain: &str) -> Result<i32> {
        let conn = self.connect()?;
        let mut stmt = conn.prepare("SELECT zoom FROM zoom_levels WHERE domain = ?1")?;

        let zoom = stmt.query_row(params![domain], |row| row.get(0));
        Ok(zoom.unwrap_or(100))
    }
}
