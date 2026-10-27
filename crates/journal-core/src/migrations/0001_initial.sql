-- Schema v1. Mirrors docs/SPEC.md ("Database schema"). Released migrations are
-- never edited; add 0002_*.sql instead. user_version is set by the migrator.

CREATE TABLE entries (
  rowid         INTEGER PRIMARY KEY,   -- stable integer key for FTS
  id            TEXT NOT NULL UNIQUE,  -- UUID v7, used everywhere outside the DB
  created_at    INTEGER NOT NULL,      -- unix ms, UTC
  updated_at    INTEGER NOT NULL,
  title         TEXT,
  original_text TEXT NOT NULL,         -- never modified after insert
  edited_text   TEXT,                  -- latest edit; NULL = unchanged
  duration_s    INTEGER NOT NULL,
  mood          INTEGER CHECK (mood BETWEEN 1 AND 5),
  has_audio     INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX entries_created_at ON entries(created_at DESC);

CREATE TABLE tags (
  id   INTEGER PRIMARY KEY,
  name TEXT NOT NULL UNIQUE
);

CREATE TABLE entry_tags (
  entry_rowid INTEGER NOT NULL REFERENCES entries(rowid) ON DELETE CASCADE,
  tag_id      INTEGER NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
  PRIMARY KEY (entry_rowid, tag_id)
);

-- Regular (not external-content) FTS table: the duplication is small and
-- avoids the rowid-drift and sync bugs of external content tables.
CREATE VIRTUAL TABLE entries_fts USING fts5(
  title, original_text, edited_text, tags,
  tokenize = 'porter unicode61 remove_diacritics 2'
);

CREATE TRIGGER entries_ai AFTER INSERT ON entries BEGIN
  INSERT INTO entries_fts(rowid, title, original_text, edited_text, tags)
  VALUES (new.rowid, new.title, new.original_text, new.edited_text, '');
END;

CREATE TRIGGER entries_au AFTER UPDATE OF title, edited_text ON entries BEGIN
  UPDATE entries_fts SET title = new.title, edited_text = new.edited_text
  WHERE rowid = new.rowid;
END;

CREATE TRIGGER entries_ad AFTER DELETE ON entries BEGIN
  DELETE FROM entries_fts WHERE rowid = old.rowid;
END;

CREATE TRIGGER entry_tags_ai AFTER INSERT ON entry_tags BEGIN
  UPDATE entries_fts SET tags = (
    SELECT group_concat(t.name, ' ') FROM entry_tags et JOIN tags t ON t.id = et.tag_id
    WHERE et.entry_rowid = new.entry_rowid)
  WHERE rowid = new.entry_rowid;
END;

CREATE TRIGGER entry_tags_ad AFTER DELETE ON entry_tags BEGIN
  UPDATE entries_fts SET tags = coalesce((
    SELECT group_concat(t.name, ' ') FROM entry_tags et JOIN tags t ON t.id = et.tag_id
    WHERE et.entry_rowid = old.entry_rowid), '')
  WHERE rowid = old.entry_rowid;
END;

CREATE TABLE settings (
  key   TEXT PRIMARY KEY,
  value TEXT NOT NULL      -- JSON-encoded value
);
