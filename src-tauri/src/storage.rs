use crate::model::{DownloadTask, Settings, TaskStatus};
use rusqlite::{params, Connection};
use std::{path::Path, sync::Mutex};

pub struct Storage {
    connection: Mutex<Connection>,
}

impl Storage {
    pub fn open(path: &Path) -> Result<Self, String> {
        let connection = Connection::open(path).map_err(|e| e.to_string())?;
        connection.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL; CREATE TABLE IF NOT EXISTS tasks (id TEXT PRIMARY KEY, body TEXT NOT NULL); CREATE TABLE IF NOT EXISTS settings (id INTEGER PRIMARY KEY CHECK(id=1), body TEXT NOT NULL);").map_err(|e| e.to_string())?;
        Ok(Self {
            connection: Mutex::new(connection),
        })
    }

    pub fn load_tasks(&self) -> Result<Vec<DownloadTask>, String> {
        let db = self.connection.lock().map_err(|e| e.to_string())?;
        let mut statement = db
            .prepare("SELECT body FROM tasks")
            .map_err(|e| e.to_string())?;
        let rows = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|e| e.to_string())?;
        let mut tasks = Vec::new();
        for row in rows {
            let mut task: DownloadTask = serde_json::from_str(&row.map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
            if matches!(
                task.status,
                TaskStatus::Downloading | TaskStatus::Merging | TaskStatus::Queued
            ) {
                task.status = TaskStatus::Paused;
                task.error = Some("上次下载已中断，点击继续可从本地记录恢复。".into());
            }
            task.speed = 0;
            tasks.push(task);
        }
        Ok(tasks)
    }

    pub fn save_task(&self, task: &DownloadTask) -> Result<(), String> {
        let json = serde_json::to_string(task).map_err(|e| e.to_string())?;
        self.connection
            .lock()
            .map_err(|e| e.to_string())?
            .execute(
                "INSERT OR REPLACE INTO tasks (id, body) VALUES (?1, ?2)",
                params![task.id, json],
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn remove_task(&self, id: &str) -> Result<(), String> {
        self.connection
            .lock()
            .map_err(|e| e.to_string())?
            .execute("DELETE FROM tasks WHERE id=?1", [id])
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Commit a whole enqueue/start reservation before exposing it to the scheduler.
    pub fn save_tasks(&self, tasks: &[DownloadTask]) -> Result<(), String> {
        let mut db = self.connection.lock().map_err(|e| e.to_string())?;
        let transaction = db.transaction().map_err(|e| e.to_string())?;
        for task in tasks {
            let body = serde_json::to_string(task).map_err(|e| e.to_string())?;
            transaction
                .execute(
                    "INSERT OR REPLACE INTO tasks (id, body) VALUES (?1, ?2)",
                    params![task.id, body],
                )
                .map_err(|e| e.to_string())?;
        }
        transaction.commit().map_err(|e| e.to_string())
    }

    pub fn load_settings(&self, defaults: Settings) -> Result<Settings, String> {
        let db = self.connection.lock().map_err(|e| e.to_string())?;
        let mut stmt = db
            .prepare("SELECT body FROM settings WHERE id=1")
            .map_err(|e| e.to_string())?;
        let mut rows = stmt.query([]).map_err(|e| e.to_string())?;
        match rows.next().map_err(|e| e.to_string())? {
            Some(row) => serde_json::from_str(&row.get::<_, String>(0).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string()),
            None => Ok(defaults),
        }
    }

    pub fn save_settings(&self, settings: &Settings) -> Result<(), String> {
        let json = serde_json::to_string(settings).map_err(|e| e.to_string())?;
        self.connection
            .lock()
            .map_err(|e| e.to_string())?
            .execute(
                "INSERT OR REPLACE INTO settings (id,body) VALUES (1,?1)",
                [json],
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn settings_roundtrip() {
        let store = Storage::open(Path::new(":memory:")).unwrap();
        let settings = Settings {
            clipboard_enabled: true,
            clipboard_auto_parse: false,
            download_dir: "D:\\视频".into(),
            concurrency: 2,
            max_speed_mbps: 0,
            save_cover: true,
            save_danmaku: false,
            save_subtitles: false,
        };
        store.save_settings(&settings).unwrap();
        assert_eq!(
            store.load_settings(settings).unwrap().download_dir,
            "D:\\视频"
        );
    }

    #[test]
    fn batch_write_rolls_back_every_row_if_one_insert_fails() {
        let store = Storage::open(Path::new(":memory:")).unwrap();
        store.connection.lock().unwrap().execute_batch("CREATE TRIGGER reject_bad_task BEFORE INSERT ON tasks WHEN NEW.id='bad' BEGIN SELECT RAISE(ABORT, 'test failure'); END;").unwrap();
        let task_json = serde_json::json!({"id":"good","bvid":"BV1xx411c7mD","cid":1,"title":"test","pageTitle":"P1","page":1,"cover":"","quality":32,"qualityLabel":"480P","format":"video","status":"queued","progress":0.0,"downloadedBytes":0,"totalBytes":0,"speed":0,"error":null,"outputPath":null,"createdAt":0,"downloadDir":"D:\\videos","saveCover":false,"saveDanmaku":false,"saveSubtitles":false,"warning":null});
        let good: DownloadTask = serde_json::from_value(task_json).unwrap();
        let mut bad = good.clone();
        bad.id = "bad".into();
        assert!(store.save_tasks(&[good, bad]).is_err());
        assert!(store.load_tasks().unwrap().is_empty());
    }
}
