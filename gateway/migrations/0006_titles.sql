-- One row of display data per title and invite. Lists and watch status keep
-- only the catalog id and read the title, poster, and year from here.
CREATE TABLE IF NOT EXISTS titles (
  invite_code_hash TEXT NOT NULL,
  catalog_id TEXT NOT NULL,
  canonical_id TEXT,
  media_type TEXT,
  title TEXT,
  poster TEXT,
  year INTEGER,
  updated_at INTEGER NOT NULL,
  PRIMARY KEY (invite_code_hash, catalog_id)
);

CREATE INDEX IF NOT EXISTS idx_titles_canonical ON titles(invite_code_hash, canonical_id);

INSERT INTO titles (invite_code_hash, catalog_id, media_type, title, poster, updated_at)
SELECT p.invite_code_hash, li.catalog_id, li.media_type, li.title, li.poster, li.added_at
FROM list_items li
JOIN lists l ON l.id = li.list_id
JOIN profiles p ON p.id = l.profile_id
WHERE true
ON CONFLICT(invite_code_hash, catalog_id) DO UPDATE SET
  title = COALESCE(titles.title, excluded.title),
  poster = COALESCE(titles.poster, excluded.poster);

INSERT INTO titles (invite_code_hash, catalog_id, media_type, title, poster, updated_at)
SELECT p.invite_code_hash, ws.catalog_id, ws.media_type, ws.title, ws.poster, ws.updated_at
FROM watch_status ws
JOIN profiles p ON p.id = ws.profile_id
WHERE true
ON CONFLICT(invite_code_hash, catalog_id) DO UPDATE SET
  media_type = COALESCE(titles.media_type, excluded.media_type),
  title = COALESCE(titles.title, excluded.title),
  poster = COALESCE(titles.poster, excluded.poster);

ALTER TABLE list_items DROP COLUMN title;
ALTER TABLE list_items DROP COLUMN poster;
ALTER TABLE watch_status DROP COLUMN title;
ALTER TABLE watch_status DROP COLUMN poster;
ALTER TABLE watch_status DROP COLUMN media_type;
