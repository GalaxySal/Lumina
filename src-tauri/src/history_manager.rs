use rusqlite::{params, Connection, Result};
use serde::{Serialize, Deserialize};
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct HistoryItem {
    pub url: String,
    pub title: String,
    pub visit_count: i64,
    pub last_visit: i64,
}

pub struct HistoryManager {
    db_path: PathBuf,
}

impl HistoryManager {
    pub fn new(app_data_dir: PathBuf) -> Self {
        let db_path = app_data_dir.join("history.db");
        let manager = Self {
            db_path,
        };
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
             LIMIT 20"
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
             LIMIT ?1"
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
}
