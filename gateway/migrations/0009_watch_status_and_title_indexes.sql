-- The watch-status board reads one status column at a time
-- (WHERE profile_id = ? AND status = ?); a composite index serves that
-- filter directly. The title index is for local search by name later,
-- never for identity lookups, since two different titles can share a name.
CREATE INDEX IF NOT EXISTS idx_watch_status_profile_status ON watch_status(profile_id, status);
CREATE INDEX IF NOT EXISTS idx_titles_title ON titles(invite_code_hash, title COLLATE NOCASE);
