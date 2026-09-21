use anyhow::{anyhow, Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

const SCHEMA_VERSION: i32 = 3;
const DELETION_RETENTION_MS: i64 = 90 * 24 * 60 * 60 * 1000;

pub const KIND_PROFILE: &str = "profile";
pub const KIND_LIST: &str = "list";
pub const KIND_LIST_ITEM: &str = "list_item";
pub const KIND_WATCH_STATUS: &str = "watch_status";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub avatar_key: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListRecord {
    pub id: String,
    pub profile_id: String,
    pub name: String,
    pub created_at: i64,
}

/// Display data for one title, stored once in the `titles` table and shared
/// by every list item and watch status that points at the same catalog id.
/// Every field is optional: a `None` never overwrites a stored value.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TitleMeta<'a> {
    pub media_type: Option<&'a str>,
    pub title: Option<&'a str>,
    pub poster: Option<&'a str>,
    pub year: Option<i64>,
    /// The id that identifies the same title across catalogs (for example the
    /// IMDb id of a `kitsu:` anime). Used to stop one title appearing twice.
    pub canonical_id: Option<&'a str>,
}

impl TitleMeta<'_> {
    fn is_empty(&self) -> bool {
        self.media_type.is_none()
            && self.title.is_none()
            && self.poster.is_none()
            && self.year.is_none()
            && self.canonical_id.is_none()
    }
}

/// A title record retrieved from the local titles table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredTitle {
    pub catalog_id: String,
    pub canonical_id: Option<String>,
    pub media_type: Option<String>,
    pub title: Option<String>,
    pub poster: Option<String>,
    pub year: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListItem {
    pub list_id: String,
    pub catalog_id: String,
    pub media_type: String,
    pub added_at: i64,
    pub title: Option<String>,
    pub poster: Option<String>,
    pub year: Option<i64>,
    pub canonical_id: Option<String>,
}

/// A title's watch state for one profile. Distinct from list membership:
/// a title can carry a status with no list ever containing it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WatchState {
    ToWatch,
    InProgress,
    Watched,
}

impl WatchState {
    pub fn as_str(self) -> &'static str {
        match self {
            WatchState::ToWatch => "to_watch",
            WatchState::InProgress => "in_progress",
            WatchState::Watched => "watched",
        }
    }

    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "to_watch" => Ok(WatchState::ToWatch),
            "in_progress" => Ok(WatchState::InProgress),
            "watched" => Ok(WatchState::Watched),
            other => Err(anyhow!("Unknown watch status: {other}")),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WatchStatus {
    pub profile_id: String,
    pub catalog_id: String,
    pub status: WatchState,
    pub episode_id: Option<String>,
    pub resume_seconds: Option<i64>,
    pub updated_at: i64,
    pub title: Option<String>,
    pub poster: Option<String>,
    pub media_type: Option<String>,
    pub year: Option<i64>,
    pub canonical_id: Option<String>,
}

/// A record that something was deleted. Sync needs it: without one, a row
/// that one device removed would be copied back from another device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Deletion {
    pub kind: String,
    pub key: String,
    pub deleted_at: i64,
}

pub fn list_item_key(list_id: &str, catalog_id: &str) -> String {
    format!("{list_id}:{catalog_id}")
}

pub fn watch_status_key(profile_id: &str, catalog_id: &str) -> String {
    format!("{profile_id}:{catalog_id}")
}

/// Milliseconds since the epoch, matching the gateway's `Date.now()` - every
/// timestamp in this store must stay in the same unit as the gateway's, or
/// last-write-wins sync would compare seconds against milliseconds.
fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

fn is_unique_violation(error: &rusqlite::Error) -> bool {
    matches!(
        error,
        rusqlite::Error::SqliteFailure(failure, _)
            if failure.code == rusqlite::ErrorCode::ConstraintViolation
    )
}

fn duplicate_list_error(name: &str) -> anyhow::Error {
    anyhow!("A list named \"{name}\" already exists")
}

/// Local, per-invite store for profiles, lists, and watch status. Backed by
/// SQLite because this data is relational and grows over the life of the app.
///
/// The connection is behind a `Mutex`: `rusqlite::Connection` is `Send` but
/// not `Sync`, so a bare `Connection` field would stop `Arc<ProfileStore>`
/// from moving into `tokio::task::spawn_blocking`.
pub struct ProfileStore {
    conn: Mutex<Connection>,
}

impl ProfileStore {
    /// Opens (creating if absent) `profiles.sqlite3` in `dir`, in WAL mode.
    pub fn open(dir: &Path) -> Result<Self> {
        std::fs::create_dir_all(dir).context("Failed to create app data directory")?;
        let path = dir.join("profiles.sqlite3");
        let raw = Connection::open(&path)
            .with_context(|| format!("Failed to open {}", path.display()))?;
        raw.query_row("PRAGMA journal_mode = WAL", [], |row| {
            row.get::<_, String>(0)
        })
        .context("Failed to enable WAL mode")?;
        raw.execute_batch(
            "PRAGMA synchronous = NORMAL;
             PRAGMA temp_store = MEMORY;
             PRAGMA cache_size = -2000;",
        )
        .context("Failed to configure database performance PRAGMAs")?;
        raw.pragma_update(None, "foreign_keys", true)
            .context("Failed to enable foreign key enforcement")?;
        let store = Self {
            conn: Mutex::new(raw),
        };
        store.migrate()?;
        store.purge_old_deletions()?;
        Ok(store)
    }

    /// A poisoned mutex only happens after a panic mid-statement; SQLite, not
    /// Rust state, is the source of truth, so recovering the guard is safe.
    fn conn(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.conn
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn migrate(&self) -> Result<()> {
        let conn = self.conn();
        conn.execute_batch("CREATE TABLE IF NOT EXISTS schema_version (version INTEGER NOT NULL);")
            .context("Failed to create schema_version table")?;
        let current: i32 = conn
            .query_row("SELECT version FROM schema_version LIMIT 1", [], |row| {
                row.get(0)
            })
            .optional()
            .context("Failed to read schema_version")?
            .unwrap_or(0);

        if current < 1 {
            conn.execute_batch(
                "CREATE TABLE profiles (
                    id TEXT PRIMARY KEY,
                    name TEXT NOT NULL,
                    avatar_key TEXT NOT NULL,
                    created_at INTEGER NOT NULL,
                    updated_at INTEGER NOT NULL
                );
                CREATE TABLE lists (
                    id TEXT PRIMARY KEY,
                    profile_id TEXT NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
                    name TEXT NOT NULL,
                    created_at INTEGER NOT NULL
                );
                CREATE INDEX idx_lists_profile ON lists(profile_id);
                CREATE TABLE list_items (
                    list_id TEXT NOT NULL REFERENCES lists(id) ON DELETE CASCADE,
                    catalog_id TEXT NOT NULL,
                    media_type TEXT NOT NULL,
                    added_at INTEGER NOT NULL,
                    PRIMARY KEY (list_id, catalog_id)
                );
                CREATE TABLE watch_status (
                    profile_id TEXT NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
                    catalog_id TEXT NOT NULL,
                    status TEXT NOT NULL,
                    episode_id TEXT,
                    resume_seconds INTEGER,
                    updated_at INTEGER NOT NULL,
                    PRIMARY KEY (profile_id, catalog_id)
                );
                CREATE INDEX idx_watch_status_profile ON watch_status(profile_id);",
            )
            .context("Failed to create profiles schema")?;
        }

        if current < 2 {
            // Titles hold display data once. Deletions let sync tell "removed
            // here" apart from "never existed here". The name index makes a
            // duplicate list name impossible, even from two offline devices.
            conn.execute_batch(
                "CREATE TABLE titles (
                    catalog_id TEXT PRIMARY KEY,
                    canonical_id TEXT,
                    media_type TEXT,
                    title TEXT,
                    poster TEXT,
                    year INTEGER,
                    updated_at INTEGER NOT NULL
                );
                CREATE INDEX idx_titles_canonical ON titles(canonical_id);
                INSERT OR IGNORE INTO titles (catalog_id, media_type, updated_at)
                    SELECT catalog_id, media_type, added_at FROM list_items;
                CREATE TABLE deletions (
                    kind TEXT NOT NULL,
                    key TEXT NOT NULL,
                    deleted_at INTEGER NOT NULL,
                    PRIMARY KEY (kind, key)
                );
                UPDATE lists SET name = name || ' (' || substr(id, 1, 4) || ')'
                    WHERE EXISTS (
                        SELECT 1 FROM lists other
                        WHERE other.profile_id = lists.profile_id
                          AND lower(other.name) = lower(lists.name)
                          AND (other.created_at < lists.created_at
                               OR (other.created_at = lists.created_at AND other.id < lists.id))
                    );
                CREATE UNIQUE INDEX idx_lists_profile_name
                    ON lists(profile_id, name COLLATE NOCASE);",
            )
            .context("Failed to migrate profiles schema to version 2")?;
        }

        if current < 3 {
            // The watch-status board reads one status column at a time
            // (`WHERE profile_id = ? AND status = ?`); a composite index
            // serves that filter directly instead of scanning every status
            // row of the profile. The title index is for local, offline
            // search by name later - never for identity lookups, since two
            // different titles can share a name (for example two different
            // films both called "Heat").
            conn.execute_batch(
                "CREATE INDEX idx_watch_status_profile_status ON watch_status(profile_id, status);
                CREATE INDEX idx_titles_title ON titles(title COLLATE NOCASE);",
            )
            .context("Failed to migrate profiles schema to version 3")?;
        }

        conn.execute("DELETE FROM schema_version", [])
            .context("Failed to clear schema_version")?;
        conn.execute(
            "INSERT INTO schema_version (version) VALUES (?1)",
            params![SCHEMA_VERSION],
        )
        .context("Failed to record schema_version")?;
        Ok(())
    }

    fn purge_old_deletions(&self) -> Result<()> {
        self.conn()
            .execute(
                "DELETE FROM deletions WHERE deleted_at < ?1",
                params![now() - DELETION_RETENTION_MS],
            )
            .context("Failed to purge old deletion records")?;
        Ok(())
    }

    // ---- Shared helpers (take the connection so callers can group writes) ----

    fn record_deletion(conn: &Connection, kind: &str, key: &str, at: i64) -> Result<()> {
        conn.execute(
            "INSERT INTO deletions (kind, key, deleted_at) VALUES (?1, ?2, ?3)
             ON CONFLICT(kind, key) DO UPDATE SET deleted_at = excluded.deleted_at",
            params![kind, key, at],
        )
        .context("Failed to record deletion")?;
        Ok(())
    }

    fn clear_deletion(conn: &Connection, kind: &str, key: &str) -> Result<()> {
        conn.execute(
            "DELETE FROM deletions WHERE kind = ?1 AND key = ?2",
            params![kind, key],
        )
        .context("Failed to clear deletion record")?;
        Ok(())
    }

    fn upsert_title(conn: &Connection, catalog_id: &str, meta: &TitleMeta) -> Result<()> {
        if meta.is_empty() {
            return Ok(());
        }
        conn.execute(
            "INSERT INTO titles (catalog_id, canonical_id, media_type, title, poster, year, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(catalog_id) DO UPDATE SET
                canonical_id = COALESCE(excluded.canonical_id, titles.canonical_id),
                media_type = COALESCE(excluded.media_type, titles.media_type),
                title = COALESCE(excluded.title, titles.title),
                poster = COALESCE(excluded.poster, titles.poster),
                year = COALESCE(excluded.year, titles.year),
                updated_at = excluded.updated_at",
            params![
                catalog_id,
                meta.canonical_id,
                meta.media_type,
                meta.title,
                meta.poster,
                meta.year,
                now()
            ],
        )
        .context("Failed to store title data")?;
        Ok(())
    }

    /// Saves title metadata to the titles table.
    pub fn save_title(&self, catalog_id: &str, meta: &TitleMeta) -> Result<()> {
        Self::upsert_title(&self.conn(), catalog_id, meta)
    }

    /// Reads a title record by its catalog identifier.
    pub fn get_title(&self, catalog_id: &str) -> Result<Option<StoredTitle>> {
        let conn = self.conn();
        conn.query_row(
            "SELECT catalog_id, canonical_id, media_type, title, poster, year
             FROM titles WHERE catalog_id = ?1",
            params![catalog_id],
            |row| {
                Ok(StoredTitle {
                    catalog_id: row.get(0)?,
                    canonical_id: row.get(1)?,
                    media_type: row.get(2)?,
                    title: row.get(3)?,
                    poster: row.get(4)?,
                    year: row.get(5)?,
                })
            },
        )
        .optional()
        .context("Failed to query title by catalog id")
    }

    /// Finds titles with names that match the query, ignoring case.
    pub fn find_titles_by_name(&self, title: &str) -> Result<Vec<StoredTitle>> {
        let conn = self.conn();
        let mut stmt = conn
            .prepare(
                "SELECT catalog_id, canonical_id, media_type, title, poster, year
                 FROM titles WHERE title = ?1 COLLATE NOCASE",
            )
            .context("Failed to prepare title search query")?;
        let rows = stmt
            .query_map(params![title], |row| {
                Ok(StoredTitle {
                    catalog_id: row.get(0)?,
                    canonical_id: row.get(1)?,
                    media_type: row.get(2)?,
                    title: row.get(3)?,
                    poster: row.get(4)?,
                    year: row.get(5)?,
                })
            })
            .context("Failed to execute title search")?;
        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        Ok(results)
    }

    // ---- Profiles ----

    pub fn create_profile(&self, name: &str, avatar_key: &str) -> Result<Profile> {
        let profile = Profile {
            id: Uuid::new_v4().to_string(),
            name: name.to_string(),
            avatar_key: avatar_key.to_string(),
            created_at: now(),
            updated_at: now(),
        };
        self.conn()
            .execute(
                "INSERT INTO profiles (id, name, avatar_key, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    profile.id,
                    profile.name,
                    profile.avatar_key,
                    profile.created_at,
                    profile.updated_at
                ],
            )
            .context("Failed to insert profile")?;
        Ok(profile)
    }

    /// Inserts a profile with a caller-given id, or updates name/avatar/
    /// `updated_at` in place if that id already exists - `created_at` is
    /// never overwritten. Used by gateway sync so a profile keeps one id on
    /// every device.
    pub fn upsert_profile(
        &self,
        id: &str,
        name: &str,
        avatar_key: &str,
        created_at: i64,
        updated_at: i64,
    ) -> Result<()> {
        let conn = self.conn();
        conn.execute(
            "INSERT INTO profiles (id, name, avatar_key, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                avatar_key = excluded.avatar_key,
                updated_at = excluded.updated_at",
            params![id, name, avatar_key, created_at, updated_at],
        )
        .context("Failed to upsert profile")?;
        Self::clear_deletion(&conn, KIND_PROFILE, id)
    }

    pub fn list_profiles(&self) -> Result<Vec<Profile>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT id, name, avatar_key, created_at, updated_at FROM profiles ORDER BY created_at ASC",
        )?;
        let rows = stmt
            .query_map([], |row| {
                Ok(Profile {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    avatar_key: row.get(2)?,
                    created_at: row.get(3)?,
                    updated_at: row.get(4)?,
                })
            })
            .context("Failed to query profiles")?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .context("Failed to read profile row")
    }

    /// Renames a profile, changes its avatar, or both. Passing `None` for a
    /// field leaves it unchanged.
    pub fn update_profile(
        &self,
        id: &str,
        name: Option<&str>,
        avatar_key: Option<&str>,
    ) -> Result<()> {
        if name.is_none() && avatar_key.is_none() {
            return Ok(());
        }
        let changed = self
            .conn()
            .execute(
                "UPDATE profiles SET
                    name = COALESCE(?2, name),
                    avatar_key = COALESCE(?3, avatar_key),
                    updated_at = ?4
                 WHERE id = ?1",
                params![id, name, avatar_key, now()],
            )
            .context("Failed to update profile")?;
        if changed == 0 {
            return Err(anyhow!("No profile with id {id}"));
        }
        Ok(())
    }

    /// Deletes a profile and, via `ON DELETE CASCADE`, its lists, list
    /// items, and watch status. Records the deletion for sync.
    pub fn delete_profile(&self, id: &str) -> Result<()> {
        let conn = self.conn();
        let changed = conn
            .execute("DELETE FROM profiles WHERE id = ?1", params![id])
            .context("Failed to delete profile")?;
        if changed > 0 {
            Self::record_deletion(&conn, KIND_PROFILE, id, now())?;
        }
        Ok(())
    }

    // ---- Lists ----

    pub fn create_list(&self, profile_id: &str, name: &str) -> Result<ListRecord> {
        let list = ListRecord {
            id: Uuid::new_v4().to_string(),
            profile_id: profile_id.to_string(),
            name: name.to_string(),
            created_at: now(),
        };
        let conn = self.conn();
        conn.execute(
            "INSERT INTO lists (id, profile_id, name, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![list.id, list.profile_id, list.name, list.created_at],
        )
        .map_err(|error| {
            if is_unique_violation(&error) {
                duplicate_list_error(name)
            } else {
                anyhow::Error::new(error).context("Failed to insert list")
            }
        })?;
        Ok(list)
    }

    /// Inserts a list with a caller-given id, or updates its name in place.
    /// Used by gateway sync to pull a remote list with its id preserved. The
    /// caller must settle a name collision with another list first.
    pub fn upsert_list(
        &self,
        id: &str,
        profile_id: &str,
        name: &str,
        created_at: i64,
    ) -> Result<()> {
        let conn = self.conn();
        conn.execute(
            "INSERT INTO lists (id, profile_id, name, created_at)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(id) DO UPDATE SET name = excluded.name",
            params![id, profile_id, name, created_at],
        )
        .map_err(|error| {
            if is_unique_violation(&error) {
                duplicate_list_error(name)
            } else {
                anyhow::Error::new(error).context("Failed to upsert list")
            }
        })?;
        Self::clear_deletion(&conn, KIND_LIST, id)
    }

    pub fn find_list_by_name(&self, profile_id: &str, name: &str) -> Result<Option<ListRecord>> {
        self.conn()
            .query_row(
                "SELECT id, profile_id, name, created_at FROM lists
                 WHERE profile_id = ?1 AND name = ?2 COLLATE NOCASE",
                params![profile_id, name],
                |row| {
                    Ok(ListRecord {
                        id: row.get(0)?,
                        profile_id: row.get(1)?,
                        name: row.get(2)?,
                        created_at: row.get(3)?,
                    })
                },
            )
            .optional()
            .context("Failed to look up list by name")
    }

    pub fn list_lists(&self, profile_id: &str) -> Result<Vec<ListRecord>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT id, profile_id, name, created_at FROM lists WHERE profile_id = ?1 ORDER BY created_at ASC",
        )?;
        let rows = stmt
            .query_map(params![profile_id], |row| {
                Ok(ListRecord {
                    id: row.get(0)?,
                    profile_id: row.get(1)?,
                    name: row.get(2)?,
                    created_at: row.get(3)?,
                })
            })
            .context("Failed to query lists")?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .context("Failed to read list row")
    }

    /// Deletes a list and, via `ON DELETE CASCADE`, its items. Records the
    /// deletion for sync.
    pub fn delete_list(&self, list_id: &str) -> Result<()> {
        let conn = self.conn();
        let changed = conn
            .execute("DELETE FROM lists WHERE id = ?1", params![list_id])
            .context("Failed to delete list")?;
        if changed > 0 {
            Self::record_deletion(&conn, KIND_LIST, list_id, now())?;
        }
        Ok(())
    }

    /// Adds a title to a list. Adding a title that is already there, under
    /// the same id or under an alias id for the same title, changes nothing.
    pub fn add_list_item(
        &self,
        list_id: &str,
        catalog_id: &str,
        media_type: &str,
        meta: &TitleMeta,
    ) -> Result<()> {
        self.write_list_item(list_id, catalog_id, media_type, now(), meta, true)
    }

    /// Inserts or updates a list item, keeping the given `added_at`. Used by
    /// gateway sync to pull remote list items.
    pub fn upsert_list_item(
        &self,
        list_id: &str,
        catalog_id: &str,
        media_type: &str,
        added_at: i64,
        meta: &TitleMeta,
    ) -> Result<()> {
        self.write_list_item(list_id, catalog_id, media_type, added_at, meta, false)
    }

    fn write_list_item(
        &self,
        list_id: &str,
        catalog_id: &str,
        media_type: &str,
        added_at: i64,
        meta: &TitleMeta,
        skip_existing: bool,
    ) -> Result<()> {
        let conn = self.conn();
        let tx = conn.unchecked_transaction()?;
        let canonical = meta.canonical_id.unwrap_or(catalog_id);
        let alias_present: bool = tx
            .query_row(
                "SELECT EXISTS(
                    SELECT 1 FROM list_items li
                    LEFT JOIN titles t ON t.catalog_id = li.catalog_id
                    WHERE li.list_id = ?1 AND li.catalog_id != ?2
                      AND COALESCE(t.canonical_id, li.catalog_id) = ?3)",
                params![list_id, catalog_id, canonical],
                |row| row.get(0),
            )
            .context("Failed to check for an alias list item")?;
        if alias_present {
            return Ok(());
        }
        Self::upsert_title(&tx, catalog_id, meta)?;
        let conflict = if skip_existing {
            "DO NOTHING"
        } else {
            "DO UPDATE SET media_type = excluded.media_type, added_at = excluded.added_at"
        };
        tx.execute(
            &format!(
                "INSERT INTO list_items (list_id, catalog_id, media_type, added_at)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(list_id, catalog_id) {conflict}"
            ),
            params![list_id, catalog_id, media_type, added_at],
        )
        .context("Failed to write list item")?;
        Self::clear_deletion(&tx, KIND_LIST_ITEM, &list_item_key(list_id, catalog_id))?;
        tx.commit().context("Failed to commit list item")
    }

    pub fn remove_list_item(&self, list_id: &str, catalog_id: &str) -> Result<()> {
        let conn = self.conn();
        let changed = conn
            .execute(
                "DELETE FROM list_items WHERE list_id = ?1 AND catalog_id = ?2",
                params![list_id, catalog_id],
            )
            .context("Failed to remove list item")?;
        if changed > 0 {
            Self::record_deletion(
                &conn,
                KIND_LIST_ITEM,
                &list_item_key(list_id, catalog_id),
                now(),
            )?;
        }
        Ok(())
    }

    /// Replaces a list with another list that has the same name and a
    /// different id. The old list's items move to the new list and the old
    /// list is deleted, all in one transaction. Used when sync merges two
    /// lists that two devices made with one name.
    pub fn replace_list(
        &self,
        old_id: &str,
        new_id: &str,
        profile_id: &str,
        name: &str,
        created_at: i64,
    ) -> Result<()> {
        let conn = self.conn();
        let tx = conn.unchecked_transaction()?;
        let items: Vec<(String, String, i64)> = {
            let mut stmt = tx.prepare(
                "SELECT catalog_id, media_type, added_at FROM list_items WHERE list_id = ?1",
            )?;
            let rows = stmt.query_map(params![old_id], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })?;
            rows.collect::<rusqlite::Result<Vec<_>>>()?
        };
        tx.execute("DELETE FROM lists WHERE id = ?1", params![old_id])
            .context("Failed to delete the replaced list")?;
        Self::record_deletion(&tx, KIND_LIST, old_id, now())?;
        tx.execute(
            "INSERT INTO lists (id, profile_id, name, created_at) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(id) DO UPDATE SET name = excluded.name",
            params![new_id, profile_id, name, created_at],
        )
        .context("Failed to write the replacing list")?;
        Self::clear_deletion(&tx, KIND_LIST, new_id)?;
        for (catalog_id, media_type, added_at) in items {
            tx.execute(
                "INSERT OR IGNORE INTO list_items (list_id, catalog_id, media_type, added_at)
                 VALUES (?1, ?2, ?3, ?4)",
                params![new_id, catalog_id, media_type, added_at],
            )?;
        }
        tx.commit().context("Failed to commit list replacement")
    }

    const ITEM_SELECT: &'static str =
        "SELECT li.list_id, li.catalog_id, li.media_type, li.added_at,
                t.title, t.poster, t.year, t.canonical_id
         FROM list_items li LEFT JOIN titles t ON t.catalog_id = li.catalog_id";

    fn row_to_item(row: &rusqlite::Row) -> rusqlite::Result<ListItem> {
        Ok(ListItem {
            list_id: row.get(0)?,
            catalog_id: row.get(1)?,
            media_type: row.get(2)?,
            added_at: row.get(3)?,
            title: row.get(4)?,
            poster: row.get(5)?,
            year: row.get(6)?,
            canonical_id: row.get(7)?,
        })
    }

    pub fn list_items(&self, list_id: &str) -> Result<Vec<ListItem>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(&format!(
            "{} WHERE li.list_id = ?1 ORDER BY li.added_at ASC",
            Self::ITEM_SELECT
        ))?;
        let rows = stmt
            .query_map(params![list_id], Self::row_to_item)
            .context("Failed to query list items")?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .context("Failed to read list item row")
    }

    /// Every item in every list of one profile, in one query. Callers group
    /// them by `list_id` instead of running one query per list.
    pub fn items_for_profile(&self, profile_id: &str) -> Result<Vec<ListItem>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(&format!(
            "{} JOIN lists l ON l.id = li.list_id
             WHERE l.profile_id = ?1 ORDER BY li.added_at ASC",
            Self::ITEM_SELECT
        ))?;
        let rows = stmt
            .query_map(params![profile_id], Self::row_to_item)
            .context("Failed to query profile list items")?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .context("Failed to read list item row")
    }

    // ---- Watch status ----

    /// Creates or updates the watch status for one title under one profile.
    /// A profile holds one status per title. A status already stored under an
    /// alias id for the same title is removed, so the title never shows twice.
    pub fn set_watch_status(
        &self,
        profile_id: &str,
        catalog_id: &str,
        status: WatchState,
        episode_id: Option<&str>,
        resume_seconds: Option<i64>,
        meta: &TitleMeta,
    ) -> Result<()> {
        self.write_watch_status(
            profile_id,
            catalog_id,
            status,
            episode_id,
            resume_seconds,
            now(),
            meta,
        )
    }

    /// Same as `set_watch_status` but keeps the given `updated_at`. Used by
    /// gateway sync to pull a remote status.
    #[allow(clippy::too_many_arguments)]
    pub fn upsert_watch_status(
        &self,
        profile_id: &str,
        catalog_id: &str,
        status: WatchState,
        episode_id: Option<&str>,
        resume_seconds: Option<i64>,
        updated_at: i64,
        meta: &TitleMeta,
    ) -> Result<()> {
        self.write_watch_status(
            profile_id,
            catalog_id,
            status,
            episode_id,
            resume_seconds,
            updated_at,
            meta,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn write_watch_status(
        &self,
        profile_id: &str,
        catalog_id: &str,
        status: WatchState,
        episode_id: Option<&str>,
        resume_seconds: Option<i64>,
        updated_at: i64,
        meta: &TitleMeta,
    ) -> Result<()> {
        let conn = self.conn();
        let tx = conn.unchecked_transaction()?;
        Self::upsert_title(&tx, catalog_id, meta)?;
        let aliases = Self::status_aliases(&tx, profile_id, catalog_id)?;
        // A newer status under an alias id already covers this title.
        if aliases.iter().any(|(_, alias_at)| *alias_at > updated_at) {
            return Ok(());
        }
        tx.execute(
            "INSERT INTO watch_status
                (profile_id, catalog_id, status, episode_id, resume_seconds, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(profile_id, catalog_id) DO UPDATE SET
                status = excluded.status,
                episode_id = excluded.episode_id,
                resume_seconds = excluded.resume_seconds,
                updated_at = excluded.updated_at",
            params![
                profile_id,
                catalog_id,
                status.as_str(),
                episode_id,
                resume_seconds,
                updated_at
            ],
        )
        .context("Failed to set watch status")?;
        Self::clear_deletion(
            &tx,
            KIND_WATCH_STATUS,
            &watch_status_key(profile_id, catalog_id),
        )?;
        for (alias, _) in aliases {
            tx.execute(
                "DELETE FROM watch_status WHERE profile_id = ?1 AND catalog_id = ?2",
                params![profile_id, alias],
            )?;
            let key = watch_status_key(profile_id, &alias);
            Self::record_deletion(&tx, KIND_WATCH_STATUS, &key, now())?;
        }
        tx.commit().context("Failed to commit watch status")
    }

    /// Other statuses of one profile that describe the same title as
    /// `catalog_id`, with their `updated_at`.
    fn status_aliases(
        conn: &Connection,
        profile_id: &str,
        catalog_id: &str,
    ) -> Result<Vec<(String, i64)>> {
        let canonical: String = conn
            .query_row(
                "SELECT COALESCE(canonical_id, catalog_id) FROM titles WHERE catalog_id = ?1",
                params![catalog_id],
                |row| row.get(0),
            )
            .optional()
            .context("Failed to read canonical id")?
            .unwrap_or_else(|| catalog_id.to_string());
        let mut stmt = conn.prepare(
            "SELECT ws.catalog_id, ws.updated_at FROM watch_status ws
             LEFT JOIN titles t ON t.catalog_id = ws.catalog_id
             WHERE ws.profile_id = ?1 AND ws.catalog_id != ?2
               AND COALESCE(t.canonical_id, ws.catalog_id) = ?3",
        )?;
        let rows = stmt.query_map(params![profile_id, catalog_id, canonical], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .context("Failed to read alias statuses")
    }

    /// Removes a title from the watch status board. Records the deletion for
    /// sync.
    pub fn remove_watch_status(&self, profile_id: &str, catalog_id: &str) -> Result<()> {
        let conn = self.conn();
        let changed = conn
            .execute(
                "DELETE FROM watch_status WHERE profile_id = ?1 AND catalog_id = ?2",
                params![profile_id, catalog_id],
            )
            .context("Failed to remove watch status")?;
        if changed > 0 {
            Self::record_deletion(
                &conn,
                KIND_WATCH_STATUS,
                &watch_status_key(profile_id, catalog_id),
                now(),
            )?;
        }
        Ok(())
    }

    const STATUS_SELECT: &'static str = "SELECT ws.profile_id, ws.catalog_id, ws.status, ws.episode_id,
                ws.resume_seconds, ws.updated_at, t.title, t.poster, t.media_type, t.year, t.canonical_id
         FROM watch_status ws LEFT JOIN titles t ON t.catalog_id = ws.catalog_id";

    pub fn get_watch_status(
        &self,
        profile_id: &str,
        catalog_id: &str,
    ) -> Result<Option<WatchStatus>> {
        self.conn()
            .query_row(
                &format!(
                    "{} WHERE ws.profile_id = ?1 AND ws.catalog_id = ?2",
                    Self::STATUS_SELECT
                ),
                params![profile_id, catalog_id],
                Self::row_to_watch_status,
            )
            .optional()
            .context("Failed to read watch status")
    }

    /// Every title under one status, for the three-column watch-status board.
    pub fn watch_status_by_state(
        &self,
        profile_id: &str,
        status: WatchState,
    ) -> Result<Vec<WatchStatus>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(&format!(
            "{} WHERE ws.profile_id = ?1 AND ws.status = ?2 ORDER BY ws.updated_at DESC",
            Self::STATUS_SELECT
        ))?;
        let rows = stmt
            .query_map(
                params![profile_id, status.as_str()],
                Self::row_to_watch_status,
            )
            .context("Failed to query watch status")?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .context("Failed to read watch status row")
    }

    /// All statuses of one profile in one query.
    pub fn all_watch_status(&self, profile_id: &str) -> Result<Vec<WatchStatus>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(&format!(
            "{} WHERE ws.profile_id = ?1 ORDER BY ws.updated_at DESC",
            Self::STATUS_SELECT
        ))?;
        let rows = stmt
            .query_map(params![profile_id], Self::row_to_watch_status)
            .context("Failed to query watch status")?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .context("Failed to read watch status row")
    }

    fn row_to_watch_status(row: &rusqlite::Row) -> rusqlite::Result<WatchStatus> {
        let status: String = row.get(2)?;
        Ok(WatchStatus {
            profile_id: row.get(0)?,
            catalog_id: row.get(1)?,
            status: WatchState::parse(&status).unwrap_or(WatchState::ToWatch),
            episode_id: row.get(3)?,
            resume_seconds: row.get(4)?,
            updated_at: row.get(5)?,
            title: row.get(6)?,
            poster: row.get(7)?,
            media_type: row.get(8)?,
            year: row.get(9)?,
            canonical_id: row.get(10)?,
        })
    }

    // ---- Deletions (for sync) ----

    pub fn deletions(&self) -> Result<Vec<Deletion>> {
        let conn = self.conn();
        let mut stmt = conn.prepare("SELECT kind, key, deleted_at FROM deletions")?;
        let rows = stmt
            .query_map([], |row| {
                Ok(Deletion {
                    kind: row.get(0)?,
                    key: row.get(1)?,
                    deleted_at: row.get(2)?,
                })
            })
            .context("Failed to query deletions")?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .context("Failed to read deletion row")
    }

    /// Applies a deletion that another device made, keeping its timestamp.
    pub fn apply_deletion(&self, kind: &str, key: &str, deleted_at: i64) -> Result<()> {
        let conn = self.conn();
        match kind {
            KIND_PROFILE => {
                conn.execute("DELETE FROM profiles WHERE id = ?1", params![key])?;
            }
            KIND_LIST => {
                conn.execute("DELETE FROM lists WHERE id = ?1", params![key])?;
            }
            KIND_LIST_ITEM => {
                if let Some((list_id, catalog_id)) = key.split_once(':') {
                    conn.execute(
                        "DELETE FROM list_items WHERE list_id = ?1 AND catalog_id = ?2",
                        params![list_id, catalog_id],
                    )?;
                }
            }
            KIND_WATCH_STATUS => {
                if let Some((profile_id, catalog_id)) = key.split_once(':') {
                    conn.execute(
                        "DELETE FROM watch_status WHERE profile_id = ?1 AND catalog_id = ?2",
                        params![profile_id, catalog_id],
                    )?;
                }
            }
            other => return Err(anyhow!("Unknown deletion kind: {other}")),
        }
        Self::record_deletion(&conn, kind, key, deleted_at)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NO_META: TitleMeta<'static> = TitleMeta {
        media_type: None,
        title: None,
        poster: None,
        year: None,
        canonical_id: None,
    };

    fn open_in_memory() -> ProfileStore {
        let conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "foreign_keys", true).unwrap();
        let store = ProfileStore {
            conn: Mutex::new(conn),
        };
        store.migrate().unwrap();
        store
    }

    fn store_with_profile() -> (ProfileStore, Profile) {
        let store = open_in_memory();
        let profile = store.create_profile("Ada", "star-violet").unwrap();
        (store, profile)
    }

    fn meta<'a>(title: &'a str, media_type: &'a str) -> TitleMeta<'a> {
        TitleMeta {
            title: Some(title),
            media_type: Some(media_type),
            poster: Some("https://example.com/p.jpg"),
            year: Some(2011),
            canonical_id: None,
        }
    }

    #[test]
    fn is_send_and_sync_so_it_can_cross_into_spawn_blocking() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<ProfileStore>();
    }

    #[test]
    fn opens_a_fresh_data_directory_and_creates_the_database() {
        let dir = tempfile::tempdir().unwrap();
        let store = ProfileStore::open(dir.path()).unwrap();
        assert!(dir.path().join("profiles.sqlite3").exists());
        assert_eq!(store.list_profiles().unwrap(), Vec::new());
    }

    #[test]
    fn reopening_an_existing_database_does_not_lose_data() {
        let dir = tempfile::tempdir().unwrap();
        {
            let store = ProfileStore::open(dir.path()).unwrap();
            store.create_profile("Ada", "star-violet").unwrap();
        }
        let store = ProfileStore::open(dir.path()).unwrap();
        assert_eq!(store.list_profiles().unwrap()[0].name, "Ada");
    }

    #[test]
    fn migrates_a_v1_database_keeps_data_and_renames_duplicate_list_names() {
        let dir = tempfile::tempdir().unwrap();
        {
            let conn = Connection::open(dir.path().join("profiles.sqlite3")).unwrap();
            conn.execute_batch(
                "CREATE TABLE schema_version (version INTEGER NOT NULL);
                 INSERT INTO schema_version VALUES (1);
                 CREATE TABLE profiles (id TEXT PRIMARY KEY, name TEXT NOT NULL,
                    avatar_key TEXT NOT NULL, created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL);
                 CREATE TABLE lists (id TEXT PRIMARY KEY, profile_id TEXT NOT NULL,
                    name TEXT NOT NULL, created_at INTEGER NOT NULL);
                 CREATE TABLE list_items (list_id TEXT NOT NULL, catalog_id TEXT NOT NULL,
                    media_type TEXT NOT NULL, added_at INTEGER NOT NULL, PRIMARY KEY (list_id, catalog_id));
                 CREATE TABLE watch_status (profile_id TEXT NOT NULL, catalog_id TEXT NOT NULL,
                    status TEXT NOT NULL, episode_id TEXT, resume_seconds INTEGER,
                    updated_at INTEGER NOT NULL, PRIMARY KEY (profile_id, catalog_id));
                 INSERT INTO profiles VALUES ('p1', 'Ada', 'star', 1, 1);
                 INSERT INTO lists VALUES ('aaaa1111', 'p1', 'Weekend', 1);
                 INSERT INTO lists VALUES ('bbbb2222', 'p1', 'weekend', 2);
                 INSERT INTO list_items VALUES ('aaaa1111', 'tt1', 'movie', 5);",
            )
            .unwrap();
        }
        let store = ProfileStore::open(dir.path()).unwrap();
        let lists = store.list_lists("p1").unwrap();
        assert_eq!(lists[0].name, "Weekend");
        assert_eq!(lists[1].name, "weekend (bbbb)");
        let items = store.list_items("aaaa1111").unwrap();
        assert_eq!(items[0].catalog_id, "tt1");
        assert_eq!(items[0].title, None);
        assert!(store.create_list("p1", "WEEKEND").is_err());
    }

    #[test]
    fn migrating_to_version_3_adds_the_watch_status_and_title_indexes() {
        let store = open_in_memory();
        let names: Vec<String> = store
            .conn()
            .prepare("SELECT name FROM sqlite_master WHERE type = 'index'")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        assert!(names.contains(&"idx_watch_status_profile_status".to_string()));
        assert!(names.contains(&"idx_titles_title".to_string()));
    }

    #[test]
    fn creates_lists_profiles_and_updates_them() {
        let (store, profile) = store_with_profile();
        store.create_profile("Femi", "bolt-amber").unwrap();
        assert_eq!(store.list_profiles().unwrap().len(), 2);
        store
            .update_profile(&profile.id, Some("Adaeze"), Some("moon-sky"))
            .unwrap();
        let updated = &store.list_profiles().unwrap()[0];
        assert_eq!(updated.name, "Adaeze");
        assert_eq!(updated.avatar_key, "moon-sky");
        assert!(store.update_profile("missing", Some("x"), None).is_err());
    }

    #[test]
    fn upsert_profile_keeps_created_at() {
        let store = open_in_memory();
        store
            .upsert_profile("r1", "Ada", "a", 1_000, 1_000)
            .unwrap();
        store
            .upsert_profile("r1", "Adaeze", "b", 9_999, 2_000)
            .unwrap();
        let p = &store.list_profiles().unwrap()[0];
        assert_eq!(
            (p.name.as_str(), p.created_at, p.updated_at),
            ("Adaeze", 1_000, 2_000)
        );
    }

    #[test]
    fn list_names_are_unique_per_profile_ignoring_case() {
        let (store, profile) = store_with_profile();
        let other = store.create_profile("Femi", "b").unwrap();
        store.create_list(&profile.id, "Weekend picks").unwrap();
        let err = store.create_list(&profile.id, "WEEKEND PICKS").unwrap_err();
        assert!(err.to_string().contains("already exists"));
        // A different profile may reuse the name.
        store.create_list(&other.id, "Weekend picks").unwrap();
    }

    #[test]
    fn find_list_by_name_ignores_case() {
        let (store, profile) = store_with_profile();
        let list = store.create_list(&profile.id, "Anime").unwrap();
        let found = store.find_list_by_name(&profile.id, "aNiMe").unwrap();
        assert_eq!(found.unwrap().id, list.id);
    }

    #[test]
    fn deleting_a_profile_cascades_and_records_a_deletion() {
        let (store, profile) = store_with_profile();
        let list = store.create_list(&profile.id, "Weekend").unwrap();
        store
            .add_list_item(&list.id, "tt1", "movie", &meta("A", "movie"))
            .unwrap();
        store
            .set_watch_status(
                &profile.id,
                "tt1",
                WatchState::Watched,
                None,
                None,
                &NO_META,
            )
            .unwrap();

        store.delete_profile(&profile.id).unwrap();

        assert!(store.list_profiles().unwrap().is_empty());
        assert!(store.list_lists(&profile.id).unwrap().is_empty());
        assert!(store.list_items(&list.id).unwrap().is_empty());
        assert!(store
            .get_watch_status(&profile.id, "tt1")
            .unwrap()
            .is_none());
        let deletions = store.deletions().unwrap();
        assert!(deletions
            .iter()
            .any(|d| d.kind == KIND_PROFILE && d.key == profile.id));
    }

    #[test]
    fn deleting_a_list_removes_only_its_items() {
        let (store, profile) = store_with_profile();
        let keep = store.create_list(&profile.id, "Keep").unwrap();
        let drop = store.create_list(&profile.id, "Drop").unwrap();
        store
            .add_list_item(&drop.id, "tt1", "movie", &NO_META)
            .unwrap();
        store.delete_list(&drop.id).unwrap();
        assert_eq!(store.list_lists(&profile.id).unwrap(), vec![keep]);
        assert!(store.list_items(&drop.id).unwrap().is_empty());
        assert!(store
            .deletions()
            .unwrap()
            .iter()
            .any(|d| d.kind == KIND_LIST));
    }

    #[test]
    fn list_items_carry_title_poster_and_year_from_the_titles_table() {
        let (store, profile) = store_with_profile();
        let list = store.create_list(&profile.id, "Weekend").unwrap();
        store
            .add_list_item(&list.id, "tt1", "series", &meta("Shameless", "series"))
            .unwrap();
        let item = &store.list_items(&list.id).unwrap()[0];
        assert_eq!(item.title.as_deref(), Some("Shameless"));
        assert_eq!(item.poster.as_deref(), Some("https://example.com/p.jpg"));
        assert_eq!(item.year, Some(2011));
    }

    #[test]
    fn titles_are_shared_between_a_list_item_and_a_status() {
        let (store, profile) = store_with_profile();
        let list = store.create_list(&profile.id, "Weekend").unwrap();
        store
            .add_list_item(&list.id, "tt1", "movie", &meta("Inception", "movie"))
            .unwrap();
        // The status call carries no title; it reads the shared one.
        store
            .set_watch_status(
                &profile.id,
                "tt1",
                WatchState::InProgress,
                None,
                Some(60),
                &NO_META,
            )
            .unwrap();
        let status = store.get_watch_status(&profile.id, "tt1").unwrap().unwrap();
        assert_eq!(status.title.as_deref(), Some("Inception"));
        assert_eq!(status.media_type.as_deref(), Some("movie"));
    }

    #[test]
    fn a_later_call_without_metadata_never_blanks_stored_metadata() {
        let (store, profile) = store_with_profile();
        store
            .set_watch_status(
                &profile.id,
                "tt1",
                WatchState::ToWatch,
                None,
                None,
                &meta("A", "movie"),
            )
            .unwrap();
        store
            .set_watch_status(
                &profile.id,
                "tt1",
                WatchState::InProgress,
                None,
                Some(600),
                &NO_META,
            )
            .unwrap();
        let s = store.get_watch_status(&profile.id, "tt1").unwrap().unwrap();
        assert_eq!(
            (s.status, s.resume_seconds),
            (WatchState::InProgress, Some(600))
        );
        assert_eq!(s.title.as_deref(), Some("A"));
        assert_eq!(s.year, Some(2011));
    }

    #[test]
    fn adding_the_same_title_twice_keeps_one_row() {
        let (store, profile) = store_with_profile();
        let list = store.create_list(&profile.id, "Weekend").unwrap();
        store
            .add_list_item(&list.id, "tt1", "movie", &NO_META)
            .unwrap();
        store
            .add_list_item(&list.id, "tt1", "movie", &NO_META)
            .unwrap();
        assert_eq!(store.list_items(&list.id).unwrap().len(), 1);
    }

    #[test]
    fn removing_a_list_item_records_a_deletion_and_re_adding_clears_it() {
        let (store, profile) = store_with_profile();
        let list = store.create_list(&profile.id, "Weekend").unwrap();
        store
            .add_list_item(&list.id, "tt1", "movie", &NO_META)
            .unwrap();
        store.remove_list_item(&list.id, "tt1").unwrap();
        let key = list_item_key(&list.id, "tt1");
        assert!(store.deletions().unwrap().iter().any(|d| d.key == key));

        store
            .add_list_item(&list.id, "tt1", "movie", &NO_META)
            .unwrap();
        assert!(!store.deletions().unwrap().iter().any(|d| d.key == key));
    }

    #[test]
    fn an_alias_id_for_a_listed_title_is_not_added_twice() {
        let (store, profile) = store_with_profile();
        let list = store.create_list(&profile.id, "Anime").unwrap();
        store
            .add_list_item(&list.id, "tt99", "series", &meta("Show", "series"))
            .unwrap();
        let alias = TitleMeta {
            canonical_id: Some("tt99"),
            ..meta("Show", "anime")
        };
        store
            .add_list_item(&list.id, "kitsu:7", "anime", &alias)
            .unwrap();
        let items = store.list_items(&list.id).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].catalog_id, "tt99");
    }

    #[test]
    fn setting_a_status_replaces_a_status_stored_under_an_alias_id() {
        let (store, profile) = store_with_profile();
        let alias = TitleMeta {
            canonical_id: Some("tt99"),
            ..meta("Show", "anime")
        };
        store
            .set_watch_status(
                &profile.id,
                "kitsu:7",
                WatchState::ToWatch,
                None,
                None,
                &alias,
            )
            .unwrap();
        store
            .set_watch_status(
                &profile.id,
                "tt99",
                WatchState::Watched,
                None,
                None,
                &meta("Show", "series"),
            )
            .unwrap();
        let all = store.all_watch_status(&profile.id).unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(
            (all[0].catalog_id.as_str(), all[0].status),
            ("tt99", WatchState::Watched)
        );
        let key = watch_status_key(&profile.id, "kitsu:7");
        assert!(store.deletions().unwrap().iter().any(|d| d.key == key));
    }

    #[test]
    fn one_status_per_title_moves_between_columns() {
        let (store, profile) = store_with_profile();
        for state in [
            WatchState::ToWatch,
            WatchState::InProgress,
            WatchState::Watched,
        ] {
            store
                .set_watch_status(&profile.id, "tt1", state, None, None, &NO_META)
                .unwrap();
        }
        assert_eq!(store.all_watch_status(&profile.id).unwrap().len(), 1);
        assert_eq!(
            store
                .watch_status_by_state(&profile.id, WatchState::ToWatch)
                .unwrap()
                .len(),
            0
        );
        assert_eq!(
            store
                .watch_status_by_state(&profile.id, WatchState::Watched)
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn removing_a_status_deletes_it_and_records_a_deletion() {
        let (store, profile) = store_with_profile();
        store
            .set_watch_status(
                &profile.id,
                "tt1",
                WatchState::Watched,
                None,
                None,
                &NO_META,
            )
            .unwrap();
        store.remove_watch_status(&profile.id, "tt1").unwrap();
        assert!(store
            .get_watch_status(&profile.id, "tt1")
            .unwrap()
            .is_none());
        assert!(store
            .deletions()
            .unwrap()
            .iter()
            .any(|d| d.kind == KIND_WATCH_STATUS));
        // Removing a status that does not exist is not an error.
        store.remove_watch_status(&profile.id, "tt404").unwrap();
    }

    #[test]
    fn status_is_scoped_to_its_profile() {
        let (store, ada) = store_with_profile();
        let femi = store.create_profile("Femi", "b").unwrap();
        store
            .set_watch_status(&ada.id, "tt1", WatchState::Watched, None, None, &NO_META)
            .unwrap();
        assert!(store.get_watch_status(&femi.id, "tt1").unwrap().is_none());
    }

    #[test]
    fn items_for_profile_returns_every_list_in_one_query() {
        let (store, profile) = store_with_profile();
        let a = store.create_list(&profile.id, "A").unwrap();
        let b = store.create_list(&profile.id, "B").unwrap();
        store
            .add_list_item(&a.id, "tt1", "movie", &NO_META)
            .unwrap();
        store
            .add_list_item(&b.id, "tt2", "movie", &NO_META)
            .unwrap();
        assert_eq!(store.items_for_profile(&profile.id).unwrap().len(), 2);
    }

    #[test]
    fn replace_list_moves_items_and_records_the_old_list_as_deleted() {
        let (store, profile) = store_with_profile();
        let old = store.create_list(&profile.id, "Weekend").unwrap();
        store
            .add_list_item(&old.id, "tt1", "movie", &NO_META)
            .unwrap();
        store
            .replace_list(&old.id, "remote-id", &profile.id, "weekend", 5)
            .unwrap();
        let lists = store.list_lists(&profile.id).unwrap();
        assert_eq!((lists.len(), lists[0].id.as_str()), (1, "remote-id"));
        assert_eq!(store.list_items("remote-id").unwrap().len(), 1);
        assert!(store.deletions().unwrap().iter().any(|d| d.key == old.id));
    }

    #[test]
    fn an_older_alias_status_does_not_replace_a_newer_one() {
        let (store, profile) = store_with_profile();
        store
            .upsert_watch_status(
                &profile.id,
                "tt99",
                WatchState::Watched,
                None,
                None,
                200,
                &meta("Show", "series"),
            )
            .unwrap();
        let alias = TitleMeta {
            canonical_id: Some("tt99"),
            ..meta("Show", "anime")
        };
        store
            .upsert_watch_status(
                &profile.id,
                "kitsu:7",
                WatchState::ToWatch,
                None,
                None,
                100,
                &alias,
            )
            .unwrap();
        let all = store.all_watch_status(&profile.id).unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].catalog_id, "tt99");
    }

    #[test]
    fn apply_deletion_removes_the_row_and_keeps_the_given_timestamp() {
        let (store, profile) = store_with_profile();
        store
            .set_watch_status(
                &profile.id,
                "kitsu:1:2",
                WatchState::Watched,
                None,
                None,
                &NO_META,
            )
            .unwrap();
        let key = watch_status_key(&profile.id, "kitsu:1:2");
        store.apply_deletion(KIND_WATCH_STATUS, &key, 1234).unwrap();
        assert!(store
            .get_watch_status(&profile.id, "kitsu:1:2")
            .unwrap()
            .is_none());
        let d = store
            .deletions()
            .unwrap()
            .into_iter()
            .find(|d| d.key == key)
            .unwrap();
        assert_eq!(d.deleted_at, 1234);
        assert!(store.apply_deletion("bogus", "x", 1).is_err());
    }

    #[test]
    fn old_deletion_records_are_purged_on_open() {
        let dir = tempfile::tempdir().unwrap();
        {
            let store = ProfileStore::open(dir.path()).unwrap();
            let conn = store.conn();
            ProfileStore::record_deletion(&conn, KIND_LIST, "old", 1).unwrap();
            ProfileStore::record_deletion(&conn, KIND_LIST, "new", now()).unwrap();
        }
        let store = ProfileStore::open(dir.path()).unwrap();
        let keys: Vec<_> = store
            .deletions()
            .unwrap()
            .into_iter()
            .map(|d| d.key)
            .collect();
        assert_eq!(keys, vec!["new".to_string()]);
    }

    #[test]
    fn stores_and_retrieves_titles_by_catalog_id() {
        let dir = tempfile::tempdir().unwrap();
        let store = ProfileStore::open(dir.path()).unwrap();
        assert!(store.get_title("tt0113277").unwrap().is_none());

        let meta = TitleMeta {
            media_type: Some("movie"),
            title: Some("Heat"),
            poster: Some("https://example.com/heat.jpg"),
            year: Some(1995),
            canonical_id: None,
        };
        store.save_title("tt0113277", &meta).unwrap();

        let title = store
            .get_title("tt0113277")
            .unwrap()
            .expect("Title must exist");
        assert_eq!(title.catalog_id, "tt0113277");
        assert_eq!(title.title.as_deref(), Some("Heat"));
        assert_eq!(title.media_type.as_deref(), Some("movie"));
        assert_eq!(title.year, Some(1995));
        assert_eq!(
            title.poster.as_deref(),
            Some("https://example.com/heat.jpg")
        );
    }

    #[test]
    fn finds_titles_by_name_ignoring_case() {
        let dir = tempfile::tempdir().unwrap();
        let store = ProfileStore::open(dir.path()).unwrap();

        store
            .save_title(
                "tt1",
                &TitleMeta {
                    media_type: Some("movie"),
                    title: Some("Inception"),
                    poster: None,
                    year: Some(2010),
                    canonical_id: None,
                },
            )
            .unwrap();

        let results = store.find_titles_by_name("inception").unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].catalog_id, "tt1");
        assert_eq!(results[0].title.as_deref(), Some("Inception"));

        let empty = store.find_titles_by_name("Nonexistent").unwrap();
        assert!(empty.is_empty());
    }
}
