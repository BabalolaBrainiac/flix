-- Rename existing duplicates first, or the unique index cannot be created.
-- The oldest list of a name keeps it. Later lists get a short id suffix.
UPDATE lists SET name = name || ' (' || substr(id, 1, 4) || ')'
WHERE EXISTS (
  SELECT 1 FROM lists other
  WHERE other.profile_id = lists.profile_id
    AND lower(other.name) = lower(lists.name)
    AND (other.created_at < lists.created_at
         OR (other.created_at = lists.created_at AND other.id < lists.id))
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_lists_profile_name
  ON lists(profile_id, name COLLATE NOCASE);
