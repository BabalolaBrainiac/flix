CREATE TABLE IF NOT EXISTS profiles (
  id TEXT PRIMARY KEY,
  invite_code_hash TEXT NOT NULL,
  name TEXT NOT NULL,
  avatar_key TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  FOREIGN KEY (invite_code_hash) REFERENCES invites(code_hash)
);

CREATE TABLE IF NOT EXISTS lists (
  id TEXT PRIMARY KEY,
  profile_id TEXT NOT NULL,
  name TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  FOREIGN KEY (profile_id) REFERENCES profiles(id)
);

CREATE TABLE IF NOT EXISTS list_items (
  list_id TEXT NOT NULL,
  catalog_id TEXT NOT NULL,
  media_type TEXT NOT NULL,
  added_at INTEGER NOT NULL,
  PRIMARY KEY (list_id, catalog_id),
  FOREIGN KEY (list_id) REFERENCES lists(id)
);

CREATE TABLE IF NOT EXISTS watch_status (
  profile_id TEXT NOT NULL,
  catalog_id TEXT NOT NULL,
  status TEXT NOT NULL,
  episode_id TEXT,
  resume_seconds INTEGER,
  updated_at INTEGER NOT NULL,
  PRIMARY KEY (profile_id, catalog_id),
  FOREIGN KEY (profile_id) REFERENCES profiles(id)
);

CREATE INDEX IF NOT EXISTS idx_profiles_invite ON profiles(invite_code_hash);
CREATE INDEX IF NOT EXISTS idx_lists_profile ON lists(profile_id);
CREATE INDEX IF NOT EXISTS idx_list_items_list ON list_items(list_id);
CREATE INDEX IF NOT EXISTS idx_watch_status_profile ON watch_status(profile_id);
