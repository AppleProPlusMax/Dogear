//! 剪切板历史的 SQLite 持久化。
//! 启动时把上一次留下的记录读回内存，运行中每次新增、去重挪前、淘汰或 OCR 更新都同步落库。

use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use rusqlite::{params, Connection};
use tauri::{AppHandle, Manager};

use crate::state::Clip;

static DB: OnceLock<Mutex<Connection>> = OnceLock::new();

pub struct LoadedHistory {
    pub clips: Vec<Clip>,
    pub next_id: i64,
}

impl Default for LoadedHistory {
    fn default() -> Self {
        Self {
            clips: Vec::new(),
            next_id: 1,
        }
    }
}

pub fn init(app: &AppHandle, limit: usize) -> LoadedHistory {
    let path = match db_path(app) {
        Ok(path) => path,
        Err(err) => {
            eprintln!("无法定位历史数据库路径，本次仅使用内存: {err}");
            return LoadedHistory::default();
        }
    };
    let conn = match open_connection(&path) {
        Ok(conn) => conn,
        Err(err) => {
            eprintln!("打开历史数据库失败，本次仅使用内存: {err}");
            return LoadedHistory::default();
        }
    };
    let mut conn = conn;
    let loaded = match load_and_prune(&mut conn, limit, true) {
        Ok(loaded) => loaded,
        Err(err) => {
            eprintln!("读取历史数据库失败，从空列表开始: {err}");
            LoadedHistory::default()
        }
    };
    let _ = DB.set(Mutex::new(conn));
    crate::images::cleanup_orphans(app, &loaded.clips);
    loaded
}

/// 保存新增或挪到最前的记录，并顺带删掉超出上限被挤出去的旧记录。
pub fn save_step(clip: &Clip, removed_ids: &[i64]) {
    let Some(lock) = DB.get() else {
        return;
    };
    let Ok(mut conn) = lock.lock() else {
        return;
    };
    if let Err(err) = save_step_in(&mut conn, clip, removed_ids) {
        eprintln!("写入历史记录失败: {err}");
    }
}

/// 同一张图片再次复制时更新时间并排到最前。
pub fn touch_clip(id: i64, created_at: i64) {
    let Some(lock) = DB.get() else {
        return;
    };
    let Ok(conn) = lock.lock() else {
        return;
    };
    if let Err(err) = touch_clip_in(&conn, id, created_at) {
        eprintln!("更新历史记录时间失败: {err}");
    }
}

/// 同步图片的 OCR 识别状态与结果。
pub fn update_ocr(id: i64, status: &str, text: Option<&str>, lang: Option<&str>) {
    let Some(lock) = DB.get() else {
        return;
    };
    let Ok(conn) = lock.lock() else {
        return;
    };
    if let Err(err) = update_ocr_in(&conn, id, status, text, lang) {
        eprintln!("更新历史 OCR 状态失败: {err}");
    }
}

fn db_path(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|err| format!("无法获取数据目录: {err}"))?;
    std::fs::create_dir_all(&dir).map_err(|err| format!("无法创建数据目录: {err}"))?;
    Ok(dir.join("dogear.db"))
}

fn open_connection(path: &Path) -> Result<Connection, String> {
    let conn = Connection::open(path).map_err(|err| err.to_string())?;
    conn.busy_timeout(std::time::Duration::from_millis(1500))
        .map_err(|err| err.to_string())?;
    conn.execute_batch(
        "PRAGMA journal_mode = WAL;
         PRAGMA synchronous = NORMAL;",
    )
    .map_err(|err| err.to_string())?;
    create_schema(&conn)?;
    Ok(conn)
}

fn create_schema(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS clips (
            id INTEGER PRIMARY KEY,
            content TEXT NOT NULL,
            created_at INTEGER NOT NULL,
            kind TEXT NOT NULL,
            language TEXT,
            file_path TEXT,
            thumb_path TEXT,
            hash TEXT,
            width INTEGER,
            height INTEGER,
            ocr_status TEXT NOT NULL DEFAULT 'none',
            ocr_text TEXT,
            ocr_lang TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_clips_created_at
            ON clips(created_at DESC, id DESC);",
    )
    .map_err(|err| err.to_string())
}

fn load_and_prune(
    conn: &mut Connection,
    limit: usize,
    check_image_files: bool,
) -> Result<LoadedHistory, String> {
    let max_id: i64 = conn
        .query_row("SELECT COALESCE(MAX(id), 0) FROM clips", [], |row| row.get(0))
        .map_err(|err| err.to_string())?;

    let all = query_all(conn)?;
    let mut clips = Vec::with_capacity(all.len().min(limit));
    let mut stale_ids = Vec::new();

    for clip in all {
        if check_image_files && clip.kind == "image" {
            let exists = clip
                .file_path
                .as_deref()
                .is_some_and(|p| Path::new(p).is_file());
            if !exists {
                stale_ids.push(clip.id);
                continue;
            }
        }
        if clips.len() < limit {
            clips.push(clip);
        } else {
            stale_ids.push(clip.id);
        }
    }

    if !stale_ids.is_empty() {
        let tx = conn.transaction().map_err(|err| err.to_string())?;
        delete_ids(&tx, &stale_ids)?;
        tx.commit().map_err(|err| err.to_string())?;
    }

    let next_id = clips
        .iter()
        .map(|clip| clip.id)
        .max()
        .unwrap_or(0)
        .max(max_id)
        + 1;

    Ok(LoadedHistory { clips, next_id })
}

fn query_all(conn: &Connection) -> Result<Vec<Clip>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT
                id, content, created_at, kind, language,
                file_path, thumb_path, hash, width, height,
                ocr_status, ocr_text, ocr_lang
             FROM clips
             ORDER BY created_at DESC, id DESC",
        )
        .map_err(|err| err.to_string())?;

    let rows = stmt
        .query_map([], |row| {
            let width: Option<i64> = row.get(8)?;
            let height: Option<i64> = row.get(9)?;
            Ok(Clip {
                id: row.get(0)?,
                content: row.get(1)?,
                created_at: row.get(2)?,
                kind: row.get(3)?,
                language: row.get(4)?,
                file_path: row.get(5)?,
                thumb_path: row.get(6)?,
                hash: row.get(7)?,
                width: width.and_then(|v| u32::try_from(v).ok()),
                height: height.and_then(|v| u32::try_from(v).ok()),
                ocr_status: row.get(10)?,
                ocr_text: row.get(11)?,
                ocr_lang: row.get(12)?,
            })
        })
        .map_err(|err| err.to_string())?;

    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(|err| err.to_string())?);
    }
    Ok(out)
}

fn save_step_in(conn: &mut Connection, clip: &Clip, removed_ids: &[i64]) -> Result<(), String> {
    let tx = conn.transaction().map_err(|err| err.to_string())?;
    tx.execute(
        "INSERT INTO clips (
            id, content, created_at, kind, language,
            file_path, thumb_path, hash, width, height,
            ocr_status, ocr_text, ocr_lang
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
        ON CONFLICT(id) DO UPDATE SET
            content = excluded.content,
            created_at = excluded.created_at,
            kind = excluded.kind,
            language = excluded.language,
            file_path = excluded.file_path,
            thumb_path = excluded.thumb_path,
            hash = excluded.hash,
            width = excluded.width,
            height = excluded.height,
            ocr_status = excluded.ocr_status,
            ocr_text = excluded.ocr_text,
            ocr_lang = excluded.ocr_lang",
        params![
            clip.id,
            clip.content,
            clip.created_at,
            clip.kind,
            clip.language,
            clip.file_path,
            clip.thumb_path,
            clip.hash,
            clip.width.map(i64::from),
            clip.height.map(i64::from),
            clip.ocr_status,
            clip.ocr_text,
            clip.ocr_lang,
        ],
    )
    .map_err(|err| err.to_string())?;

    delete_ids(&tx, removed_ids)?;
    tx.commit().map_err(|err| err.to_string())
}

fn touch_clip_in(conn: &Connection, id: i64, created_at: i64) -> Result<(), String> {
    conn.execute(
        "UPDATE clips SET created_at = ?1 WHERE id = ?2",
        params![created_at, id],
    )
    .map_err(|err| err.to_string())?;
    Ok(())
}

fn update_ocr_in(
    conn: &Connection,
    id: i64,
    status: &str,
    text: Option<&str>,
    lang: Option<&str>,
) -> Result<(), String> {
    conn.execute(
        "UPDATE clips SET ocr_status = ?1, ocr_text = ?2, ocr_lang = ?3 WHERE id = ?4",
        params![status, text, lang, id],
    )
    .map_err(|err| err.to_string())?;
    Ok(())
}

fn delete_ids(conn: &Connection, ids: &[i64]) -> Result<(), String> {
    if ids.is_empty() {
        return Ok(());
    }
    let mut stmt = conn
        .prepare_cached("DELETE FROM clips WHERE id = ?1")
        .map_err(|err| err.to_string())?;
    for id in ids {
        stmt.execute(params![id]).map_err(|err| err.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        create_schema, load_and_prune, save_step_in, touch_clip_in, update_ocr_in, Connection,
    };
    use crate::state::Clip;

    fn memory_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        create_schema(&conn).unwrap();
        conn
    }

    #[test]
    fn saves_reorders_and_restores_clips() {
        let mut conn = memory_db();
        let mut first = Clip::text(1, "alpha".into(), 100, "text".into(), None);
        first.hash = Some("h1".into());
        let mut second = Clip::text(2, "const x = 1;".into(), 200, "code".into(), Some("js".into()));
        second.hash = Some("h2".into());

        save_step_in(&mut conn, &first, &[]).unwrap();
        save_step_in(&mut conn, &second, &[]).unwrap();

        let loaded = load_and_prune(&mut conn, 200, false).unwrap();
        assert_eq!(loaded.next_id, 3);
        assert_eq!(loaded.clips.len(), 2);
        assert_eq!(loaded.clips[0].id, 2);
        assert_eq!(loaded.clips[1].id, 1);

        // 第一条再次被复制，更新时间后应排到最前。
        touch_clip_in(&conn, 1, 300).unwrap();
        let loaded = load_and_prune(&mut conn, 200, false).unwrap();
        assert_eq!(loaded.clips[0].id, 1);
        assert_eq!(loaded.clips[0].created_at, 300);
    }

    #[test]
    fn updates_ocr_and_evicts_removed_ids() {
        let mut conn = memory_db();
        let img = Clip {
            id: 5,
            content: String::new(),
            created_at: 500,
            kind: "image".into(),
            language: None,
            file_path: Some("C:/fake/a.png".into()),
            thumb_path: Some("C:/fake/a-thumb.png".into()),
            hash: Some("hash-a".into()),
            width: Some(320),
            height: Some(180),
            ocr_status: "pending".into(),
            ocr_text: None,
            ocr_lang: None,
        };
        save_step_in(&mut conn, &img, &[]).unwrap();
        update_ocr_in(&conn, 5, "done", Some("识别结果"), Some("zh-Hans-CN")).unwrap();

        let loaded = load_and_prune(&mut conn, 200, false).unwrap();
        assert_eq!(loaded.clips.len(), 1);
        assert_eq!(loaded.clips[0].ocr_status, "done");
        assert_eq!(loaded.clips[0].ocr_text.as_deref(), Some("识别结果"));
        assert_eq!(loaded.clips[0].ocr_lang.as_deref(), Some("zh-Hans-CN"));

        let next = Clip::text(6, "newer".into(), 600, "text".into(), None);
        save_step_in(&mut conn, &next, &[5]).unwrap();
        let loaded = load_and_prune(&mut conn, 200, false).unwrap();
        assert_eq!(loaded.clips.len(), 1);
        assert_eq!(loaded.clips[0].id, 6);
        assert_eq!(loaded.next_id, 7);
    }

    #[test]
    fn prunes_missing_image_files_on_startup() {
        let mut conn = memory_db();
        let missing_img = Clip {
            id: 1,
            content: String::new(),
            created_at: 100,
            kind: "image".into(),
            language: None,
            file_path: Some("C:/definitely-not-existing-dogear/missing.png".into()),
            thumb_path: None,
            hash: Some("missing".into()),
            width: Some(100),
            height: Some(100),
            ocr_status: "done".into(),
            ocr_text: Some("old".into()),
            ocr_lang: None,
        };
        let valid_text = Clip::text(2, "keep me".into(), 200, "text".into(), None);
        save_step_in(&mut conn, &missing_img, &[]).unwrap();
        save_step_in(&mut conn, &valid_text, &[]).unwrap();

        let loaded = load_and_prune(&mut conn, 200, true).unwrap();
        assert_eq!(loaded.clips.len(), 1);
        assert_eq!(loaded.clips[0].id, 2);
    }
}
