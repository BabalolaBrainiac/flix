-- A record of each deletion, so a device that missed the delete does not
-- copy the row back on its next sync. The client purges old rows.
CREATE TABLE IF NOT EXISTS deletions (
  invite_code_hash TEXT NOT NULL,
  kind TEXT NOT NULL,
  key TEXT NOT NULL,
  deleted_at INTEGER NOT NULL,
  PRIMARY KEY (invite_code_hash, kind, key)
);

CREATE INDEX IF NOT EXISTS idx_deletions_age ON deletions(deleted_at);
