//! Full-text search over the encrypted FTS5 index (docs/SPEC.md "Search and backup").

use rusqlite::ToSql;
use serde::{Deserialize, Serialize};

use crate::entries::{EntrySummary, SUMMARY_SELECT, summary};
use crate::journal::{Journal, JournalError};

/// Wraps each match in a snippet. Private-use code points, so they can't
/// collide with journal text; the UI turns them into highlights.
pub const MATCH_START: char = '\u{E000}';
pub const MATCH_END: char = '\u{E001}';

#[derive(Debug, Default, Clone, Deserialize)]
pub struct SearchQuery {
    /// Free text. Every word must match, as a prefix, in any stemmed form.
    #[serde(default)]
    pub text: String,
    /// Unix ms, inclusive.
    pub from: Option<i64>,
    /// Unix ms, exclusive.
    pub to: Option<i64>,
    pub mood: Option<u8>,
    pub tag: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SearchHit {
    pub entry: EntrySummary,
    /// Text around the best match, with matches between MATCH_START and
    /// MATCH_END. Empty when the query has no text.
    pub snippet: String,
}

/// Turns user input into a safe FTS5 query: words only, each a quoted prefix
/// term, all required. Operators and quotes in the input are just ignored.
fn fts_query(text: &str) -> Option<String> {
    let terms: Vec<String> = text
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(|w| format!("\"{w}\"*"))
        .collect();
    (!terms.is_empty()).then(|| terms.join(" "))
}

impl Journal {
    /// Matching entries, newest first.
    pub fn search(&self, q: &SearchQuery) -> Result<Vec<SearchHit>, JournalError> {
        let mut params: Vec<Box<dyn ToSql>> = Vec::new();
        let mut filters = Vec::new();
        let (from, snippet) = match fts_query(&q.text) {
            Some(fts) => {
                params.push(Box::new(fts));
                filters.push(format!("entries_fts MATCH ?{}", params.len()));
                (
                    "entries_fts JOIN entries e ON e.rowid = entries_fts.rowid",
                    format!("snippet(entries_fts, -1, '{MATCH_START}', '{MATCH_END}', '…', 16)"),
                )
            }
            None => ("entries e", "''".to_owned()),
        };
        if let Some(from) = q.from {
            params.push(Box::new(from));
            filters.push(format!("e.created_at >= ?{}", params.len()));
        }
        if let Some(to) = q.to {
            params.push(Box::new(to));
            filters.push(format!("e.created_at < ?{}", params.len()));
        }
        if let Some(mood) = q.mood {
            params.push(Box::new(mood));
            filters.push(format!("e.mood = ?{}", params.len()));
        }
        if let Some(tag) = q.tag.as_deref().map(|t| t.trim().to_lowercase()).filter(|t| !t.is_empty()) {
            params.push(Box::new(tag));
            filters.push(format!(
                "EXISTS (SELECT 1 FROM entry_tags et JOIN tags t ON t.id = et.tag_id WHERE et.entry_rowid = e.rowid AND t.name = ?{})",
                params.len()
            ));
        }
        let filters = if filters.is_empty() { String::new() } else { format!("WHERE {}", filters.join(" AND ")) };

        let sql = format!("SELECT {SUMMARY_SELECT}, {snippet} FROM {from} {filters} ORDER BY e.created_at DESC");
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(params.iter()), |r| {
            Ok((r.get::<_, i64>(0)?, summary(r)?, r.get::<_, String>(8)?))
        })?;
        rows.map(|row| {
            let (rowid, mut entry, snippet) = row?;
            entry.tags = self.tags_of(rowid)?;
            Ok(SearchHit { entry, snippet })
        })
        .collect()
    }
}
