//! Journal entries (docs/SPEC.md "Entries"). Everything here is journal text:
//! never log it.

use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{OptionalExtension, Row, params};
use serde::Serialize;

use crate::journal::{Journal, JournalError};

/// New entry IDs are UUID v7: time-ordered, so they sort like `created_at`.
pub fn new_entry_id() -> String {
    uuid::Uuid::now_v7().to_string()
}

pub struct NewEntry {
    pub id: String,
    /// Unix ms, UTC.
    pub created_at: i64,
    /// The transcript (or typed text) as first saved; becomes `original_text`.
    pub text: String,
    pub duration_s: u32,
    pub has_audio: bool,
    pub mood: Option<u8>,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EntrySummary {
    pub id: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub title: Option<String>,
    pub duration_s: u32,
    pub mood: Option<u8>,
    pub has_audio: bool,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Entry {
    #[serde(flatten)]
    pub summary: EntrySummary,
    pub original_text: String,
    /// The latest edit; None means unchanged.
    pub edited_text: Option<String>,
}

const SUMMARY_COLUMNS: &str = "e.rowid, e.id, e.created_at, e.updated_at, e.title, e.duration_s, e.mood, e.has_audio";

pub fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as i64)
}

/// The default title: the first six words of the text.
fn default_title(text: &str) -> Option<String> {
    let words: Vec<&str> = text.split_whitespace().take(6).collect();
    (!words.is_empty()).then(|| words.join(" "))
}

fn normalize_tags(tags: &[String]) -> Vec<String> {
    let mut out: Vec<String> = tags.iter().map(|t| t.trim().to_lowercase()).filter(|t| !t.is_empty()).collect();
    out.sort();
    out.dedup();
    out
}

fn check_mood(mood: Option<u8>) -> Result<(), JournalError> {
    match mood {
        Some(m) if !(1..=5).contains(&m) => Err(JournalError::Invalid(format!("mood must be 1–5, got {m}"))),
        _ => Ok(()),
    }
}

impl Journal {
    pub fn create_entry(&self, e: &NewEntry) -> Result<(), JournalError> {
        check_mood(e.mood)?;
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "INSERT INTO entries (id, created_at, updated_at, title, original_text, duration_s, mood, has_audio)
             VALUES (?1, ?2, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![e.id, e.created_at, default_title(&e.text), e.text, e.duration_s, e.mood, e.has_audio],
        )?;
        let rowid = tx.last_insert_rowid();
        self.write_tags(rowid, &normalize_tags(&e.tags))?;
        tx.commit()?;
        Ok(())
    }

    /// All entries, newest first.
    pub fn list_entries(&self) -> Result<Vec<EntrySummary>, JournalError> {
        let mut stmt = self.conn.prepare(&format!("SELECT {SUMMARY_COLUMNS} FROM entries e ORDER BY e.created_at DESC"))?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, summary(r)?)))?;
        rows.map(|row| {
            let (rowid, mut s) = row?;
            s.tags = self.tags_of(rowid)?;
            Ok(s)
        })
        .collect()
    }

    pub fn get_entry(&self, id: &str) -> Result<Option<Entry>, JournalError> {
        let found = self
            .conn
            .query_row(
                &format!("SELECT {SUMMARY_COLUMNS}, e.original_text, e.edited_text FROM entries e WHERE e.id = ?1"),
                [id],
                |r| Ok((r.get::<_, i64>(0)?, summary(r)?, r.get(8)?, r.get(9)?)),
            )
            .optional()?;
        let Some((rowid, mut summary, original_text, edited_text)) = found else { return Ok(None) };
        summary.tags = self.tags_of(rowid)?;
        Ok(Some(Entry { summary, original_text, edited_text }))
    }

    /// None or blank clears the title (the UI then shows "Untitled").
    pub fn set_title(&self, id: &str, title: Option<&str>) -> Result<(), JournalError> {
        let title = title.map(str::trim).filter(|t| !t.is_empty());
        self.update("title = ?2", params![id, title])
    }

    /// Stores the latest edit. Text identical to the original clears the edit.
    pub fn set_edited_text(&self, id: &str, text: &str) -> Result<(), JournalError> {
        self.update("edited_text = NULLIF(?2, original_text)", params![id, text])
    }

    /// "Revert to original".
    pub fn revert_entry(&self, id: &str) -> Result<(), JournalError> {
        self.update("edited_text = NULL", params![id])
    }

    pub fn set_mood(&self, id: &str, mood: Option<u8>) -> Result<(), JournalError> {
        check_mood(mood)?;
        self.update("mood = ?2", params![id, mood])
    }

    pub fn set_has_audio(&self, id: &str, has_audio: bool) -> Result<(), JournalError> {
        self.update("has_audio = ?2", params![id, has_audio])
    }

    /// Replaces the entry's tags.
    pub fn set_tags(&self, id: &str, tags: &[String]) -> Result<(), JournalError> {
        let tx = self.conn.unchecked_transaction()?;
        let rowid = self.rowid(id)?;
        tx.execute("DELETE FROM entry_tags WHERE entry_rowid = ?1", [rowid])?;
        self.write_tags(rowid, &normalize_tags(tags))?;
        tx.execute("UPDATE entries SET updated_at = ?2 WHERE rowid = ?1", params![rowid, now_ms()])?;
        self.drop_unused_tags()?;
        tx.commit()?;
        Ok(())
    }

    /// Returns whether the entry existed. The caller deletes its audio file.
    pub fn delete_entry(&self, id: &str) -> Result<bool, JournalError> {
        let tx = self.conn.unchecked_transaction()?;
        let deleted = tx.execute("DELETE FROM entries WHERE id = ?1", [id])? > 0;
        self.drop_unused_tags()?;
        tx.commit()?;
        Ok(deleted)
    }

    /// Every tag in use, alphabetically (for autocomplete and filters).
    pub fn all_tags(&self) -> Result<Vec<String>, JournalError> {
        let mut stmt = self.conn.prepare("SELECT name FROM tags ORDER BY name")?;
        let tags = stmt.query_map([], |r| r.get(0))?.collect::<Result<_, _>>()?;
        Ok(tags)
    }

    pub(crate) fn tags_of(&self, rowid: i64) -> Result<Vec<String>, JournalError> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT t.name FROM entry_tags et JOIN tags t ON t.id = et.tag_id WHERE et.entry_rowid = ?1 ORDER BY t.name",
        )?;
        let tags = stmt.query_map([rowid], |r| r.get(0))?.collect::<Result<_, _>>()?;
        Ok(tags)
    }

    fn update(&self, set: &str, params: &[&dyn rusqlite::ToSql]) -> Result<(), JournalError> {
        let sql = format!("UPDATE entries SET {set}, updated_at = {} WHERE id = ?1", now_ms());
        if self.conn.execute(&sql, params)? == 0 {
            return Err(JournalError::NotFound);
        }
        Ok(())
    }

    fn rowid(&self, id: &str) -> Result<i64, JournalError> {
        self.conn
            .query_row("SELECT rowid FROM entries WHERE id = ?1", [id], |r| r.get(0))
            .optional()?
            .ok_or(JournalError::NotFound)
    }

    fn write_tags(&self, rowid: i64, tags: &[String]) -> Result<(), JournalError> {
        for tag in tags {
            self.conn.execute("INSERT OR IGNORE INTO tags (name) VALUES (?1)", [tag])?;
            self.conn.execute(
                "INSERT OR IGNORE INTO entry_tags (entry_rowid, tag_id) SELECT ?1, id FROM tags WHERE name = ?2",
                params![rowid, tag],
            )?;
        }
        Ok(())
    }

    fn drop_unused_tags(&self) -> Result<(), JournalError> {
        self.conn.execute("DELETE FROM tags WHERE id NOT IN (SELECT tag_id FROM entry_tags)", [])?;
        Ok(())
    }
}

/// Columns in `SUMMARY_COLUMNS` order, starting at index 1 (0 is the rowid).
pub(crate) fn summary(r: &Row<'_>) -> rusqlite::Result<EntrySummary> {
    Ok(EntrySummary {
        id: r.get(1)?,
        created_at: r.get(2)?,
        updated_at: r.get(3)?,
        title: r.get(4)?,
        duration_s: r.get(5)?,
        mood: r.get(6)?,
        has_audio: r.get(7)?,
        tags: Vec::new(),
    })
}

pub(crate) const SUMMARY_SELECT: &str = SUMMARY_COLUMNS;
